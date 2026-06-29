import SwiftUI
import WidgetKit

struct WorkRhythmView: View {
    let entry: TodaySummaryEntry

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    private func content(_ data: WidgetData) -> some View {
        let hours = data.hourlyDistribution ?? Array(repeating: 0, count: 24)
        let maxH = hours.max() ?? 1
        let peakHour = hours.enumerated().max(by: { $0.element < $1.element })?.offset

        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 5) {
                Image(systemName: "clock.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.primary.opacity(0.6))
                Text("rhythm.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                if let peak = peakHour, hours[peak] > 0 {
                    Text("rhythm.peak \(peak):00")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(.tertiary)
                }
            }
            Spacer(minLength: 8)
            // 24h histogram
            HStack(alignment: .bottom, spacing: 1) {
                ForEach(0..<24, id: \.self) { h in
                    let pct = maxH > 0 ? CGFloat(hours[h]) / CGFloat(maxH) : 0
                    RoundedRectangle(cornerRadius: 1)
                        .fill(h == peakHour ? Color.blue.opacity(0.7) : Color.primary.opacity(0.15))
                        .frame(height: max(pct * 44, 1))
                }
            }
            .frame(height: 44)
            Spacer(minLength: 4)
            HStack {
                Text("0")
                    .font(.system(size: 8, weight: .medium, design: .monospaced))
                    .foregroundStyle(.tertiary)
                Spacer()
                Text("6")
                    .font(.system(size: 8, weight: .medium, design: .monospaced))
                    .foregroundStyle(.tertiary)
                Spacer()
                Text("12")
                    .font(.system(size: 8, weight: .medium, design: .monospaced))
                    .foregroundStyle(.tertiary)
                Spacer()
                Text("18")
                    .font(.system(size: 8, weight: .medium, design: .monospaced))
                    .foregroundStyle(.tertiary)
                Spacer()
                Text("24")
                    .font(.system(size: 8, weight: .medium, design: .monospaced))
                    .foregroundStyle(.tertiary)
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "clock")
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
