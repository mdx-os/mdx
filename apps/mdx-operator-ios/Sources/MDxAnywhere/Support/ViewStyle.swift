import SwiftUI

extension View {
  func mobileCard() -> some View {
    self
      .padding(18)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
      .overlay {
        RoundedRectangle(cornerRadius: 22, style: .continuous)
          .stroke(.primary.opacity(0.07), lineWidth: 0.5)
      }
  }

  func mobileScrollContent() -> some View {
    self
      .padding(.horizontal, 16)
      .padding(.top, 8)
      .padding(.bottom, 72)
  }

  func mobileAdaptiveNavigationTitle(_ dynamicTypeSize: DynamicTypeSize) -> some View {
    self.navigationBarTitleDisplayMode(dynamicTypeSize.isAccessibilitySize ? .inline : .large)
  }

  func mobileFocusedDestination() -> some View {
    self.toolbar(.hidden, for: .tabBar)
  }
}

struct MobileEyebrow: View {
  let title: String
  let symbol: String
  var tint: Color = .accentColor

  var body: some View {
    Label(title.uppercased(), systemImage: symbol)
      .font(.caption.weight(.bold))
      .tracking(0.5)
      .foregroundStyle(tint)
      .dynamicTypeSize(...DynamicTypeSize.accessibility2)
  }
}

struct MobileSectionHeader: View {
  let title: String
  let detail: String?

  init(_ title: String, detail: String? = nil) {
    self.title = title
    self.detail = detail
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 3) {
      Text(title).font(.headline)
      if let detail {
        Text(detail).font(.subheadline).foregroundStyle(.secondary)
      }
    }
    .accessibilityElement(children: .combine)
  }
}

struct MobileEmptyState: View {
  let title: String
  let detail: String
  let symbol: String

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      Image(systemName: symbol)
        .font(.title2)
        .foregroundStyle(.tint)
      Text(title).font(.headline)
      Text(detail).font(.subheadline).foregroundStyle(.secondary)
    }
    .mobileCard()
    .accessibilityElement(children: .combine)
  }
}
