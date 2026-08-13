import SwiftUI

/// Content cards sit on a calm flat surface on every OS version. Liquid
/// Glass is a CHROME material (sidebar, toolbars, palette, menu bar window);
/// Apple's adoption guidance, and our own visual law, keep it off stacked
/// content cards where it reads as noise and costs compositing.
struct MDxGlassSurface: ViewModifier {
  let interactive: Bool

  func body(content: Content) -> some View {
    content
      .background(Color(nsColor: .quaternarySystemFill).opacity(0.55))
      .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
      .overlay(
        RoundedRectangle(cornerRadius: 11, style: .continuous)
          .stroke(Color.secondary.opacity(0.12), lineWidth: 1)
      )
  }
}

extension View {
  func mdxGlassSurface(interactive: Bool = false) -> some View {
    modifier(MDxGlassSurface(interactive: interactive))
  }

  @ViewBuilder
  func mdxPrimaryButtonStyle() -> some View {
    if #available(macOS 26.0, *) {
      buttonStyle(.glassProminent)
    } else {
      buttonStyle(.borderedProminent)
    }
  }

  /// One prominent verb per state: the state's primary action gets the filled
  /// treatment, every other verb stays a quiet bordered control beside it.
  @ViewBuilder
  func mdxActionEmphasis(_ isPrimary: Bool) -> some View {
    if isPrimary {
      mdxPrimaryButtonStyle()
    } else {
      buttonStyle(.bordered)
    }
  }
}
