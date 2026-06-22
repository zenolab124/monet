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
