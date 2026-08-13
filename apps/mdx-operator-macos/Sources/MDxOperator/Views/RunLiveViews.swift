import AppKit
import SwiftUI

// MARK: - Live-scoped children
//
// These views are the structural fix for the live-run layout corruption and
// hang storm. The mechanism had two parts:
//
// 1. RunDetailView.body read store.liveEvents (through `trailEvents`) for both
//    the pinned header's current-action line and the activity list, so every
//    stream flush re-executed the entire run-detail body: the two ViewThatFits
//    containers (StageTracker, RunWorkIdentityStrip) were re-measured and the
//    full-height activity VStack re-diffed on the main thread. That synchronous
//    re-measure racing the auto-scroll is what scrambled glyphs and detached
//    the action-bar buttons.
// 2. The 4s run poll republished the run, whose value grew every poll, so it
//    re-rendered the whole trail even though the trail's content comes from the
//    stream, not the poll. That wasted O(n) layout was ~one main-thread hang
//    per poll.
//
// The fix: the live line and the trail read the stream in their own bodies, and
// the trail list takes only stable inputs (runID, isRunning) so a poll does not
// re-render it. Crucially the trail reads store.selectedRun ONLY when the live
// buffer is empty, so once a run is streaming, @Observable stops subscribing the
// trail to the run list and polls no longer touch it.

/// The single live-updating line in the pinned header. Reads the stream in its
/// own body so a flush invalidates this view alone, not the whole header, and it
/// does not take `run`, so a 4s poll does not re-render it.
struct LiveActionLine: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  private var latest: ForgeEvent? {
    let live = store.liveEvents
    if !live.isEmpty { return live.last }
    return store.selectedRun?.events.last
  }

  var body: some View {
    if let event = latest {
      HStack(spacing: 8) {
        // The working cue. An AppKit spinner while motion is allowed (it animates
        // off SwiftUI's render path, so it does not saturate the main thread the
        // way a SwiftUI symbolEffect did); a static glyph under Reduce Motion.
        if reduceMotion {
          Image(systemName: "ellipsis")
            .font(.callout.weight(.semibold))
            .foregroundStyle(Color.accentColor)
        } else {
          WorkingSpinner()
            .frame(width: 16, height: 16)
        }
        // No cross-fade here: this line changes several times a second while a
        // run streams, and an opacity content-transition on every change is
        // exactly the "text flying in and out" flicker. Plain text updates.
        Text(event.displaySummary)
          .font(.callout)
          .lineLimit(1)
          .truncationMode(.tail)
        Spacer(minLength: 8)
        LiveDot()
      }
      .padding(10)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(
        RoundedRectangle(cornerRadius: 8, style: .continuous)
          .fill(Color.accentColor.opacity(0.06))
      )
    }
  }
}

/// The activity tab: the running-context card (which does update on each poll)
/// plus the trail list (which must not). Splitting them keeps a poll from
/// re-rendering the expensive list.
struct ActivityTab: View {
  @Environment(OperatorStore.self) private var store
  let run: ForgeRun
  @Binding var fileFilter: String?

  private var latestEvent: ForgeEvent? {
    let live = store.liveEvents
    if !live.isEmpty { return live.last }
    return run.events.last
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 0) {
      if run.isRunning || run.execution.isWide || run.parallelCandidate.isParallel {
        ActiveRunContextCard(
          run: run,
          group: store.parallelGroup(containing: run.id),
          latestEvent: latestEvent
        )
        .padding(.bottom, 10)
      }
      if let filter = fileFilter {
        HStack(spacing: 8) {
          Image(systemName: "line.3.horizontal.decrease.circle")
            .foregroundStyle(Color.accentColor)
          Text("Steps that touched \((filter as NSString).lastPathComponent)")
            .font(.callout)
          Spacer(minLength: 8)
          Button("Show everything") {
            withAnimation(.easeOut(duration: 0.15)) { fileFilter = nil }
          }
          .controlSize(.small)
        }
        .padding(10)
        .background(
          RoundedRectangle(cornerRadius: 8, style: .continuous)
            .fill(Color.accentColor.opacity(0.07))
        )
        .padding(.bottom, 6)
      }
      // Stable inputs: a poll that only grew the run's events does not change
      // runID/isRunning, so SwiftUI skips this child. It re-renders only when
      // the live buffer it reads internally changes (the coalesced stream).
      ActivityTrailList(runID: run.id, isRunning: run.isRunning, fileFilter: $fileFilter)
    }
  }
}

/// The scrolling trail. Reads the live buffer directly, and the selected run's
/// recorded events only while the live buffer is empty (a just-opened run), so a
/// streaming run's poll never re-renders it.
struct ActivityTrailList: View {
  @Environment(OperatorStore.self) private var store
  let runID: String
  let isRunning: Bool
  @Binding var fileFilter: String?

  private var events: [ForgeEvent] {
    let live = store.liveEvents
    if !live.isEmpty { return live }
    // Only reached before the stream delivers its first batch; reading
    // selectedRun here (and only here) is what keeps the steady-state stream
    // decoupled from the run list.
    return store.selectedRun?.events ?? []
  }

  private var filtered: [ForgeEvent] {
    guard let filter = fileFilter else { return events }
    let basename = (filter as NSString).lastPathComponent
    return events.filter {
      $0.summary.localizedCaseInsensitiveContains(basename)
        || $0.detail.localizedCaseInsensitiveContains(basename)
    }
  }

  var body: some View {
    let events = self.events
    let filtered = self.filtered
    return Group {
      if events.isEmpty {
        EmptyStateView(text: isRunning ? "Waiting for the first step to arrive…" : "No activity was recorded for this run.")
      } else if fileFilter != nil, filtered.isEmpty {
        EmptyStateView(text: "No recorded steps mention this file by name. The full trail is still here under Show everything.")
      } else {
        // Lazy so a stream flush only lays out the rows on screen, not all of
        // the (up to 1000) live events. Rows are never dropped from the list,
        // and stable event ids let the ForEach reuse existing rows, so an append
        // is O(new rows). The enclosing ScrollView's .defaultScrollAnchor(.bottom)
        // follows the tail natively, so there is no per-flush proxy.scrollTo.
        LazyVStack(alignment: .leading, spacing: 0) {
          ForEach(filtered) { event in
            LogRow(event: event)
          }
        }
      }
    }
  }
}
