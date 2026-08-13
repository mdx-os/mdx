import MDxOperatorShared
import SwiftUI

struct MobileSessionTimelineView: View {
  @EnvironmentObject private var store: ForgeAnywhereStore
  let sessionID: String
  @State private var direction = ""

  private var session: ForgeWorkSession? {
    store.snapshot.sessions.first { $0.id == sessionID }
  }

  var body: some View {
    List {
      if let session {
        Section {
          SessionCard(session: session)
            .listRowInsets(EdgeInsets())
            .listRowBackground(Color.clear)
        }

        Section("Steer") {
          TextField("Add direction for the next safe turn", text: $direction, axis: .vertical)
            .lineLimit(2...5)
            .accessibilityIdentifier("session.direction")
          Button {
            let instruction = direction.trimmingCharacters(in: .whitespacesAndNewlines)
            Task {
              await store.followUp(session: session, instruction: instruction)
              if store.lastCommandSucceeded { direction = "" }
            }
          } label: {
            Label("Send direction", systemImage: "arrow.up.circle.fill")
          }
          .disabled(
            direction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
              || !store.canControl(session) || store.isCommandInFlight
          )

          ViewThatFits(in: .horizontal) {
            HStack {
              sessionActions(session)
            }
            VStack(alignment: .leading, spacing: 10) {
              sessionActions(session)
            }
          }
          .disabled(!store.canControl(session) || store.isCommandInFlight)
        }
      }

      if let feedback = store.commandFeedback {
        Section("Latest") {
          Text(feedback).foregroundStyle(.secondary)
        }
      }

      Section("Live work") {
        let events = store.events(for: sessionID)
        if events.isEmpty {
          MobileEmptyState(
            title: "Waiting for the next update",
            detail:
              "The durable session is current. New milestones appear here without streaming private model output.",
            symbol: "bolt.horizontal.circle"
          )
          .listRowInsets(EdgeInsets())
          .listRowBackground(Color.clear)
          .accessibilityIdentifier("session.events-empty")
        } else {
          ForEach(events.reversed()) { event in
            TimelineEventRow(event: event)
          }
        }
      }

      Section("Safety boundary") {
        Label(
          "Provider credentials and raw model output stay out of mobile views",
          systemImage: "key.slash"
        )
        Label("Ship authority remains closed", systemImage: "lock.shield")
      }
      .font(.subheadline)
    }
    .navigationTitle(session?.repositoryID ?? "Build")
    .navigationBarTitleDisplayMode(.inline)
    .mobileFocusedDestination()
    .accessibilityIdentifier("session.timeline")
  }

  @ViewBuilder
  private func sessionActions(_ session: ForgeWorkSession) -> some View {
    if session.state == .paused {
      Button("Resume") { Task { await store.resume(session: session) } }
        .buttonStyle(.borderedProminent)
    } else if session.state == .running {
      Button("Pause") { Task { await store.pause(session: session) } }
        .buttonStyle(.bordered)
    }

    Menu {
      if store.followedSessionID == session.sessionID {
        Button("Stop Live Activity") {
          store.stopLiveActivity(sessionID: session.sessionID)
        }
      } else {
        Button("Follow on Lock Screen") {
          store.followLiveActivity(session: session)
        }
      }
      if session.state == .running || session.state == .paused {
        Button("Stop build", role: .destructive) {
          Task { await store.stop(session: session) }
        }
      }
      if let destination = store.alternateTarget(for: session),
        session.latestCheckpointRef != nil || session.state == .reviewReady
          || session.state == .completed || session.state == .paused
      {
        Button("Move to \(destination.displayName)") {
          Task { await store.requestHandoff(session: session, destination: destination) }
        }
      }
    } label: {
      Label("Session actions", systemImage: "ellipsis.circle")
    }
    .buttonStyle(.bordered)
    .accessibilityIdentifier("session.actions")
  }
}

private struct TimelineEventRow: View {
  let event: MobileRelayEnvelope

  var body: some View {
    HStack(alignment: .top, spacing: 12) {
      Image(systemName: symbol)
        .foregroundStyle(color)
        .frame(width: 22)
      VStack(alignment: .leading, spacing: 4) {
        Text(event.summary)
        HStack {
          Text(event.stage.capitalized)
          Text("#\(event.sequence)")
          Text(event.occurredAt, style: .time)
        }
        .font(.caption)
        .foregroundStyle(.secondary)
      }
    }
    .accessibilityElement(children: .combine)
  }

  private var symbol: String {
    switch event.kind {
    case "checkpointed": "checkmark.circle"
    case "review_ready": "doc.text.magnifyingglass"
    case "completed", "failed", "stopped": "flag.checkered"
    case "progress": "wrench.and.screwdriver"
    default: "sparkle"
    }
  }

  private var color: Color {
    event.state == "failed" ? .red : event.kind == "review_ready" ? .green : .accentColor
  }
}
