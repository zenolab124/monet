use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde_json::Value;

use crate::models::*;

/// 完整解析 JSONL 文件，返回所有会话记录
pub fn parse_all(path: &Path) -> Vec<SessionRecord> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };
    let reader = BufReader::new(file);
    reader
        .lines()
        .filter_map(|line| {
            let line = line.ok()?;
            if line.trim().is_empty() {
                return None;
            }
            let value: Value = serde_json::from_str(&line).ok()?;
            SessionRecord::from_json(&value)
        })
        .collect()
}

/// 懒解析：提取摘要信息，不加载完整对话
/// 读取前 max_lines 行 + 文件尾部搜索 ai-title
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
    let reader = BufReader::new(file);

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
        let value: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let record_type = value.get("type").and_then(|t| t.as_str());

        match record_type {
            Some("user") => {
                message_count += 1;
                // 提取首条用户消息
                if first_user_message.is_none() {
                    first_user_message = extract_first_text(&value);
                }
                // 提取元数据（从最早的记录）
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

        // 前 max_lines 行之后只继续查找 token 累加和 ai-title
        if i >= max_lines
            && title.is_some()
            && first_user_message.is_some()
            && model.is_some()
        {
            // 已有足够信息但仍需累加 token，继续扫描
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
