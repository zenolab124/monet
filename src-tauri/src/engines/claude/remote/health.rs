//! 连接体检：一次 ssh 往返报告 SSH 可达、远端系统、CLI 路径、CLI 版本、数据根五项。
//!
//! 远端脚本只读：uname、command -v、`claude --version`、目录存在判断。
//! 非交互 ssh 的 PATH 只有系统目录，用户级 CLI 需经登录 shell 定位；
//! claude 多为 node 脚本，版本查询同样经登录 shell 执行，保证 node 在 PATH 上。

use std::time::Duration;

use serde::Serialize;

use super::executor::{sh_quote, ExecOutput, RemoteExecutor};
use super::hosts::SshTarget;

/// 依赖新协议的功能以此为闸门（与本机 Claude 会话使用的最低协议版本一致）
pub const MIN_CLI_VERSION: (u32, u32, u32) = (2, 1, 306);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(30);
const STDERR_TAIL_CHARS: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Pass,
    Warn,
    Fail,
    Skipped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    Unreachable,
    AuthFailed,
    HostKeyUntrusted,
    UnsupportedOs,
    CliNotFound,
    DataRootMissing,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckItem {
    /// ssh / os / cli_path / cli_version / data_root
    pub id: &'static str,
    pub state: CheckState,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub items: Vec<CheckItem>,
    /// 首个失败项的类别；全部通过（含仅版本警告）时为 None
    pub failure: Option<FailureKind>,
    pub stderr_tail: Option<String>,
    pub os: Option<String>,
    pub cli_path: Option<String>,
    pub cli_version: Option<String>,
    pub data_root: Option<String>,
    pub has_projects: bool,
    /// 可注册为引擎实例：SSH、系统、CLI 路径、数据根四项通过
    pub usable: bool,
}

/// 远端体检脚本；`claude_path` 已由 hosts::valid_claude_path 校验，仍按字面量转义
pub fn health_script(claude_path: Option<&str>) -> String {
    let user_path = claude_path.map(sh_quote).unwrap_or_else(|| "''".into());
    format!(
        r#"printf 'os=%s\n' "$(uname -s 2>/dev/null)"
login_sh="${{SHELL:-/bin/sh}}"
p={user_path}
if [ -n "$p" ]; then
  if [ -x "$p" ]; then printf 'claude=%s\n' "$p"; else printf 'claude_missing=user\n'; p=''; fi
else
  p=$(command -v claude 2>/dev/null)
  if [ -z "$p" ]; then p=$("$login_sh" -lc 'command -v claude' 2>/dev/null | tail -n 1); fi
  case "$p" in /*) printf 'claude=%s\n' "$p" ;; *) printf 'claude_missing=auto\n'; p='' ;; esac
fi
if [ -n "$p" ]; then printf 'version=%s\n' "$("$login_sh" -lc '"$0" --version' "$p" 2>/dev/null | head -n 1)"; fi
ccd=$("$login_sh" -lc 'printf %s "${{CLAUDE_CONFIG_DIR:-}}"' 2>/dev/null | tail -n 1)
root="${{ccd:-$HOME/.claude}}"
printf 'root=%s\n' "$root"
if [ -d "$root" ]; then printf 'root_exists=yes\n'; else printf 'root_exists=no\n'; fi
if [ -d "$root/projects" ]; then printf 'projects=yes\n'; else printf 'projects=no\n'; fi
"#
    )
}

fn parse_version(value: &str) -> Option<(u32, u32, u32)> {
    let token = value.split_whitespace().next()?;
    let mut parts = token.split('.').map(|part| part.parse::<u32>().ok());
    Some((
        parts.next()??,
        parts.next()??,
        parts.next().flatten().unwrap_or(0),
    ))
}

fn tail(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let chars: Vec<char> = trimmed.chars().collect();
    let start = chars.len().saturating_sub(STDERR_TAIL_CHARS);
    Some(chars[start..].iter().collect())
}

/// ssh 自身失败（退出码 255）按 stderr 归类
fn classify_ssh_failure(stderr: &str) -> FailureKind {
    if stderr.contains("Host key verification failed")
        || stderr.contains("REMOTE HOST IDENTIFICATION HAS CHANGED")
    {
        FailureKind::HostKeyUntrusted
    } else if stderr.contains("Permission denied")
        || stderr.contains("Too many authentication failures")
    {
        FailureKind::AuthFailed
    } else {
        FailureKind::Unreachable
    }
}

fn item(id: &'static str, state: CheckState, detail: Option<String>) -> CheckItem {
    CheckItem { id, state, detail }
}

fn skipped_after(items: &mut Vec<CheckItem>, ids: &[&'static str]) {
    for id in ids {
        items.push(item(id, CheckState::Skipped, None));
    }
}

/// 由一次执行结果生成体检报告（纯函数，便于用替身输出测试）
pub fn report_from_output(output: &ExecOutput) -> HealthReport {
    let stderr_tail = tail(&output.stderr);
    let mut report = HealthReport {
        items: Vec::new(),
        failure: None,
        stderr_tail: stderr_tail.clone(),
        os: None,
        cli_path: None,
        cli_version: None,
        data_root: None,
        has_projects: false,
        usable: false,
    };
    let rest = ["os", "cli_path", "cli_version", "data_root"];
    if output.timed_out {
        report.items.push(item("ssh", CheckState::Fail, None));
        skipped_after(&mut report.items, &rest);
        report.failure = Some(FailureKind::Timeout);
        return report;
    }
    if output.code == Some(255) || output.code.is_none() {
        report
            .items
            .push(item("ssh", CheckState::Fail, stderr_tail));
        skipped_after(&mut report.items, &rest);
        report.failure = Some(classify_ssh_failure(&output.stderr));
        return report;
    }
    report.items.push(item("ssh", CheckState::Pass, None));

    let mut fields = std::collections::HashMap::new();
    for line in output.stdout.lines() {
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    let os = fields.get("os").cloned().filter(|v| !v.is_empty());
    report.os = os.clone();
    match os.as_deref() {
        Some("Darwin") | Some("Linux") => report.items.push(item("os", CheckState::Pass, os)),
        other => {
            report
                .items
                .push(item("os", CheckState::Fail, other.map(String::from)));
            skipped_after(&mut report.items, &["cli_path", "cli_version", "data_root"]);
            report.failure = Some(FailureKind::UnsupportedOs);
            return report;
        }
    }

    let cli_path = fields.get("claude").cloned();
    report.cli_path = cli_path.clone();
    match &cli_path {
        Some(path) => report
            .items
            .push(item("cli_path", CheckState::Pass, Some(path.clone()))),
        None => {
            report.items.push(item(
                "cli_path",
                CheckState::Fail,
                fields.get("claude_missing").cloned(),
            ));
            report.failure.get_or_insert(FailureKind::CliNotFound);
        }
    }

    let version = fields.get("version").cloned().filter(|v| !v.is_empty());
    report.cli_version = version.clone();
    let version_state = match (&cli_path, version.as_deref().and_then(parse_version)) {
        (None, _) => CheckState::Skipped,
        (Some(_), Some(parsed)) if parsed >= MIN_CLI_VERSION => CheckState::Pass,
        (Some(_), _) => CheckState::Warn,
    };
    report
        .items
        .push(item("cli_version", version_state, version));

    report.data_root = fields.get("root").cloned();
    report.has_projects = fields.get("projects").map(String::as_str) == Some("yes");
    if fields.get("root_exists").map(String::as_str) == Some("yes") {
        report.items.push(item(
            "data_root",
            CheckState::Pass,
            report.data_root.clone(),
        ));
    } else {
        report.items.push(item(
            "data_root",
            CheckState::Fail,
            report.data_root.clone(),
        ));
        report.failure.get_or_insert(FailureKind::DataRootMissing);
    }

    report.usable = report.failure.is_none();
    report
}

pub fn check(
    executor: &dyn RemoteExecutor,
    target: &SshTarget,
    claude_path: Option<&str>,
) -> HealthReport {
    let output = executor
        .run(target, &health_script(claude_path), HEALTH_TIMEOUT)
        .unwrap_or_else(|error| ExecOutput {
            code: None,
            stdout: String::new(),
            stderr: error.to_string(),
            timed_out: false,
        });
    report_from_output(&output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(stdout: &str) -> ExecOutput {
        ExecOutput {
            code: Some(0),
            stdout: stdout.into(),
            stderr: String::new(),
            timed_out: false,
        }
    }

    fn states(report: &HealthReport) -> Vec<(&'static str, CheckState)> {
        report.items.iter().map(|i| (i.id, i.state)).collect()
    }

    #[test]
    fn all_checks_pass_on_a_ready_host() {
        let report = report_from_output(&ok(
            "os=Darwin\nclaude=/Users/alice/.local/bin/claude\nversion=2.1.306 (Claude Code)\nroot=/Users/alice/.claude\nroot_exists=yes\nprojects=yes\n",
        ));
        assert!(report.usable);
        assert_eq!(report.failure, None);
        assert!(report.items.iter().all(|i| i.state == CheckState::Pass));
        assert_eq!(
            report.cli_path.as_deref(),
            Some("/Users/alice/.local/bin/claude")
        );
        assert!(report.has_projects);
    }

    #[test]
    fn old_cli_only_warns_and_stays_usable() {
        let report = report_from_output(&ok("os=Linux\nclaude=/usr/local/bin/claude\nversion=2.0.9\nroot=/home/bob/.claude\nroot_exists=yes\nprojects=no\n"));
        assert!(report.usable);
        assert!(states(&report).contains(&("cli_version", CheckState::Warn)));
    }

    #[test]
    fn unsupported_os_stops_further_checks() {
        let report = report_from_output(&ok("os=MINGW64_NT\n"));
        assert!(!report.usable);
        assert_eq!(report.failure, Some(FailureKind::UnsupportedOs));
        assert_eq!(
            states(&report)[2..],
            [
                ("cli_path", CheckState::Skipped),
                ("cli_version", CheckState::Skipped),
                ("data_root", CheckState::Skipped)
            ]
        );
    }

    #[test]
    fn missing_cli_and_missing_root_report_the_first_failure() {
        let report = report_from_output(&ok(
            "os=Darwin\nclaude_missing=auto\nroot=/Users/x/.claude\nroot_exists=no\nprojects=no\n",
        ));
        assert!(!report.usable);
        assert_eq!(report.failure, Some(FailureKind::CliNotFound));
        assert!(states(&report).contains(&("data_root", CheckState::Fail)));
    }

    #[test]
    fn ssh_failures_are_classified_from_stderr() {
        let fail = |stderr: &str| ExecOutput {
            code: Some(255),
            stdout: String::new(),
            stderr: stderr.into(),
            timed_out: false,
        };
        assert_eq!(
            report_from_output(&fail("Host key verification failed.\r\n")).failure,
            Some(FailureKind::HostKeyUntrusted)
        );
        assert_eq!(
            report_from_output(&fail("alice@h: Permission denied (publickey).")).failure,
            Some(FailureKind::AuthFailed)
        );
        assert_eq!(
            report_from_output(&fail("ssh: connect to host h port 22: Connection refused")).failure,
            Some(FailureKind::Unreachable)
        );
        let timeout = ExecOutput {
            code: None,
            stdout: String::new(),
            stderr: String::new(),
            timed_out: true,
        };
        assert_eq!(
            report_from_output(&timeout).failure,
            Some(FailureKind::Timeout)
        );
    }

    #[test]
    fn stderr_tail_keeps_the_last_400_characters() {
        let long = format!("{}END", "x".repeat(1000));
        let out = ExecOutput {
            code: Some(255),
            stdout: String::new(),
            stderr: long,
            timed_out: false,
        };
        let tail = report_from_output(&out).stderr_tail.unwrap();
        assert_eq!(tail.chars().count(), 400);
        assert!(tail.ends_with("END"));
    }

    #[test]
    fn health_script_quotes_user_path_and_only_reads() {
        let script = health_script(Some("/opt/claude"));
        assert!(script.contains("p='/opt/claude'"));
        for forbidden in [" rm ", ">", "mkdir", "touch", "tee "] {
            let writes = script
                .lines()
                .filter(|l| l.contains(forbidden) && !l.contains("2>/dev/null"))
                .count();
            assert_eq!(writes, 0, "{forbidden}");
        }
    }

    #[test]
    fn health_script_runs_in_posix_sh() {
        let script = health_script(None);
        let status = std::process::Command::new("/bin/sh")
            .arg("-n")
            .arg("-c")
            .arg(&script)
            .status()
            .unwrap();
        assert!(status.success());
    }
}
