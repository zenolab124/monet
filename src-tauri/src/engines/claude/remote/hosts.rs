//! 远端主机配置：来源为 `~/.ssh/config`（只读）与用户手动添加的连接串。
//!
//! Monet 只保存主机别名、启用开关和可选的远端 CLI 路径，不保存任何凭据；
//! 连接时把别名原样交给系统 ssh，由 ssh 自己解析配置、密钥与跳板。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub use super::executor::RemotePlatform;

/// 主机来源：ssh config 中的 Host，或用户手动填写的 `user@host[:port]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostSource {
    SshConfig,
    Manual,
}

/// 持久化的单台主机设置（键为 alias）
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostSettings {
    #[serde(default)]
    pub enabled: bool,
    /// 用户填写的远端 claude 绝对路径；空则由体检自动定位
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_path: Option<String>,
    /// 仅手动添加的主机记录在此；ssh config 主机删掉配置后自然消失
    #[serde(default)]
    pub manual: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct HostStore {
    #[serde(default)]
    pub hosts: BTreeMap<String, HostSettings>,
}

/// 设置页展示用：来源 + 设置合并后的一行
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteHost {
    pub alias: String,
    /// 引擎实例 id（由 alias 派生，见 `instance_id_for_alias`）
    pub instance_id: String,
    pub source: HostSource,
    pub enabled: bool,
    pub claude_path: Option<String>,
}

/// ssh 目标：ssh config 别名直接作为 destination；手动连接串拆出端口
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshTarget {
    pub destination: String,
    pub port: Option<u16>,
    /// 远端系统：决定远端命令的包装方式与脚本方言；由体检识别并记在镜像状态里
    pub platform: RemotePlatform,
}

/// 规范化部分的长度上限：`ssh-` + 主体 + `-` + 6 位短码不超过实例 id 的 64 字节
const INSTANCE_BODY_MAX: usize = 53;

/// FNV-1a 64：标准库哈希不保证跨 Rust 版本稳定，实例 id 需要长期不变
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// alias → 合法的引擎实例 id：`ssh-<规范化 alias>-<6 位短码>`。
/// 规范化后相同的 alias（如 `PC` 与 `pc`）靠基于原名的短码区分；同一 alias 每次结果相同。
pub fn instance_id_for_alias(alias: &str) -> String {
    let mut body = String::new();
    for c in alias.chars().map(|c| c.to_ascii_lowercase()) {
        let mapped = if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' {
            c
        } else {
            '-'
        };
        if mapped == '-' && body.ends_with('-') {
            continue;
        }
        body.push(mapped);
    }
    let body = body.trim_matches('-');
    let body: String = body.chars().take(INSTANCE_BODY_MAX).collect();
    let body = body.trim_end_matches('-');
    let body = if body.is_empty() { "host" } else { body };
    format!("ssh-{}-{:06x}", body, fnv1a64(alias.as_bytes()) & 0xff_ffff)
}

fn store_path() -> PathBuf {
    crate::config::data_dir().join("remote").join("hosts.json")
}

pub fn load_store() -> HostStore {
    std::fs::read_to_string(store_path())
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_store(store: &HostStore) -> Result<(), String> {
    let text = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    crate::config::atomic_write(&store_path(), &text).map_err(|e| e.to_string())
}

/// 手动连接串校验：`user@host` 或 `user@host:port`；拒绝空白、以 `-` 开头（防止被 ssh 当作选项）
pub fn parse_manual_target(input: &str) -> Option<SshTarget> {
    if input.is_empty() || input.starts_with('-') || input.chars().any(char::is_whitespace) {
        return None;
    }
    let re = regex::Regex::new(r"^([A-Za-z0-9._-]+)@([A-Za-z0-9.-]+)(?::(\d{1,5}))?$").ok()?;
    let caps = re.captures(input)?;
    let port = match caps.get(3) {
        Some(port) => Some(port.as_str().parse::<u16>().ok().filter(|p| *p > 0)?),
        None => None,
    };
    Some(SshTarget {
        destination: format!("{}@{}", &caps[1], &caps[2]),
        port,
        platform: RemotePlatform::Posix,
    })
}

/// 别名到 ssh 目标：手动项按连接串解析，其余视为 ssh config 别名
pub fn target_for(alias: &str, manual: bool) -> Option<SshTarget> {
    if manual {
        return parse_manual_target(alias);
    }
    if alias.is_empty() || alias.starts_with('-') || alias.chars().any(char::is_whitespace) {
        return None;
    }
    Some(SshTarget {
        destination: alias.to_string(),
        port: None,
        platform: RemotePlatform::Posix,
    })
}

/// 已配置主机的 ssh 目标，带上镜像状态里记下的远端方言
pub fn target_for_host(host: &RemoteHost) -> Option<SshTarget> {
    let mut target = target_for(&host.alias, host.source == HostSource::Manual)?;
    target.platform = super::sync::known_platform(&host.instance_id);
    Some(target)
}

/// 从 ssh config 文本中取出可直接连接的 Host 别名（跳过通配、否定与 Match 块）。
/// `Include` 只展开不含通配的路径与文件名中的 `*`，相对路径按 `~/.ssh/` 解析。
pub fn ssh_config_aliases(ssh_dir: &Path, text: &str, depth: u8) -> Vec<String> {
    let mut aliases = Vec::new();
    let mut in_match = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.splitn(2, |c: char| c.is_whitespace() || c == '=');
        let key = parts.next().unwrap_or("").to_ascii_lowercase();
        let value = parts.next().unwrap_or("").trim();
        match key.as_str() {
            "match" => in_match = true,
            "host" => {
                in_match = false;
                for pattern in value.split_whitespace() {
                    if !pattern.contains(['*', '?', '!']) {
                        aliases.push(pattern.to_string());
                    }
                }
            }
            "include" if !in_match && depth < 4 => {
                for path in value.split_whitespace() {
                    for file in expand_include(ssh_dir, path) {
                        if let Ok(nested) = std::fs::read_to_string(&file) {
                            aliases.extend(ssh_config_aliases(ssh_dir, &nested, depth + 1));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    aliases
}

fn expand_include(ssh_dir: &Path, raw: &str) -> Vec<PathBuf> {
    let path = if let Some(rest) = raw.strip_prefix("~/") {
        dirs::home_dir()
            .map(|home| home.join(rest))
            .unwrap_or_default()
    } else if Path::new(raw).is_absolute() {
        PathBuf::from(raw)
    } else {
        ssh_dir.join(raw)
    };
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    if !name.contains('*') {
        return vec![path];
    }
    let (prefix, suffix) = name.split_once('*').unwrap_or((name, ""));
    let Some(dir) = path.parent() else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(prefix) && n.ends_with(suffix))
        })
        .collect();
    files.sort();
    files
}

fn read_ssh_config_aliases() -> Vec<String> {
    let Some(ssh_dir) = dirs::home_dir().map(|home| home.join(".ssh")) else {
        return Vec::new();
    };
    std::fs::read_to_string(ssh_dir.join("config"))
        .map(|text| ssh_config_aliases(&ssh_dir, &text, 0))
        .unwrap_or_default()
}

/// 合并 ssh config 主机与已保存设置；ssh config 中已不存在的非手动项不再列出
pub fn merge_hosts(config_aliases: &[String], store: &HostStore) -> Vec<RemoteHost> {
    let mut seen = std::collections::HashSet::new();
    let mut hosts = Vec::new();
    for alias in config_aliases {
        if !seen.insert(alias.clone()) {
            continue;
        }
        let settings = store.hosts.get(alias).cloned().unwrap_or_default();
        hosts.push(RemoteHost {
            alias: alias.clone(),
            instance_id: instance_id_for_alias(alias),
            source: HostSource::SshConfig,
            enabled: settings.enabled,
            claude_path: settings.claude_path,
        });
    }
    for (alias, settings) in &store.hosts {
        if settings.manual && seen.insert(alias.clone()) {
            hosts.push(RemoteHost {
                alias: alias.clone(),
                instance_id: instance_id_for_alias(alias),
                source: HostSource::Manual,
                enabled: settings.enabled,
                claude_path: settings.claude_path.clone(),
            });
        }
    }
    hosts
}

pub fn list_hosts() -> Vec<RemoteHost> {
    merge_hosts(&read_ssh_config_aliases(), &load_store())
}

/// 远端 claude 路径只接受绝对路径且不含空白与引号，避免拼进远端命令时被解释
pub fn valid_claude_path(path: &str) -> bool {
    path.starts_with('/')
        && !path
            .chars()
            .any(|c| c.is_whitespace() || c == '\'' || c == '"')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_target_accepts_user_host_and_port() {
        assert_eq!(
            parse_manual_target("alice@example.com:2222"),
            Some(SshTarget {
                destination: "alice@example.com".into(),
                port: Some(2222),
                platform: RemotePlatform::Posix,
            })
        );
        assert_eq!(
            parse_manual_target("alice@10.0.0.5"),
            Some(SshTarget {
                destination: "alice@10.0.0.5".into(),
                port: None,
                platform: RemotePlatform::Posix,
            })
        );
    }

    #[test]
    fn manual_target_rejects_options_whitespace_and_bad_ports() {
        for input in [
            "",
            "-oProxyCommand=x@h",
            "alice@host name",
            "alice@host\n",
            "host",
            "alice@host:0",
            "alice@host:70000",
            "a;b@host",
        ] {
            assert_eq!(parse_manual_target(input), None, "{input:?}");
        }
    }

    #[test]
    fn config_aliases_skip_wildcards_negations_and_match_blocks() {
        let text = "Host devbox 100\n  HostName 10.0.0.2\nHost *.internal\nHost !bad ok\nMatch host foo\n  User x\nHost=alias-eq\n# Host commented\n";
        let dir = std::env::temp_dir();
        assert_eq!(
            ssh_config_aliases(&dir, text, 0),
            vec!["devbox", "100", "ok", "alias-eq"]
        );
    }

    #[test]
    fn config_include_expands_star_in_file_name() {
        let dir = std::env::temp_dir().join(format!("monet-ssh-include-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("conf.d")).unwrap();
        std::fs::write(dir.join("conf.d/a.conf"), "Host inc-a\n").unwrap();
        std::fs::write(dir.join("conf.d/b.conf"), "Host inc-b\n").unwrap();
        std::fs::write(dir.join("conf.d/skip.txt"), "Host nope\n").unwrap();
        let aliases = ssh_config_aliases(&dir, "Include conf.d/*.conf\nHost top\n", 0);
        assert_eq!(aliases, vec!["inc-a", "inc-b", "top"]);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn merge_keeps_manual_hosts_and_drops_stale_config_settings() {
        let mut store = HostStore::default();
        store.hosts.insert(
            "devbox".into(),
            HostSettings {
                enabled: true,
                claude_path: None,
                manual: false,
            },
        );
        store.hosts.insert(
            "gone".into(),
            HostSettings {
                enabled: true,
                claude_path: None,
                manual: false,
            },
        );
        store.hosts.insert(
            "bob@box".into(),
            HostSettings {
                enabled: false,
                claude_path: Some("/opt/claude".into()),
                manual: true,
            },
        );
        let hosts = merge_hosts(&["devbox".into(), "devbox".into(), "new".into()], &store);
        let summary: Vec<_> = hosts
            .iter()
            .map(|h| (h.alias.as_str(), h.source, h.enabled))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("devbox", HostSource::SshConfig, true),
                ("new", HostSource::SshConfig, false),
                ("bob@box", HostSource::Manual, false),
            ]
        );
    }

    #[test]
    fn instance_ids_are_valid_stable_and_distinct() {
        let aliases = [
            "PC",
            "pc",
            "my_host",
            "my-host",
            "alice@example.com:2222",
            "--",
            "主机",
            &"x".repeat(90),
        ];
        let ids: Vec<String> = aliases.iter().map(|a| instance_id_for_alias(a)).collect();
        for (alias, id) in aliases.iter().zip(&ids) {
            assert!(
                crate::engines::core::EngineInstanceId::new("claude-code", id.as_str()).is_ok(),
                "{alias} -> {id}"
            );
            assert_eq!(&instance_id_for_alias(alias), id, "stable for {alias}");
        }
        let unique: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        assert!(ids[0].starts_with("ssh-pc-") && ids[1].starts_with("ssh-pc-"));
        assert!(ids[4].starts_with("ssh-alice-example.com-2222-"));
        assert!(ids[5].starts_with("ssh-host-") && ids[6].starts_with("ssh-host-"));
    }

    #[test]
    fn instance_id_is_pinned_for_a_known_alias() {
        // 编码规则一旦发布就不能变：变了会让已有主机的会话元数据失联
        assert_eq!(
            instance_id_for_alias("devbox"),
            format!("ssh-devbox-{:06x}", fnv1a64(b"devbox") & 0xff_ffff)
        );
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn claude_path_must_be_absolute_without_quotes_or_spaces() {
        assert!(valid_claude_path("/Users/alice/.local/bin/claude"));
        for path in ["claude", "/a b/claude", "/a'/claude", "~/claude"] {
            assert!(!valid_claude_path(path), "{path}");
        }
    }
}
