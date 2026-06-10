use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{ContentBlock, TokenUsage};

/// JSONL 行的顶层记录，按 type 字段区分
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionRecord {
    User(UserRecord),
    Assistant(AssistantRecord),
    System(SystemRecord),
    AiTitle(AiTitleRecord),
    QueueOperation(QueueOperationRecord),
    FileHistorySnapshot(FileHistorySnapshotRecord),
    Unknown { raw_type: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub uuid: Option<String>,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub cwd: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    #[serde(rename = "isSidechain")]
    pub is_sidechain: Option<bool>,
    #[serde(rename = "userType")]
    pub user_type: Option<String>,
    #[serde(rename = "permissionMode")]
    pub permission_mode: Option<String>,
    pub message: Option<UserMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMessage {
    pub role: Option<String>,
    pub content: MessageContent,
}

/// message.content 可能是纯字符串或内容块数组
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl MessageContent {
    /// 提取第一个文本内容
    pub fn first_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(s) => Some(s.as_str()),
            MessageContent::Blocks(blocks) => blocks.iter().find_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantRecord {
    pub uuid: Option<String>,
    #[serde(rename = "parentUuid")]
    pub parent_uuid: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub cwd: Option<String>,
    pub version: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    #[serde(rename = "isSidechain")]
    pub is_sidechain: Option<bool>,
    #[serde(rename = "userType")]
    pub user_type: Option<String>,
    pub message: Option<AssistantMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantMessage {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub message_type: Option<String>,
    pub role: Option<String>,
    #[serde(default)]
    pub content: Vec<ContentBlock>,
    pub model: Option<String>,
    pub stop_reason: Option<String>,
    pub usage: Option<TokenUsage>,
}

/// system 类型记录：api_error（API 报错/重试）、compact_boundary（上下文压缩）等子类型
/// 字段取各 subtype 的并集，全部可缺省；error/compactMetadata 保持 Value 灵活透传
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemRecord {
    pub subtype: Option<String>,
    pub content: Option<String>,
    pub level: Option<String>,
    pub timestamp: Option<String>,
    pub uuid: Option<String>,
    pub error: Option<Value>,
    pub compact_metadata: Option<Value>,
    pub retry_attempt: Option<u32>,
    pub max_retries: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiTitleRecord {
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(rename = "aiTitle")]
    pub ai_title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueOperationRecord {
    pub operation: Option<String>,
    pub timestamp: Option<String>,
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileHistorySnapshotRecord {
    #[serde(rename = "messageId")]
    pub message_id: Option<String>,
    #[serde(rename = "isSnapshotUpdate")]
    pub is_snapshot_update: Option<bool>,
    pub snapshot: Option<Value>,
}

/// 手动反序列化 SessionRecord，因为 JSONL 中的 type 字段值与 Rust 枚举变体不一致
impl SessionRecord {
    /// 取所有权版本，避免 Value::clone 开销
    pub fn from_json_owned(value: Value) -> Option<Self> {
        let record_type = value.get("type")?.as_str()?.to_string();
        match record_type.as_str() {
            "user" => serde_json::from_value(value).ok().map(SessionRecord::User),
            "assistant" => serde_json::from_value(value)
                .ok()
                .map(SessionRecord::Assistant),
            "system" => serde_json::from_value(value)
                .ok()
                .map(SessionRecord::System),
            "ai-title" => serde_json::from_value(value)
                .ok()
                .map(SessionRecord::AiTitle),
            "queue-operation" => serde_json::from_value(value)
                .ok()
                .map(SessionRecord::QueueOperation),
            "file-history-snapshot" => serde_json::from_value(value)
                .ok()
                .map(SessionRecord::FileHistorySnapshot),
            other => Some(SessionRecord::Unknown {
                raw_type: other.to_string(),
            }),
        }
    }
}
