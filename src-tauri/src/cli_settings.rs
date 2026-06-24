use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use serde_json::Value;

use crate::config;

static SCHEMA_CACHE: Mutex<Option<Value>> = Mutex::new(None);

const SCHEMA_URL: &str = "https://json.schemastore.org/claude-code-settings.json";

fn cc_space_dir() -> PathBuf {
    config::data_dir().to_path_buf()
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

// ---------------------------------------------------------------------------
// MCP Server registration
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn is_codesigned(path: &std::path::Path) -> bool {
    std::process::Command::new("codesign")
        .args(["--verify", "--quiet", path.to_string_lossy().as_ref()])
        .output()
        .map_or(false, |o| o.status.success())
}

#[cfg(not(target_os = "macos"))]
fn is_codesigned(_path: &std::path::Path) -> bool {
    true
}

fn mcp_bin_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "cc-space-mcp.exe"
    } else {
        "cc-space-mcp"
    }
}

fn installed_mcp_path() -> PathBuf {
    config::data_dir().join("bin").join(mcp_bin_name())
}

fn bundled_mcp_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(mcp_bin_name());
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(mcp_bin_name())
}

fn install_mcp_binary() -> Result<PathBuf, String> {
    let source = bundled_mcp_path();
    let target = installed_mcp_path();

    if !source.exists() {
        return Err(format!("MCP binary not found at: {}", source.display()));
    }

    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let needs_install = if target.exists() {
        let src_meta = fs::metadata(&source).map_err(|e| e.to_string())?;
        let dst_meta = fs::metadata(&target).map_err(|e| e.to_string())?;
        src_meta.len() != dst_meta.len() || !is_codesigned(&target)
    } else {
        true
    };

    if needs_install {
        fs::copy(&source, &target).map_err(|e| format!("install failed: {}", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&target, fs::Permissions::from_mode(0o755));
        }
        #[cfg(target_os = "macos")]
        {
            let _ = std::process::Command::new("codesign")
                .args(["--sign", "-", "--force", target.to_string_lossy().as_ref()])
                .output();
        }
    }

    Ok(target)
}

#[tauri::command]
pub fn get_mcp_status() -> serde_json::Value {
    let path = claude_settings_path();
    let settings: serde_json::Map<String, serde_json::Value> = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let registered = settings
        .get("mcpServers")
        .and_then(serde_json::Value::as_object)
        .is_some_and(|servers| servers.contains_key("cc-space"));

    serde_json::json!({ "registered": registered })
}

#[tauri::command]
pub fn register_mcp() -> Result<(), String> {
    let mcp_path = install_mcp_binary()?;
    let settings_path = claude_settings_path();

    let mut settings: serde_json::Map<String, serde_json::Value> =
        fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

    let mcp_servers = settings
        .entry("mcpServers")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .ok_or("mcpServers is not an object")?;

    let mut server_config = serde_json::Map::new();
    server_config.insert(
        "command".to_string(),
        serde_json::Value::String(mcp_path.to_string_lossy().to_string()),
    );
    server_config.insert("args".to_string(), serde_json::json!([]));

    if let Ok(dir) = std::env::var("CC_SPACE_DATA_DIR") {
        let mut env = serde_json::Map::new();
        env.insert(
            "CC_SPACE_DATA_DIR".to_string(),
            serde_json::Value::String(dir),
        );
        server_config.insert("env".to_string(), serde_json::Value::Object(env));
    }

    mcp_servers.insert(
        "cc-space".to_string(),
        serde_json::Value::Object(server_config),
    );

    let json_str = serde_json::to_string_pretty(&serde_json::Value::Object(settings))
        .map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&settings_path, json_str).map_err(|e| format!("写入失败: {}", e))
}

#[tauri::command]
pub fn unregister_mcp() -> Result<(), String> {
    let settings_path = claude_settings_path();

    let mut settings: serde_json::Map<String, serde_json::Value> =
        fs::read_to_string(&settings_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();

    if let Some(servers) = settings
        .get_mut("mcpServers")
        .and_then(serde_json::Value::as_object_mut)
    {
        servers.remove("cc-space");
    }

    let json_str = serde_json::to_string_pretty(&serde_json::Value::Object(settings))
        .map_err(|e| format!("序列化失败: {}", e))?;
    fs::write(&settings_path, json_str).map_err(|e| format!("写入失败: {}", e))
}

/// 定位 claude CLI 的真实二进制路径（解析 symlink）
pub fn find_claude_binary() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    let versions_dir = home.join(".local/share/claude/versions");
    if versions_dir.is_dir() {
        let mut versions: Vec<_> = fs::read_dir(&versions_dir)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().ok().is_some_and(|ft| ft.is_file()))
            .collect();
        versions.sort_by_key(|e| e.file_name());
        if let Some(latest) = versions.last() {
            return Some(latest.path());
        }
    }
    let output = Command::new("sh")
        .args(["-c", "readlink -f $(which claude) 2>/dev/null"])
        .output()
        .ok()?;
    let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !path.is_empty() && PathBuf::from(&path).is_file() {
        return Some(PathBuf::from(path));
    }
    None
}

/// 从 CLI 二进制中提取字段默认值（Python 读二进制 + 正则模式匹配，不经 Agent）
/// 返回 JSON 字符串：[{key, default, confidence}]
pub fn extract_defaults_from_binary(fields_json: &str) -> Result<String, String> {
    let binary = find_claude_binary()
        .ok_or_else(|| "找不到 Claude CLI 二进制".to_string())?;
    eprintln!("[extract_defaults] binary: {}", binary.display());

    let script = r#"
import sys, json, re

binary_path = sys.argv[1]
fields = json.loads(sys.argv[2])

with open(binary_path, 'rb') as f:
    data = f.read()

code_markers = [b'??', b'===', b'!==', b'return', b'void 0']

results = []
for field in fields:
    key = field['key']
    needle = key.encode('utf-8')
    snippets = []
    pos = 0
    while len(snippets) < 5:
        idx = data.find(needle, pos)
        if idx == -1:
            break
        start = max(0, idx - 200)
        end = min(len(data), idx + len(needle) + 200)
        chunk = data[start:end]
        if any(m in chunk for m in code_markers):
            snippets.append(chunk.decode('utf-8', errors='replace'))
        pos = idx + 1

    default = None
    confidence = 'low'
    ek = re.escape(key)

    for ctx in snippets:
        if re.search(ek + r'===!1\)return!1;return!0', ctx):
            default, confidence = True, 'high'; break
        if re.search(ek + r'===!0[^0-9]', ctx):
            default, confidence = False, 'high'; break
        m = re.search(ek + r'\?\?(!0|true)', ctx)
        if m:
            default, confidence = True, 'high'; break
        m = re.search(ek + r'\?\?(!1|false)', ctx)
        if m:
            default, confidence = False, 'high'; break
        m = re.search(ek + r'\?\?"([^"]+)"', ctx)
        if m:
            default, confidence = m.group(1), 'high'; break
        m = re.search(ek + r'\?\?(\d+)', ctx)
        if m:
            default, confidence = int(m.group(1)), 'high'; break
        m = re.search(ek + r'!==void 0\)return[^;]+;return(!0|!1|true|false)', ctx)
        if m:
            val = m.group(1)
            default, confidence = val in ('!0', 'true'), 'high'; break
        if re.search(ek + r'!==!1', ctx):
            default, confidence = True, 'medium'

    results.append({'key': key, 'default': default, 'confidence': confidence})

print(json.dumps(results))
"#;

    let output = Command::new("python3")
        .args(["-c", script, &binary.display().to_string(), fields_json])
        .output()
        .map_err(|e| format!("python3 执行失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("python3 错误: {}", stderr.chars().take(300).collect::<String>()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(stdout)
}
