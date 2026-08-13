import MDxOperatorShared
import SwiftUI

struct NowView: View {
  @EnvironmentObject private var store: ForgeAnywhereStore
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize
  let onStartBuild: () -> Void
  let onReview: () -> Void

  var body: some View {
    NavigationStack {
      ScrollView {
        LazyVStack(alignment: .leading, spacing: 18) {
          statusHeader

          if !featuredSessions.isEmpty {
            MobileEyebrow(title: "Your Forge work", symbol: "hammer")
            ForEach(featuredSessions) { session in
              sessionLink(session)
            }
          }

          ForEach(store.needsYou) { item in
            MobileNeedsYouCard(item: item)
          }

          if store.snapshot.connection == .loading {
            ProgressView("Loading your work")
              .frame(maxWidth: .infinity, minHeight: 220)
              .accessibilityIdentifier("now.loading")
          } else if store.snapshot.connection == .failed {
            BoundaryCard(
              title: "Could not refresh", detail: store.snapshot.safeStatus,
              symbol: "exclamationmark.triangle"
            )
            .accessibilityIdentifier("now.error")
          } else if store.snapshot.sessions.isEmpty {
            EmptyNowCard(onStartBuild: onStartBuild)
          } else {
            if store.snapshot.connection == .offline {
              BoundaryCard(
                title: "You are offline", detail: store.snapshot.safeStatus, symbol: "wifi.slash"
              )
              .accessibilityIdentifier("now.offline")
            }

            ForEach(store.snapshot.attention) { item in
              AttentionCard(item: item)
            }

            ForEach(store.snapshot.reviews) { review in
              ReviewReadyCard(review: review, action: onReview)
            }

            if store.snapshot.sessions.count > featuredSessions.count {
              MobileEyebrow(title: "Earlier work", symbol: "clock.arrow.circlepath")
            }
            ForEach(Array(store.snapshot.sessions.dropFirst(featuredSessions.count))) { session in
              sessionLink(session)
            }
          }
        }
        .mobileScrollContent()
      }
      .navigationTitle("Now")
      .mobileAdaptiveNavigationTitle(dynamicTypeSize)
      .toolbar {
        ToolbarItem(placement: .topBarTrailing) {
          Button {
            Task { await store.enableNotifications() }
          } label: {
            Image(systemName: "bell")
          }
          .accessibilityLabel("Notifications")
        }
      }
    }
  }

  private var featuredSessions: [ForgeWorkSession] {
    Array(store.snapshot.sessions.prefix(3))
  }

  private func sessionLink(_ session: ForgeWorkSession) -> some View {
    NavigationLink {
      MobileSessionTimelineView(sessionID: session.id)
        .mobileFocusedDestination()
    } label: {
      SessionCard(session: session)
    }
    .buttonStyle(.plain)
  }

  private var statusHeader: some View {
    VStack(alignment: .leading, spacing: 8) {
      MobileEyebrow(title: "Current signal", symbol: "waveform.path.ecg")
      Text(store.snapshot.safeStatus)
        .font(dynamicTypeSize.isAccessibilitySize ? .headline : .title2.weight(.semibold))
        .accessibilityIdentifier("now.status")
      if !dynamicTypeSize.isAccessibilitySize {
        Text("Interruptions only when your judgment changes the outcome.")
          .font(.subheadline)
          .foregroundStyle(.secondary)
      }
    }
  }
}

private struct EmptyNowCard: View {
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize
  let onStartBuild: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 14) {
      if !dynamicTypeSize.isAccessibilitySize {
        Image(systemName: "iphone.and.arrow.forward")
          .font(.largeTitle)
          .foregroundStyle(.tint)
      }
      Text("Build from wherever you are")
        .font(.headline)
        .accessibilityIdentifier("now.empty")
      Button("Start build", action: onStartBuild)
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityIdentifier("now.start-build")
      Text(
        dynamicTypeSize.isAccessibilitySize
          ? "Brief Forge, choose an execution target, then return for judgment."
          : "Brief Forge from your phone, choose your Mac or MDx Cloud, then return only when the work needs judgment or review."
      )
      .foregroundStyle(.secondary)
    }
    .mobileCard()
  }
}

private struct MobileNeedsYouCard: View {
  @EnvironmentObject private var store: ForgeAnywhereStore
  let item: MobileNeedsYouItem

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      Label("Needs you", systemImage: "person.crop.circle.badge.exclamationmark")
        .font(.caption.weight(.bold))
        .foregroundStyle(.orange)
      Text(item.title).font(.headline)
      Text(item.detail).foregroundStyle(.secondary)
      VStack(alignment: .leading, spacing: 5) {
        Text("Recommended").font(.caption.weight(.bold)).foregroundStyle(.secondary)
        Text(item.recommendedChoice)
      }
      if item.mobileActionSupported {
        Button {
          Task { await store.approve(item) }
        } label: {
          Label("Confirm with Face ID", systemImage: "faceid")
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .disabled(store.isCommandInFlight)
      } else {
        Label("Review on your Mac", systemImage: "laptopcomputer")
          .font(.subheadline)
          .foregroundStyle(.secondary)
      }
      Text("This action cannot start workers or ship code.")
        .font(.caption)
        .foregroundStyle(.secondary)
    }
    .mobileCard()
    .accessibilityIdentifier("now.mobile-needs-you")
  }
}

private struct AttentionCard: View {
  let item: AttentionItem

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      Label("Needs you", systemImage: "person.crop.circle.badge.exclamationmark")
        .font(.caption.weight(.bold))
        .foregroundStyle(.orange)
      Text(item.title).font(.headline)
      Text(item.detail).foregroundStyle(.secondary)
      NavigationLink {
        MobileSessionTimelineView(sessionID: item.sessionID)
          .mobileFocusedDestination()
      } label: {
        Text(item.actionLabel)
      }
      .buttonStyle(.borderedProminent)
    }
    .mobileCard()
    .accessibilityElement(children: .contain)
    .accessibilityIdentifier("now.needs-you")
  }
}

struct SessionCard: View {
  let session: ForgeWorkSession

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack {
        Label(
          session.activeTarget.kind.shortLabel,
          systemImage: session.activeTarget.kind == .pairedHost ? "laptopcomputer" : "cloud"
        )
        .font(.caption.weight(.semibold))
        Spacer()
        Text(session.state.label)
          .font(.caption.weight(.bold))
          .foregroundStyle(session.needsUser ? .orange : .secondary)
      }
      Text(session.repositoryID).font(.headline)
      Text(session.goal).font(.body)
      ProgressView(value: stageProgress)
      HStack {
        Text(session.stage.rawValue.capitalized)
        Spacer()
        Text(MobileFormatters.shortRevision(session.sourceRevision)).monospaced()
      }
      .font(.caption)
      .foregroundStyle(.secondary)
    }
    .mobileCard()
    .accessibilityElement(children: .combine)
    .accessibilityLabel(
      "\(session.goal), \(session.repositoryID), \(session.state.label), \(session.stage.rawValue), on \(session.activeTarget.displayName)"
    )
    .accessibilityIdentifier("now.session.\(session.id)")
  }

  private var stageProgress: Double {
    let stages = ForgeSessionStage.allCases
    guard let index = stages.firstIndex(of: session.stage) else { return 0 }
    return Double(index + 1) / Double(stages.count)
  }
}

private struct ReviewReadyCard: View {
  let review: ReviewDigest
  let action: () -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 10) {
      Label("Ready to review", systemImage: "checkmark.seal.fill")
        .font(.caption.weight(.bold))
        .foregroundStyle(.green)
        .accessibilityIdentifier("now.review-ready")
      Text(review.outcome).font(.headline)
      Text(review.demo).foregroundStyle(.secondary)
      Button("Review outcome", action: action)
        .buttonStyle(.borderedProminent)
    }
    .mobileCard()
  }
}

struct BoundaryCard: View {
  let title: String
  let detail: String
  let symbol: String

  var body: some View {
    Label {
      VStack(alignment: .leading, spacing: 4) {
        Text(title).font(.headline)
        Text(detail).font(.subheadline).foregroundStyle(.secondary)
      }
    } icon: {
      Image(systemName: symbol).foregroundStyle(.orange)
    }
    .mobileCard()
  }
}
