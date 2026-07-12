use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem},
    tray::{TrayIconBuilder, TrayIconId},
    AppHandle, Manager,
};

use crate::{agent, quota, streaming};

#[cfg(target_os = "macos")]
const TAB_STOP_PT: f64 = 220.0;

static QUOTA_TIMER_RUNNING: AtomicBool = AtomicBool::new(false);
const TRAY_ID: &str = "main-tray";

pub fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let menu = build_menu(app, None)?;

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .icon(
            // 专用 template glyph(日出剪影):主图标是满幅 squircle,template 化会变实心方块
            Image::from_bytes(include_bytes!("../icons/tray-template.png"))
                .expect("failed to load tray icon"),
        )
        .icon_as_template(true)
        .menu(&menu)
        .tooltip("Monet")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "refresh" => {
                let handle = app.clone();
                std::thread::spawn(move || refresh_tray(&handle));
            }
            "quit" => {
                streaming::close_all_sessions();
                agent::shutdown();
                app.exit(0);
            }
            _ => {}
        })
        .build(app)?;

    start_quota_timer(app.clone());
    Ok(())
}

fn build_menu(
    app: &AppHandle,
    info: Option<&quota::QuotaInfo>,
) -> Result<tauri::menu::Menu<tauri::Wry>, Box<dyn std::error::Error>> {
    let zh = is_chinese();
    let mut b = MenuBuilder::new(app);

    if let Some(qi) = info {
        if qi.error.is_none() {
            let plan = qi.plan.as_deref().unwrap_or("");
            let title = if plan.is_empty() {
                "Claude Code".to_string()
            } else {
                format!("Claude Code · {plan}")
            };
            b = b.item(&MenuItemBuilder::new(title).id("title").build(app)?);
            b = b.item(&PredefinedMenuItem::separator(app)?);

            if let Some(s) = &qi.session {
                let label = format_quota_line(
                    if zh { "本轮" } else { "Session" },
                    s.used_percent, s.resets_in_secs, zh,
                );
                b = b.item(&MenuItemBuilder::new(label).id("s").build(app)?);
            }
            if let Some(w) = &qi.weekly {
                let label = format_quota_line(
                    if zh { "每周" } else { "Weekly" },
                    w.used_percent, w.resets_in_secs, zh,
                );
                b = b.item(&MenuItemBuilder::new(label).id("w").build(app)?);
            }
            for (i, m) in qi.weekly_models.iter().enumerate() {
                let name = m.display_name.as_deref().unwrap_or(&m.model);
                let label = format!("{name}  {:.0}%", m.used_percent);
                b = b.item(&MenuItemBuilder::new(label).id(format!("m{i}")).build(app)?);
            }

            b = b.item(&PredefinedMenuItem::separator(app)?);
        }
    }

    let show_label = if zh { "打开 Monet" } else { "Open Monet" };
    let refresh_label = if zh { "刷新额度" } else { "Refresh Quota" };
    let quit_label = if zh { "退出" } else { "Quit" };

    b = b.item(&MenuItemBuilder::new(show_label).id("show").build(app)?);
    b = b.item(&MenuItemBuilder::new(refresh_label).id("refresh").build(app)?);
    b = b.item(&PredefinedMenuItem::separator(app)?);
    b = b.item(&MenuItemBuilder::new(quit_label).id("quit").build(app)?);

    Ok(b.build()?)
}

fn format_quota_line(label: &str, used: f64, resets: Option<i64>, zh: bool) -> String {
    let left = format!("{label}  {used:.0}%");
    let right = format_reset(resets, zh);
    if right.is_empty() { left } else { format!("{left}\t{right}") }
}

fn format_reset(secs: Option<i64>, zh: bool) -> String {
    match secs {
        Some(s) if s > 0 => {
            let h = s / 3600;
            let m = (s % 3600) / 60;
            if zh {
                if h > 24 { format!("{}天{}小时后重置", h / 24, h % 24) }
                else if h > 0 { format!("{h}小时{m}分后重置") }
                else { format!("{m}分后重置") }
            } else {
                if h > 24 { format!("resets in {}d {}h", h / 24, h % 24) }
                else if h > 0 { format!("resets in {h}h {m}m") }
                else { format!("resets in {m}m") }
            }
        }
        _ => String::new(),
    }
}

fn is_chinese() -> bool {
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("defaults")
            .args(["read", "NSGlobalDomain", "AppleLanguages"])
            .output()
        {
            let s = String::from_utf8_lossy(&output.stdout);
            if let Some(first) = s.lines().find(|l| l.contains('"')) {
                return first.contains("zh");
            }
        }
    }
    for key in ["LANG", "LC_ALL", "LC_MESSAGES"] {
        if let Ok(val) = std::env::var(key) {
            if val.starts_with("zh") { return true; }
        }
    }
    false
}

fn refresh_tray(app: &AppHandle) {
    let info = quota::refresh_quota();
    if let Some(err) = &info.error {
        log::warn!("quota fetch error: {err}");
    }
    if let Ok(menu) = build_menu(app, Some(&info)) {
        if let Some(tray) = app.tray_by_id(&TrayIconId::new(TRAY_ID)) {
            let _ = tray.set_menu(Some(menu));
            let _ = tray.set_tooltip(Some(&quota::format_tray_tooltip(&info)));
            let _ = tray.set_title(quota::format_tray_title(&info).as_deref());
            #[cfg(target_os = "macos")]
            patch_menu_styles(&tray);
        }
    }
}

#[cfg(target_os = "macos")]
fn patch_menu_styles(tray: &tauri::tray::TrayIcon<tauri::Wry>) {
    use objc2::{runtime::AnyObject, AnyThread};
    use objc2_app_kit::{
        NSColor, NSFont, NSFontAttributeName, NSForegroundColorAttributeName,
        NSMutableParagraphStyle, NSParagraphStyleAttributeName,
        NSTextAlignment, NSTextTab,
    };
    use objc2_core_foundation::CGFloat;
    use objc2_foundation::{
        MainThreadMarker, NSArray, NSDictionary, NSMutableAttributedString,
        NSRange, NSString,
    };

    let _ = tray.with_inner_tray_icon(|inner| {
        let Some(status_item) = inner.ns_status_item() else { return };
        let Some(mtm) = MainThreadMarker::new() else { return };
        let Some(ns_menu) = status_item.menu(mtm) else { return };

        // Paragraph style with right-aligned tab stop
        let para = NSMutableParagraphStyle::new();
        let empty_tabs = NSArray::<NSTextTab>::new();
        para.setTabStops(Some(&empty_tabs));
        let tab = unsafe {
            NSTextTab::initWithTextAlignment_location_options(
                NSTextTab::alloc(),
                NSTextAlignment::Right,
                TAB_STOP_PT as CGFloat,
                &NSDictionary::<NSString, AnyObject>::new(),
            )
        };
        para.addTabStop(&tab);

        let menu_font = NSFont::menuFontOfSize(0.0);
        let bold_font = NSFont::boldSystemFontOfSize(0.0);
        let gray = unsafe { NSColor::secondaryLabelColor() };

        let count = ns_menu.numberOfItems();
        for i in 0..count {
            let Some(item) = ns_menu.itemAtIndex(i) else { continue };
            if item.isSeparatorItem() { continue; }

            let title_rs = item.title().to_string();

            // Title row: bold
            if title_rs.starts_with("Claude Code") {
                let ns_str = NSString::from_str(&title_rs);
                let attr = unsafe {
                    NSMutableAttributedString::initWithString(
                        NSMutableAttributedString::alloc(), &ns_str,
                    )
                };
                let full = NSRange { location: 0, length: ns_str.length() };
                let bold_obj: &AnyObject = &*bold_font;
                unsafe {
                    attr.addAttribute_value_range(NSFontAttributeName, bold_obj, full);
                }
                item.setAttributedTitle(Some(&attr));
                continue;
            }

            // Quota rows with tab: left normal, right gray
            if title_rs.contains('\t') {
                let ns_str = NSString::from_str(&title_rs);
                let attr = unsafe {
                    NSMutableAttributedString::initWithString(
                        NSMutableAttributedString::alloc(), &ns_str,
                    )
                };
                let full = NSRange { location: 0, length: ns_str.length() };

                let font_obj: &AnyObject = &*menu_font;
                let para_obj: &AnyObject = &*para;
                unsafe {
                    attr.addAttribute_value_range(NSFontAttributeName, font_obj, full);
                    attr.addAttribute_value_range(NSParagraphStyleAttributeName, para_obj, full);
                }

                // Gray color for everything after \t
                if let Some(tab_byte) = title_rs.find('\t') {
                    let tab_utf16 = title_rs[..tab_byte].encode_utf16().count() + 1;
                    let rest_len = ns_str.length() - tab_utf16;
                    if rest_len > 0 {
                        let rest_range = NSRange { location: tab_utf16, length: rest_len };
                        let gray_obj: &AnyObject = &*gray;
                        unsafe {
                            attr.addAttribute_value_range(
                                NSForegroundColorAttributeName, gray_obj, rest_range,
                            );
                        }
                    }
                }

                item.setAttributedTitle(Some(&attr));
                continue;
            }

            // Model rows (no tab, but contain %): attributedTitle for consistent styling
            if title_rs.contains('%') {
                let ns_str = NSString::from_str(&title_rs);
                let attr = unsafe {
                    NSMutableAttributedString::initWithString(
                        NSMutableAttributedString::alloc(), &ns_str,
                    )
                };
                let full = NSRange { location: 0, length: ns_str.length() };
                let font_obj: &AnyObject = &*menu_font;
                unsafe {
                    attr.addAttribute_value_range(NSFontAttributeName, font_obj, full);
                }
                item.setAttributedTitle(Some(&attr));
            }
        }
    });
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn start_quota_timer(app: AppHandle) {
    if QUOTA_TIMER_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(3));
        loop {
            if quota::quota_available() {
                refresh_tray(&app);
            }
            std::thread::sleep(Duration::from_secs(120));
        }
    });
}
