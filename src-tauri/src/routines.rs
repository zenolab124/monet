use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::config;
use crate::scheduler;

// ---------------------------------------------------------------------------
// Data structures（RoutineDefinition/RoutineSource 见 routine_types.rs 单一事实源）
// ---------------------------------------------------------------------------

pub use crate::routine_types::{RoutineDefinition, RoutineSource};

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
    /// 正在运行时的开始时刻（RFC3339），供前端显示已耗时
    pub running_started_at: Option<String>,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

static ROUTINES: Mutex<Option<Vec<RoutineDefinition>>> = Mutex::new(None);
/// 运行中任务：id → 开始时刻（RFC3339）
static RUNNING: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

// ---------------------------------------------------------------------------
// File paths
// ---------------------------------------------------------------------------

fn routines_path() -> PathBuf {
    config::data_dir().join("routines.json")
}

fn app_settings_path() -> PathBuf {
    config::data_dir().join("settings.json")
}

// ---------------------------------------------------------------------------
// App settings (wake policy etc.)
// ---------------------------------------------------------------------------

fn read_app_setting(key: &str) -> Option<serde_json::Value> {
    let content = fs::read_to_string(app_settings_path()).ok()?;
    let settings: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&content).ok()?;
    settings.get(key).cloned()
}

fn write_app_setting(key: &str, value: serde_json::Value) {
    let path = app_settings_path();
    // 文件存在但解析失败时拒绝覆写，避免带着空 map 清掉其他设置键
    let mut settings: serde_json::Map<String, serde_json::Value> =
        match fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(m) => m,
                Err(_) => return,
            },
            Err(_) => Default::default(),
        };
    if value.is_null() {
        settings.remove(key);
    } else {
        settings.insert(key.to_string(), value);
    }
    if let Ok(json) = serde_json::to_string_pretty(&serde_json::Value::Object(settings)) {
        let _ = crate::config::atomic_write(&path, &json);
    }
}

pub fn wake_policy() -> String {
    read_app_setting("routineWakePolicy")
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "passive".to_string())
}

fn logs_dir(routine_id: &str) -> PathBuf {
    config::data_dir()
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
    if let Ok(json) = serde_json::to_string_pretty(data) {
        let _ = crate::config::atomic_write(&routines_path(), &json);
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

pub fn invalidate_cache() {
    let mut guard = ROUTINES.lock().unwrap();
    *guard = None;
}

pub fn sync_scheduler() {
    let runner_path = scheduler::runner_binary_path();
    let routines_snapshot: Vec<RoutineDefinition> = with_routines(|routines| routines.clone());
    if let Err(e) = scheduler::sync_all(&routines_snapshot, &runner_path) {
        log::warn!("routine scheduler sync (external change): {}", e);
    }
}

fn is_running(id: &str) -> bool {
    RUNNING
        .lock()
        .unwrap()
        .as_ref()
        .map_or(false, |s| s.contains_key(id))
}

fn running_started_at(id: &str) -> Option<String> {
    RUNNING
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|s| s.get(id).cloned())
}

fn set_running(id: &str, started_at: Option<&str>) {
    let mut guard = RUNNING.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    match started_at {
        Some(at) => {
            map.insert(id.to_string(), at.to_string());
        }
        None => {
            map.remove(id);
        }
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

fn agent_cwd() -> PathBuf {
    let p = config::data_dir().join("agent");
    let _ = fs::create_dir_all(&p);
    p
}

fn execute_routine(routine: &RoutineDefinition, app: &AppHandle) {
    let id = routine.id.clone();
    let name = routine.name.clone();
    let prompt = routine.prompt.clone();
    let app = app.clone();

    let started_at = Utc::now().to_rfc3339();
    let t0 = std::time::Instant::now();
    set_running(&id, Some(&started_at));

    // 开始事件：前端据此立即刷新出「运行中」状态
    let _ = app.emit("routine-started", serde_json::json!({
        "routineId": &id,
        "name": &name,
        "startedAt": &started_at,
    }));

    tauri::async_runtime::spawn_blocking(move || {

        // .app 环境 PATH 极简，裸命令名找不到 claude，必须走 locator 显式定位
        let output = match crate::claude_locator::locate() {
            Ok(located) => Command::new(&located.path)
                .arg("-p")
                .arg(&prompt)
                .arg("--output-format")
                .arg("text")
                .arg("--no-session-persistence")
                .env("PATH", crate::streaming::enhanced_path())
                .current_dir(agent_cwd())
                .output(),
            Err(e) => Err(std::io::Error::new(std::io::ErrorKind::NotFound, e)),
        };

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

        set_running(&id, None);

        // 完成事件带结果概要：前端据此弹完成/失败 toast，无需再查日志
        let _ = app.emit("routine-executed", serde_json::json!({
            "routineId": id,
            "name": name,
            "exitCode": log.exit_code,
            "durationMs": t0.elapsed().as_millis() as u64,
        }));
    });
}

// ---------------------------------------------------------------------------
// System scheduler sync (replaces tick loop)
// ---------------------------------------------------------------------------

pub fn startup_sync() {
    // Install runner binary to stable path
    if let Err(e) = scheduler::install_runner_binary() {
        log::warn!("routine runner install failed: {}", e);
        return;
    }

    let runner_path = scheduler::runner_binary_path();

    // Ensure all enabled routines have next_run computed
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

    // Sync with OS scheduler
    let routines_snapshot: Vec<RoutineDefinition> =
        with_routines(|routines| routines.clone());

    if let Err(e) = scheduler::sync_all(&routines_snapshot, &runner_path) {
        log::warn!("routine scheduler sync failed: {}", e);
    }

    // Sync wake schedule
    scheduler::sync_wake_schedule(&routines_snapshot, &wake_policy());
}

fn sync_wake_if_active() {
    let policy = wake_policy();
    if policy == "active" {
        let snapshot: Vec<RoutineDefinition> = with_routines(|r| r.clone());
        scheduler::sync_wake_schedule(&snapshot, &policy);
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
                running_started_at: running_started_at(&r.id),
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
        source: Some(RoutineSource::ui()),
    };

    with_routines(|routines| {
        routines.push(routine.clone());
        save_routines(routines);
    });

    if routine.enabled {
        let runner_path = scheduler::runner_binary_path();
        if let Err(e) = scheduler::register_routine(&routine, &runner_path) {
            log::warn!("scheduler register failed: {}", e);
        }
    }

    sync_wake_if_active();
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

        if cron_changed {
            r.last_run = None;
        }
        if (cron_changed || enabled_changed) && r.enabled {
            r.next_run = compute_next_run_full(&r.cron_expression);
        } else if !r.enabled {
            r.next_run = None;
        }

        let result = r.clone();
        save_routines(routines);

        // Sync system scheduler
        if cron_changed || enabled_changed {
            let runner_path = scheduler::runner_binary_path();
            if result.enabled {
                let _ = scheduler::unregister_routine(&result.id);
                if let Err(e) = scheduler::register_routine(&result, &runner_path) {
                    log::warn!("scheduler re-register failed: {}", e);
                }
            } else {
                let _ = scheduler::unregister_routine(&result.id);
            }
            sync_wake_if_active();
        }

        Ok(result)
    })
}

#[tauri::command]
pub fn delete_routine(id: String) -> Result<(), String> {
    if let Err(e) = scheduler::unregister_routine(&id) {
        log::warn!("scheduler unregister failed: {}", e);
    }

    with_routines(|routines| {
        routines.retain(|r| r.id != id);
        save_routines(routines);
    });
    sync_wake_if_active();
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

#[tauri::command]
pub fn get_routine_wake_policy() -> String {
    wake_policy()
}

#[tauri::command]
pub fn set_routine_wake_policy(policy: String) -> Result<(), String> {
    if policy != "passive" && policy != "active" {
        return Err("无效策略，支持: passive | active".to_string());
    }
    write_app_setting(
        "routineWakePolicy",
        serde_json::Value::String(policy.clone()),
    );

    let snapshot: Vec<RoutineDefinition> = with_routines(|r| r.clone());
    scheduler::sync_wake_schedule(&snapshot, &policy);

    Ok(())
}
