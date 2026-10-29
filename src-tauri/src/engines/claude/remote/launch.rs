//! 远端会话进程：启动脚本、远端 pid 登记与清理、远端进程列表。
//!
//! 远端只执行三类命令：读（ps）、启动 claude、kill Monet 自己记录的 pid。
//! 启动脚本先输出远端 pid 再 exec，Monet 据此在会话关闭、断线或下次连接时精确清理。

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::executor::{sh_quote, RemoteExecutor};
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

static CLI_CACHE: Mutex<Vec<(String, Instant, String)>> = Mutex::new(Vec::new());

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
    let target = hosts::target_for(&host.alias, host.source == hosts::HostSource::Manual)
        .ok_or("REMOTE_INVALID_CONNECTION")?;
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
    let (host, target) = host_for(instance)?;
    let loopback = super::loopback_root().is_some();
    let claude = if loopback {
        crate::claude_locator::locate_lightweight()?
            .path
            .to_string_lossy()
            .into_owned()
    } else if let Some(path) = cached_cli(&host.instance_id) {
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
        let mut cache = CLI_CACHE.lock().unwrap_or_else(|e| e.into_inner());
        cache.retain(|(id, _, _)| id != &host.instance_id);
        cache.push((host.instance_id.clone(), Instant::now(), path.clone()));
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

fn cached_cli(instance_id: &str) -> Option<String> {
    CLI_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .iter()
        .find(|(id, at, _)| id == instance_id && at.elapsed() < CLI_CACHE_TTL)
        .map(|(_, _, path)| path.clone())
}

// ---------- 远端 pid 登记：Monet 异常退出后，下次连接该主机时据此清理 ----------

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

pub fn record_pid(instance_id: &str, pid: u32, cli_session_id: &str) {
    update_pids(instance_id, |pids| {
        pids.retain(|p| p.pid != pid);
        pids.push(RecordedPid {
            pid,
            session: cli_session_id.to_string(),
        });
    });
}

pub fn forget_pid(instance_id: &str, pid: u32) {
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

/// 上次运行遗留的远端进程：每台主机每次运行 Monet 只清一次（此时还没有本次启动的远端会话）
pub fn cleanup_stale_pids(executor: &dyn RemoteExecutor, target: &SshTarget, instance_id: &str) {
    static CLEANED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    {
        let mut cleaned = CLEANED.lock().unwrap_or_else(|e| e.into_inner());
        if cleaned.iter().any(|id| id == instance_id) {
            return;
        }
        cleaned.push(instance_id.to_string());
    }
    let stale: Vec<RecordedPid> = std::fs::read_to_string(pids_path(instance_id))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    for entry in &stale {
        let _ = executor.run(
            target,
            &kill_script(entry.pid, &entry.session),
            KILL_TIMEOUT,
        );
    }
    if !stale.is_empty() {
        update_pids(instance_id, |pids| {
            pids.retain(|p| !stale.contains(p));
        });
    }
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
