import SwiftUI
import WidgetKit

struct TokenTrendView: View {
    let entry: TodaySummaryEntry

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    /// Convert "2026-07-02" → "7/2"
    private func shortDateMD(_ dateStr: String) -> String {
        let parts = dateStr.split(separator: "-")
        guard parts.count >= 3,
              let month = Int(parts[1]),
              let day = Int(parts[2]) else { return dateStr }
        return "\(month)/\(day)"
    }

    private func tickPositions(count: Int) -> [Int] {
        guard count > 1 else { return [0] }
        let last = count - 1
        return [0, last / 4, last / 2, last * 3 / 4, last]
    }

    private func content(_ data: WidgetData) -> some View {
        let days: [DayTokens]
        if let heatmap = data.dailyHeatmap, !heatmap.isEmpty {
            days = heatmap
        } else {
            days = data.weeklyTokens ?? []
        }
        let maxTokens = max(days.map(\.tokens).max() ?? 0, 1)
        let ticks = tickPositions(count: days.count)
        let todayStr: String = {
            let f = DateFormatter()
            f.dateFormat = "yyyy-MM-dd"
            return f.string(from: Date())
        }()

        return VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack(spacing: 5) {
                Image(systemName: "chart.bar.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.primary.opacity(0.6))
                Text("trend.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text("本月")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }

            Spacer().frame(height: 4)

            // Hero
            HStack(alignment: .firstTextBaseline, spacing: 4) {
                Text(data.formattedMonthlyTokens)
                    .font(.system(size: 24, weight: .bold, design: .rounded))
                    .foregroundStyle(.primary)
                    .minimumScaleFactor(0.6)
                    .lineLimit(1)
                Text("tokens")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
                if let pct = data.monthlyTrendPercent {
                    Text("\(pct >= 0 ? "↑" : "↓") \(abs(pct))%")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(pct >= 0 ? .orange : .blue)
                }
            }

            Spacer(minLength: 6)

            // 30-day bar chart
            if !days.isEmpty {
                HStack(alignment: .bottom, spacing: 1.5) {
                    ForEach(Array(days.enumerated()), id: \.offset) { idx, day in
                        let isToday = day.date == todayStr
                        let isZero = day.tokens == 0

                        RoundedRectangle(cornerRadius: 1.5)
                            .fill(
                                isToday ? Color.blue.opacity(0.7) :
                                isZero ? Color.primary.opacity(0.08) :
                                Color.primary.opacity(0.15)
                            )
                            .frame(maxWidth: .infinity)
                            .frame(height: isZero ? 1 : max(CGFloat(Double(day.tokens) / Double(maxTokens) * 54), 1))
                    }
                }
                .frame(height: 64)

                // Date ticks
                if days.count > 1 {
                    dateTicks(days: days, ticks: ticks, todayStr: todayStr)
                }
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private func dateTicks(days: [DayTokens], ticks: [Int], todayStr: String) -> some View {
        HStack {
            ForEach(Array(ticks.enumerated()), id: \.offset) { i, tickIdx in
                if i > 0 {
                    Spacer()
                }
                let isToday = days[tickIdx].date == todayStr
                Text(shortDateMD(days[tickIdx].date))
                    .font(.system(size: 7, weight: isToday ? .semibold : .medium, design: .monospaced))
                    .foregroundStyle(isToday ? AnyShapeStyle(Color.blue.opacity(0.7)) : AnyShapeStyle(.tertiary))
            }
        }
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "chart.bar")
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
