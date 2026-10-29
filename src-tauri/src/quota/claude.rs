use std::process::{Command, Stdio};

use crate::proc_ext::HideConsole;
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// 错误分类，供 UI 细分文案与行动引导（tray 状态行按此路由）：
    /// token_expired | no_credentials | rate_limited | network | api
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
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
    #[allow(dead_code)] // API 响应字段，反序列化需要
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
    /// 只交给 Claude Code CLI 的 auth login 使用；Monet 不直接调用 token endpoint。
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    scopes: Option<Vec<String>>,
    rate_limit_tier: Option<String>,
    subscription_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Cache (memory + disk)
// ---------------------------------------------------------------------------

// 内存缓存时间戳用墙钟毫秒而非 Instant：一与磁盘缓存的 fetched_at_ms 同源可比
// （peek 取两者新者），二 Instant 在 macOS 睡眠期间暂停，合盖过夜后会把
// 隔夜数据误判为「仍在 TTL 内」
struct CachedQuota {
    info: QuotaInfo,
    fetched_at_ms: i64,
}

static CACHE: Mutex<Option<CachedQuota>> = Mutex::new(None);

// usage API 限流预算有限（曾被 120s 节奏叠加 CLI 打爆，429 窗口 ~15 分钟），
// 菜单栏额度分钟级新鲜度足够，放宽到 5 分钟
const CACHE_TTL: Duration = Duration::from_secs(300);

#[derive(Serialize, Deserialize)]
struct DiskCache {
    info: QuotaInfo,
    fetched_at_ms: i64,
}

pub(super) fn disk_cache_path() -> std::path::PathBuf {
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

// ---------------------------------------------------------------------------
// 会话实时额度：主应用收到 CLI 流内的 rate_limit_event 时写快照。快照 10 分钟内有效，
// 期间自动刷新（周期与活跃两路）不再请求 usage API，直接把快照叠加到上次的缓存上。
// 用户手动刷新不受影响。快照只含窗口类型、利用率与恢复时刻，不含凭据。
// ---------------------------------------------------------------------------

const LIVE_SNAPSHOT_TTL_MS: i64 = 10 * 60 * 1000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveRateLimitSnapshot {
    pub rate_limit_type: Option<String>,
    pub utilization: Option<f64>,
    pub resets_at_ms: Option<i64>,
    pub received_at_ms: i64,
}

pub(super) fn live_snapshot_path() -> std::path::PathBuf {
    crate::config::data_dir().join("quota-live.json")
}

/// 主应用在收到实时额度事件时调用
pub fn record_live_rate_limit(snapshot: &LiveRateLimitSnapshot) {
    if let Ok(json) = serde_json::to_string(snapshot) {
        let _ = crate::config::atomic_write(&live_snapshot_path(), &json);
    }
}

fn fresh_live_snapshot(now_ms: i64) -> Option<LiveRateLimitSnapshot> {
    let content = std::fs::read_to_string(live_snapshot_path()).ok()?;
    let snapshot: LiveRateLimitSnapshot = serde_json::from_str(&content).ok()?;
    (now_ms - snapshot.received_at_ms < LIVE_SNAPSHOT_TTL_MS).then_some(snapshot)
}

/// 把实时快照叠加到已有数据上；快照未覆盖的窗口沿用上次结果
fn overlay_live(
    base: Option<QuotaInfo>,
    live: &LiveRateLimitSnapshot,
    now: &DateTime<Utc>,
) -> QuotaInfo {
    let mut info = base.unwrap_or_else(|| QuotaInfo {
        session: None,
        weekly: None,
        weekly_models: Vec::new(),
        extra_usage: None,
        plan: None,
        account_email: None,
        updated_at: now.to_rfc3339(),
        error: None,
        error_kind: None,
    });
    info.error = None;
    info.error_kind = None;
    let resets_at = live
        .resets_at_ms
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .map(|t| t.to_rfc3339());
    let target = match live.rate_limit_type.as_deref() {
        Some("five_hour") => Some(&mut info.session),
        Some("seven_day") => Some(&mut info.weekly),
        _ => None,
    };
    if let Some(slot) = target {
        // CLI 报 0~1 的比例；兼容直接报百分数
        let percent = live
            .utilization
            .map(|u| if u <= 1.0 { u * 100.0 } else { u })
            .or_else(|| slot.as_ref().map(|w| w.used_percent));
        if let Some(used_percent) = percent {
            let window = make_window(used_percent, resets_at.as_deref(), now);
            *slot = Some(window);
        }
    }
    info.updated_at = now.to_rfc3339();
    info
}

/// 有新鲜实时快照时直接采用并写入缓存；返回 None 表示需要走正常刷新
fn apply_live_snapshot(now: DateTime<Utc>) -> Option<QuotaInfo> {
    let live = fresh_live_snapshot(now.timestamp_millis())?;
    let base = read_disk_cache().map(|(info, _)| info).or_else(|| {
        CACHE
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|c| c.info.clone()))
    });
    let info = overlay_live(base, &live, &now);
    write_disk_cache(&info);
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(CachedQuota {
            info: info.clone(),
            fetched_at_ms: now.timestamp_millis(),
        });
    }
    Some(info)
}

fn disk_cache_age_secs() -> Option<i64> {
    let content = std::fs::read_to_string(disk_cache_path()).ok()?;
    let dc: DiskCache = serde_json::from_str(&content).ok()?;
    let age = Utc::now().timestamp_millis() - dc.fetched_at_ms;
    Some(age / 1000)
}

// ---------------------------------------------------------------------------
// 429 退避：usage API 的限流窗口约 15 分钟（Retry-After），冷却期内任何请求都会
// 续期惩罚窗口。曾因 tray 固定 120s 盲目重试导致限流永不恢复，故退避状态必须
// 落盘、tray 与主应用双进程共享，冷却期内一律不打 API（含手动刷新）。
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct BackoffState {
    until_ms: i64,
}

pub(super) fn backoff_path() -> std::path::PathBuf {
    crate::config::data_dir().join("quota-backoff.json")
}

fn write_backoff(secs: i64) {
    let state = BackoffState {
        until_ms: Utc::now().timestamp_millis() + secs * 1000,
    };
    if let Ok(json) = serde_json::to_string(&state) {
        let _ = crate::config::atomic_write(&backoff_path(), &json);
    }
}

fn clear_backoff() {
    let _ = std::fs::remove_file(backoff_path());
}

// CLI 登录失败时避免 tray 与主应用每轮都尝试兑换同一 refresh token。
const AUTH_RETRY_SECS: i64 = 600;

fn auth_backoff_path() -> std::path::PathBuf {
    crate::config::data_dir().join("quota-auth-backoff.json")
}

fn auth_backoff_active() -> bool {
    let Ok(content) = std::fs::read_to_string(auth_backoff_path()) else {
        return false;
    };
    serde_json::from_str::<BackoffState>(&content)
        .is_ok_and(|state| state.until_ms > Utc::now().timestamp_millis())
}

fn write_auth_backoff() {
    let state = BackoffState {
        until_ms: Utc::now().timestamp_millis() + AUTH_RETRY_SECS * 1000,
    };
    if let Ok(json) = serde_json::to_string(&state) {
        let _ = crate::config::atomic_write(&auth_backoff_path(), &json);
    }
}

fn clear_auth_backoff() {
    let _ = std::fs::remove_file(auth_backoff_path());
}

/// 限流冷却剩余秒数；不在冷却期返回 None。tray 用它渲染「限流中」提示行。
pub fn backoff_remaining_secs() -> Option<i64> {
    let content = std::fs::read_to_string(backoff_path()).ok()?;
    let state: BackoffState = serde_json::from_str(&content).ok()?;
    let remain = (state.until_ms - Utc::now().timestamp_millis()) / 1000;
    (remain > 0).then_some(remain)
}

/// 从 RFC3339 重置时刻现算剩余秒数。菜单每次重建时调用，
/// 替代缓存里 fetch 时刻算死的 resets_in_secs（会随缓存年龄漂移）。
pub fn secs_until(resets_at: Option<&str>) -> Option<i64> {
    let dt = DateTime::parse_from_rfc3339(resets_at?).ok()?;
    Some((dt.with_timezone(&Utc) - Utc::now()).num_seconds().max(0))
}

// ---------------------------------------------------------------------------
// IPC commands
// ---------------------------------------------------------------------------

pub fn get_quota() -> QuotaInfo {
    let now_ms = Utc::now().timestamp_millis();
    if let Ok(guard) = CACHE.lock() {
        if let Some(cached) = guard.as_ref() {
            if now_ms - cached.fetched_at_ms < CACHE_TTL.as_millis() as i64 {
                return cached.info.clone();
            }
        }
    }
    fetch_and_cache(false)
}

/// 手动刷新：跳过内存缓存与磁盘 TTL，必要时委托 CLI 自动续期后查询 API。
/// 同时被 monet-tray 独立进程调用。
pub fn refresh_quota() -> QuotaInfo {
    fetch_and_cache(true)
}

// ---------------------------------------------------------------------------
// 活跃节流刷新（会话事件驱动）
// ---------------------------------------------------------------------------

/// 活跃刷新的最小间隔：低于周期刷新的 300s，把活跃期数据延迟从「最多 5 分钟」
/// 压到 turn 落盘后 ~90s 内；高于 60s 保住 usage API 的限流预算
const ACTIVITY_MIN_INTERVAL_SECS: i64 = 90;

static ACTIVITY_INFLIGHT: AtomicBool = AtomicBool::new(false);
static LAST_ACTIVITY_MS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// 会话活跃信号（watcher 发射 projects-changed 时调用）：缓存年龄超过最小间隔
/// 才真正打 API，后台线程执行不阻塞调用方。
pub fn notify_session_activity<F>(refresh: F)
where
    F: FnOnce(i64) + Send + 'static,
{
    // 进程内粗闸：watcher 发射至多每秒一次，90s 内直接返回免得反复起线程；
    // 精确判断与跨进程 single-flight 由回调在后台线程内完成。
    let now_ms = Utc::now().timestamp_millis();
    if now_ms - LAST_ACTIVITY_MS.load(Ordering::Relaxed) < ACTIVITY_MIN_INTERVAL_SECS * 1000 {
        return;
    }
    if ACTIVITY_INFLIGHT.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        refresh(now_ms);
        ACTIVITY_INFLIGHT.store(false, Ordering::SeqCst);
    });
}

pub(super) fn refresh_for_activity(now_ms: i64) {
    // 必须在统一 Provider 锁内重读共享缓存时间；另一进程可能在等待锁期间刚完成刷新。
    let latest = latest_fetched_at_ms().unwrap_or(0);
    if now_ms - latest >= ACTIVITY_MIN_INTERVAL_SECS * 1000
        && apply_live_snapshot(Utc::now()).is_some()
    {
        LAST_ACTIVITY_MS.store(Utc::now().timestamp_millis(), Ordering::Relaxed);
        return;
    }
    if now_ms - latest >= ACTIVITY_MIN_INTERVAL_SECS * 1000
        && backoff_remaining_secs().is_none()
        && quota_available()
    {
        // force=true 绕过磁盘 TTL（年龄闸门已由上方 90s 判断承担）。
        // 失败也推进粗闸：90s 内不重试，避免失败风暴
        let _ = fetch_and_cache(true);
        LAST_ACTIVITY_MS.store(Utc::now().timestamp_millis(), Ordering::Relaxed);
    } else {
        // 数据已新鲜（周期刷新或另一进程刚刷过）：粗闸对齐到该时间戳，
        // 下次需要检查的时刻自然顺延
        LAST_ACTIVITY_MS.store(latest, Ordering::Relaxed);
    }
}

/// 内存与磁盘缓存中较新者的抓取时间戳
fn latest_fetched_at_ms() -> Option<i64> {
    let mem = CACHE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|c| c.fetched_at_ms));
    let disk = read_disk_cache().map(|(_, t)| t);
    match (mem, disk) {
        (Some(m), Some(d)) => Some(m.max(d)),
        (m, d) => m.or(d),
    }
}

/// 只读取现有缓存，绝不发起网络请求。内存与磁盘按时间戳取新者——
/// 双进程各持独立内存缓存，磁盘可能被另一进程刚刷新过，
/// 无条件内存优先会让 tray 的磁盘 mtime 侦测永远采不到主应用的新数据。
/// 供 monet-tray 主线程即时重渲染（tray-title / quota-cache 变更）使用。
pub fn peek_cached_quota() -> Option<QuotaInfo> {
    let mem = CACHE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().map(|c| (c.info.clone(), c.fetched_at_ms)));
    let disk = read_disk_cache();
    match (mem, disk) {
        (Some((mi, mt)), Some((di, dt))) => Some(if dt > mt { di } else { mi }),
        (Some((mi, _)), None) => Some(mi),
        (None, Some((di, _))) => Some(di),
        (None, None) => None,
    }
}

pub fn quota_available() -> bool {
    read_credential().is_some()
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

fn fetch_and_cache(force: bool) -> QuotaInfo {
    // 限流冷却期内一律不打 API——手动刷新也不例外：打了必 429 且会续期惩罚窗口。
    // 诚实返回旧数据 + 错误标注，让 UI 告知用户「限流中，X 分钟后自动恢复」。
    if let Some(remain) = backoff_remaining_secs() {
        let mins = (remain + 59) / 60;
        return stale_with_error(format!("Rate limited, retry in {mins}m"), "rate_limited");
    }

    // 会话正在报实时额度：自动刷新直接采用，不请求 usage API
    if !force {
        if let Some(info) = apply_live_snapshot(Utc::now()) {
            return info;
        }
    }

    // 磁盘缓存 TTL 内直接采用（另一进程可能刚刷过，双进程共享省 API 调用）；
    // force（用户手动刷新）跳过，保证「点了刷新就真的刷新」
    if !force {
        if let Some(age) = disk_cache_age_secs() {
            if age < CACHE_TTL.as_secs() as i64 {
                if let Some((info, fetched_at_ms)) = read_disk_cache() {
                    if info.error.is_none() {
                        if let Ok(mut guard) = CACHE.lock() {
                            *guard = Some(CachedQuota {
                                info: info.clone(),
                                fetched_at_ms,
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
        Err((msg, kind)) => return stale_with_error(msg, kind),
    };
    if info.error.is_none() {
        write_disk_cache(&info);
    }
    if let Ok(mut guard) = CACHE.lock() {
        *guard = Some(CachedQuota {
            info: info.clone(),
            fetched_at_ms: Utc::now().timestamp_millis(),
        });
    }
    info
}

/// 刷新失败时的诚实回退：返回旧缓存数据（updated_at 保持旧值）+ error 标注，
/// 让 UI 能同时展示「上次成功的数据」和「本次为什么失败」。不写盘、不进内存缓存
/// （error 结果缓存后会在 TTL 内遮蔽恢复）。
fn stale_with_error(err: String, kind: &str) -> QuotaInfo {
    if let Some((mut cached, _)) = read_disk_cache() {
        if cached.error.is_none() {
            cached.error = Some(err);
            cached.error_kind = Some(kind.to_string());
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
        error: Some(err),
        error_kind: Some(kind.to_string()),
    }
}

fn fetch_quota_inner() -> Result<QuotaInfo, (String, &'static str)> {
    let (access_token, credential) = get_valid_token()?;

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| (format!("HTTP client error: {e}"), "network"))?;

    let resp = client
        .get("https://api.anthropic.com/api/oauth/usage")
        .header("Authorization", format!("Bearer {access_token}"))
        .header("anthropic-beta", "oauth-2025-04-20")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("User-Agent", "monet/1.0")
        .send()
        .map_err(|e| (format!("Network error: {e}"), "network"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        if status.as_u16() == 429 {
            // 无 Retry-After 时按实测窗口 ~15 分钟兜底；钳位防服务端异常值
            let retry_secs = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.trim().parse::<i64>().ok())
                .unwrap_or(900)
                .clamp(60, 3600);
            write_backoff(retry_secs);
            let mins = (retry_secs + 59) / 60;
            return Err((format!("Rate limited, retry in {mins}m"), "rate_limited"));
        }
        let body = resp.text().unwrap_or_default();
        return Err((format!("API error {status}: {body}"), "api"));
    }

    let api: ApiUsageResponse = resp
        .json()
        .map_err(|e| (format!("Parse error: {e}"), "api"))?;
    clear_backoff();
    let now = Utc::now();

    let session = api.five_hour.as_ref().and_then(|w| {
        w.utilization
            .map(|u| make_window(u, w.resets_at.as_deref(), &now))
    });

    let weekly = select_primary_weekly(&api).map(|w| make_window(w.0, w.1, &now));

    let weekly_models = build_model_quotas(&api);

    let extra_usage = api.extra_usage.as_ref().and_then(|e| {
        if e.is_enabled != Some(true) {
            return None;
        }
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
        error_kind: None,
    })
}

fn make_window(utilization: f64, resets_at: Option<&str>, now: &DateTime<Utc>) -> QuotaWindow {
    let resets_in = resets_at.and_then(|s| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| (dt.with_timezone(&Utc) - *now).num_seconds().max(0))
    });
    QuotaWindow {
        used_percent: utilization,
        resets_at: resets_at.map(|s| s.to_string()),
        resets_in_secs: resets_in,
    }
}

fn select_primary_weekly(api: &ApiUsageResponse) -> Option<(f64, Option<&str>)> {
    for win in [
        &api.seven_day,
        &api.seven_day_oauth_apps,
        &api.seven_day_sonnet,
        &api.seven_day_opus,
    ]
    .into_iter()
    .flatten()
    {
        if let Some(u) = win.utilization {
            return Some((u, win.resets_at.as_deref()));
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
    if combined.is_empty() {
        return None;
    }

    if combined.contains("max") {
        if combined.contains("20x") {
            return Some("Max 20x".into());
        }
        if combined.contains("5x") {
            return Some("Max 5x".into());
        }
        return Some("Max".into());
    }
    if combined.contains("ultra") {
        return Some("Ultra".into());
    }
    if combined.contains("team") {
        return Some("Team".into());
    }
    if combined.contains("enterprise") {
        return Some("Enterprise".into());
    }
    if combined.contains("pro") {
        return Some("Pro".into());
    }
    None
}

// ---------------------------------------------------------------------------
// Token management
// ---------------------------------------------------------------------------

/// 获取可用的 access token。过期时把续期交给官方 CLI，Monet 始终不写 CLI 凭据。
fn get_valid_token() -> Result<(String, OAuthCredential), (String, &'static str)> {
    let mut cred =
        read_credential().ok_or(("No Claude credentials found".to_string(), "no_credentials"))?;
    if token_expiring(&cred) {
        cred = renew_with_cli(cred);
    }
    if token_expiring(&cred) {
        return Err((
            "Claude Code credentials expired; automatic renewal failed".into(),
            "token_expired",
        ));
    }
    let has_profile = cred
        .scopes
        .as_ref()
        .is_some_and(|s| s.iter().any(|sc| sc == "user:profile"));
    if !has_profile {
        return Err((
            "Credentials lack user:profile scope".into(),
            "no_credentials",
        ));
    }
    clear_auth_backoff();
    let token = cred
        .access_token
        .clone()
        .ok_or(("No access token".to_string(), "no_credentials"))?;
    Ok((token, cred))
}

fn token_expiring(cred: &OAuthCredential) -> bool {
    let now_ms = Utc::now().timestamp_millis() as u64;
    cred.expires_at
        .is_some_and(|exp| now_ms >= exp.saturating_sub(60_000))
}

/// 官方 CLI 是 refresh token 的唯一写入者。即使它返回非零退出码，也以重读后的
/// 凭据为准：某些 CLI 路径在完成 Keychain 续期后仍会报告命令失败。
fn renew_with_cli(cred: OAuthCredential) -> OAuthCredential {
    let latest = read_credential().unwrap_or(cred);
    if !token_expiring(&latest) {
        clear_auth_backoff();
        return latest;
    }
    if auth_backoff_active() {
        return latest;
    }
    let Some(refresh_token) = latest
        .refresh_token
        .as_deref()
        .filter(|token| !token.is_empty())
    else {
        return latest;
    };
    let Some(scopes) = latest.scopes.as_ref().filter(|scopes| !scopes.is_empty()) else {
        return latest;
    };
    let Ok(located) = crate::claude_locator::locate_lightweight() else {
        write_auth_backoff();
        return latest;
    };

    let mut cmd = Command::new(&located.path);
    cmd.args(["auth", "login", "--claudeai"])
        .env("PATH", crate::streaming::enhanced_path())
        .env("CLAUDE_CODE_OAUTH_REFRESH_TOKEN", refresh_token)
        .env("CLAUDE_CODE_OAUTH_SCOPES", scopes.join(" "));
    for key in [
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_BASE_URL",
        "CLAUDE_CODE_OAUTH_TOKEN",
    ] {
        cmd.env_remove(key);
    }
    if let Some((key, value)) = crate::config::claude_config_dir_env() {
        cmd.env(key, value);
    }
    // 不记录 CLI 输出，也不把 refresh token 放入 argv 或磁盘。
    let _ = output_with_timeout(cmd, Duration::from_secs(20));
    let renewed = read_credential().unwrap_or(latest);
    if token_expiring(&renewed) {
        write_auth_backoff();
    } else {
        clear_auth_backoff();
    }
    renewed
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
    // 3s 超时：macOS 26 有 security 挂起的社区报告；hang 会让 tray 的
    // fetching 标志永久卡死、周期刷新静默停摆，比读取失败更隐蔽
    let mut cmd = Command::new("security");
    cmd.args([
        "find-generic-password",
        "-s",
        "Claude Code-credentials",
        "-w",
    ]);
    let output = output_with_timeout(cmd, Duration::from_secs(3))?;
    if !output.status.success() {
        return None;
    }
    let json_str = String::from_utf8(output.stdout).ok()?;
    let creds: CredentialsFile = serde_json::from_str(json_str.trim()).ok()?;
    creds.claude_ai_oauth
}

/// spawn 子进程并限时收集输出；超时 kill 并返回 None。
/// 轮询 try_wait 而非阻塞 wait：std 无内置超时，且子进程输出量远小于管道缓冲。
fn output_with_timeout(mut cmd: Command, timeout: Duration) -> Option<std::process::Output> {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .hide_console()
        .spawn()
        .ok()?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().ok(),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

fn read_file_credential() -> Option<OAuthCredential> {
    let path = crate::config::claude_root().join(".credentials.json");
    let content = std::fs::read_to_string(path).ok()?;
    let creds: CredentialsFile = serde_json::from_str(&content).ok()?;
    creds.claude_ai_oauth
}

// ---------------------------------------------------------------------------
// Tray tooltip (已用语义)
// ---------------------------------------------------------------------------

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

#[cfg(test)]
mod live_snapshot_tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp_millis(1_761_750_000_000).unwrap()
    }

    #[test]
    fn overlay_updates_matching_window_and_keeps_others() {
        let base = overlay_live(
            None,
            &LiveRateLimitSnapshot {
                rate_limit_type: Some("seven_day".into()),
                utilization: Some(40.0),
                resets_at_ms: None,
                received_at_ms: 0,
            },
            &now(),
        );
        let merged = overlay_live(
            Some(base),
            &LiveRateLimitSnapshot {
                rate_limit_type: Some("five_hour".into()),
                utilization: Some(0.93),
                resets_at_ms: Some(1_761_753_600_000),
                received_at_ms: 0,
            },
            &now(),
        );
        let session = merged.session.unwrap();
        assert!((session.used_percent - 93.0).abs() < 1e-6);
        assert!(session.resets_at.is_some());
        assert!((merged.weekly.unwrap().used_percent - 40.0).abs() < 1e-6);
        assert!(merged.error.is_none());
    }

    #[test]
    fn overlay_ignores_unknown_windows_and_missing_utilization() {
        let merged = overlay_live(
            None,
            &LiveRateLimitSnapshot {
                rate_limit_type: Some("overage".into()),
                utilization: Some(0.5),
                resets_at_ms: None,
                received_at_ms: 0,
            },
            &now(),
        );
        assert!(merged.session.is_none() && merged.weekly.is_none());
        let merged = overlay_live(
            None,
            &LiveRateLimitSnapshot {
                rate_limit_type: Some("five_hour".into()),
                utilization: None,
                resets_at_ms: None,
                received_at_ms: 0,
            },
            &now(),
        );
        assert!(merged.session.is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 挂起的子进程必须在超时后被 kill 返回，而非无限阻塞——
    /// 这是 security hang 导致 tray 刷新静默停摆的防线
    #[test]
    fn output_with_timeout_kills_hung_process() {
        let mut cmd = Command::new("/bin/sleep");
        cmd.arg("10");
        let start = Instant::now();
        assert!(output_with_timeout(cmd, Duration::from_millis(200)).is_none());
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn output_with_timeout_collects_stdout() {
        let mut cmd = Command::new("/bin/echo");
        cmd.arg("hello");
        let out = output_with_timeout(cmd, Duration::from_secs(3)).expect("echo should succeed");
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "hello");
    }
}
