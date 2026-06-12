use tauri::Manager;

mod cache;
mod channels;
mod commands;
mod discovery;
mod models;
mod parser;
mod permission;
/// pub：schema-probe bin 复用扫描与 diff 核心
pub mod probe;
mod streaming;
mod tray;
pub mod usage_stats;
mod watcher;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;

                // 开发模式：窗口移到非主显示器（扩展屏）
                move_to_secondary_monitor(app);
            }
            // 系统托盘
            if let Err(e) = tray::setup(app.handle()) {
                log::error!("Tray setup failed: {}", e);
            }
            // 启动文件监控
            watcher::start(app.handle());
            // 渠道 runtime 残留清理(上次异常退出可能留下含 token 的合成文件)
            channels::cleanup_runtime_dir();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_projects,
            commands::get_session_records,
            commands::get_session_summary,
            commands::delete_session,
            commands::resume_in_terminal,
            commands::resume_in_vscode,
            commands::start_streaming,
            commands::stop_streaming,
            commands::respond_permission,
            commands::get_cli_settings,
            commands::check_session_running,
            commands::get_usage_stats,
            commands::get_schema_diagnosis,
            channels::list_channels,
            channels::save_channel,
            channels::delete_channel,
            channels::set_default_channel,
            channels::reveal_channels_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// 开发时把窗口移到扩展显示器居中
fn move_to_secondary_monitor(app: &tauri::App) {

    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let monitors: Vec<_> = window.available_monitors().unwrap_or_default();
    // 找非主显示器
    let secondary = monitors.iter().find(|m| {
        let pos = m.position();
        // 主显示器通常在 (0,0)
        pos.x != 0 || pos.y != 0
    });

    if let Some(monitor) = secondary {
        let pos = monitor.position();
        let size = monitor.size();
        let scale = monitor.scale_factor();

        // 窗口居中于该显示器
        let win_w = 1200.0;
        let win_h = 800.0;
        let x = pos.x as f64 + (size.width as f64 - win_w * scale) / 2.0;
        let y = pos.y as f64 + (size.height as f64 - win_h * scale) / 2.0;

        let _ = window.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
    }
}
