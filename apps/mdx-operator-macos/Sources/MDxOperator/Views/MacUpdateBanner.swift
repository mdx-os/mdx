import SwiftUI

struct MacUpdateBanner: View {
  @Environment(MacUpdaterController.self) private var updater

  var body: some View {
    if let release = updater.availableUpdate {
      HStack(spacing: 12) {
        Image(systemName: "arrow.down.circle.fill")
          .font(.title2)
          .foregroundStyle(Color.accentColor)
          .accessibilityHidden(true)

        VStack(alignment: .leading, spacing: 3) {
          Text("A newer notarized MDx is ready")
            .font(.headline)
          Text("Version \(release.version), build \(release.build). MDx can install it and reopen when ready.")
            .font(.callout)
            .foregroundStyle(.secondary)
        }

        Spacer(minLength: 12)

        Button("Install update") { Task { await updater.installAvailableUpdate() } }
          .mdxPrimaryButtonStyle()
        Button {
          updater.dismissAvailableUpdate()
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
