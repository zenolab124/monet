// 平台探测：UA 在 Tauri WebView 下可靠
// （macOS WKWebView 含 "Mac OS X"，Windows WebView2 含 "Windows NT"）。
// macOS 专属界面（红绿灯死区、TCC 权限体检、菜单栏/小组件设置）据此隐藏。
const ua = navigator.userAgent

export const isMac = /Mac/i.test(ua)
export const isWindows = /Windows/i.test(ua)

export function usePlatform() {
  return { isMac, isWindows }
}
