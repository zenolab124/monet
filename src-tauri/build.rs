fn main() {
  // ProMotion 高刷解锁：ObjC swizzle 必须在 WKWebView 创建前就位，
  // 见 src/native/high_refresh.m 顶部注释
  #[cfg(target_os = "macos")]
  {
    cc::Build::new()
      .file("src/native/high_refresh.m")
      .flag("-fobjc-arc")
      .compile("cc_space_high_refresh");
    println!("cargo:rerun-if-changed=src/native/high_refresh.m");
    println!("cargo:rustc-link-lib=framework=WebKit");
  }
  tauri_build::build()
}
