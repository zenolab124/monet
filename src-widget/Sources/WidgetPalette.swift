import SwiftUI

/// 模型分布分类色板（亮暗双套，槽位顺序固定——同一模型跨视图同色）。
/// 等明度设计：OKLCH L 收在窄带（dark 0.60–0.66 / light 0.55–0.60），
/// 避免暗底上亮暖色的光渗错觉造成堆叠段"错位感"；填充直接用实色，
/// 不要再叠 opacity（会破坏验证过的对 surface 对比度）。
/// 换色前先过 dataviz 六检查（明度带/色度下限/CVD 分离/对比度）。
enum WidgetPalette {
    /// 分类槽位：琥珀 → 蓝 → 青绿 → 淡紫 → 玫红
    static func categorical(_ scheme: ColorScheme) -> [Color] {
        scheme == .dark ? dark : light
    }

    private static let dark: [Color] = [
        Color(hex: 0xC98500), Color(hex: 0x3987E5), Color(hex: 0x199E70),
        Color(hex: 0x9085E9), Color(hex: 0xD55181),
    ]
    private static let light: [Color] = [
        Color(hex: 0xB37A00), Color(hex: 0x2A78D6), Color(hex: 0x16966C),
        Color(hex: 0x6153C6), Color(hex: 0xD55181),
    ]
}

extension Color {
    /// 0xRRGGBB 字面量初始化（sRGB）
    init(hex: UInt32) {
        self.init(
            .sRGB,
            red: Double((hex >> 16) & 0xFF) / 255,
            green: Double((hex >> 8) & 0xFF) / 255,
            blue: Double(hex & 0xFF) / 255,
            opacity: 1
        )
    }
}
