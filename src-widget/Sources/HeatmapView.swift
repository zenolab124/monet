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

        let today = Calendar.current.startOfDay(for: Date())
        let grid = buildCalendarGrid(heatmap: heatmap, today: today)
        let weekdayLabels = ["一", "二", "三", "四", "五", "六", "日"]

        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 5) {
                Image(systemName: "square.grid.3x3.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.primary.opacity(0.6))
                Text("heatmap.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text(monthLabel(heatmap))
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 4)

            HStack(spacing: 16) {
                VStack(alignment: .leading, spacing: 1) {
                    Text(data.formattedMonthlyTokens)
                        .font(.system(size: 24, weight: .bold, design: .rounded))
                    Text("heatmap.monthTokens")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(.tertiary)
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text("\(data.activeDays ?? 0)")
                        .font(.system(size: 24, weight: .bold, design: .rounded))
                    Text("heatmap.activeDays")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(.tertiary)
                }
                VStack(alignment: .leading, spacing: 1) {
                    Text("\(data.currentStreak ?? 0)")
                        .font(.system(size: 24, weight: .bold, design: .rounded))
                    Text("heatmap.streak")
                        .font(.system(size: 9, weight: .medium))
                        .foregroundStyle(.tertiary)
                }
                Spacer()
            }
            Spacer(minLength: 4)

            // Weekday header
            HStack(spacing: 2) {
                ForEach(0..<7, id: \.self) { col in
                    Text(weekdayLabels[col])
                        .font(.system(size: 8, weight: .medium, design: .monospaced))
                        .foregroundStyle(.tertiary)
                        .frame(maxWidth: .infinity)
                }
            }
            Spacer(minLength: 2)

            // Calendar grid
            VStack(spacing: 2) {
                ForEach(0..<grid.count, id: \.self) { row in
                    HStack(spacing: 2) {
                        ForEach(0..<7, id: \.self) { col in
                            let cell = grid[row][col]
                            if let cell = cell {
                                let isFuture = cell.date > todayString(today)
                                let level = isFuture ? -1 : cellLevel(cell.tokens, p25: p25, p50: p50, p75: p75, nonzeroCount: nonzero.count)
                                RoundedRectangle(cornerRadius: 2)
                                    .fill(levelColor(level))
                                    .aspectRatio(1, contentMode: .fit)
                            } else {
                                RoundedRectangle(cornerRadius: 2)
                                    .fill(Color.clear)
                                    .aspectRatio(1, contentMode: .fit)
                            }
                        }
                    }
                }
            }
            Spacer(minLength: 4)

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

    private func buildCalendarGrid(heatmap: [DayTokens], today: Date) -> [[DayTokens?]] {
        guard let firstDate = heatmap.first else { return [] }
        let cal = Calendar.current

        guard let date1 = dateFromString(firstDate.date) else { return [] }
        // 周一=1 ... 周日=7; 我们要周一在第 0 列
        var weekday = cal.component(.weekday, from: date1) // Sun=1, Mon=2, ...
        let offset = (weekday + 5) % 7 // Mon=0, Tue=1, ..., Sun=6

        let totalSlots = offset + heatmap.count
        let rows = (totalSlots + 6) / 7

        var grid = [[DayTokens?]](repeating: [DayTokens?](repeating: nil, count: 7), count: rows)
        for (i, day) in heatmap.enumerated() {
            let slot = offset + i
            let r = slot / 7
            let c = slot % 7
            grid[r][c] = day
        }
        return grid
    }

    private func dateFromString(_ s: String) -> Date? {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.date(from: s)
    }

    private func todayString(_ date: Date) -> String {
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd"
        return formatter.string(from: date)
    }

    private func monthLabel(_ heatmap: [DayTokens]) -> String {
        guard let first = heatmap.first, first.date.count >= 7 else { return "" }
        let idx = first.date.index(first.date.startIndex, offsetBy: 5)
        let endIdx = first.date.index(idx, offsetBy: 2)
        let month = String(first.date[idx..<endIdx])
        return "\(month)月"
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
        case -1: return .primary.opacity(0.06)  // future
        case 0: return .primary.opacity(0.1)     // no data
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
