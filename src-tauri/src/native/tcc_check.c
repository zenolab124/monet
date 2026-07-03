// macOS TCC 权限静默检测（设置页权限体检）。
// 返回值约定：0=granted 1=denied 2=undetermined 3=target_not_running -1=error
// OSStatus 用数字字面量比较——这些值是 ABI 稳定的，避免依赖 SDK 枚举名差异。
#include <CoreServices/CoreServices.h>
#include <ApplicationServices/ApplicationServices.h>
#include <stdbool.h>
#include <string.h>

// 自动化（Apple Events）：ask=false 时纯查询零弹窗
int cc_space_ae_permission(const char *bundle_id, bool ask) {
    AEAddressDesc target;
    OSStatus err = AECreateDesc(typeApplicationBundleID, bundle_id,
                                strlen(bundle_id), &target);
    if (err != 0) return -1;
    OSStatus perm = AEDeterminePermissionToAutomateTarget(
        &target, typeWildCard, typeWildCard, ask);
    AEDisposeDesc(&target);
    switch (perm) {
        case 0:     return 0;  // noErr
        case -1743: return 1;  // errAEEventNotPermitted
        case -1744: return 2;  // errAEEventWouldRequireUserConsent
        case -600:  return 3;  // procNotFound（目标应用未运行）
        default:    return -1;
    }
}

int cc_space_ax_trusted(void) {
    return AXIsProcessTrusted() ? 0 : 1;
}

// 辅助功能授权引导：把本进程加入系统设置列表并弹出引导窗
int cc_space_ax_prompt(void) {
    CFStringRef keys[] = { kAXTrustedCheckOptionPrompt };
    CFBooleanRef values[] = { kCFBooleanTrue };
    CFDictionaryRef opts = CFDictionaryCreate(
        NULL, (const void **)keys, (const void **)values, 1,
        &kCFTypeDictionaryKeyCallBacks, &kCFTypeDictionaryValueCallBacks);
    Boolean trusted = AXIsProcessTrustedWithOptions(opts);
    CFRelease(opts);
    return trusted ? 0 : 1;
}

int cc_space_screen_preflight(void) {
    return CGPreflightScreenCaptureAccess() ? 0 : 1;
}

// 首次调用弹系统授权窗；已 denied 时不再弹（需深链系统设置）
int cc_space_screen_request(void) {
    return CGRequestScreenCaptureAccess() ? 0 : 1;
}
