import SwiftUI

/// A compact tear-off window for watching one run beside other work (the
/// Codex floating-thread pattern). It reads the shared runs projection, so it
/// stays current on the global watcher cadence without owning a stream, and
/// it never fights the main window over the run detail's live trail.
struct RunMonitorView: View {
  let runID: String
  @Environment(OperatorStore.self) private var store

  private var run: ForgeRun? {
    store.forgeRuns.first { $0.id == runID }
  }

  var body: some View {
    Group {
      if let run {
        monitor(run)
      } else {
        VStack(spacing: 8) {
          Image(systemName: "hammer")
            .font(.title2)
            .foregroundStyle(.tertiary)
          Text("This run is no longer in the local projection.")
            .font(.callout)
            .foregroundStyle(.secondary)
        }
        .padding(24)
      }
    }
    .frame(minWidth: 340, idealWidth: 400, minHeight: 200)
    .navigationTitle(run?.displayName ?? "Run")
  }

  private func monitor(_ run: ForgeRun) -> some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .firstTextBaseline, spacing: 8) {
        RunStatusDot(status: run.status)
        Text(run.displayName)
          .font(.headline)
          .lineLimit(2)
        Spacer(minLength: 8)
        StatusPill(status: run.displayStatus, tone: forgeRunTone(run.status))
      }

      if !run.stages.isEmpty {
        StageTracker(stages: run.stages, run: run)
          .equatable()
      }

      if run.isRunning, !run.stage.isEmpty {
        HStack(spacing: 7) {
          LiveDot()
          Text(run.stage)
            .font(.callout)
            .foregroundStyle(.secondary)
            .lineLimit(2)
        }
      } else if !run.finalLine.isEmpty, !run.finalLine.contains("=") {
        Text(run.finalLine)
          .font(.callout)
          .foregroundStyle(.secondary)
          .lineLimit(3)
      }

      if let snapshot = run.localBaseSnapshot {
        Label(snapshot.shortLine, systemImage: "tray.and.arrow.down")
          .font(.caption)
          .foregroundStyle(.secondary)
          .lineLimit(2)
      }

      Spacer(minLength: 0)

      HStack(spacing: 10) {
        if !run.branch.isEmpty {
          Label(run.branch, systemImage: "arrow.triangle.branch")
            .font(.caption.monospaced())
            .foregroundStyle(.secondary)
            .lineLimit(1)
            .truncationMode(.middle)
        }
        Spacer(minLength: 0)
        Button("Open in MDx") {
          NSApp.activate(ignoringOtherApps: true)
          store.select(.forge(.runs))
          store.openRun(run.id)
        }
        .controlSize(.small)
      }
    }
    .padding(16)
  }
}
