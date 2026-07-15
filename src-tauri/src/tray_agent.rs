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
        .map(|existing| !existing.contains(&*bin_str))
        .unwrap_or(true);

    if need_install {
        let _ = Command::new("launchctl")
            .args(["unload", &plist_path.to_string_lossy()])
            .output();
        if std::fs::write(&plist_path, &plist).is_ok() {
            let _ = Command::new("launchctl")
                .args(["load", &plist_path.to_string_lossy()])
                .output();
        }
    }
}
