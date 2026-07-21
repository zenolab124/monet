//! schema-probe —— Claude Code 数据格式差距探测器（CLI 薄壳）
//!
//! 扫描与三态 diff 的核心实现在 `app_lib::probe`（与首页诊断 command 共享，
//! v2.2.0 FR-004 抽取）；本文件只负责参数解析与终端/--json 输出，
//! 两种模式的输出格式是既有契约，重构前后保持一致。
//!
//! 用法:
//!   cargo run --bin schema-probe                # 全量扫描，终端报告
//!   cargo run --bin schema-probe -- --days 14   # 只扫 mtime 14 天内的文件
//!   cargo run --bin schema-probe -- --json      # 机器可读输出（供 UI 复用）

use app_lib::probe::{run_probe, Entry, Report};
use std::collections::BTreeMap;

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

fn print_report(report: &Report) {
    println!("=== Monet Schema 探测报告 ===");
    println!(
        "扫描 {} 文件（含子会话 {}）/ {} 行 / 解析失败 {} 行 / 耗时 {:.1}s",
        report.scanned_files,
        report.subagent_files,
        report.scanned_lines,
        report.parse_errors,
        report.elapsed_ms as f32 / 1000.0
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let as_json = args.iter().any(|a| a == "--json");
    let days: Option<u64> = args
        .iter()
        .position(|a| a == "--days")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok());

    let report = match run_probe(days) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };

    // 与重构前行为一致：无文件可扫时报错退出（路径提示沿用原文案）
    if report.scanned_files == 0 {
        let root = app_lib::config::projects_dir();
        eprintln!("未找到 JSONL 文件: {}", root.display());
        std::process::exit(1);
    }

    if as_json {
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        return;
    }

    print_report(&report);
}
