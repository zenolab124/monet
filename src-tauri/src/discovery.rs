use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

use crate::cache;
use crate::models::*;

/// Claude 项目数据根目录
fn projects_root() -> PathBuf {
    crate::config::projects_dir()
}

/// 扫描所有项目，返回按最近活跃排序的项目列表
/// 使用 rayon 并行解析 + 内存缓存，大幅降低重复加载开销
pub fn discover_all() -> Vec<Project> {
    let root = projects_root();
    if !root.is_dir() {
        return vec![];
    }

    // 收集所有项目目录（排除内置 Agent 的工作目录，防旧版残留混入档案）
    let agent_dirs = crate::config::agent_project_dirs();
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
            if agent_dirs.contains(&dir_name) {
                return None;
            }
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

    cache::flush();

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
        .filter(|s| !crate::metadata::is_deleted(&s.id))
        .collect();

    // 按 last_modified 降序排序
    sessions.sort_unstable_by(|a, b| {
        b.last_modified
            .partial_cmp(&a.last_modified)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let session_count = sessions.len();
    let last_active = sessions.first().map(|s| s.last_modified);
    // display_path 优先取会话 JSONL 里的真实 cwd(编码回去与目录名一致才采信,
    // 防 worktree 迁移等 cwd 漂移):完全绕开目录名解码的有损还原
    // (`.`/`_`→`-` 不可逆、Windows 盘符形态),解码仅作无会话可用时的兜底
    let display_path = sessions
        .iter()
        .find_map(|s| {
            s.cwd
                .as_deref()
                .filter(|c| cwd_matches_dir(c, dir_name))
                .map(String::from)
        })
        .unwrap_or_else(|| cache::get_decoded_path(dir_name, decode_path));

    Some(Project {
        id: dir_name.to_string(),
        display_path,
        sessions,
        session_count,
        last_active,
    })
}

/// 采信守卫:会话 cwd 编码回去须与目录名一致才认。Windows 下 NTFS 大小写不敏感,
/// 不同大小写的 cwd(如 VS Code 集成终端的小写盘符)落在同一物理目录、目录名保留
/// 首次创建时的大小写,忽略 ASCII 大小写比较;Unix 大小写敏感,保持精确
fn cwd_matches_dir(cwd: &str, dir_name: &str) -> bool {
    let encoded = crate::config::encode_project_dir(Path::new(cwd));
    if cfg!(windows) {
        encoded.eq_ignore_ascii_case(dir_name)
    } else {
        encoded == dir_name
    }
}

/// 项目编码目录名 → 真实路径:优先采信目录下会话 JSONL 的 cwd(经采信守卫),
/// 无可采信会话时退回贪心解码。供 workshop/automation 等跨模块复用,
/// 消灭各处 `replace('-', "/")` 式有损私有副本
pub fn resolve_project_path(dir_name: &str) -> String {
    let dir = projects_root().join(dir_name);
    let trusted = fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with("agent-"))
        })
        .filter_map(|p| cache::get_summary(&p))
        .find_map(|s| s.cwd.filter(|c| cwd_matches_dir(c, dir_name)));
    trusted.unwrap_or_else(|| cache::get_decoded_path(dir_name, decode_path))
}

/// 解码项目目录名为可读路径（仅作 display_path 的无会话兜底，主路径走 JSONL cwd）
/// 编码规则（CLI 侧）：非字母数字一律 → `-`，因此：
/// - Unix：`/Users/alice/foo` → `-Users-alice-foo`（首个 `-` 代表根 `/`）
/// - Windows：`C:\Users\alice` → `C--Users-alice`（盘符 `X:\` → `X--`）
///
/// 使用贪心文件系统验证：优先匹配包含连字符的真实目录名
fn decode_path(encoded: &str) -> String {
    if encoded.is_empty() {
        return String::new();
    }

    // 形态识别:盘符前缀 `X--` 走 Windows 分支;`--` 打头在 Windows 上按 UNC 根还原;
    // 否则按 Unix 根 `-` 处理。探测一律按字节比较——projects 下可能有手工放置的
    // 非 CLI 命名目录(字母 + 多字节 UTF-8),&str 按字节切片会切在 char 边界内 panic
    let bytes = encoded.as_bytes();
    let (root, sep, rest) = if bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b'-'
        && bytes[2] == b'-'
    {
        (format!("{}:\\", &encoded[..1]), '\\', &encoded[3..])
    } else if cfg!(windows) && bytes.len() > 2 && bytes[0] == b'-' && bytes[1] == b'-' {
        // UNC 形态:\\server\share → --server-share,双连字符打头还原为 \\ 根
        (r"\\".to_string(), '\\', &encoded[2..])
    } else if let Some(stripped) = encoded.strip_prefix('-') {
        ("/".to_string(), '/', stripped)
    } else {
        ("/".to_string(), '/', encoded)
    };

    if rest.is_empty() {
        return root;
    }

    // 贪心匹配：从左到右，每个 - 位置尝试是否为真实目录分隔符
    let parts: Vec<&str> = rest.split('-').collect();
    let mut resolved = root;
    let mut i = 0;

    while i < parts.len() {
        // 贪心：尽量多合并连续 parts（处理目录名含 - 的情况）
        let mut best_len = 1;
        for j in (i + 1..=parts.len().min(i + 5)).rev() {
            let candidate: String = parts[i..j].join("-");
            // resolved 仅根自带分隔符,后续段必须补分隔符,否则候选路径粘连、exists 恒假
            let test_path = if resolved.ends_with(sep) {
                format!("{}{}", resolved, candidate)
            } else {
                format!("{}{}{}", resolved, sep, candidate)
            };
            // 合并可延伸到最后一个 part:尾目录名含 - 是最常见场景(如 monet-tauri)
            if Path::new(&test_path).exists() {
                best_len = j - i;
                break;
            }
        }

        let segment: String = parts[i..i + best_len].join("-");
        if !resolved.ends_with(sep) {
            resolved.push(sep);
        }
        resolved.push_str(&segment);
        i += best_len;
    }

    resolved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_unix_form() {
        // 不存在的路径 → 贪心不命中，逐段拆分（双平台结果一致）
        assert_eq!(decode_path("-Users-nobody-alpha"), "/Users/nobody/alpha");
    }

    #[test]
    fn decode_windows_drive_form() {
        // 盘符前缀 X-- → X:\，段间 '\'
        assert_eq!(decode_path("Z--Users-nobody-alpha"), r"Z:\Users\nobody\alpha");
    }

    #[test]
    fn decode_root_only() {
        assert_eq!(decode_path("-"), "/");
        assert_eq!(decode_path("C--"), r"C:\");
    }

    #[test]
    fn decode_non_ascii_dir_no_panic() {
        // projects 下手工放置的非 CLI 命名目录:首字节 ASCII 字母 + 多字节字符,
        // 形态探测不得切在 char 边界内 panic
        assert_eq!(decode_path("D盘备份"), "/D盘备份");
    }

    #[cfg(windows)]
    #[test]
    fn decode_unc_form() {
        // 不存在的 UNC 路径 → 贪心不命中,逐段还原
        assert_eq!(decode_path("--server-share-proj"), r"\\server\share\proj");
    }

    #[cfg(windows)]
    #[test]
    fn decode_roundtrip_hyphenated_dir() {
        // 真实目录往返：含连字符的目录名靠贪心 exists 验证合并还原
        let base = std::env::temp_dir().join(format!("monet-decode-test-{}", std::process::id()));
        let target = base.join("my-hyphen-dir");
        std::fs::create_dir_all(&target).unwrap();
        let encoded = crate::config::encode_project_dir(&target);
        let decoded = decode_path(&encoded);
        assert_eq!(decoded, target.to_string_lossy());
        let _ = std::fs::remove_dir_all(&base);
    }
}
