import SwiftUI
import WidgetKit

struct StreakView: View {
    let entry: TodaySummaryEntry

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    private func content(_ data: WidgetData) -> some View {
        let current = data.currentStreak ?? 0
        let longest = data.longestStreak ?? 0
        let ratio = min(Double(current) / Double(max(longest, 1)), 1.0)
        let remaining = longest - current

        return VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack(spacing: 5) {
                Image(systemName: "flame.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(Color.orange.opacity(0.8))
                Text("streak.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
            }

            Spacer(minLength: 2)

            // Hero streak number
            HStack(alignment: .firstTextBaseline, spacing: 4) {
                Text("\(current)")
                    .font(.system(size: 42, weight: .heavy, design: .rounded))
                    .foregroundStyle(Color.orange)
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
                Text("streak.days")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Color.orange.opacity(0.8))
            }

            Spacer().frame(height: 6)

            // Progress bar toward record
            HStack(spacing: 6) {
                GeometryReader { geo in
                    Capsule()
                        .fill(.tertiary.opacity(0.3))
                        .overlay(alignment: .leading) {
                            Capsule()
                                .fill(Color.orange.opacity(0.7))
                                .frame(width: max(geo.size.width * ratio, 4))
                        }
                }
                .frame(height: 5)

                Text("\(current)/\(longest)")
                    .font(.system(size: 9, weight: .semibold, design: .monospaced))
                    .foregroundStyle(.tertiary)
            }

            Spacer().frame(height: 2)

            // Motivational text
            if remaining > 0 {
                Text("距最长记录还差 \(remaining) 天")
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(.tertiary)
            } else {
                Text("新纪录进行中！")
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(Color.orange.opacity(0.7))
            }

            Spacer(minLength: 4)

            // Streak trail dots
            HStack(spacing: 4) {
                ForEach(0..<7, id: \.self) { i in
                    Circle()
                        .fill(Color.orange.opacity(0.8 - Double(i) * 0.08))
                        .frame(width: 12, height: 12)
                }
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "flame")
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
