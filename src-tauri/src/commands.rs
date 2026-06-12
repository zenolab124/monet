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

/// 在终端中恢复会话
#[tauri::command]
pub fn resume_in_terminal(cwd: String, session_id: String) -> Result<(), String> {
    let escaped_cwd = cwd.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "cd \"{}\" && claude --resume {}"
        end tell"#,
        escaped_cwd, session_id
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

/// 开始流式会话
#[tauri::command]
pub fn start_streaming(
    app: tauri::AppHandle,
    session_id: String,
    cwd: String,
    message: String,
    model: Option<String>,
    effort: Option<String>,
) {
    streaming::start_streaming(
        &app,
        &session_id,
        &cwd,
        &message,
        model.as_deref(),
        effort.as_deref(),
    );
}

/// 停止指定会话的流式（v2.1.0 per-session：多会话并行互不影响）
#[tauri::command]
pub fn stop_streaming(session_id: String) {
    streaming::stop_streaming(&session_id);
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

/// 检测某会话是否仍有 claude CLI 进程在运行(进程命令行含该 session-id)。
/// 覆盖两类:本应用 spawn 后随窗口关闭失联的进程、外部终端 `claude --resume <id>`。
/// 交互式 REPL(命令行不带 session-id)检测不到,属已知边界。
/// Windows 无 ps,Command 失败时返回 false 优雅降级。
#[tauri::command]
pub fn check_session_running(session_id: String) -> bool {
    // session_id 为 UUID,误匹配概率可忽略;再限定 claude 关键字防偶然碰撞
    let Ok(output) = std::process::Command::new("ps")
        .args(["-axo", "command"])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|l| l.contains(&session_id) && l.contains("claude"))
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
