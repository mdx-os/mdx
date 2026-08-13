import SwiftUI

struct MarketplacePackDetail: View {
  @Environment(OperatorStore.self) private var store
  let pack: MarketplacePack
  @State private var scope = "me"
  @State private var applications: Set<String> = []

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 20) {
        header
        beforeAfter
        actionBar
        if let outcome = store.packActionResult { outcomeBanner(outcome) }
        if let trial = store.packTrialPreview, trial.packID == pack.id { trialCard(trial) }
        Divider()
        grants
        composition
        lifecycle
        impact
        authority
      }
      .padding(28)
      .frame(maxWidth: 780, alignment: .leading)
      .frame(maxWidth: .infinity, alignment: .center)
    }
    .navigationTitle(pack.name)
    .onAppear { resetSelection() }
    .onChange(of: pack.id) { _, _ in resetSelection() }
  }

  private var header: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(spacing: 8) {
        PageMetaChip(icon: "shippingbox", text: pack.sourceLane.uppercased())
        if pack.publisherVerified { PageMetaChip(icon: "checkmark.seal", text: "Verified", tint: .green) }
        PageMetaChip(icon: "number", text: pack.state.version.isEmpty ? pack.version : pack.state.version)
        StatusPill(status: pack.lifecycleLabel, color: pack.state.ready ? .green : .orange)
      }
      Text(pack.name).font(.largeTitle.weight(.semibold))
      Text(pack.outcome).font(.title3).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
      Text("Published by \(pack.publisher) for \(pack.supportedApps.joined(separator: ", ")).")
        .font(.caption).foregroundStyle(.tertiary)
    }
  }

  private var beforeAfter: some View {
    HStack(alignment: .top, spacing: 12) {
      comparison("BEFORE", "The app plans without this job-specific toolbelt.", tint: .secondary)
      comparison("AFTER", pack.improvement.isEmpty ? pack.outcome : pack.improvement, tint: .accentColor)
    }
  }

  private func comparison(_ label: String, _ line: String, tint: Color) -> some View {
    VStack(alignment: .leading, spacing: 7) {
      Text(label).font(.caption2.weight(.bold)).tracking(1).foregroundStyle(tint)
      Text(line).font(.callout.weight(.medium)).fixedSize(horizontal: false, vertical: true)
    }
    .padding(14).frame(maxWidth: .infinity, minHeight: 88, alignment: .topLeading)
    .background(RoundedRectangle(cornerRadius: 12).fill(tint.opacity(0.07)))
    .overlay(RoundedRectangle(cornerRadius: 12).stroke(tint.opacity(0.18)))
  }

  private var actionBar: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack {
        Picker("Scope", selection: $scope) {
          ForEach(pack.installScopes, id: \.self) { Text($0.capitalized).tag($0) }
        }.fixedSize()
        Text("Use in").font(.caption).foregroundStyle(.secondary)
        ForEach(pack.applicationTargets, id: \.self) { target in
          Toggle(target.capitalized, isOn: binding(for: target)).toggleStyle(.button).controlSize(.small)
        }
      }
      HStack(spacing: 9) {
        if !pack.state.installed {
          Button("Apply pack") { store.installPack(pack, scope: scope, applications: Array(applications).sorted()) }
            .buttonStyle(.borderedProminent).disabled(store.packActionInFlight || applications.isEmpty)
          Button { store.tryPack(pack) } label: { Label("Try safely", systemImage: "eye") }
            .disabled(store.packActionInFlight)
        } else {
          if pack.state.lifecycleState == "disabled" {
            Button("Enable") { act("enable") }.buttonStyle(.borderedProminent)
          } else if pack.state.ready {
            Button(pack.firstUseLabel) { openFirstUse() }.buttonStyle(.borderedProminent)
            Button("Disable") { act("disable") }
          } else {
            if pack.state.lifecycleState == "held" {
              Button("Re-apply reviewed pieces") {
                store.installPack(pack, scope: scope, applications: Array(applications).sorted())
              }
              .buttonStyle(.borderedProminent)
            }
            Button("Disable") { act("disable") }
          }
          if !pack.updateVersion.isEmpty && pack.updateVersion != pack.state.version { Button("Update to \(pack.updateVersion)") { act("update", version: pack.updateVersion) } }
          if !pack.state.rollbackVersion.isEmpty { Button("Roll back") { act("rollback", version: pack.state.rollbackVersion) } }
          Button("Save scope") { act("rescope") }
          Button("Remove", role: .destructive) { act("remove") }
        }
        if store.packActionInFlight { ProgressView().controlSize(.small) }
      }
      if pack.state.lifecycleState == "setup_required" {
        Label("This pack cannot become ready from a confirmation click. Its required setup has no verified completion rail yet.", systemImage: "lock.trianglebadge.exclamationmark")
          .font(.caption).foregroundStyle(.orange)
      } else if pack.state.lifecycleState == "held" {
        Text("Review the held capabilities, then re-apply. Pieces already installed keep their current scope.")
          .font(.caption).foregroundStyle(.secondary)
      }
    }
  }

  private var grants: some View {
    Grid(alignment: .topLeading, horizontalSpacing: 28, verticalSpacing: 8) {
      GridRow { Text("MAY DO").font(.caption2.weight(.bold)).foregroundStyle(.green); Text("STILL BLOCKED").font(.caption2.weight(.bold)).foregroundStyle(.red) }
      let count = max(pack.requestedGrants.count, pack.blockedGrants.count)
      ForEach(0..<count, id: \.self) { index in
        GridRow {
          if pack.requestedGrants.indices.contains(index) { Label(pack.requestedGrants[index], systemImage: "checkmark.circle").foregroundStyle(.secondary) } else { Text("") }
          if pack.blockedGrants.indices.contains(index) { Label(pack.blockedGrants[index], systemImage: "xmark.circle").foregroundStyle(.secondary) } else { Text("") }
        }.font(.callout)
      }
    }
  }

  private var composition: some View {
    DisclosureGroup("What lands, dependencies, and conflicts") {
      VStack(alignment: .leading, spacing: 8) {
        ForEach(pack.items, id: \.self) { item in
          HStack { Image(systemName: pack.requiredItems.contains(item) ? "checkmark.circle.fill" : "circle.dashed"); Text(item.replacingOccurrences(of: "_", with: " ").capitalized); Spacer(); Text(pack.requiredItems.contains(item) ? "Required" : "Optional").foregroundStyle(.tertiary) }
        }
        if !pack.dependencies.isEmpty { Label("Needs: \(pack.dependencies.joined(separator: ", "))", systemImage: "link") }
        if !pack.conflicts.isEmpty { Label("Conflicts: \(pack.conflicts.joined(separator: ", "))", systemImage: "exclamationmark.triangle") }
      }.font(.caption).padding(.top, 8)
    }
  }

  private var lifecycle: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text("READINESS").font(.caption2.weight(.bold)).foregroundStyle(.tertiary)
      HStack(spacing: 7) {
        readiness("Installed", pack.state.installed)
        readiness("Configured", pack.state.configured)
        readiness("Connected", pack.state.connected)
        readiness("Authorized", pack.state.authorized)
        readiness("Enabled", pack.state.enabled)
        readiness("Applicable", pack.state.applicable)
        readiness("Executable", pack.state.executable)
      }
      if !pack.setupSteps.isEmpty { Text(pack.setupSteps.joined(separator: " ")).font(.caption).foregroundStyle(.secondary) }
    }
  }

  private func readiness(_ label: String, _ ready: Bool) -> some View {
    Label(label, systemImage: ready ? "checkmark.circle.fill" : "circle")
      .font(.caption2).foregroundStyle(ready ? Color.green : Color.secondary)
  }

  private var impact: some View {
    HStack(spacing: 18) {
      metric("USES", pack.state.useCount)
      metric("ACCEPTED", pack.state.acceptedCount)
      metric("REJECTED", pack.state.rejectedCount)
      Text("Work content is never stored in the impact ledger.").font(.caption).foregroundStyle(.tertiary)
    }
    .padding(12).background(RoundedRectangle(cornerRadius: 10).fill(Color.primary.opacity(0.035)))
  }

  private func metric(_ label: String, _ value: Int) -> some View {
    VStack { Text("\(value)").font(.title3.weight(.semibold)); Text(label).font(.caption2).foregroundStyle(.tertiary) }
  }

  private var authority: some View {
    DisclosureGroup("Authority and receipt") {
      Text("Marketplace records pack state and human decisions. It does not grant execution, secrets, inherited agent permissions, or production writes. Execution stays with the app that owns the work.")
        .font(.caption).foregroundStyle(.secondary).padding(.top, 6)
      if !pack.state.installReceiptID.isEmpty { Text(pack.state.installReceiptID).font(.caption.monospaced()).textSelection(.enabled) }
    }
  }

  private func outcomeBanner(_ outcome: RunActionOutcome) -> some View {
    Label(outcome.detail, systemImage: outcome.succeeded ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
      .font(.callout).foregroundStyle(outcome.succeeded ? Color.green : Color.orange)
      .padding(12).frame(maxWidth: .infinity, alignment: .leading)
      .background(RoundedRectangle(cornerRadius: 10).fill(Color.primary.opacity(0.04)))
  }

  private func trialCard(_ trial: PackTrialPreview) -> some View {
    VStack(alignment: .leading, spacing: 7) {
      Label("Safe trial complete", systemImage: "eye.circle.fill").font(.headline).foregroundStyle(Color.accentColor)
      Text(trial.example)
      Text("Preview available in \(trial.previewApps.joined(separator: ", ")). Nothing external changed.").font(.caption).foregroundStyle(.secondary)
      Text(trial.receiptID).font(.caption2.monospaced()).foregroundStyle(.tertiary).textSelection(.enabled)
    }.padding(14).background(RoundedRectangle(cornerRadius: 12).fill(Color.accentColor.opacity(0.07)))
  }

  private func resetSelection() {
    scope = pack.state.scope.isEmpty ? (pack.installScopes.first ?? "me") : pack.state.scope
    applications = Set(pack.state.applicationTargets.isEmpty ? pack.applicationTargets : pack.state.applicationTargets)
  }

  private func binding(for target: String) -> Binding<Bool> {
    Binding(get: { applications.contains(target) }, set: { enabled in
      if enabled { applications.insert(target) } else { applications.remove(target) }
    })
  }

  private func act(_ action: String, version: String = "") {
    store.actOnPack(pack, action: action, scope: scope, applications: Array(applications).sorted(), version: version)
  }

  private func openFirstUse() {
    guard pack.state.ready, !pack.firstUsePath.isEmpty, let url = URL(string: "http://127.0.0.1:18891\(pack.firstUsePath)") else { return }
    NSWorkspace.shared.open(url)
  }
}

struct MarketplaceCapabilityReview: View {
  @Environment(OperatorStore.self) private var store
  let capability: Capability
  @State private var scope = "team"

  var body: some View {
    ScrollView {
      VStack(alignment: .leading, spacing: 20) {
        header
        reviewDecision
        if let outcome = store.capabilityActionResult { outcomeBanner(outcome) }
        Divider()
        accessBoundary
        evidence
        authority
      }
      .padding(28)
      .frame(maxWidth: 760, alignment: .leading)
      .frame(maxWidth: .infinity, alignment: .center)
    }
    .navigationTitle(capability.name)
    .onAppear { resetSelection() }
    .onChange(of: capability.id) { _, _ in resetSelection() }
  }

  private var header: some View {
    VStack(alignment: .leading, spacing: 10) {
      HStack(spacing: 8) {
        PageMetaChip(icon: capability.glyph, text: capability.typeLabel)
        PageMetaChip(icon: "person.crop.circle.badge.questionmark", text: "Needs review", tint: .orange)
        if !capability.riskLevel.isEmpty { PageMetaChip(icon: "exclamationmark.shield", text: "\(capability.riskLevel.capitalized) risk", tint: .orange) }
      }
      Text(capability.name).font(.largeTitle.weight(.semibold))
      Text(capability.reviewReason.isEmpty ? capability.summary : capability.reviewReason)
        .font(.title3).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
      Text("\(capability.sourceLabel) source by \(capability.sourceOwner.isEmpty ? "an unnamed owner" : capability.sourceOwner), version \(capability.sourceVersion).")
        .font(.caption).foregroundStyle(.tertiary)
    }
  }

  private var reviewDecision: some View {
    VStack(alignment: .leading, spacing: 10) {
      Text("DECIDE THE BOUNDARY").font(.caption2.weight(.bold)).foregroundStyle(.tertiary)
      HStack(spacing: 10) {
        Picker("Scope", selection: $scope) {
          ForEach(capability.installTargets, id: \.self) { Text($0.capitalized).tag($0) }
        }
        .fixedSize()
        Button("Approve for \(scope)") { decide("approve") }.buttonStyle(.borderedProminent)
        Button("Approve read-only") { decide("approve", readOnly: true) }
        Button("Reject", role: .destructive) { decide("reject") }
        if store.capabilityActionInFlight { ProgressView().controlSize(.small) }
      }
      .disabled(store.capabilityActionInFlight || scope.isEmpty)
      Text("Approval applies only to the selected scope. Read-only approval cannot widen later during install.")
        .font(.caption).foregroundStyle(.secondary)
    }
  }

  private var accessBoundary: some View {
    Grid(alignment: .topLeading, horizontalSpacing: 22, verticalSpacing: 8) {
      GridRow {
        Text("REQUESTED ACCESS").font(.caption2.weight(.bold)).foregroundStyle(.orange)
        Text("STILL BLOCKED").font(.caption2.weight(.bold)).foregroundStyle(.red)
      }
      GridRow {
        Text(capability.accessLine).fixedSize(horizontal: false, vertical: true)
        VStack(alignment: .leading, spacing: 5) {
          ForEach(capability.blockedActions, id: \.self) { action in Label(action, systemImage: "xmark.circle") }
        }
      }
    }
    .font(.callout).foregroundStyle(.secondary)
  }

  private var evidence: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text("REVIEW EVIDENCE").font(.caption2.weight(.bold)).foregroundStyle(.tertiary)
      if capability.riskFindings.isEmpty {
        Text("No additional scanner findings were supplied.").font(.callout).foregroundStyle(.secondary)
      } else {
        ForEach(capability.riskFindings, id: \.self) { finding in Label(finding, systemImage: "exclamationmark.triangle") }
      }
      if !capability.requiredSecrets.isEmpty {
        Label("Would require: \(capability.requiredSecrets.joined(separator: ", "))", systemImage: "key")
      }
      Text("Last scanned \(capability.lastScanned.isEmpty ? "date unavailable" : capability.lastScanned). Artifact \(capability.artifactHash.isEmpty ? "hash unavailable" : capability.artifactHash).")
        .font(.caption.monospaced()).foregroundStyle(.tertiary).textSelection(.enabled)
    }
  }

  private var authority: some View {
    Label("This review records a scoped capability decision. It does not grant execution, secrets, inherited agent permissions, or production writes.", systemImage: "lock.shield")
      .font(.caption).foregroundStyle(.secondary)
      .padding(12).frame(maxWidth: .infinity, alignment: .leading)
      .background(RoundedRectangle(cornerRadius: 10).fill(Color.primary.opacity(0.04)))
  }

  private func outcomeBanner(_ outcome: RunActionOutcome) -> some View {
    Label(outcome.detail, systemImage: outcome.succeeded ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
      .font(.callout).foregroundStyle(outcome.succeeded ? Color.green : Color.orange)
      .padding(12).frame(maxWidth: .infinity, alignment: .leading)
      .background(RoundedRectangle(cornerRadius: 10).fill(Color.primary.opacity(0.04)))
  }

  private func resetSelection() {
    scope = capability.installTargets.contains("team") ? "team" : (capability.installTargets.first ?? "")
    store.selectCapability(capability.id)
  }

  private func decide(_ decision: String, readOnly: Bool = false) {
    store.decideCapability(capability, decision: decision, scope: scope, readOnly: readOnly)
  }
}
