import SwiftUI
import WidgetKit

struct HeatmapView: View {
    let entry: TodaySummaryEntry

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    private func content(_ data: WidgetData) -> some View {
        let heatmap = data.dailyHeatmap ?? []
        let nonzero = heatmap.map(\.tokens).filter { $0 > 0 }.sorted()
        let p25 = quantile(nonzero, 0.25)
        let p50 = quantile(nonzero, 0.50)
        let p75 = quantile(nonzero, 0.75)

        // 28 days = 4 weeks, arranged as 7 rows × 4 columns
        let grid: [[DayTokens]] = {
            var rows = [[DayTokens]]()
            for day in 0..<7 {
                var row = [DayTokens]()
                for week in 0..<4 {
                    let idx = week * 7 + day
                    if idx < heatmap.count {
                        row.append(heatmap[idx])
                    }
                }
                rows.append(row)
            }
            return rows
        }()

        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 5) {
                Image(systemName: "square.grid.3x3.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.primary.opacity(0.6))
                Text("heatmap.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text("heatmap.badge")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 4)

            // Stats row
            HStack(spacing: 16) {
                VStack(alignment: .leading, spacing: 1) {
                    Text(data.formattedMonthlyTokens)
                        .font(.system(size: 18, weight: .bold, design: .rounded))
                    Text("heatmap.monthTokens")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(.tertiary)
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text("\(data.activeDays ?? 0)")
                        .font(.system(size: 18, weight: .bold, design: .rounded))
                    Text("heatmap.activeDays")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(.tertiary)
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text("\(data.currentStreak ?? 0)")
                        .font(.system(size: 18, weight: .bold, design: .rounded))
                    Text("heatmap.streak")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(.tertiary)
                }
                Spacer()
            }
            Spacer(minLength: 8)

            // Heatmap grid
            VStack(spacing: 3) {
                ForEach(0..<grid.count, id: \.self) { rowIdx in
                    HStack(spacing: 3) {
                        ForEach(0..<grid[rowIdx].count, id: \.self) { colIdx in
                            let day = grid[rowIdx][colIdx]
                            let level = cellLevel(day.tokens, p25: p25, p50: p50, p75: p75, nonzeroCount: nonzero.count)
                            RoundedRectangle(cornerRadius: 2)
                                .fill(levelColor(level))
                                .aspectRatio(1, contentMode: .fit)
                        }
                    }
                }
            }
            Spacer(minLength: 6)

            // Legend
            HStack(spacing: 2) {
                Spacer()
                Text("heatmap.less")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(.tertiary)
                ForEach(0..<5, id: \.self) { l in
                    RoundedRectangle(cornerRadius: 1)
                        .fill(levelColor(l))
                        .frame(width: 8, height: 8)
                }
                Text("heatmap.more")
                    .font(.system(size: 8, weight: .medium))
                    .foregroundStyle(.tertiary)
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private func cellLevel(_ tokens: UInt64, p25: UInt64, p50: UInt64, p75: UInt64, nonzeroCount: Int) -> Int {
        if tokens == 0 { return 0 }
        if nonzeroCount < 4 { return 2 }
        if tokens <= p25 { return 1 }
        if tokens <= p50 { return 2 }
        if tokens <= p75 { return 3 }
        return 4
    }

    private func levelColor(_ level: Int) -> Color {
        switch level {
        case 0: return .primary.opacity(0.06)
        case 1: return .blue.opacity(0.2)
        case 2: return .blue.opacity(0.4)
        case 3: return .blue.opacity(0.6)
        default: return .blue.opacity(0.85)
        }
    }

    private func quantile(_ sorted: [UInt64], _ p: Double) -> UInt64 {
        guard !sorted.isEmpty else { return 0 }
        let idx = max(0, Int(ceil(p * Double(sorted.count))) - 1)
        return sorted[idx]
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "square.grid.3x3")
                .font(.system(size: 28, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
