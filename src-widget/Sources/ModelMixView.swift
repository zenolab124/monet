import SwiftUI
import WidgetKit

struct ModelMixView: View {
    let entry: TodaySummaryEntry

    private let barColors: [Color] = [.blue, .purple, .orange, .green, .pink]

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    private func content(_ data: WidgetData) -> some View {
        let models = Array((data.monthlyModels ?? []).prefix(5))
        let totalTokens = models.reduce(0) { $0 + $1.tokens }

        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 5) {
                Image(systemName: "cpu.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.purple.opacity(0.7))
                Text("models.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text("models.badge")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 8)
            // Stacked bar
            if totalTokens > 0 {
                GeometryReader { geo in
                    HStack(spacing: 1) {
                        ForEach(Array(models.enumerated()), id: \.offset) { idx, m in
                            let w = max(geo.size.width * Double(m.tokens) / Double(totalTokens), 3)
                            RoundedRectangle(cornerRadius: 2)
                                .fill(barColors[idx % barColors.count].opacity(0.7))
                                .frame(width: w)
                        }
                    }
                }
                .frame(height: 10)
                .clipShape(RoundedRectangle(cornerRadius: 3))
            }
            Spacer(minLength: 8)
            // Legend
            VStack(alignment: .leading, spacing: 3) {
                ForEach(Array(models.enumerated()), id: \.offset) { idx, m in
                    HStack(spacing: 6) {
                        Circle()
                            .fill(barColors[idx % barColors.count].opacity(0.7))
                            .frame(width: 6, height: 6)
                        Text(m.model)
                            .font(.system(size: 10, weight: .medium, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                        Spacer()
                        Text(WidgetData.formatTokens(m.tokens))
                            .font(.system(size: 10, weight: .medium, design: .monospaced))
                            .foregroundStyle(.tertiary)
                    }
                }
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "cpu")
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
