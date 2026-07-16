use tauri::{
    menu::{Menu, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Emitter, Manager, Wry,
};

use crate::{agent, channels, streaming};

pub fn create(app: &AppHandle) -> Result<Menu<Wry>, tauri::Error> {
    let app_menu = SubmenuBuilder::new(app, "Monet")
        .about(None)
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .item(&MenuItemBuilder::new("退出 Monet").id("quit").accelerator("CmdOrCtrl+Q").build(app)?)
        .build()?;

    let close_tab = MenuItemBuilder::new("关闭标签页")
        .id("close-tab")
        .accelerator("CmdOrCtrl+W")
        .build(app)?;
    let file_menu = SubmenuBuilder::new(app, "File")
        .item(&close_tab)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let window_menu = SubmenuBuilder::new(app, "Window")
        .minimize()
        .maximize()
        .separator()
        .fullscreen()
        .build()?;

    Menu::with_items(app, &[&app_menu, &file_menu, &edit_menu, &window_menu])
}

pub fn handle_event(app: &AppHandle, event: &tauri::menu::MenuEvent) {
    match event.id().as_ref() {
        "close-tab" => {
            let _ = app.emit("menu:close-tab", ());
        }
        "quit" => {
            // 交给前端判断：有活跃流式会话 → 确认弹窗；无 → 直接 quit_app
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            let _ = app.emit("menu:request-quit", ());
        }
        _ => {}
    }
}

/// Cmd+W 最后一个 tab / 非工作台域：收起窗口（不退出）
#[tauri::command]
pub fn hide_main_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    // 先藏窗口再清理:close_all_sessions 同步等待会话进程退出(常态 100-300ms,上限 400ms),
    // 窗口留着会呈现「按了 Q 没反应」的假死
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    streaming::close_all_sessions();
    agent::shutdown();
    // exit(0) 不保证触发窗口 Destroyed 清理，fm serve 需显式关停防孤儿
    channels::shutdown_fm_serve();
    app.exit(0);
}
