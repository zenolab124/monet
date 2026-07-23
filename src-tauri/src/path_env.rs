//! 增强 PATH 单一事实源。
//! .app/launchd 环境 PATH 极简（仅系统目录），spawn 用户级工具的进程在
//! 运行时用它补齐 homebrew/node 等常见落点。主 App（经 streaming re-export）
//! 与 monet-routine-runner（#[path] 引入）共用一份实现。
//! 禁止把结果烘焙进 launchd plist：注册期快照会陈旧，且随启动语境
//! （终端/Finder）漂移，plist 内容一变就触发重注册与系统后台项通知。

/// 构建增强 PATH 环境变量
pub fn enhanced_path() -> String {
    let home = dirs::home_dir().unwrap_or_default();
    #[cfg(not(windows))]
    let mut extra_paths = vec![
        "/opt/homebrew/bin".to_string(),
        "/opt/homebrew/sbin".to_string(),
        "/usr/local/bin".to_string(),
        format!("{}/.local/bin", home.display()),
        // routine 链路历史上带 cargo bin（旧 plist 烘焙版含它），合并时保留
        format!("{}/.cargo/bin", home.display()),
    ];
    // Windows GUI 进程的 PATH 继承自注册表（相对完整），此处仅补 node 生态常见落点，
    // 保障 claude 子进程（npx MCP servers 等）可用
    #[cfg(windows)]
    let mut extra_paths: Vec<String> = {
        let mut v = Vec::new();
        if let Ok(appdata) = std::env::var("APPDATA") {
            v.push(format!("{}\\npm", appdata));
        }
        v.push("C:\\Program Files\\nodejs".to_string());
        v.push(format!("{}\\.local\\bin", home.display()));
        v
    };

    // 检测 nvm node 路径（语义化取最新——字典序会让 v9.x 压过 v18.x）
    let nvm_dir = home.join(".nvm/versions/node");
    if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
        let latest = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let ver: Vec<u32> = name
                    .trim_start_matches('v')
                    .split('.')
                    .filter_map(|s| s.parse().ok())
                    .collect();
                (ver.len() == 3).then(|| (ver, e.path()))
            })
            .max_by(|a, b| a.0.cmp(&b.0));
        if let Some((_, path)) = latest {
            extra_paths.push(format!("{}/bin", path.display()));
        }
    }

    // Windows 下 PATH 分隔符是 ';'，硬编码 ':' 会损坏子进程 PATH
    let sep = if cfg!(windows) { ";" } else { ":" };
    let existing = std::env::var("PATH").unwrap_or_default();
    format!("{}{}{}", extra_paths.join(sep), sep, existing)
}
