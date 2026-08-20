import Foundation
import SwiftUI

/// The primary navigation rail, built on a native `List(selection:)` so
/// selection, up/down keyboard navigation, and system sidebar styling come for
/// free. The Forge lanes stay in a disclosure group, the app surfaces stay
/// under an Apps section, and the health and account footers stay pinned below
/// the list. Copy and iconography are unchanged; only the chrome went native.
struct SidebarView: View {
  @Environment(OperatorStore.self) private var store
  @Environment(CloudAuthStore.self) private var auth

  /// List selection is the app route. Reading it from the store keeps the
  /// highlight in sync when navigation happens elsewhere (welcome, palette,
  /// back/forward); writing it routes through `select` so history and streaming
  /// stay correct.
  private var selection: Binding<AppRoute?> {
    Binding(
      get: { store.selectedAppRoute },
      set: { if let route = $0 { store.select(route) } }
    )
  }

  var body: some View {
    VStack(spacing: 0) {
      List(selection: selection) {
        Section {
          SidebarRowLabel(route: .home)
            .tag(AppRoute.home)
          ForgeDisclosureGroup()
        }

        Section("Apps") {
          ForEach([AppRoute.twin, .pages, .message, .memory, .marketplace], id: \.id) { route in
            SidebarRowLabel(route: route)
              .tag(route)
          }
        }

        Section {
          SidebarRowLabel(route: .you)
            .tag(AppRoute.you)
        }
      }
      .listStyle(.sidebar)
      .environment(\.defaultMinListRowHeight, 34)

      Divider()
        .opacity(0.35)

      HealthPill()
        .padding(.horizontal, 10)
        .padding(.top, 8)

      SidebarAccountFooter(name: userDisplayName)
        .padding(10)
    }
    .navigationTitle("MDx")
    .background(SidebarBackground())
  }

  // NSFullUserName() is not free; cache it for the app's lifetime.
  private static let cachedDisplayName: String = {
    let fullName = NSFullUserName().trimmingCharacters(in: .whitespacesAndNewlines)
    return fullName.isEmpty ? NSUserName() : fullName
  }()

  private var userDisplayName: String {
    let cloudName = auth.displayName.trimmingCharacters(in: .whitespacesAndNewlines)
    return cloudName.isEmpty ? Self.cachedDisplayName : cloudName
  }
}

/// One navigation row: icon, title, and (for full rows) a one-line subtitle,
/// plus an optional count badge. No button or custom background - the enclosing
/// `List` draws selection and handles the tap, so the row is pure content.
struct SidebarRowLabel: View {
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  let route: AppRoute
  var badge: Int?
  var attention = false
  var compact = false

  var body: some View {
    HStack(spacing: 10) {
      Group {
        // The Forge Overview lane wears the same rocket brand as the Forge
        // parent row; every other lane keeps its SF Symbol.
        if route == .forge(.overview) {
          RocketMark(size: 14)
        } else {
          Image(systemName: route.systemImage)
            .font(.system(size: 14, weight: .medium))
        }
      }
      .frame(width: 18)
      .foregroundStyle(.secondary)

      VStack(alignment: .leading, spacing: 1) {
        Text(route.sidebarTitle)
          .font(.subheadline)
          .foregroundStyle(.primary)
          .lineLimit(1)

        if !compact {
          Text(route.subtitle)
            .font(.caption2)
            .foregroundStyle(.secondary)
            .lineLimit(1)
        }
      }

      Spacer(minLength: 0)

      if let badge, badge > 0 {
        Text("\(badge)")
          .font(.caption2.weight(.semibold))
          .monospacedDigit()
          .contentTransition(.numericText())
          .animation(reduceMotion ? nil : .spring(duration: 0.3), value: badge)
          .foregroundStyle(attention ? Color.accentColor : Color.secondary)
          .padding(.horizontal, 6)
          .padding(.vertical, 2)
          .background((attention ? Color.accentColor : Color.secondary).opacity(0.12))
          .clipShape(Capsule())
      }
    }
    .padding(.vertical, compact ? 1 : 2)
    .contentShape(Rectangle())
  }
}

/// The Forge lanes, disclosed under a navigable parent. The parent row still
/// opens Build on click; its disclosure triangle expands the lanes,
/// and each lane is a tagged, list-selectable row so keyboard navigation and
/// system selection reach them.
struct ForgeDisclosureGroup: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @State private var expanded = true

  var body: some View {
    DisclosureGroup(isExpanded: $expanded) {
      ForEach(ForgeLane.allCases) { lane in
        SidebarRowLabel(
          route: .forge(lane),
          badge: badge(for: lane),
          attention: lane == .runs && !store.unseenRunIDs.isEmpty,
          compact: true
        )
        .tag(AppRoute.forge(lane))
      }
    } label: {
      // Clicking the Forge parent opens Build; the disclosure triangle
      // expands the lane list on its own. The label is not list-selectable, so
      // it never collides with the Runs lane's own tag.
      Button {
        store.select(.forge(.overview))
      } label: {
        HStack(spacing: 10) {
          RocketMark()
            .frame(width: 18)
            .foregroundStyle(store.selectedAppRoute.isForge ? Color.accentColor : Color.secondary)
          VStack(alignment: .leading, spacing: 1) {
            Text("Forge")
              .font(.subheadline.weight(store.selectedAppRoute.isForge ? .semibold : .regular))
              .foregroundStyle(.primary)
            Text("Governed code")
              .font(.caption2)
              .foregroundStyle(.secondary)
          }
          Spacer(minLength: 0)
          let count = store.snapshot.runCount + store.reviewReadyCount
          if count > 0 {
            Text("\(count)")
              .font(.caption2.weight(.semibold))
              .monospacedDigit()
              .contentTransition(.numericText())
              .animation(reduceMotion ? nil : .spring(duration: 0.3), value: count)
              .foregroundStyle(.secondary)
              .padding(.horizontal, 6)
              .padding(.vertical, 2)
              .background(Color.secondary.opacity(0.12))
              .clipShape(Capsule())
          }
        }
        .contentShape(Rectangle())
      }
      .buttonStyle(.plain)
    }
  }

  private func badge(for lane: ForgeLane) -> Int? {
    switch lane {
    case .runs:
      // Unread finished runs take precedence over the raw count.
      if !store.unseenRunIDs.isEmpty { return store.unseenRunIDs.count }
      return store.snapshot.runCount > 0 ? store.snapshot.runCount : nil
    case .review:
      return store.reviewReadyCount > 0 ? store.reviewReadyCount : nil
    case .fleet:
      return store.snapshot.fleetRunCount > 0 ? store.snapshot.fleetRunCount : nil
    default:
      return nil
    }
  }
}

struct SidebarBackground: View {
  var body: some View {
    // The NavigationSplitView sidebar column already supplies the system
    // material (glass on macOS 26). Adding more here muddied it into a
    // double blur.
    Rectangle()
      .fill(.ultraThinMaterial)
  }
}

struct SidebarAccountFooter: View {
  @Environment(OperatorStore.self) private var store
  @Environment(CloudAuthStore.self) private var auth
  let name: String
  @State private var hovering = false

  private var accountRole: String? {
    guard let role = store.identitySession?.roleLabel, !role.isEmpty else { return nil }
    return role
  }

  private var accountDetail: String? {
    let email = auth.email.trimmingCharacters(in: .whitespacesAndNewlines)
    return email.isEmpty ? accountRole : email
  }

  var body: some View {
    HStack(spacing: 10) {
      Text(initials)
        .font(.caption.weight(.semibold))
        .foregroundStyle(.white)
        .frame(width: 30, height: 30)
        .background(Circle().fill(Color.accentColor.gradient))
        .accessibilityLabel(name)

      VStack(alignment: .leading, spacing: 2) {
        Text(name)
          .font(.caption.weight(.semibold))
          .lineLimit(1)
        // Connection already lives in the HealthPill just above. A hosted
        // session shows the exact account; local mode falls back to the role.
        if let accountDetail {
          Text(accountDetail)
            .font(.caption2)
            .foregroundStyle(.secondary)
            .lineLimit(1)
        }
      }

      Spacer(minLength: 6)

      Menu {
        Button("Open settings") { store.showSettings() }
        Button("Refresh local MDx") { Task { await store.refresh() } }
        Divider()
        Button("Diagnostics") { store.showDiagnosticsPanel() }
      } label: {
        Image(systemName: "ellipsis")
          .font(.caption.weight(.semibold))
          .foregroundStyle(.secondary)
          .frame(width: 22, height: 22)
      }
      .menuStyle(.borderlessButton)
      .menuIndicator(.hidden)
      .fixedSize()
      .accessibilityLabel("Account menu")
    }
    .padding(8)
    .background(
      RoundedRectangle(cornerRadius: 10, style: .continuous)
        .fill(hovering ? Color.primary.opacity(0.05) : Color.clear)
    )
    .onHover { hovering = $0 }
  }

  private var initials: String {
    let pieces = name
      .split(separator: " ")
      .prefix(2)
      .compactMap(\.first)
    let value = pieces.map { String($0) }.joined()
    return value.isEmpty ? "M" : value.uppercased()
  }
}
