//! 多渠道(profile)配置域:`~/.claude/cc-space/`
//!
//! - `settings.json`        应用设置:sessionChain/agentChain + 渠道展示元数据
//! - `channels/<id>.json`   纯净 Claude Code settings 格式(顶层 env 块等),
//!                          终端可直接 `claude --settings <路径>` 复用同一渠道
//! - `runtime/<sid>-<ns>.json` per-spawn 合成产物(渠道内容 + 防御空值 + ultracode),
//!                          进程结束即删,应用启动兜底清空
//!
//! 红线:authToken 等敏感值不回传前端(list 仅给掩码)、不进 argv(经 --settings 文件
//! 路径 + spawn env 注入)。所有读取用时重读,不做进程级缓存(同 settings.json 活文件教训)。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::config;

/// 保留 id:语义为「官方/零注入」。参与链排序,不对应 channels/ 下的文件
pub const OFFICIAL_ID: &str = "official";

/// Apple Foundation Models 虚拟渠道 id
pub const APPLE_FM_ID: &str = "apple-fm";
const APPLE_FM_PORT: u16 = 8179;

/// 注入渠道时无条件压制的认证/路由残留键
pub const DEFENSE_ENV_KEYS: [&str; 4] = [
    "ANTHROPIC_API_KEY",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

fn cc_space_dir() -> PathBuf {
    config::data_dir().to_path_buf()
}

fn channels_dir() -> PathBuf {
    cc_space_dir().join("channels")
}

fn runtime_dir() -> PathBuf {
    cc_space_dir().join("runtime")
}

fn settings_path() -> PathBuf {
    cc_space_dir().join("settings.json")
}

pub fn channel_file_path(id: &str) -> PathBuf {
    channels_dir().join(format!("{}.json", id))
}

pub fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err("渠道 ID 须为 1-64 个字符".to_string());
    }
    if id == OFFICIAL_ID {
        return Err("official 为保留 ID".to_string());
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return Err("渠道 ID 仅允许字母、数字、- 和 _".to_string());
    }
    Ok(())
}

// ---- 应用设置(settings.json) ----

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelMeta {
    pub name: Option<String>,
    pub note: Option<String>,
    pub enabled: Option<bool>,
    pub protocol: Option<String>,
    pub scope: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl ChannelMeta {
    pub fn is_enabled(&self) -> bool {
        self.enabled.unwrap_or(true)
    }
    pub fn protocol(&self) -> &str {
        self.protocol.as_deref().unwrap_or("anthropic")
    }
    pub fn scope(&self) -> &str {
        self.scope.as_deref().unwrap_or("full")
    }
    pub fn is_agent_only(&self) -> bool {
        self.scope() == "agent-only"
    }
}

#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct AgentFeaturePrefs {
    pub preferred_channel: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    #[serde(skip_serializing)]
    pub default_channel_id: Option<String>,
    pub session_chain: Vec<String>,
    pub agent_chain: Vec<String>,
    pub channels: BTreeMap<String, ChannelMeta>,
    pub agent_toggles: BTreeMap<String, bool>,
    pub agent_preferences: BTreeMap<String, AgentFeaturePrefs>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

pub(crate) fn load_app_settings() -> AppSettings {
    let mut settings: AppSettings = fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    // 迁移：旧 defaultChannelId → sessionChain/agentChain
    if settings.session_chain.is_empty() && settings.default_channel_id.is_some() {
        let old_default = settings.default_channel_id.take().unwrap();
        let mut file_ids = scan_channel_ids();

        let mut chain = Vec::new();
        if old_default != OFFICIAL_ID && file_ids.contains(&old_default) {
            chain.push(old_default.clone());
            file_ids.retain(|id| id != &old_default);
        }
        chain.push(OFFICIAL_ID.to_string());
        chain.extend(file_ids);

        settings.session_chain = chain.clone();
        settings.agent_chain = chain;
        let _ = save_app_settings(&settings);
    }

    // 首次使用：给默认链
    if settings.session_chain.is_empty() {
        settings.session_chain = vec![OFFICIAL_ID.to_string()];
    }
    if settings.agent_chain.is_empty() {
        settings.agent_chain = vec![OFFICIAL_ID.to_string()];
    }

    // 修复：去重 + 补齐未入链的文件渠道
    fn dedup(chain: &mut Vec<String>) {
        let mut seen = std::collections::HashSet::new();
        chain.retain(|id| seen.insert(id.clone()));
    }
    dedup(&mut settings.session_chain);
    dedup(&mut settings.agent_chain);

    let file_ids = scan_channel_ids();
    let all_ids = std::iter::once(OFFICIAL_ID.to_string()).chain(file_ids.into_iter());
    for id in all_ids {
        let is_agent_only = settings.channels.get(&id).map_or(false, |m| m.is_agent_only());
        if !is_agent_only && !settings.session_chain.contains(&id) {
            settings.session_chain.push(id.clone());
        }
        if !settings.agent_chain.contains(&id) {
            settings.agent_chain.push(id);
        }
    }

    settings
}

fn scan_channel_ids() -> Vec<String> {
    let mut ids = Vec::new();
    if let Ok(entries) = fs::read_dir(channels_dir()) {
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        for path in files {
            if let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(String::from) {
                if validate_id(&id).is_ok() {
                    ids.push(id);
                }
            }
        }
    }
    ids
}

fn save_app_settings(settings: &AppSettings) -> Result<(), String> {
    fs::create_dir_all(cc_space_dir()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(), text).map_err(|e| e.to_string())
}

fn write_json_0600(path: &Path, value: &Value) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

fn mask_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    if chars.len() >= 12 {
        let head: String = chars[..4].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{}…{}", head, tail)
    } else {
        "•••".to_string()
    }
}

// ---- 链解析(内部 API) ----

/// 从会话链解析第一个可用渠道 ID。
/// override_id 非空时直接使用(per-session 覆盖)。
/// 返回 None 表示走官方态(零注入)。
pub fn resolve_session_channel(override_id: Option<&str>) -> Option<String> {
    if let Some(id) = override_id {
        if id == OFFICIAL_ID { return None; }
        return Some(id.to_string());
    }
    let settings = load_app_settings();
    for id in &settings.session_chain {
        if id == OFFICIAL_ID { return None; }
        let meta = settings.channels.get(id);
        if meta.map_or(true, |m| m.is_enabled()) && channel_file_path(id).is_file() {
            return Some(id.clone());
        }
    }
    None
}

pub struct AgentChannelCredentials {
    pub id: String,
    pub is_official: bool,
    pub base_url: Option<String>,
    pub token: Option<String>,
    pub protocol: String,
}

pub fn resolve_agent_chain() -> Vec<AgentChannelCredentials> {
    let settings = load_app_settings();
    let mut result = Vec::new();
    for id in &settings.agent_chain {
        let meta = settings.channels.get(id);
        if !meta.map_or(true, |m| m.is_enabled()) { continue; }
        if id == OFFICIAL_ID {
            result.push(AgentChannelCredentials {
                id: id.clone(), is_official: true,
                base_url: None, token: None,
                protocol: "anthropic".to_string(),
            });
            continue;
        }
        if id == APPLE_FM_ID {
            result.push(AgentChannelCredentials {
                id: id.clone(), is_official: false,
                base_url: Some(format!("http://localhost:{}", APPLE_FM_PORT)),
                token: Some(String::new()),
                protocol: "openai".to_string(),
            });
            continue;
        }
        let protocol = meta.map_or("anthropic", |m| m.protocol()).to_string();
        if let Some((base_url, token)) = read_channel_credentials(id) {
            result.push(AgentChannelCredentials {
                id: id.clone(), is_official: false,
                base_url: Some(base_url), token: Some(token),
                protocol,
            });
        }
    }
    result
}

pub fn resolve_preferred_channel(channel_id: &str) -> Option<AgentChannelCredentials> {
    let settings = load_app_settings();
    if channel_id == OFFICIAL_ID {
        return Some(AgentChannelCredentials {
            id: OFFICIAL_ID.to_string(), is_official: true,
            base_url: None, token: None,
            protocol: "anthropic".to_string(),
        });
    }
    if channel_id == APPLE_FM_ID {
        let meta = settings.channels.get(APPLE_FM_ID);
        if !meta.map_or(true, |m| m.is_enabled()) { return None; }
        return Some(AgentChannelCredentials {
            id: APPLE_FM_ID.to_string(), is_official: false,
            base_url: Some(format!("http://localhost:{}", APPLE_FM_PORT)),
            token: Some(String::new()),
            protocol: "openai".to_string(),
        });
    }
    let meta = settings.channels.get(channel_id);
    if !meta.map_or(true, |m| m.is_enabled()) { return None; }
    let protocol = meta.map_or("anthropic", |m| m.protocol()).to_string();
    let (base_url, token) = read_channel_credentials(channel_id)?;
    Some(AgentChannelCredentials {
        id: channel_id.to_string(), is_official: false,
        base_url: Some(base_url), token: Some(token),
        protocol,
    })
}

pub fn preferred_for(agent_key: &str) -> Option<String> {
    load_app_settings()
        .agent_preferences
        .get(agent_key)
        .and_then(|p| p.preferred_channel.clone())
}

pub(crate) fn read_channel_credentials(id: &str) -> Option<(String, String)> {
    let text = fs::read_to_string(channel_file_path(id)).ok()?;
    let root: Value = serde_json::from_str(&text).ok()?;
    let env = root.get("env")?.as_object()?;
    // Anthropic keys
    if let Some(base_url) = env.get("ANTHROPIC_BASE_URL").and_then(|v| v.as_str()) {
        let token = env
            .get("ANTHROPIC_AUTH_TOKEN")
            .or_else(|| env.get("ANTHROPIC_API_KEY"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        return Some((base_url.to_string(), token.to_string()));
    }
    // OpenAI keys
    if let Some(base_url) = env.get("OPENAI_BASE_URL").and_then(|v| v.as_str()) {
        let token = env.get("OPENAI_API_KEY").and_then(|v| v.as_str()).unwrap_or("");
        return Some((base_url.to_string(), token.to_string()));
    }
    None
}

// ---- Fallback 执行器 ----

pub fn is_retryable_error(e: &str) -> bool {
    e.contains("connect") || e.contains("timeout") || e.contains("timed out")
        || e.contains("API 5") || e.contains(" 500") || e.contains(" 502") || e.contains(" 503")
        || e.contains("429") || e.contains("Too Many Requests")
        || e.contains("connection") || e.contains("Connection")
}

pub fn is_auth_error(e: &str) -> bool {
    e.contains("401") || e.contains("403")
        || e.contains("Unauthorized") || e.contains("Forbidden")
}

pub fn try_agent_chain<F, T>(chain: &[AgentChannelCredentials], mut f: F) -> Result<T, String>
where
    F: FnMut(&AgentChannelCredentials) -> Result<T, String>,
{
    let mut last_err = "Agent chain 为空".to_string();
    for cred in chain {
        match f(cred) {
            Ok(v) => return Ok(v),
            Err(e) => {
                if is_auth_error(&e) {
                    eprintln!("[fallback] 跳过渠道 {} (auth error): {}", cred.id, e);
                    last_err = e;
                    continue;
                }
                if is_retryable_error(&e) {
                    eprintln!("[fallback] 渠道 {} 失败, fallback: {}", cred.id, e);
                    last_err = e;
                    continue;
                }
                return Err(e);
            }
        }
    }
    Err(format!("所有渠道均失败。最后错误: {}", last_err))
}

// ---- 前端命令 ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelView {
    pub id: String,
    pub name: String,
    pub note: Option<String>,
    pub base_url: Option<String>,
    pub auth_token_masked: Option<String>,
    pub extra_env_keys: Vec<String>,
    pub valid: bool,
    pub enabled: bool,
    pub protocol: String,
    pub scope: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelListResult {
    pub channels: Vec<ChannelView>,
    pub session_chain: Vec<String>,
    pub agent_chain: Vec<String>,
}

fn build_channel_view(id: &str, meta: &ChannelMeta) -> ChannelView {
    if id == OFFICIAL_ID {
        return ChannelView {
            id: OFFICIAL_ID.to_string(),
            name: meta.name.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "Official".to_string()),
            note: meta.note.clone().filter(|s| !s.is_empty()),
            base_url: None,
            auth_token_masked: None,
            extra_env_keys: vec![],
            valid: true,
            enabled: meta.is_enabled(),
            protocol: "anthropic".to_string(),
            scope: "full".to_string(),
        };
    }
    if id == APPLE_FM_ID {
        return ChannelView {
            id: APPLE_FM_ID.to_string(),
            name: meta.name.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| "Apple FM".to_string()),
            note: meta.note.clone().filter(|s| !s.is_empty()),
            base_url: Some(format!("http://localhost:{}", APPLE_FM_PORT)),
            auth_token_masked: None,
            extra_env_keys: vec![],
            valid: true,
            enabled: meta.is_enabled(),
            protocol: "openai".to_string(),
            scope: "agent-only".to_string(),
        };
    }
    let path = channel_file_path(id);
    let parsed: Option<Value> = fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let valid = parsed.is_some();
    let env = parsed
        .as_ref()
        .and_then(|v| v.get("env"))
        .and_then(|v| v.as_object());
    let is_openai = meta.protocol() == "openai";
    let (url_key, token_key) = if is_openai {
        ("OPENAI_BASE_URL", "OPENAI_API_KEY")
    } else {
        ("ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN")
    };
    let base_url = env
        .and_then(|e| e.get(url_key))
        .and_then(|v| v.as_str())
        .map(String::from);
    let token = env
        .and_then(|e| e.get(token_key))
        .and_then(|v| v.as_str())
        .filter(|t| !t.is_empty());
    let hidden_keys: &[&str] = if is_openai {
        &["OPENAI_BASE_URL", "OPENAI_API_KEY"]
    } else {
        &["ANTHROPIC_BASE_URL", "ANTHROPIC_AUTH_TOKEN"]
    };
    let extra_env_keys = env
        .map(|e| {
            e.keys()
                .filter(|k| !hidden_keys.contains(&k.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    ChannelView {
        name: meta.name.clone().filter(|s| !s.is_empty()).unwrap_or_else(|| id.to_string()),
        note: meta.note.clone().filter(|s| !s.is_empty()),
        base_url,
        auth_token_masked: token.map(mask_token),
        extra_env_keys,
        valid,
        enabled: meta.is_enabled(),
        id: id.to_string(),
        protocol: meta.protocol().to_string(),
        scope: meta.scope().to_string(),
    }
}

#[tauri::command]
pub fn list_channels() -> ChannelListResult {
    let settings = load_app_settings();

    // 收集所有渠道 ID（文件 + official）
    let file_ids = scan_channel_ids();
    let mut seen = std::collections::HashSet::new();
    let mut channels = Vec::new();

    // 按 session_chain 顺序优先
    for id in settings.session_chain.iter().chain(settings.agent_chain.iter()) {
        if !seen.insert(id.clone()) { continue; }
        let meta = settings.channels.get(id).cloned().unwrap_or_default();
        if id == OFFICIAL_ID || id == APPLE_FM_ID || file_ids.contains(id) {
            channels.push(build_channel_view(id, &meta));
        }
    }
    // 追加未入链的文件渠道
    for id in &file_ids {
        if seen.insert(id.clone()) {
            let meta = settings.channels.get(id).cloned().unwrap_or_default();
            channels.push(build_channel_view(id, &meta));
        }
    }

    ChannelListResult {
        channels,
        session_chain: settings.session_chain,
        agent_chain: settings.agent_chain,
    }
}

#[tauri::command]
pub fn save_channel(
    id: String,
    name: String,
    base_url: String,
    auth_token: Option<String>,
    note: Option<String>,
    protocol: Option<String>,
    scope: Option<String>,
) -> Result<(), String> {
    validate_id(&id)?;
    let base_url = base_url.trim().to_string();
    if base_url.is_empty() {
        return Err("Base URL 不能为空".to_string());
    }
    let is_openai = protocol.as_deref() == Some("openai");
    fs::create_dir_all(channels_dir()).map_err(|e| e.to_string())?;
    let path = channel_file_path(&id);

    let mut root: Value = match fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| {
            format!("渠道文件已存在但 JSON 解析失败,请先手动修复后重试({}): {}", path.display(), e)
        })?,
        Err(_) => json!({}),
    };
    let obj = root
        .as_object_mut()
        .ok_or("渠道文件顶层不是 JSON 对象,请手动修复后重试")?;
    let env = obj.entry("env").or_insert_with(|| json!({}));
    let env_obj = env.as_object_mut().ok_or("渠道文件 env 字段不是对象")?;
    let token = auth_token.as_deref().map(str::trim).filter(|t| !t.is_empty());
    if is_openai {
        env_obj.insert("OPENAI_BASE_URL".to_string(), json!(base_url));
        if let Some(t) = token {
            env_obj.insert("OPENAI_API_KEY".to_string(), json!(t));
        }
    } else {
        env_obj.insert("ANTHROPIC_BASE_URL".to_string(), json!(base_url));
        if let Some(t) = token {
            env_obj.insert("ANTHROPIC_AUTH_TOKEN".to_string(), json!(t));
        } else if env_obj
            .get("ANTHROPIC_AUTH_TOKEN")
            .and_then(|v| v.as_str())
            .filter(|t| !t.is_empty())
            .is_none()
        {
            return Err("新建渠道必须提供 Auth Token".to_string());
        }
    }
    write_json_0600(&path, &root)?;

    let mut settings = load_app_settings();
    let meta = settings.channels.entry(id.clone()).or_default();
    meta.name = Some(name.trim().to_string()).filter(|s| !s.is_empty());
    meta.note = note.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    meta.protocol = protocol;
    meta.scope = scope;

    let is_agent_only = meta.is_agent_only();
    if is_agent_only {
        settings.session_chain.retain(|x| x != &id);
    } else if !settings.session_chain.contains(&id) {
        settings.session_chain.push(id.clone());
    }
    if !settings.agent_chain.contains(&id) {
        settings.agent_chain.push(id.clone());
    }
    save_app_settings(&settings)
}

#[tauri::command]
pub fn delete_channel(id: String) -> Result<(), String> {
    validate_id(&id)?;
    let path = channel_file_path(&id);
    if path.is_file() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    let mut settings = load_app_settings();
    settings.channels.remove(&id);
    settings.session_chain.retain(|x| x != &id);
    settings.agent_chain.retain(|x| x != &id);
    save_app_settings(&settings)
}

#[tauri::command]
pub fn set_channel_enabled(id: String, enabled: bool) -> Result<(), String> {
    let mut settings = load_app_settings();
    let meta = settings.channels.entry(id).or_default();
    meta.enabled = Some(enabled);
    save_app_settings(&settings)
}

#[tauri::command]
pub fn set_session_chain(chain: Vec<String>) -> Result<(), String> {
    let mut settings = load_app_settings();
    settings.session_chain = chain;
    save_app_settings(&settings)
}

#[tauri::command]
pub fn set_agent_chain(chain: Vec<String>) -> Result<(), String> {
    let mut settings = load_app_settings();
    settings.agent_chain = chain;
    save_app_settings(&settings)
}

#[tauri::command]
pub fn get_agent_toggles() -> BTreeMap<String, bool> {
    load_app_settings().agent_toggles
}

#[tauri::command]
pub fn set_agent_toggle(key: String, enabled: bool) -> Result<(), String> {
    let mut settings = load_app_settings();
    settings.agent_toggles.insert(key, enabled);
    save_app_settings(&settings)
}

pub fn is_agent_enabled(key: &str) -> bool {
    load_app_settings().agent_toggles.get(key).copied().unwrap_or(true)
}

#[tauri::command]
pub fn get_agent_preferences() -> BTreeMap<String, AgentFeaturePrefs> {
    load_app_settings().agent_preferences
}

#[tauri::command]
pub fn set_agent_preferred_channel(key: String, channel_id: Option<String>) -> Result<(), String> {
    let mut settings = load_app_settings();
    let prefs = settings.agent_preferences.entry(key).or_default();
    prefs.preferred_channel = channel_id.filter(|s| !s.is_empty());
    save_app_settings(&settings)
}

#[tauri::command]
pub fn get_channel_token(id: String) -> Result<Option<String>, String> {
    if id == OFFICIAL_ID || id == APPLE_FM_ID {
        return Ok(None);
    }
    validate_id(&id)?;
    let path = channel_file_path(&id);
    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let root: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let env = root.get("env").and_then(|e| e.as_object());
    Ok(env
        .and_then(|e| {
            e.get("ANTHROPIC_AUTH_TOKEN")
                .or_else(|| e.get("OPENAI_API_KEY"))
        })
        .and_then(|v| v.as_str())
        .filter(|t| !t.is_empty())
        .map(String::from))
}

#[tauri::command]
pub fn reveal_channels_dir() -> Result<(), String> {
    let dir = channels_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let opener = "xdg-open";
    std::process::Command::new(opener)
        .arg(&dir)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---- spawn 注入(streaming.rs 消费) ----

pub struct ChannelInjection {
    pub settings_arg: String,
    pub env: Vec<(String, String)>,
    pub clear_env: Vec<String>,
    pub runtime_path: PathBuf,
}

const ADVISOR_MODEL: &str = "claude-fable-5";
const ADVISOR_ENABLE_ENV: &str = "CLAUDE_CODE_ENABLE_EXPERIMENTAL_ADVISOR_TOOL";

pub fn prepare_injection(
    channel_id: Option<&str>,
    session_id: &str,
    ultracode: bool,
    advisor: bool,
) -> Result<Option<ChannelInjection>, String> {
    if channel_id.is_none() && !ultracode && !advisor {
        return Ok(None);
    }

    let mut root: Value = match channel_id {
        Some(id) => {
            validate_id(id)?;
            let text = fs::read_to_string(channel_file_path(id))
                .map_err(|_| format!("渠道配置不存在或不可读: {}", id))?;
            serde_json::from_str(&text)
                .map_err(|e| format!("渠道配置 JSON 解析失败({}): {}", id, e))?
        }
        None => json!({}),
    };
    let obj = root.as_object_mut().ok_or_else(|| match channel_id {
        Some(id) => format!("渠道配置顶层不是 JSON 对象: {}", id),
        None => "注入配置顶层不是 JSON 对象".to_string(),
    })?;

    let mut env_pairs = Vec::new();
    let mut clear_env: Vec<String> = Vec::new();
    {
        let env = obj.entry("env").or_insert_with(|| json!({}));
        let env_obj = env.as_object_mut().ok_or("注入配置 env 字段不是对象")?;
        if advisor {
            env_obj.insert(ADVISOR_ENABLE_ENV.to_string(), json!("1"));
        }
        for (k, v) in env_obj.iter() {
            if let Some(s) = v.as_str() {
                env_pairs.push((k.clone(), s.to_string()));
            }
        }
        if channel_id.is_some() {
            clear_env.extend(DEFENSE_ENV_KEYS.iter().map(|s| s.to_string()));
            let channel_has_token = env_obj
                .get("ANTHROPIC_AUTH_TOKEN")
                .and_then(|v| v.as_str())
                .is_some_and(|t| !t.is_empty());
            if !channel_has_token {
                clear_env.push("ANTHROPIC_AUTH_TOKEN".to_string());
            }
            for key in &clear_env {
                env_obj.entry(key.clone()).or_insert_with(|| json!(""));
            }
        }
    }
    if advisor {
        obj.insert("advisorModel".to_string(), json!(ADVISOR_MODEL));
    }
    if ultracode {
        obj.insert("ultracode".to_string(), json!(true));
    }

    fs::create_dir_all(runtime_dir()).map_err(|e| e.to_string())?;
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let runtime_path = runtime_dir().join(format!("{}-{}.json", session_id, nanos));
    write_json_0600(&runtime_path, &root)?;
    Ok(Some(ChannelInjection {
        settings_arg: runtime_path.to_string_lossy().into_owned(),
        env: env_pairs,
        clear_env,
        runtime_path,
    }))
}

pub fn cleanup_runtime_file(path: &Path) {
    let _ = fs::remove_file(path);
}

pub fn cleanup_runtime_dir() {
    if let Ok(entries) = fs::read_dir(runtime_dir()) {
        for entry in entries.filter_map(|e| e.ok()) {
            let _ = fs::remove_file(entry.path());
        }
    }
}

// ---- 渠道探活 + 模型发现 ----

use std::sync::OnceLock;
use std::time::Duration;

fn probe_client() -> Result<&'static reqwest::blocking::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| e.clone())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub online: bool,
    pub status: String,
    pub models: Vec<String>,
    pub latency_ms: u64,
}

#[tauri::command]
pub async fn probe_channel(id: String) -> Result<ProbeResult, String> {
    if id == OFFICIAL_ID {
        return Ok(ProbeResult {
            online: true,
            status: "official".to_string(),
            models: vec![],
            latency_ms: 0,
        });
    }

    let settings = load_app_settings();
    let protocol = if id == APPLE_FM_ID {
        "openai".to_string()
    } else {
        settings.channels.get(&id).map_or("anthropic", |m| m.protocol()).to_string()
    };

    let (base_url, token) = if id == APPLE_FM_ID {
        (format!("http://localhost:{}", APPLE_FM_PORT), String::new())
    } else {
        read_channel_credentials(&id)
            .ok_or_else(|| format!("渠道 {} 凭据不可读", id))?
    };

    tauri::async_runtime::spawn_blocking(move || {
        probe_channel_blocking(&base_url, &token, &protocol)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn probe_channel_blocking(base_url: &str, token: &str, protocol: &str) -> Result<ProbeResult, String> {
    let client = probe_client()?;
    let url = format!("{}/v1/models", base_url.trim_end_matches('/'));
    let start = std::time::Instant::now();

    let mut req = client.get(&url);
    if protocol == "openai" {
        if !token.is_empty() {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
    } else {
        req = req.header("x-api-key", token);
        req = req.header("anthropic-version", "2023-06-01");
    }
    let resp = req.send();

    match resp {
        Ok(r) => {
            let latency = start.elapsed().as_millis() as u64;
            let status_code = r.status().as_u16();

            if status_code == 401 || status_code == 403 {
                return Ok(ProbeResult {
                    online: false,
                    status: "auth_error".to_string(),
                    models: vec![],
                    latency_ms: latency,
                });
            }

            let mut models = Vec::new();
            if r.status().is_success() {
                if let Ok(body) = r.json::<Value>() {
                    if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                        for item in data {
                            if let Some(id) = item.get("id").and_then(|v| v.as_str()) {
                                models.push(id.to_string());
                            }
                        }
                    }
                }
            }

            Ok(ProbeResult {
                online: true,
                status: format!("{}", status_code),
                models,
                latency_ms: latency,
            })
        }
        Err(e) => {
            let latency = start.elapsed().as_millis() as u64;
            Ok(ProbeResult {
                online: false,
                status: if e.is_timeout() { "timeout".to_string() } else { "offline".to_string() },
                models: vec![],
                latency_ms: latency,
            })
        }
    }
}

// ---- Apple FM 自动检测 & 进程管理 ----

use std::sync::Mutex;
use std::process::{Child, Command, Stdio};

static FM_PROCESS: Mutex<Option<Child>> = Mutex::new(None);

fn detect_apple_fm() -> bool {
    #[cfg(not(target_os = "macos"))]
    return false;

    #[cfg(target_os = "macos")]
    Command::new("which")
        .arg("fm")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn register_apple_fm_if_available() {
    if !detect_apple_fm() { return; }

    let mut settings = load_app_settings();
    if settings.channels.contains_key(APPLE_FM_ID) { return; }

    let meta = ChannelMeta {
        name: Some("Apple FM".to_string()),
        note: Some("Apple Foundation Models (local)".to_string()),
        enabled: Some(true),
        protocol: Some("openai".to_string()),
        scope: Some("agent-only".to_string()),
        ..Default::default()
    };
    settings.channels.insert(APPLE_FM_ID.to_string(), meta);
    if !settings.agent_chain.contains(&APPLE_FM_ID.to_string()) {
        settings.agent_chain.push(APPLE_FM_ID.to_string());
    }
    let _ = save_app_settings(&settings);
    eprintln!("[apple-fm] 检测到 fm 命令，已注册 Apple FM 渠道");
}

fn probe_port_open(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(500),
    ).is_ok()
}

pub fn ensure_fm_serve_running() -> Result<(), String> {
    let mut guard = FM_PROCESS.lock().unwrap_or_else(|e| e.into_inner());

    if let Some(ref mut child) = *guard {
        if child.try_wait().ok().flatten().is_none() && probe_port_open(APPLE_FM_PORT) {
            return Ok(());
        }
    }

    if probe_port_open(APPLE_FM_PORT) {
        return Ok(());
    }

    eprintln!("[apple-fm] 启动 fm serve --port {}", APPLE_FM_PORT);
    let child = Command::new("fm")
        .args(["serve", "--port", &APPLE_FM_PORT.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("fm serve 启动失败: {}", e))?;

    *guard = Some(child);
    // 等待服务就绪
    for _ in 0..10 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if probe_port_open(APPLE_FM_PORT) {
            eprintln!("[apple-fm] fm serve 已就绪");
            return Ok(());
        }
    }
    Err("fm serve 启动超时".to_string())
}

pub fn shutdown_fm_serve() {
    let mut guard = FM_PROCESS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(mut child) = guard.take() {
        let _ = child.kill();
        eprintln!("[apple-fm] fm serve 已关闭");
    }
}

