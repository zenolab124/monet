import SwiftUI
import WidgetKit

struct ActiveProjectsView: View {
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
            HStack(spacing: 5) {
                Image(systemName: "folder.fill")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.blue.opacity(0.7))
                Text("projects.title")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.primary.opacity(0.8))
                Spacer()
            }
            Spacer(minLength: 6)
            HStack(alignment: .firstTextBaseline, spacing: 3) {
                Text("\(data.activeProjectsToday ?? 0)")
                    .font(.system(size: 36, weight: .bold, design: .rounded))
                    .foregroundStyle(.primary)
                Text("projects.activeToday")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 6)
            if let top = data.topProjects?.prefix(3), !top.isEmpty {
                VStack(alignment: .leading, spacing: 2) {
                    ForEach(Array(top.enumerated()), id: \.offset) { _, proj in
                        HStack(spacing: 4) {
                            Text(proj.name)
                                .font(.system(size: 10, weight: .medium))
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                            Spacer()
                            Text("\(proj.sessions)")
                                .font(.system(size: 10, weight: .medium, design: .monospaced))
                                .foregroundStyle(.tertiary)
                        }
                    }
                }
            }
        }
        .widgetURL(URL(string: "monet://home"))
    }

    private var emptyView: some View {
        VStack(spacing: 8) {
            Image(systemName: "folder")
                .font(.system(size: 22, weight: .light))
                .foregroundStyle(.secondary)
            Text("widget.empty")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(.secondary)
        }
        .widgetURL(URL(string: "monet://home"))
    }
}
