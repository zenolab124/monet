use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::metadata::agent_cwd;

// ---------------------------------------------------------------------------
// Data structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineDefinition {
    pub id: String,
    pub name: String,
    pub cron_expression: String,
    pub original_text: String,
    pub prompt: String,
    pub enabled: bool,
    pub created_at: String,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineExecutionLog {
    pub routine_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineRow {
    #[serde(flatten)]
    pub definition: RoutineDefinition,
    pub last_execution: Option<RoutineExecutionLog>,
    pub is_running: bool,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

static ROUTINES: Mutex<Option<Vec<RoutineDefinition>>> = Mutex::new(None);
static RUNNING: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static SCHEDULER_ALIVE: AtomicBool = AtomicBool::new(false);

// ---------------------------------------------------------------------------
// File paths
// ---------------------------------------------------------------------------

fn routines_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("cc-space")
        .join("routines.json")
}

fn logs_dir(routine_id: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("cc-space")
        .join("routines")
        .join("logs")
        .join(routine_id)
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

fn load_routines() -> Vec<RoutineDefinition> {
    let path = routines_path();
    if !path.exists() {
        return Vec::new();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_routines(data: &[RoutineDefinition]) {
    let path = routines_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(data) {
        let _ = fs::write(&path, json);
    }
}

fn with_routines<F, R>(f: F) -> R
where
    F: FnOnce(&mut Vec<RoutineDefinition>) -> R,
{
    let mut guard = ROUTINES.lock().unwrap();
    let store = guard.get_or_insert_with(load_routines);
    f(store)
}

fn is_running(id: &str) -> bool {
    RUNNING
        .lock()
        .unwrap()
        .as_ref()
        .map_or(false, |s| s.contains(id))
}

fn set_running(id: &str, running: bool) {
    let mut guard = RUNNING.lock().unwrap();
    let set = guard.get_or_insert_with(HashSet::new);
    if running {
        set.insert(id.to_string());
    } else {
        set.remove(id);
    }
}

// ---------------------------------------------------------------------------
// Cron helpers
// ---------------------------------------------------------------------------

fn validate_cron(cron_expr: &str) -> Result<(), String> {
    use cron::Schedule;
    use std::str::FromStr;
    // cron crate 需要 6/7 字段（秒 分 时 日 月 周），我们的 5 字段格式需要补秒
    let full = format!("0 {}", cron_expr);
    Schedule::from_str(&full).map_err(|e| format!("无效的 cron 表达式: {}", e))?;
    Ok(())
}

fn compute_next_run_full(cron_expr: &str) -> Option<String> {
    use cron::Schedule;
    use std::str::FromStr;
    let full = format!("0 {}", cron_expr);
    let schedule = Schedule::from_str(&full).ok()?;
    let next = schedule.upcoming(chrono::Local).next()?;
    Some(next.to_rfc3339())
}

// ---------------------------------------------------------------------------
// Log helpers
// ---------------------------------------------------------------------------

fn read_latest_log(routine_id: &str) -> Option<RoutineExecutionLog> {
    let dir = logs_dir(routine_id);
    if !dir.exists() {
        return None;
    }
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
        .collect();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    let latest = entries.first()?;
    let content = fs::read_to_string(latest.path()).ok()?;
    serde_json::from_str(&content).ok()
}

fn write_execution_log(log: &RoutineExecutionLog) {
    let dir = logs_dir(&log.routine_id);
    let _ = fs::create_dir_all(&dir);
    let epoch_ms = chrono::DateTime::parse_from_rfc3339(&log.started_at)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_else(|_| Utc::now().timestamp_millis());
    let path = dir.join(format!("{}.json", epoch_ms));
    if let Ok(json) = serde_json::to_string_pretty(log) {
        let _ = fs::write(&path, json);
    }
}

fn read_logs(routine_id: &str, limit: usize) -> Vec<RoutineExecutionLog> {
    let dir = logs_dir(routine_id);
    if !dir.exists() {
        return Vec::new();
    }
    let mut entries: Vec<_> = fs::read_dir(&dir)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map_or(false, |ext| ext == "json"))
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries
        .into_iter()
        .take(limit)
        .filter_map(|e| {
            let content = fs::read_to_string(e.path()).ok()?;
            serde_json::from_str(&content).ok()
        })
        .collect()
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…(已截断)", &s[..max])
    }
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

fn execute_routine(routine: &RoutineDefinition, app: &AppHandle) {
    let id = routine.id.clone();
    let prompt = routine.prompt.clone();
    let app = app.clone();

    set_running(&id, true);

    tauri::async_runtime::spawn_blocking(move || {
        let started_at = Utc::now().to_rfc3339();

        let output = Command::new("claude")
            .arg("-p")
            .arg(&prompt)
            .arg("--model")
            .arg("claude-haiku-4-5-20251001")
            .arg("--output-format")
            .arg("text")
            .arg("--no-session-persistence")
            .current_dir(agent_cwd())
            .output();

        let finished_at = Utc::now().to_rfc3339();

        let log = match output {
            Ok(out) => RoutineExecutionLog {
                routine_id: id.clone(),
                started_at,
                finished_at: Some(finished_at),
                exit_code: out.status.code(),
                stdout: truncate_str(&String::from_utf8_lossy(&out.stdout), 10240),
                stderr: truncate_str(&String::from_utf8_lossy(&out.stderr), 4096),
            },
            Err(e) => RoutineExecutionLog {
                routine_id: id.clone(),
                started_at,
                finished_at: Some(finished_at),
                exit_code: Some(-1),
                stdout: String::new(),
                stderr: format!("spawn failed: {}", e),
            },
        };

        write_execution_log(&log);

        with_routines(|routines| {
            if let Some(r) = routines.iter_mut().find(|r| r.id == id) {
                r.last_run = Some(log.started_at.clone());
                r.next_run = compute_next_run_full(&r.cron_expression);
            }
            save_routines(routines);
        });

        set_running(&id, false);

        let _ = app.emit("routine-executed", serde_json::json!({
            "routineId": id,
        }));
    });
}

// ---------------------------------------------------------------------------
// Scheduler (tick loop)
// ---------------------------------------------------------------------------

pub fn init_scheduler(app: AppHandle) {
    SCHEDULER_ALIVE.store(true, Ordering::SeqCst);

    // 初始化 next_run
    with_routines(|routines| {
        let mut changed = false;
        for r in routines.iter_mut() {
            if r.enabled && r.next_run.is_none() {
                r.next_run = compute_next_run_full(&r.cron_expression);
                changed = true;
            }
        }
        if changed {
            save_routines(routines);
        }
    });

    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            if !SCHEDULER_ALIVE.load(Ordering::SeqCst) {
                break;
            }
            tick(&app);
        }
    });
}

pub fn shutdown_scheduler() {
    SCHEDULER_ALIVE.store(false, Ordering::SeqCst);
}

fn tick(app: &AppHandle) {
    let now = Utc::now();
    let to_fire: Vec<RoutineDefinition> = with_routines(|routines| {
        routines
            .iter()
            .filter(|r| {
                r.enabled
                    && !is_running(&r.id)
                    && r.next_run.as_ref().map_or(false, |nr| {
                        chrono::DateTime::parse_from_rfc3339(nr)
                            .map_or(false, |dt| dt <= now)
                    })
            })
            .cloned()
            .collect()
    });

    for routine in &to_fire {
        execute_routine(routine, app);
    }

    // 更新已触发 routine 的 next_run
    if !to_fire.is_empty() {
        with_routines(|routines| {
            for fired in &to_fire {
                if let Some(r) = routines.iter_mut().find(|r| r.id == fired.id) {
                    r.next_run = compute_next_run_full(&r.cron_expression);
                }
            }
            save_routines(routines);
        });
    }
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_routines() -> Result<Vec<RoutineRow>, String> {
    Ok(with_routines(|routines| {
        routines
            .iter()
            .map(|r| RoutineRow {
                definition: r.clone(),
                last_execution: read_latest_log(&r.id),
                is_running: is_running(&r.id),
            })
            .collect()
    }))
}

#[tauri::command]
pub fn create_routine(
    name: String,
    cron_expression: String,
    original_text: String,
    prompt: String,
    enabled: bool,
) -> Result<RoutineDefinition, String> {
    validate_cron(&cron_expression)?;

    let next_run = if enabled {
        compute_next_run_full(&cron_expression)
    } else {
        None
    };

    let routine = RoutineDefinition {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        cron_expression,
        original_text,
        prompt,
        enabled,
        created_at: Utc::now().to_rfc3339(),
        last_run: None,
        next_run,
    };

    with_routines(|routines| {
        routines.push(routine.clone());
        save_routines(routines);
    });

    Ok(routine)
}

#[tauri::command]
pub fn update_routine(
    id: String,
    name: Option<String>,
    cron_expression: Option<String>,
    original_text: Option<String>,
    prompt: Option<String>,
    enabled: Option<bool>,
) -> Result<RoutineDefinition, String> {
    if let Some(ref expr) = cron_expression {
        validate_cron(expr)?;
    }

    with_routines(|routines| {
        let r = routines
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or_else(|| format!("找不到任务: {}", id))?;

        if let Some(v) = name {
            r.name = v;
        }
        let cron_changed = cron_expression.is_some();
        if let Some(v) = cron_expression {
            r.cron_expression = v;
        }
        if let Some(v) = original_text {
            r.original_text = v;
        }
        if let Some(v) = prompt {
            r.prompt = v;
        }
        let enabled_changed = enabled.is_some();
        if let Some(v) = enabled {
            r.enabled = v;
        }

        if (cron_changed || enabled_changed) && r.enabled {
            r.next_run = compute_next_run_full(&r.cron_expression);
        } else if !r.enabled {
            r.next_run = None;
        }

        let result = r.clone();
        save_routines(routines);
        Ok(result)
    })
}

#[tauri::command]
pub fn delete_routine(id: String) -> Result<(), String> {
    with_routines(|routines| {
        routines.retain(|r| r.id != id);
        save_routines(routines);
    });
    Ok(())
}

#[tauri::command]
pub fn get_routine_logs(id: String, limit: Option<usize>) -> Result<Vec<RoutineExecutionLog>, String> {
    Ok(read_logs(&id, limit.unwrap_or(20)))
}

#[tauri::command]
pub fn run_routine_now(id: String, app: AppHandle) -> Result<(), String> {
    if is_running(&id) {
        return Err("任务正在运行中".to_string());
    }

    let routine = with_routines(|routines| {
        routines.iter().find(|r| r.id == id).cloned()
    })
    .ok_or_else(|| format!("找不到任务: {}", id))?;

    execute_routine(&routine, &app);
    Ok(())
}
