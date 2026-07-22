use std::path::{Path, PathBuf};

use crate::config;
use crate::routines::RoutineDefinition;

/// 机器级调度资源（launchd LaunchAgents / pmset 唤醒 / schtasks）是单例的，
/// 只归「默认数据目录」的实例所有。数据目录被 MONET_DATA_DIR 重定向的实例
/// （测试/多实例场景）读的是另一套 routines，若允许其装卸，会把默认实例
/// 注册的 agent 当孤儿清掉、再把自己的注册进真实系统——双向污染
fn owns_machine_schedule() -> bool {
    std::env::var("MONET_DATA_DIR").map_or(true, |v| v.trim().is_empty())
}

pub fn register_routine(routine: &RoutineDefinition, runner_path: &Path) -> Result<(), String> {
    if !owns_machine_schedule() {
        return Ok(());
    }
    platform::register(routine, runner_path)
}

pub fn unregister_routine(routine_id: &str) -> Result<(), String> {
    if !owns_machine_schedule() {
        return Ok(());
    }
    platform::unregister(routine_id)
}

pub fn sync_all(routines: &[RoutineDefinition], runner_path: &Path) -> Result<(), String> {
    if !owns_machine_schedule() {
        return Ok(());
    }
    let known_ids: std::collections::HashSet<&str> =
        routines.iter().map(|r| r.id.as_str()).collect();
    platform::cleanup_orphans(&known_ids);

    for routine in routines {
        if routine.enabled {
            if !platform::is_registered(&routine.id) {
                platform::register(routine, runner_path)?;
            } else if platform::needs_update(routine, runner_path) {
                let _ = platform::unregister(&routine.id);
                platform::register(routine, runner_path)?;
            }
        } else if platform::is_registered(&routine.id) {
            platform::unregister(&routine.id)?;
        }
    }
    Ok(())
}

pub fn runner_binary_path() -> PathBuf {
    config::data_dir().join("bin").join(runner_bin_name())
}

pub fn install_runner_binary() -> Result<(), String> {
    let target = runner_binary_path();
    let source = bundled_runner_path();

    if !source.exists() {
        return Err(format!(
            "runner binary not found at: {}",
            source.display()
        ));
    }

    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let needs_install = if target.exists() {
        let src_meta = std::fs::metadata(&source).map_err(|e| e.to_string())?;
        let dst_meta = std::fs::metadata(&target).map_err(|e| e.to_string())?;
        src_meta.len() != dst_meta.len() || !is_codesigned(&target)
    } else {
        true
    };

    if needs_install {
        std::fs::copy(&source, &target).map_err(|e| {
            format!("failed to install runner binary: {}", e)
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755));
        }
        #[cfg(target_os = "macos")]
        crate::signing::sign(&target, "io.github.zenolab124.monet.routine-runner");
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn is_codesigned(path: &Path) -> bool {
    std::process::Command::new("codesign")
        .args(["--verify", path.to_string_lossy().as_ref()])
        .output()
        .is_ok_and(|o| o.status.success())
}

#[cfg(not(target_os = "macos"))]
fn is_codesigned(_path: &Path) -> bool {
    true
}

fn bundled_runner_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(runner_bin_name());
            if candidate.exists() {
                return candidate;
            }
        }
    }
    PathBuf::from(runner_bin_name())
}

fn runner_bin_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "monet-routine-runner.exe"
    } else {
        "monet-routine-runner"
    }
}

// ---------------------------------------------------------------------------
// Wake schedule management
// ---------------------------------------------------------------------------

pub fn sync_wake_schedule(
    routines: &[RoutineDefinition],
    policy: &str,
) -> crate::wake::SyncOutcome {
    if !owns_machine_schedule() {
        return crate::wake::SyncOutcome::Synced;
    }
    platform::sync_wake(routines, policy)
}

// Windows 专用：schtasks 的 WakeToRun 属性在 register 时需要读取策略；
// macOS 的唤醒策略读取在 routines.rs（wake_policy）
#[cfg(target_os = "windows")]
fn read_wake_policy_file() -> String {
    let path = config::data_dir().join("settings.json");
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("routineWakePolicy")?.as_str().map(String::from))
        .unwrap_or_else(|| "passive".to_string())
}

// ---------------------------------------------------------------------------
// macOS: launchd
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::fs;
    use std::process::Command;

    const LABEL_PREFIX: &str = "io.github.zenolab124.monet.routine.";
    // 旧前缀（CC Space 时期）：仅用于查找/删除兼容，不再新建
    const LEGACY_LABEL_PREFIX: &str = "com.cc-space.routine.";

    fn launch_agents_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_default()
            .join("Library")
            .join("LaunchAgents")
    }

    fn plist_path(routine_id: &str) -> PathBuf {
        launch_agents_dir().join(format!("{}{}.plist", LABEL_PREFIX, routine_id))
    }

    fn legacy_plist_path(routine_id: &str) -> PathBuf {
        launch_agents_dir().join(format!("{}{}.plist", LEGACY_LABEL_PREFIX, routine_id))
    }

    fn label(routine_id: &str) -> String {
        format!("{}{}", LABEL_PREFIX, routine_id)
    }

    pub fn is_registered(routine_id: &str) -> bool {
        // 新旧两套前缀任一在位即视为已注册
        plist_path(routine_id).exists() || legacy_plist_path(routine_id).exists()
    }

    pub fn needs_update(routine: &RoutineDefinition, runner_path: &Path) -> bool {
        let path = plist_path(&routine.id);
        let existing = match fs::read_to_string(&path) {
            Ok(s) => s,
            // 新前缀 plist 不存在：旧前缀在位则需要迁移（重建为新前缀+新路径）
            Err(_) => return true,
        };
        let calendar_intervals = match cron_to_calendar_intervals(&routine.cron_expression) {
            Ok(ci) => ci,
            Err(_) => return false,
        };
        let expected = generate_plist(
            &label(&routine.id),
            runner_path,
            &routine.id,
            &calendar_intervals,
        );
        existing.trim() != expected.trim()
    }

    pub fn cleanup_orphans(known_ids: &std::collections::HashSet<&str>) {
        let agents_dir = launch_agents_dir();
        let entries = match fs::read_dir(&agents_dir) {
            Ok(e) => e,
            Err(_) => return,
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // 新旧两套前缀都识别；孤儿一律回收（unregister 内部同时清新旧 plist）
            let id = name.strip_suffix(".plist").and_then(|stem| {
                stem.strip_prefix(LABEL_PREFIX)
                    .or_else(|| stem.strip_prefix(LEGACY_LABEL_PREFIX))
            });
            if let Some(id) = id {
                if !known_ids.contains(id) {
                    log::info!("cleaning up orphaned routine agent: {}", id);
                    let _ = unregister(id);
                }
            }
        }
    }

    pub fn register(routine: &RoutineDefinition, runner_path: &Path) -> Result<(), String> {
        let path = plist_path(&routine.id);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let calendar_intervals = cron_to_calendar_intervals(&routine.cron_expression)?;
        let plist_content = generate_plist(
            &label(&routine.id),
            runner_path,
            &routine.id,
            &calendar_intervals,
        );

        fs::write(&path, &plist_content)
            .map_err(|e| format!("failed to write plist: {}", e))?;

        let uid = Command::new("id").arg("-u").output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "501".to_string());
        let domain_target = format!("gui/{}", uid);

        let _ = Command::new("launchctl")
            .args(["bootout", &domain_target, &path.to_string_lossy()])
            .output();

        let output = Command::new("launchctl")
            .args(["bootstrap", &domain_target, &path.to_string_lossy()])
            .output()
            .map_err(|e| format!("launchctl bootstrap failed: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if !stderr.contains("already bootstrapped") {
                return Err(format!("launchctl bootstrap error: {}", stderr));
            }
        }

        Ok(())
    }

    pub fn unregister(routine_id: &str) -> Result<(), String> {
        let uid = Command::new("id").arg("-u").output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "501".to_string());
        let domain_target = format!("gui/{}", uid);

        // 新旧两套前缀都尝试卸载：bootout 各自 plist 后删文件（旧任务照常可删）
        for path in [plist_path(routine_id), legacy_plist_path(routine_id)] {
            let _ = Command::new("launchctl")
                .args(["bootout", &domain_target, &path.to_string_lossy()])
                .output();
            if path.exists() {
                let _ = fs::remove_file(&path);
            }
        }

        Ok(())
    }

    fn generate_plist(
        label: &str,
        runner_path: &Path,
        routine_id: &str,
        calendar_intervals: &str,
    ) -> String {
        let log_path = config::data_dir()
            .join("routines")
            .join("logs")
            .join(routine_id);
        let _ = std::fs::create_dir_all(&log_path);
        let stdout_log = log_path.join("launchd.log");

        let path_env = enhanced_path();

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{label}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{runner}</string>
		<string>--routine-id</string>
		<string>{routine_id}</string>
	</array>
{calendar_intervals}
	<key>RunAtLoad</key>
	<true/>
	<key>EnvironmentVariables</key>
	<dict>
		<key>PATH</key>
		<string>{path_env}</string>
	</dict>
	<key>StandardOutPath</key>
	<string>{stdout_log}</string>
	<key>StandardErrorPath</key>
	<string>{stdout_log}</string>
</dict>
</plist>
"#,
            label = label,
            runner = runner_path.display(),
            routine_id = routine_id,
            calendar_intervals = calendar_intervals,
            path_env = path_env,
            stdout_log = stdout_log.display(),
        )
    }

    fn enhanced_path() -> String {
        let home = dirs::home_dir().unwrap_or_default();
        let extra = [
            home.join(".local/bin"),
            home.join(".cargo/bin"),
            home.join(".nvm/versions/node").join("current").join("bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/opt/homebrew/bin"),
        ];
        let base = std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_string());
        let mut parts: Vec<String> = extra
            .iter()
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        parts.push(base);
        parts.join(":")
    }

    fn cron_to_calendar_intervals(cron_expr: &str) -> Result<String, String> {
        use cron::Schedule;
        use std::str::FromStr;

        let full = format!("0 {}", cron_expr);
        let schedule = Schedule::from_str(&full)
            .map_err(|e| format!("invalid cron: {}", e))?;

        #[allow(clippy::type_complexity)] // 局部临时元组，提取 type 别名增加反而不直观
        let mut entries: Vec<(u32, u32, Option<u32>, Option<u32>, Option<u32>)> = Vec::new();

        // Sample next 366 occurrences (covers a full year cycle) to find the pattern
        for dt in schedule.upcoming(chrono::Local).take(366) {
            let min = dt.minute();
            let hour = dt.hour();
            let day = dt.day();
            let month = dt.month();
            let weekday = dt.weekday().num_days_from_sunday(); // 0=Sun

            let entry = (min, hour, Some(day), Some(month), Some(weekday));
            if !entries.contains(&entry) {
                entries.push(entry);
            }
            if entries.len() > 200 {
                break;
            }
        }

        // Analyze: if all entries share the same minute+hour and vary only by date,
        // it's a simple daily/weekly pattern
        let all_same_time = entries.iter().all(|e| e.0 == entries[0].0 && e.1 == entries[0].1);
        let unique_weekdays: std::collections::HashSet<_> =
            entries.iter().filter_map(|e| e.4).collect();
        let unique_days: std::collections::HashSet<_> =
            entries.iter().filter_map(|e| e.2).collect();

        if all_same_time && unique_days.len() >= 28 {
            // Daily pattern (all days covered) — simplest: just hour+minute
            return Ok(format!(
                "\t<key>StartCalendarInterval</key>\n\t<dict>\n\t\t<key>Hour</key>\n\t\t<integer>{}</integer>\n\t\t<key>Minute</key>\n\t\t<integer>{}</integer>\n\t</dict>",
                entries[0].1, entries[0].0
            ));
        }

        if all_same_time && unique_weekdays.len() <= 7 && unique_days.len() < 28 {
            // Weekly pattern — emit one dict per weekday
            let mut intervals = String::from("\t<key>StartCalendarInterval</key>\n\t<array>\n");
            for wd in &unique_weekdays {
                intervals.push_str(&format!(
                    "\t\t<dict>\n\t\t\t<key>Hour</key>\n\t\t\t<integer>{}</integer>\n\t\t\t<key>Minute</key>\n\t\t\t<integer>{}</integer>\n\t\t\t<key>Weekday</key>\n\t\t\t<integer>{}</integer>\n\t\t</dict>\n",
                    entries[0].1, entries[0].0, wd
                ));
            }
            intervals.push_str("\t</array>");
            return Ok(intervals);
        }

        // Fallback: high-frequency or complex — use multiple calendar intervals
        // Cap at 48 entries (e.g. every 30 min = 48/day)
        let capped = &entries[..entries.len().min(48)];
        if capped.len() == 1 {
            let e = &capped[0];
            return Ok(format!(
                "\t<key>StartCalendarInterval</key>\n\t<dict>\n\t\t<key>Hour</key>\n\t\t<integer>{}</integer>\n\t\t<key>Minute</key>\n\t\t<integer>{}</integer>\n\t</dict>",
                e.1, e.0
            ));
        }

        let mut intervals = String::from("\t<key>StartCalendarInterval</key>\n\t<array>\n");
        // For sub-hourly patterns, just emit minute values
        let unique_minutes: std::collections::HashSet<_> = entries.iter().map(|e| e.0).collect();
        let unique_hours: std::collections::HashSet<_> = entries.iter().map(|e| e.1).collect();

        if unique_hours.len() == 24 {
            // Every-N-minutes pattern — emit per-minute dicts
            for min in &unique_minutes {
                intervals.push_str(&format!(
                    "\t\t<dict>\n\t\t\t<key>Minute</key>\n\t\t\t<integer>{}</integer>\n\t\t</dict>\n",
                    min
                ));
            }
        } else {
            let mut seen = std::collections::HashSet::new();
            for e in capped {
                if seen.insert((e.1, e.0)) {
                    intervals.push_str(&format!(
                        "\t\t<dict>\n\t\t\t<key>Hour</key>\n\t\t\t<integer>{}</integer>\n\t\t\t<key>Minute</key>\n\t\t\t<integer>{}</integer>\n\t\t</dict>\n",
                        e.1, e.0
                    ));
                }
            }
        }
        intervals.push_str("\t</array>");
        Ok(intervals)
    }

    // -----------------------------------------------------------------------
    // Wake schedule：实现在 crate::wake（pmset schedule 多点 + sudoers 静默）
    // -----------------------------------------------------------------------

    pub fn sync_wake(routines: &[RoutineDefinition], policy: &str) -> crate::wake::SyncOutcome {
        let cron_exprs: Vec<String> = routines
            .iter()
            .filter(|r| r.enabled)
            .map(|r| r.cron_expression.clone())
            .collect();
        crate::wake::sync(config::data_dir(), &cron_exprs, policy)
    }

    use chrono::Timelike;
    use chrono::Datelike;
}

// ---------------------------------------------------------------------------
// Windows: Task Scheduler
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn task_name(routine_id: &str) -> String {
        format!("Monet\\Routine-{}", routine_id)
    }

    // 旧任务名（CC Space 时期）：仅用于查找/删除兼容，不再新建
    fn legacy_task_name(routine_id: &str) -> String {
        format!("CC-Space\\Routine-{}", routine_id)
    }

    fn xml_path(routine_id: &str) -> PathBuf {
        config::data_dir()
            .join("routines")
            .join("tasks")
            .join(format!("{}.xml", routine_id))
    }

    pub fn is_registered(routine_id: &str) -> bool {
        // 新旧两套任务名任一存在即视为已注册
        let query = |tn: &str| {
            Command::new("schtasks")
                .args(["/Query", "/TN", tn])
                .output()
                .map_or(false, |o| o.status.success())
        };
        query(&task_name(routine_id)) || query(&legacy_task_name(routine_id))
    }

    pub fn needs_update(_routine: &RoutineDefinition, _runner_path: &Path) -> bool {
        false
    }

    pub fn cleanup_orphans(_known_ids: &std::collections::HashSet<&str>) {}

    pub fn sync_wake(routines: &[RoutineDefinition], policy: &str) -> crate::wake::SyncOutcome {
        let wake = policy == "active";
        let runner_path = super::runner_binary_path();
        for routine in routines.iter().filter(|r| r.enabled) {
            if let Ok(xml) = generate_task_xml(&runner_path, &routine.id, &routine.cron_expression, wake) {
                let xml_file = xml_path(&routine.id);
                if let Some(parent) = xml_file.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if fs::read_to_string(&xml_file).ok().as_deref() == Some(&xml) {
                    continue;
                }
                let _ = fs::write(&xml_file, &xml);
                let _ = Command::new("schtasks")
                    .args(["/Create", "/TN", &task_name(&routine.id), "/XML", &xml_file.to_string_lossy(), "/F"])
                    .output();
                // 新任务已建，删除同 id 的旧任务名，避免新旧双份被调度重复执行
                let _ = Command::new("schtasks")
                    .args(["/Delete", "/TN", &legacy_task_name(&routine.id), "/F"])
                    .output();
            }
        }
        crate::wake::SyncOutcome::Synced
    }

    pub fn register(routine: &RoutineDefinition, runner_path: &Path) -> Result<(), String> {
        let wake = super::read_wake_policy_file() == "active";
        let xml_file = xml_path(&routine.id);
        if let Some(parent) = xml_file.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let xml = generate_task_xml(runner_path, &routine.id, &routine.cron_expression, wake)?;
        fs::write(&xml_file, &xml).map_err(|e| format!("write task xml: {}", e))?;

        let output = Command::new("schtasks")
            .args([
                "/Create",
                "/TN", &task_name(&routine.id),
                "/XML", &xml_file.to_string_lossy(),
                "/F",
            ])
            .output()
            .map_err(|e| format!("schtasks create: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "schtasks error: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }

        // 新任务已建，删除同 id 的旧任务名，避免新旧双份被调度重复执行
        let _ = Command::new("schtasks")
            .args(["/Delete", "/TN", &legacy_task_name(&routine.id), "/F"])
            .output();
        Ok(())
    }

    pub fn unregister(routine_id: &str) -> Result<(), String> {
        // 新旧两套任务名都尝试删除（旧任务照常可删）
        let _ = Command::new("schtasks")
            .args(["/Delete", "/TN", &task_name(routine_id), "/F"])
            .output();
        let _ = Command::new("schtasks")
            .args(["/Delete", "/TN", &legacy_task_name(routine_id), "/F"])
            .output();
        let xml = xml_path(routine_id);
        if xml.exists() {
            let _ = fs::remove_file(&xml);
        }
        Ok(())
    }

    fn generate_task_xml(
        runner_path: &Path,
        routine_id: &str,
        cron_expr: &str,
        wake: bool,
    ) -> Result<String, String> {
        use cron::Schedule;
        use std::str::FromStr;

        let full = format!("0 {}", cron_expr);
        let schedule = Schedule::from_str(&full).map_err(|e| format!("invalid cron: {}", e))?;
        let next = schedule.upcoming(chrono::Local).next().ok_or("no next run")?;
        let start_time = next.format("%Y-%m-%dT%H:%M:%S").to_string();
        let wake_str = if wake { "true" } else { "false" };

        Ok(format!(
            r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>{start_time}</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByDay>
        <DaysInterval>1</DaysInterval>
      </ScheduleByDay>
    </CalendarTrigger>
  </Triggers>
  <Settings>
    <StartWhenAvailable>true</StartWhenAvailable>
    <WakeToRun>{wake}</WakeToRun>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
  </Settings>
  <Actions>
    <Exec>
      <Command>{runner}</Command>
      <Arguments>--routine-id {routine_id}</Arguments>
    </Exec>
  </Actions>
</Task>
"#,
            start_time = start_time,
            runner = runner_path.display(),
            routine_id = routine_id,
            wake = wake_str,
        ))
    }
}

// ---------------------------------------------------------------------------
// Linux: systemd user timer
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::fs;
    use std::process::Command;

    fn unit_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_default()
            .join(".config")
            .join("systemd")
            .join("user")
    }

    fn service_name(routine_id: &str) -> String {
        format!("monet-routine-{}.service", routine_id)
    }

    fn timer_name(routine_id: &str) -> String {
        format!("monet-routine-{}.timer", routine_id)
    }

    // 旧 unit 名（CC Space 时期）：仅用于查找/删除兼容，不再新建
    fn legacy_service_name(routine_id: &str) -> String {
        format!("cc-space-routine-{}.service", routine_id)
    }

    fn legacy_timer_name(routine_id: &str) -> String {
        format!("cc-space-routine-{}.timer", routine_id)
    }

    pub fn is_registered(routine_id: &str) -> bool {
        // 新旧两套 timer 名任一存在即视为已注册
        unit_dir().join(timer_name(routine_id)).exists()
            || unit_dir().join(legacy_timer_name(routine_id)).exists()
    }

    pub fn needs_update(_routine: &RoutineDefinition, _runner_path: &Path) -> bool {
        false
    }

    pub fn cleanup_orphans(_known_ids: &std::collections::HashSet<&str>) {}

    pub fn sync_wake(_routines: &[RoutineDefinition], _policy: &str) -> crate::wake::SyncOutcome {
        crate::wake::SyncOutcome::Synced
    }

    pub fn register(routine: &RoutineDefinition, runner_path: &Path) -> Result<(), String> {
        let dir = unit_dir();
        let _ = fs::create_dir_all(&dir);

        let service = format!(
            "[Unit]\nDescription=Monet Routine: {name}\n\n[Service]\nType=oneshot\nExecStart={runner} --routine-id {id}\n",
            name = routine.name,
            runner = runner_path.display(),
            id = routine.id,
        );

        let on_calendar = cron_to_systemd_calendar(&routine.cron_expression)?;
        let timer = format!(
            "[Unit]\nDescription=Timer for Monet Routine: {name}\n\n[Timer]\nOnCalendar={cal}\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n",
            name = routine.name,
            cal = on_calendar,
        );

        fs::write(dir.join(service_name(&routine.id)), &service)
            .map_err(|e| format!("write service: {}", e))?;
        fs::write(dir.join(timer_name(&routine.id)), &timer)
            .map_err(|e| format!("write timer: {}", e))?;

        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();

        let output = Command::new("systemctl")
            .args(["--user", "enable", "--now", &timer_name(&routine.id)])
            .output()
            .map_err(|e| format!("systemctl enable: {}", e))?;

        if !output.status.success() {
            return Err(format!(
                "systemctl enable error: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    pub fn unregister(routine_id: &str) -> Result<(), String> {
        // 新旧两套 unit 名都尝试停用+删除（旧任务照常可删）
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", &timer_name(routine_id)])
            .output();
        let _ = Command::new("systemctl")
            .args(["--user", "disable", "--now", &legacy_timer_name(routine_id)])
            .output();

        let dir = unit_dir();
        let _ = fs::remove_file(dir.join(service_name(routine_id)));
        let _ = fs::remove_file(dir.join(timer_name(routine_id)));
        let _ = fs::remove_file(dir.join(legacy_service_name(routine_id)));
        let _ = fs::remove_file(dir.join(legacy_timer_name(routine_id)));

        let _ = Command::new("systemctl")
            .args(["--user", "daemon-reload"])
            .output();

        Ok(())
    }

    fn cron_to_systemd_calendar(cron_expr: &str) -> Result<String, String> {
        let parts: Vec<&str> = cron_expr.split_whitespace().collect();
        if parts.len() != 5 {
            return Err(format!("expected 5-field cron, got {}", parts.len()));
        }
        let (min, hour, dom, _mon, dow) = (parts[0], parts[1], parts[2], parts[3], parts[4]);

        let weekday_prefix = if dow != "*" {
            let days = dow
                .split(',')
                .map(|d| match d {
                    "0" | "7" => "Sun",
                    "1" => "Mon",
                    "2" => "Tue",
                    "3" => "Wed",
                    "4" => "Thu",
                    "5" => "Fri",
                    "6" => "Sat",
                    range if range.contains('-') => range, // pass-through for ranges like 1-5
                    _ => d,
                })
                .collect::<Vec<_>>()
                .join(",");
            // Convert numeric range like 1-5 to Mon..Fri
            let days = days
                .replace("1-5", "Mon..Fri")
                .replace("0-6", "*")
                .replace("1-7", "*");
            format!("{} ", days)
        } else {
            String::new()
        };

        let day_part = if dom == "*" {
            "*-*-*".to_string()
        } else {
            format!("*-*-{}", dom.replace("*/", "1/"))
        };

        let hour_part = if hour == "*" {
            "*".to_string()
        } else {
            hour.replace("*/", "0/")
        };

        let min_part = if min == "*" {
            "*".to_string()
        } else {
            min.replace("*/", "0/")
        };

        Ok(format!("{}{}:{}:00", weekday_prefix, hour_part, min_part))
    }
}
