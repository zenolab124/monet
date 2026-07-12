use std::path::PathBuf;
use std::sync::OnceLock;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

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
