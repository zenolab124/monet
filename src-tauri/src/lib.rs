use tauri::Manager;

mod cache;
mod channels;
pub mod config;
mod commands;
mod discovery;
mod models;
mod parser;
mod permission;
/// pub：schema-probe bin 复用扫描与 diff 核心
pub mod probe;
mod streaming;
mod menu;
mod tray;
pub mod usage_stats;
mod watcher;
mod agent;
mod automation;
mod metadata;
mod routines;
mod scheduler;
mod workshop;
mod cli_settings;
mod translate;
mod widget;


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin({
            let mut ws = tauri_plugin_window_state::Builder::new();
            if cfg!(debug_assertions) {
                ws = ws.skip_initial_state("main");
            }
            ws.build()
        })
        .menu(|app| menu::create(app))
        .on_menu_event(|app, event| menu::handle_event(app, &event))
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
            // AgentService 常驻进程
            agent::init();
            // Apple FM 自动检测
            channels::register_apple_fm_if_available();
            // 系统级定时任务调度器同步
            routines::startup_sync();
            // 后台刷新 CLI settings schema
            cli_settings::refresh_settings_schema();
            // Widget LaunchAgent 自动安装
            widget::ensure_launch_agent();

            // 窗口事件拦截：红色关闭按钮→隐藏到托盘；Destroyed→清理
            let handle = app.handle().clone();
            if let Some(window) = handle.get_webview_window("main") {
                let w = window.clone();
                window.on_window_event(move |event| {
                    match event {
                        tauri::WindowEvent::CloseRequested { api, .. } => {
                            api.prevent_close();
                            let _ = w.hide();
                        }
                        tauri::WindowEvent::Destroyed => {
                            streaming::close_all_sessions();
                            agent::shutdown();
                            channels::shutdown_fm_serve();
                        }
                        _ => {}
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_projects,
            commands::get_session_records,
            commands::get_session_summary,
            commands::delete_session,
            commands::resume_in_terminal,
            commands::resume_in_vscode,
            commands::open_in_finder,
            commands::start_streaming,
            commands::stop_streaming,
            commands::close_session,
            commands::toggle_remote_control,
            commands::respond_permission,
            commands::set_permission_mode,
            commands::get_cli_settings,
            commands::check_session_running,
            commands::kill_external_session,
            commands::list_subagents,
            commands::get_subagent_records,
            commands::fork_session,
            commands::get_usage_stats,
            commands::get_schema_diagnosis,
            workshop::get_workshop_assets,
            workshop::probe_mcp_server,
            workshop::open_workshop_dir,
            automation::get_hooks_config,
            automation::get_hooks_stats,
            automation::open_hooks_config,
            channels::list_channels,
            channels::save_channel,
            channels::delete_channel,
            channels::set_channel_enabled,
            channels::set_default_session_channel,
            channels::set_default_agent_model,
            channels::get_channel_token,
            channels::get_agent_toggles,
            channels::set_agent_toggle,
            channels::get_agent_preferences,
            channels::set_agent_feature_model,
            channels::reveal_channels_dir,
            channels::probe_channel,
            channels::scan_cc_switch,
            channels::import_cc_switch,
            commands::open_in_default_app,
            commands::read_local_image,
            metadata::get_all_meta,
            metadata::update_meta,
            metadata::generate_title,
            metadata::generate_tags,
            metadata::generate_summary,
            metadata::generate_permission_hint,
            metadata::set_agent_locale,
            metadata::parse_natural_schedule,
            metadata::translate_settings_fields,
            metadata::extract_settings_defaults,
            agent::test_agent_channel,
            agent::get_agent_logs,
            agent::clear_agent_logs,
            routines::get_routines,
            routines::create_routine,
            routines::update_routine,
            routines::delete_routine,
            routines::get_routine_logs,
            routines::run_routine_now,
            routines::get_routine_wake_policy,
            routines::set_routine_wake_policy,
            cli_settings::get_settings_schema,
            cli_settings::get_full_cli_settings,
            cli_settings::update_cli_settings,
            cli_settings::refresh_settings_schema,
            cli_settings::get_mcp_status,
            cli_settings::register_mcp,
            cli_settings::unregister_mcp,
            translate::translate_locale,
            translate::parse_language_intent,
            translate::list_external_locales,
            translate::delete_external_locale,
            widget::update_widget,
            widget::get_widget_config,
            widget::set_widget_config,
            menu::hide_main_window,
            menu::quit_app,
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
