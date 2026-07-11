use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::models::ContentBlock;
use crate::permission::PermissionService;

/// 按 Claude Code 优先级链读取 permissions.defaultMode
/// Local (.claude/settings.local.json) > Project (.claude/settings.json) > User (~/.claude/settings.local.json) > User (~/.claude/settings.json)
pub fn resolve_default_permission_mode(cwd: &str) -> Option<String> {
    let read_mode = |path: PathBuf| -> Option<String> {
        let text = std::fs::read_to_string(&path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        json.get("permissions")?.get("defaultMode")?.as_str().map(String::from)
    };
    let cwd_path = std::path::Path::new(cwd);
    let candidates = [
        cwd_path.join(".claude/settings.local.json"),
        cwd_path.join(".claude/settings.json"),
        dirs::home_dir().map(|h| h.join(".claude/settings.local.json")).unwrap_or_default(),
        dirs::home_dir().map(|h| h.join(".claude/settings.json")).unwrap_or_default(),
    ];
    candidates.into_iter().find_map(read_mode)
}

/// 长活进程会话实例（进程跨轮复用，不杀不重启）
struct SessionProcess {
    child: Child,
    stdin: ChildStdin,
    request_counter: u64,
    channel: Option<String>,
    effort: Option<String>,
    /// 进程当前生效的模型(spawn --model / set_model 下达值;None = CLI 默认)。
    /// 复用判定用:意图回落 None 而进程钉着旧值时必须重启,set_model 无法"切回默认"
    model: Option<String>,
    /// spawn 时的顾问开关(advisor 经 --settings 注入,变更只能重启生效)
    advisor: bool,
}

/// 活跃的长活进程表（sessionId → SessionProcess）
static ACTIVE_PROCESSES: Mutex<Option<HashMap<String, Arc<Mutex<SessionProcess>>>>> =
    Mutex::new(None);

/// 获取 CC Space 为指定会话持有的长活进程 PID（check_session_running 排除自有进程用）
pub fn get_own_pid(session_id: &str) -> Option<u32> {
    ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id))
        .map(|sp| sp.lock().unwrap().child.id())
}

/// 前端接收的流式事件（全部携带 session_id，前端按会话分发）
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StreamEvent {
    /// 助手消息内容更新（终态快照，作为字符级 delta 的兜底校正）
    AssistantMessage {
        session_id: String,
        message_id: String,
        content: Vec<ContentBlock>,
        /// 本轮实际运行的模型(message.model 真值)
        model: Option<String>,
    },
    /// 字符级增量到达——content_block_start：某个 index 上出现新块
    BlockStart {
        session_id: String,
        message_id: String,
        index: usize,
        content_block: ContentBlock,
        /// 本轮实际运行的模型(message_start 事件 message.model 真值,随首块带出)
        model: Option<String>,
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
    /// 流式完成（携带 modelUsage 真值：上下文容量 + token 用量）
    Result {
        session_id: String,
        text: String,
        cost_usd: f64,
        context_window: Option<u64>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    /// 错误
    Error {
        session_id: String,
        message: String,
    },
}

/// 查找 claude 可执行文件路径（薄 wrapper，事实源在 claude_locator）。
/// 探测全败时保留 env fallback 兜底，行为不比旧版倒退；
/// 需要结构化错误的消费方（定时任务、设置页）直接用 claude_locator。
pub fn find_claude() -> (String, Vec<String>) {
    match crate::claude_locator::locate() {
        Ok(l) => (l.path.to_string_lossy().into_owned(), vec![]),
        Err(e) => {
            eprintln!("[claude-locator] {}", e);
            ("/usr/bin/env".to_string(), vec!["claude".to_string()])
        }
    }
}

/// 构建增强 PATH 环境变量
pub fn enhanced_path() -> String {
    let home = dirs::home_dir().unwrap_or_default();
    let mut extra_paths = vec![
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        format!("{}/.local/bin", home.display()),
    ];

    // 检测 nvm node 路径（语义化取最新——字典序会让 v9.x 压过 v18.x）
    let nvm_dir = home.join(".nvm/versions/node");
    if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
        let latest = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let ver: Vec<u32> = name
                    .trim_start_matches('v')
                    .split('.')
                    .filter_map(|s| s.parse().ok())
                    .collect();
                (ver.len() == 3).then(|| (ver, e.path()))
            })
            .max_by(|a, b| a.0.cmp(&b.0));
        if let Some((_, path)) = latest {
            extra_paths.push(format!("{}/bin", path.display()));
        }
    }

    // Windows 下 PATH 分隔符是 ';'，硬编码 ':' 会损坏子进程 PATH
    let sep = if cfg!(windows) { ";" } else { ":" };
    let existing = std::env::var("PATH").unwrap_or_default();
    format!("{}{}{}", extra_paths.join(sep), sep, existing)
}

/// 向长活进程 stdin 写入一行 JSON
fn write_stdin(stdin: &mut ChildStdin, msg: &Value) -> Result<(), String> {
    let line = serde_json::to_string(msg).map_err(|e| format!("JSON 序列化失败: {}", e))?;
    stdin
        .write_all(line.as_bytes())
        .and_then(|_| stdin.write_all(b"\n"))
        .and_then(|_| stdin.flush())
        .map_err(|e| format!("stdin 写入失败: {}", e))
}

/// SIGTERM 指定 PID（macOS/Linux）
fn sigterm_pid(pid: u32) {
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();
}

/// 断链探测：同 id 的 jsonl 是否存在于其他 cwd 编码目录（如 EnterWorktree 迁走的）。
/// 命中说明"这不是新会话，是历史不在预期位置"——静默 --session-id 会以空历史
/// 重生同名会话，复刻「会话被覆盖」事故。
fn find_session_elsewhere(session_id: &str, expected: &Path) -> Option<PathBuf> {
    let root = crate::commands::projects_dir();
    let name = format!("{}.jsonl", session_id);
    for entry in std::fs::read_dir(&root).ok()?.filter_map(|e| e.ok()) {
        let candidate = entry.path().join(&name);
        if candidate != expected && candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// 打开会话进程：spawn 长活 CLI + 初始化握手 + 启动 stdout 读取线程
fn open_session(
    app: &AppHandle,
    session_id: &str,
    cwd: &str,
    model: Option<&str>,
    effort: Option<&str>,
    channel: Option<&str>,
    advisor: bool,
    permission_mode: Option<&str>,
    append_system_prompt: Option<&str>,
    force_new: bool,
) -> Result<(), String> {
    if !std::path::Path::new(cwd).is_dir() {
        return Err(format!("工作目录不存在: {}", cwd));
    }

    // 1. 权限服务（per-session 生命周期：open 时 start，进程退出时 stop）
    let permission_service = PermissionService::start(app.clone(), session_id)
        .map_err(|e| format!("权限服务启动失败：{}", e))?;
    let socket_path = permission_service.socket_path().to_path_buf();

    // 2. MCP 二进制
    let mcp_binary = match find_mcp_binary() {
        Some(p) => p,
        None => {
            PermissionService::stop_for(session_id);
            return Err("未找到 cc-space-mcp 二进制，无法启动权限服务".to_string());
        }
    };

    // 3. MCP 配置
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

    // 4. CLI 参数（不带 --print，不传消息文本；加 --input-format stream-json）
    let (executable, prefix_args) = find_claude();
    let mut args = prefix_args;
    let session_file = crate::commands::projects_dir()
        .join(cwd.replace('/', "-"))
        .join(format!("{}.jsonl", session_id));
    let session_flag = if session_file.is_file() {
        "--resume"
    } else if !force_new {
        // jsonl 不在预期位置——全局探测：是迁走了还是真的新会话？
        if let Some(actual) = find_session_elsewhere(session_id, &session_file) {
            return Err(format!(
                "SESSION_ELSEWHERE:{}",
                actual.to_string_lossy()
            ));
        }
        "--session-id"
    } else {
        "--session-id"
    };
    args.extend([
        session_flag.to_string(),
        session_id.to_string(),
        "--output-format".to_string(),
        "stream-json".to_string(),
        "--input-format".to_string(),
        "stream-json".to_string(),
        "--verbose".to_string(),
        "--thinking-display".to_string(),
        "summarized".to_string(),
        "--mcp-config".to_string(),
        mcp_config,
        "--permission-prompt-tool".to_string(),
        "mcp__cc-space__approve_tool_use".to_string(),
    ]);

    let ultracode = effort == Some("ultracode");

    // 5. 渠道/ultracode/顾问注入（合成 --settings）
    let channel_opt = channel.filter(|s| !s.is_empty() && *s != crate::channels::OFFICIAL_ID);
    let channel_injection =
        match crate::channels::prepare_injection(channel_opt, session_id, ultracode, advisor) {
            Ok(Some(inj)) => {
                args.push("--settings".to_string());
                args.push(inj.settings_arg.clone());
                Some(inj)
            }
            Ok(None) => None,
            Err(e) => {
                PermissionService::stop_for(session_id);
                return Err(format!("会话配置加载失败:{}", e));
            }
        };

    if let Some(m) = model.filter(|s| !s.is_empty()) {
        args.push("--model".to_string());
        args.push(m.to_string());
    }
    if let Some(e) = effort.filter(|s| !s.is_empty() && *s != "ultracode") {
        args.push("--effort".to_string());
        args.push(e.to_string());
    }
    let effective_mode = permission_mode
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .or_else(|| resolve_default_permission_mode(cwd));
    if let Some(ref mode) = effective_mode {
        args.push("--permission-mode".to_string());
        args.push(mode.clone());
    }
    if let Some(prompt) = append_system_prompt.filter(|s| !s.is_empty()) {
        args.push("--append-system-prompt".to_string());
        args.push(prompt.to_string());
    }

    // 6. Spawn（stdin/stdout/stderr 全 piped）
    let mut command = Command::new(&executable);
    command
        .args(&args)
        .current_dir(cwd)
        .env("PATH", enhanced_path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(inj) = &channel_injection {
        for key in &inj.clear_env {
            command.env_remove(key);
        }
        for (k, v) in &inj.env {
            command.env(k, v);
        }
    }
    let mut child = command.spawn().map_err(|e| {
        if let Some(inj) = &channel_injection {
            crate::channels::cleanup_runtime_file(&inj.runtime_path);
        }
        PermissionService::stop_for(session_id);
        format!("启动 claude 失败: {}", e)
    })?;

    // 7. Take handles
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "无法获取 stdin".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "无法获取 stdout".to_string())?;
    let stderr = child.stderr.take();

    // 8. 初始化握手（没有这个握手，进程不处理任何消息）
    let init_msg = json!({
        "type": "control_request",
        "request_id": "init-1",
        "request": {"subtype": "initialize"}
    });
    write_stdin(&mut stdin, &init_msg).map_err(|e| {
        PermissionService::stop_for(session_id);
        if let Some(inj) = &channel_injection {
            crate::channels::cleanup_runtime_file(&inj.runtime_path);
        }
        format!("初始化握手写入失败: {}", e)
    })?;

    let mut reader = BufReader::new(stdout);
    let mut line_buf = String::new();
    loop {
        line_buf.clear();
        let bytes_read = reader.read_line(&mut line_buf).map_err(|e| {
            PermissionService::stop_for(session_id);
            if let Some(inj) = &channel_injection {
                crate::channels::cleanup_runtime_file(&inj.runtime_path);
            }
            format!("初始化握手读取失败: {}", e)
        })?;
        if bytes_read == 0 {
            PermissionService::stop_for(session_id);
            let stderr_msg = stderr
                .map(|s| {
                    let mut r = BufReader::new(s);
                    let mut buf = String::new();
                    let _ = std::io::Read::read_to_string(&mut r, &mut buf);
                    buf
                })
                .unwrap_or_default();
            if let Some(inj) = &channel_injection {
                crate::channels::cleanup_runtime_file(&inj.runtime_path);
            }
            return Err(format!(
                "进程在初始化阶段退出{}",
                if stderr_msg.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", stderr_msg.trim())
                }
            ));
        }
        if let Ok(v) = serde_json::from_str::<Value>(&line_buf) {
            let t = v.get("type").and_then(|t| t.as_str());
            if t == Some("control_response") {
                break;
            }
            // 握手阶段也可能收到 hook 事件（SessionStart hooks 在 initialize 前执行）
            if t == Some("system") {
                if let Some(sub) = v.get("subtype").and_then(|s| s.as_str()) {
                    if sub == "hook_started" || sub == "hook_response" {
                        let mut payload = v;
                        payload.as_object_mut().map(|o| o.insert("session_id".to_string(), json!(session_id)));
                        let _ = app.emit("session-hook", &payload);
                    }
                }
            }
        }
    }

    // 9. 默认开启 Remote Control（进程活着就有 RC，关进程自动断）
    let rc_msg = json!({
        "type": "control_request",
        "request_id": "rc-init",
        "request": {"subtype": "remote_control", "enabled": true}
    });
    let _ = write_stdin(&mut stdin, &rc_msg);

    // 10. 存入 ACTIVE_PROCESSES
    let pid = child.id();
    eprintln!("[long-lived] 新建进程 PID={} 会话={}", pid, &session_id[..session_id.len().min(8)]);
    let resumed = session_flag == "--resume";
    let _ = app.emit("session-connected", json!({
        "session_id": session_id,
        "resumed": resumed,
        "cwd": cwd,
    }));
    let sp_arc = Arc::new(Mutex::new(SessionProcess {
        child,
        stdin,
        request_counter: 1,
        channel: channel.map(|s| s.to_string()),
        effort: effort.map(|s| s.to_string()),
        model: model.filter(|s| !s.is_empty()).map(|s| s.to_string()),
        advisor,
    }));
    ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(session_id.to_string(), sp_arc.clone());

    // 10a. 启动 stdout 读取线程（reader 已消费掉握手响应，后续全是流式事件）
    let handle = app.clone();
    let sid = session_id.to_string();
    let sock = socket_path;
    let runtime_file = channel_injection.map(|inj| inj.runtime_path);
    std::thread::spawn(move || {
        read_stream(reader, stderr, sp_arc.clone(), &handle, &sid);
        PermissionService::stop_if_socket(&sid, &sock);
        if let Some(p) = runtime_file {
            crate::channels::cleanup_runtime_file(&p);
        }
    });

    Ok(())
}

/// 发送消息（自动 open 会话进程；替代旧 start_streaming）
pub fn send_message(
    app: &AppHandle,
    session_id: &str,
    cwd: &str,
    message: &str,
    model: Option<&str>,
    effort: Option<&str>,
    channel: Option<&str>,
    advisor: bool,
    images: Option<&[serde_json::Value]>,
    permission_mode: Option<&str>,
    append_system_prompt: Option<&str>,
    force_new: bool,
) -> Result<(), String> {
    let mut exists = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .map_or(false, |m| m.contains_key(session_id));

    if exists {
        let needs_restart = ACTIVE_PROCESSES
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|m| m.get(session_id).cloned())
            .map_or(false, |arc| {
                let sp = arc.lock().unwrap();
                sp.channel.as_deref() != channel
                    || sp.effort.as_deref() != effort
                    // advisor 经 --settings 注入,变更只能重启生效(否则只有主模型锁定生效、顾问没挂上)
                    || sp.advisor != advisor
                    // 模型意图回落默认(None)而进程钉着上次 set_model 的具体值:
                    // set_model 无法"切回默认",不重启就是旧模型粘滞(界面默认、实跑旧值)
                    || (model.is_none() && sp.model.is_some())
            });
        if needs_restart {
            eprintln!("[long-lived] 渠道/effort/advisor/模型回落变更，重启进程 会话={}", &session_id[..session_id.len().min(8)]);
            close_session(session_id);
            exists = false;
        }
    }

    if !exists {
        open_session(app, session_id, cwd, model, effort, channel, advisor, permission_mode, append_system_prompt, force_new)?;
    }

    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned())
        .ok_or("会话进程不存在")?;

    let mut sp = process.lock().unwrap();
    if exists {
        eprintln!("[long-lived] 复用进程 PID={} 会话={}", sp.child.id(), &session_id[..session_id.len().min(8)]);
        if let Some(m) = model.filter(|s| !s.is_empty()) {
            sp.request_counter += 1;
            let req_id = format!("set-model-{}", sp.request_counter);
            let ctrl = json!({
                "type": "control_request",
                "request_id": req_id,
                "request": {"subtype": "set_model", "model": m}
            });
            write_stdin(&mut sp.stdin, &ctrl)?;
            sp.model = Some(m.to_string());
        }
    }

    if let Some(mode) = permission_mode.filter(|m| !m.is_empty()) {
        sp.request_counter += 1;
        let req_id = format!("set-perm-mode-{}", sp.request_counter);
        let ctrl = json!({
            "type": "control_request",
            "request_id": req_id,
            "request": {"subtype": "set_permission_mode", "permission_mode": mode}
        });
        let _ = write_stdin(&mut sp.stdin, &ctrl);
    }
    let mut content: Vec<serde_json::Value> = Vec::new();
    if let Some(imgs) = images {
        content.extend_from_slice(imgs);
    }
    content.push(json!({"type": "text", "text": message}));
    let msg = json!({
        "type": "user",
        "session_id": "",
        "message": {
            "role": "user",
            "content": content
        },
        "parent_tool_use_id": null
    });
    write_stdin(&mut sp.stdin, &msg)
}

/// 开关 Remote Control（通过 control_request 动态启用/禁用；进程未启动时自动连接）
pub fn toggle_remote_control(
    app: &AppHandle,
    session_id: &str,
    cwd: &str,
    model: Option<&str>,
    effort: Option<&str>,
    channel: Option<&str>,
    advisor: bool,
    enabled: bool,
    permission_mode: Option<&str>,
) -> Result<(), String> {
    let exists = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .map_or(false, |m| m.contains_key(session_id));

    if !exists {
        open_session(app, session_id, cwd, model, effort, channel, advisor, permission_mode, None, false)?;
    }

    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned())
        .ok_or("会话进程不存在")?;

    let mut sp = process.lock().unwrap();
    eprintln!("[long-lived] Remote Control enabled={} PID={} 会话={}", enabled, sp.child.id(), &session_id[..session_id.len().min(8)]);
    sp.request_counter += 1;
    // 语义编进 request_id：CLI 的 response 不含 enabled 值，read_stream 只能靠它还原请求意图
    let req_id = format!("rc-{}-{}", if enabled { "on" } else { "off" }, sp.request_counter);
    let msg = json!({
        "type": "control_request",
        "request_id": req_id,
        "request": {"subtype": "remote_control", "enabled": enabled}
    });
    write_stdin(&mut sp.stdin, &msg)
}

/// 中断当前回复（发 interrupt 控制消息，不杀进程）
pub fn interrupt_session(session_id: &str) -> Result<(), String> {
    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned());

    match process {
        Some(p) => {
            let mut sp = p.lock().unwrap();
            eprintln!("[long-lived] 中断进程 PID={} 会话={}", sp.child.id(), &session_id[..session_id.len().min(8)]);
            sp.request_counter += 1;
            let req_id = format!("interrupt-{}", sp.request_counter);
            let msg = json!({
                "type": "control_request",
                "request_id": req_id,
                "request": {"subtype": "interrupt"}
            });
            write_stdin(&mut sp.stdin, &msg)
        }
        None => Ok(()),
    }
}

/// 运行时切换权限模式
pub fn set_permission_mode(session_id: &str, mode: &str) -> Result<(), String> {
    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(session_id).cloned());

    match process {
        Some(p) => {
            let mut sp = p.lock().unwrap();
            sp.request_counter += 1;
            let req_id = format!("set-perm-mode-{}", sp.request_counter);
            let msg = json!({
                "type": "control_request",
                "request_id": req_id,
                "request": {"subtype": "set_permission_mode", "permission_mode": mode}
            });
            write_stdin(&mut sp.stdin, &msg)
        }
        None => Err("会话进程不存在".to_string()),
    }
}

/// 关闭会话进程（SIGTERM → 5s → SIGKILL）
pub fn close_session(session_id: &str) {
    let process_arc = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|m| m.remove(session_id));

    if let Some(process_arc) = process_arc {
        let pid = {
            let mut sp = process_arc.lock().unwrap();
            graceful_shutdown(&mut sp);
            sp.child.id()
        };
        sigterm_pid(pid);

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            if let Ok(mut sp) = process_arc.lock() {
                if sp.child.try_wait().ok().flatten().is_none() {
                    let _ = sp.child.kill();
                }
            }
        });
    }

    PermissionService::stop_for(session_id);
}

/// 关闭所有活跃会话进程（应用退出时调用）
pub fn close_all_sessions() {
    let processes: Vec<(String, Arc<Mutex<SessionProcess>>)> = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_mut()
        .map(|m| m.drain().collect())
        .unwrap_or_default();

    for (sid, process_arc) in processes {
        if let Ok(mut sp) = process_arc.lock() {
            graceful_shutdown(&mut sp);
            sigterm_pid(sp.child.id());
        }
        PermissionService::stop_for(&sid);
    }
}

fn graceful_shutdown(sp: &mut SessionProcess) {
    sp.request_counter += 1;
    let req_id = format!("shutdown-{}", sp.request_counter);
    let _ = write_stdin(&mut sp.stdin, &json!({
        "type": "control_request",
        "request_id": req_id,
        "request": {"subtype": "remote_control", "enabled": false}
    }));
}

fn emit_error(app: &AppHandle, session_id: &str, message: String) {
    let _ = app.emit(
        "stream-event",
        StreamEvent::Error {
            session_id: session_id.to_string(),
            message,
        },
    );
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

/// 读取长活进程 stdout——逐行解析流式事件，result 立即 emit + stream-done 标记轮次结束，
/// 循环仅在 stdout EOF（进程退出）时终止
fn read_stream(
    reader: BufReader<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    process: Arc<Mutex<SessionProcess>>,
    app: &AppHandle,
    session_id: &str,
) {
    // (message_id, model):message_start 写入,后续 content_block_* 读取;model 是本轮真值
    let mut current_message_id: Option<(String, Option<String>)> = None;

    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };

        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let raw_type = value
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or("");

        if let Some(event) = decode_stream_event(&value, session_id, &mut current_message_id) {
            let _ = app.emit("stream-event", &event);
        }

        // hook 事件转发给前端（system 类型的 hook_started / hook_response）
        if raw_type == "system" {
            if let Some(subtype) = value.get("subtype").and_then(|s| s.as_str()) {
                if subtype == "hook_started" || subtype == "hook_response" {
                    let mut payload = value.clone();
                    payload.as_object_mut().map(|o| o.insert("session_id".to_string(), json!(session_id)));
                    let _ = app.emit("session-hook", &payload);
                }
            }
        }

        // Remote Control 判决回报：success/error 都上报，前端 rcActive 完全由本事件驱动
        // （不能乐观置位——判决与 invoke 返回并发，先到的判决会被晚写的乐观值覆盖）。
        // 请求意图编码在 request_id：rc-init（连接自动开）/rc-on-N/rc-off-N（手动）。
        // 第三方渠道实测 CLI 回 error "Remote Control initialization failed"
        if raw_type == "control_response" {
            if let Some(resp) = value.get("response") {
                let req_id = resp.get("request_id").and_then(|v| v.as_str()).unwrap_or("");
                if req_id.starts_with("rc-") {
                    let is_err = resp.get("subtype").and_then(|v| v.as_str()) == Some("error");
                    let wanted_on = !req_id.starts_with("rc-off");
                    // 成功 = 达成所愿；失败 = 维持原状（开失败→仍关，关失败→仍开）
                    let active = if is_err { !wanted_on } else { wanted_on };
                    let err = resp.get("error").and_then(|v| v.as_str());
                    eprintln!(
                        "[long-lived] Remote Control 判决 req={} active={} 会话={}{}",
                        req_id, active, &session_id[..session_id.len().min(8)],
                        err.map(|e| format!(" err={}", e)).unwrap_or_default()
                    );
                    let _ = app.emit("rc-status", json!({
                        "session_id": session_id,
                        "active": active,
                        // 手动开关的判决才触发 toast；连接时自动开启的失败静默（防第三方渠道每次连接被骚扰）
                        "manual": req_id != "rc-init",
                        "error": if is_err { json!(err.unwrap_or("Remote Control unavailable")) } else { Value::Null },
                    }));
                }
            }
        }

        // "result" 标记一轮结束（进程继续活着等下一条 stdin 消息）
        if raw_type == "result" {
            let _ = app.emit("stream-done", json!({ "session_id": session_id }));
        }
    }

    // stdout EOF —— 进程已退出

    // 判断是否属于非预期退出（close_session 会先从 map 移除再 SIGTERM，
    // 所以 map 中仍存在 = 非预期退出，需要上报错误）
    let was_unexpected = {
        let mut map = ACTIVE_PROCESSES.lock().unwrap();
        if let Some(m) = map.as_mut() {
            if m.get(session_id).is_some_and(|c| Arc::ptr_eq(c, &process)) {
                m.remove(session_id);
                true
            } else {
                false
            }
        } else {
            false
        }
    };

    if was_unexpected {
        // 等待并检查退出状态
        let exit_status = {
            let mut sp = process.lock().unwrap();
            sp.child.wait().ok()
        };

        if let Some(status) = exit_status {
            if !status.success() {
                let stderr_text = stderr.map(|stderr| {
                    let mut reader = BufReader::new(stderr);
                    let mut buf = String::new();
                    let _ = std::io::Read::read_to_string(&mut reader, &mut buf);
                    buf
                });
                let err_msg = stderr_text
                    .filter(|t| !t.trim().is_empty())
                    .map(|t| t.trim().to_string())
                    .unwrap_or_else(|| format!("进程异常退出 (code: {})", status));
                emit_error(app, session_id, err_msg);
            } else {
                // 正常退出但未预期（CLI 自行决定退出），通知前端收尾
                let _ = app.emit("stream-done", json!({ "session_id": session_id }));
            }
        } else {
            emit_error(app, session_id, "进程异常退出".to_string());
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
    current_message_id: &mut Option<(String, Option<String>)>,
) -> Option<StreamEvent> {
    let event_type = value.get("type")?.as_str()?;
    let sid = session_id.to_string();

    // 子 agent 的转发事件：顶层 parent_tool_use_id 非 null（实测 v2.1.x 子 agent 的
    // assistant 快照带此标记，主对话恒 null）。不进主对话流——子 agent 内容由
    // 异步面板从 subagents 转录呈现，混入主流会把子 agent 输出渲染进主会话区
    if value
        .get("parent_tool_use_id")
        .map_or(false, |v| !v.is_null())
    {
        return None;
    }

    match event_type {
        "stream_event" => {
            // CLI envelope: { type: "stream_event", event: <Anthropic SSE event> }
            let inner = value.get("event")?;
            let inner_type = inner.get("type")?.as_str()?;
            match inner_type {
                "message_start" => {
                    // 仅更新跨行状态,不 emit:让 content_block_start 自带建 turn 能力。
                    // 顺带提取 message.model——本轮实际运行模型的最早真值来源
                    let msg = inner.get("message")?;
                    let id = msg.get("id")?.as_str()?.to_string();
                    let model = msg.get("model").and_then(|v| v.as_str()).map(String::from);
                    *current_message_id = Some((id, model));
                    None
                }
                "content_block_start" => {
                    let (mid, model) = current_message_id.as_ref()?.clone();
                    let index = inner.get("index")?.as_u64()? as usize;
                    let cb_value = inner.get("content_block")?.clone();
                    let content_block: ContentBlock =
                        serde_json::from_value(cb_value).ok()?;
                    Some(StreamEvent::BlockStart {
                        session_id: sid,
                        message_id: mid,
                        index,
                        content_block,
                        model,
                    })
                }
                "content_block_delta" => {
                    let mid = current_message_id.as_ref()?.0.clone();
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
                    let mid = current_message_id.as_ref()?.0.clone();
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
            let model = msg.get("model").and_then(|v| v.as_str()).map(String::from);
            Some(StreamEvent::AssistantMessage {
                session_id: sid,
                message_id,
                content,
                model,
            })
        }
        // "progress"（老版 CLI 的子任务进度转发容器）不再混入主流：
        // 子 agent 内容归异步面板，主对话只渲染自己的消息
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

            let is_interrupt = result_text.is_empty()
                || result_text.contains("interrupted by user");
            if is_error && !is_interrupt {
                Some(StreamEvent::Error {
                    session_id: sid,
                    message: result_text,
                })
            } else {
                let cost = value
                    .get("total_cost_usd")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);
                let usage = value.get("modelUsage");
                Some(StreamEvent::Result {
                    session_id: sid,
                    text: result_text,
                    cost_usd: cost,
                    context_window: usage.and_then(|u| u.get("contextWindow")).and_then(|v| v.as_u64()),
                    input_tokens: usage.and_then(|u| u.get("inputTokens")).and_then(|v| v.as_u64()),
                    output_tokens: usage.and_then(|u| u.get("outputTokens")).and_then(|v| v.as_u64()),
                })
            }
        }
        _ => None,
    }
}
