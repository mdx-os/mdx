import MDxOperatorShared
import SwiftUI

struct ReviewView: View {
  @EnvironmentObject private var store: ForgeAnywhereStore
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize

  var body: some View {
    NavigationStack {
      ScrollView {
        LazyVStack(alignment: .leading, spacing: 18) {
          if store.snapshot.reviews.isEmpty {
            ContentUnavailableView(
              "Nothing to review", systemImage: "checkmark.seal",
              description: Text("Finished builds will bring outcome, demo, proof, and risk here.")
            )
            .frame(minHeight: 320)
            .accessibilityIdentifier("review.empty")
          } else {
            ForEach(store.snapshot.reviews) { digest in
              ReviewDigestView(digest: digest)
            }
          }
        }
        .mobileScrollContent()
      }
      .navigationTitle("Review")
      .mobileAdaptiveNavigationTitle(dynamicTypeSize)
      .task { await store.refreshForReview() }
      .safeAreaInset(edge: .bottom) {
        if let feedback = store.commandFeedback {
          Text(feedback)
            .font(.footnote)
            .padding(12)
            .frame(maxWidth: .infinity)
            .background(.thinMaterial)
        }
      }
    }
  }
}

private struct ReviewDigestView: View {
  @EnvironmentObject private var store: ForgeAnywhereStore
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize
  let digest: ReviewDigest
  @State private var changeRequest = ""
  @State private var shipReason = ""
  @State private var showingChangeRequest = false
  @State private var showingShipApproval = false

  var body: some View {
    VStack(alignment: .leading, spacing: 18) {
      VStack(alignment: .leading, spacing: 8) {
        MobileEyebrow(
          title: readyForJudgment ? "Ready for judgment" : "Work in progress",
          symbol: readyForJudgment ? "checkmark.seal.fill" : "hammer.fill",
          tint: readyForJudgment ? .green : .blue
        )
        .accessibilityIdentifier("review.digest.\(digest.id)")
        Text(digest.nextMove)
          .font(.headline)
      }

      ReviewSection(title: "Outcome", symbol: "sparkles", content: digest.outcome)
      ReviewSection(title: "Demo", symbol: "play.rectangle", content: digest.demo)

      VStack(alignment: .leading, spacing: 10) {
        Label("Proof", systemImage: "checkmark.shield").font(.headline)
        Text(digest.proofSummary).font(.subheadline).foregroundStyle(.secondary)
        ForEach(digest.proof) { proof in
          Label(
            proof.name,
            systemImage: proof.passed ? "checkmark.circle.fill" : "xmark.octagon.fill"
          )
          .font(.subheadline)
          .foregroundStyle(proof.passed ? AnyShapeStyle(.secondary) : AnyShapeStyle(.red))
        }
      }
      .padding(14)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(.green.opacity(0.08), in: RoundedRectangle(cornerRadius: 16))

      ReviewSection(title: "Risk", symbol: "exclamationmark.triangle", content: digest.risk)

      if !digest.changeMap.isEmpty {
        VStack(alignment: .leading, spacing: 10) {
          Label("Change map", systemImage: "map").font(.headline)
          Text("+\(digest.added) -\(digest.removed) across \(digest.changedFiles) files")
            .font(.caption.monospaced())
            .foregroundStyle(.secondary)
          ForEach(digest.changeMap) { area in
            HStack {
              VStack(alignment: .leading) {
                Text(area.label).font(.subheadline.weight(.semibold))
                Text("\(area.fileCount) files")
                  .font(.caption)
                  .foregroundStyle(.secondary)
              }
              Spacer()
              Text("+\(area.added) -\(area.removed)")
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
            }
          }
        }
      }

      if !digest.files.isEmpty {
        NavigationLink {
          FocusedDiffView(digest: digest)
            .mobileFocusedDestination()
        } label: {
          Label(
            "Review \(digest.changedFiles) changed files",
            systemImage: "doc.text.magnifyingglass"
          )
          .frame(maxWidth: .infinity)
        }
        .buttonStyle(.bordered)
        .accessibilityIdentifier("review.diff.\(digest.id)")
      }

      ViewThatFits(in: .horizontal) {
        HStack {
          reviewActions
        }
        VStack(spacing: 10) {
          reviewActions
        }
      }

      Text(
        "Approval records your reason for this exact commit and opens a pull request against the connected repository's recorded default branch. CI, merge, deployment, and production-write authority remain separate."
      )
      .font(.caption)
      .foregroundStyle(.secondary)

      if let url = store.actionURL(for: digest.sessionID) {
        Link("Open pull request", destination: url)
          .font(.subheadline.weight(.semibold))
      }
    }
    .mobileCard()
    .sheet(isPresented: $showingChangeRequest) {
      NavigationStack {
        Form {
          TextField("What should Forge change?", text: $changeRequest, axis: .vertical)
            .lineLimit(5...12)
            .accessibilityIdentifier("review.change-request")
          Text(
            "This starts a governed revision with the same proof and source-host boundaries."
          )
          .font(.caption)
          .foregroundStyle(.secondary)
        }
        .navigationTitle("Request changes")
        .toolbar {
          ToolbarItem(placement: .cancellationAction) {
            Button("Cancel") { showingChangeRequest = false }
          }
          ToolbarItem(placement: .confirmationAction) {
            Button("Send to Forge") {
              let comment = changeRequest.trimmingCharacters(in: .whitespacesAndNewlines)
              Task {
                await store.requestChanges(review: digest, comment: comment)
                if store.lastCommandSucceeded {
                  showingChangeRequest = false
                  changeRequest = ""
                }
              }
            }
            .disabled(changeRequest.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            .accessibilityIdentifier("review.send-changes")
          }
        }
      }
    }
    .sheet(isPresented: $showingShipApproval) {
      NavigationStack {
        Form {
          TextField("Why is this ready to ship?", text: $shipReason, axis: .vertical)
            .lineLimit(4...10)
            .accessibilityIdentifier("review.ship-reason")
          Text(
            "Face ID confirms this reviewed commit and your reason. MDx will not choose a base branch or merge the pull request for you."
          )
          .font(.caption)
          .foregroundStyle(.secondary)
        }
        .navigationTitle("Open pull request")
        .toolbar {
          ToolbarItem(placement: .cancellationAction) {
            Button("Cancel") { showingShipApproval = false }
          }
          ToolbarItem(placement: .confirmationAction) {
            Button("Approve and open") {
              let reason = shipReason.trimmingCharacters(in: .whitespacesAndNewlines)
              Task {
                await store.approveAndOpenPullRequest(review: digest, reason: reason)
                if store.lastCommandSucceeded {
                  showingShipApproval = false
                  shipReason = ""
                }
              }
            }
            .disabled(shipReason.trimmingCharacters(in: .whitespacesAndNewlines).count < 8)
            .accessibilityIdentifier("review.approve-open")
          }
        }
      }
    }
  }

  private var readyForJudgment: Bool {
    digest.reviewStatus == "ready_for_review" || digest.reviewStatus == "ready_to_ship"
  }

  @ViewBuilder
  private var reviewActions: some View {
    Button("Request changes") { showingChangeRequest = true }
      .buttonStyle(.bordered)
      .controlSize(.large)
      .frame(maxWidth: dynamicTypeSize.isAccessibilitySize ? .infinity : nil)
      .accessibilityIdentifier("review.request-changes")
    Button("Approve and open PR", systemImage: "faceid") {
      showingShipApproval = true
    }
    .buttonStyle(.borderedProminent)
    .controlSize(.large)
    .frame(maxWidth: dynamicTypeSize.isAccessibilitySize ? .infinity : nil)
    .disabled(store.isCommandInFlight || digest.commitSHA.isEmpty)
    .accessibilityIdentifier("review.approve-pr")
  }
}

private struct FocusedDiffView: View {
  let digest: ReviewDigest

  var body: some View {
    List(digest.files) { file in
      NavigationLink {
        ScrollView([.horizontal, .vertical]) {
          Text(file.patch)
            .font(.caption.monospaced())
            .textSelection(.enabled)
            .padding()
        }
        .navigationTitle(file.path.split(separator: "/").last.map(String.init) ?? file.path)
        .navigationBarTitleDisplayMode(.inline)
        .mobileFocusedDestination()
      } label: {
        VStack(alignment: .leading, spacing: 5) {
          Text(file.path).font(.subheadline)
          HStack {
            Text("+\(file.added) -\(file.removed)").monospaced()
            if let confidence = file.agentConfidence {
              Text(confidence.replacingOccurrences(of: "_", with: " "))
            }
          }
          .font(.caption)
          .foregroundStyle(.secondary)
        }
      }
    }
    .navigationTitle("Focused diff")
    .navigationBarTitleDisplayMode(.inline)
    .mobileFocusedDestination()
    .safeAreaInset(edge: .bottom) {
      if digest.filesTruncated || digest.files.contains(where: { $0.patchTruncated }) {
        Text("This is a bounded mobile diff. Open the source-host diff for the complete review.")
          .font(.caption)
          .padding(10)
          .frame(maxWidth: .infinity)
          .background(.thinMaterial)
      }
    }
  }
}

private struct ReviewSection: View {
  let title: String
  let symbol: String
  let content: String

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      Label(title, systemImage: symbol).font(.headline)
      Text(content).foregroundStyle(.secondary)
    }
  }
}
