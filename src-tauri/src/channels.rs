//! 多渠道(profile)配置域:`~/.claude/cc-space/`
//!
//! - `settings.json`        应用设置:defaultChannelId + 渠道展示元数据(以文件名 stem 为 key)
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

/// 保留 id:语义为「官方/零注入」,不允许作为渠道文件名
pub const OFFICIAL_ID: &str = "official";

/// 注入渠道时无条件压制的认证/路由残留键:
/// - 继承环境层:spawn 前 env_remove(防父进程/shell 残留)
/// - settings 层:runtime 文件 env 块以空串占位(实测 CLI 的 settings env 会反向覆盖
///   进程环境变量,空串在 JS 侧按 falsy 处理等效于未设置)
/// 防止用户残留的官方 ANTHROPIC_API_KEY 以 x-api-key 头泄漏给第三方主机,
/// 或 Bedrock/Vertex 开关(认证优先级第一)劫持渠道路由。
/// 注意 ANTHROPIC_AUTH_TOKEN 不在此列——它是渠道自身的凭据载体,仅在渠道未提供时
/// 才条件性压制(见 prepare_injection),否则会清掉渠道要用的 token。
pub const DEFENSE_ENV_KEYS: [&str; 4] = [
    "ANTHROPIC_API_KEY",
    "CLAUDE_CODE_USE_BEDROCK",
    "CLAUDE_CODE_USE_VERTEX",
    "CLAUDE_CODE_USE_FOUNDRY",
];

fn cc_space_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("cc-space")
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

/// 渠道文件路径(id 必须先过 validate_id)
pub fn channel_file_path(id: &str) -> PathBuf {
    channels_dir().join(format!("{}.json", id))
}

/// 渠道 id 即文件名 stem:限定字符集防路径穿越;'official' 保留
pub fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err("渠道 ID 须为 1-64 个字符".to_string());
    }
    if id == OFFICIAL_ID {
        return Err("official 为保留 ID(语义为官方渠道)".to_string());
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

/// 渠道展示元数据。flatten 保留手编未知字段,save 不抹除
#[derive(Serialize, Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelMeta {
    pub name: Option<String>,
    pub note: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `~/.claude/cc-space/settings.json`:渠道域之外的未来设置项经 flatten 原样保留
#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub default_channel_id: Option<String>,
    pub channels: BTreeMap<String, ChannelMeta>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn load_app_settings() -> AppSettings {
    fs::read_to_string(settings_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
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

/// token 掩码:仅首尾各 4 字符,短 token 全掩
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

// ---- 前端命令 ----

/// 渠道的前端视图:不含任何敏感原值
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelView {
    pub id: String,
    pub name: String,
    pub note: Option<String>,
    pub base_url: Option<String>,
    pub auth_token_masked: Option<String>,
    /// env 块中 BASE_URL/AUTH_TOKEN 之外的键名(仅键,不含值):提示该渠道有高级配置
    pub extra_env_keys: Vec<String>,
    /// 文件 JSON 是否可解析(false 时设置页提示手动修复)
    pub valid: bool,
    pub is_default: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelListResult {
    pub channels: Vec<ChannelView>,
    pub default_channel_id: Option<String>,
}

/// 列出全部渠道(扫描 channels/*.json + 合并元数据)。用时重读,文件即事实源
#[tauri::command]
pub fn list_channels() -> ChannelListResult {
    let settings = load_app_settings();
    let mut channels = Vec::new();
    if let Ok(entries) = fs::read_dir(channels_dir()) {
        let mut files: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        files.sort();
        for path in files {
            let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(String::from) else {
                continue;
            };
            // 跳过过不了 validate_id 的文件(official.json、含非法字符的 stem):
            // 它们列出来也不可保存/删除/注入,徒增困惑
            if validate_id(&id).is_err() {
                continue;
            }
            let meta = settings.channels.get(&id).cloned().unwrap_or_default();
            let parsed: Option<Value> = fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());
            let valid = parsed.is_some();
            let env = parsed
                .as_ref()
                .and_then(|v| v.get("env"))
                .and_then(|v| v.as_object());
            let base_url = env
                .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
                .and_then(|v| v.as_str())
                .map(String::from);
            let token = env
                .and_then(|e| e.get("ANTHROPIC_AUTH_TOKEN"))
                .and_then(|v| v.as_str())
                .filter(|t| !t.is_empty());
            let extra_env_keys = env
                .map(|e| {
                    e.keys()
                        .filter(|k| {
                            k.as_str() != "ANTHROPIC_BASE_URL" && k.as_str() != "ANTHROPIC_AUTH_TOKEN"
                        })
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            channels.push(ChannelView {
                name: meta
                    .name
                    .clone()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| id.clone()),
                note: meta.note.clone().filter(|s| !s.is_empty()),
                base_url,
                auth_token_masked: token.map(mask_token),
                extra_env_keys,
                valid,
                is_default: settings.default_channel_id.as_deref() == Some(id.as_str()),
                id,
            });
        }
    }
    // 悬空默认(手删了渠道文件但 settings.json 仍指向它):视为未设默认,
    // 否则前端下拉/设置页 select 绑一个无对应选项的 id 会显示空白
    let default_channel_id = settings
        .default_channel_id
        .filter(|id| channels.iter().any(|c| &c.id == id));
    ChannelListResult {
        channels,
        default_channel_id,
    }
}

/// 新建/编辑渠道。编辑时 auth_token 传空 = 保持不变;只更新表单覆盖的 env 键,
/// 用户手编的其他字段(model/permissions/额外 env)原样保留
#[tauri::command]
pub fn save_channel(
    id: String,
    name: String,
    base_url: String,
    auth_token: Option<String>,
    note: Option<String>,
) -> Result<(), String> {
    validate_id(&id)?;
    let base_url = base_url.trim().to_string();
    if base_url.is_empty() {
        return Err("Base URL 不能为空".to_string());
    }
    fs::create_dir_all(channels_dir()).map_err(|e| e.to_string())?;
    let path = channel_file_path(&id);

    // 文件已存在但 JSON 损坏时报错而非按空对象重建——否则会静默丢弃用户手编的
    // model/permissions/额外 env 等全部字段。文件不存在(新建)才用空对象起步。
    let mut root: Value = match fs::read_to_string(&path) {
        Ok(s) => serde_json::from_str(&s).map_err(|e| {
            format!(
                "渠道文件已存在但 JSON 解析失败,请先手动修复后重试({}): {}",
                path.display(),
                e
            )
        })?,
        Err(_) => json!({}),
    };
    let obj = root
        .as_object_mut()
        .ok_or("渠道文件顶层不是 JSON 对象,请手动修复后重试")?;
    let env = obj.entry("env").or_insert_with(|| json!({}));
    let env_obj = env.as_object_mut().ok_or("渠道文件 env 字段不是对象")?;
    env_obj.insert("ANTHROPIC_BASE_URL".to_string(), json!(base_url));
    let token = auth_token
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty());
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
    write_json_0600(&path, &root)?;

    let mut settings = load_app_settings();
    let meta = settings.channels.entry(id).or_default();
    meta.name = Some(name.trim().to_string()).filter(|s| !s.is_empty());
    meta.note = note.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    save_app_settings(&settings)
}

/// 删除渠道文件与元数据;若为默认渠道则一并清除默认指向
#[tauri::command]
pub fn delete_channel(id: String) -> Result<(), String> {
    validate_id(&id)?;
    let path = channel_file_path(&id);
    if path.is_file() {
        fs::remove_file(&path).map_err(|e| e.to_string())?;
    }
    let mut settings = load_app_settings();
    settings.channels.remove(&id);
    if settings.default_channel_id.as_deref() == Some(id.as_str()) {
        settings.default_channel_id = None;
    }
    save_app_settings(&settings)
}

/// 设置默认渠道。None = 官方(零注入)
#[tauri::command]
pub fn set_default_channel(id: Option<String>) -> Result<(), String> {
    if let Some(ref i) = id {
        validate_id(i)?;
        if !channel_file_path(i).is_file() {
            return Err(format!("渠道配置不存在: {}", i));
        }
    }
    let mut settings = load_app_settings();
    settings.default_channel_id = id;
    save_app_settings(&settings)
}

/// 在 Finder/资源管理器中打开渠道配置目录(高级 env 手编入口)
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

/// 渠道注入产物
pub struct ChannelInjection {
    /// 传给 `--settings` 的 runtime 文件路径(argv 只见路径,token 不可见于 ps)
    pub settings_arg: String,
    /// 渠道 env 块的真实键值:spawn env 双保险注入,堵 --settings 启动期个别请求
    /// 未合并命令行设置的缝隙(实测 7 之 1)
    pub env: Vec<(String, String)>,
    /// 需从继承环境移除的键(spawn 前 env_remove):DEFENSE_ENV_KEYS 固定项 +
    /// 渠道自身未提供 ANTHROPIC_AUTH_TOKEN 时条件性加入它(防继承的官方 token 发往第三方)
    pub clear_env: Vec<String>,
    /// 进程结束后清理用
    pub runtime_path: PathBuf,
}

/// 顾问模式注入的顾问模型(经 settings 下发——/advisor 命令只接受 opus/sonnet,
/// 填不了 fable,必须走 advisorModel 字段)。未来可在设置页全局配置(见 settings-backlog 第 3 条)
const ADVISOR_MODEL: &str = "claude-fable-5";
/// 顾问功能默认灰度隐藏,该 env flag 强制点亮;远期 CLI 放开灰度后可去掉
const ADVISOR_ENABLE_ENV: &str = "CLAUDE_CODE_ENABLE_EXPERIMENTAL_ADVISOR_TOOL";

/// 读渠道文件(可选) → 合并防御空值、ultracode、顾问配置 → 写 per-spawn runtime 文件(0600)。
/// `channel_id=None` 时从空配置起步(纯顾问/ultracode 注入,官方渠道态)。
/// 三类注入(渠道 / ultracode / 顾问)全无时返回 `None`(无需 `--settings`)。
/// `--settings` 只能出现一次,故三者必须合并进同一文件。
/// 文件名带纳秒后缀:同会话连发时旧 turn 退出清理不会误删新 turn 的文件
pub fn prepare_injection(
    channel_id: Option<&str>,
    session_id: &str,
    ultracode: bool,
    advisor: bool,
) -> Result<Option<ChannelInjection>, String> {
    // 三类注入全无 → 不产出合成文件,调用方不附加 --settings
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
        None => "顾问注入配置顶层不是 JSON 对象".to_string(),
    })?;

    let mut env_pairs = Vec::new();
    let mut clear_env: Vec<String> = Vec::new();
    {
        let env = obj.entry("env").or_insert_with(|| json!({}));
        let env_obj = env
            .as_object_mut()
            .ok_or("注入配置 env 字段不是对象")?;
        // 顾问 env flag 先于收集插入 → 与渠道 env 同等待遇做 spawn 双注入(堵 --settings 启动期缝隙)
        if advisor {
            env_obj.insert(ADVISOR_ENABLE_ENV.to_string(), json!("1"));
        }
        for (k, v) in env_obj.iter() {
            if let Some(s) = v.as_str() {
                env_pairs.push((k.clone(), s.to_string()));
            }
        }
        // 渠道在场才压制认证残留(防第三方劫持);纯顾问/ultracode 为官方态无需占位
        if channel_id.is_some() {
            clear_env.extend(DEFENSE_ENV_KEYS.iter().map(|s| s.to_string()));
            // 渠道未提供非空 AUTH_TOKEN(手编漏填/用其他鉴权)时一并压制:
            // 否则继承环境残留的官方 ANTHROPIC_AUTH_TOKEN 会随 Bearer 头发往第三方 base_url
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

/// 进程结束后删除本 turn 的 runtime 文件(含 token 副本,不留盘)
pub fn cleanup_runtime_file(path: &Path) {
    let _ = fs::remove_file(path);
}

/// 应用启动兜底:清空 runtime 目录(上次异常退出的残留)
pub fn cleanup_runtime_dir() {
    if let Ok(entries) = fs::read_dir(runtime_dir()) {
        for entry in entries.filter_map(|e| e.ok()) {
            let _ = fs::remove_file(entry.path());
        }
    }
}
