use std::sync::mpsc;
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter};

/// 启动文件监控，检测 ~/.claude/projects/ 下的变化
/// 防抖 1 秒后向前端发送 "projects-changed" 事件
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

        let mut last_emit = Instant::now() - Duration::from_secs(10);

        loop {
            // 阻塞等待事件，超时 500ms
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(_event) => {
                    // 收到事件，防抖：如果距离上次发射不足 1 秒，等待
                    let now = Instant::now();
                    if now.duration_since(last_emit) >= Duration::from_secs(1) {
                        // 排空积压的事件
                        while rx.try_recv().is_ok() {}
                        last_emit = Instant::now();
                        let _ = handle.emit("projects-changed", ());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}
