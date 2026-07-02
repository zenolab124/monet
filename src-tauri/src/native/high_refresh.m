// ProMotion 高刷解锁（macOS）。
//
// WebKit 将页面渲染更新（rAF/style/layout 节奏）钳制在 ~60fps 省电：
// experimental feature `PreferPageRenderingUpdatesNear60FPSEnabled`（macOS 26 起
// 旧 SPI `_setPreferPageRenderingUpdatesNear60FPS:` 已移除，只剩 feature flag 形态）。
// 实验证明（docs/research/perf-audit-2026-07.md）该 feature 仅在 WKWebView 创建
// 时刻从 configuration.preferences 读取——运行时改、reload、NSUserDefaults 覆盖
// 均无效，唯一时机是 init 之前。wry 不暴露 configuration 钩子，故 swizzle
// WKWebView 的指定初始化器，在创建瞬间对传入 configuration 关闭该 feature。
//
// 防御：feature key 不存在（未来 WebKit 更名/移除）时静默无操作，保持 60fps。
// 由 Rust 侧在 tauri::Builder 运行前调用 cc_space_install_high_refresh_unlock()。

#import <WebKit/WebKit.h>
#import <objc/runtime.h>
#import <objc/message.h>

static IMP g_orig_init = NULL;

static void cc_space_disable_sixty_clamp(WKWebViewConfiguration *config) {
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Warc-performSelector-leaks"
    SEL featuresSel = NSSelectorFromString(@"_experimentalFeatures");
    if (![WKPreferences respondsToSelector:featuresSel]) return;
    NSArray *features = [(id)[WKPreferences class] performSelector:featuresSel];
    SEL setSel = NSSelectorFromString(@"_setEnabled:forFeature:");
    if (![config.preferences respondsToSelector:setSel]) return;
    for (id f in features) {
        NSString *key = [f performSelector:NSSelectorFromString(@"key")];
        if ([key isEqualToString:@"PreferPageRenderingUpdatesNear60FPSEnabled"]) {
            ((void (*)(id, SEL, BOOL, id))objc_msgSend)(config.preferences, setSel, NO, f);
            return;
        }
    }
#pragma clang diagnostic pop
}

static id cc_space_swizzled_init(id self, SEL _cmd, CGRect frame, WKWebViewConfiguration *config) {
    if (config) cc_space_disable_sixty_clamp(config);
    return ((id (*)(id, SEL, CGRect, id))g_orig_init)(self, _cmd, frame, config);
}

void cc_space_install_high_refresh_unlock(void) {
    if (g_orig_init) return; // 幂等
    Method m = class_getInstanceMethod([WKWebView class], @selector(initWithFrame:configuration:));
    if (!m) return;
    g_orig_init = method_getImplementation(m);
    method_setImplementation(m, (IMP)cc_space_swizzled_init);
}
