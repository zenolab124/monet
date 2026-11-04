//! 远端主机上 Codex 会话的只读镜像：只同步、供检索，不注册为可浏览的引擎实例。
//!
//! 远端家目录为 `CODEX_HOME`（默认 `~/.codex`），只读 `sessions/` 与 `archived_sessions/`
//! 下的 `rollout-*.jsonl`；镜像写在实例根的 `codex/` 下，状态与 Claude 镜像分开记账。
//! Codex 归档会把文件从 `sessions/` 移到 `archived_sessions/`：同名文件换了位置时
//! 搬动本地镜像并沿用已镜像长度，不重拉，也不留下重复的一份。

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::executor::{RemoteExecutor, RemotePlatform};
use super::hosts::SshTarget;
use super::sync::{self, FetchBase, Mirror, RemoteFile, SyncReport, SyncState};
use super::windows;

const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const HOME_MARKER: &str = "MONET_CODEX_HOME=";

/// 只接受 `sessions|archived_sessions/[...]/rollout-*.jsonl`：无 `..`、无隐藏项、无控制字符
pub fn valid_codex_rel(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let (Some(top), Some(name)) = (parts.first(), parts.last()) else {
        return false;
    };
    matches!(*top, "sessions" | "archived_sessions")
        && (2..=6).contains(&parts.len())
        && parts.iter().all(|part| {
            !part.is_empty() && !part.starts_with('.') && !part.chars().any(char::is_control)
        })
        && name.starts_with("rollout-")
        && name.ends_with(".jsonl")
}

/// POSIX 列目录：首行 `<marker><家目录绝对路径>`，其后每行 `大小 修改时间 ./相对路径`
pub fn list_script() -> String {
    format!(
        r#"cd "${{CODEX_HOME:-$HOME/.codex}}" 2>/dev/null || exit 3
printf '%s%s\n' '{HOME_MARKER}' "$(pwd -P)"
{stat}
for d in sessions archived_sessions; do
  [ -d "$d" ] && find "./$d" -type f -name 'rollout-*.jsonl' -exec stat "$o" "$f" {{}} +
done
exit 0
"#,
        stat = sync::STAT_SETUP,
    )
}

/// 解析列目录输出：(远端家目录, 文件)；缺少家目录行时为 None
pub fn parse_listing(stdout: &str) -> Option<(String, Vec<RemoteFile>)> {
    let home = stdout
        .lines()
        .find_map(|line| line.trim_end_matches('\r').strip_prefix(HOME_MARKER))?
        .to_string();
    let files = sync::parse_listing_with(stdout, valid_codex_rel);
    Some((home, files))
}

fn file_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

/// 远端换了位置的同名文件（归档 / 取消归档）：搬动本地镜像与状态，返回搬动的数量
fn carry_moves(mirror: &Mirror, state: &mut SyncState, remote: &[RemoteFile]) -> usize {
    let listed: std::collections::HashSet<&str> = remote.iter().map(|f| f.rel.as_str()).collect();
    let mut vacated: HashMap<String, String> = state
        .files
        .keys()
        .filter(|rel| !listed.contains(rel.as_str()))
        .map(|rel| (file_name(rel).to_string(), rel.clone()))
        .collect();
    let mut moved = 0;
    for file in remote {
        if state.files.contains_key(&file.rel) {
            continue;
        }
        let Some(old) = vacated.remove(file_name(&file.rel)) else {
            continue;
        };
        let (from, to) = (mirror.file_path(&old), mirror.file_path(&file.rel));
        let carried = to
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::rename(&from, &to));
        if carried.is_ok() {
            if let Some(file_state) = state.files.remove(&old) {
                state.files.insert(file.rel.clone(), file_state);
                moved += 1;
            }
        }
    }
    moved
}

/// 同步一轮远端 Codex 会话。远端没有 Codex 家目录时什么也不做（不把已有镜像标记为删除）
pub fn sync_once(
    executor: &dyn RemoteExecutor,
    target: &SshTarget,
    mirror: &Mirror,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<SyncReport, String> {
    let script = match target.platform {
        RemotePlatform::Posix => list_script(),
        RemotePlatform::Windows => windows::codex_list_script(HOME_MARKER),
    };
    let listing = executor
        .run(target, &script, LIST_TIMEOUT)
        .map_err(|e| e.to_string())?;
    if listing.timed_out {
        return Err("list_timeout".into());
    }
    match listing.code {
        Some(0) => {}
        Some(3) => return Ok(SyncReport::default()),
        _ => return Err(format!("list_failed: {}", listing.stderr.trim())),
    }
    let (home, remote) = parse_listing(&listing.stdout).ok_or("codex_home_missing")?;

    let mut state = mirror.load_state();
    state.remote_root = Some(home.clone());
    state.platform = target.platform;
    carry_moves(mirror, &mut state, &remote);
    let report = sync::mirror_listing(
        executor,
        target,
        FetchBase::Dir(&home),
        mirror,
        &mut state,
        &remote,
        cancel,
        &mut progress,
    );
    mirror.save_state(&state).map_err(|e| e.to_string())?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::super::executor::LocalShellExecutor;
    use super::*;
    #[cfg(unix)]
    use std::io::Write;
    #[cfg(unix)]
    use std::path::PathBuf;

    const LIVE: &str = "sessions/2026/09/21/rollout-2026-09-21T08-29-00-abc.jsonl";
    const ARCHIVED: &str = "archived_sessions/rollout-2026-09-21T08-29-00-abc.jsonl";

    #[test]
    fn codex_paths_are_confined_to_session_trees() {
        assert!(valid_codex_rel(LIVE));
        assert!(valid_codex_rel(ARCHIVED));
        assert!(!valid_codex_rel("sessions/2026/../../auth.json"));
        assert!(!valid_codex_rel("config/rollout-x.jsonl"));
        assert!(!valid_codex_rel("sessions/2026/notes.jsonl"));
        assert!(!valid_codex_rel("sessions/.hidden/rollout-x.jsonl"));
        assert!(!valid_codex_rel("sessions"));
    }

    #[test]
    fn listing_needs_the_home_line() {
        let out = format!("{HOME_MARKER}C:\\Users\\a\\.codex\r\n12 34 ./{ARCHIVED}\r\n");
        let (home, files) = parse_listing(&out).unwrap();
        assert_eq!(home, "C:\\Users\\a\\.codex");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].rel, ARCHIVED);
        assert!(parse_listing("12 34 ./sessions/rollout-x.jsonl\n").is_none());
    }

    #[test]
    fn windows_listing_reads_codex_home_and_both_trees() {
        let script = windows::codex_list_script(HOME_MARKER);
        assert!(script.contains("$env:CODEX_HOME"));
        assert!(script.contains("'sessions', 'archived_sessions'"));
        assert!(script.contains("'MONET_CODEX_HOME='"));
    }

    /// 真实主机验收：`MONET_REMOTE_TEST_TARGET=<ssh 别名> cargo test --lib live_codex -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_codex_mirror_is_byte_exact_and_idempotent() {
        let Ok(destination) = std::env::var("MONET_REMOTE_TEST_TARGET") else {
            return;
        };
        let executor = super::super::executor::SshExecutor;
        let probe = SshTarget {
            destination,
            port: None,
            platform: RemotePlatform::Posix,
        };
        let health = super::super::health::check(&executor, &probe, None);
        let target = SshTarget {
            platform: health.platform,
            ..probe
        };
        let dir = std::env::temp_dir().join(format!("monet-live-codex-{}", std::process::id()));
        let mirror = Mirror::codex(dir.clone());
        let started = std::time::Instant::now();
        let report = sync_once(
            &executor,
            &target,
            &mirror,
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        println!(
            "{:?} in {:?}: listed {} fetched {} bytes {} errors {:?}",
            target.platform,
            started.elapsed(),
            report.listed,
            report.fetched,
            report.bytes,
            report.errors
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.fetched, report.listed);
        for rel in mirror.load_state().files.keys() {
            let text = std::fs::read_to_string(mirror.file_path(rel)).unwrap();
            for line in text.lines().filter(|l| !l.is_empty()) {
                assert!(
                    serde_json::from_str::<serde_json::Value>(line).is_ok(),
                    "{rel}"
                );
            }
        }
        let again = sync_once(
            &executor,
            &target,
            &mirror,
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        println!("second pass: fetched {}", again.fetched);
        let _ = std::fs::remove_dir_all(dir);
    }

    // 以下用本机 /bin/sh 充当 POSIX 远端，仅在 Unix 上运行；Windows 脚本由真机验收覆盖
    #[cfg(unix)]
    struct Fixture {
        base: PathBuf,
        home: PathBuf,
        mirror: Mirror,
    }

    #[cfg(unix)]
    impl Fixture {
        fn new(name: &str) -> Self {
            let base = std::env::temp_dir()
                .join(format!("monet-codex-mirror-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            let home = base.join("codex-home");
            std::fs::create_dir_all(&home).unwrap();
            let mirror = Mirror::codex(base.join("mirror"));
            Self { base, home, mirror }
        }

        fn write(&self, rel: &str, text: &str) {
            let path = self.home.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }

        fn append(&self, rel: &str, text: &str) {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(self.home.join(rel))
                .unwrap();
            file.write_all(text.as_bytes()).unwrap();
        }

        /// 本机 sh 充当远端；CODEX_HOME 经脚本前缀注入
        fn sync(&self) -> SyncReport {
            struct WithHome<'a>(&'a std::path::Path);
            impl RemoteExecutor for WithHome<'_> {
                fn run_raw(
                    &self,
                    target: &SshTarget,
                    script: &str,
                    timeout: Duration,
                ) -> std::io::Result<super::super::executor::RawOutput> {
                    let script = format!(
                        "CODEX_HOME={}\n{script}",
                        super::super::executor::sh_quote(self.0.to_str().unwrap())
                    );
                    LocalShellExecutor.run_raw(target, &script, timeout)
                }
            }
            let target = SshTarget {
                destination: "devbox".into(),
                port: None,
                platform: RemotePlatform::Posix,
            };
            sync_once(
                &WithHome(&self.home),
                &target,
                &self.mirror,
                &AtomicBool::new(false),
                |_, _| {},
            )
            .unwrap()
        }

        fn mirrored(&self, rel: &str) -> Option<String> {
            std::fs::read_to_string(self.mirror.file_path(rel)).ok()
        }
    }

    #[cfg(unix)]
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    #[cfg(unix)]
    #[test]
    fn mirrors_appends_and_carries_archived_sessions_without_duplicates() {
        let fx = Fixture::new("archive");
        fx.write(LIVE, "{\"n\":1}\n");
        fx.write("sessions/2026/09/21/notes.txt", "ignored\n");
        let first = fx.sync();
        assert_eq!((first.listed, first.fetched), (1, 1));
        assert_eq!(fx.mirrored(LIVE).as_deref(), Some("{\"n\":1}\n"));
        assert_eq!(
            fx.mirror.load_state().remote_root.as_deref(),
            std::fs::canonicalize(&fx.home).unwrap().to_str()
        );

        // 追加只拉新增字节；半截末行不落盘
        fx.append(LIVE, "{\"n\":2}\n{\"n\":");
        fx.sync();
        assert_eq!(fx.mirrored(LIVE).as_deref(), Some("{\"n\":1}\n{\"n\":2}\n"));

        // 归档：远端移动 → 本地搬动，旧位置不留副本，不被标记为删除
        fx.append(LIVE, "3}\n");
        std::fs::create_dir_all(fx.home.join("archived_sessions")).unwrap();
        std::fs::rename(fx.home.join(LIVE), fx.home.join(ARCHIVED)).unwrap();
        let moved = fx.sync();
        assert_eq!(moved.missing, 0);
        assert_eq!(fx.mirrored(LIVE), None);
        assert_eq!(
            fx.mirrored(ARCHIVED).as_deref(),
            Some("{\"n\":1}\n{\"n\":2}\n{\"n\":3}\n")
        );
        let state = fx.mirror.load_state();
        assert!(!state.files.contains_key(LIVE));
        assert!(!state.files[ARCHIVED].missing);
    }

    #[cfg(unix)]
    #[test]
    fn deleted_sessions_stay_mirrored_and_missing_home_is_a_no_op() {
        let fx = Fixture::new("retain");
        fx.write(LIVE, "{\"n\":1}\n");
        fx.sync();
        std::fs::remove_file(fx.home.join(LIVE)).unwrap();
        let gone = fx.sync();
        assert_eq!(gone.newly_missing, 1);
        assert!(fx.mirrored(LIVE).is_some());

        std::fs::remove_dir_all(&fx.home).unwrap();
        let absent = fx.sync();
        assert_eq!(absent.listed, 0);
        assert!(fx.mirror.load_state().files[LIVE].missing);
        assert!(fx.mirrored(LIVE).is_some());
    }
}
