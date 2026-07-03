use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

use chrono::Utc;
use serde::{Deserialize, Serialize};

// 与主 App 共享同一份定位逻辑源文件（单一事实源）；
// runner 不引 app_lib 整个 crate，避免把 tauri 链进这个轻量二进制。
// allow(dead_code)：runner 只消费 locate_lightweight，其余入口是主 App 用的
#[path = "../claude_locator.rs"]
#[allow(dead_code)]
mod claude_locator;

// TCC 权限检测（--health-check 模式），与主 App 共享同一份源文件
#[path = "../tcc.rs"]
#[allow(dead_code)]
mod tcc;

// Routine 结构单一事实源：runner 的 update_routine_state 会整文件重写
// routines.json，本地副本缺字段会抹掉其他写者（UI/MCP）的数据
#[path = "../routine_types.rs"]
#[allow(dead_code)]
mod routine_types;
use routine_types::RoutineDefinition;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExecutionLog {
    routine_id: String,
    started_at: String,
    finished_at: Option<String>,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

fn main() {
    // 权限体检模式：由主 App 经 launchd 触发（与真实定时任务相同的 TCC
    // 归因语境），自检后写结果文件退出。--prompt <kind> 时对指定权限发起
    // 请求式调用（弹系统授权窗，用户在权限页面点击驱动）
    if env::args().any(|a| a == "--health-check") {
        let args: Vec<String> = env::args().collect();
        let prompt = args
            .iter()
            .position(|a| a == "--prompt")
            .and_then(|i| args.get(i + 1))
            .cloned();
        run_health_check(prompt.as_deref());
        return;
    }

    let routine_id = parse_args();

    let routines = load_routines();
    let routine = routines
        .iter()
        .find(|r| r.id == routine_id)
        .unwrap_or_else(|| {
            eprintln!("routine not found: {}", routine_id);
            std::process::exit(1);
        })
        .clone();

    if !routine.enabled {
        eprintln!("routine is disabled, skipping");
        std::process::exit(0);
    }

    // File lock to prevent concurrent execution
    let lock_path = locks_dir().join(format!("{}.lock", routine_id));
    let _ = fs::create_dir_all(lock_path.parent().unwrap());
    let lock_file = fs::File::create(&lock_path).unwrap_or_else(|e| {
        eprintln!("cannot create lock file: {}", e);
        std::process::exit(1);
    });

    use fs2::FileExt;
    if lock_file.try_lock_exclusive().is_err() {
        eprintln!("another instance is running, skipping");
        std::process::exit(0);
    }

    // Dedup: check if already ran in current cron period
    if should_skip(&routine) {
        eprintln!("already ran in current period, skipping");
        std::process::exit(0);
    }

    // Execute：launchd 环境贫瘠，只走轻量探测（手动配置/缓存/候选扫描），
    // login shell 重探测由主 App 负责并通过缓存共享结果
    let started_at = Utc::now().to_rfc3339();

    let output = match claude_locator::locate_lightweight() {
        Ok(located) => Command::new(&located.path)
            .arg("-p")
            .arg(&routine.prompt)
            .arg("--output-format")
            .arg("text")
            .arg("--no-session-persistence")
            .current_dir(agent_cwd())
            .output(),
        Err(e) => Err(std::io::Error::new(std::io::ErrorKind::NotFound, e)),
    };

    let finished_at = Utc::now().to_rfc3339();

    let log = match output {
        Ok(out) => ExecutionLog {
            routine_id: routine_id.clone(),
            started_at: started_at.clone(),
            finished_at: Some(finished_at),
            exit_code: out.status.code(),
            stdout: truncate(&String::from_utf8_lossy(&out.stdout), 10240),
            stderr: truncate(&String::from_utf8_lossy(&out.stderr), 4096),
        },
        Err(e) => ExecutionLog {
            routine_id: routine_id.clone(),
            started_at: started_at.clone(),
            finished_at: Some(finished_at),
            exit_code: Some(-1),
            stdout: String::new(),
            stderr: format!("spawn failed: {}", e),
        },
    };

    write_log(&log);
    update_routine_state(&routine_id, &started_at);
    maybe_sleep_after_run();
}

fn run_health_check(prompt: Option<&str>) {
    // 预热 System Events：open 走 LaunchServices 不需要自动化权限，
    // 避免 AE 查询因目标未运行返回 procNotFound
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open")
            .args(["-g", "-a", "System Events"])
            .output();
        std::thread::sleep(std::time::Duration::from_millis(600));
        // 请求式调用：弹系统授权窗并阻塞至用户响应，随后照常快照
        match prompt {
            Some("automationSystemEvents") => {
                let _ = tcc::check_automation("com.apple.systemevents", true);
            }
            Some("accessibility") => {
                let _ = tcc::prompt_accessibility();
            }
            Some("screenCapture") => {
                let _ = tcc::request_screen_capture();
            }
            _ => {}
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = prompt;
    let result = serde_json::json!({
        "checkedAt": Utc::now().to_rfc3339(),
        "permissions": {
            "automationSystemEvents": tcc::check_automation("com.apple.systemevents", false),
            "accessibility": tcc::check_accessibility(),
            "screenCapture": tcc::check_screen_capture(),
            "fullDiskAccess": tcc::check_full_disk_access(),
        },
    });
    // 结果路径与主 App 读取侧硬编码一致（launchd 语境无 CC_SPACE_DATA_DIR）
    let dir = dirs::home_dir().unwrap_or_default().join(".cc-space");
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(
        dir.join("permissions-runner.json"),
        serde_json::to_string_pretty(&result).unwrap_or_default(),
    );
}

fn parse_args() -> String {
    let args: Vec<String> = env::args().collect();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--routine-id" && i + 1 < args.len() {
            return args[i + 1].clone();
        }
        i += 1;
    }
    eprintln!("usage: cc-space-routine-runner --routine-id <uuid>");
    std::process::exit(1);
}

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CC_SPACE_DATA_DIR") {
        PathBuf::from(dir)
    } else {
        dirs::home_dir().unwrap_or_default().join(".cc-space")
    }
}

fn routines_path() -> PathBuf {
    data_dir().join("routines.json")
}

fn logs_dir(routine_id: &str) -> PathBuf {
    data_dir()
        .join("routines")
        .join("logs")
        .join(routine_id)
}

fn locks_dir() -> PathBuf {
    data_dir().join("routines").join("locks")
}

fn agent_cwd() -> PathBuf {
    let p = data_dir().join("agent");
    let _ = fs::create_dir_all(&p);
    p
}

fn load_routines() -> Vec<RoutineDefinition> {
    let path = routines_path();
    if !path.exists() {
        eprintln!("routines.json not found");
        std::process::exit(1);
    }
    let content = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("read routines.json: {}", e);
        std::process::exit(1);
    });
    serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("parse routines.json: {}", e);
        std::process::exit(1);
    })
}

fn should_skip(routine: &RoutineDefinition) -> bool {
    let last_run = match &routine.last_run {
        Some(lr) => lr,
        None => return false,
    };

    let last_run_dt = match chrono::DateTime::parse_from_rfc3339(last_run) {
        Ok(dt) => dt.with_timezone(&chrono::Local),
        Err(_) => return false,
    };

    // Find the previous scheduled time before now
    use cron::Schedule;
    use std::str::FromStr;

    let full = format!("0 {}", routine.cron_expression);
    let schedule = match Schedule::from_str(&full) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let now = chrono::Local::now();
    // Get upcoming times and find the one just before now
    // by getting the next occurrence and subtracting one period
    let mut prev = None;
    // Walk backwards: get many upcoming from a past point
    let past = now - chrono::Duration::days(2);
    for dt in schedule.after(&past) {
        if dt > now {
            break;
        }
        prev = Some(dt);
    }

    match prev {
        Some(prev_scheduled) => last_run_dt >= prev_scheduled,
        None => false,
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        s[..max].to_string()
    }
}

fn write_log(log: &ExecutionLog) {
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

fn update_routine_state(routine_id: &str, last_run: &str) {
    let path = routines_path();

    // Read-modify-write with file lock
    let file = match fs::File::open(&path) {
        Ok(f) => f,
        Err(_) => return,
    };

    use fs2::FileExt;
    let _ = file.lock_exclusive();

    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut routines: Vec<RoutineDefinition> = match serde_json::from_str(&content) {
        Ok(r) => r,
        Err(_) => return,
    };

    if let Some(r) = routines.iter_mut().find(|r| r.id == routine_id) {
        r.last_run = Some(last_run.to_string());
        r.next_run = compute_next_run(&r.cron_expression);
    }

    // 原子写：锁只保证写者互斥，主 App/MCP 的读者不持锁，裸写仍有撕裂窗口
    if let Ok(json) = serde_json::to_string_pretty(&routines) {
        let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }

    let _ = file.unlock();
}

fn compute_next_run(cron_expr: &str) -> Option<String> {
    use cron::Schedule;
    use std::str::FromStr;

    let full = format!("0 {}", cron_expr);
    let schedule = Schedule::from_str(&full).ok()?;
    let next = schedule.upcoming(chrono::Local).next()?;
    Some(next.to_rfc3339())
}

fn maybe_sleep_after_run() {
    if read_wake_policy() != "active" {
        return;
    }
    if !is_lid_closed() {
        return;
    }
    // 有即将执行的 routine 则不休眠（5 分钟内）
    if has_imminent_routine() {
        return;
    }
    eprintln!("active wake: lid closed, sleeping");
    let _ = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to sleep")
        .output();
}

fn read_wake_policy() -> String {
    let path = data_dir().join("settings.json");
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("routineWakePolicy")?.as_str().map(String::from))
        .unwrap_or_else(|| "passive".to_string())
}

fn is_lid_closed() -> bool {
    Command::new("ioreg")
        .args(["-r", "-k", "AppleClamshellState"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("\"AppleClamshellState\" = Yes"))
        .unwrap_or(false)
}

fn has_imminent_routine() -> bool {
    use cron::Schedule;
    use std::str::FromStr;

    let routines = load_routines();
    let now = chrono::Local::now();
    let threshold = now + chrono::Duration::minutes(5);

    routines.iter().filter(|r| r.enabled).any(|r| {
        let full = format!("0 {}", r.cron_expression);
        Schedule::from_str(&full)
            .ok()
            .and_then(|s| s.upcoming(chrono::Local).next())
            .is_some_and(|next| next <= threshold)
    })
}
