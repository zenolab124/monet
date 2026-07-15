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
    let mut last_refresh = Instant::now() - Duration::from_secs(120);

    loop {
        run_loop_once(1.0);

        while let Ok(event) = menu_channel.try_recv() {
            match event.id.0.as_str() {
                "show" => open_main_app(),
                "refresh" => {
                    refresh_tray(&tray);
                    last_refresh = Instant::now();
                }
                "quit" => std::process::exit(0),
                _ => {}
            }
        }

        if last_refresh.elapsed() >= Duration::from_secs(120) {
            if quota::quota_available() {
                refresh_tray(&tray);
            }
            last_refresh = Instant::now();
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

fn build_menu(info: Option<&QuotaInfo>) -> tray_icon::menu::Menu {
    use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};

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
            let _ = menu.append(&MenuItem::with_id("title", title, false, None::<muda::accelerator::Accelerator>));
            let _ = menu.append(&PredefinedMenuItem::separator());

            if let Some(s) = &qi.session {
                let label = format_quota_line(
                    if zh { "本轮" } else { "Session" },
                    s.used_percent,
                    s.resets_in_secs,
                    zh,
                );
                let _ = menu.append(&MenuItem::with_id("s", label, false, None::<muda::accelerator::Accelerator>));
            }
            if let Some(w) = &qi.weekly {
                let label = format_quota_line(
                    if zh { "每周" } else { "Weekly" },
                    w.used_percent,
                    w.resets_in_secs,
                    zh,
                );
                let _ = menu.append(&MenuItem::with_id("w", label, false, None::<muda::accelerator::Accelerator>));
            }
            for (i, m) in qi.weekly_models.iter().enumerate() {
                let name = m.display_name.as_deref().unwrap_or(&m.model);
                let label = format!("{name}  {:.0}%", m.used_percent);
                let _ = menu.append(&MenuItem::with_id(format!("m{i}"), label, false, None::<muda::accelerator::Accelerator>));
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

fn refresh_tray(tray: &tray_icon::TrayIcon) {
    let info = quota::refresh_quota();
    let menu = build_menu(Some(&info));
    let _ = tray.set_menu(Some(Box::new(menu)));
    let _ = tray.set_tooltip(Some(quota::format_tray_tooltip(&info)));
    if let Some(title) = quota::format_tray_title(&info) {
        let _ = tray.set_title(Some(title));
    } else {
        let _ = tray.set_title(None::<String>);
    }
}

fn open_main_app() {
    let _ = std::process::Command::new("open")
        .args(["-b", "io.github.zenolab124.monet"])
        .spawn();
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
        format!("{left}  {right}")
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
