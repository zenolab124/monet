//! 模型价目与成本计价 —— Widget 费用估算的数据源
//!
//! 价目来自 models.dev（provider 白名单精简），三级加载：
//! 磁盘缓存（24h 新鲜）→ 远程拉取（成功即覆写缓存）→ 过期缓存 → 内置快照。
//! 内置快照由 `scripts/update-pricing-snapshot.mjs` 生成，保证离线/弱网首启不至于全零。
//!
//! 计价口径：input/output/cache_write/cache_read 四类 token 分价（$/MTok），
//! 未知模型不套兜底价——宁可标记未计价，不产出错误数字。

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::models::TokenUsage;

/// 单模型四类单价，单位 $/百万 token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    pub cache_write: f64,
    pub cache_read: f64,
}

/// 缓存文件与内置快照的共同存储格式
#[derive(Serialize, Deserialize)]
struct StoredTable {
    fetched_at: u64,
    source: String,
    /// key 为全小写模型 id
    models: HashMap<String, ModelCost>,
}

/// 价目表的实际来源，供调用方记录诊断信息
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PricingSource {
    FreshCache,
    Remote,
    StaleCache,
    Snapshot,
}

pub struct PricingTable {
    models: HashMap<String, ModelCost>,
    pub source: PricingSource,
}

const MODELS_DEV_URL: &str = "https://models.dev/api.json";
const CACHE_TTL_SECS: u64 = 24 * 60 * 60;
const FETCH_TIMEOUT_SECS: u64 = 20;
const SNAPSHOT_JSON: &str = include_str!("pricing-snapshot.json");

/// 仅收官方 provider（含常见第三方模型厂商的官方价），顺序即同名冲突时的优先级；
/// 聚合网关的镜像价一律不收。与 scripts/update-pricing-snapshot.mjs 保持一致。
const PROVIDER_ALLOWLIST: [&str; 13] = [
    "anthropic",
    "openai",
    "google",
    "zhipuai",
    "zai",
    "moonshotai",
    "moonshotai-cn",
    "deepseek",
    "minimax",
    "minimax-cn",
    "alibaba",
    "alibaba-cn",
    "xai",
];

fn cache_path() -> PathBuf {
    crate::config::data_dir().join("pricing-cache.json")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_cache() -> Option<StoredTable> {
    let text = std::fs::read_to_string(cache_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn load_snapshot() -> HashMap<String, ModelCost> {
    serde_json::from_str::<StoredTable>(SNAPSHOT_JSON)
        .map(|t| t.models)
        .unwrap_or_default()
}

/// 把 models.dev 全量 api.json 按白名单精简为价目表。
/// 缺缓存单价的条目按 Anthropic 比例兜底（写 1.25x / 读 0.1x），
/// 显式标 0 的（如缓存写免费的厂商）保留原值。
fn parse_models_dev(api: &serde_json::Value) -> HashMap<String, ModelCost> {
    let mut out: HashMap<String, ModelCost> = HashMap::new();
    for provider in PROVIDER_ALLOWLIST {
        let Some(models) = api.get(provider).and_then(|p| p.get("models")).and_then(|m| m.as_object())
        else {
            continue;
        };
        for (id, m) in models {
            let Some(cost) = m.get("cost") else { continue };
            let (Some(input), Some(output)) = (
                cost.get("input").and_then(|v| v.as_f64()),
                cost.get("output").and_then(|v| v.as_f64()),
            ) else {
                continue;
            };
            let key = id.to_lowercase();
            if out.contains_key(&key) {
                continue;
            }
            out.insert(
                key,
                ModelCost {
                    input,
                    output,
                    cache_write: cost
                        .get("cache_write")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(input * 1.25),
                    cache_read: cost
                        .get("cache_read")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(input * 0.1),
                },
            );
        }
    }
    out
}

fn fetch_remote() -> Result<HashMap<String, ModelCost>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SECS))
        .build()
        .map_err(|e| e.to_string())?;
    let api: serde_json::Value = client
        .get(MODELS_DEV_URL)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let models = parse_models_dev(&api);
    // 上游部分故障（某 provider 键临时缺失）会产出残缺但非空的表，
    // 覆写缓存会让被挤掉的模型 unpriced 一整天——低于合理下限即拒收
    if models.len() < 50 {
        return Err(format!("解析仅得 {} 条，疑似上游残缺，拒收", models.len()));
    }
    Ok(models)
}

fn write_cache(models: &HashMap<String, ModelCost>) {
    let stored = StoredTable {
        fetched_at: now_secs(),
        source: MODELS_DEV_URL.into(),
        models: models.clone(),
    };
    if let Ok(text) = serde_json::to_string(&stored) {
        let path = cache_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, text);
    }
}

/// 三级加载。同步阻塞（含最多 20s 网络等待），适合短命后台进程调用；
/// 只有远程成功才回写缓存，快照/过期缓存不回写，避免污染新鲜度判定。
pub fn load() -> PricingTable {
    let cached = read_cache();
    if let Some(c) = &cached {
        let now = now_secs();
        // fetched_at 在未来（时钟回拨）视为过期，否则缓存会在真实时间追上前永久「新鲜」
        if c.fetched_at <= now && now - c.fetched_at < CACHE_TTL_SECS && !c.models.is_empty() {
            return PricingTable {
                models: cached.unwrap().models,
                source: PricingSource::FreshCache,
            };
        }
    }
    match fetch_remote() {
        Ok(models) => {
            write_cache(&models);
            PricingTable {
                models,
                source: PricingSource::Remote,
            }
        }
        Err(_) => {
            if let Some(c) = cached {
                if !c.models.is_empty() {
                    return PricingTable {
                        models: c.models,
                        source: PricingSource::StaleCache,
                    };
                }
            }
            PricingTable {
                models: load_snapshot(),
                source: PricingSource::Snapshot,
            }
        }
    }
}

/// 仅用内置快照构表（测试与离线场景）
pub fn from_snapshot() -> PricingTable {
    PricingTable {
        models: load_snapshot(),
        source: PricingSource::Snapshot,
    }
}

/// 尾部 -YYYYMMDD 日期后缀剥离（claude-sonnet-4-5-20250929 → claude-sonnet-4-5）
fn strip_trailing_date(s: &str) -> Option<&str> {
    let (head, tail) = s.rsplit_once('-')?;
    (tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit())).then_some(head)
}

/// 尾部版本段 dot↔dash 互换变体（glm-4-6 ↔ glm-4.6），产出与原串不同才返回
fn version_tail_variant(s: &str) -> Option<String> {
    if let Some(idx) = s.rfind('.') {
        let (head, tail) = s.split_at(idx);
        let tail = &tail[1..];
        if !tail.is_empty()
            && tail.bytes().all(|b| b.is_ascii_digit())
            && head.bytes().last().is_some_and(|b| b.is_ascii_digit())
        {
            return Some(format!("{head}-{tail}"));
        }
    }
    if let Some((head, tail)) = s.rsplit_once('-') {
        if !tail.is_empty()
            && tail.bytes().all(|b| b.is_ascii_digit())
            && head.bytes().last().is_some_and(|b| b.is_ascii_digit())
        {
            return Some(format!("{head}.{tail}"));
        }
    }
    None
}

/// 前缀断点后的首段是 1-2 位纯数字 → 大概率是版本升级段而非型号后缀
fn looks_like_version_bump(rest: &str) -> bool {
    let seg = rest.split('-').next().unwrap_or(rest);
    !seg.is_empty() && seg.len() <= 2 && seg.bytes().all(|b| b.is_ascii_digit())
}

impl PricingTable {
    /// 模型名 → 单价。匹配链：小写化（剥 [1m] 类后缀）→ 精确 → 剥日期
    /// → 剥 provider/ 前缀 → 版本段 dot↔dash 变体 → 前缀最长匹配。
    /// 全部失手返回 None，由调用方按未计价处理。
    /// 已知简化：[1m] 长上下文档 >200k 部分的 input 溢价（2x）计不到——
    /// JSONL 无 token 位置信息，本就无法精确，一律按标准价。
    pub fn lookup(&self, raw_model: &str) -> Option<&ModelCost> {
        let base = raw_model
            .split('[')
            .next()
            .unwrap_or(raw_model)
            .trim()
            .to_lowercase();
        if base.is_empty() {
            return None;
        }

        let mut candidates: Vec<String> = Vec::with_capacity(8);
        let push = |s: String, list: &mut Vec<String>| {
            if !list.contains(&s) {
                list.push(s);
            }
        };
        push(base.clone(), &mut candidates);
        if let Some(s) = strip_trailing_date(&base) {
            push(s.to_string(), &mut candidates);
        }
        if let Some((_, rest)) = base.split_once('/') {
            push(rest.to_string(), &mut candidates);
            if let Some(s) = strip_trailing_date(rest) {
                push(s.to_string(), &mut candidates);
            }
        }
        for i in 0..candidates.len() {
            if let Some(v) = version_tail_variant(&candidates[i]) {
                push(v, &mut candidates);
            }
        }

        for c in &candidates {
            if let Some(hit) = self.models.get(c) {
                return Some(hit);
            }
        }

        // 前缀最长匹配：未知细分型号塌到已知系列（gpt-5-nano → gpt-5），取最长 key。
        // 断点后首段若是 1-2 位纯数字则视为版本升级（gpt-5-7 之于 gpt-5）拒绝塌落——
        // 新版本可能调价，宁可 unpriced 也不套旧版价；更长的数字段（如 -2027- 日期）不受限
        let mut best: Option<(&String, &ModelCost)> = None;
        for (key, cost) in &self.models {
            for c in &candidates {
                if c.len() > key.len()
                    && c.starts_with(key.as_str())
                    && c.as_bytes()[key.len()] == b'-'
                    && !looks_like_version_bump(&c[key.len() + 1..])
                    && best.map_or(true, |(bk, _)| key.len() > bk.len())
                {
                    best = Some((key, cost));
                }
            }
        }
        best.map(|(_, cost)| cost)
    }

    /// 四类 token 分价计价，单价表为 $/MTok
    pub fn cost_usd(&self, model: &str, usage: &TokenUsage) -> Option<f64> {
        let c = self.lookup(model)?;
        Some(
            (usage.input_tokens as f64 * c.input
                + usage.output_tokens as f64 * c.output
                + usage.cache_creation_input_tokens as f64 * c.cache_write
                + usage.cache_read_input_tokens as f64 * c.cache_read)
                / 1_000_000.0,
        )
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(entries: &[(&str, ModelCost)]) -> PricingTable {
        PricingTable {
            models: entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            source: PricingSource::Snapshot,
        }
    }

    fn cost(i: f64, o: f64, w: f64, r: f64) -> ModelCost {
        ModelCost {
            input: i,
            output: o,
            cache_write: w,
            cache_read: r,
        }
    }

    /// 内置快照可解析、非空、覆盖官方与第三方锚点模型
    #[test]
    fn snapshot_parses_with_anchors() {
        let t = from_snapshot();
        assert!(!t.is_empty());
        for anchor in ["claude-fable-5", "glm-4.6", "deepseek-chat"] {
            assert!(t.lookup(anchor).is_some(), "快照缺锚点 {anchor}");
        }
    }

    /// 匹配链：精确 / 大小写 / [1m] 后缀 / 日期剥离 / provider 前缀 / dot-dash 变体
    #[test]
    fn lookup_chain() {
        let t = table(&[
            ("claude-sonnet-4-5", cost(3.0, 15.0, 3.75, 0.3)),
            ("glm-4.6", cost(0.6, 2.2, 0.0, 0.11)),
            ("minimax-m2.5", cost(0.3, 1.2, 0.375, 0.03)),
        ]);
        assert!(t.lookup("claude-sonnet-4-5").is_some());
        assert!(t.lookup("claude-sonnet-4-5-20250929").is_some(), "日期剥离");
        assert!(t.lookup("claude-sonnet-4-5 [1m]").is_some(), "1m 后缀");
        assert!(t.lookup("anthropic/claude-sonnet-4-5").is_some(), "provider 前缀");
        assert!(t.lookup("GLM-4.6").is_some(), "大小写");
        assert!(t.lookup("glm-4-6").is_some(), "dash→dot 变体");
        assert!(t.lookup("MiniMax-M2.5").is_some(), "混合大小写");
        assert!(t.lookup("qwen3-max").is_none(), "未知模型必须 miss");
        assert!(t.lookup("").is_none());
    }

    /// 前缀最长匹配：未知细分型号塌到已知系列，且取最长键
    #[test]
    fn lookup_longest_prefix() {
        let t = table(&[
            ("gpt-5", cost(1.25, 10.0, 1.5, 0.125)),
            ("gpt-5-mini", cost(0.25, 2.0, 0.3, 0.025)),
        ]);
        let hit = t.lookup("gpt-5-mini-2027-01-01").unwrap();
        assert_eq!(hit.input, 0.25, "应命中更长的 gpt-5-mini 而非 gpt-5");
        let hit = t.lookup("gpt-5-nano").unwrap();
        assert_eq!(hit.input, 1.25, "gpt-5-nano 塌到 gpt-5");
        assert!(t.lookup("gpt-5-7").is_none(), "版本升级段拒绝塌落（新版可能调价）");
        assert!(t.lookup("gpt-5.7").is_none(), "dot 版本经变体后同样拒绝");
    }

    /// 四类分价：缓存读写与输入输出各按各价
    #[test]
    fn cost_four_kinds() {
        let t = table(&[("m", cost(3.0, 15.0, 3.75, 0.3))]);
        let usage = TokenUsage {
            input_tokens: 1_000_000,
            output_tokens: 1_000_000,
            cache_creation_input_tokens: 1_000_000,
            cache_read_input_tokens: 1_000_000,
        };
        let usd = t.cost_usd("m", &usage).unwrap();
        assert!((usd - (3.0 + 15.0 + 3.75 + 0.3)).abs() < 1e-9);
        assert!(t.cost_usd("unknown", &usage).is_none());
    }

    /// models.dev 解析：白名单收录、缺缓存字段按比例兜底、显式 0 保留、同名先到先得
    #[test]
    fn parse_allowlist_and_fallback() {
        let api = serde_json::json!({
            "anthropic": {"models": {"claude-x": {"cost": {"input": 3.0, "output": 15.0, "cache_write": 3.75, "cache_read": 0.3}}}},
            "zhipuai": {"models": {"glm-x": {"cost": {"input": 0.6, "output": 2.2, "cache_write": 0, "cache_read": 0.11}}}},
            "moonshotai": {"models": {"kimi-x": {"cost": {"input": 0.6, "output": 3.0}}}},
            "zai": {"models": {"glm-x": {"cost": {"input": 99.0, "output": 99.0}}}},
            "some-gateway": {"models": {"claude-x": {"cost": {"input": 99.0, "output": 99.0}}}},
            "openai": {"models": {"free-model": {}}}
        });
        let m = parse_models_dev(&api);
        assert_eq!(m["glm-x"].cache_write, 0.0, "显式 0 保留");
        assert!((m["kimi-x"].cache_write - 0.75).abs() < 1e-9, "缺字段 1.25x");
        assert!((m["kimi-x"].cache_read - 0.06).abs() < 1e-9, "缺字段 0.1x");
        assert_eq!(m["glm-x"].input, 0.6, "白名单顺序先到先得");
        assert_eq!(m["claude-x"].input, 3.0, "网关镜像价不收");
        assert!(!m.contains_key("free-model"), "无 cost 条目跳过");
    }

    /// 联机 smoke：真实拉取 models.dev（不进常规跑）
    /// cargo test -p app pricing -- --ignored --nocapture
    #[test]
    #[ignore]
    fn smoke_fetch_remote() {
        let models = fetch_remote().unwrap();
        println!("remote models: {}", models.len());
        assert!(models.len() > 100);
    }
}
