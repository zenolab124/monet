use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, Watcher};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

/// 启动文件监控，检测 ~/.claude/projects/ 下的变化
/// - 防抖 1 秒后向前端发送 "projects-changed" 事件
/// - 增量探测会话 jsonl 新增的 api_error 记录，发送 "session-api-error" 事件
///   （FR-010 外部会话出错兜底；是否属于工作台由前端判定过滤）
pub fn start(app: &AppHandle) {
    let root = match dirs::home_dir() {
        Some(h) => h.join(".claude").join("projects"),
        None => return,
    };
    if !root.is_dir() {
        return;
    }

    let handle = app.clone();
    std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<Event>();

        let mut watcher = match notify::recommended_watcher(move |res: Result<Event, _>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                log::error!("Failed to create file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
            log::error!("Failed to watch {:?}: {}", root, e);
            return;
        }

        log::info!("File watcher started on {:?}", root);

        // 启动时预记录全部会话文件 size：只对"启动之后新增"的内容做 api_error 探测，
        // 避免把历史错误当新事件误报
        let mut file_sizes = snapshot_sizes(&root);

        let mut last_emit = Instant::now() - Duration::from_secs(10);

        loop {
            // 阻塞等待事件，超时 500ms
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(event) => {
                    // api_error 增量探测：每个事件都处理（需要 paths，不能合并丢弃）
                    for path in &event.paths {
                        if let Some((sid, pid)) = session_file_ids(&root, path) {
                            probe_api_errors(&handle, path, &sid, &pid, &mut file_sizes);
                        }
                    }

                    // projects-changed 防抖：距离上次发射不足 1 秒则跳过
                    let now = Instant::now();
                    if now.duration_since(last_emit) >= Duration::from_secs(1) {
                        last_emit = now;
                        let _ = handle.emit("projects-changed", ());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

/// 启动时记录所有项目目录直接子级 .jsonl 的当前大小
fn snapshot_sizes(root: &Path) -> HashMap<PathBuf, u64> {
    let mut sizes = HashMap::new();
    let Ok(projects) = std::fs::read_dir(root) else {
        return sizes;
    };
    for project in projects.filter_map(|e| e.ok()) {
        let dir = project.path();
        if !dir.is_dir() {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        for file in files.filter_map(|e| e.ok()) {
            let path = file.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                if let Ok(meta) = file.metadata() {
                    sizes.insert(path, meta.len());
                }
            }
        }
    }
    sizes
}

/// 判定路径是否为「项目目录直接子级的会话 jsonl」（排除 subagents 等深层文件），
/// 返回 (session_id, project_id)
fn session_file_ids(root: &Path, path: &Path) -> Option<(String, String)> {
    if path.extension()? != "jsonl" {
        return None;
    }
    let project_dir = path.parent()?;
    // 直接子级：父目录的父目录必须是 projects root
    if project_dir.parent()? != root {
        return None;
    }
    let sid = path.file_stem()?.to_str()?.to_string();
    let pid = project_dir.file_name()?.to_str()?.to_string();
    Some((sid, pid))
}

/// 读取文件自上次记录以来的增量内容，扫描新增的 api_error 记录并 emit
fn probe_api_errors(
    app: &AppHandle,
    path: &Path,
    session_id: &str,
    project_id: &str,
    file_sizes: &mut HashMap<PathBuf, u64>,
) {
    let Ok(meta) = std::fs::metadata(path) else {
        // 文件被删：清掉记录
        file_sizes.remove(path);
        return;
    };
    let new_size = meta.len();
    let old_size = match file_sizes.get(path) {
        Some(&s) => s,
        None => {
            // 启动后新出现的文件：从头算（新会话的首条 api_error 也要捕获）
            0
        }
    };
    file_sizes.insert(path.to_path_buf(), new_size);

    if new_size <= old_size {
        // 截断或无增长（重写场景下放弃本轮，下轮以新 size 为基线）
        return;
    }

    let Ok(mut file) = std::fs::File::open(path) else {
        return;
    };
    if file.seek(SeekFrom::Start(old_size)).is_err() {
        return;
    }
    // 单轮增量上限 4MB：防御性，正常追加远小于此
    let len = (new_size - old_size).min(4 * 1024 * 1024) as usize;
    let mut buf = vec![0u8; len];
    if file.read_exact(&mut buf).is_err() {
        return;
    }
    let text = String::from_utf8_lossy(&buf);

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || !line.contains("api_error") {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let is_api_error = value.get("type").and_then(Value::as_str) == Some("system")
            && value.get("subtype").and_then(Value::as_str) == Some("api_error");
        if !is_api_error {
            continue;
        }
        let content = value
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or("API 错误")
            .to_string();
        let retry_attempt = value.get("retryAttempt").and_then(Value::as_u64);
        let _ = app.emit(
            "session-api-error",
            json!({
                "sessionId": session_id,
                "projectId": project_id,
                "content": content,
                "retryAttempt": retry_attempt,
            }),
        );
    }
}
