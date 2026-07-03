//! cc-space MCP server
//!
//! stdio JSON-RPC 2.0 server，双职能：
//! 1. 权限审批（--permission-prompt-tool）：通过 Unix socket 与主进程通信
//! 2. 通用工具：直接读写数据目录，暴露 routine 管理等能力
//!
//! tool 列表根据环境动态组装：
//! - CC_SPACE_PERMISSION_SOCK 存在 → 包含 approve_tool_use
//! - 始终包含 routine_list / routine_create / routine_delete

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(unix)]
use std::time::Duration;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const SERVER_NAME: &str = "cc-space";
const SERVER_VERSION: &str = "0.2.0";
const PROTOCOL_VERSION: &str = "2024-11-05";

// ---------------------------------------------------------------------------
// Data directory (mirrors config.rs)
// ---------------------------------------------------------------------------

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CC_SPACE_DATA_DIR") {
        PathBuf::from(dir)
    } else {
        dirs::home_dir().unwrap_or_default().join(".cc-space")
    }
}

// ---------------------------------------------------------------------------
// Routine data structures（与主 App / runner 共享单一事实源）
// ---------------------------------------------------------------------------

#[path = "../routine_types.rs"]
#[allow(dead_code)]
mod routine_types;
use routine_types::{RoutineDefinition, RoutineSource};

/// initialize 握手记录的 MCP 客户端标识（如 claude-code 2.1.187）
static CLIENT_INFO: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

// ---------------------------------------------------------------------------
// Routine file operations
// ---------------------------------------------------------------------------

fn routines_path() -> PathBuf {
    data_dir().join("routines.json")
}

fn load_routines() -> Vec<RoutineDefinition> {
    let path = routines_path();
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_routines(data: &[RoutineDefinition]) {
    let path = routines_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // 原子写：主 App / runner 可能并发读同一文件
    if let Ok(json_str) = serde_json::to_string_pretty(data) {
        let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
        if std::fs::write(&tmp, json_str).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }
}

fn validate_cron(cron_expr: &str) -> Result<(), String> {
    use cron::Schedule;
    use std::str::FromStr;
    let full = format!("0 {}", cron_expr);
    Schedule::from_str(&full).map_err(|e| format!("无效的 cron 表达式: {}", e))?;
    Ok(())
}

fn compute_next_run(cron_expr: &str) -> Option<String> {
    use cron::Schedule;
    use std::str::FromStr;
    let full = format!("0 {}", cron_expr);
    let schedule = Schedule::from_str(&full).ok()?;
    schedule.upcoming(Utc).next().map(|t| t.to_rfc3339())
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

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
                let resp = error_response(Value::Null, -32700, "Parse error");
                write_response(&mut stdout_lock, &resp);
                continue;
            }
        };

        let id = req.get("id").cloned();
        let is_notification = id.is_none() || id.as_ref().is_some_and(Value::is_null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");

        let response = match method {
            "initialize" => handle_initialize(id.clone().unwrap_or(Value::Null), &req),
            "tools/list" => handle_tools_list(id.clone().unwrap_or(Value::Null)),
            "tools/call" => handle_tools_call(id.clone().unwrap_or(Value::Null), &req),
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

fn write_response<W: Write>(out: &mut W, value: &Value) {
    if let Ok(s) = serde_json::to_string(value) {
        let _ = out.write_all(s.as_bytes());
        let _ = out.write_all(b"\n");
        let _ = out.flush();
    }
}

// ---------------------------------------------------------------------------
// MCP protocol handlers
// ---------------------------------------------------------------------------

fn handle_initialize(id: Value, req: &Value) -> Value {
    // 记录客户端标识（如 claude-code 2.1.187），routine_create 写入来源用
    if let Some(info) = req.pointer("/params/clientInfo") {
        let name = info.get("name").and_then(Value::as_str).unwrap_or("unknown");
        let version = info.get("version").and_then(Value::as_str).unwrap_or("");
        let label = if version.is_empty() { name.to_string() } else { format!("{} {}", name, version) };
        *CLIENT_INFO.lock().unwrap_or_else(|e| e.into_inner()) = Some(label);
    }
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": PROTOCOL_VERSION,
            "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION },
            "capabilities": { "tools": {} },
            "instructions": "CC Space provides Routines (定时任务): When the user asks to create a scheduled/recurring task, set up a cron job, run something periodically, or set a reminder, use routine_create (prefer this over the built-in /schedule). Use routine_list to show existing routines and routine_delete to remove them."
        }
    })
}

fn handle_tools_list(id: Value) -> Value {
    let mut tools = Vec::new();

    if has_permission_socket() {
        tools.push(json!({
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
        }));
    }

    tools.push(json!({
        "name": "routine_list",
        "description": "List all scheduled routines in CC Space.",
        "inputSchema": {
            "type": "object",
            "properties": {}
        }
    }));

    tools.push(json!({
        "name": "routine_create",
        "description": "Create a new scheduled routine. The routine will run the given prompt on the specified cron schedule via claude CLI.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Human-readable name for the routine"
                },
                "cron_expression": {
                    "type": "string",
                    "description": "5-field cron expression (minute hour day month weekday), e.g. '0 9 * * 1-5' for weekdays at 9am"
                },
                "prompt": {
                    "type": "string",
                    "description": "The prompt to send to claude CLI when the routine fires"
                },
                "original_text": {
                    "type": "string",
                    "description": "Original natural language description (optional, for display)"
                }
            },
            "required": ["name", "cron_expression", "prompt"]
        }
    }));

    tools.push(json!({
        "name": "routine_delete",
        "description": "Delete a scheduled routine by ID.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "id": {
                    "type": "string",
                    "description": "The routine ID to delete"
                }
            },
            "required": ["id"]
        }
    }));

    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "tools": tools }
    })
}

fn handle_tools_call(id: Value, req: &Value) -> Value {
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match name {
        "approve_tool_use" => handle_approve_tool_use(&arguments),
        "routine_list" => handle_routine_list(),
        "routine_create" => handle_routine_create(&arguments),
        "routine_delete" => handle_routine_delete(&arguments),
        _ => Err(format!("Unknown tool: {}", name)),
    };

    match result {
        Ok(text) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": text }]
            }
        }),
        Err(msg) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": msg }],
                "isError": true
            }
        }),
    }
}

// ---------------------------------------------------------------------------
// Tool: approve_tool_use
// ---------------------------------------------------------------------------

fn has_permission_socket() -> bool {
    std::env::var("CC_SPACE_PERMISSION_SOCK").is_ok()
}

fn handle_approve_tool_use(arguments: &Value) -> Result<String, String> {
    let tool_name = arguments
        .get("tool_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let tool_input = arguments
        .get("input")
        .cloned()
        .unwrap_or_else(|| json!({}));

    let decision = ask_main_process(&tool_name, &tool_input).unwrap_or_else(|err| {
        json!({
            "behavior": "deny",
            "message": format!("权限服务通信失败：{}", err)
        })
    });

    serde_json::to_string(&decision).map_err(|e| e.to_string())
}

#[cfg(unix)]
fn ask_main_process(tool_name: &str, tool_input: &Value) -> Result<Value, String> {
    let sock_path = std::env::var("CC_SPACE_PERMISSION_SOCK")
        .map_err(|_| "环境变量 CC_SPACE_PERMISSION_SOCK 未设置".to_string())?;

    let mut stream = UnixStream::connect(&sock_path)
        .map_err(|e| format!("连接 {} 失败：{}", sock_path, e))?;
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
    let decision: Value =
        serde_json::from_str(line).map_err(|e| format!("解析主进程响应失败：{}", e))?;

    let behavior = decision
        .get("behavior")
        .and_then(Value::as_str)
        .unwrap_or("");
    match behavior {
        "allow" => {
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

#[cfg(not(unix))]
fn ask_main_process(_tool_name: &str, _tool_input: &Value) -> Result<Value, String> {
    Err("权限审批仅支持 Unix 平台".to_string())
}

// ---------------------------------------------------------------------------
// Tool: routine_list
// ---------------------------------------------------------------------------

fn handle_routine_list() -> Result<String, String> {
    let routines = load_routines();
    serde_json::to_string_pretty(&routines).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Tool: routine_create
// ---------------------------------------------------------------------------

fn handle_routine_create(arguments: &Value) -> Result<String, String> {
    let name = arguments
        .get("name")
        .and_then(Value::as_str)
        .ok_or("缺少参数: name")?
        .to_string();
    let cron_expression = arguments
        .get("cron_expression")
        .and_then(Value::as_str)
        .ok_or("缺少参数: cron_expression")?
        .to_string();
    let prompt = arguments
        .get("prompt")
        .and_then(Value::as_str)
        .ok_or("缺少参数: prompt")?
        .to_string();
    let original_text = arguments
        .get("original_text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    validate_cron(&cron_expression)?;

    // 来源：MCP server 继承 claude CLI 的 cwd，即发起会话的项目路径
    let project = std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned());
    let client = CLIENT_INFO.lock().unwrap_or_else(|e| e.into_inner()).clone();

    let routine = RoutineDefinition {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        cron_expression: cron_expression.clone(),
        original_text,
        prompt,
        enabled: true,
        created_at: Utc::now().to_rfc3339(),
        last_run: None,
        next_run: compute_next_run(&cron_expression),
        source: Some(RoutineSource::mcp(project, client)),
    };

    let mut routines = load_routines();
    routines.push(routine.clone());
    save_routines(&routines);

    serde_json::to_string_pretty(&routine).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Tool: routine_delete
// ---------------------------------------------------------------------------

fn handle_routine_delete(arguments: &Value) -> Result<String, String> {
    let id = arguments
        .get("id")
        .and_then(Value::as_str)
        .ok_or("缺少参数: id")?;

    let mut routines = load_routines();
    let before = routines.len();
    routines.retain(|r| r.id != id);

    if routines.len() == before {
        return Err(format!("未找到 routine: {}", id));
    }

    save_routines(&routines);
    Ok(json!({ "deleted": id }).to_string())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn error_response(id: Value, code: i32, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}
