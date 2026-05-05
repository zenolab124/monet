use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::models::ContentBlock;
use crate::permission::PermissionService;

/// 活跃的 streaming 进程句柄
static ACTIVE_PROCESS: Mutex<Option<Arc<Mutex<Child>>>> = Mutex::new(None);

/// 前端接收的流式事件
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    /// 助手消息内容更新
    AssistantMessage {
        message_id: String,
        content: Vec<ContentBlock>,
    },
    /// 流式完成
    Result {
        text: String,
        cost_usd: f64,
    },
    /// 错误
    Error {
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

/// 启动流式会话
pub fn start_streaming(
    app: &AppHandle,
    session_id: &str,
    cwd: &str,
    message: &str,
    model: Option<&str>,
    effort: Option<&str>,
) {
    // 先终止已有进程
    stop_streaming();

    // 1. 启动权限服务（失败立即上报错误，不静默降级）
    let permission_service = match PermissionService::start(app.clone()) {
        Ok(s) => s,
        Err(e) => {
            let _ = app.emit(
                "stream-event",
                StreamEvent::Error {
                    message: format!("权限服务启动失败：{}", e),
                },
            );
            return;
        }
    };
    let socket_path = permission_service.socket_path().to_path_buf();

    // 2. 定位 cc-space-mcp 二进制
    let mcp_binary = match find_mcp_binary() {
        Some(p) => p,
        None => {
            PermissionService::stop_global();
            let _ = app.emit(
                "stream-event",
                StreamEvent::Error {
                    message: "未找到 cc-space-mcp 二进制，无法启动权限服务".to_string(),
                },
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
    args.extend([
        "--resume".to_string(),
        session_id.to_string(),
        "--print".to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
        "--mcp-config".to_string(),
        mcp_config,
        "--permission-prompt-tool".to_string(),
        "mcp__cc-space__approve_tool_use".to_string(),
    ]);
    // thinking 摘要由全局 settings.json 的 env.CLAUDE_CODE_EXTRA_BODY 注入到 API body,
    // 解决 vscode 插件 / cc-space / 终端三端一致问题(claude-code issue #49902 workaround)
    if let Some(m) = model.filter(|s| !s.is_empty()) {
        args.push("--model".to_string());
        args.push(m.to_string());
    }
    if let Some(e) = effort.filter(|s| !s.is_empty()) {
        args.push("--effort".to_string());
        args.push(e.to_string());
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
            PermissionService::stop_global();
            let _ = app.emit(
                "stream-event",
                StreamEvent::Error {
                    message: format!("启动 claude 失败: {}", e),
                },
            );
            return;
        }
    };

    let child = Arc::new(Mutex::new(child));
    *ACTIVE_PROCESS.lock().unwrap() = Some(child.clone());

    let handle = app.clone();
    std::thread::spawn(move || {
        read_stream(child, &handle);
        // 子进程退出后回收权限服务
        PermissionService::stop_global();
    });
}

/// 终止当前流式进程
pub fn stop_streaming() {
    if let Some(process) = ACTIVE_PROCESS.lock().unwrap().take() {
        if let Ok(mut child) = process.lock() {
            let _ = child.kill();
        }
    }
    // 一并清理权限服务（pending 请求统一按 deny 收尾）
    PermissionService::stop_global();
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
fn read_stream(process: Arc<Mutex<Child>>, app: &AppHandle) {
    let stdout = {
        let mut child = process.lock().unwrap();
        match child.stdout.take() {
            Some(out) => out,
            None => return,
        }
    };

    let reader = BufReader::new(stdout);
    let mut buffered_result: Option<StreamEvent> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };

        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if let Some(event) = decode_stream_event(&value) {
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

    // 通知流结束
    let _ = app.emit("stream-done", ());

    // 清理引用
    *ACTIVE_PROCESS.lock().unwrap() = None;
}

/// 解析单行 stream-json 为 StreamEvent
fn decode_stream_event(value: &Value) -> Option<StreamEvent> {
    let event_type = value.get("type")?.as_str()?;

    match event_type {
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
            Some(StreamEvent::AssistantMessage { message_id, content })
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
            Some(StreamEvent::AssistantMessage { message_id, content })
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
                Some(StreamEvent::Error { message: result_text })
            } else {
                let cost = value
                    .get("total_cost_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                Some(StreamEvent::Result {
                    text: result_text,
                    cost_usd: cost,
                })
            }
        }
        _ => None, // system, last-prompt 等忽略
    }
}
