use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;

use serde_json::{json, Value};

use crate::metadata::agent_cwd;
use crate::streaming::{enhanced_path, find_claude};

struct AgentProcess {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
    request_count: u64,
}

static AGENT: Mutex<Option<AgentProcess>> = Mutex::new(None);

fn spawn_agent() -> Result<AgentProcess, String> {
    let (executable, prefix_args) = find_claude();
    eprintln!("[agent-service] spawn: executable={} prefix_args={:?}", executable, prefix_args);
    let mut args = prefix_args;
    let session_id = uuid::Uuid::new_v4().to_string();
    args.extend([
        "--session-id".to_string(),
        session_id,
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--input-format".to_string(),
        "stream-json".to_string(),
        "--model".to_string(),
        "claude-haiku-4-5-20251001".to_string(),
        "--effort".to_string(),
        "low".to_string(),
        "--verbose".to_string(),
    ]);

    eprintln!("[agent-service] args: {:?}", args);

    let mut child = Command::new(&executable)
        .args(&args)
        .current_dir(agent_cwd())
        .env("PATH", enhanced_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("AgentService spawn 失败: {}", e))?;

    let mut stdin = child.stdin.take().ok_or("无法获取 agent stdin")?;
    let stdout = child.stdout.take().ok_or("无法获取 agent stdout")?;
    let stderr = child.stderr.take();
    let mut reader = BufReader::new(stdout);

    // 后台读 stderr 打日志
    if let Some(se) = stderr {
        std::thread::spawn(move || {
            let r = BufReader::new(se);
            for line in r.lines().flatten() {
                eprintln!("[agent-stderr] {}", line);
            }
        });
    }

    // 初始化握手
    let init = json!({
        "type": "control_request",
        "request_id": "agent-init",
        "request": {"subtype": "initialize"}
    });
    write_line(&mut stdin, &init)?;

    // 等 control_response（10 秒超时）
    let mut buf = String::new();
    eprintln!("[agent-service] 等待握手响应...");
    let handshake_deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if std::time::Instant::now() > handshake_deadline {
            let _ = child.kill();
            return Err("agent 握手超时(10s)".to_string());
        }
        buf.clear();
        let n = reader.read_line(&mut buf).map_err(|e| format!("agent 握手读取失败: {}", e))?;
        if n == 0 {
            return Err("agent 进程在握手阶段退出".to_string());
        }
        let trimmed = buf.trim_end();
        let preview: String = trimmed.chars().take(120).collect();
        eprintln!("[agent-service] 握手行: {}", preview);
        if let Ok(v) = serde_json::from_str::<Value>(&buf) {
            if v.get("type").and_then(|t| t.as_str()) == Some("control_response") {
                break;
            }
        }
    }

    eprintln!("[agent-service] 进程已启动 PID={}", child.id());

    Ok(AgentProcess {
        child,
        stdin,
        stdout: reader,
        request_count: 0,
    })
}

fn write_line(stdin: &mut ChildStdin, msg: &Value) -> Result<(), String> {
    let line = serde_json::to_string(msg).map_err(|e| format!("JSON 序列化失败: {}", e))?;
    stdin
        .write_all(line.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|e| format!("agent stdin 写入失败: {}", e))
}

/// 向 AgentService 发送请求，阻塞等待响应文本。
/// prompt 中包含角色指令 + 实际内容（不用 system prompt，一个进程服务多种角色）。
pub(crate) fn request_blocking_pub(prompt: &str) -> Result<String, String> {
    request_blocking(prompt)
}

fn request_blocking(prompt: &str) -> Result<String, String> {
    let start = std::time::Instant::now();
    let preview: String = prompt.chars().take(40).collect();
    eprintln!("[agent-service] request: prompt={}...", preview);
    let mut guard = AGENT.lock().unwrap_or_else(|e| e.into_inner());

    // 检查进程是否存活，不存活则重新 spawn
    let need_spawn = match &mut *guard {
        Some(agent) => {
            let dead = agent.child.try_wait().ok().flatten().is_some();
            if dead { eprintln!("[agent-service] 进程已死，需重新 spawn"); }
            dead
        }
        None => {
            eprintln!("[agent-service] 进程不存在，需 spawn");
            true
        }
    };
    if need_spawn {
        *guard = Some(spawn_agent()?);
    }

    let agent = guard.as_mut().unwrap();
    agent.request_count += 1;

    // 每 100 次重启清上下文
    if agent.request_count > 100 {
        eprintln!("[agent-service] 达到 100 次请求，重启清上下文");
        let _ = agent.child.kill();
        *guard = Some(spawn_agent()?);
        let agent = guard.as_mut().unwrap();
        return send_and_collect(agent, prompt);
    }

    send_and_collect(agent, prompt)
}

fn send_and_collect(agent: &mut AgentProcess, prompt: &str) -> Result<String, String> {
    let start = std::time::Instant::now();
    let msg = json!({
        "type": "user",
        "session_id": "",
        "message": {
            "role": "user",
            "content": [{"type": "text", "text": prompt}]
        },
        "parent_tool_use_id": null
    });
    write_line(&mut agent.stdin, &msg)?;
    eprintln!("[agent-service] 已发送 prompt，等待响应...");

    // 读取 stdout 直到收到 result 事件
    let mut text_parts: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut event_count = 0u32;

    loop {
        buf.clear();
        let n = agent.stdout.read_line(&mut buf)
            .map_err(|e| format!("agent stdout 读取失败: {}", e))?;
        if n == 0 {
            eprintln!("[agent-service] stdout EOF，进程退出");
            return Err("agent 进程意外退出".to_string());
        }

        let Ok(v) = serde_json::from_str::<Value>(&buf) else {
            eprintln!("[agent-service] 非 JSON 行: {}", buf.trim_end());
            continue;
        };

        let Some(event_type) = v.get("type").and_then(|t| t.as_str()) else {
            continue;
        };

        event_count += 1;
        if event_count <= 3 || event_type == "result" {
            eprintln!("[agent-service] event #{}: type={}", event_count, event_type);
        }

        match event_type {
            "stream_event" => {
                if let Some(inner) = v.get("event") {
                    let inner_type = inner.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if inner_type == "content_block_delta" {
                        if let Some(delta) = inner.get("delta") {
                            if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                text_parts.push(text.to_string());
                            }
                        }
                    }
                }
            }
            "assistant" | "progress" => {
                // 非流式模式下文本可能整块到达
                let msg_obj = if event_type == "assistant" {
                    v.get("message")
                } else {
                    v.get("data").and_then(|d| d.get("message")).and_then(|m| m.get("message"))
                };
                if let Some(msg) = msg_obj {
                    if let Some(content) = msg.get("content").and_then(|c| c.as_array()) {
                        for block in content {
                            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                                    text_parts.push(t.to_string());
                                }
                            }
                        }
                    }
                }
            }
            "result" => {
                let is_error = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
                let result_text = v.get("result").and_then(|r| r.as_str()).unwrap_or("");
                let preview: String = result_text.chars().take(80).collect();
                eprintln!("[agent-service] result: is_error={} text={}... elapsed={:?}",
                    is_error, preview, start.elapsed());
                if is_error {
                    return Err(format!("agent 返回错误: {}", result_text));
                }
                // result.result 作为最终 fallback
                if text_parts.is_empty() && !result_text.is_empty() {
                    text_parts.push(result_text.to_string());
                }
                break;
            }
            _ => {}
        }
    }

    let result = text_parts.join("").trim().to_string();
    eprintln!("[agent-service] 完成: {}字 events={} elapsed={:?}",
        result.chars().count(), event_count, start.elapsed());
    if result.is_empty() {
        return Err("agent 返回为空".to_string());
    }
    Ok(result)
}

/// 初始化 AgentService（app 启动时调用）
pub fn init() {
    std::thread::spawn(|| {
        let start = std::time::Instant::now();
        eprintln!("[agent-service] 初始化开始...");
        let mut guard = AGENT.lock().unwrap_or_else(|e| e.into_inner());
        match spawn_agent() {
            Ok(agent) => {
                eprintln!("[agent-service] 初始化完成 elapsed={:?}", start.elapsed());
                *guard = Some(agent);
            }
            Err(e) => {
                eprintln!("[agent-service] 启动失败: {} elapsed={:?}", e, start.elapsed());
            }
        }
    });
}

/// 关闭 AgentService（app 退出时调用）
pub fn shutdown() {
    let mut guard = AGENT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut agent) = guard.take() {
        let _ = agent.child.kill();
        eprintln!("[agent-service] 已关闭");
    }
}

// --- 公开的 agent 能力 ---

/// 生成权限批注
pub fn permission_hint(tool_name: &str, input_json: &str) -> Result<String, String> {
    let truncated = if input_json.len() > 1500 {
        format!("{}…(已截断)", &input_json[..1500])
    } else {
        input_json.to_string()
    };
    let prompt = format!(
        "【角色：权限决策助手】用户正在审批一个工具调用请求。用一句简洁的中文解释这个操作在做什么，如有风险请指出。只输出解释本身，不超过50字。\n\n工具：{}\n参数：\n{}",
        tool_name, truncated
    );
    request_blocking(&prompt)
}

/// 生成会话标题
pub fn generate_title(snippet: &str) -> Result<String, String> {
    let prompt = format!(
        "【角色：标题生成器】根据对话内容生成一个10字以内的中文标题。只输出标题本身，不要加引号、标点或任何其他内容。\n\n对话内容：\n{}",
        snippet
    );
    request_blocking(&prompt)
}

/// 解读 settings 字段——不是翻译，是专家解释
pub fn translate_settings(fields_json: &str) -> Result<String, String> {
    let prompt = format!(
        "【角色：Claude Code 配置专家】你深度理解 Claude Code CLI 的每个配置项。\
        输入是 JSON 数组，每项有 key（settings.json 字段名）和 description（官方英文说明）。\n\n\
        对每个字段，输出：\n\
        - key：原字段名\n\
        - name：中文简称（≤6字，如「自动记忆」「沙箱配置」）\n\
        - desc：面向用户的中文解读（≤60字）——不要翻译英文原文，而是用大白话说清楚：\
          这个开关/值实际控制什么行为？开了/关了/改了会怎样？什么人需要关注它？\n\n\
        输出纯 JSON 数组，不要 markdown 代码块、不要其他文字。\n\n{}",
        fields_json
    );
    request_blocking(&prompt)
}

/// 自然语言转 cron 表达式
pub fn parse_cron(text: &str) -> Result<String, String> {
    let prompt = format!(
        "【角色：cron 表达式转换器】将用户的自然语言时间描述转换为标准 5 字段 cron 表达式（分 时 日 月 周）。只输出 cron 表达式本身，不要任何解释。如果无法识别，输出 INVALID。\n\n用户输入：{}",
        text
    );
    let result = request_blocking(&prompt)?;
    if result == "INVALID" {
        return Err("无法识别时间描述".to_string());
    }
    let parts: Vec<&str> = result.split_whitespace().collect();
    if parts.len() != 5 {
        return Err(format!("返回的不是有效 cron 表达式: {}", result));
    }
    Ok(result)
}
