// 定时唤醒（macOS pmset）单一事实源：主 App（scheduler.rs 经 mod 引用）与
// monet-routine-runner（#[path] 引用）双端共享，接口只收 cron 表达式字符串，
// 不依赖 RoutineDefinition，避免 #[path] 重复引入导致的类型不等价。
//
// 授权模型：/etc/sudoers.d/monet 白名单（仅 /usr/bin/pmset），开启功能时
// osascript 提权写入一次，之后所有 pmset 读写走 `sudo -n` 静默。规则不在位时
// 绝不弹系统授权框——返回 NoAuthorization 由调用方降级。旧版授权文件在安装成功
// 后 / 撤销时一并提权清理（见 LEGACY_SUDOERS_PATH），不再新建。
//
// 唤醒机制：pmset schedule wake 一次性事件多点覆盖（pmset repeat 每类事件仅保
// 留一条，无法承载多 routine 时刻表），未来 24h 窗口滚动续设，runner 每次执行
// 完毕续设下一批形成闭环。取消必须逐条精确 cancel——schedule cancelall 会误伤
// 日历/闹钟等系统组件登记的电源事件。

use std::path::Path;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum SyncOutcome {
    /// 计划已与期望一致（含无事可做）
    Synced,
    /// policy 非 active，已尽力清理
    Disabled,
    /// sudoers 授权不在位且存在待执行的系统变更，未做任何操作
    NoAuthorization,
}

pub const SUDOERS_PATH: &str = "/etc/sudoers.d/monet";

/// 旧版（CC Space 时期）授权文件与其 staging：安装新文件成功后 / 撤销时一并提权
/// 删除，不再新建。授权状态检测靠能力探测（authorization_present 的 `sudo -n
/// pmset`），天然覆盖新旧任一文件在位的情形，无需按路径判断。
#[cfg(target_os = "macos")]
const LEGACY_SUDOERS_PATH: &str = "/etc/sudoers.d/cc-space";
#[cfg(target_os = "macos")]
const LEGACY_SUDOERS_STAGING: &str = "/etc/sudoers.d/.cc-space.staging";

#[cfg(target_os = "macos")]
mod imp {
    use super::SyncOutcome;
    use chrono::{DateTime, Duration, Local};
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::str::FromStr;

    /// 唤醒提前量：给系统从睡眠恢复留出就绪时间
    const WAKE_LEAD_MINUTES: i64 = 3;
    /// 滚动窗口：只登记未来 24h 内的唤醒点，runner/启动时续设
    const WINDOW_HOURS: i64 = 24;
    /// 单批上限：防 routine 配置异常时把系统电源事件表灌爆
    const MAX_EVENTS: usize = 16;
    /// pmset schedule 的日期格式（pmset 手册规定 "MM/dd/yy HH:mm:ss"）
    const PMSET_DATE_FMT: &str = "%m/%d/%y %H:%M:%S";

    fn cache_path(data_dir: &Path) -> PathBuf {
        data_dir.join("wake_cache.json")
    }

    // -- 授权（sudoers 白名单） ---------------------------------------------

    /// sudoers 规则是否在位：能免密执行 pmset 即视为已授权
    pub fn authorization_present() -> bool {
        Command::new("/usr/bin/sudo")
            .args(["-n", "/usr/bin/pmset", "-g"])
            .output()
            .is_ok_and(|o| o.status.success())
    }

    /// 一次提权安装 sudoers 白名单规则。校验在用户态完成（visudo -cf 只读
    /// 目标文件），提权脚本仅剩 install + mv 两步；先落 .staging（文件名含
    /// 点会被 sudoers includedir 忽略）再 mv，避免 sudo 读到半写文件。
    /// 用户取消系统弹窗返回 Err("cancelled")。
    pub fn install_authorization() -> Result<(), String> {
        // 用户名必须在用户态取好：osascript 提权环境的 $USER 是 root
        let username = Command::new("/usr/bin/id")
            .arg("-un")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty() && s != "root")
            .ok_or("cannot determine current username")?;

        let rule = format!("{} ALL=(root) NOPASSWD: /usr/bin/pmset\n", username);

        let tmp = std::env::temp_dir().join(format!("monet-sudoers-{}", std::process::id()));
        std::fs::write(&tmp, &rule).map_err(|e| format!("write sudoers tmp: {}", e))?;

        let check = Command::new("/usr/sbin/visudo")
            .args(["-cf", &tmp.to_string_lossy()])
            .output()
            .map_err(|e| format!("visudo spawn: {}", e))?;
        if !check.status.success() {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!(
                "sudoers rule failed validation: {}",
                String::from_utf8_lossy(&check.stderr)
            ));
        }

        // 安装新文件成功后（&&），best-effort 提权删除旧版文件；rm 失败不阻断整条命令
        // （{{ ... || true; }} 把失败吞掉），且只在 install+mv 成功后才执行，全程一次弹窗。
        let shell_cmd = format!(
            "/usr/bin/install -m 0440 -o root -g wheel '{tmp}' '{staging}' && /bin/mv -f '{staging}' '{dest}' && {{ /bin/rm -f '{legacy}' '{legacy_staging}' || true; }}",
            tmp = tmp.to_string_lossy(),
            staging = "/etc/sudoers.d/.monet.staging",
            dest = super::SUDOERS_PATH,
            legacy = super::LEGACY_SUDOERS_PATH,
            legacy_staging = super::LEGACY_SUDOERS_STAGING,
        );
        let result = run_privileged(&shell_cmd);
        let _ = std::fs::remove_file(&tmp);
        result
    }

    /// 提权移除 sudoers 规则（用户在设置页主动点击，弹窗在预期内）
    pub fn remove_authorization() -> Result<(), String> {
        // 撤销：新旧授权文件与两处 staging 一并删除（一次提权弹窗，rm -f 幂等）
        run_privileged(&format!(
            "/bin/rm -f '{}' '{}' '{}' '{}'",
            super::SUDOERS_PATH,
            super::LEGACY_SUDOERS_PATH,
            "/etc/sudoers.d/.monet.staging",
            super::LEGACY_SUDOERS_STAGING,
        ))
    }

    /// osascript 提权执行 shell 命令。用户点「不允许」→ Err("cancelled")
    fn run_privileged(shell_cmd: &str) -> Result<(), String> {
        let script = format!(
            "do shell script \"{}\" with administrator privileges",
            shell_cmd.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let output = Command::new("/usr/bin/osascript")
            .args(["-e", &script])
            .output()
            .map_err(|e| format!("osascript spawn: {}", e))?;

        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        // AppleScript 用户取消错误号 -128，跨系统语言稳定
        if stderr.contains("-128") {
            return Err("cancelled".to_string());
        }
        Err(format!("privileged exec failed: {}", stderr.trim()))
    }

    // -- 唤醒时刻计算 --------------------------------------------------------

    /// 未来 24h 窗口内所有唤醒点：各 cron 触发时刻减提前量，去重排序封顶。
    /// 起点偏移 90s：避免登记一个设置完成前就过期的时刻。
    pub fn compute_wake_datetimes(cron_exprs: &[String]) -> Vec<DateTime<Local>> {
        let now = Local::now();
        let horizon = now + Duration::hours(WINDOW_HOURS);
        let floor = now + Duration::seconds(90);

        let mut times: Vec<DateTime<Local>> = Vec::new();
        for expr in cron_exprs {
            // 项目 cron 为 5 段（分钟起），cron crate 需 6 段（秒起）
            let full = format!("0 {}", expr);
            let Ok(schedule) = cron::Schedule::from_str(&full) else {
                continue;
            };
            for dt in schedule.upcoming(Local) {
                if dt > horizon {
                    break;
                }
                let wake = dt - Duration::minutes(WAKE_LEAD_MINUTES);
                if wake >= floor {
                    times.push(wake);
                }
            }
        }
        times.sort();
        times.dedup();
        times.truncate(MAX_EVENTS);
        times
    }

    // -- 缓存（v2：绝对时刻；v1 遗留：每日 (h,m) 数组，读到即触发 repeat 清理）

    struct CacheState {
        scheduled: Vec<DateTime<Local>>,
        /// v1 缓存痕迹：旧实现用 pmset repeat，需要一次 repeat cancel 清遗留
        legacy_repeat: bool,
    }

    fn read_cache(data_dir: &Path) -> CacheState {
        let raw = match std::fs::read_to_string(cache_path(data_dir)) {
            Ok(s) => s,
            Err(_) => {
                return CacheState { scheduled: Vec::new(), legacy_repeat: false }
            }
        };
        let value: serde_json::Value = match serde_json::from_str(&raw) {
            Ok(v) => v,
            Err(_) => {
                return CacheState { scheduled: Vec::new(), legacy_repeat: false }
            }
        };

        // v1：裸数组 [[h,m],...]（旧 pmset repeat 实现的缓存）
        if value.is_array() {
            return CacheState { scheduled: Vec::new(), legacy_repeat: true };
        }

        let scheduled = value
            .get("scheduled")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str())
                    .filter_map(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Local))
                    .collect()
            })
            .unwrap_or_default();
        CacheState { scheduled, legacy_repeat: false }
    }

    fn write_cache(data_dir: &Path, times: &[DateTime<Local>]) {
        let json = serde_json::json!({
            "version": 2,
            "scheduled": times.iter().map(|t| t.to_rfc3339()).collect::<Vec<_>>(),
        });
        let path = cache_path(data_dir);
        let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
        if std::fs::write(&tmp, json.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    // -- 同步主流程 ----------------------------------------------------------

    pub fn sync(data_dir: &Path, cron_exprs: &[String], policy: &str) -> SyncOutcome {
        if policy != "active" {
            return teardown(data_dir);
        }

        let expected = compute_wake_datetimes(cron_exprs);
        if expected.is_empty() {
            return teardown(data_dir);
        }

        let cache = read_cache(data_dir);
        let now = Local::now();
        // 缓存中已过期的事件系统会自动丢弃，不参与差量
        let live: Vec<DateTime<Local>> =
            cache.scheduled.iter().copied().filter(|t| *t > now).collect();

        let to_add: Vec<&DateTime<Local>> =
            expected.iter().filter(|t| !live.contains(t)).collect();
        let to_cancel: Vec<&DateTime<Local>> =
            live.iter().filter(|t| !expected.contains(t)).collect();

        if to_add.is_empty() && to_cancel.is_empty() && !cache.legacy_repeat {
            return SyncOutcome::Synced;
        }

        if !authorization_present() {
            return SyncOutcome::NoAuthorization;
        }

        if cache.legacy_repeat {
            run_pmset(&["repeat", "cancel"]);
        }
        for t in &to_cancel {
            let stamp = t.format(PMSET_DATE_FMT).to_string();
            run_pmset(&["schedule", "cancel", "wake", &stamp]);
        }
        for t in &to_add {
            let stamp = t.format(PMSET_DATE_FMT).to_string();
            run_pmset(&["schedule", "wake", &stamp]);
        }

        write_cache(data_dir, &expected);
        log::info!("wake schedule synced: {} events", expected.len());
        SyncOutcome::Synced
    }

    /// 清空我们登记的唤醒计划。授权不在位时只清本地缓存——一次性事件至多
    /// 24h 内自然过期，无长期残留
    fn teardown(data_dir: &Path) -> SyncOutcome {
        let cache = read_cache(data_dir);
        let now = Local::now();
        let live: Vec<&DateTime<Local>> =
            cache.scheduled.iter().filter(|t| **t > now).collect();

        if (live.is_empty() && !cache.legacy_repeat) || !authorization_present() {
            if !cache.scheduled.is_empty() || cache.legacy_repeat {
                write_cache(data_dir, &[]);
            }
            return SyncOutcome::Disabled;
        }

        if cache.legacy_repeat {
            run_pmset(&["repeat", "cancel"]);
        }
        for t in live {
            let stamp = t.format(PMSET_DATE_FMT).to_string();
            run_pmset(&["schedule", "cancel", "wake", &stamp]);
        }
        write_cache(data_dir, &[]);
        SyncOutcome::Disabled
    }

    /// sudo -n pmset 单条执行，失败仅记日志（授权已预检，残余失败属 pmset
    /// 自身问题，不值得让整轮同步中断）
    fn run_pmset(args: &[&str]) {
        let mut cmd = Command::new("/usr/bin/sudo");
        cmd.args(["-n", "/usr/bin/pmset"]);
        cmd.args(args);
        match cmd.output() {
            Ok(o) if !o.status.success() => {
                log::warn!(
                    "pmset {:?} failed: {}",
                    args,
                    String::from_utf8_lossy(&o.stderr).trim()
                );
            }
            Err(e) => log::warn!("pmset {:?} spawn failed: {}", args, e),
            _ => {}
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::SyncOutcome;
    use std::path::Path;

    pub fn authorization_present() -> bool {
        true
    }
    pub fn install_authorization() -> Result<(), String> {
        Ok(())
    }
    pub fn remove_authorization() -> Result<(), String> {
        Ok(())
    }
    pub fn sync(_data_dir: &Path, _cron_exprs: &[String], _policy: &str) -> SyncOutcome {
        SyncOutcome::Synced
    }
}

pub fn authorization_present() -> bool {
    imp::authorization_present()
}

pub fn install_authorization() -> Result<(), String> {
    imp::install_authorization()
}

pub fn remove_authorization() -> Result<(), String> {
    imp::remove_authorization()
}

pub fn sync(data_dir: &Path, cron_exprs: &[String], policy: &str) -> SyncOutcome {
    imp::sync(data_dir, cron_exprs, policy)
}
