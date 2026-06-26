import WidgetKit

struct TodaySummaryEntry: TimelineEntry {
    let date: Date
    let data: WidgetData?
}

struct TodaySummaryProvider: TimelineProvider {
    func placeholder(in context: Context) -> TodaySummaryEntry {
        TodaySummaryEntry(date: .now, data: .placeholder)
    }

    func getSnapshot(in context: Context, completion: @escaping (TodaySummaryEntry) -> Void) {
        completion(TodaySummaryEntry(date: .now, data: WidgetData.read() ?? .placeholder))
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<TodaySummaryEntry>) -> Void) {
        let entry = TodaySummaryEntry(date: .now, data: WidgetData.read())
        let next = Calendar.current.date(byAdding: .minute, value: 15, to: .now)!
        completion(Timeline(entries: [entry], policy: .after(next)))
    }
}
