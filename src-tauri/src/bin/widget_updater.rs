use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use chrono::{Duration, Local, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WidgetSnapshot {
    today_sessions: u32,
    today_tokens: u64,
    models: Vec<String>,
    updated_at: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct WidgetConfig {
    #[serde(default)]
    day_start_hour: i8,
}

fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("CC_SPACE_DATA_DIR") {
        PathBuf::from(d)
    } else {
        dirs::home_dir().unwrap_or_default().join(".cc-space")
    }
}

fn read_config() -> WidgetConfig {
    fs::read_to_string(data_dir().join("widget-config.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn projects_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude/projects")
}

fn widget_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join("Library/Containers/com.ccspace.desktop.widget/Data/widget-data.json")
}

fn compute_day_boundary(day_start_hour: i8) -> (u64, String) {
    let now = Local::now();

    if day_start_hour < 0 {
        // 滚动 24 小时
        let start = now - Duration::hours(24);
        let ts = start.timestamp() as u64;
        let date_str = now.format("%Y-%m-%d").to_string();
        return (ts, date_str);
    }

    let hour = day_start_hour as u32;
    let boundary_time = NaiveTime::from_hms_opt(hour, 0, 0).unwrap_or_default();
    let today = now.date_naive();
    let boundary_today = today
        .and_time(boundary_time)
        .and_local_timezone(Local)
        .unwrap();

    let boundary = if now.naive_local().time() >= boundary_time {
        boundary_today
    } else {
        boundary_today - Duration::days(1)
    };

    let ts = boundary.timestamp() as u64;
    // 对于 token 统计，用 boundary 所在日期查 usage_stats 的 daily
    // usage_stats 按自然日统计（0 点切割），这里近似取 boundary 日期
    let date_str = if now.hour() < hour {
        (today - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string()
    } else {
        today.format("%Y-%m-%d").to_string()
    };

    (ts, date_str)
}

fn collect_today_stats(day_start_hour: i8) -> (u32, u64, Vec<String>) {
    let (start_ts, today_str) = compute_day_boundary(day_start_hour);

    let mut jsonl_files = Vec::new();
    collect_jsonl(&projects_dir(), &mut jsonl_files);
    let sessions = jsonl_files
        .iter()
        .filter(|p| {
            fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .is_some_and(|d| d.as_secs() >= start_ts)
        })
        .count() as u32;

    let mut tokens = 0u64;
    let mut models = Vec::new();
    if let Ok(stats) = app_lib::usage_stats::collect_usage_stats() {
        if day_start_hour < 0 {
            // 滚动 24h：累加最近两天的 daily
            let yesterday = (Local::now() - Duration::days(1))
                .format("%Y-%m-%d")
                .to_string();
            for d in &stats.daily {
                if d.date == today_str || d.date == yesterday {
                    tokens += d.total;
                }
            }
        } else {
            if let Some(day) = stats.daily.iter().find(|d| d.date == today_str) {
                tokens = day.total;
            }
        }
        models = stats.month.by_model.into_iter().map(|m| m.model).collect();
    }

    (sessions, tokens, models)
}

fn collect_jsonl(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "subagents" || name.contains("cc-space-agent") {
                continue;
            }
            collect_jsonl(&path, out);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            if !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("agent-")) {
                out.push(path);
            }
        }
    }
}

fn main() {
    let cfg = read_config();
    let (sessions, tokens, models) = collect_today_stats(cfg.day_start_hour);
    let snap = WidgetSnapshot {
        today_sessions: sessions,
        today_tokens: tokens,
        models,
        updated_at: Local::now().to_rfc3339(),
    };
    let json = serde_json::to_string_pretty(&snap).unwrap_or_default();

    let wp = widget_path();
    if let Some(parent) = wp.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&wp, &json);

    let bp = data_dir().join("widget-data.json");
    if let Some(parent) = bp.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&bp, &json);
}
