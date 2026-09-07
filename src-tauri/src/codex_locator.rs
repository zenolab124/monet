use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const FAIL_TTL: Duration = Duration::from_secs(60);
pub const RUNTIME_SOURCE_SETTING_KEY: &str = "codexRuntimeSource";
pub const BINARY_PATH_SETTING_KEY: &str = "codexBinaryPath";

static MEM_HIT: Mutex<Option<PathBuf>> = Mutex::new(None);
static MEM_FAIL: Mutex<Option<Instant>> = Mutex::new(None);
static ACTIVE_RUNTIME_SOURCE: OnceLock<CodexRuntimeSource> = OnceLock::new();
static ACTIVE_MANUAL_PATH: OnceLock<Option<String>> = OnceLock::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CodexRuntimeSource {
    Standalone,
    Desktop,
}

impl CodexRuntimeSource {
    fn from_setting(value: &serde_json::Value) -> Option<Self> {
        match value.as_str()? {
            "standalone" => Some(Self::Standalone),
            "desktop" => Some(Self::Desktop),
            _ => None,
        }
    }
}

#[cfg(unix)]
fn is_valid_binary(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.is_file()
        && std::fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_valid_binary(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
}

fn nvm_latest_bin(home: &Path) -> Option<PathBuf> {
    let root = home.join(".nvm").join("versions").join("node");
    let latest = std::fs::read_dir(root)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .filter_map(|entry| {
            let version: Vec<u32> = entry
                .file_name()
                .to_string_lossy()
                .trim_start_matches('v')
                .split('.')
                .filter_map(|part| part.parse().ok())
                .collect();
            (version.len() == 3).then(|| (version, entry.path()))
        })
        .max_by(|left, right| left.0.cmp(&right.0))?;
    Some(latest.1.join("bin").join(binary_name()))
}

fn binary_name() -> &'static str {
    if cfg!(windows) {
        "codex.exe"
    } else {
        "codex"
    }
}

fn absolute_root(path: PathBuf, home: Option<&Path>) -> Option<PathBuf> {
    if path.is_absolute() {
        return Some(path);
    }
    let suffix = path.strip_prefix("~").ok()?;
    Some(home?.join(suffix))
}

fn candidate_paths() -> Vec<PathBuf> {
    let home = dirs::home_dir().filter(|path| path.is_absolute());
    let mut candidates: Vec<PathBuf> = std::env::split_paths(&crate::path_env::enhanced_path())
        .filter_map(|directory| absolute_root(directory, home.as_deref()))
        .map(|directory| directory.join(binary_name()))
        .collect();

    if let Some(home) = home.as_deref() {
        candidates.extend([
            home.join(".local").join("bin").join(binary_name()),
            home.join(".cargo").join("bin").join(binary_name()),
            home.join(".volta").join("bin").join(binary_name()),
            home.join(".bun").join("bin").join(binary_name()),
        ]);
    }

    #[cfg(target_os = "macos")]
    {
        candidates.extend([
            PathBuf::from("/opt/homebrew/bin/codex"),
            PathBuf::from("/usr/local/bin/codex"),
        ]);
        if let Some(home) = home.as_deref() {
            candidates.push(home.join("Library").join("pnpm").join("codex"));
        }
    }

    #[cfg(target_os = "linux")]
    {
        candidates.extend([
            PathBuf::from("/usr/local/bin/codex"),
            PathBuf::from("/usr/bin/codex"),
        ]);
        if let Some(home) = home.as_deref() {
            candidates.push(home.join(".local").join("share").join("pnpm").join("codex"));
        }
    }

    #[cfg(windows)]
    {
        for key in [
            "NPM_CONFIG_PREFIX",
            "npm_config_prefix",
            "PNPM_HOME",
            "NVM_SYMLINK",
        ] {
            if let Some(root) = std::env::var_os(key)
                .and_then(|value| absolute_root(PathBuf::from(value), home.as_deref()))
            {
                candidates.push(root.join(binary_name()));
            }
        }
        if let Ok(appdata) = std::env::var("APPDATA") {
            if let Some(root) = absolute_root(PathBuf::from(appdata), home.as_deref()) {
                candidates.push(root.join("npm").join(binary_name()));
            }
        }
        if let Ok(localappdata) = std::env::var("LOCALAPPDATA") {
            if let Some(root) = absolute_root(PathBuf::from(localappdata), home.as_deref()) {
                candidates.push(root.join("Programs").join("Codex").join(binary_name()));
            }
        }
    }

    if let Some(home) = home.as_deref() {
        if let Some(path) = nvm_latest_bin(home) {
            candidates.push(path);
        }
    }
    #[cfg(windows)]
    {
        // npm 的入口通常是 codex.cmd；直接定位包内原生程序，不执行 shell shim。
        let roots: Vec<_> = candidates.iter().filter_map(|p| p.parent()).collect();
        let mut npm_candidates = Vec::new();
        for root in roots {
            npm_candidates.extend(windows_npm_candidates(root, std::env::consts::ARCH));
        }
        candidates.extend(npm_candidates);
    }
    candidates
}

/// 同时覆盖 npm 平台依赖（提升/嵌套安装）和早期包内 vendor 布局。
/// 不执行或解析 cmd/PowerShell 内容；可在所有宿主上用 Windows 布局测试。
#[cfg(any(windows, test))]
fn windows_npm_candidates(root: &Path, arch: &str) -> Vec<PathBuf> {
    let (package, target) = match arch {
        "x86_64" => ("codex-win32-x64", "x86_64-pc-windows-msvc"),
        "aarch64" => ("codex-win32-arm64", "aarch64-pc-windows-msvc"),
        _ => return Vec::new(),
    };
    let mut paths = Vec::new();
    for package_root in [root.to_path_buf(), root.join("node_modules/@openai/codex")] {
        let mut roots = vec![package_root.clone()];
        if let Ok(real) = std::fs::canonicalize(&package_root) {
            if real != package_root {
                roots.push(real);
            }
        }
        for root in roots {
            let mut vendors = vec![root
                .join("node_modules/@openai")
                .join(package)
                .join("vendor")];
            if let Some(scope) = root.parent() {
                vendors.push(scope.join(package).join("vendor"));
            }
            vendors.push(root.join("vendor"));
            for vendor in vendors {
                for directory in ["bin", "codex"] {
                    paths.push(vendor.join(target).join(directory).join("codex.exe"));
                }
            }
        }
    }
    paths
}

pub fn resolve_manual_path(input: &str) -> Result<PathBuf, String> {
    let path = absolute_root(PathBuf::from(input.trim()), dirs::home_dir().as_deref())
        .ok_or_else(|| "请选择 Codex 可执行文件或安装目录的绝对路径".to_string())?;
    if is_valid_binary(&path) {
        return Ok(path);
    }
    let mut candidates = Vec::new();
    if path.is_dir() {
        candidates.extend([
            path.join(binary_name()),
            path.join("bin").join(binary_name()),
        ]);
    }
    #[cfg(windows)]
    {
        let root = if path.is_dir() {
            Some(path.as_path())
        } else if path.is_file()
            && path.file_name().is_some_and(|name| {
                ["codex.cmd", "codex.ps1", "codex"]
                    .iter()
                    .any(|value| name.eq_ignore_ascii_case(value))
            })
        {
            path.parent()
        } else {
            None
        };
        if let Some(root) = root {
            candidates.extend(windows_npm_candidates(root, std::env::consts::ARCH));
        }
    }
    candidates
        .into_iter()
        .find(|path| is_valid_binary(path))
        .ok_or_else(|| {
            "未在所选位置找到 Codex 原生可执行文件；npm 安装请确认平台依赖完整".to_string()
        })
}

fn settings_path() -> Option<PathBuf> {
    std::env::var_os("MONET_DATA_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".monet")))
        .map(|directory| directory.join("settings.json"))
}

pub fn configured_runtime_source() -> CodexRuntimeSource {
    settings_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|settings| settings.get(RUNTIME_SOURCE_SETTING_KEY).cloned())
        .as_ref()
        .and_then(CodexRuntimeSource::from_setting)
        .unwrap_or(CodexRuntimeSource::Standalone)
}

pub fn configured_manual_path() -> Option<String> {
    settings_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|settings| {
            settings
                .get(BINARY_PATH_SETTING_KEY)?
                .as_str()
                .map(str::to_owned)
        })
        .filter(|path| !path.trim().is_empty())
}

pub fn active_manual_path() -> Option<&'static str> {
    ACTIVE_MANUAL_PATH
        .get_or_init(configured_manual_path)
        .as_deref()
}

pub fn path_restart_required() -> bool {
    configured_runtime_source() == CodexRuntimeSource::Standalone
        && active_manual_path() != configured_manual_path().as_deref()
}

pub fn locate_configured_standalone() -> Result<PathBuf, String> {
    locate_with_manual(configured_manual_path().as_deref())
}

pub fn active_runtime_source() -> CodexRuntimeSource {
    *ACTIVE_RUNTIME_SOURCE.get_or_init(configured_runtime_source)
}

/// Detect the Codex binary bundled inside the ChatGPT desktop app without
/// treating it as a standalone CLI candidate. Monet only uses this path when
/// the user explicitly selects the desktop runtime.
pub fn desktop_bundle_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut candidates = vec![PathBuf::from(
            "/Applications/ChatGPT.app/Contents/Resources/codex",
        )];
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join("Applications/ChatGPT.app/Contents/Resources/codex"));
        }
        candidates.into_iter().find(|path| is_valid_binary(path))
    }

    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub fn locate() -> Result<PathBuf, String> {
    match active_runtime_source() {
        CodexRuntimeSource::Standalone => locate_standalone(),
        CodexRuntimeSource::Desktop => desktop_bundle_path().ok_or_else(|| {
            "The selected ChatGPT bundled Codex runtime is not available".to_string()
        }),
    }
}

pub fn locate_standalone() -> Result<PathBuf, String> {
    locate_with_manual(active_manual_path())
}

fn locate_with_manual(manual: Option<&str>) -> Result<PathBuf, String> {
    // 显式选择失效时保留错误，不静默切到另一个安装。
    if let Some(manual) = manual {
        return resolve_manual_path(manual);
    }
    {
        let mut hit = MEM_HIT.lock().unwrap_or_else(|error| error.into_inner());
        if let Some(path) = hit.clone() {
            if is_valid_binary(&path) {
                return Ok(path);
            }
            *hit = None;
        }
    }

    {
        let failed = MEM_FAIL.lock().unwrap_or_else(|error| error.into_inner());
        if failed.is_some_and(|at| at.elapsed() < FAIL_TTL) {
            return Err("Codex CLI not found".into());
        }
    }

    for path in candidate_paths() {
        if is_valid_binary(&path) {
            *MEM_HIT.lock().unwrap_or_else(|error| error.into_inner()) = Some(path.clone());
            *MEM_FAIL.lock().unwrap_or_else(|error| error.into_inner()) = None;
            return Ok(path);
        }
    }

    *MEM_FAIL.lock().unwrap_or_else(|error| error.into_inner()) = Some(Instant::now());
    Err("Codex CLI not found".into())
}

pub fn is_available() -> bool {
    locate().is_ok()
}

/// 清除探测缓存后重新定位，供设置页安装完成后立即复测。
pub fn redetect_standalone() -> Result<PathBuf, String> {
    *MEM_HIT.lock().unwrap_or_else(|error| error.into_inner()) = None;
    *MEM_FAIL.lock().unwrap_or_else(|error| error.into_inner()) = None;
    locate_configured_standalone()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("monet-codex-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn binary(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, b"fixture").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn finds_npm_native_binaries_in_supported_windows_layouts() {
        for layout in [
            "node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/bin/codex.exe",
            "node_modules/@openai/codex/node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc/codex/codex.exe",
            "node_modules/@openai/codex/vendor/x86_64-pc-windows-msvc/codex/codex.exe",
        ] {
            let fixture = Fixture::new();
            let binary = fixture.binary(&format!("custom npm prefix/{layout}"));
            let root = fixture.0.join("custom npm prefix");
            assert_eq!(windows_npm_candidates(&root, "x86_64").into_iter().find(|p| p.is_file()), Some(binary));
            assert!(!windows_npm_candidates(&root, "aarch64").iter().any(|p| p.is_file()));
        }
    }

    #[test]
    fn missing_platform_dependency_does_not_select_a_command_shim() {
        let fixture = Fixture::new();
        fixture.binary("codex.cmd");
        fixture.binary("codex.ps1");
        fixture.binary("node_modules/@openai/codex/bin/codex.js");
        assert!(!windows_npm_candidates(&fixture.0, "x86_64")
            .iter()
            .any(|p| p.is_file()));
        assert!(windows_npm_candidates(&fixture.0, "unsupported").is_empty());
    }

    #[test]
    fn finds_arm64_platform_dependency() {
        let fixture = Fixture::new();
        let binary = fixture.binary(
            "node_modules/@openai/codex-win32-arm64/vendor/aarch64-pc-windows-msvc/bin/codex.exe",
        );
        assert_eq!(
            windows_npm_candidates(&fixture.0, "aarch64")
                .into_iter()
                .find(|p| p.is_file()),
            Some(binary)
        );
    }

    #[test]
    fn accepts_absolute_files_and_directories_with_spaces() {
        let fixture = Fixture::new();
        let binary = fixture.binary(&format!("Custom CLI/bin/{}", binary_name()));
        assert_eq!(
            resolve_manual_path(binary.to_str().unwrap()).unwrap(),
            binary
        );
        assert_eq!(
            resolve_manual_path(fixture.0.join("Custom CLI").to_str().unwrap()).unwrap(),
            binary
        );
        assert!(resolve_manual_path("relative/codex").is_err());
        std::fs::remove_file(binary).unwrap();
        assert!(locate_with_manual(Some(fixture.0.join("Custom CLI").to_str().unwrap())).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn resolves_selected_npm_shim_without_executing_it() {
        let fixture = Fixture::new();
        let shim = fixture.binary("codex.cmd");
        let binary = windows_npm_candidates(&fixture.0, std::env::consts::ARCH)
            .pop()
            .unwrap();
        fixture.binary(binary.strip_prefix(&fixture.0).unwrap().to_str().unwrap());
        assert_eq!(resolve_manual_path(shim.to_str().unwrap()).unwrap(), binary);
    }

    #[test]
    fn manual_changes_wait_for_restart_and_redetect_clears_failure_cache() {
        const CHILD: &str = "MONET_CODEX_LOCATOR_TEST_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let fixture = Fixture::new();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "codex_locator::tests::manual_changes_wait_for_restart_and_redetect_clears_failure_cache", "--nocapture"])
                .env(CHILD, "1").env("MONET_DATA_DIR", &fixture.0).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let fixture = Fixture::new();
        let original = fixture.binary(&format!("original/{}", binary_name()));
        let replacement = fixture.binary(&format!("replacement/{}", binary_name()));
        let settings = settings_path().unwrap();
        let write = |path: &Path| {
            std::fs::write(
                &settings,
                serde_json::to_vec(&serde_json::json!({BINARY_PATH_SETTING_KEY: path})).unwrap(),
            )
            .unwrap()
        };
        write(&original);
        assert_eq!(locate_standalone().unwrap(), original);
        write(&replacement);
        assert_eq!(locate_configured_standalone().unwrap(), replacement);
        assert_eq!(locate_standalone().unwrap(), original);
        assert!(path_restart_required());
        *MEM_FAIL.lock().unwrap() = Some(Instant::now());
        redetect_standalone().unwrap();
        assert!(MEM_FAIL.lock().unwrap().is_none());
        write(&original);
        assert!(!path_restart_required());
        std::fs::remove_file(original).unwrap();
        assert!(locate_standalone().is_err());
    }

    #[test]
    fn candidates_are_absolute() {
        assert!(candidate_paths().iter().all(|path| path.is_absolute()));
    }

    #[test]
    fn expands_tilde_and_rejects_other_relative_roots() {
        let home = std::env::current_dir().unwrap();
        assert_eq!(
            absolute_root(PathBuf::from("~").join("bin"), Some(&home)),
            Some(home.join("bin")),
        );
        assert_eq!(
            absolute_root(PathBuf::from("relative/bin"), Some(&home)),
            None
        );
    }

    #[test]
    fn parses_supported_runtime_source_values() {
        assert_eq!(
            CodexRuntimeSource::from_setting(&serde_json::json!("standalone")),
            Some(CodexRuntimeSource::Standalone)
        );
        assert_eq!(
            CodexRuntimeSource::from_setting(&serde_json::json!("desktop")),
            Some(CodexRuntimeSource::Desktop)
        );
        assert_eq!(
            CodexRuntimeSource::from_setting(&serde_json::json!("unknown")),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_rejects_command_shims() {
        assert!(candidate_paths().iter().all(|path| {
            path.extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("exe"))
        }));
    }
}
