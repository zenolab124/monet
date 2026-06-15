use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use serde_json::Value;

static SCHEMA_CACHE: Mutex<Option<Value>> = Mutex::new(None);

const SCHEMA_URL: &str = "https://json.schemastore.org/claude-code-settings.json";

fn cc_space_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("cc-space")
}

fn schema_cache_path() -> PathBuf {
    cc_space_dir().join("settings-schema.json")
}

fn claude_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("settings.json")
}

/// 读取 schema：内存缓存 → 磁盘缓存 → 远程 fetch
fn load_schema() -> Option<Value> {
    {
        let guard = SCHEMA_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref s) = *guard {
            return Some(s.clone());
        }
    }

    // 磁盘缓存
    if let Ok(content) = fs::read_to_string(schema_cache_path()) {
        if let Ok(val) = serde_json::from_str::<Value>(&content) {
            let mut guard = SCHEMA_CACHE.lock().unwrap_or_else(|e| e.into_inner());
            *guard = Some(val.clone());
            return Some(val);
        }
    }

    None
}

/// 后台 fetch 远程 schema 并写入磁盘+内存缓存
fn fetch_and_cache_schema() {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build();
    let Ok(client) = client else { return };

    let Ok(resp) = client.get(SCHEMA_URL).send() else {
        return;
    };
    let Ok(text) = resp.text() else { return };

    if let Ok(val) = serde_json::from_str::<Value>(&text) {
        let dir = cc_space_dir();
        let _ = fs::create_dir_all(&dir);
        let _ = fs::write(schema_cache_path(), &text);

        let mut guard = SCHEMA_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(val);
    }
}

/// 返回 schema 的 properties 部分（每个字段的 type/description/enum/default）
/// + 当前 settings.json 的完整值
#[tauri::command]
pub fn get_settings_schema() -> Value {
    let schema = load_schema();

    let properties = schema
        .as_ref()
        .and_then(|s| s.get("properties"))
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::new()));

    let defs = schema
        .as_ref()
        .and_then(|s| s.get("$defs"))
        .cloned()
        .unwrap_or(Value::Object(serde_json::Map::new()));

    serde_json::json!({
        "properties": properties,
        "$defs": defs,
        "hasSchema": schema.is_some(),
    })
}

/// 返回 settings.json 完整内容（原始 JSON）
#[tauri::command]
pub fn get_full_cli_settings() -> Value {
    let path = claude_settings_path();
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or(Value::Object(serde_json::Map::new()))
}

/// 更新 settings.json 中的指定字段（合并写入）
#[tauri::command]
pub fn update_cli_settings(updates: HashMap<String, Value>) -> Result<(), String> {
    let path = claude_settings_path();

    let mut current: serde_json::Map<String, Value> = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    for (key, value) in updates {
        if value.is_null() {
            current.remove(&key);
        } else {
            current.insert(key, value);
        }
    }

    let json_str = serde_json::to_string_pretty(&Value::Object(current))
        .map_err(|e| format!("序列化失败: {}", e))?;

    fs::write(&path, json_str).map_err(|e| format!("写入失败: {}", e))
}

/// 后台刷新 schema（非阻塞）
#[tauri::command]
pub fn refresh_settings_schema() {
    std::thread::spawn(fetch_and_cache_schema);
}
