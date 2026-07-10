import SwiftUI
import WidgetKit

struct ModelMixView: View {
    let entry: TodaySummaryEntry

    private let barColors: [Color] = [.blue, .orange, .green, .purple, .pink]

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    private func content(_ data: WidgetData) -> some View {
        let models = Array((data.monthlyModels ?? []).prefix(4))
        let totalTokens = models.reduce(0) { $0 + $1.tokens }

        return VStack(alignment: .leading, spacing: 0) {
            // Header
            HStack(spacing: 5) {
                Image(systemName: "cpu.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(Color.purple.opacity(0.7))
                Text("models.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text("models.badge")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }

            Spacer().frame(height: 6)

            // Hero dominant model
            if let dom = data.dominantModel {
                HStack(alignment: .firstTextBaseline, spacing: 5) {
                    Text(dom.name)
                        .font(.system(size: 18, weight: .bold))
                        .foregroundStyle(.primary)
                    Text("\(dom.percent)%")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Color.purple.opacity(0.7))
                    Text("主力模型")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(.tertiary)
                }
            }

            Spacer().frame(height: 8)

            // Stacked bar with gaps
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
                .clipShape(RoundedRectangle(cornerRadius: 4))
            }

            Spacer().frame(height: 10)

            // Legend list
            VStack(alignment: .leading, spacing: 6) {
                ForEach(Array(models.enumerated()), id: \.offset) { idx, m in
                    HStack(spacing: 6) {
                        Circle()
                            .fill(barColors[idx % barColors.count].opacity(0.7))
                            .frame(width: 6, height: 6)
                        Text(WidgetData.formatModelName(m.model))
                            .font(.system(size: 10, weight: .medium, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                        Spacer(minLength: 4)
                        Text(WidgetData.formatTokensCompact(m.tokens))
                            .font(.system(size: 10, weight: .medium, design: .monospaced))
                            .foregroundStyle(.tertiary)
                            .frame(width: 56, alignment: .trailing)
                        Text("\(data.modelPercent(m))%")
                            .font(.system(size: 10, weight: .medium, design: .monospaced))
                            .foregroundStyle(.tertiary)
                            .frame(width: 28, alignment: .trailing)
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
