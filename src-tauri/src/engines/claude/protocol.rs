//! Claude Code stream-json 协议的纯函数解析层。
//!
//! 流式层（`streaming.rs`）只负责读写进程管道；这里把 CLI 报出的事件、控制回包与
//! 反向请求翻译成中立结构。所有解析宽松：字段缺失或类型不符时返回 None 或空集合，
//! 由调用方回退现有行为，绝不让一条畸形消息中断会话。

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Value};

/// 本期协议对照基线：Monet 已按该版本 CLI 的 schema 核对过本模块解析的全部字段。
pub const VERIFIED_CLI_VERSION: &str = "2.1.306";

/// 权限模式切换请求的 request_id 前缀；回包据此认领。
pub const PERMISSION_REQUEST_PREFIX: &str = "set-perm-mode-";

/// 宿主未实现的反向请求统一回复的错误文本。
pub const UNSUPPORTED_BY_HOST: &str = "unsupported by host";

// ---------------------------------------------------------------------------
// 控制回包
// ---------------------------------------------------------------------------

/// CLI 对宿主控制请求的判决。
#[derive(Debug, Clone, PartialEq)]
pub enum ControlOutcome {
    Success,
    Error(String),
}

/// 拆出 control_response 的 request_id、判决与响应体（`response.response`）。
pub fn control_response(value: &Value) -> Option<(String, ControlOutcome, Option<&Value>)> {
    if value.get("type")?.as_str()? != "control_response" {
        return None;
    }
    let response = value.get("response")?;
    let request_id = response.get("request_id")?.as_str()?.to_string();
    let outcome = match response.get("subtype").and_then(Value::as_str) {
        Some("success") => ControlOutcome::Success,
        Some("error") => ControlOutcome::Error(
            response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
                .to_string(),
        ),
        _ => return None,
    };
    Some((request_id, outcome, response.get("response")))
}

/// 运行中切换权限模式。CLI 校验的字段名是 `mode`，读不到即判错误。
pub fn permission_mode_request(request_id: &str, mode: &str) -> Value {
    json!({
        "type": "control_request",
        "request_id": request_id,
        "request": {"subtype": "set_permission_mode", "mode": mode}
    })
}

// ---------------------------------------------------------------------------
// 反向请求兜底
// ---------------------------------------------------------------------------

/// CLI 发给宿主的 control_request 的处置结论。
#[derive(Debug, Clone, PartialEq)]
pub enum ReverseRequest {
    /// 不是反向请求
    NotARequest,
    /// 宿主未实现：立即回错误，CLI 走取消分支
    Reject { subtype: String, response: Value },
    /// 缺 request_id，无法应答，只记日志
    MissingId { subtype: String },
}

/// Monet 不在 stdio 控制通道上实现任何反向请求（权限走 MCP 权限桥），因此全部兜底拒绝。
pub fn classify_reverse_request(value: &Value) -> ReverseRequest {
    if value.get("type").and_then(Value::as_str) != Some("control_request") {
        return ReverseRequest::NotARequest;
    }
    let subtype = value
        .get("request")
        .and_then(|r| r.get("subtype"))
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    match value.get("request_id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => ReverseRequest::Reject {
            response: json!({
                "type": "control_response",
                "response": {
                    "subtype": "error",
                    "request_id": id,
                    "error": UNSUPPORTED_BY_HOST
                }
            }),
            subtype,
        },
        _ => ReverseRequest::MissingId { subtype },
    }
}

// ---------------------------------------------------------------------------
// 限额事件
// ---------------------------------------------------------------------------

/// 中立的额度信号。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RateLimitSignal {
    /// allowed / allowed_warning / rejected
    pub status: String,
    pub rate_limit_type: Option<String>,
    /// 恢复时刻（epoch ms）
    pub resets_at_ms: Option<i64>,
    pub utilization: Option<f64>,
    pub is_using_overage: bool,
}

fn epoch_ms(value: &Value) -> Option<i64> {
    let raw = value
        .as_i64()
        .or_else(|| value.as_f64().map(|f| f as i64))?;
    // CLI 报秒；兼容未来改报毫秒
    Some(if raw < 100_000_000_000 {
        raw * 1000
    } else {
        raw
    })
}

pub fn parse_rate_limit_event(value: &Value) -> Option<RateLimitSignal> {
    if value.get("type")?.as_str()? != "rate_limit_event" {
        return None;
    }
    let info = value.get("rate_limit_info")?;
    let status = info.get("status")?.as_str()?;
    if !matches!(status, "allowed" | "allowed_warning" | "rejected") {
        return None;
    }
    Some(RateLimitSignal {
        status: status.to_string(),
        rate_limit_type: info
            .get("rateLimitType")
            .and_then(Value::as_str)
            .map(String::from),
        resets_at_ms: info.get("resetsAt").and_then(epoch_ms),
        utilization: info.get("utilization").and_then(Value::as_f64),
        is_using_overage: info
            .get("isUsingOverage")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

// ---------------------------------------------------------------------------
// 握手与 system/init
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ReportedCommand {
    pub name: String,
    pub description: String,
    pub argument_hint: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ReportedModel {
    pub value: String,
    pub display_name: String,
    pub description: Option<String>,
}

/// CLI 在握手响应里自报的能力目录。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ReportedCatalog {
    pub commands: Vec<ReportedCommand>,
    pub agents: Vec<String>,
    pub models: Vec<ReportedModel>,
    pub output_style: Option<String>,
    pub available_output_styles: Vec<String>,
    pub cli_version: Option<String>,
}

fn str_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .filter(|s| !s.is_empty())
}

/// 解析 initialize 的响应体（control_response 的 `response.response`）。
pub fn parse_initialize_response(body: &Value) -> Option<ReportedCatalog> {
    let body = body.as_object()?;
    let commands = body
        .get("commands")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(ReportedCommand {
                        name: str_field(item, &["name"])?
                            .trim_start_matches('/')
                            .to_string(),
                        description: str_field(item, &["description"]).unwrap_or("").to_string(),
                        argument_hint: str_field(item, &["argumentHint", "argument_hint"])
                            .map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let agents = body
        .get("agents")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_str()
                        .or_else(|| str_field(item, &["name"]))
                        .map(String::from)
                })
                .collect()
        })
        .unwrap_or_default();
    let models = body
        .get("models")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let value = str_field(item, &["value", "id"])?;
                    Some(ReportedModel {
                        value: value.to_string(),
                        display_name: str_field(item, &["displayName", "display_name"])
                            .unwrap_or(value)
                            .to_string(),
                        description: str_field(item, &["description"]).map(String::from),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let available_output_styles = body
        .get("available_output_styles")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    Some(ReportedCatalog {
        commands,
        agents,
        models,
        output_style: body
            .get("output_style")
            .and_then(Value::as_str)
            .map(String::from),
        available_output_styles,
        cli_version: body
            .get("claude_code_version")
            .and_then(Value::as_str)
            .map(String::from),
    })
}

/// 插件或 MCP server 的加载问题。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InitIssue {
    /// plugin / mcp
    pub source: String,
    pub name: String,
    pub message: String,
}

/// system/init 中与本期相关的字段。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InitReport {
    pub cli_version: Option<String>,
    pub issues: Vec<InitIssue>,
}

pub fn parse_system_init(value: &Value) -> Option<InitReport> {
    if value.get("type")?.as_str()? != "system" || value.get("subtype")?.as_str()? != "init" {
        return None;
    }
    let mut issues = Vec::new();
    let mut collect = |key: &str, source: &str, name_keys: &[&str]| {
        for item in value
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (name, message) = match item {
                Value::String(text) => (String::new(), text.clone()),
                _ => (
                    str_field(item, name_keys).unwrap_or("").to_string(),
                    str_field(item, &["message", "error"])
                        .unwrap_or("failed to load")
                        .to_string(),
                ),
            };
            issues.push(InitIssue {
                source: source.to_string(),
                name,
                message,
            });
        }
    };
    collect("plugin_errors", "plugin", &["plugin", "name"]);
    collect("mcp_server_errors", "mcp", &["server", "name"]);
    for server in value
        .get("mcp_servers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if server.get("status").and_then(Value::as_str) != Some("failed") {
            continue;
        }
        let Some(name) = str_field(server, &["name"]) else {
            continue;
        };
        if issues
            .iter()
            .any(|issue| issue.source == "mcp" && issue.name == name)
        {
            continue;
        }
        issues.push(InitIssue {
            source: "mcp".to_string(),
            name: name.to_string(),
            message: "failed to connect".to_string(),
        });
    }
    Some(InitReport {
        cli_version: value
            .get("claude_code_version")
            .and_then(Value::as_str)
            .map(String::from),
        issues,
    })
}

// ---------------------------------------------------------------------------
// 实时状态白名单
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveSignal {
    Retrying {
        attempt: Option<u64>,
        max_retries: Option<u64>,
    },
    Compacting,
    CompactDone,
    ModelFallback {
        from: Option<String>,
        to: Option<String>,
    },
    PermissionDenied {
        tool: Option<String>,
    },
}

pub fn parse_live_signal(value: &Value) -> Option<LiveSignal> {
    if value.get("type")?.as_str()? != "system" {
        return None;
    }
    // 子 agent 的事件不进主会话状态
    if value.get("agent_id").is_some_and(|v| !v.is_null()) {
        return None;
    }
    let owned = |key: &str| value.get(key).and_then(Value::as_str).map(String::from);
    match value.get("subtype")?.as_str()? {
        "api_retry" => Some(LiveSignal::Retrying {
            attempt: value.get("attempt").and_then(Value::as_u64),
            max_retries: value.get("max_retries").and_then(Value::as_u64),
        }),
        "status" => (value.get("status").and_then(Value::as_str) == Some("compacting"))
            .then_some(LiveSignal::Compacting),
        "compact_boundary" => Some(LiveSignal::CompactDone),
        "model_fallback" => Some(LiveSignal::ModelFallback {
            from: owned("original_model"),
            to: owned("fallback_model"),
        }),
        "model_refusal_fallback" => {
            // scope=local 只影响子 agent / 旁路问答的单次回答，主会话模型未变
            if value.get("scope").and_then(Value::as_str) == Some("local") {
                return None;
            }
            Some(LiveSignal::ModelFallback {
                from: owned("original_model").or_else(|| owned("from_model")),
                to: owned("fallback_model").or_else(|| owned("to_model")),
            })
        }
        "permission_denied" => Some(LiveSignal::PermissionDenied {
            tool: owned("tool_name"),
        }),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 轮次进行中插入输入
// ---------------------------------------------------------------------------

/// 插入消息的 request_id 前缀（撤回用的 cancel_async_message）。
pub const CANCEL_INJECTION_PREFIX: &str = "cancel-inject-";

/// 运行中插入的用户消息。`priority: "next"` 让 CLI 在下一个工具间隙把它并入
/// 当前轮次、不打断执行；`"now"` 会中止正在跑的工具并另起一轮（2.1.306 实测），
/// 不使用。uuid 是 command_lifecycle 回报的关联键。
pub fn mid_turn_user_message(uuid: &str, content: Vec<Value>) -> Value {
    json!({
        "type": "user",
        "session_id": "",
        "uuid": uuid,
        "priority": "next",
        "message": {"role": "user", "content": content},
        "parent_tool_use_id": null
    })
}

/// 撤回尚未被 CLI 处理的插入消息（确认超时后防止迟到的执行与重发重复）。
pub fn cancel_async_message_request(request_id: &str, message_uuid: &str) -> Value {
    json!({
        "type": "control_request",
        "request_id": request_id,
        "request": {"subtype": "cancel_async_message", "message_uuid": message_uuid}
    })
}

/// 排队命令的命运：queued → started → 恰好一个终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandState {
    Queued,
    Started,
    Completed,
    Cancelled,
    Discarded,
    Refused,
}

impl CommandState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Discarded => "discarded",
            Self::Refused => "refused",
        }
    }

    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Queued | Self::Started)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLifecycle {
    pub command_uuid: String,
    pub state: CommandState,
}

pub fn parse_command_lifecycle(value: &Value) -> Option<CommandLifecycle> {
    if value.get("type").and_then(Value::as_str) != Some("command_lifecycle") {
        return None;
    }
    let command_uuid = value
        .get("command_uuid")
        .and_then(Value::as_str)
        .filter(|uuid| !uuid.is_empty())?
        .to_string();
    let state = match value.get("state").and_then(Value::as_str)? {
        "queued" => CommandState::Queued,
        "started" => CommandState::Started,
        "completed" => CommandState::Completed,
        "cancelled" => CommandState::Cancelled,
        "discarded" => CommandState::Discarded,
        "refused" => CommandState::Refused,
        _ => return None,
    };
    Some(CommandLifecycle {
        command_uuid,
        state,
    })
}

// ---------------------------------------------------------------------------
// 未知协议面采样
// ---------------------------------------------------------------------------

/// 事件的结构指纹：只有键名与值类型，不含任何值。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ProtocolShape {
    pub kind: String,
    pub subtype: Option<String>,
    pub keys: BTreeMap<String, String>,
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

pub fn shape_of(value: &Value) -> Option<ProtocolShape> {
    let object = value.as_object()?;
    let kind = object.get("type")?.as_str()?.to_string();
    let subtype = object
        .get("subtype")
        .and_then(Value::as_str)
        .or_else(|| {
            object
                .get("request")
                .and_then(|r| r.get("subtype"))
                .and_then(Value::as_str)
        })
        .map(String::from);
    Some(ProtocolShape {
        kind,
        subtype,
        keys: object
            .iter()
            .map(|(key, value)| (key.clone(), type_name(value).to_string()))
            .collect(),
    })
}

/// Monet 已处理（或明确忽略）的事件；其余进入采样。
pub fn is_known_event(value: &Value) -> bool {
    let kind = value.get("type").and_then(Value::as_str).unwrap_or("");
    let subtype = value.get("subtype").and_then(Value::as_str).unwrap_or("");
    match kind {
        "stream_event" | "assistant" | "user" | "result" | "control_response"
        | "rate_limit_event" | "keep_alive" | "command_lifecycle" => true,
        "system" => matches!(
            subtype,
            "init"
                | "hook_started"
                | "hook_response"
                | "notification"
                | "api_retry"
                | "status"
                | "compact_boundary"
                | "model_fallback"
                | "model_refusal_fallback"
                | "permission_denied"
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_request_uses_mode_field() {
        let request = permission_mode_request("set-perm-mode-2", "acceptEdits");
        assert_eq!(request["request"]["mode"], "acceptEdits");
        assert!(request["request"].get("permission_mode").is_none());
    }

    #[test]
    fn control_response_success_and_error() {
        let ok = json!({"type": "control_response", "response": {"subtype": "success", "request_id": "a", "response": {"x": 1}}});
        let (id, outcome, body) = control_response(&ok).unwrap();
        assert_eq!(id, "a");
        assert_eq!(outcome, ControlOutcome::Success);
        assert_eq!(body.unwrap()["x"], 1);

        let err = json!({"type": "control_response", "response": {"subtype": "error", "request_id": "b", "error": "denied by policy"}});
        let (_, outcome, _) = control_response(&err).unwrap();
        assert_eq!(outcome, ControlOutcome::Error("denied by policy".into()));
    }

    #[test]
    fn control_response_rejects_malformed() {
        assert!(control_response(&json!({"type": "control_response"})).is_none());
        assert!(control_response(
            &json!({"type": "control_response", "response": {"subtype": "success"}})
        )
        .is_none());
        assert!(control_response(&json!({"type": "control_response", "response": {"subtype": "weird", "request_id": "c"}})).is_none());
    }

    #[test]
    fn reverse_requests_are_rejected_with_their_id() {
        let request = json!({"type": "control_request", "request_id": "r1", "request": {"subtype": "elicitation"}});
        match classify_reverse_request(&request) {
            ReverseRequest::Reject { subtype, response } => {
                assert_eq!(subtype, "elicitation");
                assert_eq!(response["response"]["request_id"], "r1");
                assert_eq!(response["response"]["subtype"], "error");
                assert_eq!(response["response"]["error"], UNSUPPORTED_BY_HOST);
            }
            other => panic!("expected reject, got {other:?}"),
        }
    }

    #[test]
    fn reverse_request_without_id_is_not_answered() {
        let request =
            json!({"type": "control_request", "request": {"subtype": "request_user_dialog"}});
        assert_eq!(
            classify_reverse_request(&request),
            ReverseRequest::MissingId {
                subtype: "request_user_dialog".into()
            }
        );
        assert_eq!(
            classify_reverse_request(&json!({"type": "assistant"})),
            ReverseRequest::NotARequest
        );
    }

    #[test]
    fn rate_limit_event_parses_seconds_to_ms() {
        let event = json!({"type": "rate_limit_event", "rate_limit_info": {
            "status": "allowed_warning", "resetsAt": 1_761_750_000, "rateLimitType": "five_hour",
            "utilization": 0.92, "isUsingOverage": true
        }});
        let signal = parse_rate_limit_event(&event).unwrap();
        assert_eq!(signal.status, "allowed_warning");
        assert_eq!(signal.rate_limit_type.as_deref(), Some("five_hour"));
        assert_eq!(signal.resets_at_ms, Some(1_761_750_000_000));
        assert_eq!(signal.utilization, Some(0.92));
        assert!(signal.is_using_overage);
    }

    #[test]
    fn rate_limit_event_without_reset_time_still_parses() {
        let event = json!({"type": "rate_limit_event", "rate_limit_info": {"status": "rejected"}});
        let signal = parse_rate_limit_event(&event).unwrap();
        assert_eq!(signal.resets_at_ms, None);
        assert!(!signal.is_using_overage);
    }

    #[test]
    fn rate_limit_event_rejects_bad_types() {
        assert!(parse_rate_limit_event(
            &json!({"type": "rate_limit_event", "rate_limit_info": {"status": 3}})
        )
        .is_none());
        assert!(parse_rate_limit_event(
            &json!({"type": "rate_limit_event", "rate_limit_info": {"status": "maybe"}})
        )
        .is_none());
        assert!(parse_rate_limit_event(&json!({"type": "rate_limit_event"})).is_none());
    }

    #[test]
    fn initialize_response_is_parsed_leniently() {
        let body = json!({
            "commands": [
                {"name": "review", "description": "Review code", "argumentHint": "[pr]"},
                {"name": "/deploy:staging", "description": ""},
                {"description": "missing name"},
                42
            ],
            "agents": [{"name": "Explore", "description": "search"}, "Plan"],
            "models": [{"value": "opus", "displayName": "Opus", "description": "best"}, {"displayName": "no value"}],
            "output_style": "default",
            "available_output_styles": ["default", "Explanatory", 7],
            "claude_code_version": "2.1.306"
        });
        let catalog = parse_initialize_response(&body).unwrap();
        assert_eq!(catalog.commands.len(), 2);
        assert_eq!(catalog.commands[0].argument_hint.as_deref(), Some("[pr]"));
        assert_eq!(catalog.commands[1].name, "deploy:staging");
        assert_eq!(catalog.agents, vec!["Explore", "Plan"]);
        assert_eq!(catalog.models.len(), 1);
        assert_eq!(catalog.models[0].display_name, "Opus");
        assert_eq!(
            catalog.available_output_styles,
            vec!["default", "Explanatory"]
        );
        assert_eq!(catalog.cli_version.as_deref(), Some("2.1.306"));
    }

    #[test]
    fn initialize_response_with_wrong_shape_is_none_or_empty() {
        assert!(parse_initialize_response(&json!([1, 2])).is_none());
        let catalog = parse_initialize_response(&json!({"commands": "nope"})).unwrap();
        assert!(catalog.commands.is_empty());
        assert!(catalog.models.is_empty());
    }

    #[test]
    fn system_init_collects_plugin_and_mcp_issues() {
        let init = json!({
            "type": "system", "subtype": "init", "claude_code_version": "2.1.312",
            "plugin_errors": [{"plugin": "deploy-kit", "type": "manifest", "message": "bad manifest"}],
            "mcp_servers": [{"name": "github", "status": "failed"}, {"name": "ok", "status": "connected"}]
        });
        let report = parse_system_init(&init).unwrap();
        assert_eq!(report.cli_version.as_deref(), Some("2.1.312"));
        assert_eq!(report.issues.len(), 2);
        assert_eq!(report.issues[0].name, "deploy-kit");
        assert_eq!(report.issues[1].source, "mcp");
        assert_eq!(report.issues[1].name, "github");
    }

    #[test]
    fn system_init_without_issue_fields_is_clean() {
        let init = json!({"type": "system", "subtype": "init", "plugin_errors": "oops"});
        let report = parse_system_init(&init).unwrap();
        assert!(report.issues.is_empty());
        assert!(report.cli_version.is_none());
        assert!(parse_system_init(&json!({"type": "system", "subtype": "status"})).is_none());
    }

    #[test]
    fn live_signals_cover_whitelist() {
        let retry =
            json!({"type": "system", "subtype": "api_retry", "attempt": 2, "max_retries": 10});
        assert_eq!(
            parse_live_signal(&retry),
            Some(LiveSignal::Retrying {
                attempt: Some(2),
                max_retries: Some(10)
            })
        );
        let compacting = json!({"type": "system", "subtype": "status", "status": "compacting"});
        assert_eq!(parse_live_signal(&compacting), Some(LiveSignal::Compacting));
        let idle = json!({"type": "system", "subtype": "status", "status": null});
        assert_eq!(parse_live_signal(&idle), None);
        let fallback = json!({"type": "system", "subtype": "model_fallback", "original_model": "opus", "fallback_model": "sonnet"});
        assert_eq!(
            parse_live_signal(&fallback),
            Some(LiveSignal::ModelFallback {
                from: Some("opus".into()),
                to: Some("sonnet".into())
            })
        );
        let partial = json!({"type": "system", "subtype": "model_fallback", "fallback_model": 5});
        assert_eq!(
            parse_live_signal(&partial),
            Some(LiveSignal::ModelFallback {
                from: None,
                to: None
            })
        );
        let denied = json!({"type": "system", "subtype": "permission_denied", "tool_name": "Bash"});
        assert_eq!(
            parse_live_signal(&denied),
            Some(LiveSignal::PermissionDenied {
                tool: Some("Bash".into())
            })
        );
        let subagent = json!({"type": "system", "subtype": "permission_denied", "tool_name": "Bash", "agent_id": "a1"});
        assert_eq!(parse_live_signal(&subagent), None);
        let local =
            json!({"type": "system", "subtype": "model_refusal_fallback", "scope": "local"});
        assert_eq!(parse_live_signal(&local), None);
    }

    #[test]
    fn mid_turn_message_folds_into_current_turn() {
        let message = mid_turn_user_message("m-1", vec![json!({"type": "text", "text": "hi"})]);
        assert_eq!(message["priority"], "next");
        assert_eq!(message["uuid"], "m-1");
        assert_eq!(message["message"]["content"][0]["text"], "hi");
        let cancel = cancel_async_message_request("cancel-inject-1", "m-1");
        assert_eq!(cancel["request"]["subtype"], "cancel_async_message");
        assert_eq!(cancel["request"]["message_uuid"], "m-1");
    }

    #[test]
    fn command_lifecycle_parses_known_states() {
        let event = json!({"type": "command_lifecycle", "command_uuid": "m-1", "state": "started", "uuid": "x"});
        let parsed = parse_command_lifecycle(&event).unwrap();
        assert_eq!(parsed.command_uuid, "m-1");
        assert_eq!(parsed.state, CommandState::Started);
        assert!(!parsed.state.is_terminal());
        let refused =
            json!({"type": "command_lifecycle", "command_uuid": "m-2", "state": "refused"});
        assert!(parse_command_lifecycle(&refused)
            .unwrap()
            .state
            .is_terminal());
        assert!(is_known_event(&event));
    }

    #[test]
    fn command_lifecycle_rejects_malformed_input() {
        for event in [
            json!({"type": "command_lifecycle", "state": "queued"}),
            json!({"type": "command_lifecycle", "command_uuid": "", "state": "queued"}),
            json!({"type": "command_lifecycle", "command_uuid": "m", "state": "exploded"}),
            json!({"type": "command_lifecycle", "command_uuid": 3, "state": "queued"}),
            json!({"type": "result", "command_uuid": "m", "state": "queued"}),
        ] {
            assert!(parse_command_lifecycle(&event).is_none(), "{event}");
        }
    }

    #[test]
    fn shape_keeps_only_key_types() {
        let event = json!({"type": "system", "subtype": "session_title_changed", "title": "secret", "n": 3});
        let shape = shape_of(&event).unwrap();
        assert_eq!(shape.subtype.as_deref(), Some("session_title_changed"));
        assert_eq!(shape.keys.get("title").map(String::as_str), Some("string"));
        assert_eq!(shape.keys.get("n").map(String::as_str), Some("number"));
        assert!(!serde_json::to_string(&shape).unwrap().contains("secret"));
        assert!(is_known_event(
            &json!({"type": "system", "subtype": "init"})
        ));
        assert!(!is_known_event(&event));
    }
}
