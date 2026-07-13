use tauri::Manager;

mod cache;
mod channels;
/// pub：定位逻辑同时被 monet-routine-runner 以 #[path] 方式复用
pub mod claude_locator;
pub mod config;
mod commands;
mod discovery;
mod image_protocol;
mod models;
mod parser;
mod permission;
/// pub：schema-probe bin 复用扫描与 diff 核心
pub mod probe;
mod streaming;
mod menu;
mod tray;
pub mod usage_stats;
mod perf;
mod watcher;
mod agent;
mod automation;
mod metadata;
mod routines;
/// pub：结构定义同时被 monet-mcp / monet-routine-runner 以 #[path] 方式复用
pub mod routine_types;
/// pub：搜索引擎同时被 monet-mcp 以 #[path] 方式复用（search_sessions 工具）
pub mod search;
mod scheduler;
/// pub：唤醒计划逻辑同时被 monet-routine-runner 以 #[path] 方式复用
pub mod wake;
#[cfg(target_os = "macos")]
mod signing;
mod tcc;
mod workshop;
mod cli_env;
mod cli_settings;
mod quota;
mod translate;
mod turn_signal;
mod widget;


#[cfg(target_os = "macos")]
extern "C" {
    /// src/native/high_refresh.m：swizzle WKWebView 初始化器解锁 ProMotion 高刷。
    /// 必须先于任何 WKWebView 创建执行——feature 仅在创建时刻读取
    fn monet_install_high_refresh_unlock();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "macos")]
    unsafe {
        monet_install_high_refresh_unlock();
    }
    tauri::Builder::default()
        // ccimg 自定义协议：历史区图片按需取（base64 已从 records 剥离）。
        // 异步 responder：读 JSONL/decode 走 tauri 线程池，不阻塞主线程。
        .register_asynchronous_uri_scheme_protocol("ccimg", |_ctx, request, responder| {
            image_protocol::handle_request(request, responder);
        })
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
            // 会话状态跟踪扩展：已安装则恢复信号监听
            turn_signal::start_listener_if_installed(app.handle().clone());
            // 渠道 runtime 残留清理(上次异常退出可能留下含 token 的合成文件)
            channels::cleanup_runtime_dir();
            // AgentService 常驻进程
            agent::init();
            // Apple FM 自动检测
            channels::register_apple_fm_if_available();
            // 系统级定时任务调度器同步（launchctl/pmset 耗时且可能弹授权框，不得阻塞主线程）
            tauri::async_runtime::spawn_blocking(routines::startup_sync);
            // 后台刷新 CLI settings schema
            cli_settings::refresh_settings_schema();
            // MCP 二进制启动自愈（存量 adhoc 安装收敛到稳定签名）
            cli_settings::startup_sync_mcp();
            // Widget LaunchAgent 自动安装
            widget::ensure_launch_agent();
            // 搜索缓存预热：延迟避开启动高峰，后台建/对账文本缓存
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_secs(3));
                search::warm();
            });

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
            commands::get_perf_stats,
            commands::delete_session,
            commands::resume_in_terminal,
            commands::check_system_permissions,
            commands::request_system_permission,
            commands::open_privacy_settings,
            commands::run_runner_health_check,
            commands::get_runner_health_snapshot,
            commands::resume_in_vscode,
            commands::open_in_finder,
            commands::get_agent_session_dir,
            commands::start_streaming,
            commands::stop_streaming,
            commands::close_session,
            commands::toggle_remote_control,
            commands::respond_permission,
            commands::set_permission_mode,
            commands::get_cli_settings,
            commands::check_session_running,
            commands::has_own_process,
            commands::kill_external_session,
            commands::list_subagents,
            commands::get_subagent_records,
            commands::read_task_output,
            commands::fork_session,
            commands::get_usage_stats,
            commands::get_schema_diagnosis,
            commands::search_query,
            commands::search_status,
            commands::smart_search,
            workshop::get_workshop_assets,
            workshop::probe_mcp_server,
            workshop::open_workshop_dir,
            automation::get_hooks_config,
            automation::get_hooks_stats,
            automation::open_hooks_config,
            turn_signal::turn_signal_status,
            turn_signal::turn_signal_install,
            turn_signal::turn_signal_uninstall,
            cli_env::claude_env_check,
            cli_env::claude_env_upgrade,
            cli_env::claude_env_diagnose,
            channels::list_channels,
            channels::save_channel,
            channels::set_official_defaults,
            channels::delete_channel,
            channels::set_channel_enabled,
            channels::set_default_session_channel,
            channels::set_default_agent_model,
            channels::get_channel_token,
            channels::get_agent_toggles,
            channels::set_agent_toggle,
            channels::get_agent_session_persist,
            channels::set_agent_session_persist,
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
            routines::get_wake_authorization_status,
            routines::enable_wake_active,
            routines::remove_wake_authorization,
            cli_settings::get_settings_schema,
            cli_settings::get_full_cli_settings,
            cli_settings::update_cli_settings,
            cli_settings::refresh_settings_schema,
            cli_settings::get_mcp_status,
            cli_settings::register_mcp,
            cli_settings::unregister_mcp,
            cli_settings::get_claude_binary_info,
            cli_settings::set_claude_binary_path,
            cli_settings::redetect_claude_binary,
            translate::translate_locale,
            translate::parse_language_intent,
            translate::list_external_locales,
            translate::delete_external_locale,
            widget::update_widget,
            widget::get_widget_config,
            widget::set_widget_config,
            quota::get_quota,
            quota::refresh_quota,
            quota::quota_available,
            quota::get_tray_title_config,
            quota::set_tray_title_config,
            menu::hide_main_window,
            menu::hide_to_tray,
            menu::quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            match &event {
                // code=None 是最后一个窗口关闭触发的自动退出请求，拦下保持托盘常驻；
                // code=Some 来自显式 app.exit()（Cmd+Q 确认退出），必须放行，
                // 否则 quit_app 已 SIGTERM 子进程而应用不退，探测会把自家濒死进程误报为外部运行
                tauri::RunEvent::ExitRequested { api, code, .. } => {
                    if code.is_none() {
                        api.prevent_exit();
                    }
                }
                tauri::RunEvent::Reopen { .. } => {
                    if let Some(window) = app_handle.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                _ => {}
            }
        });
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
