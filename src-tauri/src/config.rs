use std::path::{Path, PathBuf};
use std::sync::OnceLock;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();
static CLAUDE_ROOT: OnceLock<PathBuf> = OnceLock::new();

pub fn data_dir() -> &'static PathBuf {
    DATA_DIR.get_or_init(|| {
        if let Ok(dir) = std::env::var("MONET_DATA_DIR") {
            PathBuf::from(dir)
        } else {
            let home = dirs::home_dir().unwrap_or_default();
            let target = home.join(".monet");
            // 数据目录更名迁移(cc-space → monet):新目录尚未建立且旧目录仍在时，
            // 整体 rename 搬迁;失败则回退沿用旧目录(不丢数据,仅打日志)。
            // OnceLock 初始化只跑一次,迁移检测无高频代价。
            if !target.exists() {
                let legacy = home.join(".cc-space");
                if legacy.is_dir() {
                    if let Err(e) = std::fs::rename(&legacy, &target) {
                        // 本文件经 #[path] 编进精简 bin(monet-mcp),坚持 std-only,不引 log
                        eprintln!("[monet] 数据目录迁移 ~/.cc-space → ~/.monet 失败,继续沿用旧目录: {e}");
                        return legacy;
                    }
                }
            }
            target
        }
    })
}

/// 展开路径开头的 `~`（仅 `~` 与 `~/...` 两种形式）
fn expand_tilde(path: &str) -> PathBuf {
    if path == "~" {
        return dirs::home_dir().unwrap_or_default();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir().unwrap_or_default().join(rest);
    }
    PathBuf::from(path)
}

/// 读取 ~/.monet/settings.json 中的单个设置键（散读，不依赖 typed struct）
pub fn read_app_setting(key: &str) -> Option<serde_json::Value> {
    let path = data_dir().join("settings.json");
    let content = std::fs::read_to_string(path).ok()?;
    let settings: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(&content).ok()?;
    settings.get(key).cloned()
}

/// 写入 ~/.monet/settings.json 中的单个设置键；value 为 null 时删除该键。
/// 文件存在但解析失败时拒绝覆写，避免带着空 map 清掉其他设置键
pub fn write_app_setting(key: &str, value: serde_json::Value) {
    let path = data_dir().join("settings.json");
    let mut settings: serde_json::Map<String, serde_json::Value> =
        match std::fs::read_to_string(&path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(m) => m,
                Err(_) => return,
            },
            Err(_) => Default::default(),
        };
    if value.is_null() {
        settings.remove(key);
    } else {
        settings.insert(key.to_string(), value);
    }
    if let Ok(json) = serde_json::to_string_pretty(&serde_json::Value::Object(settings)) {
        let _ = atomic_write(&path, &json);
    }
}

/// Claude Code 数据根目录（默认 ~/.claude）的**唯一**解析入口。
/// 全项目禁止再手拼 `home.join(".claude")` —— 一律经此收口。
///
/// 优先级：
/// 1. env `MONET_CLAUDE_ROOT`（进程级临时覆盖，不落设置）
/// 2. env `CLAUDE_CONFIG_DIR`（Claude CLI 官方变量，终端启动时对齐 CLI 行为）
/// 3. 设置项 `claudeRoot`（~/.monet/settings.json，App 内可配）
/// 4. 默认 `~/.claude`
///
/// 进程级缓存（OnceLock）：watcher 与搜索索引均为启动时一次性绑定，
/// 改设置后统一「重启生效」，缓存与该语义自洽
pub fn claude_root() -> &'static PathBuf {
    CLAUDE_ROOT.get_or_init(resolve_claude_root)
}

/// 即时解析（绕过进程级缓存）：设置页用它对比「当前生效」与「重启后生效」，
/// 判断是否需要提示重启。业务代码一律用 claude_root()，不要用这个
pub fn resolve_claude_root() -> PathBuf {
    for var in ["MONET_CLAUDE_ROOT", "CLAUDE_CONFIG_DIR"] {
        if let Ok(dir) = std::env::var(var) {
            let dir = dir.trim();
            if !dir.is_empty() {
                return expand_tilde(dir);
            }
        }
    }
    if let Some(v) = read_app_setting("claudeRoot") {
        if let Some(s) = v.as_str() {
            let s = s.trim();
            if !s.is_empty() {
                return expand_tilde(s);
            }
        }
    }
    default_claude_root()
}

/// 默认 Claude 数据根（不含任何覆盖），供设置 UI 展示与「是否默认」判断
pub fn default_claude_root() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude")
}

/// 会话 JSONL 所在的 projects 根目录
pub fn projects_dir() -> PathBuf {
    claude_root().join("projects")
}

/// 顶层 `.claude.json` 的位置：默认根时位于 `~/.claude.json`（home 下），
/// 自定义根时随 CLAUDE_CONFIG_DIR 官方语义位于 `<root>/.claude.json`
pub fn claude_json_path() -> PathBuf {
    let root = claude_root();
    if is_default_claude_root(root) {
        dirs::home_dir().unwrap_or_default().join(".claude.json")
    } else {
        root.join(".claude.json")
    }
}

fn is_default_claude_root(root: &Path) -> bool {
    *root == default_claude_root()
}

/// 内置 Agent 的专属工作目录：spawn CLI 时的 cwd 固定在这里，
/// 与用户项目隔离（Agent 调用另有 --no-session-persistence 保证不落盘）
pub fn agent_cwd() -> PathBuf {
    let p = data_dir().join("agent");
    let _ = std::fs::create_dir_all(&p);
    p
}

/// Claude CLI 对 cwd 的 projects 目录编码：非字母数字字符一律替换为 `-`
fn encode_project_dir(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// 内置 Agent 工作目录对应的 projects 编码名（含历史数据目录）。
/// Agent 会话默认落盘（设置项 agentSessionPersist 可关，供事后追溯）；
/// 这份清单是全部扫描面的软屏蔽口径：档案馆/搜索/watcher/用量统计/Widget
/// 据此排除 Agent 会话目录，不混入用户视图
pub fn agent_project_dirs() -> Vec<String> {
    // CLI 对 cwd 先 canonicalize 再编码（symlink 场景逻辑路径会对不上），对齐口径；
    // canonicalize 失败（目录尚不存在等）回落逻辑路径
    let cwd = agent_cwd();
    let canonical = cwd.canonicalize().unwrap_or(cwd);
    let mut names = vec![encode_project_dir(&canonical)];
    if let Some(home) = dirs::home_dir() {
        // 数据目录迁移前的旧位置 ~/.claude/cc-space/agent
        let legacy = home.join(".claude").join("cc-space").join("agent");
        let legacy = legacy.canonicalize().unwrap_or(legacy);
        names.push(encode_project_dir(&legacy));
    }
    names.dedup();
    names
}

/// 原子写文本文件（临时文件 + rename）。
/// settings.json 等被主 App 与 runner 跨进程读写的文件必须走这里，
/// 裸 fs::write 的 truncate-write 间隙会被并发读者读到半截 JSON
pub fn atomic_write(path: &std::path::Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, content)?;
    std::fs::rename(&tmp, path)
}
