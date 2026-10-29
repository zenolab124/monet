//! 远端主机的 Claude Code 引擎实例。
//!
//! 历史源读取 Monet 数据目录里的只读镜像（由 `sync` 维护）；运行时经 ssh 在远端启动
//! claude（见 `launch`），由原生控制器的流式链路承载，所以这里只声明能力、不实现中立运行时。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::health::{self, CheckState, FailureKind, HealthReport};
use super::hosts::{self, RemoteHost, SshTarget};
use crate::engines::core::*;

/// 体检结果缓存时长：引擎中心与选择器频繁读取健康状态，避免每次都建立 ssh 连接
const HEALTH_CACHE_TTL: Duration = Duration::from_secs(60);

pub struct RemoteClaudeEngine {
    descriptor: EngineDescriptor,
    source: crate::engines::claude::ClaudeSource,
    mirror_projects: PathBuf,
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
                // 与本机相同的流式与交互能力；分叉一期不做
                runtime: Some(RuntimeCapabilities {
                    create: true,
                    resume: true,
                    fork: false,
                    fork_with_cwd: false,
                    send_while_running: false,
                    interrupt: true,
                    streaming: StreamingCapabilities {
                        text: StreamGranularity::Delta,
                        reasoning: StreamGranularity::Delta,
                        tool_progress: StreamGranularity::Item,
                    },
                    model_catalog: ModelCatalogKind::Configured,
                    interactions: vec![
                        InteractionKind::Command,
                        InteractionKind::Question,
                        InteractionKind::Plan,
                    ],
                }),
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
        let descriptor = Self::descriptor_for(host)?;
        let mirror_projects =
            super::sync::Mirror::new(super::sync::mirror_root_for(&host.instance_id))
                .projects_dir();
        Ok(Arc::new(Self {
            source: crate::engines::claude::ClaudeSource::mirror(
                descriptor.instance.clone(),
                mirror_projects.clone(),
            ),
            mirror_projects,
            descriptor,
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

/// 体检报告 → 引擎健康状态（纯函数）；`mirrored` 表示本机已有该主机的镜像
pub fn health_from_report(
    instance: EngineInstanceId,
    report: &HealthReport,
    mirrored: bool,
) -> EngineHealth {
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
    // 镜像已存在时离线也能查阅历史；否则说明为什么还没有
    let source = match (mirrored, report.failure) {
        (true, _) => CapabilityHealth::available(),
        (false, Some(kind)) => CapabilityHealth::unavailable(failure_code(kind)),
        (false, None) => CapabilityHealth::unavailable("engine.remote.mirrorPending"),
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
        // 远端 CLI 可用（找得到且版本够新）才能续聊；离线或缺 CLI 时只能查看镜像
        runtime: match (report.usable, report.failure) {
            (true, _) => CapabilityHealth::available(),
            (false, Some(kind)) => CapabilityHealth::unavailable(failure_code(kind)),
            (false, None) => CapabilityHealth::unavailable("engine.remote.cliTooOld"),
        },
        diagnostics,
    }
}

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
                        // 调试版回环时体检本机 shell，与同步、启动一致
                        health::check(super::remote_executor(), &target, claude_path.as_deref())
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
                self.mirror_projects.is_dir(),
            ))
        })
    }

    fn session_source(&self) -> &dyn SessionSource {
        &self.source
    }

    fn notify_source_change(&self, change: SourceChange) {
        self.source.publish_change(change);
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
    fn descriptor_uses_derived_id_and_declares_runtime_without_fork() {
        let descriptor = RemoteClaudeEngine::descriptor_for(&host("DevBox")).unwrap();
        assert_eq!(descriptor.instance.engine_id(), "claude-code");
        assert!(descriptor.instance.instance_id().starts_with("ssh-devbox-"));
        assert_eq!(descriptor.display_name, "Claude Code · DevBox");
        let runtime = descriptor.capabilities.runtime.unwrap();
        assert!(runtime.create && runtime.resume && runtime.interrupt);
        assert!(!runtime.fork);
        assert_eq!(descriptor.ui.session_surface, SessionSurface::Native);
    }

    #[test]
    fn ready_host_is_available_and_can_run_sessions() {
        let health = health_from_report(instance(), &report("os=Linux\nclaude=/usr/local/bin/claude\nversion=2.1.306\nroot=/home/a/.claude\nroot_exists=yes\nprojects=yes\n", Some(0), ""), false);
        assert_eq!(health.status, EngineHealthStatus::Available);
        assert!(health.installed);
        assert_eq!(health.version_supported, Some(true));
        assert!(health.runtime.available);
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
            false,
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
    fn existing_mirror_keeps_history_available_while_offline() {
        let health = health_from_report(
            instance(),
            &report("", Some(255), "Connection timed out"),
            true,
        );
        assert_eq!(health.status, EngineHealthStatus::Unavailable);
        assert!(health.source.available);
        assert!(!health.runtime.available);
    }

    #[test]
    fn missing_cli_is_degraded() {
        let health = health_from_report(instance(), &report("os=Darwin\nclaude_missing=auto\nroot=/Users/a/.claude\nroot_exists=yes\nprojects=no\n", Some(0), ""), false);
        assert_eq!(health.status, EngineHealthStatus::Degraded);
        assert!(!health.installed);
        assert_eq!(
            health.source.reason_code.as_deref(),
            Some("engine.remote.cliNotFound")
        );
    }
}
