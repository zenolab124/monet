use serde::{Deserialize, Serialize};
use crate::config;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidget {
    pub id: String,
    pub name: String,
    pub description: String,
    pub width: u32,
    pub height: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardWidgetWithHtml {
    #[serde(flatten)]
    pub meta: DashboardWidget,
    pub html: String,
}

fn widgets_meta_path() -> PathBuf {
    config::data_dir().join("widgets.json")
}

fn widgets_dir() -> PathBuf {
    config::data_dir().join("widgets")
}

fn load_widgets() -> Vec<DashboardWidget> {
    let path = widgets_meta_path();
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub fn list_dashboard_widgets() -> Result<Vec<DashboardWidgetWithHtml>, String> {
    let widgets = load_widgets();
    let dir = widgets_dir();
    let result: Vec<_> = widgets
        .into_iter()
        .map(|w| {
            let html = std::fs::read_to_string(dir.join(format!("{}.html", w.id)))
                .unwrap_or_default();
            DashboardWidgetWithHtml { meta: w, html }
        })
        .collect();
    Ok(result)
}

#[tauri::command]
pub fn delete_dashboard_widget(id: String) -> Result<(), String> {
    let mut widgets = load_widgets();
    let before = widgets.len();
    widgets.retain(|w| w.id != id);
    if widgets.len() == before {
        return Err(format!("未找到 widget: {}", id));
    }
    let path = widgets_meta_path();
    let json = serde_json::to_string_pretty(&widgets).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(widgets_dir().join(format!("{}.html", id)));
    Ok(())
}
