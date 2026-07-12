import SwiftUI
import WidgetKit

struct TokenPulseView: View {
    let entry: TodaySummaryEntry

    private var todayDateString: String {
        let f = DateFormatter()
        f.dateFormat = "M/d"
        return f.string(from: Date())
    }

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    private func content(_ data: WidgetData) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack(spacing: 5) {
                Image(systemName: "bolt.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.yellow.opacity(0.8))
                Text("Token")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text(todayDateString)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }

            Spacer(minLength: 4)

            // Hero
            Text(data.formattedTokens)
                .font(.system(size: 36, weight: .bold, design: .rounded))
                .foregroundStyle(.primary)
                .minimumScaleFactor(0.6)
                .lineLimit(1)

            Spacer(minLength: 6)

            // Multiplier row
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(String(format: "%.1f\u{00D7}", data.todayMultiplier))
                    .font(.system(size: 20, weight: .bold, design: .rounded))
                    .foregroundStyle(Color.orange)
                Text("日均")
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(.tertiary)
            }

            Spacer().frame(height: 2)

            // Average value
            Text("avg " + WidgetData.formatTokens(data.dailyAverage) + "/天")
                .font(.system(size: 10, weight: .medium, design: .monospaced))
                .foregroundStyle(.tertiary)
        }
        .widgetURL(URL(string: "monet://home"))
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "bolt")
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "monet://home"))
    }
}
