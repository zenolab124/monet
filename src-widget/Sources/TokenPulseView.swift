import SwiftUI
import WidgetKit

struct TokenPulseView: View {
    let entry: TodaySummaryEntry

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    private func content(_ data: WidgetData) -> some View {
        let monthly = data.monthlyTokens ?? 1
        let ratio = monthly > 0 ? min(Double(data.todayTokens) / Double(monthly), 1.0) : 0

        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 5) {
                Image(systemName: "bolt.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.yellow.opacity(0.8))
                Text("pulse.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
            }
            Spacer(minLength: 6)
            HStack(alignment: .firstTextBaseline, spacing: 3) {
                Text(data.formattedTokens)
                    .font(.system(size: 28, weight: .bold, design: .rounded))
                    .foregroundStyle(.primary)
                    .minimumScaleFactor(0.6)
                    .lineLimit(1)
                Text("widget.tokens")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            // Progress bar
            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    Capsule()
                        .fill(.tertiary.opacity(0.3))
                        .frame(height: 6)
                    Capsule()
                        .fill(.blue.opacity(0.7))
                        .frame(width: max(geo.size.width * ratio, 4), height: 6)
                }
            }
            .frame(height: 6)
            HStack {
                Text("pulse.ofMonthly")
                    .font(.system(size: 9, weight: .medium))
                    .foregroundStyle(.tertiary)
                Spacer()
                Text(data.formattedMonthlyTokens)
                    .font(.system(size: 9, weight: .medium, design: .monospaced))
                    .foregroundStyle(.tertiary)
            }
            .padding(.top, 3)
        }
        .widgetURL(URL(string: "ccspace://home"))
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
        .widgetURL(URL(string: "ccspace://home"))
    }
}
