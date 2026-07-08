import SwiftUI
import WidgetKit

struct CostView: View {
    let entry: TodaySummaryEntry

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
                Image(systemName: "dollarsign.circle.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(Color.green.opacity(0.7))
                Text("cost.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text("cost.badge")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }

            Spacer(minLength: 4)

            // Hero approximate cost
            HStack(alignment: .firstTextBaseline, spacing: 2) {
                Text("≈")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.tertiary)
                Text("$\(Int((data.estimatedCostUsd ?? 0).rounded()))")
                    .font(.system(size: 38, weight: .bold, design: .rounded))
                    .foregroundStyle(.primary)
                    .minimumScaleFactor(0.5)
                    .lineLimit(1)
            }

            Spacer().frame(height: 4)

            Spacer(minLength: 4)

            // Disclaimer
            Text("cost.disclaimer")
                .font(.system(size: 8, weight: .medium))
                .foregroundStyle(.tertiary.opacity(0.7))
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "dollarsign.circle")
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
