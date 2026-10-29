//! SSH 远端主机：经用户自己的 ssh 配置访问另一台 macOS / Linux 电脑上的 Claude Code。
//!
//! 本模块目前提供主机配置与连接体检；远端实例注册、镜像同步与远端进程在后续阶段接入。

pub mod executor;
pub mod health;
pub mod hosts;

use executor::SshExecutor;
use health::HealthReport;
use hosts::{HostSettings, RemoteHost};

#[tauri::command]
pub fn list_remote_hosts() -> Vec<RemoteHost> {
    hosts::list_hosts()
}

/// 手动添加 `user@host[:port]`；格式无效时拒绝
#[tauri::command]
pub fn add_remote_host(connection: String) -> Result<RemoteHost, String> {
    let connection = connection.trim().to_string();
    if hosts::parse_manual_target(&connection).is_none() {
        return Err("invalid_connection".into());
    }
    let mut store = hosts::load_store();
    if store.hosts.get(&connection).is_some_and(|h| h.manual) {
        return Err("duplicate_host".into());
    }
    store.hosts.insert(
        connection.clone(),
        HostSettings {
            enabled: false,
            claude_path: None,
            manual: true,
        },
    );
    hosts::save_store(&store)?;
    Ok(RemoteHost {
        alias: connection,
        source: hosts::HostSource::Manual,
        enabled: false,
        claude_path: None,
    })
}

#[tauri::command]
pub fn update_remote_host(
    alias: String,
    enabled: bool,
    claude_path: Option<String>,
) -> Result<(), String> {
    let claude_path = claude_path
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty());
    if claude_path
        .as_deref()
        .is_some_and(|p| !hosts::valid_claude_path(p))
    {
        return Err("invalid_claude_path".into());
    }
    let mut store = hosts::load_store();
    let known = hosts::list_hosts().into_iter().any(|h| h.alias == alias);
    if !known {
        return Err("unknown_host".into());
    }
    let entry = store.hosts.entry(alias).or_default();
    entry.enabled = enabled;
    entry.claude_path = claude_path;
    hosts::save_store(&store)
}

/// 只能移除手动添加的主机；ssh config 中的主机由用户在配置文件里管理
#[tauri::command]
pub fn remove_remote_host(alias: String) -> Result<(), String> {
    let mut store = hosts::load_store();
    match store.hosts.get(&alias) {
        Some(settings) if settings.manual => {
            store.hosts.remove(&alias);
            hosts::save_store(&store)
        }
        _ => Err("not_manual_host".into()),
    }
}

#[tauri::command]
pub async fn check_remote_host(alias: String) -> Result<HealthReport, String> {
    let host = hosts::list_hosts()
        .into_iter()
        .find(|h| h.alias == alias)
        .ok_or("unknown_host")?;
    let target = hosts::target_for(&host.alias, host.source == hosts::HostSource::Manual)
        .ok_or("invalid_connection")?;
    tauri::async_runtime::spawn_blocking(move || {
        health::check(&SshExecutor, &target, host.claude_path.as_deref())
    })
    .await
    .map_err(|e| e.to_string())
}
