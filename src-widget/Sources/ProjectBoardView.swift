import SwiftUI
import WidgetKit

struct ProjectBoardView: View {
    let entry: TodaySummaryEntry

    var body: some View {
        if let data = entry.data {
            content(data)
        } else {
            emptyView
        }
    }

    private func content(_ data: WidgetData) -> some View {
        let projects = Array((data.topProjects ?? []).prefix(5))
        let maxS = projects.map(\.sessions).max() ?? 1

        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 5) {
                Image(systemName: "list.number")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.primary.opacity(0.6))
                Text("board.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
                Text("\(data.totalSessions ?? 0) \(NSLocalizedString("board.total", comment: ""))")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 6)
            VStack(alignment: .leading, spacing: 5) {
                ForEach(Array(projects.enumerated()), id: \.offset) { idx, proj in
                    HStack(spacing: 6) {
                        Text("\(idx + 1)")
                            .font(.system(size: 10, weight: .bold, design: .rounded))
                            .foregroundStyle(.tertiary)
                            .frame(width: 14)
                        Text(proj.name)
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(.primary.opacity(0.8))
                            .lineLimit(1)
                        Spacer()
                        GeometryReader { geo in
                            RoundedRectangle(cornerRadius: 2)
                                .fill(.blue.opacity(0.25))
                                .frame(width: max(geo.size.width * Double(proj.sessions) / Double(maxS), 4))
                                .frame(maxWidth: .infinity, alignment: .trailing)
                        }
                        .frame(width: 60, height: 8)
                        Text("\(proj.sessions)")
                            .font(.system(size: 10, weight: .medium, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .frame(width: 28, alignment: .trailing)
                    }
                }
            }
        }
        .widgetURL(URL(string: "ccspace://home"))
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "list.number")
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "ccspace://home"))
    }
}
