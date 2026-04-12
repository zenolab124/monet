use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::models::*;

/// 轻量结构体：仅提取 assistant 消息的 usage，跳过 content 反序列化
#[derive(Deserialize)]
struct UsageExtractor {
    #[serde(rename = "type")]
    record_type: Option<String>,
    message: Option<UsageMessage>,
}

#[derive(Deserialize)]
struct UsageMessage {
    usage: Option<TokenUsage>,
}

/// 只解析 user/assistant 消息记录，跳过 file-history-snapshot 等大型记录
/// 避免 Value 中间层，直接反序列化到目标类型
pub fn parse_messages(path: &Path) -> Vec<SessionRecord> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };
    let reader = BufReader::with_capacity(64 * 1024, file);
    let mut results = Vec::new();

    for line in reader.lines() {
        let line = match line {
            Ok(l) if !l.trim().is_empty() => l,
            _ => continue,
        };

        // 快速字符串检测，跳过非消息类型（避免解析巨大的 snapshot 等）
        if line.contains("\"file-history-snapshot\"")
            || line.contains("\"queue-operation\"")
            || line.contains("\"ai-title\"")
        {
            continue;
        }

        // 直接反序列化到目标类型，不经过 Value 中间层
        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(record) = SessionRecord::from_json_owned(value) {
            results.push(record);
        }
    }

    results
}

/// 懒解析：提取摘要信息，不加载完整对话
/// 前 max_lines 行完整解析提取元数据，后续行用轻量结构体仅提取 token usage
pub fn parse_summary(path: &Path, max_lines: usize) -> Option<SessionSummary> {
    let metadata = fs::metadata(path).ok()?;
    let file_size = metadata.len();
    let last_modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs_f64();

    let session_id = path.file_stem()?.to_str()?.to_string();

    let file = File::open(path).ok()?;
    let reader = BufReader::with_capacity(64 * 1024, file);

    let mut title: Option<String> = None;
    let mut first_user_message: Option<String> = None;
    let mut model: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut version: Option<String> = None;
    let mut earliest_timestamp: Option<String> = None;
    let mut total_tokens = TokenUsage::default();
    let mut message_count: u32 = 0;

    for (i, line) in reader.lines().enumerate() {
        let line = match line.ok() {
            Some(l) if !l.trim().is_empty() => l,
            _ => continue,
        };

        if i < max_lines {
            // 前 max_lines 行：完整解析提取所有元数据
            let value: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let record_type = value.get("type").and_then(|t| t.as_str());

            match record_type {
                Some("user") => {
                    message_count += 1;
                    if first_user_message.is_none() {
                        first_user_message = extract_first_text(&value);
                    }
                    if earliest_timestamp.is_none() {
                        earliest_timestamp =
                            value.get("timestamp").and_then(|t| t.as_str()).map(String::from);
                    }
                    if git_branch.is_none() {
                        git_branch = value
                            .get("gitBranch")
                            .and_then(|b| b.as_str())
                            .map(String::from);
                    }
                    if cwd.is_none() {
                        cwd = value.get("cwd").and_then(|c| c.as_str()).map(String::from);
                    }
                    if version.is_none() {
                        version = value
                            .get("version")
                            .and_then(|v| v.as_str())
                            .map(String::from);
                    }
                }
                Some("assistant") => {
                    message_count += 1;
                    if let Some(msg) = value.get("message") {
                        if model.is_none() {
                            model = msg.get("model").and_then(|m| m.as_str()).map(String::from);
                        }
                        if let Some(usage) = msg.get("usage") {
                            let u: TokenUsage =
                                serde_json::from_value(usage.clone()).unwrap_or_default();
                            total_tokens.accumulate(&u);
                        }
                    }
                    if earliest_timestamp.is_none() {
                        earliest_timestamp =
                            value.get("timestamp").and_then(|t| t.as_str()).map(String::from);
                    }
                }
                Some("ai-title") => {
                    title = value
                        .get("aiTitle")
                        .and_then(|t| t.as_str())
                        .map(String::from);
                }
                _ => {}
            }
        } else {
            // 后续行：轻量路径，只提取 token 和计数
            // 快速字符串检测，跳过不相关行
            if line.contains("\"file-history-snapshot\"") || line.contains("\"queue-operation\"") {
                continue;
            }

            if line.contains("\"ai-title\"") {
                // 用轻量解析提取标题
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if value.get("type").and_then(|t| t.as_str()) == Some("ai-title") {
                        title = value
                            .get("aiTitle")
                            .and_then(|t| t.as_str())
                            .map(String::from);
                    }
                }
                continue;
            }

            if line.contains("\"user\"") && !line.contains("\"assistant\"") {
                message_count += 1;
                continue;
            }

            if line.contains("\"assistant\"") {
                message_count += 1;
                // 用轻量结构体只提取 usage，跳过 content 反序列化
                if line.contains("\"usage\"") {
                    if let Ok(ext) = serde_json::from_str::<UsageExtractor>(&line) {
                        if ext.record_type.as_deref() == Some("assistant") {
                            if let Some(msg) = ext.message {
                                if let Some(u) = msg.usage {
                                    total_tokens.accumulate(&u);
                                }
                            }
                        }
                    }
                }
                continue;
            }
        }
    }

    // 如果前面没找到 ai-title，在文件尾部搜索
    if title.is_none() {
        title = search_tail_for_title(path, 4096);
    }

    Some(SessionSummary {
        id: session_id,
        title,
        first_user_message,
        model,
        git_branch,
        cwd,
        version,
        timestamp: earliest_timestamp,
        last_modified,
        total_tokens,
        file_size,
        message_count,
    })
}

/// 从文件尾部搜索 ai-title 记录
fn search_tail_for_title(path: &Path, tail_size: u64) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let file_len = file.metadata().ok()?.len();
    let seek_pos = file_len.saturating_sub(tail_size);
    file.seek(SeekFrom::Start(seek_pos)).ok()?;

    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;

    // 从后往前查找 ai-title
    for line in buf.lines().rev() {
        if line.contains("\"ai-title\"") {
            if let Ok(value) = serde_json::from_str::<Value>(line) {
                if value.get("type").and_then(|t| t.as_str()) == Some("ai-title") {
                    return value
                        .get("aiTitle")
                        .and_then(|t| t.as_str())
                        .map(String::from);
                }
            }
        }
    }
    None
}

/// 从 JSONL value 中提取第一段用户文本
fn extract_first_text(value: &Value) -> Option<String> {
    let message = value.get("message")?;
    let content = message.get("content")?;

    if let Some(text) = content.as_str() {
        return Some(text.to_string());
    }

    if let Some(blocks) = content.as_array() {
        for block in blocks {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    return Some(text.to_string());
                }
            }
        }
    }
    None
}
