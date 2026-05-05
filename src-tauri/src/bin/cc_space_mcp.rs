//! cc-space MCP server
//!
//! 一个最小化的 stdio JSON-RPC 2.0 server，作为 claude CLI 的 `--permission-prompt-tool` 工具实现。
//!
//! 协议要点：
//! - stdin/stdout 用作与 claude CLI 之间的 MCP JSON-RPC 通道（每行一个 JSON 对象）
//! - 仅实现 MCP 三个核心方法：`initialize` / `tools/list` / `tools/call`
//! - 暴露唯一工具 `approve_tool_use`，签名 `{ tool_name: string, input: object }`
//!   - 通过环境变量 `CC_SPACE_PERMISSION_SOCK` 找到主进程的 Unix socket
//!   - 把请求转发给主进程，等主进程返回前端用户决策
//!   - 把决策包装成 MCP `tools/call` 返回值（content 为单条 text，text 内容是 JSON 字符串）
//!
//! 与 claude CLI（参考 docs.claude.com/en/agent-sdk/user-input）契约：
//!   Allow → `{ "behavior": "allow", "updatedInput": <object> }`
//!   Deny  → `{ "behavior": "deny",  "message": "<reason>" }`

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{json, Value};

/// MCP server 元信息（与 claude CLI 协商时返回）
const SERVER_NAME: &str = "cc-space";
const SERVER_VERSION: &str = "0.1.0";
/// MCP 协议版本（写死一个常用版本，claude CLI 容忍主版本号匹配）
const PROTOCOL_VERSION: &str = "2024-11-05";

/// 主进程 socket 通信超时（>= PRD 60s + 余量）
const SOCKET_READ_TIMEOUT_SECS: u64 = 75;

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdout_lock = stdout.lock();
    let reader = BufReader::new(stdin.lock());

    for line in reader.lines() {
        let Ok(line) = line else { break };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                // 解析失败，写一个 parse error 但保持连接（id 用 null）
                let resp = error_response(Value::Null, -32700, "Parse error");
                write_response(&mut stdout_lock, &resp);
                continue;
            }
        };

        // notification（无 id）：MCP 里通知不返回响应
        let id = req.get("id").cloned();
        let is_notification = id.is_none() || id.as_ref().is_some_and(Value::is_null);

        let method = req.get("method").and_then(Value::as_str).unwrap_or("");

        let response = match method {
            "initialize" => handle_initialize(id.clone().unwrap_or(Value::Null)),
            "tools/list" => handle_tools_list(id.clone().unwrap_or(Value::Null)),
            "tools/call" => handle_tools_call(id.clone().unwrap_or(Value::Null), &req),
            // 收到 notifications/initialized 等通知，不响应
            m if m.starts_with("notifications/") => continue,
            _ if is_notification => continue,
            _ => error_response(
                id.clone().unwrap_or(Value::Null),
                -32601,
                &format!("Method not found: {}", method),
            ),
        };

        if !is_notification {
            write_response(&mut stdout_lock, &response);
        }
    }
}

/// 写一行 JSON 到 stdout（每条消息独占一行）
fn write_response<W: Write>(out: &mut W, value: &Value) {
    if let Ok(s) = serde_json::to_string(value) {
        let _ = out.write_all(s.as_bytes());
        let _ = out.write_all(b"\n");
        let _ = out.flush();
    }
}

fn handle_initialize(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": PROTOCOL_VERSION,
            "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
            "capabilities": { "tools": {} }
        }
    })
}

fn handle_tools_list(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "approve_tool_use",
                    "description": "Approve or deny a tool use request from the user.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "tool_name": { "type": "string" },
                            "input": { "type": "object" }
                        },
                        "required": ["tool_name", "input"]
                    }
                }
            ]
        }
    })
}

fn handle_tools_call(id: Value, req: &Value) -> Value {
    // 提取参数
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    if name != "approve_tool_use" {
        return error_response(id, -32602, &format!("Unknown tool: {}", name));
    }
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    let tool_name = arguments
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let tool_input = arguments
        .get("input")
        .cloned()
        .unwrap_or_else(|| json!({}));

    // 走主进程 socket 等待用户决策
    let decision = ask_main_process(&tool_name, &tool_input).unwrap_or_else(|err| {
        // 通信失败，按 deny 处理（保安全）
        json!({
            "behavior": "deny",
            "message": format!("权限服务通信失败：{}", err)
        })
    });

    // MCP tools/call result：content 数组，单条 text，text 内容是 JSON 字符串
    let text = serde_json::to_string(&decision).unwrap_or_else(|_| {
        "{\"behavior\":\"deny\",\"message\":\"内部序列化错误\"}".to_string()
    });

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [ { "type": "text", "text": text } ]
        }
    })
}

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

/// 走 Unix socket 与主进程通信。
///
/// 通信协议（自定义、行分隔 JSON）：
/// 1. 连上 `CC_SPACE_PERMISSION_SOCK`
/// 2. 发送一行：`{"toolName": "...", "input": <object>}`
/// 3. 等一行：`{"behavior": "allow"|"deny", "updatedInput"?: ..., "message"?: ...}`
fn ask_main_process(tool_name: &str, tool_input: &Value) -> Result<Value, String> {
    let sock_path = std::env::var("CC_SPACE_PERMISSION_SOCK")
        .map_err(|_| "环境变量 CC_SPACE_PERMISSION_SOCK 未设置".to_string())?;

    let mut stream = UnixStream::connect(&sock_path)
        .map_err(|e| format!("连接 {} 失败：{}", sock_path, e))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(SOCKET_READ_TIMEOUT_SECS)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;

    let req_line = serde_json::to_string(&json!({
        "toolName": tool_name,
        "input": tool_input,
    }))
    .map_err(|e| e.to_string())?;
    stream
        .write_all(req_line.as_bytes())
        .map_err(|e| format!("发送请求失败：{}", e))?;
    stream
        .write_all(b"\n")
        .map_err(|e| format!("发送换行失败：{}", e))?;
    stream.flush().map_err(|e| e.to_string())?;

    // 读一行响应
    let mut reader = BufReader::new(stream);
    let mut buf = String::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = reader
            .read(&mut tmp)
            .map_err(|e| format!("读响应失败：{}", e))?;
        if n == 0 {
            break;
        }
        buf.push_str(&String::from_utf8_lossy(&tmp[..n]));
        if buf.contains('\n') {
            break;
        }
    }

    let line = buf
        .lines()
        .next()
        .ok_or_else(|| "主进程未返回任何数据".to_string())?;
    let decision: Value = serde_json::from_str(line)
        .map_err(|e| format!("解析主进程响应失败：{}", e))?;

    // 校验：只允许 allow / deny 两种
    let behavior = decision
        .get("behavior")
        .and_then(Value::as_str)
        .unwrap_or("");
    match behavior {
        "allow" => {
            // 缺 updatedInput 时回填原始 input，避免 claude CLI 那边 fallback warn
            let mut out = decision.clone();
            if out.get("updatedInput").is_none() {
                if let Some(obj) = out.as_object_mut() {
                    obj.insert("updatedInput".to_string(), tool_input.clone());
                }
            }
            Ok(out)
        }
        "deny" => Ok(decision),
        _ => Err(format!("主进程返回未知 behavior：{:?}", behavior)),
    }
}
