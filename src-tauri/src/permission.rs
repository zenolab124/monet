//! 权限请求服务
//!
//! 在主进程内启动 Unix socket server，作为 monet-mcp 子进程与前端的桥梁。
//! v2.1.0 起按会话隔离：每个流式会话一个 PermissionService 实例（工作台多列并行）。
//! 主流程：
//! 1. `PermissionService::start(app, session_id)` 绑定 `/tmp/monet-perm-<pid>-<rand>.sock`
//! 2. 接受连接（每条连接对应 monet-mcp 转发的一次权限请求）
//! 3. emit Tauri Event `permission-request` 给前端（payload 带 sessionId）
//! 4. 前端通过 `respond_permission` Tauri Command 回写决策（allow/deny）
//! 5. service 把决策写回 socket，monet-mcp 据此返回 claude CLI

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

/// 等待轮询步长（避免 busy-loop）
const POLL_INTERVAL_MS: u64 = 50;

/// 一次 pending 请求的状态
struct PendingRequest {
    /// 收到响应后写到这里
    decision: Mutex<Option<Decision>>,
}

/// 用户决策
#[derive(Debug, Clone)]
enum Decision {
    /// 携带可选 updatedInput：交互工具（AskUserQuestion 等）经此回传用户答案，
    /// None 时 monet-mcp 回填原始 input
    Allow(Option<Value>),
    Deny(String),
}

/// 权限服务（每个流式会话一个实例，会话归属由 SERVICES 的 key 承载）
pub struct PermissionService {
    /// socket 文件路径（销毁时清理）
    socket_path: PathBuf,
    /// 关闭信号
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
    /// 共享的 pending 请求表
    pending: Arc<Mutex<HashMap<String, Arc<PendingRequest>>>>,
}

/// 会话 → 服务实例表
static SERVICES: Mutex<Option<HashMap<String, Arc<PermissionService>>>> = Mutex::new(None);

/// requestId 自增（全进程唯一，respond 时跨实例查找无歧义）
static REQ_COUNTER: AtomicU64 = AtomicU64::new(1);

/// 推送给前端的事件
#[derive(Debug, Clone, Serialize)]
struct PermissionRequestPayload {
    #[serde(rename = "requestId")]
    request_id: String,
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(rename = "toolName")]
    tool_name: String,
    input: Value,
    /// Unix 毫秒
    timestamp: u128,
}

impl PermissionService {
    /// 启动指定会话的权限服务，返回实例（socket 路径注入 monet-mcp 子进程环境变量）
    pub fn start(app: AppHandle, session_id: &str) -> Result<Arc<Self>, String> {
        // 同会话旧实例先停（同会话重发场景）
        Self::stop_for(session_id);

        let socket_path = make_socket_path();
        // 防止残留文件
        let _ = std::fs::remove_file(&socket_path);

        let listener = UnixListener::bind(&socket_path)
            .map_err(|e| format!("绑定权限 socket 失败 ({:?}): {}", socket_path, e))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("设置 socket 非阻塞失败：{}", e))?;

        let stop_flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let pending: Arc<Mutex<HashMap<String, Arc<PendingRequest>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        let service = Arc::new(PermissionService {
            socket_path: socket_path.clone(),
            stop_flag: stop_flag.clone(),
            pending: pending.clone(),
        });

        // accept loop 单独线程
        {
            let pending = pending.clone();
            let stop_flag = stop_flag.clone();
            let sid = session_id.to_string();
            thread::spawn(move || {
                accept_loop(listener, app, sid, pending, stop_flag);
            });
        }

        let mut services = SERVICES.lock().unwrap();
        services
            .get_or_insert_with(HashMap::new)
            .insert(session_id.to_string(), service.clone());
        Ok(service)
    }

    pub fn socket_path(&self) -> &std::path::Path {
        &self.socket_path
    }

    /// 提交一个用户决策（requestId 全进程唯一，跨实例查找）。返回是否找到对应的 pending 请求
    pub fn respond(
        request_id: &str,
        allow: bool,
        message: Option<String>,
        updated_input: Option<Value>,
    ) -> bool {
        let services: Vec<Arc<PermissionService>> = SERVICES
            .lock()
            .unwrap()
            .as_ref()
            .map(|m| m.values().cloned().collect())
            .unwrap_or_default();
        for service in services {
            let pending = service.pending.lock().unwrap();
            if let Some(req) = pending.get(request_id) {
                let mut slot = req.decision.lock().unwrap();
                if slot.is_none() {
                    *slot = Some(if allow {
                        Decision::Allow(updated_input)
                    } else {
                        Decision::Deny(message.unwrap_or_else(|| "用户拒绝".to_string()))
                    });
                    return true;
                }
            }
        }
        false
    }

    /// 停掉指定会话的服务（关闭 socket、pending 请求统一按 deny 收尾）
    pub fn stop_for(session_id: &str) {
        let service = SERVICES
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|m| m.remove(session_id));
        if let Some(service) = service {
            service.shutdown();
        }
    }

    /// 仅当该会话当前注册的服务正是 `socket_path` 这一实例时才停止。
    ///
    /// 同会话连发场景下，每个 streaming turn 新建一个 socket。旧 turn 的 read_stream
    /// 线程退出时若按 session_id 盲调 stop_for，会误删新 turn 刚注册的服务（socket 被
    /// shutdown → 新 turn 的 monet-mcp 连接 Connection refused / pending 被强制 deny）。
    /// 用 socket 路径做实例身份校验：被新 turn 接管后旧线程不再触碰。
    pub fn stop_if_socket(session_id: &str, socket_path: &std::path::Path) {
        let service = {
            let mut guard = SERVICES.lock().unwrap();
            let is_current = guard
                .as_ref()
                .and_then(|m| m.get(session_id))
                .is_some_and(|s| s.socket_path.as_path() == socket_path);
            if is_current {
                guard.as_mut().and_then(|m| m.remove(session_id))
            } else {
                None
            }
        };
        if let Some(service) = service {
            service.shutdown();
        }
    }

    /// 内部：置停止位 + deny 全部 pending + 清 socket 文件
    fn shutdown(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        let pending = self.pending.lock().unwrap();
        for req in pending.values() {
            let mut slot = req.decision.lock().unwrap();
            if slot.is_none() {
                *slot = Some(Decision::Deny("流式中断，自动拒绝".to_string()));
            }
        }
        drop(pending);
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

impl Drop for PermissionService {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

/// 生成一个进程内不冲突的 socket 路径
fn make_socket_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    PathBuf::from(format!("/tmp/monet-perm-{}-{}.sock", pid, nanos))
}

/// accept loop：每条连接 spawn 一个处理线程
fn accept_loop(
    listener: UnixListener,
    app: AppHandle,
    session_id: String,
    pending: Arc<Mutex<HashMap<String, Arc<PendingRequest>>>>,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
) {
    while !stop_flag.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let app = app.clone();
                let pending = pending.clone();
                let stop_flag = stop_flag.clone();
                let sid = session_id.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, app, sid, pending, stop_flag) {
                        log::warn!("权限 socket 处理失败：{}", e);
                    }
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
            Err(e) => {
                log::warn!("权限 socket accept 失败：{}", e);
                thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
        }
    }
}

/// 处理单次连接：读一行请求 → emit event → 等用户响应 → 写回一行决策
fn handle_connection(
    stream: UnixStream,
    app: AppHandle,
    session_id: String,
    pending: Arc<Mutex<HashMap<String, Arc<PendingRequest>>>>,
    stop_flag: Arc<std::sync::atomic::AtomicBool>,
) -> Result<(), String> {
    stream
        .set_nonblocking(false)
        .map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream.try_clone().map_err(|e| e.to_string())?);
    let mut req_line = String::new();
    reader
        .read_line(&mut req_line)
        .map_err(|e| format!("读权限请求失败：{}", e))?;
    let req: Value = serde_json::from_str(req_line.trim())
        .map_err(|e| format!("解析权限请求 JSON 失败：{}", e))?;

    let tool_name = req
        .get("toolName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let input = req.get("input").cloned().unwrap_or_else(|| json!({}));

    let request_id = format!("perm-{}", REQ_COUNTER.fetch_add(1, Ordering::Relaxed));
    let pending_req = Arc::new(PendingRequest {
        decision: Mutex::new(None),
    });

    pending
        .lock()
        .unwrap()
        .insert(request_id.clone(), pending_req.clone());

    // emit 给前端
    let payload = PermissionRequestPayload {
        request_id: request_id.clone(),
        session_id,
        tool_name,
        input,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0),
    };
    let _ = app.emit("permission-request", &payload);

    // 永不超时:阻塞等用户响应或服务停止(中断收尾会唤醒并按 deny 写回)
    let final_decision = wait_decision(&pending_req, &stop_flag);

    // 移除 pending
    pending.lock().unwrap().remove(&request_id);

    // 写回响应
    let resp = match final_decision {
        Decision::Allow(Some(updated)) => {
            json!({ "behavior": "allow", "updatedInput": updated })
        }
        Decision::Allow(None) => json!({ "behavior": "allow" }),
        Decision::Deny(msg) => json!({ "behavior": "deny", "message": msg }),
    };
    let mut out = stream;
    let line = serde_json::to_string(&resp).map_err(|e| e.to_string())?;
    out.write_all(line.as_bytes())
        .map_err(|e| format!("写响应失败：{}", e))?;
    out.write_all(b"\n").map_err(|e| e.to_string())?;
    out.flush().map_err(|e| e.to_string())?;
    Ok(())
}

/// 阻塞等待决策（轮询，永不超时）：命中用户响应或服务停止两者之一返回。
/// 不再自动拒绝——卡住等用户点，直到 stop_for/shutdown 在流式中断收尾时置 stop_flag 唤醒。
fn wait_decision(
    req: &PendingRequest,
    stop_flag: &Arc<std::sync::atomic::AtomicBool>,
) -> Decision {
    loop {
        {
            let slot = req.decision.lock().unwrap();
            if let Some(d) = slot.as_ref() {
                return d.clone();
            }
        }
        if stop_flag.load(Ordering::Relaxed) {
            // 服务停止时按拒绝处理
            return Decision::Deny("流式中断，自动拒绝".to_string());
        }
        thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
    }
}

// ----- 单元测试 -----

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn make_socket_path_unique() {
        let a = make_socket_path();
        std::thread::sleep(Duration::from_millis(2));
        let b = make_socket_path();
        assert_ne!(a, b);
        assert!(a.to_string_lossy().starts_with("/tmp/monet-perm-"));
    }

    #[test]
    fn parse_permission_request_ok() {
        let line = r#"{"toolName":"Bash","input":{"command":"ls"}}"#;
        let v: Value = serde_json::from_str(line).unwrap();
        assert_eq!(v.get("toolName").unwrap().as_str().unwrap(), "Bash");
        assert_eq!(
            v.get("input").unwrap().get("command").unwrap().as_str().unwrap(),
            "ls"
        );
    }
}
