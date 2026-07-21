use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WidgetSnapshot {
    today_sessions: u32,
    today_tokens: u64,
    models: Vec<String>,
    updated_at: String,
    // Streak
    current_streak: u32,
    longest_streak: u32,
    active_days: u32,
    // Monthly
    monthly_tokens: u64,
    last_month_tokens: u64,
    monthly_models: Vec<ModelStat>,
    // Cost
    estimated_cost_usd: f64,
    // Weekly (last 7 days)
    weekly_tokens: Vec<DayTokens>,
    // Projects
    active_projects_today: u32,
    top_projects: Vec<ProjectStat>,
    // Hourly distribution (24 entries)
    hourly_distribution: Vec<u32>,
    // Heatmap (last 28 days)
    daily_heatmap: Vec<DayTokens>,
    // Totals
    total_sessions: u32,
    total_tokens: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ModelStat {
    model: String,
    count: u32,
    tokens: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DayTokens {
    date: String,
    tokens: u64,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ProjectStat {
    name: String,
    sessions: u32,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct WidgetConfig {
    #[serde(default)]
    day_start_hour: i8,
    #[serde(default)]
    month_mode: String,
}

fn data_dir() -> PathBuf {
    if let Ok(d) = std::env::var("MONET_DATA_DIR") {
        PathBuf::from(d)
    } else {
        dirs::home_dir().unwrap_or_default().join(".monet")
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
        .join("Library/Containers/io.github.zenolab124.monet.widget/Data/widget-data.json")
}

fn compute_day_boundary(day_start_hour: i8) -> (u64, String) {
    let now = Local::now();

    if day_start_hour < 0 {
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
    let date_str = if now.hour() < hour {
        (today - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string()
    } else {
        today.format("%Y-%m-%d").to_string()
    };

    (ts, date_str)
}

/// 内置 Agent 工作目录的 projects 编码名（与主 App discovery/search/usage 同源清单）
fn agent_dirs() -> &'static std::collections::HashSet<String> {
    static CELL: std::sync::OnceLock<std::collections::HashSet<String>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| app_lib::config::agent_project_dirs().into_iter().collect())
}

fn collect_jsonl(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "subagents" || agent_dirs().contains(name.as_ref()) {
                continue;
            }
            collect_jsonl(&path, out);
        } else if path.extension().is_some_and(|e| e == "jsonl")
            && !path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("agent-")) {
                out.push(path);
            }
    }
}

fn compute_streak(daily: &[app_lib::usage_stats::DailyUsage]) -> (u32, u32, u32) {
    let today = Local::now().date_naive();
    let active_dates: std::collections::HashSet<NaiveDate> = daily
        .iter()
        .filter(|d| d.total > 0)
        .filter_map(|d| NaiveDate::parse_from_str(&d.date, "%Y-%m-%d").ok())
        .collect();

    let active_days = active_dates.len() as u32;

    let mut current = 0u32;
    let mut day = today;
    if !active_dates.contains(&day) {
        day -= Duration::days(1);
    }
    while active_dates.contains(&day) {
        current += 1;
        day -= Duration::days(1);
    }

    let mut longest = 0u32;
    let mut sorted: Vec<NaiveDate> = active_dates.into_iter().collect();
    sorted.sort();
    let mut streak = 0u32;
    for (i, d) in sorted.iter().enumerate() {
        if i == 0 || *d != sorted[i - 1] + Duration::days(1) {
            streak = 1;
        } else {
            streak += 1;
        }
        longest = longest.max(streak);
    }

    (current, longest, active_days)
}

fn estimate_cost(models: &[app_lib::usage_stats::ModelUsage]) -> f64 {
    let mut cost = 0.0;
    for m in models {
        // 加权混合价：约 80% input + 20% output
        let blended_per_million = if m.model.contains("opus") {
            15.0 * 0.8 + 75.0 * 0.2 // 27.0
        } else if m.model.contains("sonnet") {
            3.0 * 0.8 + 15.0 * 0.2 // 5.4
        } else if m.model.contains("haiku") {
            0.25 * 0.8 + 1.25 * 0.2 // 0.45
        } else if m.model.contains("fable") {
            15.0 * 0.8 + 75.0 * 0.2 // 27.0
        } else {
            5.0 * 0.8 + 25.0 * 0.2 // 9.0
        };
        cost += (m.total as f64 / 1_000_000.0) * blended_per_million;
    }
    (cost * 100.0).round() / 100.0
}

fn collect_project_stats(start_ts: u64) -> (u32, Vec<ProjectStat>, u32, Vec<u32>) {
    let mut project_counts: HashMap<String, u32> = HashMap::new();
    let mut active_today = std::collections::HashSet::new();
    let mut hourly = vec![0u32; 24];
    let mut total_sessions = 0u32;

    let Ok(entries) = fs::read_dir(projects_dir()) else {
        return (0, Vec::new(), 0, hourly);
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() { continue; }
        let proj_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        // 内置 Agent 目录不进项目榜（collect_jsonl 只在递归下降时过滤，入口目录须在此拦截）
        if agent_dirs().contains(&proj_name) { continue; }
        // 目录名编码规则：首个 `-` 是根 `/`，其余 `-` 是路径分隔符
        let decoded_path = if let Some(stripped) = proj_name.strip_prefix('-') {
            stripped.replace('-', "/")
        } else {
            proj_name.replace('-', "/")
        };
        let display = decoded_path.rsplit('/').next().unwrap_or(&proj_name);

        let mut jsonls = Vec::new();
        collect_jsonl(&path, &mut jsonls);
        let count = jsonls.len() as u32;
        total_sessions += count;
        *project_counts.entry(display.to_string()).or_default() += count;

        for jf in &jsonls {
            if let Ok(meta) = fs::metadata(jf) {
                if let Ok(modified) = meta.modified() {
                    if let Ok(dur) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                        let ts = dur.as_secs();
                        if ts >= start_ts {
                            active_today.insert(proj_name.clone());
                        }
                        let dt = chrono::DateTime::from_timestamp(ts as i64, 0);
                        if let Some(dt) = dt {
                            let local = dt.with_timezone(&Local);
                            hourly[local.hour() as usize] += 1;
                        }
                    }
                }
            }
        }
    }

    let mut top: Vec<ProjectStat> = project_counts
        .into_iter()
        .map(|(name, sessions)| ProjectStat { name, sessions })
        .collect();
    top.sort_by(|a, b| b.sessions.cmp(&a.sessions));
    top.truncate(8);

    (active_today.len() as u32, top, total_sessions, hourly)
}

fn main() {
    let cfg = read_config();
    let (start_ts, today_str) = compute_day_boundary(cfg.day_start_hour);

    let mut jsonl_files = Vec::new();
    collect_jsonl(&projects_dir(), &mut jsonl_files);
    let today_sessions = jsonl_files
        .iter()
        .filter(|p| {
            fs::metadata(p)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                .is_some_and(|d| d.as_secs() >= start_ts)
        })
        .count() as u32;

    let (today_tokens, models, monthly_tokens, last_month_tokens, monthly_models,
         estimated_cost, weekly_tokens, daily_heatmap, current_streak, longest_streak,
         active_days, total_tokens) =
        if let Ok(stats) = app_lib::usage_stats::collect_usage_stats() {
            let now = Local::now();
            let today_date = now.date_naive();

            // Today tokens
            let mut tt = 0u64;
            if cfg.day_start_hour < 0 {
                let yesterday = (now - Duration::days(1)).format("%Y-%m-%d").to_string();
                for d in &stats.daily {
                    if d.date == today_str || d.date == yesterday { tt += d.total; }
                }
            } else if let Some(day) = stats.daily.iter().find(|d| d.date == today_str) {
                tt = day.total;
            }

            // Last month tokens
            let lm = if now.month() == 1 { 12 } else { now.month() - 1 };
            let ly = if now.month() == 1 { now.year() - 1 } else { now.year() };
            let lmt: u64 = stats.daily.iter()
                .filter(|d| {
                    if let Ok(nd) = NaiveDate::parse_from_str(&d.date, "%Y-%m-%d") {
                        nd.year() == ly && nd.month() == lm
                    } else { false }
                })
                .map(|d| d.total)
                .sum();

            let is_rolling = cfg.month_mode == "rolling";

            // Monthly tokens: natural month or rolling 30 days
            let monthly_t = if is_rolling {
                let cutoff = (today_date - Duration::days(30)).format("%Y-%m-%d").to_string();
                stats.daily.iter().filter(|d| d.date > cutoff).map(|d| d.total).sum()
            } else {
                stats.month.total
            };

            // Model distribution & cost: always from natural month (daily has no model granularity)
            let mm: Vec<ModelStat> = stats.month.by_model.iter().map(|m| ModelStat {
                model: m.model.clone(),
                count: 0,
                tokens: m.total,
            }).collect();
            let cost = estimate_cost(&stats.month.by_model);
            let models_list: Vec<String> = stats.month.by_model.iter().map(|m| m.model.clone()).collect();

            // Weekly (last 7 days)
            let weekly: Vec<DayTokens> = (0..7).rev().map(|i| {
                let d = today_date - Duration::days(i);
                let ds = d.format("%Y-%m-%d").to_string();
                let t = stats.daily.iter().find(|x| x.date == ds).map(|x| x.total).unwrap_or(0);
                DayTokens { date: ds, tokens: t }
            }).collect();

            // Heatmap
            let heatmap: Vec<DayTokens> = if is_rolling {
                (0..30).rev().map(|i| {
                    let d = today_date - Duration::days(i);
                    let ds = d.format("%Y-%m-%d").to_string();
                    let t = stats.daily.iter().find(|x| x.date == ds).map(|x| x.total).unwrap_or(0);
                    DayTokens { date: ds, tokens: t }
                }).collect()
            } else {
                let month_start = NaiveDate::from_ymd_opt(today_date.year(), today_date.month(), 1).unwrap();
                let next_month = if today_date.month() == 12 {
                    NaiveDate::from_ymd_opt(today_date.year() + 1, 1, 1).unwrap()
                } else {
                    NaiveDate::from_ymd_opt(today_date.year(), today_date.month() + 1, 1).unwrap()
                };
                let days_in_month = (next_month - month_start).num_days();
                (0..days_in_month).map(|i| {
                    let d = month_start + Duration::days(i);
                    let ds = d.format("%Y-%m-%d").to_string();
                    let t = stats.daily.iter().find(|x| x.date == ds).map(|x| x.total).unwrap_or(0);
                    DayTokens { date: ds, tokens: t }
                }).collect()
            };

            let (cs, ls, ad) = compute_streak(&stats.daily);
            let total_t: u64 = stats.daily.iter().map(|d| d.total).sum();

            (tt, models_list, monthly_t, lmt, mm, cost, weekly, heatmap, cs, ls, ad, total_t)
        } else {
            (0, Vec::new(), 0, 0, Vec::new(), 0.0, Vec::new(), Vec::new(), 0, 0, 0, 0)
        };

    let (active_projects, top_projects, total_sessions, hourly) = collect_project_stats(start_ts);

    let snap = WidgetSnapshot {
        today_sessions,
        today_tokens,
        models,
        updated_at: Local::now().to_rfc3339(),
        current_streak,
        longest_streak,
        active_days,
        monthly_tokens,
        last_month_tokens,
        monthly_models,
        estimated_cost_usd: estimated_cost,
        weekly_tokens,
        active_projects_today: active_projects,
        top_projects,
        hourly_distribution: hourly,
        daily_heatmap,
        total_sessions,
        total_tokens,
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
