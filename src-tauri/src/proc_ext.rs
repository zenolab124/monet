//! spawn 子进程的 Windows 控制台抑制收口。
//!
//! Windows 下 GUI 子系统进程 spawn 控制台程序（claude/git/schtasks/cmd 等）
//! 会闪出一个终端窗口，必须带 CREATE_NO_WINDOW 创建标志。此前各调用点手写
//! `#[cfg(windows)] creation_flags` 极易遗漏，统一收口为 `.hide_console()`；
//! 非 Windows 平台为空操作。
//!
//! 例外（不经此收口，保持自包含）：`claude_locator.rs` 与 `bin/` 下的独立
//! 二进制通过 `#[path]` 共享源码、不链接 app_lib，各自内联同样的 cfg 块。

pub trait HideConsole {
    /// Windows：抑制子进程控制台窗口；其他平台无操作
    fn hide_console(&mut self) -> &mut Self;
}

impl HideConsole for std::process::Command {
    #[cfg(windows)]
    fn hide_console(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
    }

    #[cfg(not(windows))]
    fn hide_console(&mut self) -> &mut Self {
        self
    }
}
