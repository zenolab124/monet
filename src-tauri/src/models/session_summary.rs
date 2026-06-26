use serde::{Deserialize, Serialize};

use super::TokenUsage;

/// 会话摘要，从 JSONL 文件中懒加载提取
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: Option<String>,
    pub first_user_message: Option<String>,
    pub model: Option<String>,
    pub git_branch: Option<String>,
    pub cwd: Option<String>,
    pub version: Option<String>,
    /// 最早记录的时间戳
    pub timestamp: Option<String>,
    /// 文件最后修改时间（秒级 Unix 时间戳）
    pub last_modified: f64,
    pub total_tokens: TokenUsage,
    /// 文件大小（字节）
    pub file_size: u64,
    /// 用户+助手消息计数
    pub message_count: u32,
    /// API 报告的上下文容量（JSONL result 事件 modelUsage.contextWindow，取最后一条）
    pub context_window: Option<u64>,
}

impl SessionSummary {
    /// 显示标题逻辑：AI 标题 > 首条用户消息（截断 60 字）> 默认
    pub fn display_title(&self) -> String {
        if let Some(title) = &self.title {
            if !title.is_empty() {
                return title.clone();
            }
        }
        if let Some(msg) = &self.first_user_message {
            let cleaned = strip_private_tags(msg);
            if cleaned.chars().count() > 60 {
                return format!("{}…", cleaned.chars().take(60).collect::<String>());
            }
            if !cleaned.is_empty() {
                return cleaned;
            }
        }
        "无标题会话".to_string()
    }
}

/// 去除系统私有标签内容
fn strip_private_tags(text: &str) -> String {
    let tag_names = [
        "ide_opened_file",
        "ide_selection",
        "task-notification",
        "loop-pause",
        "system-reminder",
        "user-prompt-submit-hook",
        "persisted-output",
        "tool_use_error",
        "command-name",
        "command-message",
        "command-args",
        "local-command-caveat",
        "local-command-stdout",
    ];
    let mut result = text.to_string();
    for tag in &tag_names {
        // 移除 <tag>...</tag> 及 <tag .../> 形式
        let pattern = format!(r"<{0}[\s\S]*?</{0}>|<{0}[^>]*/?>", tag);
        if let Ok(re) = regex::Regex::new(&pattern) {
            result = re.replace_all(&result, "").to_string();
        }
    }
    result.trim().to_string()
}
