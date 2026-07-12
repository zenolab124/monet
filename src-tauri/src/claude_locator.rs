//! Claude CLI 定位器 —— 全项目唯一的 claude 可执行文件事实源。
//!
//! 四层探测链（成本递增、能力递增）：
//!   L0 用户手动配置（settings.json: claudeBinaryPath），无效时向下自愈
//!   L1 上次探测缓存（settings.json: claudeBinaryPathCached），失效自动清除
//!   L2 候选路径快速扫描（已知安装位置，同步微秒级）
//!   L3 login shell 解析（$SHELL -ilc 'command -v claude'，仅主 App 走）
//! L2/L3 命中后写回 L1 缓存；runner 等贫瘠环境走 locate_lightweight（L0-L2），
//! 由主 App 负责重探测并共享结果。
//!
//! 进程内另有内存缓存：命中结果免重复读盘；完整探测失败后 60s 内快速失败，
//! 避免 claude 缺失的机器上每次 spawn 都付 login shell 的秒级成本。
//!
//! 缓存存 symlink 路径而非 canonicalize 结果：claude 自更新只换链接目标、
//! 不动链接本身，这样 CLI 升级不打断缓存。需要真实二进制的消费方自行 resolve。
//!
//! 已知限制（Windows）：npm 全局安装只生成 claude.cmd（无 .exe），而 .cmd 经
//! std spawn 有 BatBadBut 注入防护问题，故候选表与 where 结果均只收 .exe；
//! npm-on-Windows 用户需官方原生安装或手动指定路径。
//!
//! 本文件同时被 app_lib（mod claude_locator）和 monet-routine-runner
//! （#[path] mod）编译，只允许依赖 std / dirs / serde / serde_json，
//! 禁止 use crate::* 引用宿主 crate 的其他模块。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

const MANUAL_KEY: &str = "claudeBinaryPath";
const CACHED_KEY: &str = "claudeBinaryPathCached";
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);
/// 完整探测（含 L3）失败后的负缓存时长：期间直接快速失败，不重跑 login shell
const FAIL_TTL: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// 数据结构
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LocateSource {
    Manual,
    Cached,
    Scan,
    LoginShell,
}

#[derive(Debug, Clone)]
pub struct Located {
    pub path: PathBuf,
    pub source: LocateSource,
}

/// 供设置页展示的完整状态
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocateInfo {
    /// 当前生效路径（探测失败为 None）
    pub path: Option<String>,
    pub source: Option<LocateSource>,
    /// 用户手动配置的路径原文（可能无效，供 UI 回显）
    pub manual_path: Option<String>,
    pub manual_valid: bool,
    /// 探测失败时已尝试的位置清单
    pub attempted: Vec<String>,
}

// ---------------------------------------------------------------------------
// 进程内缓存
// ---------------------------------------------------------------------------

static MEM_HIT: Mutex<Option<Located>> = Mutex::new(None);
static MEM_FAIL: Mutex<Option<(Instant, String)>> = Mutex::new(None);

fn invalidate_mem() {
    *MEM_HIT.lock().unwrap_or_else(|e| e.into_inner()) = None;
    *MEM_FAIL.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

// ---------------------------------------------------------------------------
// settings.json 读写（与主 App / runner 共用 ~/.monet/settings.json）
// ---------------------------------------------------------------------------

fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("MONET_DATA_DIR") {
        PathBuf::from(dir)
    } else {
        dirs::home_dir().unwrap_or_default().join(".monet")
    }
}

fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

fn read_setting_from(path: &Path, key: &str) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let settings: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&content).ok()?;
    settings.get(key)?.as_str().map(String::from)
}

fn write_setting_to(path: &Path, key: &str, value: Option<&str>) {
    // 文件存在但解析失败 → 拒绝覆写：宁可丢这次缓存写入，
    // 也不能带着空 map 把用户的其他设置键全部清掉
    let mut settings: serde_json::Map<String, serde_json::Value> =
        match std::fs::read_to_string(path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(m) => m,
                Err(_) => return,
            },
            Err(_) => Default::default(),
        };
    match value {
        Some(v) => {
            settings.insert(key.to_string(), serde_json::Value::String(v.to_string()));
        }
        None => {
            if settings.remove(key).is_none() {
                return; // 本来就没有，跳过写盘
            }
        }
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // 临时文件 + rename 原子写：跨进程读者（主 App / runner）永远看到完整文件
    if let Ok(json) = serde_json::to_string_pretty(&serde_json::Value::Object(settings)) {
        let tmp = path.with_extension(format!("json.tmp{}", std::process::id()));
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }
}

fn read_setting(key: &str) -> Option<String> {
    read_setting_from(&settings_path(), key)
}

fn write_setting(key: &str, value: Option<&str>) {
    write_setting_to(&settings_path(), key, value);
}

// ---------------------------------------------------------------------------
// 路径校验与候选
// ---------------------------------------------------------------------------

fn expand_tilde(input: &str) -> PathBuf {
    if input == "~" {
        return dirs::home_dir().unwrap_or_default();
    }
    if let Some(rest) = input.strip_prefix("~/").or_else(|| input.strip_prefix("~\\")) {
        return dirs::home_dir().unwrap_or_default().join(rest);
    }
    PathBuf::from(input)
}

#[cfg(unix)]
fn is_valid_binary(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.is_file()
        && std::fs::metadata(p)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_valid_binary(p: &Path) -> bool {
    p.is_file()
}

/// nvm 版本目录取语义化最新（字典序会让 v9.x 压过 v18.x）
fn nvm_latest_bin(home: &Path) -> Option<PathBuf> {
    let nvm_dir = home.join(".nvm").join("versions").join("node");
    let latest = std::fs::read_dir(&nvm_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let ver: Vec<u32> = name
                .trim_start_matches('v')
                .split('.')
                .filter_map(|s| s.parse().ok())
                .collect();
            (ver.len() == 3).then(|| (ver, e.path()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))?;
    Some(latest.1.join("bin").join("claude"))
}

/// 已知安装位置候选表（顺序即优先级，官方原生安装路径在前）
fn candidate_paths() -> Vec<PathBuf> {
    let home = dirs::home_dir().unwrap_or_default();
    let bin = if cfg!(target_os = "windows") { "claude.exe" } else { "claude" };

    let mut candidates = vec![
        home.join(".local").join("bin").join(bin),
        home.join(".claude").join("local").join("bin").join(bin),
    ];

    #[cfg(unix)]
    {
        // legacy migrate-installer 落点：wrapper 脚本 ~/.claude/local/claude
        candidates.push(home.join(".claude").join("local").join("claude"));
    }

    #[cfg(target_os = "macos")]
    {
        candidates.push(PathBuf::from("/opt/homebrew/bin/claude"));
        candidates.push(PathBuf::from("/usr/local/bin/claude"));
        candidates.push(home.join("Library").join("pnpm").join("claude"));
    }

    #[cfg(target_os = "linux")]
    {
        candidates.push(PathBuf::from("/usr/local/bin/claude"));
        candidates.push(PathBuf::from("/usr/bin/claude"));
        candidates.push(home.join(".local").join("share").join("pnpm").join("claude"));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            candidates.push(
                PathBuf::from(localappdata)
                    .join("Programs")
                    .join("claude")
                    .join(bin),
            );
        }
        // 注意：npm 全局在 Windows 只生成 claude.cmd（不 spawn，见模块头注释），
        // 此处仅覆盖官方安装器可能的 APPDATA 落点
        if let Ok(appdata) = std::env::var("APPDATA") {
            candidates.push(PathBuf::from(appdata).join("npm").join(bin));
        }
    }

    #[cfg(unix)]
    {
        candidates.push(home.join(".npm-global").join("bin").join("claude"));
        candidates.push(home.join(".cargo").join("bin").join("claude"));
        candidates.push(home.join(".volta").join("bin").join("claude"));
        candidates.push(home.join(".bun").join("bin").join("claude"));
        if let Some(nvm) = nvm_latest_bin(&home) {
            candidates.push(nvm);
        }
    }

    candidates
}

// ---------------------------------------------------------------------------
// L3：login shell 解析
// ---------------------------------------------------------------------------

/// 从 shell 输出中提取 claude 绝对路径：rc 文件可能吐脏输出（neofetch 等），
/// 只认「以 / 开头的最后一行」（仅 unix L3 使用；Windows 走 where 逐行过滤）
#[cfg(unix)]
fn parse_shell_output(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('/'))
        .next_back()
        .map(String::from)
}

/// 击杀子进程。unix 下子进程被放入独立进程组（见 run_with_timeout），
/// 整组击杀可一并回收 rc 里挂死的孙进程
fn kill_child(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("/bin/kill")
            .args(["-9", &format!("-{}", child.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// 带超时运行命令并增量收集 stdout。
/// 不依赖 EOF：rc 派生的后台进程（gpg-agent 等）会继承 stdout 写端导致
/// read_to_end 永不返回，因此 reader 线程按块转发，主循环在子进程退出后
/// 只 drain 已到达的数据。残余代价：该场景下 reader 线程会阻塞到孙进程退出，
/// 但 L3 命中即写缓存、失败有负缓存，触发频率极低。
fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Option<Vec<u8>> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW：GUI 下不闪控制台
    }

    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take()?;

    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        use std::io::Read;
        let mut chunk = [0u8; 4096];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if tx.send(chunk[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let deadline = Instant::now() + timeout;
    let mut buf: Vec<u8> = Vec::new();
    let mut exited = false;
    loop {
        while let Ok(chunk) = rx.try_recv() {
            buf.extend_from_slice(&chunk);
        }
        match child.try_wait() {
            Ok(Some(_)) => {
                exited = true;
                break;
            }
            Ok(None) => {
                if Instant::now() > deadline {
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
            Err(_) => break,
        }
    }
    if !exited {
        // 超时或 try_wait 出错：整组击杀 + 回收，避免僵尸/孤儿
        kill_child(&mut child);
        let _ = child.wait();
        return None;
    }
    // shell 已退出，stdout 缓冲已 flush 进管道：50ms 无新数据即收完
    while let Ok(chunk) = rx.recv_timeout(Duration::from_millis(50)) {
        buf.extend_from_slice(&chunk);
    }
    Some(buf)
}

#[cfg(unix)]
fn login_shell_lookup() -> Option<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") { "/bin/zsh".into() } else { "/bin/bash".into() }
    });
    let mut cmd = Command::new(&shell);
    // -l 加载 profile，-i 加载 rc（多数用户 PATH 写在 rc 里），fish 同样兼容这组 flag
    cmd.args(["-ilc", "command -v claude"]);
    let out = run_with_timeout(cmd, LOGIN_SHELL_TIMEOUT)?;
    let text = String::from_utf8_lossy(&out);
    let path = PathBuf::from(parse_shell_output(&text)?);
    is_valid_binary(&path).then_some(path)
}

#[cfg(windows)]
fn login_shell_lookup() -> Option<PathBuf> {
    let mut cmd = Command::new("where");
    cmd.arg("claude");
    let out = run_with_timeout(cmd, LOGIN_SHELL_TIMEOUT)?;
    let text = String::from_utf8_lossy(&out);
    // 只收 .exe：npm 的无扩展名 sh shim 与 .cmd 均不可直接 spawn（BatBadBut）
    text.lines()
        .map(str::trim)
        .filter(|l| l.to_ascii_lowercase().ends_with(".exe"))
        .map(PathBuf::from)
        .find(|p| is_valid_binary(p))
}

// ---------------------------------------------------------------------------
// 探测链
// ---------------------------------------------------------------------------

/// 完整探测（L0-L3），主 App 使用
pub fn locate() -> Result<Located, String> {
    locate_detail(true).map_err(format_err)
}

/// 轻量探测（L0-L2），runner 等贫瘠环境使用——login shell 在 launchd
/// 环境不可靠，重探测由主 App 负责并通过 L1 缓存共享
pub fn locate_lightweight() -> Result<Located, String> {
    locate_detail(false).map_err(format_err)
}

fn format_err(attempted: Vec<String>) -> String {
    format!("未找到 Claude CLI，已尝试：{}", attempted.join("、"))
}

fn locate_detail(allow_login_shell: bool) -> Result<Located, Vec<String>> {
    // 内存命中：免重复读盘；路径失效则作废重探
    {
        let mut guard = MEM_HIT.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hit) = guard.clone() {
            if is_valid_binary(&hit.path) {
                return Ok(hit);
            }
            *guard = None;
        }
    }
    // 负缓存：完整探测刚失败过，60s 内快速失败，不重付 login shell 成本
    if allow_login_shell {
        let guard = MEM_FAIL.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, msg)) = guard.as_ref() {
            if at.elapsed() < FAIL_TTL {
                return Err(msg.split('、').map(String::from).collect());
            }
        }
    }

    let mut attempted: Vec<String> = Vec::new();

    // L0：手动配置。无效时不硬失败，继续向下自愈（UI 会提示手动路径无效）
    if let Some(manual) = read_setting(MANUAL_KEY) {
        let p = expand_tilde(&manual);
        if is_valid_binary(&p) {
            return Ok(remember(Located { path: p, source: LocateSource::Manual }));
        }
        attempted.push(format!("{} (manual, invalid)", manual));
    }

    // L1：缓存。失效即清除，避免反复撞死路径
    if let Some(cached) = read_setting(CACHED_KEY) {
        let p = PathBuf::from(&cached);
        if is_valid_binary(&p) {
            return Ok(remember(Located { path: p, source: LocateSource::Cached }));
        }
        write_setting(CACHED_KEY, None);
    }

    // L2：候选扫描
    for c in candidate_paths() {
        if is_valid_binary(&c) {
            write_setting(CACHED_KEY, Some(&c.to_string_lossy()));
            return Ok(remember(Located { path: c, source: LocateSource::Scan }));
        }
        attempted.push(c.to_string_lossy().into_owned());
    }

    // L3：login shell
    if allow_login_shell {
        if let Some(p) = login_shell_lookup() {
            write_setting(CACHED_KEY, Some(&p.to_string_lossy()));
            return Ok(remember(Located { path: p, source: LocateSource::LoginShell }));
        }
        attempted.push("login shell (command -v claude)".to_string());
        // 只有付出过 L3 成本的完整失败才进负缓存
        *MEM_FAIL.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((Instant::now(), attempted.join("、")));
    }

    Err(attempted)
}

fn remember(located: Located) -> Located {
    let mut guard = MEM_HIT.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(located.clone());
    *MEM_FAIL.lock().unwrap_or_else(|e| e.into_inner()) = None;
    located
}

/// 清缓存后重新完整探测，返回探测后状态（设置页「重新探测」按钮）
pub fn redetect_info() -> LocateInfo {
    write_setting(CACHED_KEY, None);
    invalidate_mem();
    current_info()
}

/// 设置/清除手动路径。设置时校验可执行，清除传 None
pub fn set_manual_path(path: Option<&str>) -> Result<(), String> {
    match path {
        Some(p) if !p.trim().is_empty() => {
            let expanded = expand_tilde(p.trim());
            if !is_valid_binary(&expanded) {
                return Err(format!("路径无效或不可执行: {}", expanded.display()));
            }
            write_setting(MANUAL_KEY, Some(p.trim()));
            invalidate_mem();
            Ok(())
        }
        _ => {
            write_setting(MANUAL_KEY, None);
            invalidate_mem();
            Ok(())
        }
    }
}

/// 当前完整状态（设置页展示）
pub fn current_info() -> LocateInfo {
    let manual = read_setting(MANUAL_KEY);
    let manual_valid = manual
        .as_deref()
        .map(|m| is_valid_binary(&expand_tilde(m)))
        .unwrap_or(false);

    match locate_detail(true) {
        Ok(l) => LocateInfo {
            path: Some(l.path.to_string_lossy().into_owned()),
            source: Some(l.source),
            manual_path: manual,
            manual_valid,
            attempted: Vec::new(),
        },
        Err(attempted) => LocateInfo {
            path: None,
            source: None,
            manual_path: manual,
            manual_valid,
            attempted,
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn parse_shell_output_takes_last_path_line() {
        let dirty = "Welcome banner!\nsome rc noise\n/usr/local/bin/claude\n/Users/x/.local/bin/claude\n";
        assert_eq!(
            parse_shell_output(dirty),
            Some("/Users/x/.local/bin/claude".to_string())
        );
    }

    #[cfg(unix)]
    #[test]
    fn parse_shell_output_rejects_non_path_output() {
        assert_eq!(parse_shell_output("claude not found\n"), None);
        assert_eq!(parse_shell_output(""), None);
    }

    #[test]
    fn expand_tilde_resolves_home() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_tilde("~/x/y"), home.join("x/y"));
        assert_eq!(expand_tilde("~"), home);
        assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
    }

    #[test]
    fn settings_roundtrip() {
        let dir = std::env::temp_dir().join(format!("monet-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("settings.json");

        write_setting_to(&path, "claudeBinaryPath", Some("/a/b"));
        assert_eq!(read_setting_from(&path, "claudeBinaryPath"), Some("/a/b".into()));

        // 写第二个 key 不影响第一个
        write_setting_to(&path, "claudeBinaryPathCached", Some("/c/d"));
        assert_eq!(read_setting_from(&path, "claudeBinaryPath"), Some("/a/b".into()));

        // 删除
        write_setting_to(&path, "claudeBinaryPath", None);
        assert_eq!(read_setting_from(&path, "claudeBinaryPath"), None);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_setting_refuses_to_clobber_corrupt_file() {
        let dir = std::env::temp_dir().join(format!("monet-test-corrupt-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("settings.json");

        // 半截 JSON（模拟撕裂读/手编损坏）
        std::fs::write(&path, "{\"defaultChannel\": \"anthro").unwrap();
        write_setting_to(&path, "claudeBinaryPathCached", Some("/x"));
        // 原内容原封不动，没有被空 map 覆写
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"defaultChannel\": \"anthro"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_with_timeout_survives_background_child_holding_pipe() {
        // 复刻审查实证场景：后台进程持有 stdout 写端，EOF 永不到来，
        // 但增量读应在 shell 退出后拿到已打印的内容
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "sleep 30 & echo /found/it"]);
        let out = run_with_timeout(cmd, Duration::from_secs(5)).expect("should not time out");
        assert!(String::from_utf8_lossy(&out).contains("/found/it"));
    }

    #[test]
    fn run_with_timeout_kills_hung_process_tree() {
        let started = Instant::now();
        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "sleep 60"]);
        assert!(run_with_timeout(cmd, Duration::from_millis(300)).is_none());
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
