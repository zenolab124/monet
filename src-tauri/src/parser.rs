use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::models::*;

/// 轻量结构体：仅提取 assistant 消息的 id/usage，跳过 content 反序列化
#[derive(Deserialize)]
struct UsageExtractor {
    #[serde(rename = "type")]
    record_type: Option<String>,
    message: Option<UsageMessage>,
}

#[derive(Deserialize)]
struct UsageMessage {
    id: Option<String>,
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
        if let Some(mut record) = SessionRecord::from_json_owned(value) {
            // 为每个 image block 注入深度优先序号（ccimg 协议按此 img_index 反查 base64）
            inject_image_indices(&mut record);
            results.push(record);
        }
    }

    results
}

// ============================================================================
// image block 深度优先遍历 —— img_index 的单一权威定义
// ----------------------------------------------------------------------------
// img_index = record 内第 N 个 image block（0 起）。遍历顺序（深度优先）：
//   顶层 message.content 数组按序遍历；遇到 tool_result 且其 content 为 Blocks
//   时，先递归遍历其内嵌 blocks，再继续外层。
//
// 这套顺序是 Rust parser（注入序号）与 ccimg 协议 handler（按序号反查 base64）
// 的共同契约。parser 走 typed 结构注入，handler 走 raw JSON 提取——两条路径必须
// 产出完全一致的序号。计数口径 = 「type == "image" 即计数」：typed 侧靠 ImageSource
// 全字段 default 保证畸形块（缺 media_type / 缺 source）也进 Image 变体不落 Unknown。
// 交叉验证测试：image_protocol::tests::traversal_order_matches_typed_injection。
// ============================================================================

/// 给一条记录内所有 image block 按深度优先序注入 img_index（typed 路径）。
/// 仅 User / Assistant 记录携带 message.content，其余记录无 image，跳过。
fn inject_image_indices(record: &mut SessionRecord) {
    let counter = &mut 0u32;
    match record {
        SessionRecord::User(u) => {
            if let Some(msg) = u.message.as_mut() {
                if let MessageContent::Blocks(blocks) = &mut msg.content {
                    walk_blocks_assign(blocks, counter);
                }
            }
        }
        SessionRecord::Assistant(a) => {
            if let Some(msg) = a.message.as_mut() {
                walk_blocks_assign(&mut msg.content, counter);
            }
        }
        _ => {}
    }
}

/// 深度优先遍历 typed blocks，为遇到的每个 Image 赋递增 img_index。
/// pub(crate)：image_protocol 的交叉验证测试直接调用，确保与 raw 遍历序号一致
pub(crate) fn walk_blocks_assign(blocks: &mut [ContentBlock], counter: &mut u32) {
    for block in blocks.iter_mut() {
        match block {
            ContentBlock::Image { source } => {
                source.img_index = *counter;
                *counter += 1;
            }
            ContentBlock::ToolResult {
                content: ToolResultContent::Blocks(inner),
                ..
            } => {
                walk_blocks_assign(inner, counter);
            }
            _ => {}
        }
    }
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
    let mut custom_title: Option<String> = None;
    let mut first_user_message: Option<String> = None;
    let mut model: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut version: Option<String> = None;
    let mut earliest_timestamp: Option<String> = None;
    let mut total_tokens = TokenUsage::default();
    let mut message_count: u32 = 0;
    let mut context_window: Option<u64> = None;
    // 同一次 API 响应拆多行时每行重复携带相同 usage，按 message.id 去重只计首次
    // （v2.2.0 FR-007；id 缺失的行按行独立计）。set 跨完整/轻量两条路径共享
    let mut seen_usage_ids: HashSet<String> = HashSet::new();

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
                            let first_seen = match msg.get("id").and_then(|i| i.as_str()) {
                                Some(id) => seen_usage_ids.insert(id.to_string()),
                                None => true,
                            };
                            if first_seen {
                                let u: TokenUsage =
                                    serde_json::from_value(usage.clone()).unwrap_or_default();
                                total_tokens.accumulate(&u);
                            }
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
                Some("custom-title") => {
                    custom_title = value
                        .get("customTitle")
                        .and_then(|t| t.as_str())
                        .map(String::from);
                }
                Some("result") => {
                    if let Some(cw) = value
                        .get("modelUsage")
                        .and_then(|u| u.get("contextWindow"))
                        .and_then(|v| v.as_u64())
                    {
                        context_window = Some(cw);
                    }
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

            if line.contains("\"custom-title\"") {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if value.get("type").and_then(|t| t.as_str()) == Some("custom-title") {
                        custom_title = value
                            .get("customTitle")
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
                                    let first_seen = match &msg.id {
                                        Some(id) => seen_usage_ids.insert(id.clone()),
                                        None => true,
                                    };
                                    if first_seen {
                                        total_tokens.accumulate(&u);
                                    }
                                }
                            }
                        }
                    }
                }
                continue;
            }

            if line.contains("\"result\"") && line.contains("\"modelUsage\"") {
                if let Ok(value) = serde_json::from_str::<Value>(&line) {
                    if value.get("type").and_then(|t| t.as_str()) == Some("result") {
                        if let Some(cw) = value
                            .get("modelUsage")
                            .and_then(|u| u.get("contextWindow"))
                            .and_then(|v| v.as_u64())
                        {
                            context_window = Some(cw);
                        }
                    }
                }
                continue;
            }
        }
    }

    // 如果前面没找到 ai-title，在文件尾部搜索
    if title.is_none() && custom_title.is_none() {
        title = search_tail_for_title(path, 4096);
    }

    // 用户手动标题（/title 命令写入的 custom-title）优先于 AI 生成标题
    let title = custom_title.or(title);

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
        context_window,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn assistant_line(id: Option<&str>, output_tokens: u64) -> String {
        let id_part = id.map(|i| format!("\"id\":\"{i}\",")).unwrap_or_default();
        format!(
            "{{\"type\":\"assistant\",\"timestamp\":\"2026-06-11T10:00:00.000Z\",\"message\":{{{id_part}\"model\":\"claude-fable-5\",\"usage\":{{\"input_tokens\":10,\"output_tokens\":{output_tokens},\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0}}}}}}"
        )
    }

    /// FR-007：同一 message.id 多行只计一次 usage；无 id 行按行独立计。
    /// 同时覆盖完整路径（前 max_lines）与轻量路径（之后）共享去重集
    #[test]
    fn summary_dedups_usage_by_message_id() {
        let path = std::env::temp_dir().join("cc-space-test-dedup.jsonl");
        let mut f = fs::File::create(&path).unwrap();
        // 完整路径内：同 id 两行 + 无 id 一行
        writeln!(f, "{}", assistant_line(Some("msg_a"), 100)).unwrap();
        writeln!(f, "{}", assistant_line(Some("msg_a"), 100)).unwrap();
        writeln!(f, "{}", assistant_line(None, 7)).unwrap();
        // 轻量路径内（max_lines=3 之后）：msg_a 第三次出现 + 新 id 一次
        writeln!(f, "{}", assistant_line(Some("msg_a"), 100)).unwrap();
        writeln!(f, "{}", assistant_line(Some("msg_b"), 50)).unwrap();
        drop(f);

        let summary = parse_summary(&path, 3).unwrap();
        // msg_a 计一次(110) + 无 id(17) + msg_b(60)
        assert_eq!(summary.total_tokens.total(), 110 + 17 + 60);
        // message_count 维持按行计数口径不变
        assert_eq!(summary.message_count, 5);
        fs::remove_file(&path).ok();
    }
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

/// 从 JSONL value 中提取第一段用户文本（截断到 200 字符，降低 IPC 载荷）
fn extract_first_text(value: &Value) -> Option<String> {
    let message = value.get("message")?;
    let content = message.get("content")?;

    let raw = if let Some(text) = content.as_str() {
        text.to_string()
    } else if let Some(blocks) = content.as_array() {
        let mut found = None;
        for block in blocks {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                found = block.get("text").and_then(|t| t.as_str()).map(String::from);
                if found.is_some() {
                    break;
                }
            }
        }
        found?
    } else {
        return None;
    };

    Some(truncate_chars(&raw, 200))
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut chars = s.char_indices();
    if chars.nth(max).is_some() {
        let byte_end = s.char_indices().nth(max).unwrap().0;
        format!("{}…", &s[..byte_end])
    } else {
        s.to_string()
    }
}
