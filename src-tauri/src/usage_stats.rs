//! 全项目 token 用量聚合（v2.2.0 FR-001）—— 首页 Token 卡与活跃热力图的数据源
//!
//! 聚合口径（PRD docs/prd/v2.2.0-home-dashboard.md FR-001）：
//! - 仅 assistant 记录的 message.usage，四类 token 求和（计费口径）
//! - 按 message.id 去重：同一次 API 响应拆成多行时每行重复携带相同 usage，
//!   只计首次；id 缺失的行按行独立计
//! - `<synthetic>`（CLI 本地合成占位）与 timestamp 缺失的记录不进任何桶
//! - timestamp（ISO 8601 UTC）转本地时区后分天

use chrono::{DateTime, Datelike, Local, NaiveDate};
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::cache::{self, CachedContrib, CachedUsage};
use crate::models::TokenUsage;
use crate::probe;

#[derive(Debug, Serialize)]
pub struct UsageStats {
    /// 近 16 周窗口（本周一往前 15 周起）内有数据的天，date 为本地 "YYYY-MM-DD"
    pub daily: Vec<DailyUsage>,
    /// 本地时区当前自然月
    pub month: MonthUsage,
}

#[derive(Debug, Serialize)]
pub struct DailyUsage {
    pub date: String,
    pub total: u64,
}

#[derive(Debug, Serialize)]
pub struct MonthUsage {
    pub total: u64,
    #[serde(rename = "byModel")]
    pub by_model: Vec<ModelUsage>,
}

#[derive(Debug, Serialize)]
pub struct ModelUsage {
    pub model: String,
    pub total: u64,
}

/// 单条 assistant 行的贡献；model 存原始串，归一化延后到聚合阶段
/// （distinct 模型数有限，避免在 10 万级热路径上跑 regex）
struct Contribution {
    date: NaiveDate,
    model: Option<String>,
    tokens: u64,
}

/// 文件局部聚合容器，rayon map-reduce 合并。
/// by_id 的去重跨文件生效（resume/fork 复制历史行时同 id 也只计一次）
#[derive(Default)]
struct Buckets {
    by_id: HashMap<String, Contribution>,
    anon: Vec<Contribution>,
}

impl Buckets {
    /// 已知边界：rayon reduce 的合并顺序非确定，跨文件同 id 而 usage 不一致时
    /// （resume/fork 谱系、resp_* 代理会话首行 usage=0 终行才有真值）「首次」不可复现。
    /// Anthropic msg_* 行同 id 的 usage 逐字节相同，保留哪份结果一致，影响可忽略
    fn merge(mut self, other: Buckets) -> Buckets {
        for (k, v) in other.by_id {
            self.by_id.entry(k).or_insert(v);
        }
        self.anon.extend(other.anon);
        self
    }
}

/// 轻量行解析：只取去重/分桶必需字段，跳过 content 反序列化
#[derive(Deserialize)]
struct LineExtract {
    #[serde(rename = "type")]
    record_type: Option<String>,
    timestamp: Option<String>,
    message: Option<MsgExtract>,
}

#[derive(Deserialize)]
struct MsgExtract {
    id: Option<String>,
    model: Option<String>,
    usage: Option<TokenUsage>,
}

fn scan_file(path: &Path) -> Buckets {
    let mut out = Buckets::default();
    let Ok(file) = File::open(path) else { return out };
    let reader = BufReader::with_capacity(64 * 1024, file);

    for line in reader.lines() {
        let Ok(line) = line else { break };
        if !line.contains("\"assistant\"") || !line.contains("\"usage\"") {
            continue;
        }
        let Ok(ext) = serde_json::from_str::<LineExtract>(&line) else { continue };
        if ext.record_type.as_deref() != Some("assistant") {
            continue;
        }
        let Some(msg) = ext.message else { continue };
        if msg.model.as_deref() == Some("<synthetic>") {
            continue;
        }
        let Some(usage) = msg.usage else { continue };
        let Some(date) = ext
            .timestamp
            .as_deref()
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Local).date_naive())
        else {
            continue;
        };

        let contribution = Contribution {
            date,
            model: msg.model,
            tokens: usage.total(),
        };
        match msg.id {
            Some(id) => {
                out.by_id.entry(id).or_insert(contribution);
            }
            None => out.anon.push(contribution),
        }
    }
    out
}

fn scan_file_cached(path: &Path) -> Buckets {
    if let Some(cached) = cache::get_usage(path) {
        return cached_to_buckets(cached);
    }
    let buckets = scan_file(path);
    cache::set_usage(path, buckets_to_cached(&buckets));
    buckets
}

fn cached_to_buckets(cached: CachedUsage) -> Buckets {
    let by_id = cached
        .by_id
        .into_iter()
        .filter_map(|(id, c)| {
            NaiveDate::parse_from_str(&c.date, "%Y-%m-%d")
                .ok()
                .map(|date| {
                    (
                        id,
                        Contribution {
                            date,
                            model: c.model,
                            tokens: c.tokens,
                        },
                    )
                })
        })
        .collect();
    let anon = cached
        .anon
        .into_iter()
        .filter_map(|c| {
            NaiveDate::parse_from_str(&c.date, "%Y-%m-%d")
                .ok()
                .map(|date| Contribution {
                    date,
                    model: c.model,
                    tokens: c.tokens,
                })
        })
        .collect();
    Buckets { by_id, anon }
}

fn buckets_to_cached(buckets: &Buckets) -> CachedUsage {
    CachedUsage {
        by_id: buckets
            .by_id
            .iter()
            .map(|(id, c)| {
                (
                    id.clone(),
                    CachedContrib {
                        date: c.date.format("%Y-%m-%d").to_string(),
                        model: c.model.clone(),
                        tokens: c.tokens,
                    },
                )
            })
            .collect(),
        anon: buckets
            .anon
            .iter()
            .map(|c| CachedContrib {
                date: c.date.format("%Y-%m-%d").to_string(),
                model: c.model.clone(),
                tokens: c.tokens,
            })
            .collect(),
    }
}

/// 模型名归一化（PRD FR-001 规则 5，五步顺序执行）
fn normalize_model(raw: &str, date_suffix: &Regex, version_tail: &Regex) -> String {
    // ① 去方括号后缀（如 "opus-4.6 [1m]"）与首尾空白
    let s = raw.split('[').next().unwrap_or(raw).trim();
    // ② 去前缀 claude-
    let s = s.strip_prefix("claude-").unwrap_or(s);
    // ③ 去尾部 -YYYYMMDD 八位日期后缀
    let s = date_suffix.replace(s, "");
    // ④ 尾部 -数字-数字 → -数字.数字（opus-4-8 → opus-4.8）
    let s = version_tail.replace(&s, "-$1.$2");
    // ⑤ 其余原样保留
    s.into_owned()
}

/// 全量扫描 ~/.claude/projects/ 并聚合。同步阻塞实现，command 层负责丢进 blocking 线程
pub fn collect_usage_stats() -> Result<UsageStats, String> {
    let root = dirs::home_dir()
        .ok_or_else(|| "无法定位 home 目录".to_string())?
        .join(".claude")
        .join("projects");
    // 显式探测一次：不存在与不可读（EACCES 时 is_dir 仍为 true）都要走 Err，
    // 否则 collect_jsonl 静默吞错会让前端把「读不到」误显示为「本月 0 用量」
    std::fs::read_dir(&root).map_err(|e| format!("会话数据目录不可读 {}: {e}", root.display()))?;

    let mut files = Vec::new();
    probe::collect_jsonl(&root, None, &mut files);

    // 内置 Agent 落盘会话不计入用量：与档案/搜索/watcher 的软屏蔽口径一致
    let agent_dirs: std::collections::HashSet<String> =
        crate::config::agent_project_dirs().into_iter().collect();
    files.retain(|p| {
        p.strip_prefix(&root)
            .ok()
            .and_then(|rel| rel.components().next())
            .map(|c| !agent_dirs.contains(c.as_os_str().to_string_lossy().as_ref()))
            .unwrap_or(true)
    });

    let buckets = files
        .par_iter()
        .map(|p| scan_file_cached(p))
        .reduce(Buckets::default, Buckets::merge);

    let today = Local::now().date_naive();
    // 16 周窗口起点 = 本周一往前 15 周（最后一列为本周）
    let days_from_monday = today.weekday().num_days_from_monday() as i64;
    let window_start = today - chrono::Duration::days(days_from_monday + 15 * 7);

    let date_suffix = Regex::new(r"-\d{8}$").map_err(|e| e.to_string())?;
    let version_tail = Regex::new(r"-(\d+)-(\d+)$").map_err(|e| e.to_string())?;

    let mut daily: HashMap<NaiveDate, u64> = HashMap::new();
    let mut by_model: HashMap<String, u64> = HashMap::new();
    let mut month_total: u64 = 0;

    for c in buckets.by_id.into_values().chain(buckets.anon) {
        if c.date >= window_start && c.date <= today {
            *daily.entry(c.date).or_default() += c.tokens;
        }
        // 上界与 daily 同口径：时钟漂移产生的未来时戳不计入月度
        if c.date.year() == today.year() && c.date.month() == today.month() && c.date <= today {
            month_total += c.tokens;
            let model = c
                .model
                .as_deref()
                .map(|m| normalize_model(m, &date_suffix, &version_tail))
                .unwrap_or_else(|| "未知".to_string());
            *by_model.entry(model).or_default() += c.tokens;
        }
    }

    let mut daily: Vec<DailyUsage> = daily
        .into_iter()
        .map(|(date, total)| DailyUsage {
            date: date.format("%Y-%m-%d").to_string(),
            total,
        })
        .collect();
    daily.sort_unstable_by(|a, b| a.date.cmp(&b.date));

    let mut by_model: Vec<ModelUsage> = by_model
        .into_iter()
        .map(|(model, total)| ModelUsage { model, total })
        .collect();
    by_model.sort_unstable_by(|a, b| b.total.cmp(&a.total));

    cache::flush();

    Ok(UsageStats {
        daily,
        month: MonthUsage {
            total: month_total,
            by_model,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// FR-001 规则 5：模型名归一化五步
    #[test]
    fn model_normalization_rules() {
        let date_suffix = Regex::new(r"-\d{8}$").unwrap();
        let version_tail = Regex::new(r"-(\d+)-(\d+)$").unwrap();
        let norm = |s: &str| normalize_model(s, &date_suffix, &version_tail);

        assert_eq!(norm("claude-opus-4-8"), "opus-4.8");
        assert_eq!(norm("claude-fable-5"), "fable-5");
        assert_eq!(norm("claude-sonnet-4-5-20250929"), "sonnet-4.5");
        assert_eq!(norm("gpt-5.4"), "gpt-5.4");
        assert_eq!(norm("sonnet"), "sonnet");
        assert_eq!(norm("claude-opus-4-6 [1m]"), "opus-4.6");
    }

    /// FR-001 规则 2/3/4：id 去重进 map、synthetic 与缺 timestamp 不进桶、缺 cache 字段按 0
    #[test]
    fn scan_file_buckets() {
        let path = std::env::temp_dir().join("cc-space-test-usage.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        let mk = |id: &str, model: &str, ts: &str| {
            format!(
                "{{\"type\":\"assistant\",{ts}\"message\":{{\"id\":\"{id}\",\"model\":\"{model}\",\"usage\":{{\"input_tokens\":1,\"output_tokens\":2}}}}}}"
            )
        };
        let ts = "\"timestamp\":\"2026-06-11T10:00:00.000Z\",";
        writeln!(f, "{}", mk("m1", "claude-fable-5", ts)).unwrap();
        writeln!(f, "{}", mk("m1", "claude-fable-5", ts)).unwrap(); // 同 id 重复
        writeln!(f, "{}", mk("m2", "<synthetic>", ts)).unwrap(); // synthetic 剔除
        writeln!(f, "{}", mk("m3", "claude-fable-5", "")).unwrap(); // 缺 timestamp 剔除
        drop(f);

        let buckets = scan_file(&path);
        assert_eq!(buckets.by_id.len(), 1);
        assert!(buckets.anon.is_empty());
        // 极简 usage（缺两个 cache 字段）按 0 计：1 + 2 = 3
        assert_eq!(buckets.by_id.get("m1").unwrap().tokens, 3);
        std::fs::remove_file(&path).ok();
    }

    /// 联机 smoke：本机真实数据全量聚合的耗时与量级。
    /// 依赖本机 ~/.claude/projects/，不进常规跑：cargo test -- --ignored --nocapture
    #[test]
    #[ignore]
    fn smoke_full_aggregation() {
        let t0 = std::time::Instant::now();
        let stats = collect_usage_stats().unwrap();
        println!(
            "elapsed: {:?} · daily days: {} · month total: {} · by_model: {:?}",
            t0.elapsed(),
            stats.daily.len(),
            stats.month.total,
            stats
                .month
                .by_model
                .iter()
                .map(|m| (m.model.as_str(), m.total))
                .collect::<Vec<_>>()
        );
    }
}
