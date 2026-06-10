//! schema-probe —— Claude Code 数据格式差距探测器
//!
//! 全量扫描 ~/.claude/projects/ 的 JSONL（含 subagents 子会话），统计
//! record type / content block type / tool name 的真实分布，与
//! schema-support.json 声明清单做三态 diff：已支持 / 已忽略 / 未知。
//!
//! 用法:
//!   cargo run --bin schema-probe                # 全量扫描，终端报告
//!   cargo run --bin schema-probe -- --days 14   # 只扫 mtime 14 天内的文件
//!   cargo run --bin schema-probe -- --json      # 机器可读输出（供将来 UI 复用）

use rayon::prelude::*;
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

/// 编译期嵌入声明文件，避免运行时路径依赖
const SUPPORT_DECL: &str = include_str!("../../schema-support.json");

/// 每个类型最多保留的样本数
const MAX_SAMPLES: usize = 2;
/// 样本 JSON 截断长度（字符）
const SAMPLE_LEN: usize = 500;

#[derive(Debug, Clone, Serialize)]
struct Sample {
    file: String,
    line_no: usize,
    excerpt: String,
}

#[derive(Debug, Default, Clone, Serialize)]
struct Entry {
    count: usize,
    samples: Vec<Sample>,
}

impl Entry {
    fn merge(&mut self, other: Entry) {
        self.count += other.count;
        for s in other.samples {
            if self.samples.len() < MAX_SAMPLES {
                self.samples.push(s);
            }
        }
    }
}

/// 单文件/全局共用的统计容器，rayon map-reduce 合并
#[derive(Debug, Default)]
struct Stats {
    records: HashMap<String, Entry>,
    blocks: HashMap<String, Entry>,
    tools: HashMap<String, Entry>,
    files: usize,
    subagent_files: usize,
    lines: usize,
    parse_errors: usize,
}

impl Stats {
    fn merge(mut self, other: Stats) -> Stats {
        for (k, v) in other.records {
            self.records.entry(k).or_default().merge(v);
        }
        for (k, v) in other.blocks {
            self.blocks.entry(k).or_default().merge(v);
        }
        for (k, v) in other.tools {
            self.tools.entry(k).or_default().merge(v);
        }
        self.files += other.files;
        self.subagent_files += other.subagent_files;
        self.lines += other.lines;
        self.parse_errors += other.parse_errors;
        self
    }

    fn bump(map: &mut HashMap<String, Entry>, key: &str, file: &Path, line_no: usize, raw: &str) {
        let entry = map.entry(key.to_string()).or_default();
        entry.count += 1;
        if entry.samples.len() < MAX_SAMPLES {
            entry.samples.push(Sample {
                file: file.display().to_string(),
                line_no,
                excerpt: raw.chars().take(SAMPLE_LEN).collect(),
            });
        }
    }
}

/// 声明清单（与 schema-support.json 结构对应，手动解析避免引入额外 derive 依赖）
struct Decl {
    record_supported: Vec<String>,
    record_ignored: BTreeMap<String, String>,
    block_supported: Vec<String>,
    block_ignored: BTreeMap<String, String>,
    tool_dedicated: Vec<String>,
    tool_generic_ok: BTreeMap<String, String>,
    mcp_prefix: String,
}

fn parse_decl() -> Decl {
    let v: Value = serde_json::from_str(SUPPORT_DECL).expect("schema-support.json 格式错误");
    let str_vec = |v: &Value, path: &[&str]| -> Vec<String> {
        let mut cur = v;
        for p in path {
            cur = &cur[p];
        }
        cur.as_array()
            .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
            .unwrap_or_default()
    };
    let str_map = |v: &Value, path: &[&str]| -> BTreeMap<String, String> {
        let mut cur = v;
        for p in path {
            cur = &cur[p];
        }
        cur.as_object()
            .map(|o| {
                o.iter()
                    .map(|(k, val)| (k.clone(), val.as_str().unwrap_or("").to_string()))
                    .collect()
            })
            .unwrap_or_default()
    };
    Decl {
        record_supported: str_vec(&v, &["recordTypes", "supported"]),
        record_ignored: str_map(&v, &["recordTypes", "ignored"]),
        block_supported: str_vec(&v, &["blockTypes", "supported"]),
        block_ignored: str_map(&v, &["blockTypes", "ignored"]),
        tool_dedicated: str_vec(&v, &["tools", "dedicated"]),
        tool_generic_ok: str_map(&v, &["tools", "genericOk"]),
        mcp_prefix: v["tools"]["mcpPrefix"].as_str().unwrap_or("mcp__").to_string(),
    }
}

/// 递归收集 .jsonl 文件，可按 mtime 过滤
fn collect_jsonl(dir: &Path, cutoff: Option<SystemTime>, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_jsonl(&path, cutoff, out);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            // workflow 运行日志（started/result 记录），非会话格式，不在探测范围
            if path.file_name().is_some_and(|n| n == "journal.jsonl") {
                continue;
            }
            if let Some(cut) = cutoff {
                let fresh = entry
                    .metadata()
                    .and_then(|m| m.modified())
                    .map(|m| m >= cut)
                    .unwrap_or(true);
                if !fresh {
                    continue;
                }
            }
            out.push(path);
        }
    }
}

/// 解析单个 JSONL 文件，产出局部统计
fn scan_file(path: &Path) -> Stats {
    let mut stats = Stats::default();
    stats.files = 1;
    if path.components().any(|c| c.as_os_str() == "subagents") {
        stats.subagent_files = 1;
    }
    let Ok(file) = fs::File::open(path) else { return stats };
    let reader = BufReader::new(file);

    for (idx, line) in reader.lines().enumerate() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        stats.lines += 1;
        let line_no = idx + 1;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            stats.parse_errors += 1;
            continue;
        };

        let rtype = value.get("type").and_then(|t| t.as_str()).unwrap_or("(no-type)");
        Stats::bump(&mut stats.records, rtype, path, line_no, &line);

        // user / assistant 消息深入 content block 层
        if rtype == "user" || rtype == "assistant" {
            let Some(blocks) = value
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_array())
            else {
                continue;
            };
            for block in blocks {
                let btype = block.get("type").and_then(|t| t.as_str()).unwrap_or("(no-type)");
                let raw = block.to_string();
                Stats::bump(&mut stats.blocks, btype, path, line_no, &raw);
                if btype == "tool_use" {
                    if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                        Stats::bump(&mut stats.tools, name, path, line_no, &raw);
                    }
                }
            }
        }
    }
    stats
}

/// 三态 diff 结果（同时是 --json 输出结构）
#[derive(Debug, Serialize)]
struct Report {
    scanned_files: usize,
    subagent_files: usize,
    scanned_lines: usize,
    parse_errors: usize,
    elapsed_ms: u128,
    record_types: Diff,
    block_types: Diff,
    tools: ToolDiff,
}

#[derive(Debug, Serialize)]
struct Diff {
    supported: BTreeMap<String, usize>,
    ignored: BTreeMap<String, usize>,
    unknown: BTreeMap<String, Entry>,
}

#[derive(Debug, Serialize)]
struct ToolDiff {
    dedicated: BTreeMap<String, usize>,
    mcp: BTreeMap<String, usize>,
    generic_ok: BTreeMap<String, usize>,
    /// 走 Generic 兜底且未决策过的工具——分诊对象
    generic_undeclared: BTreeMap<String, Entry>,
    /// 声明了专属组件但数据中零出现——改名/下线信号
    dedicated_unseen: Vec<String>,
}

fn make_diff(found: HashMap<String, Entry>, supported: &[String], ignored: &BTreeMap<String, String>) -> Diff {
    let mut diff = Diff {
        supported: BTreeMap::new(),
        ignored: BTreeMap::new(),
        unknown: BTreeMap::new(),
    };
    for (k, v) in found {
        if supported.iter().any(|s| s == &k) {
            diff.supported.insert(k, v.count);
        } else if ignored.contains_key(&k) {
            diff.ignored.insert(k, v.count);
        } else {
            diff.unknown.insert(k, v);
        }
    }
    diff
}

fn make_tool_diff(found: HashMap<String, Entry>, decl: &Decl) -> ToolDiff {
    let mut diff = ToolDiff {
        dedicated: BTreeMap::new(),
        mcp: BTreeMap::new(),
        generic_ok: BTreeMap::new(),
        generic_undeclared: BTreeMap::new(),
        dedicated_unseen: Vec::new(),
    };
    for (k, v) in &found {
        if decl.tool_dedicated.iter().any(|s| s == k) {
            diff.dedicated.insert(k.clone(), v.count);
        } else if k.starts_with(&decl.mcp_prefix) {
            diff.mcp.insert(k.clone(), v.count);
        } else if decl.tool_generic_ok.contains_key(k) {
            diff.generic_ok.insert(k.clone(), v.count);
        } else {
            diff.generic_undeclared.insert(k.clone(), v.clone());
        }
    }
    for name in &decl.tool_dedicated {
        if !found.contains_key(name) {
            diff.dedicated_unseen.push(name.clone());
        }
    }
    diff
}

fn print_counts(label: &str, map: &BTreeMap<String, usize>) {
    if map.is_empty() {
        return;
    }
    let mut pairs: Vec<_> = map.iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1));
    let joined: Vec<String> = pairs.iter().map(|(k, c)| format!("{k}({c})")).collect();
    println!("  {label}: {}", joined.join(" "));
}

fn print_unknown(map: &BTreeMap<String, Entry>) {
    if map.is_empty() {
        println!("  ❓ 未知: 无");
        return;
    }
    let mut pairs: Vec<_> = map.iter().collect();
    pairs.sort_by(|a, b| b.1.count.cmp(&a.1.count));
    println!("  ❓ 未知 {} 种:", pairs.len());
    for (k, entry) in pairs {
        println!("     {k}  ×{}", entry.count);
        if let Some(s) = entry.samples.first() {
            println!("        样本 {}:{}", s.file, s.line_no);
            println!("        {}", s.excerpt);
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let as_json = args.iter().any(|a| a == "--json");
    let days: Option<u64> = args
        .iter()
        .position(|a| a == "--days")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok());

    let root = dirs::home_dir().expect("无法定位 home 目录").join(".claude/projects");
    let cutoff = days.map(|d| SystemTime::now() - Duration::from_secs(d * 86400));

    let mut files = Vec::new();
    collect_jsonl(&root, cutoff, &mut files);
    if files.is_empty() {
        eprintln!("未找到 JSONL 文件: {}", root.display());
        std::process::exit(1);
    }

    let start = Instant::now();
    let stats = files
        .par_iter()
        .map(|p| scan_file(p))
        .reduce(Stats::default, Stats::merge);
    let elapsed = start.elapsed();

    let decl = parse_decl();
    let report = Report {
        scanned_files: stats.files,
        subagent_files: stats.subagent_files,
        scanned_lines: stats.lines,
        parse_errors: stats.parse_errors,
        elapsed_ms: elapsed.as_millis(),
        record_types: make_diff(stats.records, &decl.record_supported, &decl.record_ignored),
        block_types: make_diff(stats.blocks, &decl.block_supported, &decl.block_ignored),
        tools: make_tool_diff(stats.tools, &decl),
    };

    if as_json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        return;
    }

    println!("=== CC Space Schema 探测报告 ===");
    println!(
        "扫描 {} 文件（含子会话 {}）/ {} 行 / 解析失败 {} 行 / 耗时 {:.1}s",
        report.scanned_files,
        report.subagent_files,
        report.scanned_lines,
        report.parse_errors,
        elapsed.as_secs_f32()
    );

    println!("\n【Record 类型】");
    print_counts("✅ 已支持", &report.record_types.supported);
    print_counts("🔕 已忽略", &report.record_types.ignored);
    print_unknown(&report.record_types.unknown);

    println!("\n【Content Block 类型】");
    print_counts("✅ 已支持", &report.block_types.supported);
    print_counts("🔕 已忽略", &report.block_types.ignored);
    print_unknown(&report.block_types.unknown);

    println!("\n【工具】");
    print_counts("✅ 专属组件", &report.tools.dedicated);
    print_counts("🔌 MCP 路由", &report.tools.mcp);
    print_counts("🔕 Generic 已决策", &report.tools.generic_ok);
    print_unknown(&report.tools.generic_undeclared);
    if !report.tools.dedicated_unseen.is_empty() {
        println!(
            "  ⚠️ 声明了专属组件但数据中零出现（改名/下线信号）: {}",
            report.tools.dedicated_unseen.join(" ")
        );
    }
}
