//! 远端执行器：所有远端命令的唯一出口。
//!
//! 真实实现只调用系统 `/usr/bin/ssh`，参数以数组传递，不经过本机 shell；
//! 远端脚本统一包成 `exec /bin/sh -c '<脚本>'`，不受远端登录 shell 方言影响。
//! 测试通过替换 `RemoteExecutor` 实现，不依赖真实主机。

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::hosts::SshTarget;

const SSH_BIN: &str = "/usr/bin/ssh";
/// macOS 的 Unix socket 路径上限是 104 字节；%C 展开为 40 位十六进制
const UNIX_SOCKET_PATH_MAX: usize = 104;
const CONTROL_HASH_LEN: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecOutput {
    /// ssh 的退出码；超时为 None
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}

pub trait RemoteExecutor: Send + Sync {
    fn run(
        &self,
        target: &SshTarget,
        script: &str,
        timeout: Duration,
    ) -> std::io::Result<ExecOutput>;
}

/// 把任意字符串转成单引号包裹的 POSIX shell 字面量
pub fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn control_dir() -> PathBuf {
    crate::config::data_dir().join("remote").join("ssh")
}

/// 连接复用目录可用时返回 ControlPath 模板；路径过长时返回 None（退回单次连接）
fn control_path() -> Option<String> {
    let dir = control_dir();
    let template = format!("{}/%C", dir.display());
    if template.len() - 2 + CONTROL_HASH_LEN >= UNIX_SOCKET_PATH_MAX {
        return None;
    }
    std::fs::create_dir_all(&dir).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok()?;
    }
    Some(template)
}

/// ssh 参数：免交互、限时连接、可选连接复用；destination 之前以 `--` 结束选项
pub fn ssh_args(
    target: &SshTarget,
    control_path: Option<&str>,
    remote_command: &str,
) -> Vec<String> {
    let mut args: Vec<String> = ["-T", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10"]
        .into_iter()
        .map(String::from)
        .collect();
    if let Some(path) = control_path {
        for option in ["ControlMaster=auto", "ControlPersist=120"] {
            args.push("-o".into());
            args.push(option.into());
        }
        args.push("-o".into());
        args.push(format!("ControlPath={path}"));
    }
    if let Some(port) = target.port {
        args.push("-p".into());
        args.push(port.to_string());
    }
    args.push("--".into());
    args.push(target.destination.clone());
    args.push(remote_command.to_string());
    args
}

pub fn remote_sh_command(script: &str) -> String {
    format!("exec /bin/sh -c {}", sh_quote(script))
}

pub struct SshExecutor;

fn spawn_reader(mut stream: impl Read + Send + 'static) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut chunk = [0u8; 8192];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(chunk[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });
    rx
}

fn drain(rx: &mpsc::Receiver<Vec<u8>>, buf: &mut Vec<u8>) {
    while let Ok(chunk) = rx.try_recv() {
        buf.extend_from_slice(&chunk);
    }
}

impl RemoteExecutor for SshExecutor {
    fn run(
        &self,
        target: &SshTarget,
        script: &str,
        timeout: Duration,
    ) -> std::io::Result<ExecOutput> {
        let control = control_path();
        let mut command = Command::new(SSH_BIN);
        command
            .args(ssh_args(
                target,
                control.as_deref(),
                &remote_sh_command(script),
            ))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        // 连接复用的后台主进程可能继承管道写端：不等 EOF，子进程退出后只收已到达的数据
        let out_rx = spawn_reader(child.stdout.take().expect("piped stdout"));
        let err_rx = spawn_reader(child.stderr.take().expect("piped stderr"));
        let deadline = Instant::now() + timeout;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let status = loop {
            drain(&out_rx, &mut stdout);
            drain(&err_rx, &mut stderr);
            if let Some(status) = child.try_wait()? {
                break Some(status);
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(30));
        };
        let settle = Instant::now() + Duration::from_millis(100);
        while Instant::now() < settle {
            drain(&out_rx, &mut stdout);
            drain(&err_rx, &mut stderr);
            std::thread::sleep(Duration::from_millis(20));
        }
        Ok(ExecOutput {
            code: status.and_then(|s| s.code()),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            timed_out: status.is_none(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sh_quote_escapes_single_quotes() {
        assert_eq!(sh_quote("a'b"), r"'a'\''b'");
        assert_eq!(sh_quote(""), "''");
    }

    #[test]
    fn ssh_args_end_options_before_destination_and_keep_script_as_one_argument() {
        let target = SshTarget {
            destination: "alice@example.com".into(),
            port: Some(2222),
        };
        let args = ssh_args(
            &target,
            Some("/tmp/c/%C"),
            &remote_sh_command("echo 'hi'; uname -s"),
        );
        let dash = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(&args[dash + 1], "alice@example.com");
        assert_eq!(args.len(), dash + 3);
        assert_eq!(
            args.last().unwrap(),
            r"exec /bin/sh -c 'echo '\''hi'\''; uname -s'"
        );
        assert!(args.contains(&"BatchMode=yes".to_string()));
        assert!(args.contains(&"ControlPath=/tmp/c/%C".to_string()));
        assert!(args.windows(2).any(|w| w == ["-p", "2222"]));
    }

    #[test]
    fn ssh_args_without_control_path_skip_multiplexing() {
        let target = SshTarget {
            destination: "devbox".into(),
            port: None,
        };
        let args = ssh_args(&target, None, "true");
        assert!(!args.iter().any(|a| a.starts_with("Control")));
        assert!(!args.contains(&"-p".to_string()));
    }
}
