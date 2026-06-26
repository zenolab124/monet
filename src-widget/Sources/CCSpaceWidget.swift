import WidgetKit
import SwiftUI

@main
struct CCSpaceWidgetBundle: WidgetBundle {
    var body: some Widget {
        TodaySummaryWidget()
    }
}

struct TodaySummaryWidget: Widget {
    let kind = "TodaySummary"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: kind, provider: TodaySummaryProvider()) { entry in
            TodaySummaryView(entry: entry)
                .containerBackground(for: .widget) {
                    ZStack {
                        Color.clear
                        Rectangle()
                            .fill(.ultraThinMaterial)
                    }
                }
        }
        .configurationDisplayName(Text("widget.displayName"))
        .description(Text("widget.description"))
        .supportedFamilies([.systemSmall])
    }
}
