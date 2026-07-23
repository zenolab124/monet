use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::models::SessionSummary;
use crate::parser;

// v2: CachedContrib 从 tokens 总量改为四类分量（成本分价计算需要），旧缓存整体重扫
const CACHE_VERSION: u32 = 2;

// ── Disk format ──────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct DiskCache {
    version: u32,
    summaries: HashMap<String, DiskSummaryEntry>,
    #[serde(default)]
    usages: HashMap<String, DiskUsageEntry>,
}

#[derive(Serialize, Deserialize)]
struct DiskSummaryEntry {
    mtime_secs: u64,
    mtime_nsec: u32,
    size: u64,
    summary: SessionSummary,
}

#[derive(Serialize, Deserialize)]
struct DiskUsageEntry {
    mtime_secs: u64,
    mtime_nsec: u32,
    size: u64,
    usage: CachedUsage,
}

// ── Public types (usage_stats 消费) ──────────────────────

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CachedUsage {
    pub by_id: Vec<(String, CachedContrib)>,
    pub anon: Vec<CachedContrib>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CachedContrib {
    pub date: String,
    pub model: Option<String>,
    pub usage: crate::models::TokenUsage,
}

// ── In-memory state ──────────────────────────────────────

struct SummaryEntry {
    mtime_secs: u64,
    mtime_nsec: u32,
    size: u64,
    summary: SessionSummary,
}

struct UsageEntry {
    mtime_secs: u64,
    mtime_nsec: u32,
    size: u64,
    usage: CachedUsage,
}

struct CacheState {
    summaries: HashMap<PathBuf, SummaryEntry>,
    usages: HashMap<PathBuf, UsageEntry>,
    paths: HashMap<String, String>,
    dirty: bool,
}

static CACHE: OnceLock<Mutex<CacheState>> = OnceLock::new();

// ── Private helpers ──────────────────────────────────────

fn cache_file_path() -> Option<PathBuf> {
    Some(
        crate::config::claude_root()
            .join("cc-space")
            .join("cache")
            .join("sessions.json"),
    )
}

fn file_stamp(meta: &fs::Metadata) -> (u64, u32, u64) {
    let d = meta
        .modified()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (d.as_secs(), d.subsec_nanos(), meta.len())
}

fn load_from_disk() -> CacheState {
    let (summaries, usages) = cache_file_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|data| serde_json::from_str::<DiskCache>(&data).ok())
        .filter(|dc| dc.version == CACHE_VERSION)
        .map(|dc| {
            let summaries = dc
                .summaries
                .into_iter()
                .map(|(k, v)| {
                    (
                        PathBuf::from(k),
                        SummaryEntry {
                            mtime_secs: v.mtime_secs,
                            mtime_nsec: v.mtime_nsec,
                            size: v.size,
                            summary: v.summary,
                        },
                    )
                })
                .collect();
            let usages = dc
                .usages
                .into_iter()
                .map(|(k, v)| {
                    (
                        PathBuf::from(k),
                        UsageEntry {
                            mtime_secs: v.mtime_secs,
                            mtime_nsec: v.mtime_nsec,
                            size: v.size,
                            usage: v.usage,
                        },
                    )
                })
                .collect();
            (summaries, usages)
        })
        .unwrap_or_default();

    CacheState {
        summaries,
        usages,
        paths: HashMap::new(),
        dirty: false,
    }
}

fn state() -> &'static Mutex<CacheState> {
    CACHE.get_or_init(|| Mutex::new(load_from_disk()))
}

// ── Public API: summary ──────────────────────────────────

pub fn get_summary(path: &Path) -> Option<SessionSummary> {
    let meta = fs::metadata(path).ok()?;
    let (secs, nanos, size) = file_stamp(&meta);

    if let Ok(cache) = state().lock() {
        if let Some(e) = cache.summaries.get(path) {
            if e.mtime_secs == secs && e.mtime_nsec == nanos && e.size == size {
                return Some(e.summary.clone());
            }
        }
    }

    let summary = parser::parse_summary(path, 50)?;
    if let Ok(mut cache) = state().lock() {
        cache.summaries.insert(
            path.to_path_buf(),
            SummaryEntry {
                mtime_secs: secs,
                mtime_nsec: nanos,
                size,
                summary: summary.clone(),
            },
        );
        cache.dirty = true;
    }
    Some(summary)
}

// ── Public API: usage ────────────────────────────────────

pub fn get_usage(path: &Path) -> Option<CachedUsage> {
    let meta = fs::metadata(path).ok()?;
    let (secs, nanos, size) = file_stamp(&meta);

    if let Ok(cache) = state().lock() {
        if let Some(e) = cache.usages.get(path) {
            if e.mtime_secs == secs && e.mtime_nsec == nanos && e.size == size {
                return Some(e.usage.clone());
            }
        }
    }
    None
}

pub fn set_usage(path: &Path, usage: CachedUsage) {
    let Ok(meta) = fs::metadata(path) else { return };
    let (secs, nanos, size) = file_stamp(&meta);

    if let Ok(mut cache) = state().lock() {
        cache.usages.insert(
            path.to_path_buf(),
            UsageEntry {
                mtime_secs: secs,
                mtime_nsec: nanos,
                size,
                usage,
            },
        );
        cache.dirty = true;
    }
}

// ── Public API: flush to disk ────────────────────────────

pub fn flush() {
    let json = {
        let mut cache = match state().lock() {
            Ok(c) => c,
            Err(_) => return,
        };
        if !cache.dirty {
            return;
        }
        cache.dirty = false;

        let disk = DiskCache {
            version: CACHE_VERSION,
            summaries: cache
                .summaries
                .iter()
                .map(|(k, v)| {
                    (
                        k.to_string_lossy().into_owned(),
                        DiskSummaryEntry {
                            mtime_secs: v.mtime_secs,
                            mtime_nsec: v.mtime_nsec,
                            size: v.size,
                            summary: v.summary.clone(),
                        },
                    )
                })
                .collect(),
            usages: cache
                .usages
                .iter()
                .map(|(k, v)| {
                    (
                        k.to_string_lossy().into_owned(),
                        DiskUsageEntry {
                            mtime_secs: v.mtime_secs,
                            mtime_nsec: v.mtime_nsec,
                            size: v.size,
                            usage: v.usage.clone(),
                        },
                    )
                })
                .collect(),
        };

        match serde_json::to_string(&disk) {
            Ok(s) => Some(s),
            Err(_) => {
                cache.dirty = true;
                None
            }
        }
    };

    if let Some(json) = json {
        if let Some(path) = cache_file_path() {
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&path, json);
        }
    }
}

// ── Public API: path decode cache ────────────────────────

pub fn get_decoded_path(encoded: &str, decode_fn: impl FnOnce(&str) -> String) -> String {
    if let Ok(cache) = state().lock() {
        if let Some(decoded) = cache.paths.get(encoded) {
            return decoded.clone();
        }
    }

    let decoded = decode_fn(encoded);
    if let Ok(mut cache) = state().lock() {
        cache.paths.insert(encoded.to_string(), decoded.clone());
    }
    decoded
}
