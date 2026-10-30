//! SSH 远端主机：经用户自己的 ssh 配置访问另一台 macOS / Linux 电脑上的 Claude Code。
//!
//! 本模块提供主机配置、连接体检、引擎实例注册与只读镜像同步；远端进程在后续阶段接入。

pub mod adapter;
pub mod executor;
pub mod health;
pub mod hosts;
pub mod launch;
pub mod sync;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, TryLockError};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::engines::core::{
    EngineInstanceId, ProjectRef, SessionRef, SourceChange, SourceChangeKind,
};
use executor::{RemoteExecutor, RemotePlatform, SshExecutor};
use health::HealthReport;
use hosts::{HostSettings, RemoteHost, SshTarget};

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
    let target = hosts::target_for_host(&host).ok_or("invalid_connection")?;
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

/// 每台主机一个同步槽：全量同步与单文件跟随共用 `io` 锁，避免并发改写同步状态
#[derive(Default)]
struct HostSlot {
    io: Mutex<()>,
    /// 正在进行的全量同步的取消标志
    full: Mutex<Option<Arc<AtomicBool>>>,
}

static SLOTS: Mutex<Vec<(String, Arc<HostSlot>)>> = Mutex::new(Vec::new());

fn slot_for(alias: &str) -> Arc<HostSlot> {
    let mut slots = SLOTS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, slot)) = slots.iter().find(|(a, _)| a == alias) {
        return slot.clone();
    }
    let slot = Arc::new(HostSlot::default());
    slots.push((alias.to_string(), slot.clone()));
    slot
}

const PROGRESS_EVENT: &str = "remote-sync-progress";
/// 一轮变化的会话超过这个数（如首次同步）时改发全量刷新，避免逐条读取摘要
const INCREMENTAL_LIMIT: usize = 200;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncProgress<'a> {
    alias: &'a str,
    instance_id: &'a str,
    done: usize,
    total: usize,
    running: bool,
}

fn emit_progress(app: &AppHandle, host: &RemoteHost, done: usize, total: usize, running: bool) {
    let _ = app.emit(
        PROGRESS_EVENT,
        SyncProgress {
            alias: &host.alias,
            instance_id: &host.instance_id,
            done,
            total,
            running,
        },
    );
}

fn host_by_alias(alias: &str) -> Result<(RemoteHost, SshTarget), String> {
    let host = hosts::list_hosts()
        .into_iter()
        .find(|h| h.alias == alias)
        .ok_or("unknown_host")?;
    let target = hosts::target_for_host(&host).ok_or("invalid_connection")?;
    Ok((host, target))
}

/// 调试版回环时用本机 shell，否则走系统 ssh
fn remote_executor() -> &'static dyn RemoteExecutor {
    #[cfg(debug_assertions)]
    if loopback_root().is_some() {
        return &executor::LocalShellExecutor;
    }
    &SshExecutor
}

/// 远端数据根与方言：回环取环境变量，否则由体检解析
fn remote_root(target: &SshTarget, host: &RemoteHost) -> Result<(String, RemotePlatform), String> {
    if let Some(root) = loopback_root() {
        return Ok((root, RemotePlatform::Posix));
    }
    let report = health::check(&SshExecutor, target, host.claude_path.as_deref());
    let platform = report.platform;
    report
        .data_root
        .filter(|_| report.usable || report.failure == Some(health::FailureKind::CliNotFound))
        .map(|root| (root, platform))
        .ok_or_else(|| format!("health_{:?}", report.failure).to_lowercase())
}

/// 立即同步一台主机的会话镜像；同一主机同一时间只跑一轮，进度经 `remote-sync-progress` 推送
#[tauri::command]
pub async fn sync_remote_host(app: AppHandle, alias: String) -> Result<sync::SyncReport, String> {
    let (host, target) = host_by_alias(&alias)?;
    let slot = slot_for(&alias);
    let cancel = {
        let mut full = slot.full.lock().map_err(|e| e.to_string())?;
        if full.is_some() {
            return Err("sync_in_progress".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *full = Some(cancel.clone());
        cancel
    };
    emit_progress(&app, &host, 0, 0, true);
    let result = {
        let (app, host, slot) = (app.clone(), host.clone(), slot.clone());
        tauri::async_runtime::spawn_blocking(move || {
            let (root, platform) = remote_root(&target, &host)?;
            let target = SshTarget { platform, ..target };
            // 遗留的远端进程（Monet 异常退出、断线）：跳过本次仍连着的会话
            launch::cleanup_stale_pids(remote_executor(), &target, &host.instance_id);
            let _io = slot.io.lock().unwrap_or_else(|e| e.into_inner());
            let mirror = sync::Mirror::new(sync::mirror_root_for(&host.instance_id));
            sync::sync_once(
                remote_executor(),
                &target,
                &root,
                &mirror,
                &cancel,
                |done, total| emit_progress(&app, &host, done, total, true),
            )
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|result| result)
    };
    if let Ok(mut full) = slot.full.lock() {
        *full = None;
    }
    emit_progress(&app, &host, 0, 0, false);
    let report = result?;
    publish_mirror_changes(
        &app,
        &host.instance_id,
        &report.changed,
        report.newly_missing > 0,
    );
    Ok(report)
}

/// 取消一台主机正在进行的全量同步；在当前文件拉完后停下，已拉完的文件保留
#[tauri::command]
pub fn cancel_remote_sync(alias: String) -> bool {
    let slot = slot_for(&alias);
    let full = slot.full.lock().unwrap_or_else(|e| e.into_inner());
    match full.as_ref() {
        Some(cancel) => {
            cancel.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

fn host_by_instance(instance: &EngineInstanceId) -> Result<(RemoteHost, SshTarget), String> {
    let host = hosts::list_hosts()
        .into_iter()
        .find(|h| h.instance_id == instance.instance_id())
        .ok_or("unknown_host")?;
    let target = hosts::target_for_host(&host).ok_or("invalid_connection")?;
    Ok((host, target))
}

/// 跟随一个打开中的远端会话：只同步它自己的文件，主文件有变化时顺带同步它的子 Agent。
/// 主机正忙于全量同步时跳过本次，由那一轮带上它。返回镜像是否有变化；有变化时按会话发出增量刷新。
#[tauri::command]
pub async fn follow_remote_session(
    app: AppHandle,
    instance: EngineInstanceId,
    project_id: String,
    session_id: String,
) -> Result<bool, String> {
    let (host, target) = host_by_instance(&instance)?;
    let rel = format!("{project_id}/{session_id}.jsonl");
    let slot = slot_for(&host.alias);
    let instance_id = host.instance_id.clone();
    let follow_rel = rel.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let _io = match slot.io.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => return Ok(sync::Follow::Unchanged),
            Err(TryLockError::Poisoned(e)) => e.into_inner(),
        };
        let mirror = sync::Mirror::new(sync::mirror_root_for(&instance_id));
        let outcome = sync::follow_once(remote_executor(), &target, &mirror, &follow_rel)?;
        if outcome == sync::Follow::Updated {
            // 子 Agent 文件失败不影响主会话的跟随结果
            let _ = sync::sync_subagents(
                remote_executor(),
                &target,
                &mirror,
                &project_id,
                &session_id,
            );
        }
        Ok::<sync::Follow, String>(outcome)
    })
    .await
    .map_err(|e| e.to_string())??;
    if outcome != sync::Follow::Unchanged {
        publish_mirror_changes(
            &app,
            &host.instance_id,
            &[rel],
            outcome == sync::Follow::Gone,
        );
    }
    Ok(outcome != sync::Follow::Unchanged)
}

/// 打开远端会话时按需拉取它的子 Agent 文件；返回有变化的文件数
#[tauri::command]
pub async fn sync_remote_subagents(
    instance: EngineInstanceId,
    project_id: String,
    session_id: String,
) -> Result<usize, String> {
    let (host, target) = host_by_instance(&instance)?;
    let slot = slot_for(&host.alias);
    tauri::async_runtime::spawn_blocking(move || {
        let _io = slot.io.lock().unwrap_or_else(|e| e.into_inner());
        let mirror = sync::Mirror::new(sync::mirror_root_for(&host.instance_id));
        sync::sync_subagents(
            remote_executor(),
            &target,
            &mirror,
            &project_id,
            &session_id,
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 把镜像变化发给界面：少量变化按会话增量（与本机 watcher 同一事件）；大量变化或出现远端删除
/// （增量摘要不带删除标记）时改为该实例全量刷新
fn publish_mirror_changes(
    app: &AppHandle,
    instance_id: &str,
    changed: &[String],
    force_full: bool,
) {
    if changed.is_empty() {
        return;
    }
    let Ok(instance) = EngineInstanceId::new(super::ENGINE_ID, instance_id) else {
        return;
    };
    let full = force_full || changed.len() > INCREMENTAL_LIMIT;
    let sessions: Vec<(&str, &str)> = if full {
        Vec::new()
    } else {
        changed
            .iter()
            .filter_map(|rel| {
                let (project, file) = rel.split_once('/')?;
                Some((project, file.strip_suffix(".jsonl")?))
            })
            .collect()
    };
    if full {
        crate::engines::system::notify_source_change(
            &instance,
            SourceChange {
                kind: SourceChangeKind::FullRefresh,
                project: None,
                session: None,
            },
        );
    }
    for (project, session) in &sessions {
        crate::engines::system::notify_source_change(
            &instance,
            SourceChange {
                kind: SourceChangeKind::SessionChanged,
                project: ProjectRef::new(instance.clone(), *project).ok(),
                session: SessionRef::new(instance.clone(), *session).ok(),
            },
        );
    }
    let changes: Vec<serde_json::Value> = sessions
        .iter()
        .map(|(project, session)| serde_json::json!({ "projectId": project, "sessionId": session }))
        .collect();
    let _ = app.emit(
        "projects-changed",
        serde_json::json!({ "full": full, "changes": changes, "instance": instance }),
    );
}
