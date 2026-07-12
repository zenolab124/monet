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
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    scopes: Option<Vec<String>>,
    rate_limit_tier: Option<String>,
    subscription_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

// ---------------------------------------------------------------------------
// Cache (memory + disk)
// ---------------------------------------------------------------------------

struct CachedQuota {
    info: QuotaInfo,
    fetched_at: Instant,
}

static CACHE: Mutex<Option<CachedQuota>> = Mutex::new(None);
static TOKEN_CACHE: Mutex<Option<TokenState>> = Mutex::new(None);

struct TokenState {
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    credential: OAuthCredential,
}

const CACHE_TTL: Duration = Duration::from_secs(120);
const OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

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
        let _ = std::fs::write(disk_cache_path(), json);
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
    fetch_and_cache()
}

#[tauri::command]
pub fn refresh_quota() -> QuotaInfo {
    fetch_and_cache()
}

#[tauri::command]
pub fn quota_available() -> bool {
    read_credential().is_some()
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

fn fetch_and_cache() -> QuotaInfo {
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
    let token = get_valid_token()?;
    let credential = token.credential.clone();

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {}", token.access_token))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", "monet/1.0")
        .send()
        .map_err(|e| format!("Network error: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            if let Ok(mut guard) = TOKEN_CACHE.lock() {
                *guard = None;
            }
        }
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

fn get_valid_token() -> Result<TokenState, String> {
    if let Ok(guard) = TOKEN_CACHE.lock() {
        if let Some(state) = guard.as_ref() {
            let now_ms = Utc::now().timestamp_millis() as u64;
            if state.expires_at.map_or(true, |exp| now_ms < exp.saturating_sub(60_000)) {
                return Ok(TokenState {
                    access_token: state.access_token.clone(),
                    refresh_token: state.refresh_token.clone(),
                    expires_at: state.expires_at,
                    credential: state.credential.clone(),
                });
            }
            if let Some(rt) = &state.refresh_token {
                if let Ok(refreshed) = refresh_token(rt) {
                    let new_state = TokenState {
                        access_token: refreshed.access_token.clone(),
                        refresh_token: refreshed.refresh_token.clone().or_else(|| state.refresh_token.clone()),
                        expires_at: refreshed.expires_in.map(|ei| {
                            Utc::now().timestamp_millis() as u64 + ei * 1000
                        }),
                        credential: state.credential.clone(),
                    };
                    drop(guard);
                    if let Ok(mut g) = TOKEN_CACHE.lock() {
                        let ret = TokenState {
                            access_token: new_state.access_token.clone(),
                            refresh_token: new_state.refresh_token.clone(),
                            expires_at: new_state.expires_at,
                            credential: new_state.credential.clone(),
                        };
                        *g = Some(new_state);
                        return Ok(ret);
                    }
                }
            }
        }
    }

    let cred = read_credential().ok_or("No Claude credentials found")?;

    let has_profile = cred.scopes.as_ref()
        .map_or(false, |s| s.iter().any(|sc| sc == "user:profile"));
    if !has_profile {
        return Err("Credentials lack user:profile scope".into());
    }

    let now_ms = Utc::now().timestamp_millis() as u64;
    let (access_token, refresh_token, expires_at) =
        if cred.expires_at.map_or(false, |exp| now_ms >= exp.saturating_sub(60_000)) {
            if let Some(rt) = &cred.refresh_token {
                let refreshed = refresh_token(rt)?;
                (
                    refreshed.access_token,
                    refreshed.refresh_token.or_else(|| cred.refresh_token.clone()),
                    refreshed.expires_in.map(|ei| now_ms + ei * 1000),
                )
            } else {
                return Err("Token expired and no refresh token available".into());
            }
        } else {
            (
                cred.access_token.clone().ok_or("No access token")?,
                cred.refresh_token.clone(),
                cred.expires_at,
            )
        };

    let state = TokenState {
        access_token: access_token.clone(),
        refresh_token: refresh_token.clone(),
        expires_at,
        credential: cred,
    };
    if let Ok(mut guard) = TOKEN_CACHE.lock() {
        *guard = Some(TokenState {
            access_token: state.access_token.clone(),
            refresh_token: state.refresh_token.clone(),
            expires_at: state.expires_at,
            credential: state.credential.clone(),
        });
    }
    Ok(state)
}

fn refresh_token(refresh_token: &str) -> Result<RefreshResponse, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .post("https://platform.claude.com/v1/oauth/token")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header("Accept", "application/json")
        .body(format!(
            "grant_type=refresh_token&refresh_token={}&client_id={}",
            urlencoding::encode(refresh_token),
            OAUTH_CLIENT_ID,
        ))
        .send()
        .map_err(|e| format!("Refresh error: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Refresh failed: {}", resp.status()));
    }
    resp.json::<RefreshResponse>().map_err(|e| format!("Refresh parse error: {e}"))
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
pub fn set_tray_title_config(app: tauri::AppHandle, slots: Vec<TrayTitleSlot>) -> Result<(), String> {
    let cfg = TrayTitleConfig { slots };
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(tray_title_config_path(), json).map_err(|e| e.to_string())?;

    // Immediately update tray title from cached quota
    use tauri::Manager;
    if let Some(tray) = app.tray_by_id(&tauri::tray::TrayIconId::new("main-tray")) {
        if let Ok(guard) = CACHE.lock() {
            if let Some(cached) = guard.as_ref() {
                let _ = tray.set_title(format_tray_title(&cached.info).as_deref());
            }
        }
    }
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
