use tauri::{
    menu::{Menu, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Emitter, Manager, Wry,
};

use crate::{agent, streaming};

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
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
            let _ = app.emit("menu:request-quit", ());
        }
        _ => {}
    }
}

#[tauri::command]
pub fn hide_main_window(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

#[tauri::command]
pub fn hide_to_tray(app: AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    #[cfg(target_os = "macos")]
    set_activation_policy_accessory();
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    streaming::close_all_sessions();
    agent::shutdown();
    app.exit(0);
}

pub fn restore_from_tray(app: &AppHandle) {
    #[cfg(target_os = "macos")]
    set_activation_policy_regular();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg(target_os = "macos")]
fn set_activation_policy_accessory() {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::MainThreadMarker;
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    }
}

#[cfg(target_os = "macos")]
fn set_activation_policy_regular() {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::MainThreadMarker;
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    }
}
