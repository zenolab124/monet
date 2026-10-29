//! 远端主机的 Claude Code 引擎实例。
//!
//! 本阶段只负责注册与健康状态：历史源尚未接入只读镜像，运行时尚未接入远端进程，
//! 因此描述符不声明运行时能力——新建任务等入口据此把它显示为不可选，
//! 不会误用本机进程。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::executor::SshExecutor;
use super::health::{self, CheckState, FailureKind, HealthReport};
use super::hosts::{self, RemoteHost, SshTarget};
use crate::engines::core::*;

/// 体检结果缓存时长：引擎中心与选择器频繁读取健康状态，避免每次都建立 ssh 连接
const HEALTH_CACHE_TTL: Duration = Duration::from_secs(60);

pub struct RemoteClaudeEngine {
    descriptor: EngineDescriptor,
    target: SshTarget,
    claude_path: Option<String>,
    cached: Mutex<Option<(Instant, HealthReport)>>,
}

impl RemoteClaudeEngine {
    pub fn descriptor_for(host: &RemoteHost) -> EngineResult<EngineDescriptor> {
        Ok(EngineDescriptor {
            instance: EngineInstanceId::new(super::super::ENGINE_ID, host.instance_id.as_str())?,
            display_name: format!("Claude Code · {}", host.alias),
            enabled: true,
            capabilities: EngineCapabilities {
                history: HistoryCapabilities {
                    pagination: HistoryPagination::Emulated,
                    change_delivery: ChangeDelivery::Poll,
                    search: false,
                    assets: false,
                },
                runtime: None,
                facets: FacetCapabilities {
                    assets: false,
                    automation: false,
                    configuration: false,
                    quota: false,
                    runtime_commands: false,
                },
            },
            ui: EngineUiIntegration {
                identity: UiIdentityMode::Native,
                session_surface: SessionSurface::Native,
                install_guide_url: Some("https://code.claude.com/docs/en/installation".into()),
            },
        })
    }

    pub fn new(host: &RemoteHost) -> EngineResult<Arc<Self>> {
        let target = hosts::target_for(&host.alias, host.source == hosts::HostSource::Manual)
            .ok_or_else(|| {
                EngineError::new(
                    EngineErrorKind::InvalidIdentity,
                    "invalid remote host alias",
                )
            })?;
        Ok(Arc::new(Self {
            descriptor: Self::descriptor_for(host)?,
            target,
            claude_path: host.claude_path.clone(),
            cached: Mutex::new(None),
        }))
    }

    fn cached_report(&self) -> Option<HealthReport> {
        let cached = self.cached.lock().ok()?;
        cached
            .as_ref()
            .filter(|(at, _)| at.elapsed() < HEALTH_CACHE_TTL)
            .map(|(_, report)| report.clone())
    }
}

fn failure_code(kind: FailureKind) -> &'static str {
    match kind {
        FailureKind::Unreachable => "engine.remote.unreachable",
        FailureKind::AuthFailed => "engine.remote.authFailed",
        FailureKind::HostKeyUntrusted => "engine.remote.hostKeyUntrusted",
        FailureKind::UnsupportedOs => "engine.remote.unsupportedOs",
        FailureKind::CliNotFound => "engine.remote.cliNotFound",
        FailureKind::DataRootMissing => "engine.remote.dataRootMissing",
        FailureKind::Timeout => "engine.remote.timeout",
    }
}

/// 体检报告 → 引擎健康状态（纯函数）
pub fn health_from_report(instance: EngineInstanceId, report: &HealthReport) -> EngineHealth {
    let ssh_failed = report
        .items
        .iter()
        .any(|item| item.id == "ssh" && item.state == CheckState::Fail);
    let status = if ssh_failed {
        EngineHealthStatus::Unavailable
    } else if report.usable {
        EngineHealthStatus::Available
    } else {
        EngineHealthStatus::Degraded
    };
    let version_supported = report
        .items
        .iter()
        .find(|item| item.id == "cli_version")
        .and_then(|item| match item.state {
            CheckState::Pass => Some(true),
            CheckState::Warn => Some(false),
            _ => None,
        });
    let source = match report.failure {
        Some(kind) => CapabilityHealth::unavailable(failure_code(kind)),
        None => CapabilityHealth::unavailable("engine.remote.mirrorPending"),
    };
    let diagnostics = report
        .failure
        .map(|kind| HealthDiagnostic {
            code: failure_code(kind)
                .trim_start_matches("engine.remote.")
                .into(),
            message: report.stderr_tail.clone().unwrap_or_default(),
        })
        .into_iter()
        .filter(|diagnostic| !diagnostic.message.is_empty())
        .collect();
    EngineHealth {
        instance,
        status,
        installed: report.cli_path.is_some(),
        authenticated: None,
        version: report.cli_version.clone(),
        version_supported,
        executable_path: report.cli_path.clone(),
        source,
        runtime: CapabilityHealth::unavailable("engine.remote.runtimePending"),
        diagnostics,
    }
}

/// 尚未接入镜像的空历史源
struct EmptySource;

fn not_found() -> EngineError {
    EngineError::new(
        EngineErrorKind::NotFound,
        "remote session history is not available yet",
    )
}

impl SessionSource for EmptySource {
    fn list_projects(&self, _query: ProjectQuery) -> EngineFuture<'_, ProjectPage> {
        Box::pin(async {
            Ok(ProjectPage {
                projects: Vec::new(),
                next_cursor: None,
            })
        })
    }

    fn list_sessions(
        &self,
        _project: ProjectRef,
        _query: SessionQuery,
    ) -> EngineFuture<'_, SessionPage> {
        Box::pin(async {
            Ok(SessionPage {
                sessions: Vec::new(),
                next_cursor: None,
            })
        })
    }

    fn load_timeline(
        &self,
        _session: SessionRef,
        _page: TimelinePage,
    ) -> EngineFuture<'_, ConversationPage> {
        Box::pin(async { Err(not_found()) })
    }

    fn subscribe_changes(&self, _sink: SourceChangeSink) -> EngineResult<SubscriptionHandle> {
        Ok(Box::new(()))
    }

    fn build_search_document(&self, _session: SessionRef) -> EngineFuture<'_, SearchDocument> {
        Box::pin(async { Err(not_found()) })
    }

    fn resolve_asset(&self, _asset: AssetRef) -> EngineFuture<'_, ResolvedAsset> {
        Box::pin(async { Err(not_found()) })
    }

    fn session_actions(&self, _session: SessionRef) -> EngineFuture<'_, SessionActions> {
        Box::pin(async { Err(not_found()) })
    }
}

static EMPTY_SOURCE: EmptySource = EmptySource;

impl EngineAdapter for RemoteClaudeEngine {
    fn descriptor(&self) -> EngineDescriptor {
        self.descriptor.clone()
    }

    fn health(&self) -> EngineFuture<'_, EngineHealth> {
        Box::pin(async move {
            let report = match self.cached_report() {
                Some(report) => report,
                None => {
                    let target = self.target.clone();
                    let claude_path = self.claude_path.clone();
                    let report = tauri::async_runtime::spawn_blocking(move || {
                        health::check(&SshExecutor, &target, claude_path.as_deref())
                    })
                    .await
                    .map_err(|error| {
                        EngineError::new(EngineErrorKind::Internal, error.to_string())
                    })?;
                    if let Ok(mut cached) = self.cached.lock() {
                        *cached = Some((Instant::now(), report.clone()));
                    }
                    report
                }
            };
            Ok(health_from_report(
                self.descriptor.instance.clone(),
                &report,
            ))
        })
    }

    fn session_source(&self) -> &dyn SessionSource {
        &EMPTY_SOURCE
    }

    fn runtime(&self) -> Option<&dyn AgentRuntime> {
        None
    }
}

/// 启动时需要注册的远端主机：已启用且别名可连接
pub fn enabled_hosts() -> Vec<RemoteHost> {
    hosts::list_hosts()
        .into_iter()
        .filter(|host| host.enabled)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engines::claude::remote::executor::ExecOutput;

    fn host(alias: &str) -> RemoteHost {
        RemoteHost {
            alias: alias.into(),
            instance_id: hosts::instance_id_for_alias(alias),
            source: hosts::HostSource::SshConfig,
            enabled: true,
            claude_path: None,
        }
    }

    fn instance() -> EngineInstanceId {
        EngineInstanceId::new("claude-code", "ssh-devbox-000001").unwrap()
    }

    fn report(stdout: &str, code: Option<i32>, stderr: &str) -> HealthReport {
        health::report_from_output(&ExecOutput {
            code,
            stdout: stdout.into(),
            stderr: stderr.into(),
            timed_out: false,
        })
    }

    #[test]
    fn descriptor_uses_derived_id_and_declares_no_runtime_yet() {
        let descriptor = RemoteClaudeEngine::descriptor_for(&host("DevBox")).unwrap();
        assert_eq!(descriptor.instance.engine_id(), "claude-code");
        assert!(descriptor.instance.instance_id().starts_with("ssh-devbox-"));
        assert_eq!(descriptor.display_name, "Claude Code · DevBox");
        assert!(descriptor.capabilities.runtime.is_none());
        assert_eq!(descriptor.ui.session_surface, SessionSurface::Native);
    }

    #[test]
    fn ready_host_is_available_with_runtime_pending() {
        let health = health_from_report(
            instance(),
            &report("os=Linux\nclaude=/usr/local/bin/claude\nversion=2.1.306\nroot=/home/a/.claude\nroot_exists=yes\nprojects=yes\n", Some(0), ""),
        );
        assert_eq!(health.status, EngineHealthStatus::Available);
        assert!(health.installed);
        assert_eq!(health.version_supported, Some(true));
        assert_eq!(
            health.runtime.reason_code.as_deref(),
            Some("engine.remote.runtimePending")
        );
        assert_eq!(
            health.source.reason_code.as_deref(),
            Some("engine.remote.mirrorPending")
        );
        assert!(health.diagnostics.is_empty());
    }

    #[test]
    fn unreachable_host_is_unavailable_with_stderr_diagnostic() {
        let health = health_from_report(
            instance(),
            &report(
                "",
                Some(255),
                "ssh: connect to host h port 22: Connection refused",
            ),
        );
        assert_eq!(health.status, EngineHealthStatus::Unavailable);
        assert_eq!(
            health.source.reason_code.as_deref(),
            Some("engine.remote.unreachable")
        );
        assert_eq!(health.diagnostics.len(), 1);
        assert_eq!(health.diagnostics[0].code, "unreachable");
    }

    #[test]
    fn missing_cli_is_degraded() {
        let health = health_from_report(instance(), &report("os=Darwin\nclaude_missing=auto\nroot=/Users/a/.claude\nroot_exists=yes\nprojects=no\n", Some(0), ""));
        assert_eq!(health.status, EngineHealthStatus::Degraded);
        assert!(!health.installed);
        assert_eq!(
            health.source.reason_code.as_deref(),
            Some("engine.remote.cliNotFound")
        );
    }
}
