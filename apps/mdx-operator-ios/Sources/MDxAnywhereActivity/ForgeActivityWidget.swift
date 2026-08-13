import ActivityKit
import MDxOperatorShared
import SwiftUI
import WidgetKit

struct ForgeActivityWidget: Widget {
  var body: some WidgetConfiguration {
    ActivityConfiguration(for: ForgeActivityAttributes.self) { context in
      HStack {
        Image(
          systemName: context.state.needsUser
            ? "person.crop.circle.badge.exclamationmark" : "hammer.fill")
        VStack(alignment: .leading) {
          Text(context.attributes.repositoryLabel).font(.headline)
          Text(context.state.safeSummary).font(.caption).foregroundStyle(.secondary)
        }
      }
      .padding()
      .activityBackgroundTint(.black.opacity(0.08))
    } dynamicIsland: { context in
      DynamicIsland {
        DynamicIslandExpandedRegion(.leading) { Image(systemName: "hammer.fill") }
        DynamicIslandExpandedRegion(.center) { Text(context.state.safeSummary).lineLimit(2) }
        DynamicIslandExpandedRegion(.bottom) { Text(context.state.stage).font(.caption) }
      } compactLeading: {
        Image(systemName: "hammer.fill")
      } compactTrailing: {
        Image(systemName: context.state.needsUser ? "person.fill.questionmark" : "ellipsis")
      } minimal: {
        Image(systemName: "hammer.fill")
      }
    }
  }
}

@main
struct MDxAnywhereActivityBundle: WidgetBundle {
  var body: some Widget {
    ForgeActivityWidget()
  }
}
