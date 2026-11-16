//! 回退期间独占会话输入；CLI 是文件及对话恢复的唯一写入方。
use super::*;
use std::collections::HashSet;

static OPERATIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(super) struct SessionOperation(String);
impl SessionOperation {
    pub(super) fn acquire(id: &str) -> Result<Self, String> {
        let mut active = OPERATIONS.get_or_init(Default::default).lock().unwrap();
        if !active.insert(id.to_string()) {
            return Err("A session operation is already in progress".into());
        }
        Ok(Self(id.into()))
    }
}
impl Drop for SessionOperation {
    fn drop(&mut self) {
        OPERATIONS.get().unwrap().lock().unwrap().remove(&self.0);
    }
}

#[derive(Clone, Copy, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RewindMode {
    Files,
    Conversation,
    Both,
}

#[derive(Default, Serialize)]
pub struct RewindSessionResult {
    pub files_restored: bool,
    pub conversation_rewound: bool,
    pub skipped_links: u64,
    pub prefill_text: Option<String>,
    pub error: Option<String>,
    pub outcome_unknown: bool,
}

fn idle_process(id: &str) -> Result<Arc<Mutex<SessionProcess>>, String> {
    let process = ACTIVE_PROCESSES
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|m| m.get(id).cloned())
        .ok_or("Session process is not connected")?;
    {
        let sp = process.lock().unwrap();
        if sp.turn_active || !sp.pending_injections.is_empty() {
            return Err("Wait for the current task to finish before rewinding".into());
        }
    }
    Ok(process)
}

fn control(
    process: &Arc<Mutex<SessionProcess>>,
    request: impl FnOnce(&str) -> Value,
) -> Result<Value, String> {
    let (reply, receiver) = std::sync::mpsc::channel();
    let request_id = {
        let mut sp = process.lock().unwrap();
        sp.request_counter += 1;
        let id = format!("rewind-{}", sp.request_counter);
        // 与 reader 使用同一把锁，避免回包早于注册。
        write_stdin(&mut sp.stdin, &request(&id))?;
        sp.pending_controls.insert(id.clone(), reply);
        id
    };
    let outcome = receiver.recv_timeout(REWIND_FILES_TIMEOUT);
    process.lock().unwrap().pending_controls.remove(&request_id);
    match outcome {
        Ok(result) => result,
        Err(_) => Err("REWIND_OUTCOME_UNKNOWN".into()),
    }
}

pub(super) fn files(
    id: &str,
    target: &str,
    dry_run: bool,
) -> Result<protocol::RewindFilesResult, String> {
    let _operation = SessionOperation::acquire(id)?;
    let process = idle_process(id)?;
    let result = control(&process, |rid| {
        protocol::rewind_files_request(rid, target, dry_run)
    });
    if result.as_ref().err().map(String::as_str) == Some("REWIND_OUTCOME_UNKNOWN") && !dry_run {
        close_session(id);
    }
    result.map(|body| protocol::parse_rewind_files_result(&body))
}

/// 两种回退不是原子事务：先恢复文件，成功后才回退对话；任何部分失败都逐项回报。
pub fn rewind_session(
    id: &str,
    target: &str,
    last_seen: &str,
    mode: RewindMode,
) -> Result<RewindSessionResult, String> {
    let _operation = SessionOperation::acquire(id)?;
    let process = idle_process(id)?;
    // 远端镜像需额外的同步确认；当前保留远端已支持的仅文件操作。
    if mode != RewindMode::Files && process.lock().unwrap().remote.is_some() {
        return Err("Conversation rewind is only available for local sessions".into());
    }
    if uuid::Uuid::parse_str(target).is_err() || uuid::Uuid::parse_str(last_seen).is_err() {
        return Err("Invalid rewind message identifier".into());
    }
    let result = execute(
        mode,
        || {
            control(&process, |rid| {
                protocol::rewind_files_request(rid, target, false)
            })
            .map(|body| protocol::parse_rewind_files_result(&body))
        },
        || {
            control(&process, |rid| {
                protocol::rewind_conversation_request(rid, target, last_seen)
            })
            .map(|body| protocol::parse_rewind_conversation_result(&body))
        },
    );
    if result.outcome_unknown {
        close_session(id);
    }
    Ok(result)
}

fn execute(
    mode: RewindMode,
    files: impl FnOnce() -> Result<protocol::RewindFilesResult, String>,
    conversation: impl FnOnce() -> Result<protocol::RewindConversationResult, String>,
) -> RewindSessionResult {
    let mut result = RewindSessionResult::default();
    let operation = (|| -> Result<(), String> {
        if mode != RewindMode::Conversation {
            let files = files()?;
            if !files.can_rewind {
                return Err(files
                    .error
                    .unwrap_or_else(|| "File rewind was not confirmed".into()));
            }
            result.files_restored = true;
            result.skipped_links = files.skipped_links;
            if files.skipped_links > 0 && mode == RewindMode::Both {
                return Err("Some files were skipped; the conversation was kept".into());
            }
        }
        if mode != RewindMode::Files {
            let conversation = conversation()?;
            if !conversation.rewound {
                return Err(conversation.error.unwrap_or_default());
            }
            result.conversation_rewound = true;
            result.prefill_text = conversation.prefill_text;
        }
        Ok(())
    })();
    if let Err(error) = operation {
        result.outcome_unknown = error == "REWIND_OUTCOME_UNKNOWN";
        result.error = Some(error);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exclusive_operation_rejects_same_session_and_releases_on_drop() {
        let first = SessionOperation::acquire("rewind-test-a").unwrap();
        assert!(SessionOperation::acquire("rewind-test-a").is_err());
        assert!(SessionOperation::acquire("rewind-test-b").is_ok());
        drop(first);
        assert!(SessionOperation::acquire("rewind-test-a").is_ok());
    }
    #[test]
    fn failed_or_skipped_files_never_discard_conversation() {
        for body in [
            json!({"canRewind":false,"error":"missing checkpoint"}),
            json!({"canRewind":true,"skippedLinks":1}),
        ] {
            let result = execute(
                RewindMode::Both,
                || Ok(protocol::parse_rewind_files_result(&body)),
                || panic!("must keep the conversation"),
            );
            assert!(!result.conversation_rewound);
            assert!(result.error.is_some());
        }
    }

    #[test]
    fn reports_partial_success_and_uncertain_outcomes_without_retry() {
        let files = || {
            Ok(protocol::parse_rewind_files_result(
                &json!({"canRewind":true}),
            ))
        };
        let result = execute(RewindMode::Both, files, || {
            Err("REWIND_OUTCOME_UNKNOWN".into())
        });
        assert!(result.files_restored);
        assert!(!result.conversation_rewound);
        assert!(result.outcome_unknown);
        let refused = execute(RewindMode::Both, files, || {
            Ok(protocol::parse_rewind_conversation_result(
                &json!({"rewound":false,"error":"stale target"}),
            ))
        });
        assert!(refused.files_restored);
        assert!(!refused.outcome_unknown);
        assert_eq!(refused.error.as_deref(), Some("stale target"));
    }

    #[test]
    fn conversation_only_never_touches_files() {
        let result = execute(
            RewindMode::Conversation,
            || panic!("must not write files"),
            || {
                Ok(protocol::parse_rewind_conversation_result(
                    &json!({"rewound":true,"prefillText":"original"}),
                ))
            },
        );
        assert!(result.conversation_rewound);
        assert!(!result.files_restored);
        assert_eq!(result.prefill_text.as_deref(), Some("original"));
    }
}
