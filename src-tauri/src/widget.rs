use std::path::PathBuf;
use std::process::Command;

use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct WidgetConfig {
    #[serde(default)]
    pub day_start_hour: i8,
    #[serde(default)]
    pub month_mode: String,
}

impl Default for WidgetConfig {
    fn default() -> Self {
        Self { day_start_hour: 0, month_mode: "natural".into() }
    }
}

fn widget_config_path() -> PathBuf {
    config::data_dir().join("widget-config.json")
}

fn widget_container_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
        h.join("Library/Containers/io.github.zenolab124.monet.widget/Data/widget-data.json")
    })
}

pub fn read_widget_config() -> WidgetConfig {
    std::fs::read_to_string(widget_config_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn get_widget_config() -> WidgetConfig {
    read_widget_config()
}

#[tauri::command]
pub fn set_widget_config(day_start_hour: i8, month_mode: String) -> Result<(), String> {
    let cfg = WidgetConfig { day_start_hour, month_mode };
    let json = serde_json::to_string_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(widget_config_path(), json).map_err(|e| e.to_string())
}

const LAUNCH_AGENT_LABEL: &str = "io.github.zenolab124.monet.widget-updater";

pub fn ensure_launch_agent() {
    let Some(home) = dirs::home_dir() else { return };

    // 旧标签清理:更名前(com.ccspace.widget-updater)安装的 LaunchAgent 会在用户机
    // 残留,检测到旧 plist 就卸载并删除(失败不阻断新装)
    let legacy_plist = home
        .join("Library/LaunchAgents")
        .join("com.ccspace.widget-updater.plist");
    if legacy_plist.exists() {
        let _ = Command::new("launchctl")
            .args(["remove", "com.ccspace.widget-updater"])
            .output();
        let _ = std::fs::remove_file(&legacy_plist);
    }

    let plist_path = home.join("Library/LaunchAgents").join(format!("{LAUNCH_AGENT_LABEL}.plist"));

    let updater = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("widget-updater")));
    let Some(updater) = updater else { return };
    if !updater.exists() {
        return;
    }

    let updater_str = updater.to_string_lossy();
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LAUNCH_AGENT_LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{updater_str}</string>
	</array>
	<key>StartInterval</key>
	<integer>1800</integer>
	<key>RunAtLoad</key>
	<true/>
	<key>StandardErrorPath</key>
	<string>/tmp/monet-widget-updater.log</string>
</dict>
</plist>"#
    );

    let need_install = std::fs::read_to_string(&plist_path)
        .map(|existing| !existing.contains(&*updater_str))
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

#[tauri::command]
pub fn update_widget(
    today_sessions: u32,
    today_tokens: u64,
    models: Vec<String>,
) -> Result<(), String> {
    let backup_path = config::data_dir().join("widget-data.json");

    let mut doc: serde_json::Value = std::fs::read_to_string(&backup_path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));

    let obj = doc.as_object_mut().ok_or("invalid widget data")?;
    obj.insert("todaySessions".into(), today_sessions.into());
    obj.insert("todayTokens".into(), today_tokens.into());
    obj.insert("models".into(), serde_json::json!(models));
    obj.insert("updatedAt".into(), Local::now().to_rfc3339().into());

    let json = serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?;

    if let Some(path) = widget_container_path() {
        let _ = std::fs::write(&path, &json);
    }

    if let Some(parent) = backup_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&backup_path, &json).map_err(|e| e.to_string())
}
