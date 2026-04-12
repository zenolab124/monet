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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    pub media_type: String,
    /// 只保留前 64 字节前缀，避免传输完整 base64
    #[serde(skip)]
    pub data_prefix: String,
    #[serde(skip)]
    pub data_length: usize,
}

/// 自定义反序列化：截取 image data 前缀，记录长度
impl ImageSource {
    pub fn from_raw(source_type: String, media_type: String, data: &str) -> Self {
        let prefix_len = data.len().min(64);
        Self {
            source_type,
            media_type,
            data_prefix: data[..prefix_len].to_string(),
            data_length: data.len(),
        }
    }
}
