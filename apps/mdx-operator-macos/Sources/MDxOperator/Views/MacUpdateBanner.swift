import SwiftUI

struct MacUpdateBanner: View {
  @Environment(OperatorStore.self) private var store

  var body: some View {
    if let release = store.availableMacRelease {
      HStack(spacing: 12) {
        Image(systemName: "arrow.down.circle.fill")
          .font(.title2)
          .foregroundStyle(Color.accentColor)
          .accessibilityHidden(true)

        VStack(alignment: .leading, spacing: 3) {
          Text("A newer notarized MDx is ready")
            .font(.headline)
          Text("Version \(release.version), build \(release.build). Download it from your private beta page.")
            .font(.callout)
            .foregroundStyle(.secondary)
        }

        Spacer(minLength: 12)

        Button("Download update") { store.openMacUpdate() }
          .mdxPrimaryButtonStyle()
        Button {
          store.dismissMacUpdate()
        } label: {
          Image(systemName: "xmark")
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Dismiss this update notice")
        .accessibilityLabel("Dismiss update notice")
      }
      .padding(14)
      .background(
        RoundedRectangle(cornerRadius: 14, style: .continuous)
          .fill(Color.accentColor.opacity(0.07))
      )
      .overlay(
        RoundedRectangle(cornerRadius: 14, style: .continuous)
          .stroke(Color.accentColor.opacity(0.24), lineWidth: 1)
      )
    }
  }
}
