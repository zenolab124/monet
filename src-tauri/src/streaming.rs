use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::models::ContentBlock;
use crate::permission::PermissionService;

/// 活跃的 streaming 进程表（v2.1.0 per-session：sessionId → child，多会话并行流式）
static ACTIVE_PROCESSES: Mutex<Option<HashMap<String, Arc<Mutex<Child>>>>> = Mutex::new(None);

/// 前端接收的流式事件（全部携带 session_id，前端按会话分发）
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    /// 助手消息内容更新（终态快照，作为字符级 delta 的兜底校正）
    AssistantMessage {
        session_id: String,
        message_id: String,
        content: Vec<ContentBlock>,
    },
    /// 字符级增量到达——content_block_start：某个 index 上出现新块
    BlockStart {
        session_id: String,
        message_id: String,
        index: usize,
        content_block: ContentBlock,
    },
    /// 字符级增量到达——content_block_delta：某个 index 上的块字段增长
    /// delta 原样透传给前端，由前端按 delta.type 派发：
    /// - text_delta { type, text }
    /// - input_json_delta { type, partial_json }
    /// - thinking_delta { type, thinking }
    /// - signature_delta { type, signature }
    BlockDelta {
        session_id: String,
        message_id: String,
        index: usize,
        delta: Value,
    },
    /// 字符级增量到达——content_block_stop：某个 index 上的块结束
    BlockStop {
        session_id: String,
        message_id: String,
        index: usize,
    },
    /// 流式完成
    Result {
        session_id: String,
        text: String,
        cost_usd: f64,
    },
    /// 错误
    Error {
        session_id: String,
        message: String,
    },
}

/// 查找 claude 可执行文件路径
fn find_claude() -> (String, Vec<String>) {
    let candidates = [
        "/opt/homebrew/bin/claude",
        "/usr/local/bin/claude",
    ];

    let home = dirs::home_dir().unwrap_or_default();
    let home_candidates = [
        home.join(".claude/local/bin/claude"),
        home.join(".npm-global/bin/claude"),
    ];

    for path in candidates.iter().map(|s| std::path::PathBuf::from(s)).chain(home_candidates) {
        if path.is_file() {
            return (path.to_string_lossy().into_owned(), vec![]);
        }
    }

    // fallback: 用 env 解析
    ("/usr/bin/env".to_string(), vec!["claude".to_string()])
}

/// 构建增强 PATH 环境变量
fn enhanced_path() -> String {
    let home = dirs::home_dir().unwrap_or_default();
    let mut extra_paths = vec![
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        format!("{}/.local/bin", home.display()),
    ];

    // 检测 nvm node 路径
    let nvm_dir = home.join(".nvm/versions/node");
    if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
        let mut versions: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .map(|e| e.path())
            .collect();
        versions.sort();
        if let Some(latest) = versions.last() {
            extra_paths.push(format!("{}/bin", latest.display()));
        }
    }

    let existing = std::env::var("PATH").unwrap_or_default();
    format!("{}:{}", extra_paths.join(":"), existing)
}

/// 启动流式会话（同会话已有流式则先终止旧流；不同会话互不影响）
pub fn start_streaming(
    app: &AppHandle,
    session_id: &str,
    cwd: &str,
    message: &str,
    model: Option<&str>,
    effort: Option<&str>,
) {
    // 同会话旧进程先终止（重发场景）；其他会话的并行流式不受影响
    stop_streaming(session_id);

    // cwd 缺失时 spawn 报 ENOENT,与 claude 二进制缺失同文案无法区分——前置校验给出指向性报错
    // (场景:项目源目录已删除/改名,或路径解码歧义)
    if !std::path::Path::new(cwd).is_dir() {
        emit_error(app, session_id, format!("工作目录不存在: {}", cwd));
        return;
    }

    // 1. 启动该会话的权限服务（失败立即上报错误，不静默降级）
    let permission_service = match PermissionService::start(app.clone(), session_id) {
        Ok(s) => s,
        Err(e) => {
            emit_error(app, session_id, format!("权限服务启动失败：{}", e));
            return;
        }
    };
    let socket_path = permission_service.socket_path().to_path_buf();

    // 2. 定位 cc-space-mcp 二进制
    let mcp_binary = match find_mcp_binary() {
        Some(p) => p,
        None => {
            PermissionService::stop_for(session_id);
            emit_error(
                app,
                session_id,
                "未找到 cc-space-mcp 二进制，无法启动权限服务".to_string(),
            );
            return;
        }
    };

    // 3. 构建 MCP 配置 JSON（claude CLI 接受 JSON 字符串或文件路径）
    let mcp_config = json!({
        "mcpServers": {
            "cc-space": {
                "type": "stdio",
                "command": mcp_binary.to_string_lossy().into_owned(),
                "args": [],
                "env": {
                    "CC_SPACE_PERMISSION_SOCK": socket_path.to_string_lossy().into_owned()
                }
            }
        }
    })
    .to_string();

    let (executable, prefix_args) = find_claude();
    let mut args = prefix_args;
    // jsonl 已落盘 → --resume 续聊;未落盘(工作台应用内新建的草稿)→ --session-id
    // 以前端指定的 UUID 新建会话。文件系统即事实源,首条消息失败后重试仍走新建,自愈。
    let session_file = crate::commands::projects_dir()
        .join(cwd.replace('/', "-"))
        .join(format!("{}.jsonl", session_id));
    let session_flag = if session_file.is_file() { "--resume" } else { "--session-id" };
    args.extend([
        session_flag.to_string(),
        session_id.to_string(),
        "--print".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
        "--include-partial-messages".to_string(),
        // --print 模式下新模型 thinking.display 默认 omitted(thinking_delta 全是空串),
        // 该隐藏参数(hideHelp)恢复 summarized 摘要明文。版本飘移撤参时 CLI 会以
        // unknown option 退出,stderr 经 StreamEvent::Error 上屏可定位,届时参照
        // docs/knowledge/pitfalls/thinking-redacted-opus-4-7.md
        "--thinking-display".to_string(),
        "summarized".to_string(),
        "--mcp-config".to_string(),
        mcp_config,
        "--permission-prompt-tool".to_string(),
        "mcp__cc-space__approve_tool_use".to_string(),
    ]);
    if let Some(m) = model.filter(|s| !s.is_empty()) {
        args.push("--model".to_string());
        args.push(m.to_string());
    }
    if let Some(e) = effort.filter(|s| !s.is_empty()) {
        if e == "ultracode" {
            // ultracode 是会话级设置而非 effort 档位(CLI 不支持在 settings.json 持久化,
            // session-only),经 --settings 注入;自带 xhigh + 对实质任务自动编排多智能体 workflow
            args.push("--settings".to_string());
            args.push(r#"{"ultracode": true}"#.to_string());
        } else {
            args.push("--effort".to_string());
            args.push(e.to_string());
        }
    }
    args.push(message.to_string());

    let child = match Command::new(&executable)
        .args(&args)
        .current_dir(cwd)
        .env("PATH", enhanced_path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            PermissionService::stop_for(session_id);
            emit_error(app, session_id, format!("启动 claude 失败: {}", e));
            return;
        }
    };

    let child = Arc::new(Mutex::new(child));
    ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(session_id.to_string(), child.clone());

    let handle = app.clone();
    let sid = session_id.to_string();
    let sock = socket_path.clone();
    std::thread::spawn(move || {
        read_stream(child, &handle, &sid);
        // 子进程退出后回收该会话的权限服务——仅清理本 turn 自己的实例。
        // 不能盲调 stop_for(sid)：同会话连发时新 turn 可能已注册新 socket，
        // 盲删会害新 turn 的权限请求 Connection refused（race，见 stop_if_socket 注释）
        PermissionService::stop_if_socket(&sid, &sock);
    });
}

/// 终止指定会话的流式进程
pub fn stop_streaming(session_id: &str) {
    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|m| m.remove(session_id));
    if let Some(process) = process {
        if let Ok(mut child) = process.lock() {
            let _ = child.kill();
        }
    }
    // 一并清理该会话的权限服务（pending 请求统一按 deny 收尾）
    PermissionService::stop_for(session_id);
}

fn emit_error(app: &AppHandle, session_id: &str, message: String) {
    let _ = app.emit(
        "stream-event",
        StreamEvent::Error {
            session_id: session_id.to_string(),
            message,
        },
    );
    // 错误即终态：补发 stream-done 让前端走统一收尾
    let _ = app.emit("stream-done", json!({ "session_id": session_id }));
}

/// 定位 cc-space-mcp 二进制
///
/// 顺序：
/// 1. 环境变量 `CC_SPACE_MCP_BIN`（手动覆盖）
/// 2. 与当前进程同目录（生产打包后 sidecar 就在主程序旁）
/// 3. cargo target 目录（开发模式：current_exe 直接就是 target/{debug,release}/app）
fn find_mcp_binary() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CC_SPACE_MCP_BIN") {
        let path = PathBuf::from(p);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join("cc-space-mcp");
            if candidate.is_file() {
                return Some(candidate);
            }
            // dev: tauri 启动时 current_exe 在 target/debug，二进制名为 cc_space_mcp
            let candidate2 = dir.join("cc_space_mcp");
            if candidate2.is_file() {
                return Some(candidate2);
            }
        }
    }
    None
}

/// 读取进程 stdout，解析并 emit 事件
fn read_stream(process: Arc<Mutex<Child>>, app: &AppHandle, session_id: &str) {
    let stdout = {
        let mut child = process.lock().unwrap();
        match child.stdout.take() {
            Some(out) => out,
            None => return,
        }
    };

    let reader = BufReader::new(stdout);
    let mut buffered_result: Option<StreamEvent> = None;
    // partial messages 模式下，content_block_* 不携带 message_id，
    // 由 message_start 注入、后续 block 事件复用，直到下一个 message_start 覆盖
    let mut current_message_id: Option<String> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };

        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(event) = decode_stream_event(&value, session_id, &mut current_message_id) {
            match &event {
                StreamEvent::Result { .. } => {
                    // result 事件暂存，等进程退出后再发送
                    buffered_result = Some(event);
                }
                _ => {
                    let _ = app.emit("stream-event", &event);
                }
            }
        }
    }

    // 等待进程退出
    let exit_status = {
        let mut child = process.lock().unwrap();
        child.wait().ok()
    };

    // 检查非正常退出
    if let Some(status) = exit_status {
        if !status.success() {
            // 读取 stderr
            let stderr_text = {
                let mut child = process.lock().unwrap();
                child.stderr.take().map(|stderr| {
                    let mut reader = BufReader::new(stderr);
                    let mut buf = String::new();
                    let _ = std::io::Read::read_to_string(&mut reader, &mut buf);
                    buf
                })
            };
            if let Some(text) = stderr_text {
                if !text.trim().is_empty() {
                    let _ = app.emit("stream-event", StreamEvent::Error {
                        session_id: session_id.to_string(),
                        message: text.trim().to_string(),
                    });
                }
            }
        }
    }

    // 发送暂存的 result
    if let Some(result) = buffered_result {
        let _ = app.emit("stream-event", &result);
    }

    // 通知该会话的流结束
    let _ = app.emit("stream-done", json!({ "session_id": session_id }));

    // 清理引用——仅移除本 turn 自己的 child。同会话连发时新 turn 可能已注册新 child，
    // 按 session_id 盲删会误删新 turn 的进程句柄（之后 stop_streaming 找不到它、无法 kill）。
    // 用 Arc 指针身份校验：被新 turn 接管后不动。
    if let Some(map) = ACTIVE_PROCESSES.lock().unwrap().as_mut() {
        if map.get(session_id).is_some_and(|c| Arc::ptr_eq(c, &process)) {
            map.remove(session_id);
        }
    }
}

/// 解析单行 stream-json 为 StreamEvent
///
/// `current_message_id` 由 `stream_event/message_start` 写入，后续 content_block_* 事件读取——
/// CLI 的 partial messages envelope 是 Anthropic Messages API 原生 SSE 的透传，
/// 单个 block 事件本身不带 message_id，需要靠跨行状态拼接。
fn decode_stream_event(
    value: &Value,
    session_id: &str,
    current_message_id: &mut Option<String>,
) -> Option<StreamEvent> {
    let event_type = value.get("type")?.as_str()?;
    let sid = session_id.to_string();

    match event_type {
        "stream_event" => {
            // CLI envelope: { type: "stream_event", event: <Anthropic SSE event> }
            let inner = value.get("event")?;
            let inner_type = inner.get("type")?.as_str()?;
            match inner_type {
                "message_start" => {
                    // 仅更新跨行状态,不 emit:让 content_block_start 自带建 turn 能力
                    let id = inner
                        .get("message")?
                        .get("id")?
                        .as_str()?
                        .to_string();
                    *current_message_id = Some(id);
                    None
                }
                "content_block_start" => {
                    let mid = current_message_id.as_ref()?.clone();
                    let index = inner.get("index")?.as_u64()? as usize;
                    let cb_value = inner.get("content_block")?.clone();
                    let content_block: ContentBlock =
                        serde_json::from_value(cb_value).ok()?;
                    Some(StreamEvent::BlockStart {
                        session_id: sid,
                        message_id: mid,
                        index,
                        content_block,
                    })
                }
                "content_block_delta" => {
                    let mid = current_message_id.as_ref()?.clone();
                    let index = inner.get("index")?.as_u64()? as usize;
                    let delta = inner.get("delta")?.clone();
                    Some(StreamEvent::BlockDelta {
                        session_id: sid,
                        message_id: mid,
                        index,
                        delta,
                    })
                }
                "content_block_stop" => {
                    let mid = current_message_id.as_ref()?.clone();
                    let index = inner.get("index")?.as_u64()? as usize;
                    Some(StreamEvent::BlockStop {
                        session_id: sid,
                        message_id: mid,
                        index,
                    })
                }
                // message_delta / message_stop 忽略,终结由 result 事件兜底
                _ => None,
            }
        }
        "assistant" => {
            let msg = value.get("message")?;
            let message_id = msg
                .get("id")
                .and_then(|v| v.as_str())
                .or_else(|| value.get("uuid").and_then(|v| v.as_str()))
                .unwrap_or("unknown")
                .to_string();
            let content: Vec<ContentBlock> = msg
                .get("content")
                .and_then(|c| serde_json::from_value(c.clone()).ok())
                .unwrap_or_default();
            Some(StreamEvent::AssistantMessage {
                session_id: sid,
                message_id,
                content,
            })
        }
        "progress" => {
            // data.message.type 必须是 "assistant"
            let data = value.get("data")?;
            let outer_msg = data.get("message")?;
            if outer_msg.get("type")?.as_str()? != "assistant" {
                return None;
            }
            let inner_msg = outer_msg.get("message")?;
            let message_id = inner_msg
                .get("id")
                .and_then(|v| v.as_str())
                .or_else(|| value.get("uuid").and_then(|v| v.as_str()))
                .unwrap_or("unknown")
                .to_string();
            let content: Vec<ContentBlock> = inner_msg
                .get("content")
                .and_then(|c| serde_json::from_value(c.clone()).ok())
                .unwrap_or_default();
            Some(StreamEvent::AssistantMessage {
                session_id: sid,
                message_id,
                content,
            })
        }
        "result" => {
            let result_text = value
                .get("result")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let is_error = value
                .get("is_error")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if is_error {
                Some(StreamEvent::Error {
                    session_id: sid,
                    message: result_text,
                })
            } else {
                let cost = value
                    .get("total_cost_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                Some(StreamEvent::Result {
                    session_id: sid,
                    text: result_text,
                    cost_usd: cost,
                })
            }
        }
        _ => None, // system, last-prompt 等忽略
    }
}
