import Foundation

struct ModelStat: Codable {
    let model: String
    let count: Int
    let tokens: UInt64
}

struct DayTokens: Codable {
    let date: String
    let tokens: UInt64
}

struct ProjectStat: Codable {
    let name: String
    let sessions: Int
}

struct WidgetData: Codable {
    let todaySessions: Int
    let todayTokens: UInt64
    let models: [String]
    let updatedAt: String

    let currentStreak: Int?
    let longestStreak: Int?
    let activeDays: Int?

    let monthlyTokens: UInt64?
    let lastMonthTokens: UInt64?
    let monthlyModels: [ModelStat]?

    let estimatedCostUsd: Double?

    let weeklyTokens: [DayTokens]?

    let activeProjectsToday: Int?
    let topProjects: [ProjectStat]?

    let hourlyDistribution: [Int]?

    let dailyHeatmap: [DayTokens]?

    let totalSessions: Int?
    let totalTokens: UInt64?

    static let placeholder = WidgetData(
        todaySessions: 3,
        todayTokens: 125_600,
        models: ["opus-4.6"],
        updatedAt: "",
        currentStreak: 5,
        longestStreak: 12,
        activeDays: 30,
        monthlyTokens: 2_500_000,
        lastMonthTokens: 2_000_000,
        monthlyModels: [
            ModelStat(model: "opus-4.6", count: 0, tokens: 1_500_000),
            ModelStat(model: "sonnet-4.6", count: 0, tokens: 800_000),
        ],
        estimatedCostUsd: 45.50,
        weeklyTokens: (0..<7).map { DayTokens(date: "", tokens: UInt64($0 * 50000 + 100000)) },
        activeProjectsToday: 2,
        topProjects: [
            ProjectStat(name: "my-project", sessions: 25),
            ProjectStat(name: "another", sessions: 12),
        ],
        hourlyDistribution: (0..<24).map { h in h >= 9 && h <= 18 ? 5 : 1 },
        dailyHeatmap: (0..<28).map { DayTokens(date: "", tokens: UInt64($0 * 30000)) },
        totalSessions: 150,
        totalTokens: 15_000_000
    )

    static func read() -> WidgetData? {
        let url = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("widget-data.json")
        guard let data = try? Data(contentsOf: url) else { return nil }
        return try? JSONDecoder().decode(WidgetData.self, from: data)
    }

    var formattedTokens: String {
        Self.formatTokens(todayTokens)
    }

    var formattedMonthlyTokens: String {
        Self.formatTokens(monthlyTokens ?? 0)
    }

    var monthlyTrendPercent: Int? {
        guard let last = lastMonthTokens, last > 0, let current = monthlyTokens else { return nil }
        return Int(((Double(current) - Double(last)) / Double(last)) * 100)
    }

    static func formatTokens(_ value: UInt64) -> String {
        if value >= 1_000_000 {
            return String(format: "%.1fM", Double(value) / 1_000_000)
        } else if value >= 1_000 {
            return String(format: "%.1fK", Double(value) / 1_000)
        }
        return "\(value)"
    }

    static func formatCost(_ value: Double) -> String {
        let formatter = NumberFormatter()
        formatter.numberStyle = .currency
        formatter.currencyCode = "USD"
        formatter.maximumFractionDigits = value >= 100 ? 0 : 2
        return formatter.string(from: NSNumber(value: value)) ?? String(format: "$%.0f", value)
    }

    static func shortDate(_ dateStr: String) -> String {
        guard dateStr.count >= 10 else { return dateStr }
        let idx = dateStr.index(dateStr.startIndex, offsetBy: 5)
        return String(dateStr[idx...])
    }
}
