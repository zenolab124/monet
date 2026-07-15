use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use app_lib::quota::{self, QuotaInfo};

fn main() {
    #[cfg(target_os = "macos")]
    macos_main();

    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("monet-tray is macOS-only");
        std::process::exit(1);
    }
}

#[cfg(target_os = "macos")]
const TAB_STOP_PT: f64 = 220.0;

#[cfg(target_os = "macos")]
fn macos_main() {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    use objc2_foundation::MainThreadMarker;

    let mtm = MainThreadMarker::new().expect("must run on main thread");
    let ns_app = NSApplication::sharedApplication(mtm);
    ns_app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let icon = load_icon();
    let menu = build_menu(None);

    let tray = tray_icon::TrayIconBuilder::new()
        .with_icon(icon)
        .with_icon_as_template(true)
        .with_menu(Box::new(menu))
        .with_tooltip("Monet")
        .build()
        .expect("failed to create tray icon");

    let menu_channel = tray_icon::menu::MenuEvent::receiver();
    let pending: Arc<Mutex<Option<QuotaInfo>>> = Arc::new(Mutex::new(None));
    let fetching = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut last_refresh = Instant::now() - Duration::from_secs(120);

    loop {
        run_loop_once(1.0);

        while let Ok(event) = menu_channel.try_recv() {
            match event.id.0.as_str() {
                "show" => open_main_app(),
                "refresh" => {
                    request_refresh(&pending, &fetching);
                    last_refresh = Instant::now();
                }
                "quit" => {
                    unregister_and_exit();
                }
                _ => {}
            }
        }

        // 主线程消费后台线程的 quota 结果
        if let Ok(mut guard) = pending.try_lock() {
            if let Some(info) = guard.take() {
                apply_to_tray(&tray, &info);
            }
        }

        if last_refresh.elapsed() >= Duration::from_secs(120) {
            if quota::quota_available() {
                request_refresh(&pending, &fetching);
            }
            last_refresh = Instant::now();
        }
    }
}

/// 发起后台 quota 刷新（不阻塞主线程）
fn request_refresh(
    pending: &Arc<Mutex<Option<QuotaInfo>>>,
    fetching: &Arc<std::sync::atomic::AtomicBool>,
) {
    if fetching.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let pending = Arc::clone(pending);
    let fetching = Arc::clone(fetching);
    std::thread::spawn(move || {
        let info = quota::refresh_quota();
        if let Ok(mut guard) = pending.lock() {
            *guard = Some(info);
        }
        fetching.store(false, std::sync::atomic::Ordering::SeqCst);
    });
}

/// 主线程上应用 quota 数据到 tray（set_menu + patch_menu_styles + set_tooltip + set_title）
#[cfg(target_os = "macos")]
fn apply_to_tray(tray: &tray_icon::TrayIcon, info: &QuotaInfo) {
    let menu = build_menu(Some(info));
    patch_menu_styles(&menu);
    let _ = tray.set_menu(Some(Box::new(menu)));
    let _ = tray.set_tooltip(Some(quota::format_tray_tooltip(info)));
    match quota::format_tray_title(info) {
        Some(t) => {
            let _ = tray.set_title(Some(t));
        }
        None => {
            let _ = tray.set_title(None::<String>);
        }
    }
}

#[cfg(target_os = "macos")]
fn run_loop_once(seconds: f64) {
    use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSRunLoop};
    unsafe {
        let date = NSDate::dateWithTimeIntervalSinceNow(seconds);
        let _ = NSRunLoop::currentRunLoop().runMode_beforeDate(NSDefaultRunLoopMode, &date);
    }
}

fn load_icon() -> tray_icon::Icon {
    let img = image::load_from_memory(include_bytes!("../../icons/tray-template.png"))
        .expect("failed to decode tray icon");
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    tray_icon::Icon::from_rgba(rgba.into_raw(), w, h).expect("failed to create tray icon")
}

fn build_menu(info: Option<&QuotaInfo>) -> muda::Menu {
    use muda::{Menu, MenuItem, PredefinedMenuItem};

    let menu = Menu::new();
    let zh = is_chinese();

    if let Some(qi) = info {
        if qi.error.is_none() {
            let plan = qi.plan.as_deref().unwrap_or("");
            let title = if plan.is_empty() {
                "Claude Code".to_string()
            } else {
                format!("Claude Code · {plan}")
            };
            let _ = menu.append(&MenuItem::with_id("title", title, true, None::<muda::accelerator::Accelerator>));
            let _ = menu.append(&PredefinedMenuItem::separator());

            if let Some(s) = &qi.session {
                let label = format_quota_line(
                    if zh { "本轮" } else { "Session" },
                    s.used_percent,
                    s.resets_in_secs,
                    zh,
                );
                let _ = menu.append(&MenuItem::with_id("s", label, true, None::<muda::accelerator::Accelerator>));
            }
            if let Some(w) = &qi.weekly {
                let label = format_quota_line(
                    if zh { "每周" } else { "Weekly" },
                    w.used_percent,
                    w.resets_in_secs,
                    zh,
                );
                let _ = menu.append(&MenuItem::with_id("w", label, true, None::<muda::accelerator::Accelerator>));
            }
            for (i, m) in qi.weekly_models.iter().enumerate() {
                let name = m.display_name.as_deref().unwrap_or(&m.model);
                let label = format!("{name}  {:.0}%", m.used_percent);
                let _ = menu.append(&MenuItem::with_id(
                    format!("m{i}"),
                    label,
                    true,
                    None::<muda::accelerator::Accelerator>,
                ));
            }

            let _ = menu.append(&PredefinedMenuItem::separator());
        }
    }

    let show_label = if zh { "打开 Monet" } else { "Open Monet" };
    let refresh_label = if zh { "刷新额度" } else { "Refresh Quota" };
    let quit_label = if zh { "退出菜单栏" } else { "Quit Menu Bar" };

    let _ = menu.append(&MenuItem::with_id("show", show_label, true, None::<muda::accelerator::Accelerator>));
    let _ = menu.append(&MenuItem::with_id("refresh", refresh_label, true, None::<muda::accelerator::Accelerator>));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id("quit", quit_label, true, None::<muda::accelerator::Accelerator>));

    menu
}

fn open_main_app() {
    // 不用 -b bundle-id：tray 也在同一 bundle 内，macOS 会混淆。
    // 直接用路径打开，确保启动的是主应用窗口进程。
    let app_path = std::env::current_exe()
        .ok()
        .and_then(|p| {
            // 二进制在 Monet.app/Contents/MacOS/monet-tray，往上三级是 .app
            p.parent()
                .and_then(|macos| macos.parent())
                .and_then(|contents| contents.parent())
                .map(|app| app.to_path_buf())
        });
    if let Some(path) = app_path {
        let _ = std::process::Command::new("open")
            .arg(path)
            .spawn();
    }
}

fn unregister_and_exit() {
    // KeepAlive > SuccessfulExit: false 保证 exit(0) 不触发重启
    // 不 bootout——plist 留在位，下次登录或主应用启动时 kickstart 拉起
    std::process::exit(0);
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
            if val.starts_with("zh") {
                return true;
            }
        }
    }
    false
}

fn format_quota_line(label: &str, used: f64, resets: Option<i64>, zh: bool) -> String {
    let left = format!("{label}  {used:.0}%");
    let right = format_reset(resets, zh);
    if right.is_empty() {
        left
    } else {
        format!("{left}\t{right}")
    }
}

fn format_reset(secs: Option<i64>, zh: bool) -> String {
    match secs {
        Some(s) if s > 0 => {
            let h = s / 3600;
            let m = (s % 3600) / 60;
            if zh {
                if h > 24 {
                    format!("{}天{}小时后重置", h / 24, h % 24)
                } else if h > 0 {
                    format!("{h}小时{m}分后重置")
                } else {
                    format!("{m}分后重置")
                }
            } else if h > 24 {
                format!("resets in {}d {}h", h / 24, h % 24)
            } else if h > 0 {
                format!("resets in {h}h {m}m")
            } else {
                format!("resets in {m}m")
            }
        }
        _ => String::new(),
    }
}

// ---------------------------------------------------------------------------
// macOS: attributed string styling (bold title, tab-stop aligned reset times)
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn patch_menu_styles(menu: &muda::Menu) {
    use muda::ContextMenu;
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2_app_kit::{
        NSColor, NSFont, NSFontAttributeName, NSForegroundColorAttributeName,
        NSMutableParagraphStyle, NSParagraphStyleAttributeName, NSTextAlignment, NSTextTab,
    };
    use objc2_core_foundation::CGFloat;
    use objc2_foundation::{
        MainThreadMarker, NSArray, NSDictionary, NSMutableAttributedString, NSRange, NSString,
    };

    let Some(_mtm) = MainThreadMarker::new() else {
        return;
    };

    let ns_menu_ptr = menu.ns_menu();
    let ns_menu: &objc2_app_kit::NSMenu =
        unsafe { &*(ns_menu_ptr as *const objc2_app_kit::NSMenu) };

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
    let gray = NSColor::secondaryLabelColor();

    let count = ns_menu.numberOfItems();
    for i in 0..count {
        let Some(item) = ns_menu.itemAtIndex(i) else {
            continue;
        };
        if item.isSeparatorItem() {
            continue;
        }

        let title_rs = item.title().to_string();

        if title_rs.starts_with("Claude Code") {
            let ns_str = NSString::from_str(&title_rs);
            let attr = NSMutableAttributedString::initWithString(
                NSMutableAttributedString::alloc(),
                &ns_str,
            );
            let full = NSRange {
                location: 0,
                length: ns_str.length(),
            };
            let bold_obj: &AnyObject = &*bold_font;
            unsafe {
                attr.addAttribute_value_range(NSFontAttributeName, bold_obj, full);
            }
            item.setAttributedTitle(Some(&attr));
            continue;
        }

        if title_rs.contains('\t') {
            let ns_str = NSString::from_str(&title_rs);
            let attr = NSMutableAttributedString::initWithString(
                NSMutableAttributedString::alloc(),
                &ns_str,
            );
            let full = NSRange {
                location: 0,
                length: ns_str.length(),
            };

            let font_obj: &AnyObject = &*menu_font;
            let para_obj: &AnyObject = &*para;
            unsafe {
                attr.addAttribute_value_range(NSFontAttributeName, font_obj, full);
                attr.addAttribute_value_range(NSParagraphStyleAttributeName, para_obj, full);
            }

            if let Some(tab_byte) = title_rs.find('\t') {
                let tab_utf16 = title_rs[..tab_byte].encode_utf16().count() + 1;
                let rest_len = ns_str.length() - tab_utf16;
                if rest_len > 0 {
                    let rest_range = NSRange {
                        location: tab_utf16,
                        length: rest_len,
                    };
                    let gray_obj: &AnyObject = &*gray;
                    unsafe {
                        attr.addAttribute_value_range(
                            NSForegroundColorAttributeName,
                            gray_obj,
                            rest_range,
                        );
                    }
                }
            }

            item.setAttributedTitle(Some(&attr));
            continue;
        }

        if title_rs.contains('%') {
            let ns_str = NSString::from_str(&title_rs);
            let attr = NSMutableAttributedString::initWithString(
                NSMutableAttributedString::alloc(),
                &ns_str,
            );
            let full = NSRange {
                location: 0,
                length: ns_str.length(),
            };
            let font_obj: &AnyObject = &*menu_font;
            unsafe {
                attr.addAttribute_value_range(NSFontAttributeName, font_obj, full);
            }
            item.setAttributedTitle(Some(&attr));
        }
    }
}
