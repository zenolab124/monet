use std::fs;
use std::path::PathBuf;

use crate::discovery;
use crate::models::*;
use crate::parser;
use crate::permission::PermissionService;
use crate::probe;
use crate::streaming;
use crate::usage_stats;

/// 获取所有项目（含会话摘要）
#[tauri::command]
pub fn get_projects() -> Vec<Project> {
    discovery::discover_all()
}

/// 获取会话消息记录（仅 user/assistant，跳过 snapshot 等大型记录）
#[tauri::command]
pub fn get_session_records(project_id: String, session_id: String) -> Vec<SessionRecord> {
    let path = session_path(&project_id, &session_id);
    let t0 = std::time::Instant::now();
    let records = parser::parse_messages(&path);
    if cfg!(debug_assertions) {
        // dev 埋点:与前端 [perf] 会话加载报告的 invoke 段对照,差值即 IPC 序列化成本
        eprintln!(
            "[perf] parse_messages {}: {} records · {:.1}ms",
            &session_id[..session_id.len().min(8)],
            records.len(),
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }
    records
}

/// 获取单个会话的摘要信息
#[tauri::command]
pub fn get_session_summary(project_id: String, session_id: String) -> Option<SessionSummary> {
    let path = session_path(&project_id, &session_id);
    parser::parse_summary(&path, 50)
}

/// 删除会话（.jsonl 文件 + 子会话目录）
#[tauri::command]
pub fn delete_session(project_id: String, session_id: String) -> Result<(), String> {
    let jsonl = session_path(&project_id, &session_id);
    if jsonl.exists() {
        fs::remove_file(&jsonl).map_err(|e| e.to_string())?;
    }
    // 删除可能存在的子会话目录
    let subdir = projects_dir().join(&project_id).join(&session_id);
    if subdir.is_dir() {
        fs::remove_dir_all(&subdir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 在终端中恢复会话。channel 非空时经 `--settings <渠道文件>` 带上会话渠道——
/// 终端用渠道原文件(非 runtime 合成):与「终端可直接复用渠道文件」的设计一致,
/// 终端环境的变量残留属用户自己的 shell 管辖,不做防御注入
#[tauri::command]
pub fn resume_in_terminal(
    cwd: String,
    session_id: String,
    channel: Option<String>,
) -> Result<(), String> {
    let settings_part = match channel
        .as_deref()
        .filter(|c| !c.is_empty() && *c != crate::channels::OFFICIAL_ID)
    {
        Some(ch) => {
            crate::channels::validate_id(ch)?;
            let path = crate::channels::channel_file_path(ch);
            if !path.is_file() {
                return Err(format!("渠道配置不存在: {}", ch));
            }
            format!(" --settings \\\"{}\\\"", path.display())
        }
        None => String::new(),
    };
    let escaped_cwd = cwd.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "cd \"{}\" && claude{} --resume {}"
        end tell"#,
        escaped_cwd, settings_part, session_id
    );
    std::process::Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 在 VSCode 中打开项目目录
#[tauri::command]
pub fn resume_in_vscode(cwd: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(format!("vscode://file{}", cwd))
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// 发送消息（长活进程：自动 open + stdin 写入；替代旧 per-message spawn）。
/// async + spawn_blocking：open_session 的初始化握手是阻塞 I/O，不能卡 IPC 主线程
#[tauri::command]
pub async fn start_streaming(
    app: tauri::AppHandle,
    session_id: String,
    cwd: String,
    message: String,
    model: Option<String>,
    effort: Option<String>,
    channel: Option<String>,
    advisor: bool,
    images: Option<Vec<serde_json::Value>>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        streaming::send_message(
            &app,
            &session_id,
            &cwd,
            &message,
            model.as_deref(),
            effort.as_deref(),
            channel.as_deref(),
            advisor,
            images.as_deref(),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 中断当前回复（发 interrupt 控制消息，不杀进程）
#[tauri::command]
pub fn stop_streaming(session_id: String) -> Result<(), String> {
    streaming::interrupt_session(&session_id)
}

/// 运行时切换权限模式
#[tauri::command]
pub fn set_permission_mode(session_id: String, mode: String) -> Result<(), String> {
    streaming::set_permission_mode(&session_id, &mode)
}

/// 开关 Remote Control（进程未启动时自动连接）
#[tauri::command]
pub async fn toggle_remote_control(
    app: tauri::AppHandle,
    session_id: String,
    cwd: String,
    model: Option<String>,
    effort: Option<String>,
    channel: Option<String>,
    advisor: bool,
    enabled: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        streaming::toggle_remote_control(
            &app,
            &session_id,
            &cwd,
            model.as_deref(),
            effort.as_deref(),
            channel.as_deref(),
            advisor,
            enabled,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 关闭会话进程（SIGTERM → 5s → SIGKILL）
#[tauri::command]
pub fn close_session(session_id: String) {
    streaming::close_session(&session_id);
}

/// 前端响应权限请求
///
/// `allow=true` 时透传给 claude CLI `{"behavior":"allow","updatedInput"?:...}`，
/// updated_input 用于交互工具（AskUserQuestion 答案注入等），缺省由 mcp 回填原 input
/// `allow=false` 时返回 `{"behavior":"deny","message":...}`，message 缺省为「用户拒绝」
#[tauri::command]
pub fn respond_permission(
    request_id: String,
    allow: bool,
    message: Option<String>,
    updated_input: Option<serde_json::Value>,
) -> Result<(), String> {
    let ok = PermissionService::respond(&request_id, allow, message, updated_input);
    if ok {
        Ok(())
    } else {
        Err(format!(
            "未找到 pending 权限请求，可能已超时或已被处理：{}",
            request_id
        ))
    }
}

/// CLI 全局配置摘要(~/.claude/settings.json):顶栏「默认」项展示真值用
#[derive(serde::Serialize)]
pub struct CliSettings {
    pub model: Option<String>,
    pub effort_level: Option<String>,
    pub ultracode: bool,
}

/// 读取 CLI settings.json 的模型/努力默认值。
/// settings.json 是活文件(CLI 内 /effort 等实时改写),每次调用现读现解析、
/// 绝不进程级缓存,见 docs/knowledge/pitfalls/cli-settings-live-rewrite.md
#[tauri::command]
pub fn get_cli_settings() -> CliSettings {
    let path = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("settings.json");
    let json: Option<serde_json::Value> = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let get_str = |key: &str| {
        json.as_ref()
            .and_then(|j| j.get(key))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    CliSettings {
        model: get_str("model"),
        effort_level: get_str("effortLevel"),
        ultracode: json
            .as_ref()
            .and_then(|j| j.get("ultracode"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    }
}

/// 检测某会话是否有**外部** claude CLI 进程在运行。
/// 排除 CC Space 自身持有的长活进程 PID，只报告终端 `claude --resume` / VS Code 等外部进程。
/// 交互式 REPL(命令行不带 session-id)检测不到,属已知边界。
/// Windows 无 ps,Command 失败时返回 false 优雅降级。
#[tauri::command]
pub fn check_session_running(session_id: String) -> bool {
    let own_pid = crate::streaming::get_own_pid(&session_id);

    let Ok(output) = std::process::Command::new("ps")
        .args(["-axo", "pid,command"])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|l| {
            if !l.contains(&session_id) || !l.contains("claude") {
                return false;
            }
            if let Some(own) = own_pid {
                let pid = l.trim().split_whitespace().next().and_then(|s| s.parse::<u32>().ok());
                if pid == Some(own) {
                    return false;
                }
            }
            true
        })
}

/// 全项目 token 用量聚合（v2.2.0 FR-001）：首页 Token 卡 / 活跃热力图数据源。
/// 全量扫描秒级耗时，丢 blocking 线程池跑，不占 IPC 主线程
#[tauri::command]
pub async fn get_usage_stats() -> Result<usage_stats::UsageStats, String> {
    tauri::async_runtime::spawn_blocking(usage_stats::collect_usage_stats)
        .await
        .map_err(|e| e.to_string())?
}

/// schema-probe 全量扫描（v2.2.0 FR-004）：首页兼容性诊断卡数据源。
/// 返回结构与 CLI `--json` 输出同构（既有契约）
#[tauri::command]
pub async fn get_schema_diagnosis() -> Result<probe::Report, String> {
    tauri::async_runtime::spawn_blocking(|| probe::run_probe(None))
        .await
        .map_err(|e| e.to_string())?
}

/// 用系统默认应用打开文件(macOS: open / Windows: cmd start / Linux: xdg-open)
#[tauri::command]
pub fn open_in_default_app(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", &path])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn session_path(project_id: &str, session_id: &str) -> PathBuf {
    projects_dir()
        .join(project_id)
        .join(format!("{}.jsonl", session_id))
}

pub(crate) fn projects_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("projects")
}
