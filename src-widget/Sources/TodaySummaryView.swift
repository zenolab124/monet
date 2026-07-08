import SwiftUI
import WidgetKit

struct TodaySummaryView: View {
    let entry: TodaySummaryEntry

    var body: some View {
        if let data = entry.data {
            smallView(data)
        } else {
            emptyView
        }
    }

    private var todayDateString: String {
        let f = DateFormatter()
        f.dateFormat = "M/d"
        return f.string(from: Date())
    }

    private func smallView(_ data: WidgetData) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 5) {
                Image(systemName: "terminal.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.primary.opacity(0.6))
                Text("widget.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text(todayDateString)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 6)

            HStack(alignment: .firstTextBaseline, spacing: 3) {
                Text("\(data.todaySessions)")
                    .font(.system(size: 28, weight: .bold, design: .rounded))
                    .foregroundStyle(.primary)
                Text("widget.sessions")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            HStack(alignment: .firstTextBaseline, spacing: 3) {
                Text(data.formattedTokens)
                    .font(.system(size: 18, weight: .semibold, design: .rounded))
                    .foregroundStyle(.primary.opacity(0.85))
                Text("widget.tokens")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 6)

            if let model = data.models.first {
                Text(WidgetData.formatModelName(model))
                    .font(.system(size: 10, weight: .medium, design: .monospaced))
                    .foregroundStyle(.secondary)
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "terminal.fill")
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
