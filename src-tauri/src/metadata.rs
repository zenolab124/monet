use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::commands::projects_dir;
use crate::models::{SessionRecord, MessageContent};
use crate::parser;

static STORE: Mutex<Option<HashMap<String, SessionMeta>>> = Mutex::new(None);

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deleted_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starred: Option<bool>,
}

fn meta_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("cc-space")
        .join("metadata.json")
}

fn load() -> HashMap<String, SessionMeta> {
    let path = meta_path();
    if !path.exists() {
        return HashMap::new();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(data: &HashMap<String, SessionMeta>) {
    let path = meta_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(data) {
        let _ = fs::write(&path, json);
    }
}

fn with_store<F, R>(f: F) -> R
where
    F: FnOnce(&mut HashMap<String, SessionMeta>) -> R,
{
    let mut guard = STORE.lock().unwrap();
    let store = guard.get_or_insert_with(load);
    f(store)
}

#[tauri::command]
pub fn get_all_meta() -> HashMap<String, SessionMeta> {
    with_store(|s| s.clone())
}

#[tauri::command]
pub fn update_meta(session_id: String, patch: SessionMeta) -> Result<SessionMeta, String> {
    with_store(|store| {
        let entry = store.entry(session_id).or_default();
        if let Some(v) = patch.title {
            entry.title = Some(v);
        }
        if let Some(v) = patch.deleted {
            entry.deleted = Some(v);
        }
        if let Some(v) = patch.deleted_at {
            entry.deleted_at = Some(v);
        }
        if let Some(v) = patch.tags {
            entry.tags = Some(v);
        }
        if let Some(v) = patch.starred {
            entry.starred = Some(v);
        }
        let result = entry.clone();
        save(store);
        Ok(result)
    })
}

pub fn agent_cwd() -> PathBuf {
    let p = dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("cc-space-agent");
    let _ = fs::create_dir_all(&p);
    p
}

fn extract_conversation_snippet(project_id: &str, session_id: &str) -> Option<String> {
    let path = projects_dir()
        .join(project_id)
        .join(format!("{}.jsonl", session_id));
    let records = parser::parse_messages(&path);

    let mut lines = Vec::new();
    for r in &records {
        if let SessionRecord::User(u) = r {
            if let Some(msg) = &u.message {
                let text = match &msg.content {
                    MessageContent::Text(s) => s.clone(),
                    MessageContent::Blocks(blocks) => {
                        use crate::models::ContentBlock;
                        blocks
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                    }
                };
                if !text.is_empty() {
                    let truncated: String = text.chars().take(200).collect();
                    lines.push(truncated);
                }
            }
        }
        if lines.len() >= 5 {
            break;
        }
    }

    if lines.is_empty() {
        return None;
    }
    Some(lines.join("\n"))
}

#[tauri::command]
pub async fn generate_title(
    project_id: String,
    session_id: String,
) -> Result<String, String> {
    let sid = session_id.clone();
    let pid = project_id.clone();

    let title = tauri::async_runtime::spawn_blocking(move || {
        let snippet = extract_conversation_snippet(&pid, &sid)
            .ok_or_else(|| "会话无内容".to_string())?;
        crate::agent::generate_title(&snippet)
    })
    .await
    .map_err(|e| e.to_string())??;

    with_store(|store| {
        let entry = store.entry(session_id).or_default();
        entry.title = Some(title.clone());
        save(store);
    });

    Ok(title)
}

#[tauri::command]
pub async fn generate_permission_hint(
    tool_name: String,
    input_json: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::agent::permission_hint(&tool_name, &input_json)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn parse_natural_schedule(text: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::agent::parse_cron(&text)
    })
    .await
    .map_err(|e| e.to_string())?
}
