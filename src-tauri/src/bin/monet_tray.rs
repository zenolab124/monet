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
    // 完成应用注册握手：裸 NSRunLoop 轮询（无 NSApp.run）场景下的稳态加固
    ns_app.finishLaunching();

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
    let mut title_config_mtime = tray_title_mtime();

    loop {
        run_loop_once(1.0);

        while let Ok(event) = menu_channel.try_recv() {
            match event.id.0.as_str() {
                "show" => open_main_app(),
                "refresh" => {
                    // 手动刷新：强制打 API（跳过磁盘 TTL）
                    request_refresh(&pending, &fetching, true);
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

        // 设置页改了菜单栏标题配置 → mtime 变化 → 用现有缓存即时重渲染（不打 API）
        let mtime = tray_title_mtime();
        if mtime != title_config_mtime {
            title_config_mtime = mtime;
            if let Some(info) = quota::peek_cached_quota() {
                apply_to_tray(&tray, &info);
            }
        }

        if last_refresh.elapsed() >= Duration::from_secs(120) {
            if quota::quota_available() {
                // 周期刷新：共享磁盘缓存 TTL（主应用可能刚刷过，省 API 调用）
                request_refresh(&pending, &fetching, false);
            }
            last_refresh = Instant::now();
        }
    }
}

fn tray_title_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(app_lib::config::data_dir().join("tray-title.json"))
        .ok()
        .and_then(|m| m.modified().ok())
}

/// 发起后台 quota 刷新（不阻塞主线程）。force=true 跳过磁盘缓存 TTL。
fn request_refresh(
    pending: &Arc<Mutex<Option<QuotaInfo>>>,
    fetching: &Arc<std::sync::atomic::AtomicBool>,
    force: bool,
) {
    if fetching.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let pending = Arc::clone(pending);
    let fetching = Arc::clone(fetching);
    std::thread::spawn(move || {
        let info = if force { quota::refresh_quota() } else { quota::get_quota() };
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
    // MonetTray.app 是独立 Helper App，位于 Monet.app/Contents/Library/LoginItems/ 下。
    // 当前二进制: MonetTray.app/Contents/MacOS/monet-tray
    // 主应用:     Monet.app（上 7 级）
    let app_path = std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent() // MacOS/
                .and_then(|d| d.parent()) // Contents/
                .and_then(|d| d.parent()) // MonetTray.app/
                .and_then(|d| d.parent()) // LoginItems/
                .and_then(|d| d.parent()) // Library/
                .and_then(|d| d.parent()) // Contents/
                .and_then(|d| d.parent()) // Monet.app
                .map(|app| app.to_path_buf())
        })
        // 防御：dev 直跑/旧布局下回溯结果不是 .app，避免 open 打开错误目录
        .filter(|p| p.extension().is_some_and(|e| e == "app"));
    if let Some(path) = app_path {
        let _ = std::process::Command::new("open").arg(path).spawn();
    } else {
        // 非标准布局回退 bundle id（Helper 与主应用 id 不同，无歧义）
        let _ = std::process::Command::new("open")
            .args(["-b", "io.github.zenolab124.monet"])
            .spawn();
    }
}

fn unregister_and_exit() {
    // 持久化「用户主动退出」意图：主应用下次启动看到标记就不再自动拉起。
    // 路径约定与 tray_agent::disabled_marker_path 一致。
    let marker = app_lib::config::data_dir().join("tray-disabled");
    let _ = std::fs::write(&marker, "");
    // KeepAlive > SuccessfulExit: false 保证 exit(0) 不触发 launchd 重启
    std::process::exit(0);
}

/// 系统语言进程生命周期内不变，缓存避免每次建菜单都 spawn `defaults` 子进程
fn is_chinese() -> bool {
    static CACHED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *CACHED.get_or_init(detect_chinese)
}

fn detect_chinese() -> bool {
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
