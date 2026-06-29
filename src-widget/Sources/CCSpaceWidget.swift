import WidgetKit
import SwiftUI

@main
struct CCSpaceWidgetBundle: WidgetBundle {
    var body: some Widget {
        // Small
        TodaySummaryWidget()
        StreakWidget()
        CostWidget()
        TokenPulseWidget()
        ActiveProjectsWidget()
        // Medium
        TokenTrendWidget()
        ModelMixWidget()
        ProjectBoardWidget()
        WorkRhythmWidget()
        WeeklySummaryWidget()
        // Large
        MonthlyDashboardWidget()
        HeatmapWidget()
    }
}

// MARK: - Small Widgets

struct TodaySummaryWidget: Widget {
    let kind = "TodaySummary"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            TodaySummaryView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("widget.displayName"))
        .description(Text("widget.description"))
        .supportedFamilies([.systemSmall])
    }
}

struct StreakWidget: Widget {
    let kind = "Streak"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            StreakView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("streak.displayName"))
        .description(Text("streak.description"))
        .supportedFamilies([.systemSmall])
    }
}

struct CostWidget: Widget {
    let kind = "Cost"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            CostView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("cost.displayName"))
        .description(Text("cost.description"))
        .supportedFamilies([.systemSmall])
    }
}

struct TokenPulseWidget: Widget {
    let kind = "TokenPulse"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            TokenPulseView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("pulse.displayName"))
        .description(Text("pulse.description"))
        .supportedFamilies([.systemSmall])
    }
}

struct ActiveProjectsWidget: Widget {
    let kind = "ActiveProjects"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            ActiveProjectsView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("projects.displayName"))
        .description(Text("projects.description"))
        .supportedFamilies([.systemSmall])
    }
}

// MARK: - Medium Widgets

struct TokenTrendWidget: Widget {
    let kind = "TokenTrend"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            TokenTrendView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("trend.displayName"))
        .description(Text("trend.description"))
        .supportedFamilies([.systemMedium])
    }
}

struct ModelMixWidget: Widget {
    let kind = "ModelMix"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            ModelMixView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("models.displayName"))
        .description(Text("models.description"))
        .supportedFamilies([.systemMedium])
    }
}

struct ProjectBoardWidget: Widget {
    let kind = "ProjectBoard"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            ProjectBoardView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("board.displayName"))
        .description(Text("board.description"))
        .supportedFamilies([.systemMedium])
    }
}

struct WorkRhythmWidget: Widget {
    let kind = "WorkRhythm"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            WorkRhythmView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("rhythm.displayName"))
        .description(Text("rhythm.description"))
        .supportedFamilies([.systemMedium])
    }
}

struct WeeklySummaryWidget: Widget {
    let kind = "WeeklySummary"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            WeeklySummaryView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("weekly.displayName"))
        .description(Text("weekly.description"))
        .supportedFamilies([.systemMedium])
    }
}

// MARK: - Large Widgets

struct MonthlyDashboardWidget: Widget {
    let kind = "MonthlyDashboard"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            MonthlyDashboardView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("dashboard.displayName"))
        .description(Text("dashboard.description"))
        .supportedFamilies([.systemLarge])
    }
}

struct HeatmapWidget: Widget {
    let kind = "Heatmap"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            HeatmapView(entry: entry)
                .containerBackground(for: .widget) { Color.clear }
        }
        .configurationDisplayName(Text("heatmap.displayName"))
        .description(Text("heatmap.description"))
        .supportedFamilies([.systemLarge])
    }
}
