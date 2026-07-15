use std::process::Command;

const LAUNCH_AGENT_LABEL: &str = "io.github.zenolab124.monet.tray";

pub fn ensure_launch_agent() {
    let Some(home) = dirs::home_dir() else { return };

    let plist_path = home
        .join("Library/LaunchAgents")
        .join(format!("{LAUNCH_AGENT_LABEL}.plist"));

    let tray_bin = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("monet-tray")));
    let Some(tray_bin) = tray_bin else { return };
    if !tray_bin.exists() {
        return;
    }

    let bin_str = tray_bin.to_string_lossy();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LAUNCH_AGENT_LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{bin_str}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>SuccessfulExit</key>
		<false/>
	</dict>
	<key>StandardErrorPath</key>
	<string>/tmp/monet-tray.log</string>
</dict>
</plist>"#
    );

    let need_install = std::fs::read_to_string(&plist_path)
        .map(|existing| existing != plist)
        .unwrap_or(true);

    let uid = unsafe { libc::getuid() };
    let domain_target = format!("gui/{uid}");
    let service_target = format!("{domain_target}/{LAUNCH_AGENT_LABEL}");

    if need_install {
        let _ = Command::new("launchctl")
            .args(["bootout", &service_target])
            .output();
        if std::fs::write(&plist_path, &plist).is_ok() {
            let _ = Command::new("launchctl")
                .args(["bootstrap", &domain_target, &plist_path.to_string_lossy()])
                .output();
        }
    } else {
        // plist 没变但 tray 可能被用户手动退出了，kickstart 确保在跑
        let _ = Command::new("launchctl")
            .args(["kickstart", &service_target])
            .output();
    }
}

#[tauri::command]
pub fn launch_tray() {
    let uid = unsafe { libc::getuid() };
    let service_target = format!("gui/{uid}/{LAUNCH_AGENT_LABEL}");

    // 先尝试 kickstart（plist 已注册但进程没跑）
    let output = Command::new("launchctl")
        .args(["kickstart", &service_target])
        .output();

    let kicked = output.as_ref().is_ok_and(|o| o.status.success());
    if kicked {
        return;
    }

    // kickstart 失败说明服务未注册，重新 bootstrap
    if let Some(home) = dirs::home_dir() {
        let plist_path = home
            .join("Library/LaunchAgents")
            .join(format!("{LAUNCH_AGENT_LABEL}.plist"));
        if plist_path.exists() {
            let domain = format!("gui/{uid}");
            let _ = Command::new("launchctl")
                .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
                .output();
        }
    }
}
