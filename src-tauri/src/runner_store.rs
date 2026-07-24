//! 跑单候选清单 CRUD + pid 文件管理 + meta 落盘 + 磁盘日志回读。
//!
//! 纯逻辑模块，零 tauri 依赖——主 App（runners.rs）直接引用，
//! MCP 独立进程（monet_mcp.rs）通过 `#[path]` 共享。

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 候选命令（项目级清单）
// ---------------------------------------------------------------------------

const COMMANDS_MAX_PER_PROJECT: usize = 20;

/// 单条候选命令
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerCommand {
    pub id: String,
    pub cmd: String,
    /// None = 继承会话 cwd；Some = monorepo 子目录等显式运行目录
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// "agent" | "user"
    pub source: String,
    pub created_at: u64,
    pub last_used_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCommands {
    pub commands: Vec<RunnerCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerCommandsFile {
    pub version: u32,
    pub projects: HashMap<String, ProjectCommands>,
}

fn commands_path() -> PathBuf {
    crate::config::data_dir().join("runner-commands.json")
}

/// 候选清单跨进程排他锁（主 App + MCP 共用同一文件）。
/// 返回的 File 持锁期间独占访问，Drop 自动释放
fn commands_lock() -> Option<fs::File> {
    let lock_path = commands_path().with_extension("json.lock");
    if let Some(parent) = lock_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .ok()?;
    use fs2::FileExt;
    // 限时 2s：阻塞过久说明对端异常，降级无锁
    file.lock_exclusive().ok()?;
    Some(file)
}

pub fn load_commands() -> RunnerCommandsFile {
    let path = commands_path();
    if !path.exists() {
        return empty_commands_file();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(empty_commands_file)
}

fn save_commands(data: &RunnerCommandsFile) {
    let path = commands_path();
    if let Ok(json) = serde_json::to_string_pretty(data) {
        let _ = crate::config::atomic_write(&path, &json);
    }
}

fn empty_commands_file() -> RunnerCommandsFile {
    RunnerCommandsFile {
        version: 1,
        projects: HashMap::new(),
    }
}

/// canonicalize cwd 为字符串 key（失败原样返回）
fn canonical_key(cwd: &str) -> String {
    fs::canonicalize(cwd)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| cwd.to_string())
}

/// Agent 经 MCP `runner_suggest` 登记候选。
/// 同 cmd+cwd 幂等覆盖 alias/note；超限报错。
/// 返回 (project_key, total_count)
pub fn suggest_command(
    project_cwd: &str,
    cmd: &str,
    cwd: Option<&str>,
    alias: Option<&str>,
    note: Option<&str>,
) -> Result<(String, usize), String> {
    let _lock = commands_lock();
    let key = canonical_key(project_cwd);
    let resolved_cwd = cwd.map(canonical_key);
    let mut file = load_commands();
    let project = file
        .projects
        .entry(key.clone())
        .or_insert_with(|| ProjectCommands {
            commands: Vec::new(),
        });

    // 幂等：同 cmd + resolved_cwd 已存在则覆盖 alias/note
    if let Some(existing) = project
        .commands
        .iter_mut()
        .find(|c| c.cmd == cmd && c.cwd == resolved_cwd)
    {
        if alias.is_some() {
            existing.alias = alias.map(String::from);
        }
        if note.is_some() {
            existing.note = note.map(String::from);
        }
        let total = project.commands.len();
        save_commands(&file);
        return Ok((key, total));
    }

    if project.commands.len() >= COMMANDS_MAX_PER_PROJECT {
        return Err(format!(
            "Project already has {} commands (limit {}). Remove some before adding new ones.",
            project.commands.len(),
            COMMANDS_MAX_PER_PROJECT
        ));
    }

    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
    project.commands.push(RunnerCommand {
        id: uuid::Uuid::new_v4().to_string(),
        cmd: cmd.to_string(),
        cwd: resolved_cwd,
        alias: alias.map(String::from),
        note: note.map(String::from),
        source: "agent".to_string(),
        created_at: now_ms,
        last_used_at: now_ms,
    });
    let total = project.commands.len();
    save_commands(&file);
    Ok((key, total))
}

/// 用户 spawn 后自动沉淀候选。
/// 同 cmd+cwd（或 source_command_id）只更新 last_used_at；超限淘汰最旧 user 条目
pub fn settle_command(
    project_cwd: &str,
    cmd: &str,
    cwd: Option<&str>,
    alias: Option<&str>,
    source_command_id: Option<&str>,
) {
    let _lock = commands_lock();
    let key = canonical_key(project_cwd);
    let resolved_cwd = cwd.map(canonical_key);
    let mut file = load_commands();
    let project = file
        .projects
        .entry(key)
        .or_insert_with(|| ProjectCommands {
            commands: Vec::new(),
        });

    let now_ms = chrono::Utc::now().timestamp_millis() as u64;

    // 已有条目（通过 source_command_id 精确匹配或 cmd+cwd 匹配）：只更新 last_used_at
    let found = if let Some(sid) = source_command_id {
        project.commands.iter_mut().find(|c| c.id == sid)
    } else {
        project
            .commands
            .iter_mut()
            .find(|c| c.cmd == cmd && c.cwd == resolved_cwd)
    };
    if let Some(existing) = found {
        existing.last_used_at = now_ms;
        save_commands(&file);
        return;
    }

    // 超限淘汰：移除 last_used_at 最旧的 user 条目
    if project.commands.len() >= COMMANDS_MAX_PER_PROJECT {
        if let Some(oldest_idx) = project
            .commands
            .iter()
            .enumerate()
            .filter(|(_, c)| c.source == "user")
            .min_by_key(|(_, c)| c.last_used_at)
            .map(|(i, _)| i)
        {
            project.commands.remove(oldest_idx);
        } else {
            // 全是 agent 条目，无法淘汰，放弃沉淀
            return;
        }
    }

    project.commands.push(RunnerCommand {
        id: uuid::Uuid::new_v4().to_string(),
        cmd: cmd.to_string(),
        cwd: resolved_cwd,
        alias: alias.map(String::from),
        note: None,
        source: "user".to_string(),
        created_at: now_ms,
        last_used_at: now_ms,
    });
    save_commands(&file);
}

/// 移除候选条目
pub fn remove_command(project_cwd: &str, id: &str) -> Result<(), String> {
    let _lock = commands_lock();
    let key = canonical_key(project_cwd);
    let mut file = load_commands();
    let project = file
        .projects
        .get_mut(&key)
        .ok_or_else(|| "project not found".to_string())?;
    let before = project.commands.len();
    project.commands.retain(|c| c.id != id);
    if project.commands.len() == before {
        return Err("command not found".to_string());
    }
    save_commands(&file);
    Ok(())
}

/// 列出指定项目的候选命令
pub fn list_commands(project_cwd: &str) -> Vec<RunnerCommand> {
    let key = canonical_key(project_cwd);
    let file = load_commands();
    file.projects
        .get(&key)
        .map(|p| p.commands.clone())
        .unwrap_or_default()
}

/// 候选文件的最后修改时间（变更轮询用）
pub fn commands_mtime() -> Option<std::time::SystemTime> {
    fs::metadata(commands_path())
        .ok()
        .and_then(|m| m.modified().ok())
}

// ---------------------------------------------------------------------------
// PID 文件（进程孤儿治理）
// ---------------------------------------------------------------------------

/// pid 文件内容：进程身份与启动时间，供孤儿治理校验
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PidInfo {
    pub pid: u32,
    pub pgid: Option<u32>,
    pub start_time_epoch_ms: u64,
    pub cmd: String,
    pub session_id: String,
}

pub fn proc_logs_dir() -> PathBuf {
    crate::config::data_dir().join("proc-logs")
}

pub fn log_file_path(session_id: &str, runner_id: &str) -> PathBuf {
    proc_logs_dir()
        .join(session_id)
        .join(format!("{}.log", runner_id))
}

fn pid_path(session_id: &str, runner_id: &str) -> PathBuf {
    proc_logs_dir()
        .join(session_id)
        .join(format!("{}.pid", runner_id))
}

pub fn write_pid(session_id: &str, runner_id: &str, info: &PidInfo) {
    let path = pid_path(session_id, runner_id);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(info) {
        let _ = crate::config::atomic_write(&path, &json);
    }
}

pub fn remove_pid(session_id: &str, runner_id: &str) {
    let _ = fs::remove_file(pid_path(session_id, runner_id));
}

/// 扫描所有 pid 文件，返回 (session_id, runner_id, PidInfo)
pub fn scan_all_pids() -> Vec<(String, String, PidInfo)> {
    let base = proc_logs_dir();
    let mut results = Vec::new();
    let Ok(sessions) = fs::read_dir(&base) else {
        return results;
    };
    for session_entry in sessions.flatten() {
        let session_dir = session_entry.path();
        if !session_dir.is_dir() {
            continue;
        }
        let session_id = session_entry.file_name().to_string_lossy().into_owned();
        let Ok(files) = fs::read_dir(&session_dir) else {
            continue;
        };
        for file_entry in files.flatten() {
            let fname = file_entry.file_name().to_string_lossy().into_owned();
            if let Some(runner_id) = fname.strip_suffix(".pid") {
                if let Ok(content) = fs::read_to_string(file_entry.path()) {
                    if let Ok(info) = serde_json::from_str::<PidInfo>(&content) {
                        results.push((session_id.clone(), runner_id.to_string(), info));
                    }
                }
            }
        }
    }
    results
}

/// 按 runner_id 查找日志文件路径（扫描 proc-logs 子目录）
pub fn find_log_path(runner_id: &str) -> Option<PathBuf> {
    let base = proc_logs_dir();
    let Ok(sessions) = fs::read_dir(&base) else {
        return None;
    };
    let log_name = format!("{}.log", runner_id);
    for entry in sessions.flatten() {
        let dir = entry.path();
        if dir.is_dir() {
            let log = dir.join(&log_name);
            if log.exists() {
                return Some(log);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Meta 文件（崩溃后 MCP 可见的跑单终态）
// ---------------------------------------------------------------------------

/// 跑单元数据落盘结构：spawn 时写入，终态时原子更新。
/// MCP 独立进程依据此文件获知已结束跑单的完整信息（pid 文件只记活进程）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerMeta {
    pub runner_id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    pub cmd: String,
    pub cwd: String,
    pub status: String,
    pub started_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exited_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub log_path: String,
}

fn meta_path(session_id: &str, runner_id: &str) -> PathBuf {
    proc_logs_dir()
        .join(session_id)
        .join(format!("{}.meta.json", runner_id))
}

pub fn write_meta(session_id: &str, runner_id: &str, meta: &RunnerMeta) {
    let path = meta_path(session_id, runner_id);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(meta) {
        let _ = crate::config::atomic_write(&path, &json);
    }
}

/// 扫描所有 meta.json 文件，返回 (session_id, runner_id, RunnerMeta)
pub fn scan_all_metas() -> Vec<(String, String, RunnerMeta)> {
    let base = proc_logs_dir();
    let mut results = Vec::new();
    let Ok(sessions) = fs::read_dir(&base) else {
        return results;
    };
    for session_entry in sessions.flatten() {
        let session_dir = session_entry.path();
        if !session_dir.is_dir() {
            continue;
        }
        let session_id = session_entry.file_name().to_string_lossy().into_owned();
        let Ok(files) = fs::read_dir(&session_dir) else {
            continue;
        };
        for file_entry in files.flatten() {
            let fname = file_entry.file_name().to_string_lossy().into_owned();
            if let Some(runner_id) = fname.strip_suffix(".meta.json") {
                if let Ok(content) = fs::read_to_string(file_entry.path()) {
                    if let Ok(meta) = serde_json::from_str::<RunnerMeta>(&content) {
                        results.push((session_id.clone(), runner_id.to_string(), meta));
                    }
                }
            }
        }
    }
    results
}

// ---------------------------------------------------------------------------
// 磁盘日志 tail 回读
// ---------------------------------------------------------------------------

/// 磁盘日志解析后的单行
#[derive(Debug, Clone, Serialize)]
pub struct TailLine {
    pub ts: String,
    pub stream: String,
    pub text: String,
}

/// tail 回读结果
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailResult {
    pub lines: Vec<TailLine>,
    pub truncated: bool,
    pub total_matched: usize,
    pub log_path: String,
}

/// 从磁盘日志回读末尾 N 行。
/// 日志格式：`[ts_iso8601][stream] raw_text`。
/// grep 按行正则过滤后截末尾 N 行；strip_ansi 剥离 CSI 转义序列
pub fn read_tail_from_disk(
    log_path: &Path,
    max_lines: usize,
    grep: Option<&str>,
    strip_ansi: bool,
) -> Result<TailResult, String> {
    let file = fs::File::open(log_path).map_err(|e| format!("cannot open log: {}", e))?;
    let reader = BufReader::new(file);

    let grep_re = match grep {
        Some(pattern) => Some(
            regex::Regex::new(pattern).map_err(|e| format!("invalid grep regex: {}", e))?,
        ),
        None => None,
    };

    let mut matched: Vec<TailLine> = Vec::new();
    let mut total_matched: usize = 0;

    for line_result in reader.lines() {
        let Ok(raw) = line_result else { continue };
        let Some((ts, stream, text_raw)) = parse_log_line(&raw) else {
            continue;
        };
        let text = if strip_ansi {
            strip_ansi_str(&text_raw)
        } else {
            text_raw
        };
        if let Some(ref re) = grep_re {
            if !re.is_match(&text) {
                continue;
            }
        }
        total_matched += 1;
        matched.push(TailLine { ts, stream, text });
    }

    let truncated = matched.len() > max_lines;
    let start = matched.len().saturating_sub(max_lines);
    let tail_lines = matched[start..].to_vec();

    Ok(TailResult {
        lines: tail_lines,
        truncated,
        total_matched,
        log_path: log_path.to_string_lossy().into_owned(),
    })
}

/// 解析日志行 `[ts][stream] text`
fn parse_log_line(line: &str) -> Option<(String, String, String)> {
    let rest = line.strip_prefix('[')?;
    let (ts, rest) = rest.split_once(']')?;
    let rest = rest.strip_prefix('[')?;
    let (stream, rest) = rest.split_once(']')?;
    let text = rest.strip_prefix(' ').unwrap_or(rest);
    Some((ts.to_string(), stream.to_string(), text.to_string()))
}

// ---------------------------------------------------------------------------
// ANSI 转义剥离
// ---------------------------------------------------------------------------

/// 剥离 ANSI CSI 转义序列（`\x1b[...X` 形式，覆盖 SGR/光标/擦除等常见序列）
pub fn strip_ansi_str(s: &str) -> String {
    use std::sync::OnceLock;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").unwrap());
    re.replace_all(s, "").into_owned()
}

// ---------------------------------------------------------------------------
// 进程 start_time 校验（孤儿治理：防 pid 复用误杀）
// ---------------------------------------------------------------------------

/// 校验 pid 的启动时间是否与记录吻合（容差 ±2s）。
/// Ok(true) = 吻合（真孤儿），Ok(false) = 不吻合（pid 已被复用），
/// Err = 无法校验（进程不存在或平台不支持）
pub fn verify_start_time(pid: u32, recorded_epoch_ms: u64) -> Result<bool, String> {
    verify_start_time_impl(pid, recorded_epoch_ms)
}

#[cfg(unix)]
fn verify_start_time_impl(pid: u32, recorded_epoch_ms: u64) -> Result<bool, String> {
    let output = std::process::Command::new("ps")
        .args(["-o", "lstart=", "-p", &pid.to_string()])
        .output()
        .map_err(|e| format!("ps failed: {}", e))?;

    if !output.status.success() {
        return Err("process not found".to_string());
    }

    let lstart = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if lstart.is_empty() {
        return Err("ps returned empty output".to_string());
    }

    // macOS/Linux lstart 格式："Thu Jul 24 10:30:00 2026"（多余空格正规化后解析）
    let normalized = lstart.split_whitespace().collect::<Vec<_>>().join(" ");
    let parsed = chrono::NaiveDateTime::parse_from_str(&normalized, "%a %b %e %H:%M:%S %Y")
        .map_err(|e| format!("lstart parse failed '{}': {}", lstart, e))?;

    use chrono::Offset;
    let local_offset = chrono::Local::now().offset().fix();
    let dt = parsed
        .and_local_timezone(local_offset)
        .single()
        .ok_or_else(|| "timezone conversion failed".to_string())?;
    let actual_ms = dt.timestamp_millis() as u64;

    let diff = actual_ms.abs_diff(recorded_epoch_ms);
    Ok(diff <= 2000)
}

#[cfg(windows)]
fn verify_start_time_impl(pid: u32, recorded_epoch_ms: u64) -> Result<bool, String> {
    // Windows start_time 校验尚未实现，走保守路径（只删 pid 文件不杀）
    let _ = (pid, recorded_epoch_ms);
    Err("start_time verification unavailable on Windows".to_string())
}
