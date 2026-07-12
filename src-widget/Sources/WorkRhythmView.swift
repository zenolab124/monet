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
                    Text("\(String(localized: "rhythm.peak")) \(peak):00")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(.tertiary)
                }
            }
            Spacer(minLength: 8)
            // 24h histogram — time-of-day gradient
            HStack(alignment: .bottom, spacing: 1) {
                ForEach(0..<24, id: \.self) { h in
                    let pct = maxH > 0 ? CGFloat(hours[h]) / CGFloat(maxH) : 0
                    let barColor = hours[h] > 0 ? Self.hourColor(h, intensity: pct, isPeak: h == peakHour) : Color.primary.opacity(0.08)
                    RoundedRectangle(cornerRadius: 1)
                        .fill(barColor)
                        .frame(height: hours[h] > 0 ? max(pct * 44, 2) : 1)
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
        .widgetURL(URL(string: "monet://home"))
    }

    private static func hourColor(_ hour: Int, intensity: CGFloat, isPeak: Bool) -> Color {
        // 深夜靛蓝 → 晨间青 → 午间蓝 → 傍晚紫 → 夜间靛蓝
        let hue: Double
        switch hour {
        case 0...4:   hue = 0.68   // indigo
        case 5...7:   hue = 0.52   // teal
        case 8...11:  hue = 0.55   // cyan-blue
        case 12...14: hue = 0.60   // blue
        case 15...17: hue = 0.62   // blue
        case 18...20: hue = 0.75   // purple
        default:      hue = 0.70   // indigo-purple
        }
        let sat = isPeak ? 0.8 : 0.6
        let brightness = isPeak ? 0.85 : 0.4 + Double(intensity) * 0.4
        return Color(hue: hue, saturation: sat, brightness: brightness)
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
        .widgetURL(URL(string: "monet://home"))
    }
}
