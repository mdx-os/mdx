import SwiftUI

struct MarketplaceView: View {
  @Environment(OperatorStore.self) private var store
  @State private var reviewSelectionID: String?
  @State private var sourceLane: String?

  var body: some View {
    @Bindable var store = store
    HStack(spacing: 0) {
      List(selection: $store.marketplaceSection) {
        Section("Marketplace") {
          ForEach(MarketplaceSection.allCases) { section in
            Label(section.rawValue, systemImage: section.symbol)
              .tag(section)
              .badge(badge(for: section))
          }
        }
        Section("Source lanes") {
          sourceLaneButton("MDx verified", value: "mdx", systemImage: "checkmark.seal")
          sourceLaneButton("Company and team", value: "company", systemImage: "building.2")
          sourceLaneButton("Personal", value: "personal", systemImage: "person")
          sourceLaneButton("Quarantine", value: "quarantine", systemImage: "lock.trianglebadge.exclamationmark")
        }
      }
      .listStyle(.sidebar)
      .frame(minWidth: 165, idealWidth: 190, maxWidth: 220)

      Divider()

      VStack(spacing: 0) {
        SurfaceSearchField(placeholder: "What should MDx do better?", text: $store.marketplaceSearch)
          .padding(10)
        Divider()
        if store.marketplaceSection == .review {
          if reviewCapabilities.isEmpty && reviewPacks.isEmpty {
            ContentUnavailableView(
              "Nothing is waiting",
              systemImage: "checkmark.circle",
              description: Text("Capabilities and held packs appear here when they need human judgment.")
            )
          } else {
            List(selection: $reviewSelectionID) {
              if !reviewCapabilities.isEmpty {
                Section("Capabilities") {
                  ForEach(reviewCapabilities) { capability in
                    MarketplaceCapabilityReviewRow(capability: capability)
                      .tag("capability:\(capability.id)")
                  }
                }
              }
              if !reviewPacks.isEmpty {
                Section("Held packs") {
                  ForEach(reviewPacks) { pack in
                    MarketplacePackRow(pack: pack)
                      .tag("pack:\(pack.id)")
                  }
                }
              }
            }
            .listStyle(.inset)
          }
        } else if displayedPacks.isEmpty {
          ContentUnavailableView(
            store.marketplaceSection == .installed ? "No installed packs" : "No packs found",
            systemImage: "shippingbox",
            description: Text(store.marketplaceSection == .installed
              ? "Apply a job pack and its readiness, scope, and impact appear here."
              : "Try a different job or source.")
          )
        } else {
          List(displayedPacks, selection: $store.selectedPackID) { pack in
            MarketplacePackRow(pack: pack)
              .tag(pack.id)
          }
          .listStyle(.inset)
        }
      }
      .frame(minWidth: 280, idealWidth: 320, maxWidth: 380)

      Divider()

      Group {
        if store.marketplaceSection == .review, let capability = selectedReviewCapability {
          MarketplaceCapabilityReview(capability: capability)
        } else if store.marketplaceSection == .review, let pack = selectedReviewPack {
          MarketplacePackDetail(pack: pack)
        } else if store.marketplaceSection == .review {
          ContentUnavailableView("Choose a decision", systemImage: "person.crop.circle.badge.checkmark", description: Text("Review the requested access, scope, risk, and blocked actions before deciding."))
        } else if let pack = store.selectedPack {
          MarketplacePackDetail(pack: pack)
        } else {
          ContentUnavailableView("Choose a pack", systemImage: "shippingbox", description: Text("See the outcome, exact grants, app targets, setup, lifecycle, and measured impact."))
        }
      }
      .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
    .task {
      if store.marketplacePacks.isEmpty { await store.loadMarketplace() }
      ensureReviewSelection()
    }
    .onChange(of: store.marketplaceSection) { _, section in
      if section == .review { ensureReviewSelection() }
    }
    .onChange(of: store.capabilities) { _, _ in ensureReviewSelection() }
    .onChange(of: store.marketplacePacks) { _, _ in ensureReviewSelection() }
    .onChange(of: store.marketplaceSearch) { _, _ in ensureReviewSelection() }
  }

  private func badge(for section: MarketplaceSection) -> Int {
    switch section {
    case .forYou, .packs: return 0
    case .installed: return store.marketplacePacks.filter(\.state.installed).count
    case .review: return store.capabilities.filter(\.needsReview).count + store.marketplacePacks.filter { !$0.state.heldItems.isEmpty }.count
    }
  }

  private var displayedPacks: [MarketplacePack] {
    guard let sourceLane else { return store.visibleMarketplacePacks }
    return store.visibleMarketplacePacks.filter { pack in
      let lane = pack.sourceLane.lowercased()
      switch sourceLane {
      case "mdx": return lane == "mdx" || pack.publisherVerified
      case "company": return lane.contains("company") || lane.contains("team")
      default: return lane.contains(sourceLane)
      }
    }
  }

  private func sourceLaneButton(_ label: String, value: String, systemImage: String) -> some View {
    Button {
      sourceLane = sourceLane == value ? nil : value
      store.marketplaceSection = .packs
      if !displayedPacks.contains(where: { $0.id == store.selectedPackID }) {
        store.selectedPackID = displayedPacks.first?.id
      }
    } label: {
      HStack {
        Label(label, systemImage: systemImage)
        Spacer()
        if sourceLane == value {
          Image(systemName: "checkmark")
            .foregroundStyle(Color.accentColor)
        }
      }
    }
    .buttonStyle(.plain)
    .accessibilityValue(sourceLane == value ? "Filtering" : "Not filtering")
  }

  private var reviewCapabilities: [Capability] {
    let query = store.marketplaceSearch.trimmingCharacters(in: .whitespacesAndNewlines)
    return store.capabilities.filter { capability in
      capability.needsReview && (query.isEmpty
        || capability.name.localizedCaseInsensitiveContains(query)
        || capability.summary.localizedCaseInsensitiveContains(query)
        || capability.reviewReason.localizedCaseInsensitiveContains(query))
    }
  }

  private var reviewPacks: [MarketplacePack] {
    let query = store.marketplaceSearch.trimmingCharacters(in: .whitespacesAndNewlines)
    return store.marketplacePacks.filter { pack in
      !pack.state.heldItems.isEmpty && (query.isEmpty
        || pack.name.localizedCaseInsensitiveContains(query)
        || pack.job.localizedCaseInsensitiveContains(query))
    }
  }

  private var selectedReviewCapability: Capability? {
    guard let id = reviewSelectionID?.replacingOccurrences(of: "capability:", with: ""), reviewSelectionID?.hasPrefix("capability:") == true else { return nil }
    return reviewCapabilities.first { $0.id == id }
  }

  private var selectedReviewPack: MarketplacePack? {
    guard let id = reviewSelectionID?.replacingOccurrences(of: "pack:", with: ""), reviewSelectionID?.hasPrefix("pack:") == true else { return nil }
    return reviewPacks.first { $0.id == id }
  }

  private func ensureReviewSelection() {
    if let reviewSelectionID {
      if reviewSelectionID.hasPrefix("capability:"), selectedReviewCapability != nil { return }
      if reviewSelectionID.hasPrefix("pack:"), selectedReviewPack != nil { return }
    }
    if let capability = reviewCapabilities.first {
      reviewSelectionID = "capability:\(capability.id)"
    } else if let pack = reviewPacks.first {
      reviewSelectionID = "pack:\(pack.id)"
    } else {
      reviewSelectionID = nil
    }
  }
}

struct MarketplaceCapabilityReviewRow: View {
  let capability: Capability

  var body: some View {
    VStack(alignment: .leading, spacing: 6) {
      HStack {
        Label(capability.name, systemImage: capability.glyph).font(.headline).lineLimit(1)
        Spacer()
        StatusPill(status: capability.riskLevel.isEmpty ? "Review" : "\(capability.riskLevel.capitalized) risk", color: .orange)
      }
      Text(capability.reviewReason.isEmpty ? capability.summary : capability.reviewReason)
        .font(.caption).foregroundStyle(.secondary).lineLimit(2)
      Text(capability.installTargets.joined(separator: " + ")).font(.caption2).foregroundStyle(.tertiary)
    }
    .padding(.vertical, 4)
    .accessibilityElement(children: .combine)
  }
}

struct MarketplacePackRow: View {
  let pack: MarketplacePack

  var body: some View {
    HStack(alignment: .top, spacing: 10) {
      Image(systemName: "shippingbox.fill")
        .font(.callout)
        .foregroundStyle(Color.accentColor)
        .frame(width: 22, height: 22)
        .background(Color.accentColor.opacity(0.10), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
      VStack(alignment: .leading, spacing: 6) {
        HStack(alignment: .firstTextBaseline) {
          Text(pack.name).font(.headline).lineLimit(2)
          Spacer(minLength: 8)
          StatusPill(status: pack.lifecycleLabel, color: statusColor)
        }
        Text(pack.job).font(.caption).foregroundStyle(.secondary).lineLimit(3)
        HStack(spacing: 6) {
          Text(pack.sourceLane.uppercased()).font(.caption2.weight(.semibold)).foregroundStyle(.tertiary)
          Text("v\(pack.state.version.isEmpty ? pack.version : pack.state.version)").font(.caption2).foregroundStyle(.tertiary)
          Spacer()
          Text(pack.supportedApps.joined(separator: " + ")).font(.caption2.weight(.medium)).foregroundStyle(Color.accentColor)
            .lineLimit(1)
        }
      }
    }
    .padding(.vertical, 7)
    .accessibilityElement(children: .combine)
  }

  private var statusColor: Color {
    switch pack.state.lifecycleState {
    case "ready": return .green
    case "held", "setup_required", "update_available": return .orange
    case "disabled": return .secondary
    default: return .accentColor
    }
  }
}
