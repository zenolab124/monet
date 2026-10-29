//! Claude 协议诊断数据：未知协议面采样与 CLI 自报模型缓存。
//!
//! 两者都只落 Monet 数据目录，不碰会话文件。采样只记结构（键名与值类型），
//! 写盘做 5 秒合并，单个会话的高频事件不会逐条写。

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::protocol::{shape_of, ReportedModel, VERIFIED_CLI_VERSION};
use crate::engines::core::EngineInstanceId;

const SAMPLE_LIMIT: usize = 500;
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolSample {
    pub cli_version: String,
    pub kind: String,
    pub subtype: Option<String>,
    pub keys: BTreeMap<String, String>,
    pub first_seen: i64,
    pub count: u64,
}

#[derive(Default)]
struct SampleStore {
    loaded: bool,
    dirty: bool,
    cli_version: Option<String>,
    samples: Vec<ProtocolSample>,
}

static STORE: Mutex<SampleStore> = Mutex::new(SampleStore {
    loaded: false,
    dirty: false,
    cli_version: None,
    samples: Vec::new(),
});
static FLUSHER: OnceLock<()> = OnceLock::new();

fn samples_path() -> PathBuf {
    crate::config::data_dir()
        .join("diagnostics")
        .join("claude-protocol-samples.json")
}

fn models_path(instance: &EngineInstanceId) -> PathBuf {
    crate::config::data_dir()
        .join("cache")
        .join(models_file_name(instance))
}

/// 本机实例沿用历史文件名；其他实例按实例 key（base64url，文件名安全）分文件。
fn models_file_name(instance: &EngineInstanceId) -> String {
    if instance.engine_id() == super::ENGINE_ID && instance.instance_id() == super::DEFAULT_INSTANCE_ID {
        "claude-models.json".into()
    } else {
        format!("claude-models.{}.json", instance.storage_key())
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn ensure_loaded(store: &mut SampleStore) {
    if store.loaded {
        return;
    }
    store.loaded = true;
    // 文件损坏时丢弃旧采样，从空开始
    store.samples = std::fs::read_to_string(samples_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
}

fn ensure_flusher() {
    FLUSHER.get_or_init(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(FLUSH_INTERVAL);
            flush();
        });
    });
}

fn flush() {
    let snapshot = {
        let mut store = STORE.lock().unwrap();
        if !store.dirty {
            return;
        }
        store.dirty = false;
        store.samples.clone()
    };
    let Ok(text) = serde_json::to_string_pretty(&snapshot) else {
        return;
    };
    if let Err(error) = crate::config::atomic_write(&samples_path(), &text) {
        eprintln!("[claude-protocol] 采样写盘失败，已丢弃本次: {error}");
    }
}

/// 记录本机 CLI 版本（来自 system/init）。
pub fn note_cli_version(version: &str) {
    let mut store = STORE.lock().unwrap();
    store.cli_version = Some(version.to_string());
}

pub fn cli_version() -> Option<String> {
    STORE.lock().unwrap().cli_version.clone()
}

/// 记录一条 Monet 未处理的协议消息的结构。
pub fn record_unknown(value: &Value) {
    let Some(shape) = shape_of(value) else {
        return;
    };
    let mut store = STORE.lock().unwrap();
    ensure_loaded(&mut store);
    let version = store
        .cli_version
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    if let Some(existing) = store.samples.iter_mut().find(|sample| {
        sample.cli_version == version
            && sample.kind == shape.kind
            && sample.subtype == shape.subtype
    }) {
        existing.count += 1;
        // 同一种消息新出现的键并入结构
        for (key, kind) in shape.keys {
            existing.keys.entry(key).or_insert(kind);
        }
    } else {
        if store.samples.len() >= SAMPLE_LIMIT {
            if let Some(oldest) = store
                .samples
                .iter()
                .enumerate()
                .min_by_key(|(_, sample)| sample.first_seen)
                .map(|(index, _)| index)
            {
                store.samples.remove(oldest);
            }
        }
        store.samples.push(ProtocolSample {
            cli_version: version,
            kind: shape.kind,
            subtype: shape.subtype,
            keys: shape.keys,
            first_seen: now_ms(),
            count: 1,
        });
    }
    store.dirty = true;
    drop(store);
    ensure_flusher();
}

#[derive(Debug, Clone, Serialize)]
pub struct ProtocolDiagnostics {
    pub cli_version: Option<String>,
    pub verified_version: &'static str,
    pub samples: Vec<ProtocolSample>,
}

pub fn protocol_diagnostics() -> ProtocolDiagnostics {
    let mut store = STORE.lock().unwrap();
    ensure_loaded(&mut store);
    let mut samples = store.samples.clone();
    samples.sort_by_key(|sample| std::cmp::Reverse(sample.first_seen));
    ProtocolDiagnostics {
        cli_version: store.cli_version.clone(),
        verified_version: VERIFIED_CLI_VERSION,
        samples,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelCache {
    pub cli_version: Option<String>,
    pub models: Vec<CachedModel>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CachedModel {
    pub value: String,
    pub display_name: String,
    pub description: Option<String>,
}

/// 握手成功报出模型列表时覆盖该实例的缓存；空列表不覆盖。
pub fn store_models(instance: &EngineInstanceId, models: &[ReportedModel]) {
    if models.is_empty() {
        return;
    }
    let cache = ModelCache {
        cli_version: cli_version(),
        models: models
            .iter()
            .map(|model| CachedModel {
                value: model.value.clone(),
                display_name: model.display_name.clone(),
                description: model.description.clone(),
            })
            .collect(),
        updated_at: now_ms(),
    };
    if let Ok(text) = serde_json::to_string_pretty(&cache) {
        if let Err(error) = crate::config::atomic_write(&models_path(instance), &text) {
            eprintln!("[claude-protocol] 模型缓存写入失败: {error}");
        }
    }
}

pub fn cached_models(instance: &EngineInstanceId) -> Option<ModelCache> {
    std::fs::read_to_string(models_path(instance))
        .ok()
        .and_then(|text| serde_json::from_str::<ModelCache>(&text).ok())
        .filter(|cache| !cache.models.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_instance_keeps_legacy_model_cache_file() {
        let local = super::super::default_instance().unwrap();
        assert_eq!(models_file_name(&local), "claude-models.json");
    }

    #[test]
    fn other_instances_get_separate_file_safe_model_caches() {
        let second = EngineInstanceId::new("claude-code", "second-a").unwrap();
        let other = EngineInstanceId::new("claude-code", "second-b").unwrap();
        let name = models_file_name(&second);

        assert_eq!(name, format!("claude-models.{}.json", second.storage_key()));
        assert_ne!(name, models_file_name(&other));
        assert!(!name.contains('/') && !name.contains('\\'));
    }
}
