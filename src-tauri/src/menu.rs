use tauri::{
    menu::{Menu, MenuItemBuilder, SubmenuBuilder},
    AppHandle, Emitter, Wry,
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
            streaming::close_all_sessions();
            agent::shutdown();
            app.exit(0);
        }
        _ => {}
    }
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    streaming::close_all_sessions();
    agent::shutdown();
    app.exit(0);
}
