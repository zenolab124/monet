use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public types — exposed via IPC
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaInfo {
    pub session: Option<QuotaWindow>,
    pub weekly: Option<QuotaWindow>,
    pub weekly_models: Vec<ModelQuota>,
    pub extra_usage: Option<ExtraUsage>,
    pub plan: Option<String>,
    pub account_email: Option<String>,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub used_percent: f64,
    pub resets_at: Option<String>,
    pub resets_in_secs: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelQuota {
    pub model: String,
    pub display_name: Option<String>,
    pub used_percent: f64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtraUsage {
    pub enabled: bool,
    pub monthly_limit_usd: Option<f64>,
    pub used_usd: Option<f64>,
    pub used_percent: f64,
}

// ---------------------------------------------------------------------------
// Raw API response types (snake_case from Anthropic)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ApiUsageResponse {
    five_hour: Option<ApiWindow>,
    seven_day: Option<ApiWindow>,
    seven_day_opus: Option<ApiWindow>,
    seven_day_sonnet: Option<ApiWindow>,
    seven_day_routines: Option<ApiWindow>,
    seven_day_oauth_apps: Option<ApiWindow>,
    extra_usage: Option<ApiExtraUsage>,
    limits: Option<Vec<ApiLimitEntry>>,
}

#[derive(Debug, Deserialize)]
struct ApiWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiExtraUsage {
    is_enabled: Option<bool>,
    monthly_limit: Option<f64>,
    used_credits: Option<f64>,
    utilization: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct ApiLimitEntry {
    #[allow(dead_code)]
    kind: Option<String>,
    #[allow(dead_code)]
    group: Option<String>,
    percent: Option<f64>,
    resets_at: Option<String>,
    scope: Option<ApiScope>,
}

#[derive(Debug, Deserialize)]
struct ApiScope {
    model: Option<ApiScopeModel>,
}

#[derive(Debug, Deserialize)]
struct ApiScopeModel {
    id: Option<String>,
    display_name: Option<String>,
}

// ---------------------------------------------------------------------------
// Credential types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CredentialsFile {
    claude_ai_oauth: Option<OAuthCredential>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OAuthCredential {
    access_token: Option<String>,
    /// 存在于凭据文件中但 Monet 故意不用：refresh 是 CLI 的专属权力，
    /// 我方 refresh 会在 token rotation 下作废 CLI 的登录态
    #[allow(dead_code)]
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    scopes: Option<Vec<String>>,
    rate_limit_tier: Option<String>,
    subscription_type: Option<String>,
}


// ---------------------------------------------------------------------------
// Cache (memory + disk)
// ---------------------------------------------------------------------------

struct CachedQuota {
    info: QuotaInfo,
    fetched_at: Instant,
}

static CACHE: Mutex<Option<CachedQuota>> = Mutex::new(None);

const CACHE_TTL: Duration = Duration::from_secs(120);

#[derive(Serialize, Deserialize)]
struct DiskCache {
    info: QuotaInfo,
    fetched_at_ms: i64,
}

fn disk_cache_path() -> std::path::PathBuf {
    crate::config::data_dir().join("quota-cache.json")
}

fn read_disk_cache() -> Option<(QuotaInfo, i64)> {
    let content = std::fs::read_to_string(disk_cache_path()).ok()?;
    let dc: DiskCache = serde_json::from_str(&content).ok()?;
    Some((dc.info, dc.fetched_at_ms))
}

fn write_disk_cache(info: &QuotaInfo) {
    let dc = DiskCache {
        info: info.clone(),
        fetched_at_ms: Utc::now().timestamp_millis(),
    };
    if let Ok(json) = serde_json::to_string(&dc) {
        // tray 与主应用双进程并发读写，必须原子写，防半截 JSON
        let _ = crate::config::atomic_write(&disk_cache_path(), &json);
    }
}

fn disk_cache_age_secs() -> Option<i64> {
    let content = std::fs::read_to_string(disk_cache_path()).ok()?;
    let dc: DiskCache = serde_json::from_str(&content).ok()?;
    let age = Utc::now().timestamp_millis() - dc.fetched_at_ms;
    Some(age / 1000)
}

// ---------------------------------------------------------------------------
// IPC commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_quota() -> QuotaInfo {
    if let Ok(guard) = CACHE.lock() {
        if let Some(cached) = guard.as_ref() {
            if cached.fetched_at.elapsed() < CACHE_TTL {
                return cached.info.clone();
            }
        }
    }
    fetch_and_cache(false)
}

/// 手动刷新：跳过内存缓存与磁盘 TTL，强制打 API。
/// 同时被 monet-tray 独立进程调用。
#[tauri::command]
pub fn refresh_quota() -> QuotaInfo {
    fetch_and_cache(true)
}

/// 只读取现有缓存（内存 → 磁盘），绝不发起网络请求。
/// 供 monet-tray 主线程即时重渲染（如 tray-title 配置变更）使用。
pub fn peek_cached_quota() -> Option<QuotaInfo> {
    if let Ok(guard) = CACHE.lock() {
        if let Some(cached) = guard.as_ref() {
            return Some(cached.info.clone());
        }
    }
    read_disk_cache().map(|(info, _)| info)
}

#[tauri::command]
pub fn quota_available() -> bool {
    read_credential().is_some()
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

fn fetch_and_cache(force: bool) -> QuotaInfo {
    // 磁盘缓存 TTL 内直接采用（另一进程可能刚刷过，双进程共享省 API 调用）；
    // force（用户手动刷新）跳过，保证「点了刷新就真的刷新」
    if !force {
        if let Some(age) = disk_cache_age_secs() {
            if age < CACHE_TTL.as_secs() as i64 {
                if let Some((info, _)) = read_disk_cache() {
                    if info.error.is_none() {
                        if let Ok(mut guard) = CACHE.lock() {
                            *guard = Some(CachedQuota {
                                info: info.clone(),
                                fetched_at: Instant::now(),
                            });
                        }
                        return info;
                    }
                }
            }
        }
    }

    let info = match fetch_quota_inner() {
        Ok(info) => info,
        Err(e) => {
            if let Some((cached, _)) = read_disk_cache() {
                if cached.error.is_none() {
                    return cached;
                }
            }
            QuotaInfo {
                session: None,
                weekly: None,
                weekly_models: vec![],
                extra_usage: None,
                plan: None,
                account_email: None,
                updated_at: Utc::now().to_rfc3339(),
                error: Some(e),
            }
        },
    };
    if info.error.is_none() {
        write_disk_cache(&info);
    }
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(CachedQuota {
            info: info.clone(),
            fetched_at: Instant::now(),
        });
    }
    info
}

fn fetch_quota_inner() -> Result<QuotaInfo, String> {
    let (access_token, credential) = get_valid_token()?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {access_token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", "monet/1.0")
        .send()
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(format!("API error {status}: {body}"));
    }

    let api: ApiUsageResponse = resp.json().map_err(|e| format!("Parse error: {e}"))?;
    let now = Utc::now();

    let session = api.five_hour.as_ref().and_then(|w| w.utilization.map(|u| {
        make_window(u, w.resets_at.as_deref(), &now)
    }));

    let weekly = select_primary_weekly(&api).map(|w| {
        make_window(w.0, w.1, &now)
    });

    let weekly_models = build_model_quotas(&api);

    let extra_usage = api.extra_usage.as_ref().and_then(|e| {
        if e.is_enabled != Some(true) { return None; }
        Some(ExtraUsage {
            enabled: true,
            monthly_limit_usd: e.monthly_limit.map(|v| v / 100.0),
            used_usd: e.used_credits.map(|v| v / 100.0),
            used_percent: e.utilization.unwrap_or(0.0),
        })
    });

    let plan = infer_plan(&credential);

    Ok(QuotaInfo {
        session,
        weekly,
        weekly_models,
        extra_usage,
        plan,
        account_email: None,
        updated_at: Utc::now().to_rfc3339(),
        error: None,
    })
}

fn make_window(utilization: f64, resets_at: Option<&str>, now: &DateTime<Utc>) -> QuotaWindow {
    let resets_in = resets_at.and_then(|s| {
        DateTime::parse_from_rfc3339(s).ok().map(|dt| {
            (dt.with_timezone(&Utc) - *now).num_seconds().max(0)
        })
    });
    QuotaWindow {
        used_percent: utilization,
        resets_at: resets_at.map(|s| s.to_string()),
        resets_in_secs: resets_in,
    }
}

fn select_primary_weekly(api: &ApiUsageResponse) -> Option<(f64, Option<&str>)> {
    for w in [&api.seven_day, &api.seven_day_oauth_apps, &api.seven_day_sonnet, &api.seven_day_opus] {
        if let Some(win) = w {
            if let Some(u) = win.utilization {
                return Some((u, win.resets_at.as_deref()));
            }
        }
    }
    None
}

fn build_model_quotas(api: &ApiUsageResponse) -> Vec<ModelQuota> {
    let mut out = vec![];
    if let Some(limits) = &api.limits {
        for entry in limits {
            if let (Some(pct), Some(scope)) = (entry.percent, &entry.scope) {
                if let Some(model) = &scope.model {
                    out.push(ModelQuota {
                        model: model.id.clone().unwrap_or_default(),
                        display_name: model.display_name.clone(),
                        used_percent: pct,
                        resets_at: entry.resets_at.clone(),
                    });
                }
            }
        }
    }
    // Also add static windows if not already covered by limits
    let ids: std::collections::HashSet<&str> = out.iter().map(|m| m.model.as_str()).collect();
    if ids.is_empty() {
        if let Some(w) = &api.seven_day_sonnet {
            if let Some(u) = w.utilization {
                out.push(ModelQuota {
                    model: "sonnet".into(),
                    display_name: Some("Sonnet".into()),
                    used_percent: u,
                    resets_at: w.resets_at.clone(),
                });
            }
        }
        if let Some(w) = &api.seven_day_opus {
            if let Some(u) = w.utilization {
                out.push(ModelQuota {
                    model: "opus".into(),
                    display_name: Some("Opus".into()),
                    used_percent: u,
                    resets_at: w.resets_at.clone(),
                });
            }
        }
    }
    out
}

fn infer_plan(cred: &OAuthCredential) -> Option<String> {
    let combined: String = [&cred.subscription_type, &cred.rate_limit_tier]
        .iter()
        .filter_map(|s| s.as_ref())
        .map(|s| s.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    if combined.is_empty() { return None; }

    if combined.contains("max") {
        if combined.contains("20x") { return Some("Max 20x".into()); }
        if combined.contains("5x") { return Some("Max 5x".into()); }
        return Some("Max".into());
    }
    if combined.contains("ultra") { return Some("Ultra".into()); }
    if combined.contains("team") { return Some("Team".into()); }
    if combined.contains("enterprise") { return Some("Enterprise".into()); }
    if combined.contains("pro") { return Some("Pro".into()); }
    None
}

// ---------------------------------------------------------------------------
// Token management
// ---------------------------------------------------------------------------

/// 获取可用的 access token。
/// 铁律：Monet 绝不主动 refresh OAuth token——refresh token rotation 场景下，
/// 我方刷新会作废 Claude Code CLI 持有的 refresh_token，烧毁用户 CLI 登录态。
/// （tray + 主应用 + CLI 三方共用同一凭据，只有 CLI 拥有写权。）
/// token 过期时重读凭据源（CLI 日常使用会保持 keychain 新鲜），仍过期则报错等待。
fn get_valid_token() -> Result<(String, OAuthCredential), String> {
    let cred = read_credential().ok_or("No Claude credentials found")?;

    let has_profile = cred.scopes.as_ref()
        .map_or(false, |s| s.iter().any(|sc| sc == "user:profile"));
    if !has_profile {
        return Err("Credentials lack user:profile scope".into());
    }

    let now_ms = Utc::now().timestamp_millis() as u64;
    if cred.expires_at.map_or(false, |exp| now_ms >= exp.saturating_sub(60_000)) {
        return Err("Token expired; waiting for Claude Code CLI to refresh it".into());
    }
    let token = cred.access_token.clone().ok_or("No access token")?;
    Ok((token, cred))
}

// ---------------------------------------------------------------------------
// Credential reading (platform-specific)
// ---------------------------------------------------------------------------

fn read_credential() -> Option<OAuthCredential> {
    #[cfg(target_os = "macos")]
    if let Some(cred) = read_keychain_credential() {
        return Some(cred);
    }
    read_file_credential()
}

#[cfg(target_os = "macos")]
fn read_keychain_credential() -> Option<OAuthCredential> {
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", "Claude Code-credentials", "-w"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let json_str = String::from_utf8(output.stdout).ok()?;
    let creds: CredentialsFile = serde_json::from_str(json_str.trim()).ok()?;
    creds.claude_ai_oauth
}

fn read_file_credential() -> Option<OAuthCredential> {
    let home = dirs::home_dir()?;
    let path = home.join(".claude").join(".credentials.json");
    let content = std::fs::read_to_string(path).ok()?;
    let creds: CredentialsFile = serde_json::from_str(&content).ok()?;
    creds.claude_ai_oauth
}

// ---------------------------------------------------------------------------
// Tray tooltip (已用语义)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tray title (menu bar text next to icon)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayTitleConfig {
    pub slots: Vec<TrayTitleSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TrayTitleSlot {
    Session,
    Weekly,
    Model(String),
}

impl Default for TrayTitleConfig {
    fn default() -> Self {
        Self {
            slots: vec![TrayTitleSlot::Session, TrayTitleSlot::Model("Fable".into())],
        }
    }
}

fn tray_title_config_path() -> std::path::PathBuf {
    crate::config::data_dir().join("tray-title.json")
}

pub fn read_tray_title_config() -> TrayTitleConfig {
    std::fs::read_to_string(tray_title_config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_tray_title_config() -> TrayTitleConfig {
    read_tray_title_config()
}

#[tauri::command]
pub fn set_tray_title_config(slots: Vec<TrayTitleSlot>) -> Result<(), String> {
    let cfg = TrayTitleConfig { slots };
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    // 原子写：tray 进程靠 mtime 侦测变更并即时重渲染，半截 JSON 会导致一次错误渲染
    crate::config::atomic_write(&tray_title_config_path(), &json).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn format_tray_title(info: &QuotaInfo) -> Option<String> {
    let cfg = read_tray_title_config();
    if cfg.slots.is_empty() { return None; }

    let parts: Vec<String> = cfg.slots.iter().filter_map(|slot| {
        match slot {
            TrayTitleSlot::Session => {
                info.session.as_ref().map(|s| format!("{:.0}%", s.used_percent))
            }
            TrayTitleSlot::Weekly => {
                info.weekly.as_ref().map(|w| format!("{:.0}%", w.used_percent))
            }
            TrayTitleSlot::Model(name) => {
                info.weekly_models.iter()
                    .find(|m| {
                        m.display_name.as_deref() == Some(name.as_str())
                            || m.model.eq_ignore_ascii_case(name)
                    })
                    .map(|m| format!("{:.0}%", m.used_percent))
            }
        }
    }).collect();

    if parts.is_empty() { None } else { Some(parts.join(" | ")) }
}

pub fn format_tray_tooltip(info: &QuotaInfo) -> String {
    let mut parts = vec![];
    if let Some(s) = &info.session {
        parts.push(format!("Session: {:.0}% used", s.used_percent));
    }
    if let Some(w) = &info.weekly {
        parts.push(format!("Weekly: {:.0}% used", w.used_percent));
    }
    if let Some(plan) = &info.plan {
        parts.push(format!("Plan: {plan}"));
    }
    if parts.is_empty() {
        "Monet".into()
    } else {
        parts.join("\n")
    }
}
