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

    private func content(_ data: WidgetData) -> some View {
        let weekly = data.weeklyTokens ?? []
        let maxT = weekly.map(\.tokens).max() ?? 1

        return VStack(alignment: .leading, spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 5) {
                        Image(systemName: "chart.bar.fill")
                            .font(.system(size: 10, weight: .medium))
                            .foregroundStyle(.primary.opacity(0.6))
                        Text("trend.title")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(.primary.opacity(0.8))
                    }
                    HStack(alignment: .firstTextBaseline, spacing: 3) {
                        Text(data.formattedMonthlyTokens)
                            .font(.system(size: 22, weight: .bold, design: .rounded))
                        Text("trend.thisMonth")
                            .font(.system(size: 10, weight: .medium))
                            .foregroundStyle(.secondary)
                        if let pct = data.monthlyTrendPercent {
                            Text("\(pct >= 0 ? "↑" : "↓")\(abs(pct))%")
                                .font(.system(size: 10, weight: .medium))
                                .foregroundStyle(pct >= 0 ? .orange : .blue)
                        }
                    }
                }
                Spacer()
            }
            Spacer(minLength: 8)
            // Bar chart
            HStack(alignment: .bottom, spacing: 4) {
                ForEach(Array(weekly.enumerated()), id: \.offset) { idx, day in
                    VStack(spacing: 2) {
                        RoundedRectangle(cornerRadius: 2)
                            .fill(idx == weekly.count - 1 ? Color.blue.opacity(0.7) : Color.primary.opacity(0.2))
                            .frame(height: max(CGFloat(day.tokens) / CGFloat(maxT) * 40, 2))
                        Text(WidgetData.shortDate(day.date))
                            .font(.system(size: 8, weight: .medium, design: .monospaced))
                            .foregroundStyle(.tertiary)
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .frame(height: 56)
        }
        .widgetURL(URL(string: "ccspace://home"))
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
