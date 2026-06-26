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

    private func smallView(_ data: WidgetData) -> some View {
        ZStack {
            // 液态玻璃底层光斑
            Circle()
                .fill(.blue.opacity(0.08))
                .frame(width: 120, height: 120)
                .blur(radius: 30)
                .offset(x: -30, y: -20)
            Circle()
                .fill(.purple.opacity(0.06))
                .frame(width: 80, height: 80)
                .blur(radius: 25)
                .offset(x: 40, y: 30)

            VStack(alignment: .leading, spacing: 0) {
                header
                Spacer(minLength: 6)

                // 主数据区域 — 玻璃卡片
                VStack(alignment: .leading, spacing: 3) {
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
                }

                Spacer(minLength: 6)

                if let model = data.models.first {
                    Text(model)
                        .font(.system(size: 10, weight: .medium, design: .monospaced))
                        .foregroundStyle(.secondary)
                }
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private var header: some View {
        HStack(spacing: 5) {
            Image(systemName: "terminal.fill")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.primary.opacity(0.6))
            Text("widget.title")
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.primary.opacity(0.8))
            Spacer()
            Text("widget.today")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(.tertiary)
        }
    }

    private var emptyView: some View {
        ZStack {
            Circle()
                .fill(.blue.opacity(0.06))
                .frame(width: 100, height: 100)
                .blur(radius: 30)

            VStack(spacing: 8) {
                Image(systemName: "terminal.fill")
                    .font(.system(size: 22, weight: .light))
                    .foregroundStyle(.secondary)
                Text("widget.empty")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.secondary)
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
