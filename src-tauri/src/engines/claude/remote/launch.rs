//! 远端会话进程：启动脚本、远端 pid 登记与清理、远端进程列表。
//!
//! 远端只执行三类命令：读（ps）、启动 claude、kill Monet 自己记录的 pid。
//! 启动脚本先输出远端 pid 再 exec，Monet 据此在会话关闭、断线或下次连接时精确清理。

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::executor::{sh_quote, RemoteExecutor, RemotePlatform};
use super::hosts::{self, RemoteHost, SshTarget};
use super::{health, sync};
use crate::engines::core::EngineInstanceId;

/// 启动脚本输出的第一行：远端 claude 的 pid（exec 前后 pid 不变）
pub const PID_MARKER: &str = "MONET_REMOTE_PID ";
/// 远端工作目录不存在时脚本输出这一行并退出
pub const CWD_MISSING_MARKER: &str = "MONET_REMOTE_ERROR cwd_missing";

const PS_TIMEOUT: Duration = Duration::from_secs(15);
const KILL_TIMEOUT: Duration = Duration::from_secs(15);
/// 同一主机的进程列表在这个窗口内只拉一次，所有打开的会话共用
const PS_CACHE_TTL: Duration = Duration::from_secs(5);
const PIDS_FILE: &str = "pids.json";
/// 远端 CLI 路径缓存：每条消息都可能重新解析启动信息，体检要一次往返
const CLI_CACHE_TTL: Duration = Duration::from_secs(300);

/// (实例 id, 体检时刻, CLI 路径, 远端方言)
static CLI_CACHE: Mutex<Vec<(String, Instant, String, RemotePlatform)>> = Mutex::new(Vec::new());

/// 一次远端启动所需的全部信息（启动前在后台线程解析好）
#[derive(Debug, Clone)]
pub struct RemoteLaunch {
    pub instance: EngineInstanceId,
    pub alias: String,
    pub target: SshTarget,
    /// 远端 CLI 的绝对路径
    pub claude: String,
    /// CLI 的会话 id（原生 uuid）；Monet 内部的进程键可能是结构化 id
    pub cli_session_id: String,
    /// 镜像里已有这个会话：用 `--resume`，否则用 `--session-id` 新建
    pub resume: bool,
    pub cwd: String,
    /// 调试版回环：用本机 shell 代替 ssh
    pub loopback: bool,
}

/// 远端启动脚本：先报 pid，再进工作目录，经登录 shell（拿到用户 PATH，npm 安装的 CLI 才找得到 node）
/// exec 到 claude。所有可变部分都单引号转义；命令行里不出现任何凭据。
pub fn launch_script(cwd: &str, claude: &str, args: &[String]) -> String {
    let quoted_args: Vec<String> = args.iter().map(|arg| sh_quote(arg)).collect();
    format!(
        r#"printf '{PID_MARKER}%s\n' "$$"
cd {cwd} 2>/dev/null || {{ printf '%s\n' '{CWD_MISSING_MARKER}'; exit 97; }}
{env}=true
export {env}
login_sh="${{SHELL:-/bin/sh}}"
exec "$login_sh" -lc 'exec "$0" "$@"' {claude} {args}
"#,
        cwd = sh_quote(cwd),
        env = crate::engines::claude::protocol::FILE_CHECKPOINTING_ENV,
        claude = sh_quote(claude),
        args = quoted_args.join(" "),
    )
}

/// 解析启动脚本的标记行；不是标记时返回 None
pub fn parse_marker(line: &str) -> Option<Result<u32, &'static str>> {
    let line = line.trim();
    if line == CWD_MISSING_MARKER {
        return Some(Err("REMOTE_CWD_MISSING"));
    }
    line.strip_prefix(PID_MARKER.trim_end())
        .map(|pid| pid.trim().parse().map_err(|_| "REMOTE_PID_INVALID"))
}

fn host_for(instance: &EngineInstanceId) -> Result<(RemoteHost, SshTarget), String> {
    let host = hosts::list_hosts()
        .into_iter()
        .find(|h| h.instance_id == instance.instance_id())
        .ok_or("REMOTE_UNKNOWN_HOST")?;
    let target = hosts::target_for_host(&host).ok_or("REMOTE_INVALID_CONNECTION")?;
    Ok((host, target))
}

/// 镜像里是否已有这个会话（CLI 落盘后才能 `--resume`）
fn mirrored(instance_id: &str, cli_session_id: &str) -> bool {
    let suffix = format!("/{cli_session_id}.jsonl");
    sync::Mirror::new(sync::mirror_root_for(instance_id))
        .load_state()
        .files
        .iter()
        .any(|(rel, state)| rel.ends_with(&suffix) && !state.missing)
}

/// 启动前解析：找主机、定位远端 CLI、判断续聊还是新建。阻塞（可能做一次体检），在后台线程调用。
pub fn resolve(
    instance: &EngineInstanceId,
    cli_session_id: &str,
    cwd: &str,
) -> Result<RemoteLaunch, String> {
    if !sync::valid_rel(&format!("p/{cli_session_id}.jsonl")) {
        return Err("REMOTE_INVALID_SESSION".into());
    }
    let (host, mut target) = host_for(instance)?;
    let loopback = super::loopback_root().is_some();
    let claude = if loopback {
        crate::claude_locator::locate_lightweight()?
            .path
            .to_string_lossy()
            .into_owned()
    } else if let Some((path, platform)) = cached_cli(&host.instance_id) {
        target.platform = platform;
        path
    } else {
        let report = health::check(
            &super::executor::SshExecutor,
            &target,
            host.claude_path.as_deref(),
        );
        if !report.usable {
            return Err(format!(
                "REMOTE_CLI_UNAVAILABLE:{}",
                report
                    .failure
                    .map(|kind| format!("{kind:?}").to_lowercase())
                    .unwrap_or_default()
            ));
        }
        let path = report.cli_path.ok_or("REMOTE_CLI_UNAVAILABLE:")?;
        target.platform = report.platform;
        let mut cache = CLI_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|(id, ..)| id != &host.instance_id);
        cache.push((
            host.instance_id.clone(),
            Instant::now(),
            path.clone(),
            report.platform,
        ));
        path
    };
    Ok(RemoteLaunch {
        instance: instance.clone(),
        alias: host.alias.clone(),
        target,
        claude,
        cli_session_id: cli_session_id.to_string(),
        resume: mirrored(&host.instance_id, cli_session_id),
        cwd: cwd.to_string(),
        loopback,
    })
}

fn cached_cli(instance_id: &str) -> Option<(String, RemotePlatform)> {
    CLI_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(id, at, ..)| id == instance_id && at.elapsed() < CLI_CACHE_TTL)
        .map(|(_, _, path, platform)| (path.clone(), *platform))
}

// ---------- 远端 pid 登记：Monet 异常退出或断线后，下一轮同步或重开该会话时据此清理 ----------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RecordedPid {
    pid: u32,
    session: String,
}

fn pids_path(instance_id: &str) -> PathBuf {
    sync::mirror_root_for(instance_id).join(PIDS_FILE)
}

static PIDS_LOCK: Mutex<()> = Mutex::new(());

fn update_pids(instance_id: &str, change: impl FnOnce(&mut Vec<RecordedPid>)) {
    let _guard = PIDS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let path = pids_path(instance_id);
    let mut pids: Vec<RecordedPid> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    change(&mut pids);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string(&pids) {
        let _ = crate::config::atomic_write(&path, &text);
    }
}

/// 本次运行中 ssh 仍连着的远端进程（实例 id, pid）：清理时一律跳过。
/// 先登记这里再写 pids.json，清理读文件后再查这里，避免把刚启动的会话当成遗留。
static LIVE_PIDS: Mutex<Vec<(String, u32)>> = Mutex::new(Vec::new());

fn live_pids_for(instance_id: &str) -> Vec<u32> {
    LIVE_PIDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .filter(|(id, _)| id == instance_id)
        .map(|(_, pid)| *pid)
        .collect()
}

pub fn record_pid(instance_id: &str, pid: u32, cli_session_id: &str) {
    {
        let mut live = LIVE_PIDS.lock().unwrap_or_else(|e| e.into_inner());
        if !live.iter().any(|(id, p)| id == instance_id && *p == pid) {
            live.push((instance_id.to_string(), pid));
        }
    }
    update_pids(instance_id, |pids| {
        pids.retain(|p| p.pid != pid);
        pids.push(RecordedPid {
            pid,
            session: cli_session_id.to_string(),
        });
    });
}

/// 本机这一侧的 ssh 已退出：远端进程不再算存活。断线时远端可能还在跑，记录留给下一轮清理
pub fn mark_exited(instance_id: &str, pid: u32) {
    LIVE_PIDS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(id, p)| !(id == instance_id && *p == pid));
}

pub fn forget_pid(instance_id: &str, pid: u32) {
    mark_exited(instance_id, pid);
    update_pids(instance_id, |pids| pids.retain(|p| p.pid != pid));
}

/// 只在远端那个 pid 的命令行仍是 Monet 启动的这条会话时才 kill（防 pid 复用误伤）
pub fn kill_script(pid: u32, cli_session_id: &str) -> String {
    format!(
        r#"c=$(ps -o command= -p {pid} 2>/dev/null) || exit 0
case "$c" in *claude*{sid}*) kill {pid} ;; esac
"#,
        sid = sh_quote(cli_session_id),
    )
}

/// 结束远端一个 Monet 自己启动的进程（已退出或已被复用时什么也不做）
pub fn kill_if_ours(launch: &RemoteLaunch, pid: u32) {
    let script = kill_script(pid, &launch.cli_session_id);
    let _ = executor_for(launch.loopback).run(&launch.target, &script, KILL_TIMEOUT);
    forget_pid(launch.instance.instance_id(), pid);
}

/// 已登记但 ssh 不再连着的条目：Monet 异常退出或断线后留在远端的进程
fn stale_entries(
    recorded: Vec<RecordedPid>,
    live: &[u32],
    keep: impl Fn(&RecordedPid) -> bool,
) -> Vec<RecordedPid> {
    recorded
        .into_iter()
        .filter(|entry| !live.contains(&entry.pid) && keep(entry))
        .collect()
}

fn kill_stale(
    executor: &dyn RemoteExecutor,
    target: &SshTarget,
    instance_id: &str,
    keep: impl Fn(&RecordedPid) -> bool,
) {
    let recorded: Vec<RecordedPid> = {
        let _guard = PIDS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        std::fs::read_to_string(pids_path(instance_id))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    };
    let stale = stale_entries(recorded, &live_pids_for(instance_id), keep);
    if stale.is_empty() {
        return;
    }
    for entry in &stale {
        let _ = executor.run(
            target,
            &kill_script(entry.pid, &entry.session),
            KILL_TIMEOUT,
        );
    }
    update_pids(instance_id, |pids| pids.retain(|p| !stale.contains(p)));
}

/// 每轮同步都清一次遗留进程；没有遗留条目时不发任何远端命令
pub fn cleanup_stale_pids(executor: &dyn RemoteExecutor, target: &SshTarget, instance_id: &str) {
    kill_stale(executor, target, instance_id, |_| true);
}

/// 启动前先结束同一会话的遗留进程：断线后立即重开时，避免两个 CLI 同时续写同一份会话文件
pub fn cleanup_session_leftovers(launch: &RemoteLaunch) {
    kill_stale(
        executor_for(launch.loopback),
        &launch.target,
        launch.instance.instance_id(),
        |entry| entry.session == launch.cli_session_id,
    );
}

fn executor_for(loopback: bool) -> &'static dyn RemoteExecutor {
    #[cfg(debug_assertions)]
    if loopback {
        return &super::executor::LocalShellExecutor;
    }
    let _ = loopback;
    &super::executor::SshExecutor
}

// ---------- 远端运行态：同一主机 5 秒内只拉一次进程列表 ----------

type ProcessList = Vec<(u32, String)>;

static PS_CACHE: Mutex<Vec<(String, Instant, ProcessList)>> = Mutex::new(Vec::new());

pub fn parse_ps(stdout: &str) -> ProcessList {
    stdout
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (pid, command) = line.split_once(char::is_whitespace)?;
            Some((pid.parse().ok()?, command.trim().to_string()))
        })
        .collect()
}

/// 进程已不在或已不是这条会话时脚本输出这一行（不 kill）
pub const GONE_MARKER: &str = "MONET_REMOTE_GONE";

/// 终止远端外部进程：kill 前再核对一次命令行；kill 失败时以非零退出并带 stderr
pub fn external_kill_script(pid: u32, cli_session_id: &str) -> String {
    format!(
        r#"c=$(ps -o command= -p {pid} 2>/dev/null) || {{ echo {GONE_MARKER}; exit 0; }}
case "$c" in *claude*{sid}*) exec kill {pid} ;; *) echo {GONE_MARKER} ;; esac
"#,
        sid = sh_quote(cli_session_id),
    )
}

fn forget_ps_cache(instance_id: &str) {
    PS_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .retain(|(id, _, _)| id != instance_id);
}

/// 终止远端一个不是 Monet 启动的会话进程：先绕过缓存重新扫描。
/// 返回 true 已 kill，false 进程已退出（没有执行 kill）
pub fn kill_external_session(
    instance: &EngineInstanceId,
    cli_session_id: &str,
    own_pid: Option<u32>,
) -> Result<bool, String> {
    forget_ps_cache(instance.instance_id());
    let Some(pid) = external_session_pid(instance, cli_session_id, own_pid)? else {
        return Ok(false);
    };
    let (_, target) = host_for(instance)?;
    let out = executor_for(super::loopback_root().is_some())
        .run(
            &target,
            &external_kill_script(pid, cli_session_id),
            KILL_TIMEOUT,
        )
        .map_err(|e| e.to_string());
    forget_ps_cache(instance.instance_id());
    let out = out?;
    if out.timed_out {
        return Err("REMOTE_KILL_FAILED:timeout".into());
    }
    if out.code != Some(0) {
        return Err(format!("REMOTE_KILL_FAILED:{}", out.stderr.trim()));
    }
    Ok(!out.stdout.contains(GONE_MARKER))
}

/// 远端是否有不是 Monet 启动的 claude 进程在跑这个会话；返回其 pid
pub fn external_session_pid(
    instance: &EngineInstanceId,
    cli_session_id: &str,
    own_pid: Option<u32>,
) -> Result<Option<u32>, String> {
    let (_, target) = host_for(instance)?;
    let key = instance.instance_id().to_string();
    let cached = {
        let cache = PS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache
            .iter()
            .find(|(id, at, _)| id == &key && at.elapsed() < PS_CACHE_TTL)
            .map(|(_, _, list)| list.clone())
    };
    let list = match cached {
        Some(list) => list,
        None => {
            let out = executor_for(super::loopback_root().is_some())
                .run(&target, "ps -axo pid=,command=", PS_TIMEOUT)
                .map_err(|e| e.to_string())?;
            if out.code != Some(0) {
                return Err("REMOTE_PS_FAILED".into());
            }
            let list = parse_ps(&out.stdout);
            let mut cache = PS_CACHE.lock().unwrap_or_else(|e| e.into_inner());
            cache.retain(|(id, _, _)| id != &key);
            cache.push((key, Instant::now(), list.clone()));
            list
        }
    };
    Ok(list
        .into_iter()
        .find(|(pid, command)| {
            Some(*pid) != own_pid
                && crate::commands::command_matches_claude_session(command, cli_session_id)
        })
        .map(|(pid, _)| pid))
}

/// 主机显示名（远端运行态与断线提示用）
pub fn alias_for(instance: &EngineInstanceId) -> Option<String> {
    host_for(instance).ok().map(|(host, _)| host.alias)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launch_script_quotes_everything_and_reports_the_pid_first() {
        let script = launch_script(
            "/home/alice/my proj",
            "/opt/claude",
            &[
                "--resume".into(),
                "abc".into(),
                "--model".into(),
                "it's".into(),
            ],
        );
        let first = script.lines().next().unwrap();
        assert!(first.contains(PID_MARKER.trim_end()));
        assert!(script.contains("cd '/home/alice/my proj'"));
        assert!(script.contains(r"'it'\''s'"));
        assert!(script.contains(
            "exec \"$login_sh\" -lc 'exec \"$0\" \"$@\"' '/opt/claude' '--resume' 'abc'"
        ));
        assert!(script.contains("CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING=true"));
        assert!(!script.contains("--no-session-persistence"));
        let status = std::process::Command::new("/bin/sh")
            .args(["-n", "-c", &script])
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn launch_script_reports_a_missing_cwd_and_the_pid() {
        let script = launch_script("/nonexistent/monet-test-dir", "/bin/echo", &[]);
        let out = std::process::Command::new("/bin/sh")
            .args(["-c", &script])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut lines = stdout.lines();
        assert!(matches!(parse_marker(lines.next().unwrap()), Some(Ok(_))));
        assert_eq!(
            parse_marker(lines.next().unwrap()),
            Some(Err("REMOTE_CWD_MISSING"))
        );
        assert_eq!(out.status.code(), Some(97));
    }

    #[test]
    fn launch_script_execs_in_place_so_the_reported_pid_is_the_cli() {
        let dir = std::env::temp_dir();
        let script = launch_script(
            dir.to_str().unwrap(),
            "/bin/sh",
            &["-c".into(), "echo $$".into()],
        );
        let out = std::process::Command::new("/bin/sh")
            .args(["-c", &script])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut lines = stdout.lines();
        let reported = parse_marker(lines.next().unwrap()).unwrap().unwrap();
        let actual: u32 = lines.last().unwrap().trim().parse().unwrap();
        assert_eq!(reported, actual);
    }

    fn entry(pid: u32, session: &str) -> RecordedPid {
        RecordedPid {
            pid,
            session: session.into(),
        }
    }

    #[test]
    fn live_processes_are_never_treated_as_leftovers() {
        let recorded = vec![entry(10, "a"), entry(11, "b"), entry(12, "a")];
        let stale = stale_entries(recorded, &[11], |_| true);
        assert_eq!(stale, vec![entry(10, "a"), entry(12, "a")]);
    }

    #[test]
    fn session_leftovers_only_cover_the_same_session() {
        let recorded = vec![entry(10, "a"), entry(11, "b"), entry(12, "a")];
        let stale = stale_entries(recorded, &[12], |e| e.session == "a");
        assert_eq!(stale, vec![entry(10, "a")]);
    }

    #[test]
    fn exited_processes_become_leftovers_until_forgotten() {
        let id = "ssh-test-live-1";
        LIVE_PIDS.lock().unwrap().push((id.to_string(), 4242));
        assert_eq!(live_pids_for(id), vec![4242]);
        mark_exited(id, 4242);
        assert!(live_pids_for(id).is_empty());
        let stale = stale_entries(vec![entry(4242, "s")], &live_pids_for(id), |_| true);
        assert_eq!(stale, vec![entry(4242, "s")]);
    }

    #[test]
    fn external_kill_rechecks_the_command_and_reports_kill_failures() {
        let script = external_kill_script(4242, "s-1");
        assert!(script.contains("-p 4242"));
        assert!(script.contains("*claude*'s-1'*) exec kill 4242"));
        assert!(script.contains(GONE_MARKER));
    }

    #[cfg(unix)]
    #[test]
    fn external_kill_script_runs_in_sh() {
        use super::super::executor::LocalShellExecutor;
        let target = SshTarget {
            destination: "local".into(),
            port: None,
            platform: RemotePlatform::Posix,
        };
        // 不存在的 pid：报告已退出，不 kill
        let out = LocalShellExecutor
            .run(&target, &external_kill_script(999_999, "s-1"), KILL_TIMEOUT)
            .unwrap();
        assert_eq!(out.code, Some(0));
        assert!(out.stdout.contains(GONE_MARKER));
        // 命令行不匹配（本 shell 自身）：同样不 kill
        let out = LocalShellExecutor
            .run(
                &target,
                &external_kill_script(std::process::id(), "s-1"),
                KILL_TIMEOUT,
            )
            .unwrap();
        assert!(out.stdout.contains(GONE_MARKER));
    }

    #[test]
    fn kill_script_only_targets_the_recorded_session() {
        let script = kill_script(4242, "s-1");
        assert!(script.contains("-p 4242"));
        assert!(script.contains("*claude*'s-1'*) kill 4242"));
        for forbidden in [" rm ", "mv ", "touch", "mkdir", "tee "] {
            assert!(!script.contains(forbidden));
        }
    }

    #[test]
    fn ps_lines_parse_pid_and_full_command() {
        let list = parse_ps("  12 /usr/bin/claude --resume abc\n 7 -zsh\nbad\n");
        assert_eq!(
            list,
            vec![
                (12, "/usr/bin/claude --resume abc".to_string()),
                (7, "-zsh".to_string())
            ]
        );
    }

    #[test]
    fn markers_are_recognised() {
        assert_eq!(parse_marker("MONET_REMOTE_PID 123\n"), Some(Ok(123)));
        assert_eq!(parse_marker("{\"type\":\"x\"}"), None);
        assert_eq!(
            parse_marker(CWD_MISSING_MARKER),
            Some(Err("REMOTE_CWD_MISSING"))
        );
    }
}
