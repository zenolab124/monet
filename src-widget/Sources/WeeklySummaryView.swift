import SwiftUI
import WidgetKit

struct WeeklySummaryView: View {
    let entry: TodaySummaryEntry

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    private func content(_ data: WidgetData) -> some View {
        let weekly = data.weeklyTokens ?? []
        let weekTokens = weekly.reduce(0) { $0 + $1.tokens }
        let activeDays = weekly.filter { $0.tokens > 0 }.count

        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 5) {
                Image(systemName: "calendar.badge.clock")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.primary.opacity(0.6))
                Text("weekly.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text("weekly.badge")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 8)
            HStack(spacing: 16) {
                VStack(alignment: .leading, spacing: 2) {
                    Text("weekly.tokens")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(.tertiary)
                    Text(WidgetData.formatTokens(weekTokens))
                        .font(.system(size: 20, weight: .bold, design: .rounded))
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text("weekly.sessions")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(.tertiary)
                    Text("\(data.todaySessions)")
                        .font(.system(size: 20, weight: .bold, design: .rounded))
                }
                VStack(alignment: .leading, spacing: 2) {
                    Text("weekly.activeDays")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(.tertiary)
                    Text("\(activeDays)/7")
                        .font(.system(size: 20, weight: .bold, design: .rounded))
                }
                Spacer()
            }
            Spacer(minLength: 8)
            // Mini sparkline
            if !weekly.isEmpty {
                let maxT = weekly.map(\.tokens).max() ?? 1
                HStack(alignment: .bottom, spacing: 3) {
                    ForEach(Array(weekly.enumerated()), id: \.offset) { _, day in
                        let h = maxT > 0 ? max(CGFloat(day.tokens) / CGFloat(maxT) * 20, 1) : 1
                        RoundedRectangle(cornerRadius: 1.5)
                            .fill(day.tokens > 0 ? Color.blue.opacity(0.5) : Color.primary.opacity(0.08))
                            .frame(height: h)
                    }
                }
                .frame(height: 20)
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "calendar")
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
