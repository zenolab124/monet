fn main() {
  // ProMotion 高刷解锁：ObjC swizzle 必须在 WKWebView 创建前就位，
  // 见 src/native/high_refresh.m 顶部注释
  #[cfg(target_os = "macos")]
  {
    cc::Build::new()
      .file("src/native/high_refresh.m")
      .flag("-fobjc-arc")
      .compile("monet_high_refresh");
    println!("cargo:rerun-if-changed=src/native/high_refresh.m");
    println!("cargo:rustc-link-lib=framework=WebKit");

    // TCC 权限静默检测（设置页权限体检），主 App 与 routine-runner 共用
    cc::Build::new()
      .file("src/native/tcc_check.c")
      .compile("monet_tcc_check");
    println!("cargo:rerun-if-changed=src/native/tcc_check.c");
    println!("cargo:rustc-link-lib=framework=CoreServices");
    println!("cargo:rustc-link-lib=framework=ApplicationServices");
    // rustc-link-lib 只随 lib target 传播；runner bin 不依赖 app lib
    //（避免链入 tauri），需要按 bin 显式补链接参数
    let out_dir = std::env::var("OUT_DIR").unwrap();
    println!(
      "cargo:rustc-link-arg-bin=monet-routine-runner={}/libmonet_tcc_check.a",
      out_dir
    );
    for arg in ["-framework", "CoreServices", "-framework", "ApplicationServices"] {
      println!("cargo:rustc-link-arg-bin=monet-routine-runner={}", arg);
    }
  }
  tauri_build::build()
}
