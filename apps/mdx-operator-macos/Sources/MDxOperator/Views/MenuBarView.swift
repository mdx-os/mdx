import AppKit
import SwiftUI

struct MenuBarLabel: View {
  var store: OperatorStore

  var body: some View {
    HStack(spacing: 4) {
      Image(systemName: store.runningRunCount > 0 ? "hammer.fill" : "hammer")
        .symbolEffect(.pulse, isActive: store.runningRunCount > 0)
      if store.runningRunCount > 0 {
        Text("\(store.runningRunCount)")
          .monospacedDigit()
          .contentTransition(.numericText())
      } else if store.attentionCount > 0 {
        Text("\(store.attentionCount)")
          .monospacedDigit()
          .contentTransition(.numericText())
      }
    }
    .animation(.spring(duration: 0.3), value: store.runningRunCount)
    .animation(.spring(duration: 0.3), value: store.attentionCount)
  }
}

struct MenuBarView: View {
  @Environment(OperatorStore.self) private var store

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Text("MDx")
          .font(.headline)
        Spacer()
        StatusPill(status: connectionText, tone: store.snapshot.connectionStatus == .ok ? .positive : .neutral)
      }

      Text(summaryLine)
        .font(.callout)
        .foregroundStyle(.secondary)

      Divider()

      if store.snapshot.connectionStatus != .ok {
        Button {
          activate()
          Task { await store.refresh() }
        } label: {
          Label("Connect local MDx", systemImage: "arrow.clockwise")
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(.plain)
      } else if store.activeRuns.isEmpty {
        Text("No runs in flight.")
          .font(.callout)
          .foregroundStyle(.secondary)
      } else {
        VStack(alignment: .leading, spacing: 6) {
          ForEach(store.activeRuns.prefix(6)) { run in
            Button {
              open(run)
            } label: {
              HStack(spacing: 8) {
                RunStatusDot(status: run.status)
                VStack(alignment: .leading, spacing: 1) {
                  Text(run.title.isEmpty ? run.id : run.title)
                    .font(.subheadline)
                    .lineLimit(1)
                  if !run.stage.isEmpty {
                    Text(run.stage)
                      .font(.caption2)
                      .foregroundStyle(.secondary)
                      .lineLimit(1)
                  }
                }
                Spacer(minLength: 0)
              }
              .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
          }
        }
      }

      Divider()

      Button {
        activate()
        store.startBuildFlow()
      } label: {
        Label("Start a build", systemImage: "hammer.fill")
          .frame(maxWidth: .infinity, alignment: .leading)
      }
      .buttonStyle(.plain)

      Button {
        CompanionPanelController.shared.toggle()
      } label: {
        HStack {
          Label("Quick panel", systemImage: "rectangle.center.inset.filled")
          Spacer()
          Text("⌥Space")
            .font(.caption)
            .foregroundStyle(.tertiary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
      }
      .buttonStyle(.plain)

      Button {
        activate()
        store.select(.forge(.overview))
      } label: {
        Label("Open MDx", systemImage: "macwindow")
          .frame(maxWidth: .infinity, alignment: .leading)
      }
      .buttonStyle(.plain)

      Button {
        NSApp.terminate(nil)
      } label: {
        Label("Quit MDx", systemImage: "power")
          .frame(maxWidth: .infinity, alignment: .leading)
      }
      .buttonStyle(.plain)
      .foregroundStyle(.secondary)
    }
    .padding(14)
    .frame(width: 320)
  }

  private var connectionText: String {
    switch store.snapshot.connectionStatus {
    case .ok: return "Connected"
    case .loading: return "Connecting"
    case .unavailable: return "Offline"
    }
  }

  private var summaryLine: String {
    guard store.snapshot.connectionStatus == .ok else {
      return "Not connected to local MDx."
    }
    let running = store.runningRunCount
    let review = store.attentionCount
    let runningText = running == 1 ? "1 running" : "\(running) running"
    let reviewText = review == 1 ? "1 ready to review" : "\(review) ready to review"
    if running == 0 && review == 0 { return "All quiet. Nothing in flight." }
    if review == 0 { return runningText }
    if running == 0 { return reviewText }
    return "\(runningText) · \(reviewText)"
  }

  private func activate() {
    NSApp.activate(ignoringOtherApps: true)
  }

  private func open(_ run: ForgeRun) {
    activate()
    store.select(.forge(.runs))
    store.openRun(run.id)
  }
}
