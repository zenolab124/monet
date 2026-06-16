use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 消息内容块，对应 JSONL 中 message.content 数组元素
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    Thinking {
        thinking: String,
        /// Anthropic redacted thinking 的加密签名,thinking 为空但 signature 存在时表示"已加密"。
        /// 流式期间需借此与"思考中"(尚未拿到 delta)区分。
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: ToolResultContent,
        #[serde(default)]
        is_error: bool,
    },
    Image {
        source: ImageSource,
    },
    /// PDF 等文档附件。source.data 的完整 base64 不在字段中声明，
    /// serde 反序列化时直接丢弃——防止大 payload 落进 Unknown(Value) 穿透 IPC
    Document {
        source: DocumentSource,
        #[serde(default)]
        title: Option<String>,
    },
    #[serde(untagged)]
    Unknown(Value),
}

/// tool_result 的 content 字段可能是字符串或内容块数组
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ToolResultContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// document 块的来源描述，刻意不声明 data 字段（base64 原文不进内存/IPC）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    #[serde(default)]
    pub data: String,
}
