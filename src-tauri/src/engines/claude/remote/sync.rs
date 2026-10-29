//! 远端会话的只读镜像同步。
//!
//! 远端只执行读命令：一次列出 `projects/*/*.jsonl` 的大小与修改时间，再按需
//! `cat` 或 `tail -c +N` 拉取。镜像写在 Monet 自己的数据目录，远端文件从不被写。
//! JSONL 只追加，所以增量拉取只取已镜像长度之后的字节；半截末行不落盘，下一轮重拉。

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::executor::{sh_quote, RemoteExecutor};
use super::hosts::SshTarget;

const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(120);
const STATE_FILE: &str = "sync-state.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteFile {
    /// `<编码后的项目目录>/<会话 id>.jsonl`
    pub rel: String,
    pub size: u64,
    pub mtime: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileState {
    /// 远端最近一次列出时的大小与修改时间
    pub size: u64,
    pub mtime: i64,
    /// 镜像里已落盘的字节数（只含完整行）
    pub mirrored: u64,
    /// 远端已不存在；镜像保留
    #[serde(default)]
    pub missing: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncState {
    #[serde(default)]
    pub files: BTreeMap<String, FileState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchPlan {
    Skip,
    Append { offset: u64 },
    Full,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub listed: usize,
    pub fetched: usize,
    pub bytes: u64,
    pub missing: usize,
    pub cancelled: bool,
    /// 单个文件失败不中断整轮；记录 `rel: 原因`
    pub errors: Vec<String>,
}

/// 只接受 `<目录>/<名称>.jsonl`：两段、无 `..`、无隐藏项、无控制字符
pub fn valid_rel(rel: &str) -> bool {
    let mut parts = rel.split('/');
    let (Some(dir), Some(file), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    let ok = |part: &str| {
        !part.is_empty()
            && !part.starts_with('.')
            && part != ".."
            && !part.chars().any(char::is_control)
    };
    ok(dir) && ok(file) && file.ends_with(".jsonl") && file.len() > ".jsonl".len()
}

/// 列目录脚本：每行 `大小\t修改时间\t相对路径`；stat 兼容 GNU（-c）与 BSD（-f）。
/// 编码后的项目目录以 `-` 开头，传给命令时一律加 `./`，防止被当成选项
pub fn list_script(root: &str) -> String {
    format!(
        r#"cd {}/projects 2>/dev/null || exit 3
for f in */*.jsonl; do
  [ -f "$f" ] || continue
  s=$(wc -c < "./$f" | tr -d ' ')
  m=$(stat -c %Y "./$f" 2>/dev/null || stat -f %m "./$f" 2>/dev/null)
  printf '%s\t%s\t%s\n' "$s" "$m" "$f"
done
"#,
        sh_quote(root)
    )
}

pub fn parse_listing(stdout: &str) -> Vec<RemoteFile> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, '\t');
            let size = fields.next()?.trim().parse().ok()?;
            let mtime = fields.next()?.trim().parse().ok()?;
            let rel = fields.next()?.to_string();
            valid_rel(&rel).then_some(RemoteFile { rel, size, mtime })
        })
        .collect()
}

pub fn plan(remote: &RemoteFile, local: Option<&FileState>) -> FetchPlan {
    let Some(local) = local else {
        return FetchPlan::Full;
    };
    if remote.size < local.mirrored || remote.mtime < local.mtime {
        return FetchPlan::Full;
    }
    if remote.size > local.mirrored {
        FetchPlan::Append {
            offset: local.mirrored,
        }
    } else {
        FetchPlan::Skip
    }
}

/// 拉取脚本：offset 为 0 时整文件，否则从第 offset+1 个字节起
pub fn fetch_script(root: &str, rel: &str, offset: u64) -> String {
    let path = sh_quote(&format!("{root}/projects/{rel}"));
    if offset == 0 {
        format!("cat {path}")
    } else {
        format!("tail -c +{} {path}", offset + 1)
    }
}

/// 截到最后一个换行（含）；没有换行时为空
pub fn complete_prefix(bytes: &[u8]) -> &[u8] {
    match bytes.iter().rposition(|b| *b == b'\n') {
        Some(end) => &bytes[..=end],
        None => &[],
    }
}

pub struct Mirror {
    root: PathBuf,
}

impl Mirror {
    /// `root` 下放 `projects/<rel>` 镜像与同步状态文件
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.root.join("projects")
    }

    fn file_path(&self, rel: &str) -> PathBuf {
        self.projects_dir().join(rel)
    }

    pub fn load_state(&self) -> SyncState {
        std::fs::read_to_string(self.root.join(STATE_FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save_state(&self, state: &SyncState) -> std::io::Result<()> {
        let text = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
        crate::config::atomic_write(&self.root.join(STATE_FILE), &text)
    }

    /// 落盘一次拉取结果，返回镜像中该文件的新长度
    pub fn apply(&self, rel: &str, plan: FetchPlan, bytes: &[u8]) -> std::io::Result<u64> {
        let path = self.file_path(rel);
        let complete = complete_prefix(bytes);
        match plan {
            FetchPlan::Skip => Ok(std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)),
            FetchPlan::Full => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let tmp = path.with_extension("jsonl.part");
                std::fs::write(&tmp, complete)?;
                std::fs::rename(&tmp, &path)?;
                Ok(complete.len() as u64)
            }
            FetchPlan::Append { offset } => {
                let current = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                if current != offset {
                    return Err(std::io::Error::other(
                        "mirror length differs from recorded state",
                    ));
                }
                let mut file = std::fs::OpenOptions::new().append(true).open(&path)?;
                file.write_all(complete)?;
                Ok(offset + complete.len() as u64)
            }
        }
    }
}

/// 同步一轮：列目录 → 逐个文件按计划拉取 → 保存状态。`cancel` 置位后在文件之间停下。
pub fn sync_once(
    executor: &dyn RemoteExecutor,
    target: &SshTarget,
    remote_root: &str,
    mirror: &Mirror,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Result<SyncReport, String> {
    let listing = executor
        .run(target, &list_script(remote_root), LIST_TIMEOUT)
        .map_err(|e| e.to_string())?;
    if listing.timed_out {
        return Err("list_timeout".into());
    }
    match listing.code {
        Some(0) => {}
        Some(3) => return Err("projects_missing".into()),
        _ => return Err(format!("list_failed: {}", listing.stderr.trim())),
    }
    let remote = parse_listing(&listing.stdout);
    let mut state = mirror.load_state();
    let mut report = SyncReport {
        listed: remote.len(),
        ..SyncReport::default()
    };
    let present: std::collections::HashSet<&str> = remote.iter().map(|f| f.rel.as_str()).collect();

    let pending: Vec<(&RemoteFile, FetchPlan)> = remote
        .iter()
        .map(|file| (file, plan(file, state.files.get(&file.rel))))
        .filter(|(_, plan)| *plan != FetchPlan::Skip)
        .collect();
    for (index, (file, file_plan)) in pending.iter().enumerate() {
        progress(index, pending.len());
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let offset = match file_plan {
            FetchPlan::Append { offset } => *offset,
            _ => 0,
        };
        let result = executor
            .run_raw(
                target,
                &fetch_script(remote_root, &file.rel, offset),
                FETCH_TIMEOUT,
            )
            .map_err(|e| e.to_string())
            .and_then(|out| match (out.timed_out, out.code) {
                (true, _) => Err("fetch_timeout".into()),
                (false, Some(0)) => Ok(out.stdout),
                _ => Err(format!("fetch_failed: {}", out.stderr.trim())),
            })
            .and_then(|bytes| {
                mirror
                    .apply(&file.rel, *file_plan, &bytes)
                    .map(|len| (len, bytes.len() as u64))
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok((mirrored, fetched_bytes)) => {
                report.fetched += 1;
                report.bytes += fetched_bytes;
                state.files.insert(
                    file.rel.clone(),
                    FileState {
                        size: file.size,
                        mtime: file.mtime,
                        mirrored,
                        missing: false,
                    },
                );
            }
            // 镜像长度与记录不符：清掉记录，下一轮全量重拉
            Err(error) => {
                state.files.remove(&file.rel);
                report.errors.push(format!("{}: {error}", file.rel));
            }
        }
    }
    progress(pending.len(), pending.len());

    for (rel, file_state) in state.files.iter_mut() {
        let gone = !present.contains(rel.as_str());
        if gone && !file_state.missing {
            file_state.missing = true;
        }
        if gone {
            report.missing += 1;
        }
    }
    mirror.save_state(&state).map_err(|e| e.to_string())?;
    Ok(report)
}

pub fn mirror_root_for(instance_storage_key: &str) -> PathBuf {
    crate::config::data_dir()
        .join("remote")
        .join(instance_storage_key)
}

/// 便于外部读取镜像目录的会话文件（供后续历史源使用）
pub fn mirrored_path(mirror_root: &Path, rel: &str) -> Option<PathBuf> {
    valid_rel(rel).then(|| mirror_root.join("projects").join(rel))
}

#[cfg(test)]
mod tests {
    use super::super::executor::LocalShellExecutor;
    use super::*;

    fn target() -> SshTarget {
        SshTarget {
            destination: "devbox".into(),
            port: None,
        }
    }

    struct Fixture {
        base: PathBuf,
        remote: PathBuf,
        mirror: Mirror,
    }

    impl Fixture {
        fn new(name: &str) -> Self {
            let base = std::env::temp_dir()
                .join(format!("monet-remote-sync-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            let remote = base.join("remote-root");
            std::fs::create_dir_all(remote.join("projects/-home-alice-foo")).unwrap();
            let mirror = Mirror::new(base.join("mirror"));
            Self {
                base,
                remote,
                mirror,
            }
        }

        fn write_remote(&self, rel: &str, text: &str) {
            let path = self.remote.join("projects").join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }

        fn append_remote(&self, rel: &str, text: &str) {
            let mut file = std::fs::OpenOptions::new()
                .append(true)
                .open(self.remote.join("projects").join(rel))
                .unwrap();
            file.write_all(text.as_bytes()).unwrap();
        }

        fn sync(&self) -> SyncReport {
            sync_once(
                &LocalShellExecutor,
                &target(),
                self.remote.to_str().unwrap(),
                &self.mirror,
                &AtomicBool::new(false),
                |_, _| {},
            )
            .unwrap()
        }

        fn mirrored(&self, rel: &str) -> String {
            std::fs::read_to_string(self.mirror.projects_dir().join(rel)).unwrap()
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn rel_paths_must_be_one_project_dir_and_one_jsonl() {
        assert!(valid_rel("-home-alice-foo/abc.jsonl"));
        for rel in [
            "abc.jsonl",
            "a/b/c.jsonl",
            "../x.jsonl",
            "a/../x.jsonl",
            ".hidden/x.jsonl",
            "a/.jsonl",
            "a/x.txt",
            "a/x\n.jsonl",
        ] {
            assert!(!valid_rel(rel), "{rel:?}");
        }
    }

    #[test]
    fn listing_ignores_malformed_lines_and_unsafe_paths() {
        let files =
            parse_listing("12\t100\tp/a.jsonl\nbad line\n5\tx\tp/b.jsonl\n7\t3\t../c.jsonl\n");
        assert_eq!(
            files,
            vec![RemoteFile {
                rel: "p/a.jsonl".into(),
                size: 12,
                mtime: 100
            }]
        );
    }

    #[test]
    fn plan_covers_new_appended_unchanged_and_rewritten_files() {
        let file = |size, mtime| RemoteFile {
            rel: "p/a.jsonl".into(),
            size,
            mtime,
        };
        let state = FileState {
            size: 10,
            mtime: 100,
            mirrored: 10,
            missing: false,
        };
        assert_eq!(plan(&file(10, 100), None), FetchPlan::Full);
        assert_eq!(plan(&file(10, 100), Some(&state)), FetchPlan::Skip);
        assert_eq!(
            plan(&file(15, 120), Some(&state)),
            FetchPlan::Append { offset: 10 }
        );
        assert_eq!(plan(&file(4, 130), Some(&state)), FetchPlan::Full);
        assert_eq!(plan(&file(15, 90), Some(&state)), FetchPlan::Full);
    }

    #[test]
    fn complete_prefix_drops_a_partial_last_line() {
        assert_eq!(complete_prefix(b"a\nb\nc"), b"a\nb\n");
        assert_eq!(complete_prefix(b"no newline"), b"");
        assert_eq!(complete_prefix(b"x\n"), b"x\n");
    }

    #[test]
    fn first_sync_mirrors_whole_files_then_appends_only_new_bytes() {
        let fx = Fixture::new("append");
        fx.write_remote("-home-alice-foo/s1.jsonl", "{\"a\":1}\n{\"a\":2}\n");
        let first = fx.sync();
        assert_eq!((first.listed, first.fetched), (1, 1));
        assert_eq!(
            fx.mirrored("-home-alice-foo/s1.jsonl"),
            "{\"a\":1}\n{\"a\":2}\n"
        );

        let unchanged = fx.sync();
        assert_eq!(unchanged.fetched, 0);

        fx.append_remote("-home-alice-foo/s1.jsonl", "{\"a\":3}\n");
        let appended = fx.sync();
        assert_eq!(appended.fetched, 1);
        assert_eq!(appended.bytes, "{\"a\":3}\n".len() as u64);
        assert_eq!(
            fx.mirrored("-home-alice-foo/s1.jsonl"),
            "{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n"
        );
    }

    #[test]
    fn partial_last_line_is_held_back_until_it_completes() {
        let fx = Fixture::new("partial");
        fx.write_remote("-home-alice-foo/s1.jsonl", "{\"a\":1}\n{\"a\":");
        fx.sync();
        assert_eq!(fx.mirrored("-home-alice-foo/s1.jsonl"), "{\"a\":1}\n");
        fx.append_remote("-home-alice-foo/s1.jsonl", "2}\n");
        fx.sync();
        assert_eq!(
            fx.mirrored("-home-alice-foo/s1.jsonl"),
            "{\"a\":1}\n{\"a\":2}\n"
        );
    }

    #[test]
    fn shrunk_file_is_fetched_again_in_full() {
        let fx = Fixture::new("shrink");
        fx.write_remote("-home-alice-foo/s1.jsonl", "{\"a\":1}\n{\"a\":2}\n");
        fx.sync();
        fx.write_remote("-home-alice-foo/s1.jsonl", "{\"b\":1}\n");
        fx.sync();
        assert_eq!(fx.mirrored("-home-alice-foo/s1.jsonl"), "{\"b\":1}\n");
    }

    #[test]
    fn deleted_remote_file_keeps_its_mirror_and_is_marked_missing() {
        let fx = Fixture::new("missing");
        fx.write_remote("-home-alice-foo/s1.jsonl", "{\"a\":1}\n");
        fx.sync();
        std::fs::remove_file(fx.remote.join("projects/-home-alice-foo/s1.jsonl")).unwrap();
        let report = fx.sync();
        assert_eq!(report.missing, 1);
        assert_eq!(fx.mirrored("-home-alice-foo/s1.jsonl"), "{\"a\":1}\n");
        assert!(fx.mirror.load_state().files["-home-alice-foo/s1.jsonl"].missing);
    }

    #[test]
    fn subagent_files_and_paths_with_quotes_are_handled() {
        let fx = Fixture::new("quotes");
        fx.write_remote("-home-alice-it's/s1.jsonl", "{\"q\":1}\n");
        fx.write_remote(
            "-home-alice-foo/s2/subagents/agent-1.jsonl",
            "{\"sub\":1}\n",
        );
        let report = fx.sync();
        assert_eq!(
            report.listed, 1,
            "subagent files are fetched on demand, not listed"
        );
        assert_eq!(fx.mirrored("-home-alice-it's/s1.jsonl"), "{\"q\":1}\n");
    }

    #[test]
    fn missing_projects_dir_is_reported() {
        let fx = Fixture::new("noprojects");
        std::fs::remove_dir_all(fx.remote.join("projects")).unwrap();
        let error = sync_once(
            &LocalShellExecutor,
            &target(),
            fx.remote.to_str().unwrap(),
            &fx.mirror,
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap_err();
        assert_eq!(error, "projects_missing");
    }

    #[test]
    fn cancel_stops_between_files_and_keeps_finished_ones() {
        let fx = Fixture::new("cancel");
        fx.write_remote("-home-alice-foo/s1.jsonl", "{\"a\":1}\n");
        fx.write_remote("-home-alice-foo/s2.jsonl", "{\"a\":2}\n");
        let cancel = AtomicBool::new(false);
        let report = sync_once(
            &LocalShellExecutor,
            &target(),
            fx.remote.to_str().unwrap(),
            &fx.mirror,
            &cancel,
            |done, _| {
                if done == 1 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        )
        .unwrap();
        assert!(report.cancelled);
        assert_eq!(report.fetched, 1);
        assert_eq!(fx.mirror.load_state().files.len(), 1);
    }

    #[test]
    fn sync_scripts_only_read_the_remote() {
        let scripts = [
            list_script("/r"),
            fetch_script("/r", "p/a.jsonl", 0),
            fetch_script("/r", "p/a.jsonl", 9),
        ];
        for script in &scripts {
            for forbidden in [" rm ", "mv ", "cp ", "touch", "mkdir", "tee ", "chmod"] {
                assert!(!script.contains(forbidden), "{forbidden} in {script}");
            }
            let redirects = script.matches('>').count() - script.matches("2>/dev/null").count();
            assert_eq!(redirects, 0, "{script}");
        }
        assert_eq!(
            fetch_script("/r", "p/a.jsonl", 9),
            "tail -c +10 '/r/projects/p/a.jsonl'"
        );
    }
}
