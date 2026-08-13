import SwiftUI

struct RootTabView: View {
  @State private var selectedTab = 0

  var body: some View {
    TabView(selection: $selectedTab) {
      NowView(
        onStartBuild: { selectedTab = 1 },
        onReview: { selectedTab = 2 }
      )
      .tabItem { Label("Now", systemImage: "sparkles") }
      .tag(0)
      .accessibilityIdentifier("tab.now")

      ForgeView()
        .tabItem { Label("Forge", systemImage: "hammer") }
        .tag(1)
        .accessibilityIdentifier("tab.forge")

      ReviewView()
        .tabItem { Label("Review", systemImage: "checkmark.seal") }
        .tag(2)
        .accessibilityIdentifier("tab.review")

      YouView()
        .tabItem { Label("You", systemImage: "person.crop.circle") }
        .tag(3)
        .accessibilityIdentifier("tab.you")
    }
    .tint(.accentColor)
  }
}
