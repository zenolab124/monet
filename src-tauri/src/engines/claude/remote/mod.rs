//! SSH 远端主机：经用户自己的 ssh 配置访问另一台 macOS / Linux 电脑上的 Claude Code。
//!
//! 本模块目前提供主机配置、连接体检与引擎实例注册；镜像同步与远端进程在后续阶段接入。

pub mod adapter;
pub mod executor;
pub mod health;
pub mod hosts;
pub mod sync;

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
        instance_id: hosts::instance_id_for_alias(&connection),
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

/// 调试版本机验收：设置 `MONET_REMOTE_LOOPBACK_ROOT` 后，同步改用本机 shell 读取该目录，
/// 把它当作远端的 Claude 数据根。发布版恒为 None。
fn loopback_root() -> Option<String> {
    #[cfg(debug_assertions)]
    {
        std::env::var("MONET_REMOTE_LOOPBACK_ROOT")
            .ok()
            .filter(|value| !value.is_empty())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

static SYNCING: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// 立即同步一台主机的会话镜像；同一主机同一时间只跑一轮
#[tauri::command]
pub async fn sync_remote_host(alias: String) -> Result<sync::SyncReport, String> {
    let host = hosts::list_hosts()
        .into_iter()
        .find(|h| h.alias == alias)
        .ok_or("unknown_host")?;
    let target = hosts::target_for(&host.alias, host.source == hosts::HostSource::Manual)
        .ok_or("invalid_connection")?;
    {
        let mut running = SYNCING.lock().map_err(|e| e.to_string())?;
        if running.contains(&alias) {
            return Err("sync_in_progress".into());
        }
        running.push(alias.clone());
    }
    let instance_id = host.instance_id.clone();
    let claude_path = host.claude_path.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mirror = sync::Mirror::new(sync::mirror_root_for(&instance_id));
        let cancel = std::sync::atomic::AtomicBool::new(false);
        #[cfg(debug_assertions)]
        if let Some(root) = loopback_root() {
            return sync::sync_once(
                &executor::LocalShellExecutor,
                &target,
                &root,
                &mirror,
                &cancel,
                |_, _| {},
            );
        }
        let _ = loopback_root();
        let report = health::check(&SshExecutor, &target, claude_path.as_deref());
        let root = report
            .data_root
            .filter(|_| report.usable || report.failure == Some(health::FailureKind::CliNotFound))
            .ok_or_else(|| format!("health_{:?}", report.failure).to_lowercase())?;
        sync::sync_once(&SshExecutor, &target, &root, &mirror, &cancel, |_, _| {})
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    if let Ok(mut running) = SYNCING.lock() {
        running.retain(|a| a != &alias);
    }
    let report = result?;
    notify_mirror_changed(&host.instance_id);
    Ok(report)
}

fn notify_mirror_changed(instance_id: &str) {
    use crate::engines::core::{EngineInstanceId, SourceChange, SourceChangeKind};
    let Ok(instance) = EngineInstanceId::new(super::ENGINE_ID, instance_id) else {
        return;
    };
    let Ok(system) = crate::engines::system::get() else {
        return;
    };
    if let Ok(adapter) = system.registry().adapter_arc(&instance) {
        adapter.notify_source_change(SourceChange {
            kind: SourceChangeKind::FullRefresh,
            project: None,
            session: None,
        });
    }
}
