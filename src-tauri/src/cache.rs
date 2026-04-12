use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::UNIX_EPOCH;

use crate::models::SessionSummary;
use crate::parser;

struct SummaryEntry {
    mtime_ns: u128,
    size: u64,
    summary: SessionSummary,
}

static SUMMARY_CACHE: OnceLock<Mutex<HashMap<PathBuf, SummaryEntry>>> = OnceLock::new();
static PATH_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn summary_cache() -> &'static Mutex<HashMap<PathBuf, SummaryEntry>> {
    SUMMARY_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn path_cache() -> &'static Mutex<HashMap<String, String>> {
    PATH_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 获取会话摘要，优先从缓存读取（mtime + size 校验）
pub fn get_summary(path: &Path) -> Option<SessionSummary> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime_ns = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    let size = meta.len();

    // 缓存命中检查
    if let Ok(cache) = summary_cache().lock() {
        if let Some(entry) = cache.get(path) {
            if entry.mtime_ns == mtime_ns && entry.size == size {
                return Some(entry.summary.clone());
            }
        }
    }

    // 缓存未命中，解析并写入缓存
    let summary = parser::parse_summary(path, 50)?;
    if let Ok(mut cache) = summary_cache().lock() {
        cache.insert(
            path.to_path_buf(),
            SummaryEntry {
                mtime_ns,
                size,
                summary: summary.clone(),
            },
        );
    }
    Some(summary)
}

/// 获取解码后的路径，缓存结果避免重复文件系统检查
pub fn get_decoded_path(encoded: &str, decode_fn: impl FnOnce(&str) -> String) -> String {
    if let Ok(cache) = path_cache().lock() {
        if let Some(decoded) = cache.get(encoded) {
            return decoded.clone();
        }
    }

    let decoded = decode_fn(encoded);
    if let Ok(mut cache) = path_cache().lock() {
        cache.insert(encoded.to_string(), decoded.clone());
    }
    decoded
}
