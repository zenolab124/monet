use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::commands::projects_dir;
use crate::config;
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_manual: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

fn meta_path() -> PathBuf {
    config::data_dir().join("metadata.json")
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
        if let Some(v) = patch.title_manual {
            entry.title_manual = Some(v);
        }
        if let Some(v) = patch.summary {
            entry.summary = Some(v);
        }
        let result = entry.clone();
        save(store);
        Ok(result)
    })
}

pub fn agent_cwd() -> PathBuf {
    let p = config::data_dir().join("agent");
    let _ = fs::create_dir_all(&p);
    p
}

fn extract_conversation_snippet(project_id: &str, session_id: &str) -> Option<(String, usize)> {
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
                    let truncated: String = text.chars().take(500).collect();
                    lines.push(truncated);
                }
            }
        }
    }

    if lines.is_empty() {
        return None;
    }
    let count = lines.len();
    Some((lines.join("\n"), count))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TitleResult {
    pub title: String,
    pub turn_count: usize,
}

#[tauri::command]
pub async fn generate_title(
    project_id: String,
    session_id: String,
) -> Result<TitleResult, String> {
    if !crate::channels::is_agent_enabled("title") {
        return Err("agent.title 已禁用".to_string());
    }
    let sid = session_id.clone();
    let pid = project_id.clone();
    let current_title = with_store(|store| {
        store.get(&sid).and_then(|m| m.title.clone())
    });

    let (title, turn_count) = tauri::async_runtime::spawn_blocking(move || {
        let (snippet, count) = extract_conversation_snippet(&pid, &sid)
            .ok_or_else(|| "会话无内容".to_string())?;
        let title = crate::agent::generate_title(&snippet, current_title.as_deref())?;
        Ok::<_, String>((title, count))
    })
    .await
    .map_err(|e| e.to_string())??;

    with_store(|store| {
        let entry = store.entry(session_id).or_default();
        entry.title = Some(title.clone());
        save(store);
    });

    Ok(TitleResult { title, turn_count })
}

#[tauri::command]
pub async fn generate_permission_hint(
    tool_name: String,
    input_json: String,
) -> Result<String, String> {
    if !crate::channels::is_agent_enabled("permission_hint") {
        return Err("agent.permission_hint 已禁用".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::agent::permission_hint(&tool_name, &input_json)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn translate_settings_fields(fields_json: String) -> Result<String, String> {
    if !crate::channels::is_agent_enabled("settings_explain") {
        return Err("agent.settings_explain 已禁用".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::agent::translate_settings(&fields_json)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn extract_settings_defaults(fields_json: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        crate::cli_settings::extract_defaults_from_binary(&fields_json)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn generate_tags(
    project_id: String,
    session_id: String,
) -> Result<Vec<String>, String> {
    if !crate::channels::is_agent_enabled("tags") {
        return Err("agent.tags 已禁用".to_string());
    }
    let sid = session_id.clone();
    let pid = project_id.clone();
    let current_tags = with_store(|store| {
        store.get(&sid).and_then(|m| m.tags.clone())
    });

    let tags = tauri::async_runtime::spawn_blocking(move || {
        let (snippet, _) = extract_conversation_snippet(&pid, &sid)
            .ok_or_else(|| "会话无内容".to_string())?;
        let raw = crate::agent::generate_tags(&snippet, current_tags.as_deref())?;
        let tags: Vec<String> = raw.split(['，', ','])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Ok::<_, String>(tags)
    })
    .await
    .map_err(|e| e.to_string())??;

    with_store(|store| {
        let entry = store.entry(session_id).or_default();
        entry.tags = Some(tags.clone());
        save(store);
    });

    Ok(tags)
}

#[tauri::command]
pub async fn generate_summary(
    project_id: String,
    session_id: String,
) -> Result<String, String> {
    if !crate::channels::is_agent_enabled("summary") {
        return Err("agent.summary 已禁用".to_string());
    }
    let sid = session_id.clone();
    let pid = project_id.clone();
    let current_summary = with_store(|store| {
        store.get(&sid).and_then(|m| m.summary.clone())
    });

    let summary = tauri::async_runtime::spawn_blocking(move || {
        let (snippet, _) = extract_conversation_snippet(&pid, &sid)
            .ok_or_else(|| "会话无内容".to_string())?;
        crate::agent::generate_summary(&snippet, current_summary.as_deref())
    })
    .await
    .map_err(|e| e.to_string())??;

    with_store(|store| {
        let entry = store.entry(session_id).or_default();
        entry.summary = Some(summary.clone());
        save(store);
    });

    Ok(summary)
}

#[tauri::command]
pub fn set_agent_locale(locale: String) {
    crate::agent::set_locale(&locale);
}

#[tauri::command]
pub async fn parse_natural_schedule(text: String) -> Result<String, String> {
    if !crate::channels::is_agent_enabled("cron_parse") {
        return Err("agent.cron_parse 已禁用".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::agent::parse_cron(&text)
    })
    .await
    .map_err(|e| e.to_string())?
}
