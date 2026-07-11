import SwiftUI
import WidgetKit

struct MonthlyDashboardView: View {
    let entry: TodaySummaryEntry

    @Environment(\.colorScheme) private var colorScheme

    private var barColors: [Color] { WidgetPalette.categorical(colorScheme) }

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    private func content(_ data: WidgetData) -> some View {
        let weekly = data.weeklyTokens ?? []
        let maxT = weekly.map(\.tokens).max() ?? 1
        let models = Array((data.monthlyModels ?? []).prefix(4))
        let totalModelTokens = models.reduce(0) { $0 + $1.tokens }

        return VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack(spacing: 5) {
                Image(systemName: "chart.xyaxis.line")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.primary.opacity(0.6))
                Text("dashboard.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
            }
            Spacer(minLength: 6)

            // Top stats row
            HStack(spacing: 0) {
                statBlock(data.formattedMonthlyTokens, "dashboard.tokens", data.monthlyTrendPercent)
                Spacer()
                statBlock(WidgetData.formatCost(data.estimatedCostUsd ?? 0), "dashboard.cost", nil)
                Spacer()
                statBlock("\(data.currentStreak ?? 0)", "dashboard.streak", nil)
                Spacer()
                statBlock("\(data.totalSessions ?? 0)", "dashboard.sessions", nil)
            }
            Spacer(minLength: 10)

            // Weekly bar chart
            HStack(alignment: .bottom, spacing: 4) {
                ForEach(Array(weekly.enumerated()), id: \.offset) { idx, day in
                    VStack(spacing: 2) {
                        RoundedRectangle(cornerRadius: 2)
                            .fill(idx == weekly.count - 1 ? barColors[0] : Color.primary.opacity(0.15))
                            .frame(height: max(CGFloat(day.tokens) / CGFloat(maxT) * 50, 2))
                        Text(WidgetData.shortDate(day.date))
                            .font(.system(size: 7, weight: .medium, design: .monospaced))
                            .foregroundStyle(.tertiary)
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .frame(height: 64)
            Spacer(minLength: 10)

            // Model mix stacked bar + legend
            if totalModelTokens > 0 {
                GeometryReader { geo in
                    HStack(spacing: 2) {
                        ForEach(Array(models.enumerated()), id: \.offset) { idx, m in
                            let w = max(geo.size.width * Double(m.tokens) / Double(totalModelTokens), 3)
                            RoundedRectangle(cornerRadius: 2)
                                .fill(barColors[idx % barColors.count])
                                .frame(width: w)
                        }
                    }
                }
                .frame(height: 8)
                .clipShape(RoundedRectangle(cornerRadius: 3))
                Spacer(minLength: 6)
                HStack(spacing: 12) {
                    ForEach(Array(models.enumerated()), id: \.offset) { idx, m in
                        HStack(spacing: 3) {
                            Circle()
                                .fill(barColors[idx % barColors.count])
                                .frame(width: 5, height: 5)
                            Text(m.model)
                                .font(.system(size: 9, weight: .medium, design: .monospaced))
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                    Spacer()
                }
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private func statBlock(_ value: String, _ label: LocalizedStringKey, _ trend: Int?) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack(alignment: .firstTextBaseline, spacing: 2) {
                Text(value)
                    .font(.system(size: 18, weight: .bold, design: .rounded))
                    .minimumScaleFactor(0.6)
                    .lineLimit(1)
                if let pct = trend {
                    Text("\(pct >= 0 ? "↑" : "↓")\(abs(pct))%")
                        .font(.system(size: 8, weight: .medium))
                        .foregroundStyle(pct >= 0 ? .orange : .blue)
                }
            }
            Text(label)
                .font(.system(size: 9, weight: .medium))
                .foregroundStyle(.tertiary)
        }
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "chart.xyaxis.line")
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
