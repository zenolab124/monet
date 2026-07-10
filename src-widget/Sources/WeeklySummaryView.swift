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

    /// Convert "2026-07-02" → "7/2"
    private func shortDateMD(_ dateStr: String) -> String {
        let parts = dateStr.split(separator: "-")
        guard parts.count >= 3,
              let month = Int(parts[1]),
              let day = Int(parts[2]) else { return dateStr }
        return "\(month)/\(day)"
    }

    private func content(_ data: WidgetData) -> some View {
        let weekly = data.weeklyTokens ?? []
        let maxTokens = max(weekly.map(\.tokens).max() ?? 0, 1)

        return VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack(spacing: 5) {
                Image(systemName: "calendar.badge.clock")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.primary.opacity(0.6))
                Text("weekly.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text(data.weekDateRange)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }

            Spacer().frame(height: 6)

            // Hero row
            HStack {
                // Left: total tokens + trend
                HStack(alignment: .firstTextBaseline, spacing: 4) {
                    Text(WidgetData.formatTokens(data.weekTotalTokens))
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

                Spacer()

                // Right: compact stats
                HStack(spacing: 10) {
                    VStack(spacing: 0) {
                        Text("\(data.todaySessions)")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(.secondary)
                        Text("weekly.sessions")
                            .font(.system(size: 9))
                            .foregroundStyle(.tertiary)
                    }
                    VStack(spacing: 0) {
                        Text("\(data.weekActiveDays)/7")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(.secondary)
                        Text("weekly.activeDays")
                            .font(.system(size: 9))
                            .foregroundStyle(.tertiary)
                    }
                }
            }

            Spacer(minLength: 6)

            // Bar chart
            HStack(alignment: .bottom, spacing: 6) {
                ForEach(Array(weekly.enumerated()), id: \.offset) { idx, day in
                    let isPeak = day.tokens == maxTokens && day.tokens > 0
                    let isLast = idx == weekly.count - 1
                    let isZero = day.tokens == 0

                    VStack(spacing: 0) {
                        if isPeak {
                            Text(WidgetData.formatTokens(day.tokens))
                                .font(.system(size: 7, weight: .semibold))
                                .foregroundStyle(.secondary)
                                .padding(.bottom, 2)
                        }

                        RoundedRectangle(cornerRadius: 2.5)
                            .fill(
                                isLast ? Color.blue.opacity(0.7) :
                                isZero ? Color.primary.opacity(0.08) :
                                Color.primary.opacity(0.15)
                            )
                            .frame(height: isZero ? 1 : max(CGFloat(Double(day.tokens) / Double(maxTokens) * 54), 1))

                        Text(shortDateMD(day.date))
                            .font(.system(size: 7, weight: isLast ? .semibold : .medium, design: .monospaced))
                            .foregroundStyle(isLast ? AnyShapeStyle(Color.blue.opacity(0.7)) : AnyShapeStyle(.tertiary))
                            .padding(.top, 3)
                    }
                    .frame(maxWidth: .infinity)
                }
            }
            .frame(height: 68)
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
