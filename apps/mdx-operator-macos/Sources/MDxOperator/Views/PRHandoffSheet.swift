import SwiftUI

/// The kernel composes a PR handoff FROM the run's receipts: title, narrative
/// body, review checklist, and honest delivery flags. This sheet is the last
/// mile: read it, copy it (or open the compare page), and the handoff receipt
/// is already on the ledger. Remote push and PR-open stay visibly closed
/// until that authority is actually granted.
struct PRHandoffSheet: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.dismiss) private var dismiss

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack(alignment: .firstTextBaseline) {
        Text("PR handoff")
          .font(.title2.weight(.semibold))
        Spacer()
        if let handoff = store.prHandoff, !handoff.receiptID.isEmpty {
          Text("Receipt \(handoff.receiptID)")
            .font(.caption)
            .foregroundStyle(.tertiary)
        }
      }

      switch store.prHandoffPhase {
      case .loading, .idle:
        HStack(spacing: 8) {
          ProgressView().controlSize(.small)
          Text("Composing the handoff from this run's receipts…")
            .font(.callout)
            .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, minHeight: 120)
      case .failed(let message), .stale(let message):
        Text(message)
          .font(.callout)
          .foregroundStyle(.secondary)
          .frame(maxWidth: .infinity, minHeight: 120)
      case .loaded, .empty:
        if let handoff = store.prHandoff {
          handoffBody(handoff)
        }
      }

      HStack(spacing: 10) {
        Spacer()
        Button("Close") { dismiss() }
          .keyboardShortcut(.cancelAction)
      }
    }
    .padding(20)
    .frame(minWidth: 620, idealWidth: 680, minHeight: 480, idealHeight: 560)
  }

  @ViewBuilder
  private func handoffBody(_ handoff: PRHandoff) -> some View {
    VStack(alignment: .leading, spacing: 10) {
      Text(handoff.title)
        .font(.headline)

      HStack(spacing: 8) {
        if !handoff.branch.isEmpty {
          Label(handoff.branch, systemImage: "arrow.triangle.branch")
            .font(.caption.monospaced())
            .foregroundStyle(.secondary)
        }
        StatusPill(status: handoff.reviewStatus.replacingOccurrences(of: "_", with: " "), tone: .neutral)
        Spacer(minLength: 0)
      }

      ScrollView {
        MarkdownText(text: handoff.bodyMarkdown)
          .frame(maxWidth: .infinity, alignment: .leading)
          .padding(12)
      }
      .frame(minHeight: 180, maxHeight: 300)
      .background(
        RoundedRectangle(cornerRadius: 9, style: .continuous)
          .fill(Color.primary.opacity(0.03))
      )

      if !handoff.pullRequestOpenAllowed {
        Label(
          handoff.requiresOperatorAction && !handoff.liveDeliveryRoute.isEmpty
            ? "Opening the PR is an explicit operator step. This draft stays dry-run; live delivery goes through \(handoff.liveDeliveryRoute) once readiness passes."
            : "Opening the PR stays a human step: push authority is closed on this kernel. Copy the description, open the compare page, and paste.",
          systemImage: "lock.shield"
        )
        .font(.caption)
        .foregroundStyle(.tertiary)
      }

      HStack(spacing: 10) {
        Button {
          Pasteboard.copy(handoff.bodyMarkdown)
        } label: {
          Label("Copy PR description", systemImage: "doc.on.doc")
        }
        Button {
          Pasteboard.copy(handoff.branch)
        } label: {
          Label("Copy branch", systemImage: "arrow.triangle.branch")
        }
        .disabled(handoff.branch.isEmpty)
        Spacer(minLength: 0)
        if let compare = compareURL(handoff) {
          Button {
            NSWorkspace.shared.open(compare)
          } label: {
            Label("Open compare on GitHub", systemImage: "arrow.up.forward.app")
          }
          .mdxPrimaryButtonStyle()
        }
      }
    }
  }

  private func compareURL(_ handoff: PRHandoff) -> URL? {
    guard let origin = store.selectedRunRepo?.webOrigin, !handoff.branch.isEmpty else { return nil }
    let branch = handoff.branch.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? handoff.branch
    return URL(string: "\(origin)/compare/\(branch)?expand=1")
  }
}
