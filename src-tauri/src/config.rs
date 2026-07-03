use std::path::PathBuf;
use std::sync::OnceLock;

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn data_dir() -> &'static PathBuf {
    DATA_DIR.get_or_init(|| {
        if let Ok(dir) = std::env::var("CC_SPACE_DATA_DIR") {
            PathBuf::from(dir)
        } else {
            dirs::home_dir().unwrap_or_default().join(".cc-space")
        }
    })
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
