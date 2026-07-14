//! macOS TCC 系统权限检测（设置页权限体检 + runner health-check 共用）。
//! 与 permission.rs（Claude 工具权限桥）无关。
//!
//! 自包含铁律：不依赖 crate::config / tauri —— routine-runner 以 #[path]
//! 方式复用本文件，引入 tauri 会把整个 GUI 依赖链进轻量二进制。
//! 非 macOS 平台所有函数返回 "unknown"，调用侧无需 cfg。

#[cfg(target_os = "macos")]
mod native {
    extern "C" {
        pub fn monet_ae_permission(
            bundle_id: *const std::os::raw::c_char,
            ask: bool,
        ) -> i32;
        pub fn monet_ax_trusted() -> i32;
        pub fn monet_ax_prompt() -> i32;
        pub fn monet_screen_preflight() -> i32;
        pub fn monet_screen_request() -> i32;
    }
}

#[cfg(target_os = "macos")]
fn status_name(code: i32) -> &'static str {
    match code {
        0 => "granted",
        1 => "denied",
        2 => "undetermined",
        3 => "targetNotRunning",
        _ => "unknown",
    }
}

/// 对目标 app 的自动化（Apple Events）权限。ask=false 纯查询零弹窗；
/// ask=true 未决时弹系统授权窗（阻塞至用户响应，调用方放 blocking 线程）
#[cfg(target_os = "macos")]
pub fn check_automation(bundle_id: &str, ask: bool) -> &'static str {
    let Ok(c) = std::ffi::CString::new(bundle_id) else {
        return "unknown";
    };
    status_name(unsafe { native::monet_ae_permission(c.as_ptr(), ask) })
}

#[cfg(target_os = "macos")]
pub fn check_accessibility() -> &'static str {
    status_name(unsafe { native::monet_ax_trusted() })
}

#[cfg(target_os = "macos")]
pub fn check_screen_capture() -> &'static str {
    status_name(unsafe { native::monet_screen_preflight() })
}

/// 屏幕录制授权请求：未决时弹窗；已 denied 不再弹（需深链系统设置）
#[cfg(target_os = "macos")]
pub fn request_screen_capture() -> &'static str {
    status_name(unsafe { native::monet_screen_request() })
}

/// 辅助功能授权引导：把本进程加入系统设置列表并弹引导窗
#[cfg(target_os = "macos")]
pub fn prompt_accessibility() -> &'static str {
    status_name(unsafe { native::monet_ax_prompt() })
}

/// 本地网络权限：Apple 不提供查询 API，加入 mDNS 组播组间接探测。
/// join_multicast 会触发系统授权弹窗（未授权时）
#[cfg(target_os = "macos")]
pub fn check_local_network() -> &'static str {
    use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};
    let sock = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return "unknown",
    };
    let multicast = Ipv4Addr::new(224, 0, 0, 251);
    sock.set_read_timeout(Some(std::time::Duration::from_secs(2))).ok();
    if sock.join_multicast_v4(&multicast, &Ipv4Addr::UNSPECIFIED).is_err() {
        return "denied";
    }
    let target = SocketAddrV4::new(multicast, 5353);
    let query: [u8; 12] = [0; 12];
    match sock.send_to(&query, target) {
        Ok(_) => "granted",
        Err(_) => "denied",
    }
}

/// 完全磁盘访问没有查询 API，试读 FDA 保护路径推断：
/// 明确的 PermissionDenied → denied；能读 → granted；路径不存在等 → 换下一个
#[cfg(target_os = "macos")]
pub fn check_full_disk_access() -> &'static str {
    let Some(home) = dirs::home_dir() else {
        return "unknown";
    };
    let probes = [
        // 系统级 TCC.db：必然存在且必受 FDA 保护，最可靠的探针
        std::path::PathBuf::from("/Library/Application Support/com.apple.TCC/TCC.db"),
        home.join("Library/Application Support/com.apple.TCC/TCC.db"),
        home.join("Library/Safari"),
    ];
    for p in probes {
        let readable = if p.extension().is_some() {
            std::fs::File::open(&p).map(|_| ())
        } else {
            std::fs::read_dir(&p).map(|_| ())
        };
        match readable {
            Ok(()) => return "granted",
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => return "denied",
            Err(_) => continue,
        }
    }
    "unknown"
}

// --- 非 macOS stub：体检功能仅 macOS，其他平台一律 unknown ---

#[cfg(not(target_os = "macos"))]
pub fn check_automation(_bundle_id: &str, _ask: bool) -> &'static str {
    "unknown"
}

#[cfg(not(target_os = "macos"))]
pub fn check_accessibility() -> &'static str {
    "unknown"
}

#[cfg(not(target_os = "macos"))]
pub fn check_screen_capture() -> &'static str {
    "unknown"
}

#[cfg(not(target_os = "macos"))]
pub fn request_screen_capture() -> &'static str {
    "unknown"
}

#[cfg(not(target_os = "macos"))]
pub fn prompt_accessibility() -> &'static str {
    "unknown"
}

#[cfg(not(target_os = "macos"))]
pub fn check_local_network() -> &'static str {
    "unknown"
}

#[cfg(not(target_os = "macos"))]
pub fn check_full_disk_access() -> &'static str {
    "unknown"
}
