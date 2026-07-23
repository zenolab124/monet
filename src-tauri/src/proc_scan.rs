//! 进程扫描：外部会话探测的底层数据源。
//!
//! macOS 走纯 syscall（proc_listpids + KERN_PROCARGS2 / proc_pidinfo），零子进程。
//! 此前每轮探测 spawn `ps` 全量扫描，而每次进程创建都会连带拉起 trustd 签名校验、
//! tccd 检查与 logd 记账，在秒级节拍的常驻轮询下这套系统级开销远超 ps 本身。
//! 其他平台保留 `ps` 降级路径（Windows 无 ps，Command 失败时返回空表优雅降级）。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 短缓存 TTL：须小于前端 3s 探测节拍，保证同一实例相邻两轮必然跨缓存窗口。
/// 缓存的意义在横向共享——工作台多列/档案馆多个探测方同窗口内只触发一次全量扫描
const CACHE_TTL: Duration = Duration::from_secs(2);

/// 短缓存槽：扫描时刻 + 快照
type CacheSlot<T> = Mutex<Option<(Instant, T)>>;

static COMMANDS_CACHE: CacheSlot<Vec<(u32, String)>> = Mutex::new(None);
static TABLE_CACHE: CacheSlot<HashMap<u32, (u32, String)>> = Mutex::new(None);

fn cached<T: Clone>(slot: &CacheSlot<T>, fetch: impl FnOnce() -> T) -> T {
    let mut guard = slot.lock().unwrap();
    if let Some((at, data)) = guard.as_ref() {
        if at.elapsed() < CACHE_TTL {
            return data.clone();
        }
    }
    let fresh = fetch();
    *guard = Some((Instant::now(), fresh.clone()));
    fresh
}

/// 全部进程的 (pid, 完整命令行)。命令行为 argv 以单空格拼接，与 `ps -axo command=` 同构。
/// 读不到 argv 的进程（跨用户、内核任务、竞态退出）直接跳过。已知边界：ps 是
/// setuid root 能读跨用户 argv，本路径读不到——sudo/root 跑的 claude 检测不到，
/// 接受（另一用户不持有本用户的会话文件，实际不会出现）。
pub fn list_commands() -> Vec<(u32, String)> {
    imp::list_commands()
}

/// list_commands 的短缓存版：秒级轮询探测用（kill 等落点动作须走新鲜版，防 pid 复用误伤）
pub fn list_commands_cached() -> Vec<(u32, String)> {
    cached(&COMMANDS_CACHE, imp::list_commands)
}

/// 进程族谱表（短缓存版）：pid → (ppid, 进程名)。归属应用名展示用，秒级旧数据无碍
pub fn process_table_cached() -> HashMap<u32, (u32, String)> {
    cached(&TABLE_CACHE, imp::process_table)
}

#[cfg(target_os = "macos")]
mod imp {
    use libc::{c_int, c_void};
    use std::collections::HashMap;

    // XNU 固定值，自声明不依赖 libc 版本是否导出
    const CTL_KERN: c_int = 1;
    const KERN_ARGMAX: c_int = 8;
    const KERN_PROCARGS2: c_int = 49;
    const PROC_ALL_PIDS: u32 = 1;
    const PROC_PIDTBSDINFO: c_int = 3;
    const PROC_PIDPATHINFO_MAXSIZE: usize = 4096;

    extern "C" {
        fn proc_listpids(kind: u32, typeinfo: u32, buffer: *mut c_void, buffersize: c_int) -> c_int;
        fn proc_pidinfo(pid: c_int, flavor: c_int, arg: u64, buffer: *mut c_void, buffersize: c_int) -> c_int;
        fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffersize: u32) -> c_int;
    }

    /// 全部 pid。两段式：先探需求量再取数，快照间隙的进程增减由 32 席余量吸收
    fn all_pids() -> Vec<u32> {
        unsafe {
            let need = proc_listpids(PROC_ALL_PIDS, 0, std::ptr::null_mut(), 0);
            if need <= 0 {
                return Vec::new();
            }
            let cap = need as usize / std::mem::size_of::<c_int>() + 32;
            let mut buf: Vec<c_int> = vec![0; cap];
            let got = proc_listpids(
                PROC_ALL_PIDS,
                0,
                buf.as_mut_ptr() as *mut c_void,
                (cap * std::mem::size_of::<c_int>()) as c_int,
            );
            if got <= 0 {
                return Vec::new();
            }
            buf.truncate((got as usize / std::mem::size_of::<c_int>()).min(cap));
            buf.into_iter().filter(|&p| p > 0).map(|p| p as u32).collect()
        }
    }

    /// 内核 argv 区上限（KERN_PROCARGS2 单进程 buffer 需求的天花板）
    fn argmax() -> usize {
        let mut value: c_int = 0;
        let mut size = std::mem::size_of::<c_int>();
        let mut mib = [CTL_KERN, KERN_ARGMAX];
        let ret = unsafe {
            libc::sysctl(
                mib.as_mut_ptr(),
                2,
                &mut value as *mut _ as *mut c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        if ret == 0 && value > 0 {
            value as usize
        } else {
            256 * 1024
        }
    }

    /// KERN_PROCARGS2 布局解析：[argc: i32][exec_path\0][\0 填充…][argv0\0]…[环境变量…]
    /// 取 argc 个 argv 以空格拼接（环境变量不读）。独立出来便于单测。
    fn parse_procargs2(buf: &[u8]) -> Option<String> {
        let argc = i32::from_ne_bytes(buf.get(..4)?.try_into().ok()?);
        if argc <= 0 {
            return None;
        }
        let rest = buf.get(4..)?;
        let mut pos = rest.iter().position(|&b| b == 0)?;
        while pos < rest.len() && rest[pos] == 0 {
            pos += 1;
        }
        // 上界防御：argc 来自内核可信，但预分配仍以数据量封顶
        let mut args: Vec<&[u8]> = Vec::with_capacity((argc as usize).min(rest.len() / 2 + 1));
        let mut start = pos;
        for i in pos..rest.len() {
            if rest[i] == 0 {
                args.push(&rest[start..i]);
                start = i + 1;
                if args.len() == argc as usize {
                    break;
                }
            }
        }
        if args.is_empty() {
            return None;
        }
        Some(
            args.iter()
                .map(|a| String::from_utf8_lossy(a))
                .collect::<Vec<_>>()
                .join(" "),
        )
    }

    pub fn list_commands() -> Vec<(u32, String)> {
        let max = argmax();
        let mut buf: Vec<u8> = vec![0; max];
        all_pids()
            .into_iter()
            .filter_map(|pid| {
                let mut size = max;
                let mut mib = [CTL_KERN, KERN_PROCARGS2, pid as c_int];
                let ret = unsafe {
                    libc::sysctl(
                        mib.as_mut_ptr(),
                        3,
                        buf.as_mut_ptr() as *mut c_void,
                        &mut size,
                        std::ptr::null_mut(),
                        0,
                    )
                };
                if ret != 0 || size < 4 {
                    return None;
                }
                parse_procargs2(&buf[..size]).map(|cmd| (pid, cmd))
            })
            .collect()
    }

    pub fn process_table() -> HashMap<u32, (u32, String)> {
        all_pids()
            .into_iter()
            .filter_map(|pid| {
                let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
                let size = std::mem::size_of::<libc::proc_bsdinfo>() as c_int;
                let got = unsafe {
                    proc_pidinfo(pid as c_int, PROC_PIDTBSDINFO, 0, &mut info as *mut _ as *mut c_void, size)
                };
                if got != size {
                    return None;
                }
                let name = exe_basename(pid).unwrap_or_else(|| comm_string(&info));
                if name.is_empty() {
                    return None;
                }
                Some((pid, (info.pbi_ppid, name)))
            })
            .collect()
    }

    /// 可执行路径 basename。相比 pbi_comm 不受 16 字节截断；相比 `ps -o comm=`
    /// 不受「路径含空格被按空白切分截断」影响（如 "iStat Menus Status" 此前只剩 "iStat"）
    fn exe_basename(pid: u32) -> Option<String> {
        let mut buf = [0u8; PROC_PIDPATHINFO_MAXSIZE];
        let n = unsafe { proc_pidpath(pid as c_int, buf.as_mut_ptr() as *mut c_void, buf.len() as u32) };
        if n <= 0 {
            return None;
        }
        let path = String::from_utf8_lossy(&buf[..n as usize]);
        let base = path.rsplit('/').next().unwrap_or(&path).to_string();
        (!base.is_empty()).then_some(base)
    }

    /// pbi_comm 兜底（proc_pidpath 对部分系统进程会失败）
    fn comm_string(info: &libc::proc_bsdinfo) -> String {
        let bytes: Vec<u8> = info
            .pbi_comm
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| c as u8)
            .collect();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// 按 KERN_PROCARGS2 布局构造合成 buffer
        fn build(argc: i32, exec_path: &str, padding: usize, args: &[&str], env: &[&str]) -> Vec<u8> {
            let mut buf = argc.to_ne_bytes().to_vec();
            buf.extend_from_slice(exec_path.as_bytes());
            buf.push(0);
            buf.extend(std::iter::repeat(0).take(padding));
            for a in args {
                buf.extend_from_slice(a.as_bytes());
                buf.push(0);
            }
            for e in env {
                buf.extend_from_slice(e.as_bytes());
                buf.push(0);
            }
            buf
        }

        #[test]
        fn parses_argv_and_skips_env() {
            let buf = build(3, "/usr/local/bin/claude", 3, &["claude", "--resume", "abc-123"], &["HOME=/tmp", "PATH=/bin"]);
            assert_eq!(parse_procargs2(&buf).as_deref(), Some("claude --resume abc-123"));
        }

        #[test]
        fn no_padding_single_arg() {
            let buf = build(1, "/bin/ls", 0, &["ls"], &[]);
            assert_eq!(parse_procargs2(&buf).as_deref(), Some("ls"));
        }

        #[test]
        fn arg_with_spaces_stays_one_token_joined() {
            // 含空格参数与 ps 同构：直接空格拼接（探测按 token 匹配 UUID，不受影响）
            let buf = build(2, "/bin/echo", 5, &["echo", "hello world"], &[]);
            assert_eq!(parse_procargs2(&buf).as_deref(), Some("echo hello world"));
        }

        #[test]
        fn rejects_garbage() {
            assert_eq!(parse_procargs2(&[]), None);
            assert_eq!(parse_procargs2(&[0, 0]), None);
            assert_eq!(parse_procargs2(&0i32.to_ne_bytes()), None);
            // argc 声称 5 但数据截断：取到多少算多少（内核 buffer 截断场景）
            let buf = build(5, "/bin/x", 2, &["x", "y"], &[]);
            assert_eq!(parse_procargs2(&buf).as_deref(), Some("x y"));
        }

        #[test]
        fn smoke_finds_own_process() {
            let me = std::process::id();
            let cmds = list_commands();
            assert!(cmds.iter().any(|(pid, _)| *pid == me), "扫描结果应包含测试进程自身");
            let table = process_table();
            let (ppid, name) = table.get(&me).expect("族谱表应包含测试进程自身");
            assert!(*ppid > 0);
            assert!(!name.is_empty());
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::collections::HashMap;

    pub fn list_commands() -> Vec<(u32, String)> {
        let Ok(output) = std::process::Command::new("ps")
            .args(["-axo", "pid=,command="])
            .output()
        else {
            return Vec::new();
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|l| {
                let trimmed = l.trim_start();
                let (pid_str, cmd) = trimmed.split_once(char::is_whitespace)?;
                Some((pid_str.parse().ok()?, cmd.to_string()))
            })
            .collect()
    }

    /// 进程名取 comm 首 token 的 basename（comm 是完整可执行路径，含空格的
    /// 应用名会被截到首段，够识别用）
    pub fn process_table() -> HashMap<u32, (u32, String)> {
        let Ok(output) = std::process::Command::new("ps")
            .args(["-axo", "pid=,ppid=,comm="])
            .output()
        else {
            return Default::default();
        };
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                let pid: u32 = it.next()?.parse().ok()?;
                let ppid: u32 = it.next()?.parse().ok()?;
                let comm = it.next().unwrap_or("");
                let name = comm.rsplit('/').next().unwrap_or(comm).to_string();
                Some((pid, (ppid, name)))
            })
            .collect()
    }
}
