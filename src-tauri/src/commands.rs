use std::fs;
use std::path::PathBuf;

use crate::discovery;
use crate::models::*;
use crate::parser;
use crate::permission::PermissionService;
use crate::streaming;

/// 获取所有项目（含会话摘要）
#[tauri::command]
pub fn get_projects() -> Vec<Project> {
    discovery::discover_all()
}

/// 获取会话消息记录（仅 user/assistant，跳过 snapshot 等大型记录）
#[tauri::command]
pub fn get_session_records(project_id: String, session_id: String) -> Vec<SessionRecord> {
    let path = session_path(&project_id, &session_id);
    parser::parse_messages(&path)
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

/// 在终端中开启指定项目的新会话（工作台左列「＋ → 新建会话」入口）。
/// 新会话由 CLI 写盘后经文件监控出现在档案馆，再「在工作台打开」。
#[tauri::command]
pub fn new_session_in_terminal(cwd: String) -> Result<(), String> {
    let escaped_cwd = cwd.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!(
        r#"tell application "Terminal"
            activate
            do script "cd \"{}\" && claude"
        end tell"#,
        escaped_cwd
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
/// `allow=true` 时透传给 claude CLI `{"behavior":"allow"}`
/// `allow=false` 时返回 `{"behavior":"deny","message":...}`，message 缺省为「用户拒绝」
#[tauri::command]
pub fn respond_permission(
    request_id: String,
    allow: bool,
    message: Option<String>,
) -> Result<(), String> {
    let ok = PermissionService::respond(&request_id, allow, message);
    if ok {
        Ok(())
    } else {
        Err(format!(
            "未找到 pending 权限请求，可能已超时或已被处理：{}",
            request_id
        ))
    }
}

fn session_path(project_id: &str, session_id: &str) -> PathBuf {
    projects_dir()
        .join(project_id)
        .join(format!("{}.jsonl", session_id))
}

fn projects_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("projects")
}
