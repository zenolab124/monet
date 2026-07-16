use std::path::PathBuf;
use std::process::Command;

const LAUNCH_AGENT_LABEL: &str = "io.github.zenolab124.monet.tray";

/// 用户「退出菜单栏」意图的持久化标记。
/// 存在 = 用户主动退过 tray，主应用启动不再自动拉起；
/// 设置页「启动菜单栏」按钮会清除它。monet-tray 退出时写入（双端共享此路径约定）。
pub fn disabled_marker_path() -> PathBuf {
    crate::config::data_dir().join("tray-disabled")
}

fn plist_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
        h.join("Library/LaunchAgents")
            .join(format!("{LAUNCH_AGENT_LABEL}.plist"))
    })
}

fn tray_bin_path() -> Option<PathBuf> {
    // Helper App: Monet.app/Contents/Library/LoginItems/MonetTray.app/Contents/MacOS/monet-tray
    // 主应用二进制在: Monet.app/Contents/MacOS/app
    std::env::current_exe().ok().and_then(|p| {
        p.parent() // MacOS/
            .and_then(|d| d.parent()) // Contents/
            .map(|contents| {
                contents.join("Library/LoginItems/MonetTray.app/Contents/MacOS/monet-tray")
            })
    })
}

fn build_plist(bin: &std::path::Path) -> String {
    let bin_str = bin.to_string_lossy();
    // MONET_TRAY_VERSION 嵌入主应用版本号：升级后 plist 内容变化 →
    // ensure_launch_agent 走重装分支（bootout 杀旧进程 + bootstrap 起新二进制），
    // 否则 tray 常驻旧版本永不更新
    let version = env!("CARGO_PKG_VERSION");
    let log_path = crate::config::data_dir().join("tray.log");
    let log_str = log_path.to_string_lossy();
    format!(
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
	<key>EnvironmentVariables</key>
	<dict>
		<key>MONET_TRAY_VERSION</key>
		<string>{version}</string>
	</dict>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>SuccessfulExit</key>
		<false/>
	</dict>
	<key>StandardErrorPath</key>
	<string>{log_str}</string>
</dict>
</plist>"#
    )
}

fn launchctl_targets() -> (String, String) {
    let uid = unsafe { libc::getuid() };
    let domain = format!("gui/{uid}");
    let service = format!("{domain}/{LAUNCH_AGENT_LABEL}");
    (domain, service)
}

/// 安装/更新 tray LaunchAgent（幂等）。
/// launchctl 有 IO 开销且可能慢，调用方须在非主线程执行。
pub fn ensure_launch_agent() {
    // 用户主动退过 tray → 尊重意图，不装不拉起
    if disabled_marker_path().exists() {
        return;
    }
    install_and_start();
}

/// 菜单栏常驻是否开启（= 用户未主动关闭）。
/// plist 的 RunAtLoad 决定开机自启，此标记决定要不要装/拉起 plist。
#[tauri::command]
pub fn get_tray_enabled() -> bool {
    !disabled_marker_path().exists()
}

/// 设置页开关：开 = 清标记 + 安装 + 拉起；关 = 写标记 + bootout（终止进程）
#[tauri::command]
pub fn set_tray_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        let _ = std::fs::remove_file(disabled_marker_path());
        install_and_start();

        // 校验确实起来了，给前端可感知的失败
        let (_, service) = launchctl_targets();
        let ok = Command::new("launchctl")
            .args(["print", &service])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            Ok(())
        } else {
            Err("tray launch failed: service not registered".into())
        }
    } else {
        std::fs::write(disabled_marker_path(), "")
            .map_err(|e| format!("write disabled marker: {e}"))?;
        let (_, service) = launchctl_targets();
        let _ = Command::new("launchctl")
            .args(["bootout", &service])
            .output();
        Ok(())
    }
}

fn install_and_start() {
    let Some(plist_path) = plist_path() else { return };
    let Some(tray_bin) = tray_bin_path() else { return };
    if !tray_bin.exists() {
        // dev 模式（cargo target 目录）无 Helper App 布局，tray 不可用
        log::info!("monet-tray helper not found at {tray_bin:?}, skipping LaunchAgent install");
        return;
    }

    let plist = build_plist(&tray_bin);
    let (domain, service) = launchctl_targets();

    let need_install = std::fs::read_to_string(&plist_path)
        .map(|existing| existing != plist)
        .unwrap_or(true);

    if need_install {
        // 全新 macOS 账号可能没有 LaunchAgents 目录
        if let Some(parent) = plist_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // bootout 杀掉旧版进程（版本升级场景），bootstrap 拉起新二进制
        let _ = Command::new("launchctl")
            .args(["bootout", &service])
            .output();
        if std::fs::write(&plist_path, &plist).is_ok() {
            let _ = Command::new("launchctl")
                .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
                .output();
        }
    } else {
        // plist 未变（同版本）：tray 可能被 kill/崩溃后未重启，kickstart 兜底。
        // kickstart 只能踢「已注册」的服务——服务被 bootout 注销过（手动运维等场景,
        // 实测事故）时它找不到目标静默死路,失败即回退 bootstrap 重新注册
        let kicked = Command::new("launchctl")
            .args(["kickstart", &service])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !kicked {
            let _ = Command::new("launchctl")
                .args(["bootstrap", &domain, &plist_path.to_string_lossy()])
                .output();
        }
    }
}
