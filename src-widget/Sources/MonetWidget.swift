import WidgetKit
import SwiftUI

@main
struct MonetWidgetBundle: WidgetBundle {
    var body: some Widget {
        TodaySummaryWidget()
        TokenWidget()
        StreakWidget()
        ActivityWidget()
    }
}

// MARK: - Today Summary: Small=今日概要, Medium=本周总结

struct TodaySummaryWidget: Widget {
    let kind = "TodaySummary"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            TodaySummaryMultiView(entry: entry)
                .containerBackground(.ultraThinMaterial, for: .widget)
        }
        .configurationDisplayName(Text("widget.displayName"))
        .description(Text("widget.description"))
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

struct TodaySummaryMultiView: View {
    let entry: TodaySummaryEntry
    @Environment(\.widgetFamily) var family
    var body: some View {
        switch family {
        case .systemMedium:
            WeeklySummaryView(entry: entry)
        default:
            TodaySummaryView(entry: entry)
        }
    }
}

// MARK: - Token: Small=脉搏, Medium=趋势

struct TokenWidget: Widget {
    let kind = "Token"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            TokenMultiView(entry: entry)
                .containerBackground(.ultraThinMaterial, for: .widget)
        }
        .configurationDisplayName(Text("pulse.displayName"))
        .description(Text("trend.description"))
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

struct TokenMultiView: View {
    let entry: TodaySummaryEntry
    @Environment(\.widgetFamily) var family
    var body: some View {
        switch family {
        case .systemMedium:
            TokenTrendView(entry: entry)
        default:
            TokenPulseView(entry: entry)
        }
    }
}

// MARK: - Streak: Small=连续天数, Medium=费用+模型

struct StreakWidget: Widget {
    let kind = "Streak"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            StreakMultiView(entry: entry)
                .containerBackground(.ultraThinMaterial, for: .widget)
        }
        .configurationDisplayName(Text("streak.displayName"))
        .description(Text("streak.description"))
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

struct StreakMultiView: View {
    let entry: TodaySummaryEntry
    @Environment(\.widgetFamily) var family
    var body: some View {
        switch family {
        case .systemMedium:
            ModelMixView(entry: entry)
        default:
            StreakView(entry: entry)
        }
    }
}

// MARK: - Activity: Small=费用, Medium=作息节奏, Large=热力图

struct ActivityWidget: Widget {
    let kind = "Activity"
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            ActivityMultiView(entry: entry)
                .containerBackground(.ultraThinMaterial, for: .widget)
        }
        .configurationDisplayName(Text("rhythm.displayName"))
        .description(Text("heatmap.description"))
        .supportedFamilies([.systemSmall, .systemMedium, .systemLarge])
    }
}

struct ActivityMultiView: View {
    let entry: TodaySummaryEntry
    @Environment(\.widgetFamily) var family
    var body: some View {
        switch family {
        case .systemMedium:
            WorkRhythmView(entry: entry)
        case .systemLarge:
            HeatmapView(entry: entry)
        default:
            CostView(entry: entry)
        }
    }
}
