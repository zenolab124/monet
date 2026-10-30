//! 远端执行器：所有远端命令的唯一出口。
//!
//! 真实实现只调用系统 `/usr/bin/ssh`，参数以数组传递，不经过本机 shell。
//! macOS / Linux 远端脚本统一包成 `exec /bin/sh -c '<脚本>'`，不受远端登录 shell 方言影响；
//! Windows 远端脚本是 PowerShell 5.1，包成 `powershell -EncodedCommand <base64>`，
//! 远端默认 shell 无论是 cmd 还是 PowerShell 都按同一字面量执行。
//! 测试通过替换 `RemoteExecutor` 实现，不依赖真实主机。

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use super::hosts::SshTarget;

/// 远端系统方言：未识别前按 POSIX 处理
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemotePlatform {
    #[default]
    Posix,
    Windows,
}

/// Windows 远端命令行（cmd）上限 8191 字符，留出余量
const WINDOWS_COMMAND_MAX: usize = 8000;

/// PowerShell 脚本公共前置：关进度流（否则 stderr 混入 CLIXML），输出统一为无 BOM 的 UTF-8
const PS_PREAMBLE: &str = "$ProgressPreference='SilentlyContinue'\n[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding $false\n";

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

/// 原始输出：会话文件需要逐字节镜像，stdout 不做文本转换
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawOutput {
    pub code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: String,
    pub timed_out: bool,
}

pub trait RemoteExecutor: Send + Sync {
    fn run_raw(
        &self,
        target: &SshTarget,
        script: &str,
        timeout: Duration,
    ) -> std::io::Result<RawOutput>;

    fn run(
        &self,
        target: &SshTarget,
        script: &str,
        timeout: Duration,
    ) -> std::io::Result<ExecOutput> {
        let raw = self.run_raw(target, script, timeout)?;
        Ok(ExecOutput {
            code: raw.code,
            stdout: String::from_utf8_lossy(&raw.stdout).into_owned(),
            stderr: raw.stderr,
            timed_out: raw.timed_out,
        })
    }
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

/// ssh 参数：免交互、限时连接、心跳、可选连接复用；destination 之前以 `--` 结束选项。
/// 这里起的连接会成为复用主进程，长活会话也挂在它上面；心跳由主进程负责
/// （复用客户端自己的 ServerAlive 不生效），所以主进程必须带心跳，断网后才能及时退出
pub fn ssh_args(
    target: &SshTarget,
    control_path: Option<&str>,
    remote_command: &str,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
    ]
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

/// 把任意字符串转成 PowerShell 单引号字面量（PowerShell 也把 ‘ ’ ‚ ‛ 当单引号）
pub fn ps_quote(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for c in value.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

pub fn remote_sh_command(script: &str) -> String {
    format!("exec /bin/sh -c {}", sh_quote(script))
}

/// 按远端系统包装脚本；Windows 命令行超长时报错（调用方需分批）
pub fn remote_command(platform: RemotePlatform, script: &str) -> std::io::Result<String> {
    match platform {
        RemotePlatform::Posix => Ok(remote_sh_command(script)),
        RemotePlatform::Windows => {
            let source = format!("{PS_PREAMBLE}{script}");
            let utf16: Vec<u8> = source.encode_utf16().flat_map(u16::to_le_bytes).collect();
            let command = format!(
                "powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -EncodedCommand {}",
                base64::engine::general_purpose::STANDARD.encode(utf16)
            );
            if command.len() > WINDOWS_COMMAND_MAX {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "remote_command_too_long",
                ));
            }
            Ok(command)
        }
    }
}

/// 脚本在 Windows 远端包装后是否仍在命令行长度限制内（批量拉取据此分批）
pub fn fits_windows_command(script: &str) -> bool {
    remote_command(RemotePlatform::Windows, script).is_ok()
}

/// 长活会话（远端 claude 进程）的 ssh 参数，与一次性命令有两处不同：
/// - 只复用已有的连接复用主进程，自己不当主进程：后台主进程会继承管道写端，
///   会话结束后 stdout 迟迟等不到 EOF；
/// - 开启心跳：连接中断 45 秒内 ssh 以 255 退出，会话据此显示断线。复用已有主进程时
///   心跳实际由主进程负责（见 `ssh_args`），主进程断开后本进程同样以 255 退出。
pub fn session_ssh_args(
    target: &SshTarget,
    control_path: Option<&str>,
    remote_command: &str,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "ConnectTimeout=10",
        "-o",
        "ServerAliveInterval=15",
        "-o",
        "ServerAliveCountMax=3",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    if let Some(path) = control_path {
        args.push("-o".into());
        args.push("ControlMaster=no".into());
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

/// 构造运行远端会话脚本的本机进程：通常是 ssh；调试版回环时直接用本机 `/bin/sh`
pub fn session_command(
    target: &SshTarget,
    script: &str,
    loopback: bool,
) -> std::io::Result<Command> {
    if loopback && cfg!(debug_assertions) {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(script);
        return Ok(command);
    }
    let control = control_path();
    let mut command = Command::new(SSH_BIN);
    command.args(session_ssh_args(
        target,
        control.as_deref(),
        &remote_command(target.platform, script)?,
    ));
    Ok(command)
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

/// 收下已到达的数据；返回写端是否已关闭（读到 EOF）
fn drain(rx: &mpsc::Receiver<Vec<u8>>, buf: &mut Vec<u8>) -> bool {
    loop {
        match rx.try_recv() {
            Ok(chunk) => buf.extend_from_slice(&chunk),
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => return true,
        }
    }
}

/// 轮询子进程的间隔：同步一轮要跑成千上万条短命令，间隔直接决定每个文件的固定开销
const POLL: Duration = Duration::from_millis(5);

/// 运行已配置好的子进程并在时限内收集输出；超时即击杀并回收
fn collect(mut command: Command, timeout: Duration) -> std::io::Result<RawOutput> {
    command
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
        std::thread::sleep(POLL);
    };
    // 两路都读到 EOF 即收齐；写端被后台主进程继承时 EOF 不会来，最多再等 100ms
    let settle = Instant::now() + Duration::from_millis(100);
    loop {
        let out_closed = drain(&out_rx, &mut stdout);
        let err_closed = drain(&err_rx, &mut stderr);
        if (out_closed && err_closed) || Instant::now() >= settle {
            break;
        }
        std::thread::sleep(POLL);
    }
    Ok(RawOutput {
        code: status.and_then(|s| s.code()),
        stdout,
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        timed_out: status.is_none(),
    })
}

impl RemoteExecutor for SshExecutor {
    fn run_raw(
        &self,
        target: &SshTarget,
        script: &str,
        timeout: Duration,
    ) -> std::io::Result<RawOutput> {
        let control = control_path();
        let mut command = Command::new(SSH_BIN);
        command.args(ssh_args(
            target,
            control.as_deref(),
            &remote_command(target.platform, script)?,
        ));
        collect(command, timeout)
    }
}

/// 本机回环执行器：直接用本机 `/bin/sh` 执行远端脚本，忽略 ssh 目标。
/// 只用于单测与调试版的本机验收，发布版不编译。
#[cfg(any(test, debug_assertions))]
pub struct LocalShellExecutor;

#[cfg(any(test, debug_assertions))]
impl RemoteExecutor for LocalShellExecutor {
    fn run_raw(
        &self,
        _target: &SshTarget,
        script: &str,
        timeout: Duration,
    ) -> std::io::Result<RawOutput> {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(script);
        collect(command, timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ssh_args_never_become_the_control_master() {
        let target = SshTarget {
            destination: "devbox".into(),
            port: None,
            platform: RemotePlatform::Posix,
        };
        let args = session_ssh_args(&target, Some("/tmp/c/%C"), "exec /bin/sh -c 'x'");
        assert!(args.contains(&"ControlMaster=no".to_string()));
        assert!(!args.iter().any(|a| a.starts_with("ControlPersist")));
        assert!(args.contains(&"ServerAliveInterval=15".to_string()));
        assert!(args.contains(&"BatchMode=yes".to_string()));
        let dash = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(args.len(), dash + 3);
    }

    #[test]
    fn ps_quote_doubles_every_powershell_single_quote() {
        assert_eq!(ps_quote("a'b"), "'a''b'");
        assert_eq!(ps_quote("x\u{2019}y"), "'x\u{2019}\u{2019}y'");
        assert_eq!(ps_quote(r"C:\Users\a b\$x"), r"'C:\Users\a b\$x'");
    }

    #[test]
    fn windows_commands_are_encoded_utf16_powershell() {
        let command = remote_command(RemotePlatform::Windows, "'中文'").unwrap();
        let encoded = command.rsplit(' ').next().unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let units: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        let source = String::from_utf16(&units).unwrap();
        assert!(source.starts_with("$ProgressPreference='SilentlyContinue'"));
        assert!(source.ends_with("'中文'"));
        assert!(command.starts_with("powershell -NoProfile -NonInteractive"));
        // base64 只含 [A-Za-z0-9+/=]，cmd 与 PowerShell 都不会解释
        assert!(encoded
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+/=".contains(c)));
        assert!(!fits_windows_command(&"x".repeat(4000)));
        assert!(fits_windows_command(&"x".repeat(2000)));
    }

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
            platform: RemotePlatform::Posix,
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
        // 复用主进程负责整条连接的心跳
        assert!(args.contains(&"ServerAliveInterval=15".to_string()));
        assert!(args.contains(&"ServerAliveCountMax=3".to_string()));
    }

    #[test]
    fn ssh_args_without_control_path_skip_multiplexing() {
        let target = SshTarget {
            destination: "devbox".into(),
            port: None,
            platform: RemotePlatform::Posix,
        };
        let args = ssh_args(&target, None, "true");
        assert!(!args.iter().any(|a| a.starts_with("Control")));
        assert!(!args.contains(&"-p".to_string()));
    }
}
