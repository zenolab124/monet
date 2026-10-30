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

use super::executor::{fits_windows_command, sh_quote, RemoteExecutor, RemotePlatform};
use super::hosts::SshTarget;
use super::windows;

const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const FETCH_TIMEOUT: Duration = Duration::from_secs(120);
const STATE_FILE: &str = "sync-state.json";
/// Windows 批量拉取：每批的文件数与字节数上限（命令行长度另行检查）
const BATCH_MAX_FILES: usize = 64;
const BATCH_MAX_BYTES: u64 = 16 * 1024 * 1024;

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
    /// 最近一次全量同步用的远端数据根；单文件跟随据此免去每次体检
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_root: Option<String>,
    /// 子 Agent 文件（`<项目目录>/<会话 id>/subagents/...`）：打开会话时按需拉取，
    /// 不参与全量索引，也不因主列表里没有而被标记为远端已删除
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub subagents: BTreeMap<String, FileState>,
    /// 最近一次全量同步识别出的远端方言；跟随、子 Agent 拉取与进程操作据此选脚本
    #[serde(default)]
    pub platform: RemotePlatform,
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
    /// 本轮镜像内容或远端存在状态有变化的 `rel`，用于按会话增量刷新
    #[serde(skip)]
    pub changed: Vec<String>,
    /// 本轮新发现远端已删除的文件数（摘要要带上标记，界面需整实例重载）
    #[serde(skip)]
    pub newly_missing: usize,
}

/// 单文件跟随的结果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Follow {
    Unchanged,
    Updated,
    /// 远端已删除，镜像保留并标记
    Gone,
}

/// 只接受 `<目录>/<名称>.jsonl`：两段、无 `..`、无隐藏项、无控制字符
pub fn valid_rel(rel: &str) -> bool {
    let mut parts = rel.split('/');
    let (Some(dir), Some(file), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    safe_part(dir) && safe_part(file) && file.ends_with(".jsonl") && file.len() > ".jsonl".len()
}

fn safe_part(part: &str) -> bool {
    !part.is_empty() && !part.starts_with('.') && !part.chars().any(char::is_control)
}

/// 子 Agent 目录内的相对路径：`<名称>` 或 `workflows/<工作流>/<名称>`；
/// 名称为按行追加的 `.jsonl`，或整份写入的 `.json`（元数据）
pub fn valid_subagent_rel(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let name = match parts.as_slice() {
        [name] => *name,
        ["workflows", workflow, name] if safe_part(workflow) => *name,
        _ => return false,
    };
    safe_part(name)
        && [".jsonl", ".json"]
            .iter()
            .any(|ext| name.ends_with(ext) && name.len() > ext.len())
}

/// 列目录脚本：每行 `大小 修改时间 ./相对路径`。用 `find -exec stat {} +` 批量取元数据，
/// 一万个文件也只起少量进程；stat 兼容 GNU（-c）与 BSD（-f）。
/// 编码后的项目目录以 `-` 开头，路径一律以 `./` 开头，不会被当成选项
pub fn list_script(root: &str) -> String {
    format!(
        r#"cd {}/projects 2>/dev/null || exit 3
{STAT_SETUP}
find . -mindepth 2 -maxdepth 2 -type f -name '*.jsonl' -exec stat "$o" "$f" {{}} +
exit 0
"#,
        sh_quote(root)
    )
}

/// 选 stat 的参数：GNU `-c '%s %Y %n'`，BSD `-f '%z %m %N'`
const STAT_SETUP: &str = r#"if stat -c %s . 2>/dev/null 1>/dev/null; then o=-c; f='%s %Y %n'; else o=-f; f='%z %m %N'; fi"#;

/// 列一个会话的子 Agent 文件，路径相对 `subagents/`；目录不存在时输出为空
pub fn subagent_list_script(root: &str, session_dir: &str) -> String {
    format!(
        r#"cd {} 2>/dev/null || exit 0
{STAT_SETUP}
find . -maxdepth 3 -type f \( -name '*.jsonl' -o -name '*.json' \) -exec stat "$o" "$f" {{}} +
exit 0
"#,
        sh_quote(&format!("{root}/projects/{session_dir}/subagents"))
    )
}

/// 只列一个文件（打开中的会话跟随用）；文件不存在时输出为空
pub fn stat_script(root: &str, rel: &str) -> String {
    let path = format!("./{}", sh_quote(rel));
    format!(
        r#"cd {}/projects 2>/dev/null || exit 3
[ -f {path} ] || exit 0
{STAT_SETUP}
stat "$o" "$f" {path}
"#,
        sh_quote(root)
    )
}

pub fn parse_listing(stdout: &str) -> Vec<RemoteFile> {
    parse_listing_with(stdout, valid_rel)
}

fn parse_listing_with(stdout: &str, valid: fn(&str) -> bool) -> Vec<RemoteFile> {
    stdout
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(3, [' ', '\t']);
            let size = fields.next()?.trim().parse().ok()?;
            let mtime = fields.next()?.trim().parse().ok()?;
            let rest = fields.next()?;
            let rel = rest.strip_prefix("./").unwrap_or(rest).to_string();
            valid(&rel).then_some(RemoteFile { rel, size, mtime })
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

/// 整份写入的 JSON：大小或修改时间任一不同就整份重拉
pub fn plan_whole(remote: &RemoteFile, local: Option<&FileState>) -> FetchPlan {
    match local {
        Some(local) if local.size == remote.size && local.mtime == remote.mtime => FetchPlan::Skip,
        _ => FetchPlan::Full,
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

    /// 落盘一次拉取结果，返回镜像中该文件的新长度。JSONL 只落完整行；
    /// `whole` 为整份文件（JSON 元数据），原样写入
    pub fn apply(
        &self,
        rel: &str,
        plan: FetchPlan,
        bytes: &[u8],
        whole: bool,
    ) -> std::io::Result<u64> {
        let path = self.file_path(rel);
        let complete = if whole { bytes } else { complete_prefix(bytes) };
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

fn plan_offset(file_plan: FetchPlan) -> u64 {
    match file_plan {
        FetchPlan::Append { offset } => offset,
        _ => 0,
    }
}

/// 取一批文件从各自 offset 起的字节；结果与请求一一对应。
/// POSIX 逐个 `cat`/`tail`（连接复用下开销很小）；Windows 一次 PowerShell 处理整批
fn fetch_remote(
    executor: &dyn RemoteExecutor,
    target: &SshTarget,
    remote_root: &str,
    items: &[(&str, u64)],
) -> Vec<Result<Vec<u8>, String>> {
    let check = |out: std::io::Result<super::executor::RawOutput>| {
        out.map_err(|e| e.to_string())
            .and_then(|out| match (out.timed_out, out.code) {
                (true, _) => Err("fetch_timeout".to_string()),
                (false, Some(0)) => Ok(out.stdout),
                _ => Err(format!("fetch_failed: {}", out.stderr.trim())),
            })
    };
    match target.platform {
        RemotePlatform::Posix => items
            .iter()
            .map(|(rel, offset)| {
                check(executor.run_raw(
                    target,
                    &fetch_script(remote_root, rel, *offset),
                    FETCH_TIMEOUT,
                ))
            })
            .collect(),
        RemotePlatform::Windows => {
            let frames = check(executor.run_raw(
                target,
                &windows::fetch_batch_script(remote_root, items),
                FETCH_TIMEOUT,
            ))
            .and_then(|stdout| windows::parse_batch(&stdout, items.len()));
            match frames {
                Ok(frames) => frames
                    .into_iter()
                    .map(|frame| frame.ok_or_else(|| "fetch_missing".to_string()))
                    .collect(),
                Err(error) => items.iter().map(|_| Err(error.clone())).collect(),
            }
        }
    }
}

/// 落盘一次拉取结果并更新状态；失败时清掉记录，下一轮全量重拉。
/// 返回本次拉取的原始字节数，以及镜像内容是否变化（只拉到半截末行时不算）。
fn record_fetch(
    mirror: &Mirror,
    files: &mut BTreeMap<String, FileState>,
    file: &RemoteFile,
    file_plan: FetchPlan,
    whole: bool,
    fetched: Result<Vec<u8>, String>,
) -> Result<(u64, bool), String> {
    let before = files.get(&file.rel).map(|state| state.mirrored);
    let result = fetched.and_then(|bytes| {
        mirror
            .apply(&file.rel, file_plan, &bytes, whole)
            .map(|len| (len, bytes.len() as u64))
            .map_err(|e| e.to_string())
    });
    match result {
        Ok((mirrored, fetched_bytes)) => {
            let grew = file_plan == FetchPlan::Full || before != Some(mirrored);
            files.insert(
                file.rel.clone(),
                FileState {
                    size: file.size,
                    mtime: file.mtime,
                    mirrored,
                    missing: false,
                },
            );
            Ok((fetched_bytes, grew))
        }
        Err(error) => {
            files.remove(&file.rel);
            Err(error)
        }
    }
}

/// 按计划拉取单个文件并落盘（跟随与子 Agent 用）
#[allow(clippy::too_many_arguments)]
fn fetch_file(
    executor: &dyn RemoteExecutor,
    target: &SshTarget,
    remote_root: &str,
    mirror: &Mirror,
    files: &mut BTreeMap<String, FileState>,
    file: &RemoteFile,
    file_plan: FetchPlan,
    whole: bool,
) -> Result<(u64, bool), String> {
    let fetched = fetch_remote(
        executor,
        target,
        remote_root,
        &[(file.rel.as_str(), plan_offset(file_plan))],
    )
    .pop()
    .unwrap_or_else(|| Err("fetch_failed".into()));
    record_fetch(mirror, files, file, file_plan, whole, fetched)
}

/// 全量同步的拉取分组：POSIX 每组一个文件；Windows 在文件数、字节数与命令行长度内尽量多装
fn fetch_groups(
    platform: RemotePlatform,
    remote_root: &str,
    pending: &[(&RemoteFile, FetchPlan)],
) -> Vec<std::ops::Range<usize>> {
    if platform == RemotePlatform::Posix {
        return (0..pending.len()).map(|i| i..i + 1).collect();
    }
    let mut groups = Vec::new();
    let mut start = 0;
    while start < pending.len() {
        let mut end = start + 1;
        let mut bytes = pending[start]
            .0
            .size
            .saturating_sub(plan_offset(pending[start].1));
        while end < pending.len() && end - start < BATCH_MAX_FILES {
            let (file, file_plan) = pending[end];
            let next = bytes + file.size.saturating_sub(plan_offset(file_plan));
            if next > BATCH_MAX_BYTES {
                break;
            }
            let items: Vec<(&str, u64)> = pending[start..=end]
                .iter()
                .map(|(f, p)| (f.rel.as_str(), plan_offset(*p)))
                .collect();
            if !fits_windows_command(&windows::fetch_batch_script(remote_root, &items)) {
                break;
            }
            bytes = next;
            end += 1;
        }
        groups.push(start..end);
        start = end;
    }
    groups
}

/// 按远端方言选择列目录类脚本
fn listing_script(platform: RemotePlatform, remote_root: &str) -> String {
    match platform {
        RemotePlatform::Posix => list_script(remote_root),
        RemotePlatform::Windows => windows::list_script(remote_root),
    }
}

fn single_stat_script(platform: RemotePlatform, remote_root: &str, rel: &str) -> String {
    match platform {
        RemotePlatform::Posix => stat_script(remote_root, rel),
        RemotePlatform::Windows => windows::stat_script(remote_root, rel),
    }
}

fn subagents_script(platform: RemotePlatform, remote_root: &str, session_dir: &str) -> String {
    match platform {
        RemotePlatform::Posix => subagent_list_script(remote_root, session_dir),
        RemotePlatform::Windows => windows::subagent_list_script(remote_root, session_dir),
    }
}

fn list_files(
    executor: &dyn RemoteExecutor,
    target: &SshTarget,
    script: &str,
) -> Result<Vec<RemoteFile>, String> {
    list_files_with(executor, target, script, valid_rel)
}

fn list_files_with(
    executor: &dyn RemoteExecutor,
    target: &SshTarget,
    script: &str,
    valid: fn(&str) -> bool,
) -> Result<Vec<RemoteFile>, String> {
    let listing = executor
        .run(target, script, LIST_TIMEOUT)
        .map_err(|e| e.to_string())?;
    if listing.timed_out {
        return Err("list_timeout".into());
    }
    match listing.code {
        Some(0) => Ok(parse_listing_with(&listing.stdout, valid)),
        Some(3) => Err("projects_missing".into()),
        _ => Err(format!("list_failed: {}", listing.stderr.trim())),
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
    let remote = list_files(
        executor,
        target,
        &listing_script(target.platform, remote_root),
    )?;
    let mut state = mirror.load_state();
    state.remote_root = Some(remote_root.to_string());
    state.platform = target.platform;
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
    for group in fetch_groups(target.platform, remote_root, &pending) {
        progress(group.start, pending.len());
        if cancel.load(Ordering::Relaxed) {
            report.cancelled = true;
            break;
        }
        let batch = &pending[group];
        let items: Vec<(&str, u64)> = batch
            .iter()
            .map(|(file, file_plan)| (file.rel.as_str(), plan_offset(*file_plan)))
            .collect();
        let fetched = fetch_remote(executor, target, remote_root, &items);
        for ((file, file_plan), fetched) in batch.iter().zip(fetched) {
            match record_fetch(mirror, &mut state.files, file, *file_plan, false, fetched) {
                Ok((fetched_bytes, grew)) => {
                    report.fetched += 1;
                    report.bytes += fetched_bytes;
                    if grew {
                        report.changed.push(file.rel.clone());
                    }
                }
                Err(error) => report.errors.push(format!("{}: {error}", file.rel)),
            }
        }
    }
    progress(pending.len(), pending.len());

    for (rel, file_state) in state.files.iter_mut() {
        let gone = !present.contains(rel.as_str());
        if gone && !file_state.missing {
            file_state.missing = true;
            report.changed.push(rel.clone());
            report.newly_missing += 1;
        }
        if gone {
            report.missing += 1;
        }
    }
    mirror.save_state(&state).map_err(|e| e.to_string())?;
    Ok(report)
}

/// 跟随一个已镜像过的会话文件：只列它自己并按计划拉取。
/// 还没做过全量同步（不知道远端数据根）时不动作。
pub fn follow_once(
    executor: &dyn RemoteExecutor,
    target: &SshTarget,
    mirror: &Mirror,
    rel: &str,
) -> Result<Follow, String> {
    if !valid_rel(rel) {
        return Err("invalid_session_path".into());
    }
    let mut state = mirror.load_state();
    let Some(remote_root) = state.remote_root.clone() else {
        return Ok(Follow::Unchanged);
    };
    let listed = list_files(
        executor,
        target,
        &single_stat_script(target.platform, &remote_root, rel),
    )?;
    let outcome = match listed.into_iter().find(|file| file.rel == rel) {
        Some(file) => {
            let file_plan = plan(&file, state.files.get(rel));
            let updated = file_plan != FetchPlan::Skip
                && fetch_file(
                    executor,
                    target,
                    &remote_root,
                    mirror,
                    &mut state.files,
                    &file,
                    file_plan,
                    false,
                )
                .is_ok_and(|(_, grew)| grew);
            if updated {
                Follow::Updated
            } else {
                Follow::Unchanged
            }
        }
        None => match state.files.get_mut(rel) {
            Some(file_state) if !file_state.missing => {
                file_state.missing = true;
                Follow::Gone
            }
            _ => Follow::Unchanged,
        },
    };
    if outcome != Follow::Unchanged {
        mirror.save_state(&state).map_err(|e| e.to_string())?;
    }
    Ok(outcome)
}

/// 拉取一个会话的子 Agent 文件（打开会话时按需）。返回有变化的文件数；
/// 远端已删除的子 Agent 文件保留镜像。还没做过全量同步时不动作。
pub fn sync_subagents(
    executor: &dyn RemoteExecutor,
    target: &SshTarget,
    mirror: &Mirror,
    project: &str,
    session: &str,
) -> Result<usize, String> {
    if !valid_rel(&format!("{project}/{session}.jsonl")) {
        return Err("invalid_session_path".into());
    }
    let mut state = mirror.load_state();
    let Some(remote_root) = state.remote_root.clone() else {
        return Ok(0);
    };
    let base = format!("{project}/{session}");
    let listed = list_files_with(
        executor,
        target,
        &subagents_script(target.platform, &remote_root, &base),
        valid_subagent_rel,
    )?;
    let files: Vec<RemoteFile> = listed
        .into_iter()
        .map(|file| RemoteFile {
            rel: format!("{base}/subagents/{}", file.rel),
            ..file
        })
        .collect();
    let pending: Vec<(&RemoteFile, FetchPlan)> = files
        .iter()
        .map(|file| {
            let local = state.subagents.get(&file.rel);
            let file_plan = if file.rel.ends_with(".json") {
                plan_whole(file, local)
            } else {
                plan(file, local)
            };
            (file, file_plan)
        })
        .filter(|(_, file_plan)| *file_plan != FetchPlan::Skip)
        .collect();
    let mut changed = 0;
    for group in fetch_groups(target.platform, &remote_root, &pending) {
        let batch = &pending[group];
        let items: Vec<(&str, u64)> = batch
            .iter()
            .map(|(file, file_plan)| (file.rel.as_str(), plan_offset(*file_plan)))
            .collect();
        let fetched = fetch_remote(executor, target, &remote_root, &items);
        for ((file, file_plan), fetched) in batch.iter().zip(fetched) {
            let whole = file.rel.ends_with(".json");
            if record_fetch(
                mirror,
                &mut state.subagents,
                file,
                *file_plan,
                whole,
                fetched,
            )
            .is_ok_and(|(_, grew)| grew)
            {
                changed += 1;
            }
        }
    }
    if changed > 0 {
        mirror.save_state(&state).map_err(|e| e.to_string())?;
    }
    Ok(changed)
}

/// 镜像状态里记下的远端方言（还没全量同步过时按 POSIX）
pub fn known_platform(instance_id: &str) -> RemotePlatform {
    Mirror::new(mirror_root_for(instance_id))
        .load_state()
        .platform
}

/// 实例镜像根：`data_dir()/remote/<实例 id>/`（实例 id 以 `ssh-` 开头，不会与 `ssh/`、`hosts.json` 冲突）
pub fn mirror_root_for(instance_id: &str) -> PathBuf {
    crate::config::data_dir().join("remote").join(instance_id)
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
            platform: RemotePlatform::Posix,
        }
    }

    fn remote_file(rel: &str, size: u64) -> RemoteFile {
        RemoteFile {
            rel: rel.into(),
            size,
            mtime: 1,
        }
    }

    #[test]
    fn windows_fetches_are_grouped_and_posix_fetches_are_not() {
        let files: Vec<RemoteFile> = (0..150)
            .map(|i| {
                remote_file(
                    &format!("C--work-project/{i:08}-0000-0000-0000-000000000000.jsonl"),
                    100,
                )
            })
            .collect();
        let pending: Vec<(&RemoteFile, FetchPlan)> =
            files.iter().map(|f| (f, FetchPlan::Full)).collect();
        assert_eq!(
            fetch_groups(RemotePlatform::Posix, "/r", &pending).len(),
            150
        );
        let groups = fetch_groups(RemotePlatform::Windows, r"C:\r", &pending);
        assert!(groups.len() > 2 && groups.len() < 150);
        assert_eq!(groups.iter().map(|g| g.len()).sum::<usize>(), 150);
        for group in &groups {
            let items: Vec<(&str, u64)> = pending[group.clone()]
                .iter()
                .map(|(f, _)| (f.rel.as_str(), 0))
                .collect();
            assert!(fits_windows_command(&windows::fetch_batch_script(
                r"C:\r", &items
            )));
        }
        // 单个超大文件独占一批，不因字节上限被丢掉
        let big = [
            remote_file("p/a.jsonl", BATCH_MAX_BYTES * 2),
            remote_file("p/b.jsonl", 1),
        ];
        let pending: Vec<(&RemoteFile, FetchPlan)> =
            big.iter().map(|f| (f, FetchPlan::Full)).collect();
        assert_eq!(
            fetch_groups(RemotePlatform::Windows, r"C:\r", &pending),
            vec![0..1, 1..2]
        );
    }

    /// 真实主机验收：`MONET_REMOTE_TEST_TARGET=<ssh 别名> cargo test --lib live_ -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_sync_mirrors_every_listed_session() {
        let Ok(destination) = std::env::var("MONET_REMOTE_TEST_TARGET") else {
            return;
        };
        let probe = SshTarget {
            destination,
            port: None,
            platform: RemotePlatform::Posix,
        };
        let health =
            super::super::health::check(&super::super::executor::SshExecutor, &probe, None);
        let target = SshTarget {
            platform: health.platform,
            ..probe
        };
        let root = health.data_root.expect("data root");
        let dir = std::env::temp_dir().join(format!("monet-live-sync-{}", std::process::id()));
        let mirror = Mirror::new(dir.clone());
        let started = std::time::Instant::now();
        let report = sync_once(
            &super::super::executor::SshExecutor,
            &target,
            &root,
            &mirror,
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        println!(
            "{:?} in {:?}: {report:?}",
            target.platform,
            started.elapsed()
        );
        assert!(report.errors.is_empty(), "{:?}", report.errors);
        assert_eq!(report.fetched, report.listed);
        let state = mirror.load_state();
        assert_eq!(state.platform, health.platform);
        // 逐字节镜像：每一行都是完整 JSON（帧边界错位、CRLF 或转码都会在这里暴露）
        for rel in state.files.keys() {
            let text = std::fs::read_to_string(mirror.file_path(rel)).unwrap();
            for line in text.lines().filter(|l| !l.is_empty()) {
                assert!(
                    serde_json::from_str::<serde_json::Value>(line).is_ok(),
                    "{rel}: {}",
                    &line[..line.len().min(80)]
                );
            }
        }
        // 第二轮应全部跳过
        let again = sync_once(
            &super::super::executor::SshExecutor,
            &target,
            &root,
            &mirror,
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        println!("second pass: {again:?}");
        // 单文件跟随与子 Agent 拉取走各自的脚本
        let mut subagent_files = 0;
        for rel in mirror.load_state().files.keys().take(10) {
            follow_once(&super::super::executor::SshExecutor, &target, &mirror, rel).unwrap();
            let (project, file) = rel.split_once('/').unwrap();
            subagent_files += sync_subagents(
                &super::super::executor::SshExecutor,
                &target,
                &mirror,
                project,
                file.trim_end_matches(".jsonl"),
            )
            .unwrap();
        }
        println!("subagent files mirrored: {subagent_files}");
        let _ = std::fs::remove_dir_all(dir);
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

    fn follow(fixture: &Fixture, rel: &str) -> Follow {
        follow_once(&LocalShellExecutor, &target(), &fixture.mirror, rel).unwrap()
    }

    #[test]
    fn report_lists_changed_files_and_remembers_the_remote_root() {
        let fixture = Fixture::new("changed");
        fixture.write_remote("-home-alice-foo/a.jsonl", "{\"n\":1}\n");
        fixture.write_remote("-home-alice-foo/b.jsonl", "{\"n\":2}\n");
        let mut first = fixture.sync();
        first.changed.sort();
        assert_eq!(
            first.changed,
            vec!["-home-alice-foo/a.jsonl", "-home-alice-foo/b.jsonl"]
        );
        assert_eq!(
            fixture.mirror.load_state().remote_root.as_deref(),
            fixture.remote.to_str()
        );

        fixture.append_remote("-home-alice-foo/a.jsonl", "{\"n\":3}\n");
        std::fs::remove_file(fixture.remote.join("projects/-home-alice-foo/b.jsonl")).unwrap();
        let mut second = fixture.sync();
        assert_eq!(second.newly_missing, 1);
        second.changed.sort();
        assert_eq!(
            second.changed,
            vec!["-home-alice-foo/a.jsonl", "-home-alice-foo/b.jsonl"]
        );
        assert!(fixture.sync().changed.is_empty());
    }

    #[test]
    fn follow_waits_for_a_full_sync_then_tracks_one_file() {
        let fixture = Fixture::new("follow");
        let rel = "-home-alice-foo/a.jsonl";
        fixture.write_remote(rel, "{\"n\":1}\n");
        assert_eq!(
            follow(&fixture, rel),
            Follow::Unchanged,
            "no remote root before the first full sync"
        );

        fixture.sync();
        assert_eq!(follow(&fixture, rel), Follow::Unchanged);
        fixture.append_remote(rel, "{\"n\":2}\n{\"half\"");
        assert_eq!(follow(&fixture, rel), Follow::Updated);
        assert_eq!(fixture.mirrored(rel), "{\"n\":1}\n{\"n\":2}\n");
        assert_eq!(
            follow(&fixture, rel),
            Follow::Unchanged,
            "a still-partial line is not a change"
        );

        std::fs::remove_file(fixture.remote.join("projects").join(rel)).unwrap();
        assert_eq!(follow(&fixture, rel), Follow::Gone);
        assert!(fixture.mirror.load_state().files[rel].missing);
        assert_eq!(follow(&fixture, rel), Follow::Unchanged);
        assert!(follow_once(
            &LocalShellExecutor,
            &target(),
            &fixture.mirror,
            "../x.jsonl"
        )
        .is_err());
    }

    #[test]
    fn subagent_paths_allow_direct_and_workflow_files_only() {
        for rel in [
            "agent-a.jsonl",
            "agent-a.meta.json",
            "workflows/w1/agent-b.jsonl",
            "workflows/w1/journal.jsonl",
        ] {
            assert!(valid_subagent_rel(rel), "{rel:?}");
        }
        for rel in [
            ".jsonl",
            "agent-a.txt",
            "../agent.jsonl",
            "workflows/../agent.jsonl",
            "workflows/.w/agent.jsonl",
            "other/w1/agent.jsonl",
            "workflows/w1/x/agent.jsonl",
        ] {
            assert!(!valid_subagent_rel(rel), "{rel:?}");
        }
    }

    #[test]
    fn subagents_are_fetched_on_demand_and_kept_out_of_the_index() {
        let fixture = Fixture::new("subagents");
        let session = "-home-alice-foo/s1.jsonl";
        let base = "-home-alice-foo/s1/subagents";
        fixture.write_remote(session, "{\"n\":1}\n");
        fixture.write_remote(&format!("{base}/agent-a.jsonl"), "{\"a\":1}\n{\"half\"");
        fixture.write_remote(
            &format!("{base}/agent-a.meta.json"),
            "{\"agentType\":\"x\"}",
        );
        fixture.write_remote(&format!("{base}/workflows/w1/agent-b.jsonl"), "{\"b\":1}\n");
        let fetch = || {
            sync_subagents(
                &LocalShellExecutor,
                &target(),
                &fixture.mirror,
                "-home-alice-foo",
                "s1",
            )
            .unwrap()
        };
        assert_eq!(fetch(), 0, "no remote root before the first full sync");

        let report = fixture.sync();
        assert_eq!(report.listed, 1, "the index lists main session files only");
        assert_eq!(fetch(), 3);
        assert_eq!(
            fixture.mirrored(&format!("{base}/agent-a.jsonl")),
            "{\"a\":1}\n"
        );
        assert_eq!(
            fixture.mirrored(&format!("{base}/agent-a.meta.json")),
            "{\"agentType\":\"x\"}",
            "metadata is written whole, not cut at the last newline"
        );
        assert_eq!(
            fixture.mirrored(&format!("{base}/workflows/w1/agent-b.jsonl")),
            "{\"b\":1}\n"
        );
        assert_eq!(fetch(), 0, "a still-partial line is not a change");

        std::thread::sleep(std::time::Duration::from_millis(1100));
        fixture.write_remote(
            &format!("{base}/agent-a.meta.json"),
            "{\"agentType\":\"y\"}",
        );
        assert_eq!(fetch(), 1, "only the rewritten metadata changed");
        assert_eq!(
            fixture.mirrored(&format!("{base}/agent-a.meta.json")),
            "{\"agentType\":\"y\"}"
        );
        let state = fixture.mirror.load_state();
        assert!(state.files.keys().all(|rel| valid_rel(rel)));
        assert!(!fixture
            .sync()
            .changed
            .iter()
            .any(|rel| rel.contains("subagents")));
        assert!(
            sync_subagents(&LocalShellExecutor, &target(), &fixture.mirror, "..", "s1").is_err()
        );
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
            stat_script("/r", "p/a.jsonl"),
            subagent_list_script("/r", "p/s"),
            fetch_script("/r", "p/a.jsonl", 0),
            fetch_script("/r", "p/a.jsonl", 9),
        ];
        for script in &scripts {
            for forbidden in [" rm ", "mv ", "cp ", "touch", "mkdir", "tee ", "chmod"] {
                assert!(!script.contains(forbidden), "{forbidden} in {script}");
            }
            let redirects = script.matches('>').count()
                - script.matches("2>/dev/null").count()
                - script.matches("1>/dev/null").count();
            assert_eq!(redirects, 0, "{script}");
        }
        assert_eq!(
            fetch_script("/r", "p/a.jsonl", 9),
            "tail -c +10 '/r/projects/p/a.jsonl'"
        );
    }
}
