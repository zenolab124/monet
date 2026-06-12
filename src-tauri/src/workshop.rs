//! 工坊域资产扫描（v2.3.0）：Skills / Commands / Subagents / MCP 四类资产的只读数据层。
//!
//! - `get_workshop_assets`：全局 + 项目级标准路径全量扫描，一次返回四类清单（FR-001）
//! - `probe_mcp_server`：http/sse 类型 MCP 在线探测（FR-005）
//! - `open_workshop_dir`：系统文件管理器打开类别全局目录（FR-006）
//!
//! 扫描核心是接受根路径参数的纯函数，便于单测注入 fixture 目录；
//! command 层只负责拼装家目录与 discovery 的项目列表。

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// source 标签：全局资产
const GLOBAL_SOURCE: &str = "全局";
/// frontmatter 解析失败的容错文案（PRD FR-001 规则 6，全角括号）
const FRONTMATTER_FAILED: &str = "（frontmatter 解析失败）";
/// commands 递归扫描深度上限：防 symlink 环
const MAX_COMMAND_DEPTH: usize = 10;

// ---------- 返回结构（PRD WorkshopAssets 接口，serde 输出 camelCase） ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillAsset {
    pub name: String,
    pub description: String,
    pub version: Option<String>,
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandAsset {
    pub name: String,
    pub description: String,
    pub argument_hint: Option<String>,
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAsset {
    pub name: String,
    pub description: String,
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerAsset {
    pub name: String,
    /// 'stdio' | 'http' | 'sse'（配置显式 type 原样透传）
    pub transport: String,
    /// stdio 的 command 主体（不拼 args）或 http/sse 的 url；敏感值不进渲染层
    pub endpoint: String,
    pub enabled: bool,
    pub source: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopAssets {
    pub skills: Vec<SkillAsset>,
    pub commands: Vec<CommandAsset>,
    pub agents: Vec<AgentAsset>,
    pub mcp_servers: Vec<McpServerAsset>,
}

// ---------- frontmatter 局部结构 ----------

/// SKILL.md frontmatter：只声明消费的字段，其余不进内存
#[derive(Debug, Default, Deserialize)]
struct SkillFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    metadata: Option<SkillMetadata>,
}

/// metadata 嵌套可整体缺失；version 兼容字符串与数字写法（如 `version: 1.2`）
#[derive(Debug, Default, Deserialize)]
struct SkillMetadata {
    #[serde(default)]
    version: Option<serde_yaml::Value>,
}

/// command .md frontmatter；kebab 字段必须显式 rename（serde 不支持 kebab 自动转换）
#[derive(Debug, Default, Deserialize)]
struct CommandFrontmatter {
    #[serde(default)]
    description: Option<String>,
    #[serde(default, rename = "argument-hint")]
    argument_hint: Option<serde_yaml::Value>,
}

/// subagent .md frontmatter
#[derive(Debug, Default, Deserialize)]
struct AgentFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

/// frontmatter 解析结果：失败不静默吞掉，落到列表项的可见文案上
enum Frontmatter<T> {
    /// 解析成功（含「无 frontmatter / 空 frontmatter」→ 全字段缺省）
    Parsed(T),
    /// YAML 非法或文件不可读
    Failed,
}

// ---------- MCP 配置局部结构 ----------

/// ~/.claude.json 局部反序列化：只声明 mcpServers 与 projects.<cwd>.{mcpServers,disabledMcpjsonServers}，
/// env/args/headers 等敏感字段一律不声明（serde 不声明即不进内存，PRD 规则 7）。
/// mcpServers 值留 Value 逐项解析（见 entry_transport_endpoint）：单个条目类型不符
/// 不拖垮整文件——否则用户级 MCP 全部计空、各项目 disabled 列表一并丢失
#[derive(Debug, Default, Deserialize)]
struct ClaudeJsonPartial {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: HashMap<String, serde_json::Value>,
    #[serde(default)]
    projects: HashMap<String, ClaudeJsonProject>,
}

/// projects.<cwd> 条目：mcpServers 为 local scope MCP（`claude mcp add` 默认写入位置），
/// disabledMcpjsonServers 只作用于该项目 .mcp.json 声明的服务器（语义勿混）
#[derive(Debug, Default, Deserialize)]
struct ClaudeJsonProject {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: HashMap<String, serde_json::Value>,
    #[serde(default, rename = "disabledMcpjsonServers")]
    disabled_mcpjson_servers: Vec<String>,
}

/// 单个 MCP 服务器配置：只取 transport 推断所需的三个字段
#[derive(Debug, Default, Deserialize)]
struct McpServerConfig {
    #[serde(default, rename = "type")]
    server_type: Option<String>,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

/// <cwd>/.mcp.json 局部结构；值同样留 Value 逐项容错
#[derive(Debug, Default, Deserialize)]
struct McpJsonFile {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: HashMap<String, serde_json::Value>,
}

// ---------- Tauri commands ----------

/// 工坊四类资产全量扫描（FR-001）。仅家目录不可访问时返回 Err。
#[tauri::command]
pub async fn get_workshop_assets() -> Result<WorkshopAssets, String> {
    tauri::async_runtime::spawn_blocking(collect_workshop_assets)
        .await
        .map_err(|e| e.to_string())?
}

/// 探活共享 HTTP client：前端对 N 个服务器并发 invoke，每次新建 Client 会重复构建
/// 连接池与 TLS 上下文，OnceLock 持有单例复用；构建失败（罕见）缓存 Err 原样上抛
fn probe_client() -> Result<&'static reqwest::Client, String> {
    static CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| e.clone())
}

/// MCP http/sse 在线探测（FR-005）：GET 收到任意 HTTP 响应（含 4xx/5xx）= true；
/// 连接拒绝 / 超时（3 秒）/ DNS 失败 = false。不做任何 stdio spawn。
#[tauri::command]
pub async fn probe_mcp_server(url: String) -> Result<bool, String> {
    let client = probe_client()?;
    Ok(client.get(&url).send().await.is_ok())
}

/// 系统文件管理器打开类别全局目录（FR-006）。
/// skills/commands/agents 目录不存在先创建再打开；mcp 打开 ~/.claude.json 所在目录（家目录，不创建）。
#[tauri::command]
pub fn open_workshop_dir(category: String) -> Result<(), String> {
    let home = dirs::home_dir().ok_or_else(|| "家目录不可访问".to_string())?;
    let dir = match category.as_str() {
        "skills" | "commands" | "agents" => {
            let d = home.join(".claude").join(&category);
            fs::create_dir_all(&d).map_err(|e| format!("创建目录失败: {}", e))?;
            d
        }
        "mcp" => home,
        other => return Err(format!("未知类别: {}", other)),
    };
    open_in_file_manager(&dir)
}

/// 按平台调用系统文件管理器；spawn 后不等待退出码
/// （Windows explorer 成功也常返回非零退出码，等待校验会误报）
fn open_in_file_manager(dir: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let opener = "open";
    #[cfg(target_os = "windows")]
    let opener = "explorer";
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let opener = "xdg-open";

    std::process::Command::new(opener)
        .arg(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("打开目录失败: {}", e))
}

// ---------- 装配层 ----------

/// command 层装配：拼家目录与 discovery 项目列表，再交给纯函数
fn collect_workshop_assets() -> Result<WorkshopAssets, String> {
    let home = dirs::home_dir().ok_or_else(|| "家目录不可访问".to_string())?;
    // 项目级遍历复用 discovery 的发现结果（display_path = 解码后的项目 cwd）；
    // 解码可能指向已不存在的目录，扫描时静默跳过是常态
    let mut seen = HashSet::new();
    let project_cwds: Vec<String> = crate::discovery::discover_all()
        .into_iter()
        .map(|p| p.display_path)
        .filter(|p| !p.is_empty() && seen.insert(p.clone()))
        .collect();
    Ok(collect_assets(&home, &project_cwds))
}

/// 纯函数扫描核心：home = 家目录根，project_cwds = 项目工作目录列表（单测注入 fixture）
fn collect_assets(home: &Path, project_cwds: &[String]) -> WorkshopAssets {
    let claude_dir = home.join(".claude");
    let mut skills = scan_skills_dir(&claude_dir.join("skills"), GLOBAL_SOURCE);
    let mut commands = scan_commands_dir(&claude_dir.join("commands"), GLOBAL_SOURCE);
    let mut agents = scan_agents_dir(&claude_dir.join("agents"), GLOBAL_SOURCE);

    // ~/.claude.json：用户级 MCP + 各项目 local scope MCP 与 disabled 列表。
    // 缺失或解析失败 → 用户级与 local scope MCP 计为空，项目级 .mcp.json 照常解析（FR-004 降级）
    let claude_json_path = home.join(".claude.json");
    let claude_json: Option<ClaudeJsonPartial> = fs::read_to_string(&claude_json_path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok());

    let mut mcp_servers: Vec<McpServerAsset> = Vec::new();
    if let Some(cj) = &claude_json {
        for (name, raw) in &cj.mcp_servers {
            let (transport, endpoint) = entry_transport_endpoint(raw);
            mcp_servers.push(McpServerAsset {
                name: name.clone(),
                transport,
                endpoint,
                // 全局服务器恒启用
                enabled: true,
                source: GLOBAL_SOURCE.to_string(),
                path: claude_json_path.display().to_string(),
            });
        }
    }

    for cwd in project_cwds {
        let source = project_source_name(cwd);
        let proj_claude = Path::new(cwd).join(".claude");
        skills.extend(scan_skills_dir(&proj_claude.join("skills"), &source));
        commands.extend(scan_commands_dir(&proj_claude.join("commands"), &source));
        agents.extend(scan_agents_dir(&proj_claude.join("agents"), &source));

        let project_entry = claude_json.as_ref().and_then(|cj| cj.projects.get(cwd));

        // 项目级 MCP 来源一：local scope（~/.claude.json 的 projects.<cwd>.mcpServers，
        // `claude mcp add` 默认写入位置）。local scope 无禁用机制 → enabled 恒 true；
        // 与 .mcp.json 同名不合并、各自成行（path 不同，前端探活 key 不冲突）
        if let Some(proj) = project_entry {
            for (name, raw) in &proj.mcp_servers {
                let (transport, endpoint) = entry_transport_endpoint(raw);
                mcp_servers.push(McpServerAsset {
                    name: name.clone(),
                    transport,
                    endpoint,
                    enabled: true,
                    source: source.clone(),
                    path: claude_json_path.display().to_string(),
                });
            }
        }

        // 项目级 MCP 来源二：.mcp.json，enabled = 服务器名不在该 cwd entry 的 disabledMcpjsonServers 中
        let disabled: &[String] = project_entry
            .map(|p| p.disabled_mcpjson_servers.as_slice())
            .unwrap_or(&[]);
        mcp_servers.extend(scan_mcp_json(&Path::new(cwd).join(".mcp.json"), &source, disabled));
    }

    // 排序：先「全局」后项目级（项目间按 source 字典序），同组内按 name 字典序
    sort_assets(&mut skills, |s| (&s.source, &s.name));
    sort_assets(&mut commands, |c| (&c.source, &c.name));
    sort_assets(&mut agents, |a| (&a.source, &a.name));
    sort_assets(&mut mcp_servers, |m| (&m.source, &m.name));

    WorkshopAssets {
        skills,
        commands,
        agents,
        mcp_servers,
    }
}

/// 统一排序：全局组在前，组间按 source、组内按 name 字典序
fn sort_assets<T>(items: &mut [T], key: impl Fn(&T) -> (&str, &str)) {
    items.sort_unstable_by(|a, b| {
        let (sa, na) = key(a);
        let (sb, nb) = key(b);
        (sa != GLOBAL_SOURCE)
            .cmp(&(sb != GLOBAL_SOURCE))
            .then_with(|| sa.cmp(sb))
            .then_with(|| na.cmp(nb))
    });
}

/// source 标签：cwd 最后一段（目录名）；取不到时退回整串
fn project_source_name(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| cwd.to_string())
}

// ---------- 四类扫描（纯函数，目录不存在一律静默返回空） ----------

/// Skills：<dir>/*/SKILL.md。symlink 目录必须跟随——
/// 判定用 path().is_dir()（自动跟随），不能用 DirEntry::file_type()（symlink 返回 symlink 而非 dir）
fn scan_skills_dir(dir: &Path, source: &str) -> Vec<SkillAsset> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let skill_md = entry_path.join("SKILL.md");
        if !skill_md.is_file() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().into_owned();
        let (name, description, version) = match read_frontmatter::<SkillFrontmatter>(&skill_md) {
            Frontmatter::Parsed(fm) => (
                fm.name.unwrap_or_else(|| dir_name.clone()),
                fm.description.unwrap_or_default(),
                fm.metadata
                    .and_then(|m| m.version.as_ref().and_then(yaml_scalar_to_string)),
            ),
            Frontmatter::Failed => (dir_name.clone(), FRONTMATTER_FAILED.to_string(), None),
        };
        out.push(SkillAsset {
            name,
            description,
            version,
            source: source.to_string(),
            path: skill_md.display().to_string(),
        });
    }
    out
}

/// Commands：递归 <dir>/**/*.md，name = 相对路径去 .md，子目录构成命名空间（分隔符统一 '/'）
fn scan_commands_dir(dir: &Path, source: &str) -> Vec<CommandAsset> {
    let mut files = Vec::new();
    walk_md_files(dir, 0, &mut files);
    let mut out = Vec::new();
    for file in files {
        let name = command_name(dir, &file);
        let (description, argument_hint) = match read_frontmatter::<CommandFrontmatter>(&file) {
            Frontmatter::Parsed(fm) => (
                fm.description.unwrap_or_default(),
                fm.argument_hint.as_ref().and_then(yaml_scalar_to_string),
            ),
            Frontmatter::Failed => (FRONTMATTER_FAILED.to_string(), None),
        };
        out.push(CommandAsset {
            name,
            description,
            argument_hint,
            source: source.to_string(),
            path: file.display().to_string(),
        });
    }
    out
}

/// Subagents：<dir>/*.md（单层），name = frontmatter name 缺失用文件名（去 .md）
fn scan_agents_dir(dir: &Path, source: &str) -> Vec<AgentAsset> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let file_stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (name, description) = match read_frontmatter::<AgentFrontmatter>(&path) {
            Frontmatter::Parsed(fm) => (
                fm.name.unwrap_or_else(|| file_stem.clone()),
                fm.description.unwrap_or_default(),
            ),
            Frontmatter::Failed => (file_stem.clone(), FRONTMATTER_FAILED.to_string()),
        };
        out.push(AgentAsset {
            name,
            description,
            source: source.to_string(),
            path: path.display().to_string(),
        });
    }
    out
}

/// 项目级 .mcp.json 扫描；disabled = 该项目在 ~/.claude.json 的 disabledMcpjsonServers
fn scan_mcp_json(path: &Path, source: &str, disabled: &[String]) -> Vec<McpServerAsset> {
    let Ok(content) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(parsed) = serde_json::from_str::<McpJsonFile>(&content) else {
        // 整文件 JSON 语法非法拿不到任何服务器名，无从按项容错，计为空
        // （单个条目类型不符的容错在 entry_transport_endpoint 逐项处理）
        return Vec::new();
    };
    parsed
        .mcp_servers
        .into_iter()
        .map(|(name, raw)| {
            let (transport, endpoint) = entry_transport_endpoint(&raw);
            let enabled = !disabled.contains(&name);
            McpServerAsset {
                name,
                transport,
                endpoint,
                enabled,
                source: source.to_string(),
                path: path.display().to_string(),
            }
        })
        .collect()
}

/// 单条 mcpServers 条目的容错解析（容错精神同 frontmatter 规则 6——单项坏不拖垮整表）：
/// 值为对象 → 正常 transport 推断；类型不符（字符串/null 等）→ ("unknown", "")，
/// 名字保留在列表中。前端对 http/sse 之外的 transport 不探活、显示「<transport> · 未探活」
fn entry_transport_endpoint(raw: &serde_json::Value) -> (String, String) {
    match McpServerConfig::deserialize(raw) {
        Ok(cfg) => infer_transport(&cfg),
        Err(_) => ("unknown".to_string(), String::new()),
    }
}

/// transport 推断：显式 type 优先；缺 type 有 command → stdio；有 url 无 type → http。
/// endpoint = stdio 的 command 主体（不拼 args）或 http/sse 的 url
fn infer_transport(cfg: &McpServerConfig) -> (String, String) {
    let transport = match cfg.server_type.as_deref() {
        Some(t) if !t.is_empty() => t.to_string(),
        _ if cfg.command.is_some() => "stdio".to_string(),
        _ if cfg.url.is_some() => "http".to_string(),
        _ => "stdio".to_string(),
    };
    let endpoint = if transport == "stdio" {
        cfg.command.clone().unwrap_or_default()
    } else {
        cfg.url.clone().unwrap_or_default()
    };
    (transport, endpoint)
}

// ---------- 工具函数 ----------

/// 递归收集 .md 文件；depth 防 symlink 环，is_dir/is_file 走 path 判定自动跟随 symlink
fn walk_md_files(dir: &Path, depth: usize, files: &mut Vec<PathBuf>) {
    if depth > MAX_COMMAND_DEPTH {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else {
        return;
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_md_files(&path, depth + 1, files);
        } else if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
            files.push(path);
        }
    }
}

/// command 名：相对 commands 根目录的路径去 .md，分隔符统一为 '/'（Windows 反斜杠归一）
fn command_name(root: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file);
    let mut parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if let Some(last) = parts.last_mut() {
        if let Some(stripped) = last.strip_suffix(".md") {
            *last = stripped.to_string();
        }
    }
    parts.join("/")
}

/// 读文件并解析 frontmatter；文件不可读视同解析失败（容错落到可见列表项，不静默吞）
fn read_frontmatter<T: DeserializeOwned + Default>(path: &Path) -> Frontmatter<T> {
    let Ok(content) = fs::read_to_string(path) else {
        return Frontmatter::Failed;
    };
    parse_frontmatter(&content)
}

/// 解析文件头 `---\n...\n---` 之间的 YAML；
/// 无 frontmatter / 空 frontmatter → 全字段缺省；YAML 非法 → Failed
fn parse_frontmatter<T: DeserializeOwned + Default>(content: &str) -> Frontmatter<T> {
    match extract_frontmatter(content) {
        None => Frontmatter::Parsed(T::default()),
        Some(text) if text.trim().is_empty() => Frontmatter::Parsed(T::default()),
        Some(text) => match serde_yaml::from_str::<T>(text) {
            Ok(v) => Frontmatter::Parsed(v),
            Err(_) => Frontmatter::Failed,
        },
    }
}

/// 提取 frontmatter 文本：首行必须是 `---`，到下一个独立 `---` 行（含 EOF 处无换行）为止
fn extract_frontmatter(content: &str) -> Option<&str> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let first_line_end = content.find('\n')?;
    if content[..first_line_end].trim_end() != "---" {
        return None;
    }
    let body = &content[first_line_end + 1..];
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Some(&body[..offset]);
        }
        offset += line.len();
    }
    None
}

/// YAML 标量转字符串：字符串/数字取字面值（兼容 `version: 1.2` 数字写法），其余类型视为缺失
fn yaml_scalar_to_string(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

// ---------- 单元测试 ----------

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 fixture：temp_dir + 进程 id + 用例名隔离，Drop 时清理
    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(case: &str) -> Self {
            let root = std::env::temp_dir().join(format!(
                "cc-space-workshop-test-{}-{}",
                std::process::id(),
                case
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).unwrap();
            Fixture { root }
        }

        fn write(&self, rel: &str, content: &str) -> PathBuf {
            let path = self.root.join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn skills_normal_parse() {
        let fx = Fixture::new("skills-normal");
        fx.write(
            "skills/alpha/SKILL.md",
            "---\nname: alpha\ndescription: 第一个\nmetadata:\n  version: \"1.0.0\"\n---\n# alpha\n",
        );
        fx.write(
            "skills/beta/SKILL.md",
            "---\ndescription: 无 name 用目录名\nmetadata:\n  version: 1.2\n---\n",
        );
        fx.write("skills/gamma/SKILL.md", "---\nname: gamma\n---\n");

        let mut skills = scan_skills_dir(&fx.root.join("skills"), GLOBAL_SOURCE);
        skills.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(skills.len(), 3);

        assert_eq!(skills[0].name, "alpha");
        assert_eq!(skills[0].description, "第一个");
        assert_eq!(skills[0].version.as_deref(), Some("1.0.0"));
        assert_eq!(skills[0].source, GLOBAL_SOURCE);
        assert!(skills[0].path.ends_with("SKILL.md"));

        // name 缺失 → 目录名；数字 version → 字面值字符串
        assert_eq!(skills[1].name, "beta");
        assert_eq!(skills[1].version.as_deref(), Some("1.2"));

        // metadata 缺失 → version None，description 缺失 → 空串
        assert_eq!(skills[2].name, "gamma");
        assert_eq!(skills[2].version, None);
        assert_eq!(skills[2].description, "");
    }

    #[test]
    fn skills_bad_yaml_tolerated() {
        let fx = Fixture::new("skills-bad-yaml");
        fx.write(
            "skills/good/SKILL.md",
            "---\nname: good\ndescription: 好的\n---\n",
        );
        fx.write(
            "skills/broken/SKILL.md",
            "---\nname: [unclosed\ndescription: ::: bad\n---\n",
        );

        let mut skills = scan_skills_dir(&fx.root.join("skills"), GLOBAL_SOURCE);
        skills.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(skills.len(), 2, "坏 YAML 项必须仍在列表中");

        // 坏项：name 用目录名，description 用容错文案
        assert_eq!(skills[0].name, "broken");
        assert_eq!(skills[0].description, FRONTMATTER_FAILED);
        assert_eq!(skills[0].version, None);

        // 好项不受影响
        assert_eq!(skills[1].name, "good");
        assert_eq!(skills[1].description, "好的");
    }

    #[test]
    fn missing_dirs_return_empty() {
        let fx = Fixture::new("missing-dirs");
        let nowhere = fx.root.join("does-not-exist");
        assert!(scan_commands_dir(&nowhere.join("commands"), GLOBAL_SOURCE).is_empty());
        assert!(scan_agents_dir(&nowhere.join("agents"), GLOBAL_SOURCE).is_empty());
        assert!(scan_skills_dir(&nowhere.join("skills"), GLOBAL_SOURCE).is_empty());
    }

    #[test]
    fn command_namespace_with_slash() {
        let fx = Fixture::new("cmd-namespace");
        fx.write(
            "commands/top.md",
            "---\ndescription: 顶层\nargument-hint: \"[arg]\"\n---\n",
        );
        fx.write("commands/git/pr.md", "---\ndescription: 命名空间\n---\n");

        let mut commands = scan_commands_dir(&fx.root.join("commands"), GLOBAL_SOURCE);
        commands.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(commands.len(), 2);

        assert_eq!(commands[0].name, "git/pr");
        assert_eq!(commands[0].description, "命名空间");
        assert_eq!(commands[0].argument_hint, None);

        assert_eq!(commands[1].name, "top");
        assert_eq!(commands[1].argument_hint.as_deref(), Some("[arg]"));
    }

    #[cfg(unix)]
    #[test]
    fn skills_symlink_dir_followed() {
        let fx = Fixture::new("skills-symlink");
        // 真实 skill 目录在 skills 之外，skills 下放 symlink
        fx.write(
            "elsewhere/linked/SKILL.md",
            "---\nname: linked\ndescription: 经 symlink 进来\n---\n",
        );
        fs::create_dir_all(fx.root.join("skills")).unwrap();
        std::os::unix::fs::symlink(
            fx.root.join("elsewhere/linked"),
            fx.root.join("skills/linked"),
        )
        .unwrap();

        let skills = scan_skills_dir(&fx.root.join("skills"), GLOBAL_SOURCE);
        assert_eq!(skills.len(), 1, "symlink 目录必须被跟随");
        assert_eq!(skills[0].name, "linked");
        assert_eq!(skills[0].description, "经 symlink 进来");
    }

    #[test]
    fn agents_name_fallback_and_bad_yaml() {
        let fx = Fixture::new("agents");
        fx.write("agents/reviewer.md", "---\ndescription: 审查\n---\n正文");
        fx.write("agents/broken.md", "---\n: : :\n---\n");

        let mut agents = scan_agents_dir(&fx.root.join("agents"), "proj");
        agents.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "broken");
        assert_eq!(agents[0].description, FRONTMATTER_FAILED);
        assert_eq!(agents[1].name, "reviewer");
        assert_eq!(agents[1].description, "审查");
        assert_eq!(agents[1].source, "proj");
    }

    #[test]
    fn mcp_transport_inference_and_disabled() {
        let fx = Fixture::new("mcp");
        let mcp_path = fx.write(
            ".mcp.json",
            r#"{
  "mcpServers": {
    "local-tool": { "command": "bun", "args": ["run", "x.ts"], "env": { "KEY": "secret" } },
    "remote": { "url": "https://example.com/mcp" },
    "events": { "type": "sse", "url": "https://example.com/sse" },
    "weird": "不是对象的坏条目"
  }
}"#,
        );
        let disabled = vec!["remote".to_string()];
        let mut servers = scan_mcp_json(&mcp_path, "proj", &disabled);
        servers.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(servers.len(), 4, "坏条目不拖垮整表，名字保留");

        // 显式 type 优先
        assert_eq!(servers[0].name, "events");
        assert_eq!(servers[0].transport, "sse");
        assert_eq!(servers[0].endpoint, "https://example.com/sse");
        assert!(servers[0].enabled);

        // 缺 type 有 command → stdio；endpoint 只含 command 主体，args/env 不进内存
        assert_eq!(servers[1].name, "local-tool");
        assert_eq!(servers[1].transport, "stdio");
        assert_eq!(servers[1].endpoint, "bun");
        assert!(servers[1].enabled);

        // 有 url 无 type → http；在 disabled 列表中 → enabled=false
        assert_eq!(servers[2].name, "remote");
        assert_eq!(servers[2].transport, "http");
        assert_eq!(servers[2].endpoint, "https://example.com/mcp");
        assert!(!servers[2].enabled);

        // 值类型不符的坏条目：保名字，transport=unknown / endpoint 空
        assert_eq!(servers[3].name, "weird");
        assert_eq!(servers[3].transport, "unknown");
        assert_eq!(servers[3].endpoint, "");
    }

    #[test]
    fn local_scope_mcp_servers_included() {
        let fx = Fixture::new("local-scope-mcp");
        let project_root = fx.root.join("proj-b");
        let project_cwd = project_root.display().to_string();
        // local scope（projects.<cwd>.mcpServers，claude mcp add 默认写入位置）：
        // 正常 stdio/http 条目 + 类型不符的坏条目 + 与 .mcp.json 同名的条目
        fx.write(
            ".claude.json",
            &format!(
                r#"{{
  "projects": {{ "{}": {{
    "mcpServers": {{
      "local-stdio": {{ "command": "uvx", "args": ["srv"], "env": {{ "KEY": "secret" }} }},
      "local-http": {{ "type": "http", "url": "http://localhost:7777" }},
      "bad-local": 42,
      "dup": {{ "command": "node" }}
    }},
    "disabledMcpjsonServers": ["dup"]
  }} }}
}}"#,
                project_cwd.replace('\\', "\\\\")
            ),
        );
        fx.write(
            "proj-b/.mcp.json",
            r#"{ "mcpServers": { "dup": { "url": "http://localhost:8888" } } }"#,
        );

        let assets = collect_assets(&fx.root, &[project_cwd]);
        let servers = &assets.mcp_servers;
        let claude_json_path = fx.root.join(".claude.json").display().to_string();

        // 5 行 = local scope 4（含坏条目）+ .mcp.json 1（同名各自成行不合并）
        assert_eq!(servers.len(), 5);
        // local scope 自然落入项目级组：source 一律为项目目录名
        assert!(servers.iter().all(|s| s.source == "proj-b"));

        // 正常条目：transport 推断正确、enabled 恒 true、path = ~/.claude.json 绝对路径
        let stdio = servers.iter().find(|s| s.name == "local-stdio").unwrap();
        assert_eq!(stdio.transport, "stdio");
        assert_eq!(stdio.endpoint, "uvx", "args/env 不进 endpoint");
        assert!(stdio.enabled);
        assert_eq!(stdio.path, claude_json_path);

        let http = servers.iter().find(|s| s.name == "local-http").unwrap();
        assert_eq!(http.transport, "http");
        assert_eq!(http.endpoint, "http://localhost:7777");
        assert!(http.enabled);

        // 坏条目逐项容错：保名字标 unknown，不拖垮其余条目
        let bad = servers.iter().find(|s| s.name == "bad-local").unwrap();
        assert_eq!(bad.transport, "unknown");
        assert_eq!(bad.endpoint, "");
        assert_eq!(bad.path, claude_json_path);

        // 同名并存两行（path 不同），disabledMcpjsonServers 只禁 .mcp.json 那一行
        let dups: Vec<_> = servers.iter().filter(|s| s.name == "dup").collect();
        assert_eq!(dups.len(), 2, "local scope 与 .mcp.json 同名必须各自成行");
        let local_dup = dups.iter().find(|s| s.path == claude_json_path).unwrap();
        assert_eq!(local_dup.transport, "stdio");
        assert!(local_dup.enabled, "local scope 无禁用机制，恒 enabled");
        let mcpjson_dup = dups
            .iter()
            .find(|s| s.path.ends_with(".mcp.json"))
            .unwrap();
        assert_eq!(mcpjson_dup.transport, "http");
        assert!(
            !mcpjson_dup.enabled,
            "disabledMcpjsonServers 仅作用于 .mcp.json 声明的服务器"
        );
    }

    #[test]
    fn collect_assets_sources_and_order() {
        let fx = Fixture::new("collect");
        // 「家目录」fixture：全局 skill + ~/.claude.json（全局 MCP + 项目 disabled 列表）
        fx.write(
            ".claude/skills/zeta/SKILL.md",
            "---\nname: zeta\ndescription: 全局\n---\n",
        );
        let project_root = fx.root.join("proj-a");
        let project_cwd = project_root.display().to_string();
        // bad-global 为类型不符的坏条目：必须不拖垮整文件解析
        // （否则用户级 MCP 全空、项目 disabled 列表丢失 → p-off 误判 enabled）
        fx.write(
            ".claude.json",
            &format!(
                r#"{{
  "mcpServers": {{ "global-mcp": {{ "url": "http://localhost:1234" }}, "bad-global": null }},
  "projects": {{ "{}": {{ "disabledMcpjsonServers": ["p-off"] }} }},
  "irrelevantTopLevelKey": 42
}}"#,
                project_cwd.replace('\\', "\\\\")
            ),
        );
        // 项目级：skill + .mcp.json
        fx.write(
            "proj-a/.claude/skills/alpha/SKILL.md",
            "---\nname: alpha\ndescription: 项目级\n---\n",
        );
        fx.write(
            "proj-a/.mcp.json",
            r#"{ "mcpServers": {
  "p-off": { "command": "deno" },
  "p-on": { "url": "http://localhost:9" }
} }"#,
        );

        let assets = collect_assets(&fx.root, &[project_cwd]);

        // 排序：全局在前，项目级在后；name 虽然 alpha < zeta，source 优先
        assert_eq!(assets.skills.len(), 2);
        assert_eq!(assets.skills[0].name, "zeta");
        assert_eq!(assets.skills[0].source, GLOBAL_SOURCE);
        assert_eq!(assets.skills[1].name, "alpha");
        assert_eq!(assets.skills[1].source, "proj-a");

        // MCP：全局恒 enabled；项目级按 disabledMcpjsonServers 判定；
        // 坏条目 bad-global 保名字列出且不影响其余条目与 disabled 列表
        assert_eq!(assets.mcp_servers.len(), 4);
        assert_eq!(assets.mcp_servers[0].name, "bad-global");
        assert_eq!(assets.mcp_servers[0].transport, "unknown");
        assert_eq!(assets.mcp_servers[0].source, GLOBAL_SOURCE);
        assert_eq!(assets.mcp_servers[1].name, "global-mcp");
        assert_eq!(assets.mcp_servers[1].source, GLOBAL_SOURCE);
        assert!(assets.mcp_servers[1].enabled);
        assert_eq!(assets.mcp_servers[2].name, "p-off");
        assert!(!assets.mcp_servers[2].enabled);
        assert_eq!(assets.mcp_servers[3].name, "p-on");
        assert!(assets.mcp_servers[3].enabled);

        // 不存在的目录静默为空
        assert!(assets.commands.is_empty());
        assert!(assets.agents.is_empty());
    }

    #[test]
    fn frontmatter_edge_cases() {
        // 无 frontmatter → 缺省解析（非失败）
        match parse_frontmatter::<SkillFrontmatter>("# 只有正文\n") {
            Frontmatter::Parsed(fm) => assert!(fm.name.is_none()),
            Frontmatter::Failed => panic!("无 frontmatter 不应判为失败"),
        }
        // 空 frontmatter → 缺省
        match parse_frontmatter::<SkillFrontmatter>("---\n---\n正文") {
            Frontmatter::Parsed(fm) => assert!(fm.description.is_none()),
            Frontmatter::Failed => panic!("空 frontmatter 不应判为失败"),
        }
        // EOF 处结束分隔符（无尾换行）
        assert_eq!(
            extract_frontmatter("---\nname: x\n---"),
            Some("name: x\n")
        );
        // CRLF 行尾
        assert_eq!(
            extract_frontmatter("---\r\nname: x\r\n---\r\n"),
            Some("name: x\r\n")
        );
    }
}
