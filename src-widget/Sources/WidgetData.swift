import Foundation

struct WidgetData: Codable {
    let todaySessions: Int
    let todayTokens: UInt64
    let models: [String]
    let updatedAt: String

    static let placeholder = WidgetData(
        todaySessions: 3,
        todayTokens: 125_600,
        models: ["opus-4.6"],
        updatedAt: ""
    )

    static func read() -> WidgetData? {
        // 沙箱内 homeDirectory 指向 ~/Library/Containers/<bundle-id>/Data/
        let url = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("widget-data.json")
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(WidgetData.self, from: data)
    }

    var formattedTokens: String {
        if todayTokens >= 1_000_000 {
            return String(format: "%.1fM", Double(todayTokens) / 1_000_000)
        } else if todayTokens >= 1_000 {
            return String(format: "%.1fK", Double(todayTokens) / 1_000)
        }
        return "\(todayTokens)"
    }
}
