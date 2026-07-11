//! 会话状态跟踪扩展（选装）：借 Claude Code 官方 hooks 机制获取 turn 级强信号，
//! 补足外部终端会话在监控列的忙/闲/阻塞状态盲区。
//!
//! 边界声明：本模块唯一触碰 CLI 领地的动作是读改写 `~/.claude/settings.json` 的
//! `hooks` 键——这是官方给外部工具的扩展点（写前备份、原子替换、幂等、可完整卸载）。
//! hook 脚本与信号文件都在 `~/.cc-space/` 领地内，JSONL transcript 保持零写入。

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::config;

/// hook 事件 → 信号状态映射（事件名以 SchemaStore claude-code-settings 为准）。
/// 刻意不挂语义宽泛的 Notification，权限等待用专门的 PermissionRequest。
const EVENTS: [(&str, &str); 5] = [
    ("UserPromptSubmit", "started"),
    ("Stop", "completed"),
    ("StopFailure", "failed"),
    ("PermissionRequest", "blocked"),
    ("SessionEnd", "ended"),
];

/// 信号文件启动时轮转阈值与保留尾部大小
const SIGNAL_MAX_BYTES: u64 = 2 * 1024 * 1024;
const SIGNAL_KEEP_BYTES: usize = 512 * 1024;
/// 启动重放时忽略超过此时长的历史信号
const REPLAY_WINDOW_SECS: u64 = 24 * 3600;

fn script_path() -> PathBuf {
    config::data_dir().join("hooks").join("turn-signal.sh")
}

fn signal_path() -> PathBuf {
    config::data_dir().join("turn-signals.jsonl")
}

fn backups_dir() -> PathBuf {
    config::data_dir().join("backups")
}

fn settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".claude")
        .join("settings.json")
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 生成 hook 脚本：POSIX sh 零依赖（date 为 /bin 系统命令），
/// 把事件状态 + stdin 的 hook 载荷包成一行 JSON 追加到信号文件。
/// hook 失败绝不能影响 CLI 本体，故一切错误吞掉并 exit 0。
fn script_body(signal: &PathBuf) -> String {
    format!(
        r#"#!/bin/sh
# CC Space 会话状态跟踪信号钩子 —— 由 CC Space 生成，设置 → 扩展中卸载即自动移除。
STATE="$1"
SIG="{sig}"
PAYLOAD=$(cat | tr -d '\n')
printf '{{"state":"%s","ts":%s,"payload":%s}}\n' "$STATE" "$(date +%s)" "${{PAYLOAD:-null}}" >> "$SIG" 2>/dev/null || true
exit 0
"#,
        sig = signal.display()
    )
}

/// hook 配置里识别「我们的条目」的标记：命令串包含脚本路径
fn is_ours(item: &Value, marker: &str) -> bool {
    item.get("command")
        .and_then(Value::as_str)
        .is_some_and(|c| c.contains(marker))
}

// ---- 状态检查 ----

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TurnSignalStatus {
    /// 是否存在任何安装痕迹（脚本或 settings 条目）
    pub installed: bool,
    /// 安装是否完整可用（脚本在位 + 5 个事件条目齐全 + settings 可解析）
    pub healthy: bool,
    pub issues: Vec<String>,
    pub script_path: String,
    pub settings_path: String,
}

fn compute_status() -> TurnSignalStatus {
    let script = script_path();
    let script_ok = script.is_file();
    let marker = script.to_string_lossy().to_string();

    let mut hooked = 0usize;
    let mut parse_issue: Option<String> = None;
    match fs::read_to_string(settings_path()) {
        Ok(raw) if !raw.trim().is_empty() => match serde_json::from_str::<Value>(&raw) {
            Ok(root) => {
                if let Some(hooks) = root.get("hooks").and_then(Value::as_object) {
                    for (event, _) in EVENTS {
                        let present = hooks
                            .get(event)
                            .and_then(Value::as_array)
                            .is_some_and(|groups| {
                                groups.iter().any(|g| {
                                    g.get("hooks")
                                        .and_then(Value::as_array)
                                        .is_some_and(|items| items.iter().any(|i| is_ours(i, &marker)))
                                })
                            });
                        if present {
                            hooked += 1;
                        }
                    }
                }
            }
            Err(e) => parse_issue = Some(format!("settings.json 解析失败: {e}")),
        },
        _ => {}
    }

    let installed = script_ok || hooked > 0;
    let healthy = script_ok && hooked == EVENTS.len() && parse_issue.is_none();
    let mut issues = Vec::new();
    if installed && !healthy {
        if !script_ok {
            issues.push("hook 脚本缺失".to_string());
        }
        if hooked < EVENTS.len() {
            issues.push(format!("settings.json 钩子条目不完整（{hooked}/{}）", EVENTS.len()));
        }
    }
    if let Some(p) = parse_issue {
        issues.push(p);
    }

    TurnSignalStatus {
        installed,
        healthy,
        issues,
        script_path: script.to_string_lossy().to_string(),
        settings_path: settings_path().to_string_lossy().to_string(),
    }
}

#[tauri::command]
pub fn turn_signal_status() -> TurnSignalStatus {
    compute_status()
}

// ---- 安装 / 卸载 ----

/// 读 settings.json 为 JSON 对象；文件不存在或空视为 {}；解析失败报错（绝不覆盖坏文件）
fn read_settings() -> Result<Value, String> {
    let path = settings_path();
    if !path.exists() {
        return Ok(json!({}));
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("读取 settings.json 失败: {e}"))?;
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|e| format!("settings.json 不是合法 JSON，已中止以免损坏: {e}"))?;
    if parsed.is_object() {
        Ok(parsed)
    } else {
        Err("settings.json 顶层不是对象，已中止".to_string())
    }
}

/// 备份 settings.json 到 ~/.cc-space/backups/（仅当原文件存在）
fn backup_settings() -> Result<(), String> {
    let src = settings_path();
    if !src.exists() {
        return Ok(());
    }
    let dir = backups_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("创建备份目录失败: {e}"))?;
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let dst = dir.join(format!("settings-{ts}.json"));
    fs::copy(&src, &dst).map_err(|e| format!("备份 settings.json 失败: {e}"))?;
    Ok(())
}

/// 原子写回：先写临时文件再 rename，避免半写状态
fn write_settings(root: &Value) -> Result<(), String> {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 ~/.claude 失败: {e}"))?;
    }
    let formatted =
        serde_json::to_string_pretty(root).map_err(|e| format!("序列化 settings.json 失败: {e}"))?;
    let tmp = path.with_extension("json.cc-space-tmp");
    fs::write(&tmp, format!("{formatted}\n")).map_err(|e| format!("写入临时文件失败: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("替换 settings.json 失败: {e}"))?;
    Ok(())
}

/// 幂等挂载单个事件的 hook：先摘掉旧的我们的条目，再追加新条目。
/// 用户已有的其他 hooks 原样保留；仅删除「hooks 数组存在且为空」的无效组。
fn merge_hook(root: &mut Value, event: &str, state: &str, script: &str, marker: &str) {
    if !root.get("hooks").is_some_and(Value::is_object) {
        root["hooks"] = json!({});
    }
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    let entry = hooks.entry(event.to_string()).or_insert_with(|| json!([]));
    if !entry.is_array() {
        *entry = json!([]);
    }
    let Some(groups) = entry.as_array_mut() else { return };

    for group in groups.iter_mut() {
        if let Some(items) = group.get_mut("hooks").and_then(Value::as_array_mut) {
            items.retain(|item| !is_ours(item, marker));
        }
    }
    groups.retain(|group| {
        group
            .get("hooks")
            .and_then(Value::as_array)
            .map(|items| !items.is_empty())
            .unwrap_or(true)
    });

    groups.push(json!({
        "hooks": [{
            "type": "command",
            "command": format!("\"{}\" {}", script, state),
            "timeout": 5
        }]
    }));
}

/// 摘除单个事件里我们的条目；事件数组空了就删事件键
fn remove_hook(root: &mut Value, event: &str, marker: &str) {
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(entry) = hooks.get_mut(event) else { return };
    if let Some(groups) = entry.as_array_mut() {
        for group in groups.iter_mut() {
            if let Some(items) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                items.retain(|item| !is_ours(item, marker));
            }
        }
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .map(|items| !items.is_empty())
                .unwrap_or(true)
        });
        if groups.is_empty() {
            hooks.remove(event);
        }
    }
}

#[tauri::command]
pub fn turn_signal_install(app: AppHandle) -> Result<TurnSignalStatus, String> {
    let script = script_path();
    let signal = signal_path();

    // 1) 领地内准备：脚本 + 信号文件
    if let Some(parent) = script.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建 hooks 目录失败: {e}"))?;
    }
    fs::write(&script, script_body(&signal)).map_err(|e| format!("写入 hook 脚本失败: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("设置脚本可执行失败: {e}"))?;
    }
    fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&signal)
        .map_err(|e| format!("初始化信号文件失败: {e}"))?;

    // 2) settings.json：读 → 备份 → 幂等 merge → 原子写回
    let mut root = read_settings()?;
    backup_settings()?;
    let script_str = script.to_string_lossy().to_string();
    for (event, state) in EVENTS {
        merge_hook(&mut root, event, state, &script_str, &script_str);
    }
    write_settings(&root)?;

    // 3) 起监听（幂等）
    start_listener(app);

    Ok(compute_status())
}

#[tauri::command]
pub fn turn_signal_uninstall() -> Result<TurnSignalStatus, String> {
    // 1) settings.json：读 → 备份 → 摘除 → 原子写回
    let mut root = read_settings()?;
    backup_settings()?;
    let marker = script_path().to_string_lossy().to_string();
    for (event, _) in EVENTS {
        remove_hook(&mut root, event, &marker);
    }
    // hooks 对象被摘空则连键一起还原
    let empty = root
        .get("hooks")
        .and_then(Value::as_object)
        .is_some_and(|h| h.is_empty());
    if empty {
        if let Some(obj) = root.as_object_mut() {
            obj.remove("hooks");
        }
    }
    write_settings(&root)?;

    // 2) 领地内清理：脚本、信号文件；hooks 目录空则一并移除
    let _ = fs::remove_file(script_path());
    let _ = fs::remove_file(signal_path());
    if let Some(dir) = script_path().parent() {
        let _ = fs::remove_dir(dir); // 仅当目录已空才会成功，其余情况静默保留
    }

    Ok(compute_status())
}

// ---- 信号监听 ----

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct TurnSignalEvent {
    session_id: String,
    state: String,
    ts: u64,
    cwd: Option<String>,
}

static LISTENER_STARTED: AtomicBool = AtomicBool::new(false);

/// app 启动时调用：已安装才起监听线程；install 命令也会调用（幂等）
pub fn start_listener_if_installed(app: AppHandle) {
    if compute_status().healthy {
        start_listener(app);
    }
}

fn start_listener(app: AppHandle) {
    if LISTENER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || listener_loop(app));
}

/// 解析信号行：外层 {state, ts, payload}，session_id / cwd 从 hook 载荷提取。
/// 解析失败（并发 append 交错的残行等）直接丢弃该行。
fn parse_line(line: &str) -> Option<TurnSignalEvent> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let state = v.get("state")?.as_str()?.to_string();
    let ts = v.get("ts").and_then(Value::as_u64).unwrap_or_else(unix_now);
    let payload = v.get("payload")?;
    let session_id = payload.get("session_id")?.as_str()?.to_string();
    let cwd = payload
        .get("cwd")
        .and_then(Value::as_str)
        .map(|s| s.to_string());
    Some(TurnSignalEvent { session_id, state, ts, cwd })
}

/// 返回 buf 中以换行结尾的完整前缀长度，残行留给下一轮
fn complete_prefix_len(buf: &str) -> usize {
    buf.rfind('\n').map(|i| i + 1).unwrap_or(0)
}

/// 启动时轮转：超过阈值则只保留尾部（按行边界），避免重放扫描过大文件。
/// 仅在监听启动前做一次，运行中不轮转（避开与 hook 脚本 append 的竞争窗口）。
fn rotate_if_oversized(path: &PathBuf) {
    let Ok(meta) = fs::metadata(path) else { return };
    if meta.len() <= SIGNAL_MAX_BYTES {
        return;
    }
    let Ok(content) = fs::read_to_string(path) else { return };
    let tail_start = content.len().saturating_sub(SIGNAL_KEEP_BYTES);
    let aligned = content[tail_start..]
        .find('\n')
        .map(|i| tail_start + i + 1)
        .unwrap_or(tail_start);
    let _ = fs::write(path, &content[aligned..]);
}

fn emit_event(app: &AppHandle, ev: TurnSignalEvent) {
    let _ = app.emit("turn-signal", ev);
}

fn listener_loop(app: AppHandle) {
    let path = signal_path();
    rotate_if_oversized(&path);

    // 内置 Agent / routine 的工作目录信号与用户无关，单点丢弃——
    // 否则每次后台 Agent 调用都会触发「任务完成」系统通知。
    // hook 上报的 cwd 是 canonicalize 后的路径（如 /tmp → /private/tmp），须对齐后比较
    let agent_dir = fs::canonicalize(crate::config::agent_cwd())
        .unwrap_or_else(|_| crate::config::agent_cwd());
    let is_agent = |ev: &TurnSignalEvent| -> bool {
        ev.cwd
            .as_deref()
            .map(std::path::Path::new)
            .is_some_and(|c| c == agent_dir)
    };

    // 启动重放：全量读一遍，重建每个 session 的最终状态（时效窗口内），再进入增量循环
    let mut offset: u64 = 0;
    if let Ok(content) = fs::read_to_string(&path) {
        offset = content.len() as u64;
        let mut last: HashMap<String, TurnSignalEvent> = HashMap::new();
        for line in content.lines() {
            if let Some(ev) = parse_line(line) {
                if is_agent(&ev) {
                    continue;
                }
                last.insert(ev.session_id.clone(), ev);
            }
        }
        let now = unix_now();
        for (_, ev) in last {
            if now.saturating_sub(ev.ts) < REPLAY_WINDOW_SECS {
                emit_event(&app, ev);
            }
        }
    }

    // 增量循环：1s 轮询 size 变化才读。turn 边界是分钟级事件，秒级延迟无感且实现最简。
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let Ok(meta) = fs::metadata(&path) else { continue };
        let size = meta.len();
        if size < offset {
            // 文件被清空/重建（卸载重装）
            offset = 0;
        }
        if size == offset {
            continue;
        }
        let Ok(mut f) = fs::File::open(&path) else { continue };
        if f.seek(SeekFrom::Start(offset)).is_err() {
            continue;
        }
        let mut buf = String::new();
        if f.read_to_string(&mut buf).is_err() {
            continue;
        }
        let consumed = complete_prefix_len(&buf);
        if consumed == 0 {
            continue;
        }
        offset += consumed as u64;
        for line in buf[..consumed].lines() {
            if let Some(ev) = parse_line(line) {
                if is_agent(&ev) {
                    continue;
                }
                emit_event(&app, ev);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_then_remove_roundtrip_keeps_user_hooks() {
        let mut root = json!({
            "model": "opus",
            "hooks": {
                "Stop": [{ "hooks": [{ "type": "command", "command": "echo user-own" }] }]
            }
        });
        let marker = "/tmp/cc-space/hooks/turn-signal.sh";
        for (event, state) in EVENTS {
            merge_hook(&mut root, event, state, marker, marker);
        }
        // 用户自己的 Stop hook 仍在
        let stop_groups = root["hooks"]["Stop"].as_array().unwrap();
        assert!(stop_groups.iter().any(|g| g["hooks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["command"].as_str().unwrap().contains("user-own"))));

        // 重复安装不产生重复条目
        for (event, state) in EVENTS {
            merge_hook(&mut root, event, state, marker, marker);
        }
        let ours: usize = stop_groups_count(&root, marker);
        assert_eq!(ours, 1);

        // 卸载后我们的条目全部消失，用户条目保留
        for (event, _) in EVENTS {
            remove_hook(&mut root, event, marker);
        }
        assert_eq!(stop_groups_count(&root, marker), 0);
        assert!(root["hooks"]["Stop"].as_array().is_some());
        assert!(root["hooks"].get("UserPromptSubmit").is_none());
        assert_eq!(root["model"], "opus");
    }

    fn stop_groups_count(root: &Value, marker: &str) -> usize {
        root["hooks"]["Stop"]
            .as_array()
            .map(|groups| {
                groups
                    .iter()
                    .flat_map(|g| g["hooks"].as_array().cloned().unwrap_or_default())
                    .filter(|i| is_ours(i, marker))
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn parse_line_extracts_session_and_tolerates_garbage() {
        let ok = r#"{"state":"started","ts":100,"payload":{"session_id":"abc","cwd":"/w"}}"#;
        let ev = parse_line(ok).unwrap();
        assert_eq!(ev.session_id, "abc");
        assert_eq!(ev.state, "started");
        assert_eq!(ev.cwd.as_deref(), Some("/w"));

        assert!(parse_line("not json").is_none());
        assert!(parse_line(r#"{"state":"x","payload":null}"#).is_none());
        assert!(parse_line(r#"{"state":"x","ts":1,"payload":{}}"#).is_none());
    }

    #[test]
    fn complete_prefix_handles_partial_tail() {
        assert_eq!(complete_prefix_len("a\nb\nhalf"), 4);
        assert_eq!(complete_prefix_len("nohalf"), 0);
        assert_eq!(complete_prefix_len("x\n"), 2);
    }
}
