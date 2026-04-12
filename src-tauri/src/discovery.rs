use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::cache;
use crate::models::*;

/// Claude 项目数据根目录
fn projects_root() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}

/// 扫描所有项目，返回按最近活跃排序的项目列表
/// 使用 rayon 并行解析 + 内存缓存，大幅降低重复加载开销
pub fn discover_all() -> Vec<Project> {
    let root = match projects_root() {
        Some(r) if r.is_dir() => r,
        _ => return vec![],
    };

    // 收集所有项目目录
    let entries: Vec<(PathBuf, String)> = fs::read_dir(&root)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            if !entry.file_type().ok()?.is_dir() {
                return None;
            }
            let dir_name = entry.file_name().to_str()?.to_string();
            Some((entry.path(), dir_name))
        })
        .collect();

    // 并行解析所有项目
    let mut projects: Vec<Project> = entries
        .par_iter()
        .filter_map(|(path, name)| discover_project(path, name))
        .filter(|p| !p.sessions.is_empty())
        .collect();

    // 按最近活跃排序（降序）
    projects.sort_unstable_by(|a, b| {
        b.last_active
            .unwrap_or(0.0)
            .partial_cmp(&a.last_active.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    projects
}

/// 扫描单个项目目录，使用缓存避免重复解析
fn discover_project(dir: &Path, dir_name: &str) -> Option<Project> {
    // 收集 .jsonl 文件路径
    let session_paths: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension()?.to_str()? != "jsonl" {
                return None;
            }
            let file_name = path.file_name()?.to_str()?;
            if file_name.starts_with("agent-") {
                return None;
            }
            if !entry.file_type().ok()?.is_file() {
                return None;
            }
            Some(path)
        })
        .collect();

    // 并行解析会话（通过缓存层，未变化的文件直接返回）
    let mut sessions: Vec<SessionSummary> = session_paths
        .par_iter()
        .filter_map(|path| cache::get_summary(path))
        .collect();

    // 按 last_modified 降序排序
    sessions.sort_unstable_by(|a, b| {
        b.last_modified
            .partial_cmp(&a.last_modified)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let session_count = sessions.len();
    let last_active = sessions.first().map(|s| s.last_modified);
    let display_path = cache::get_decoded_path(dir_name, decode_path);

    Some(Project {
        id: dir_name.to_string(),
        display_path,
        sessions,
        session_count,
        last_active,
    })
}

/// 解码项目目录名为可读路径
/// 编码规则：/ → -，首个 - 代表根 /
/// 例：-Users-xt-workspace → /Users/xt/workspace
///
/// 使用贪心文件系统验证：优先匹配包含连字符的真实目录名
fn decode_path(encoded: &str) -> String {
    if encoded.is_empty() {
        return String::new();
    }

    // 去掉开头的 - (代表根 /)
    let rest = if encoded.starts_with('-') {
        &encoded[1..]
    } else {
        encoded
    };

    if rest.is_empty() {
        return "/".to_string();
    }

    // 贪心匹配：从左到右，每个 - 位置尝试是否为真实目录分隔符
    let parts: Vec<&str> = rest.split('-').collect();
    let mut resolved = String::from("/");
    let mut i = 0;

    while i < parts.len() {
        // 贪心：尽量多合并连续 parts（处理目录名含 - 的情况）
        let mut best_len = 1;
        for j in (i + 1..=parts.len().min(i + 5)).rev() {
            let candidate: String = parts[i..j].join("-");
            let test_path = format!("{}{}", resolved, candidate);
            if Path::new(&test_path).exists() && j < parts.len() {
                best_len = j - i;
                break;
            }
        }

        let segment: String = parts[i..i + best_len].join("-");
        if i > 0 {
            resolved.push('/');
        }
        resolved.push_str(&segment);
        i += best_len;
    }

    resolved
}
