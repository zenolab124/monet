use std::fs;
use std::path::PathBuf;

use crate::discovery;
use crate::models::*;
use crate::parser;
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
) {
    streaming::start_streaming(&app, &session_id, &cwd, &message);
}

/// 停止流式会话
#[tauri::command]
pub fn stop_streaming() {
    streaming::stop_streaming();
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
