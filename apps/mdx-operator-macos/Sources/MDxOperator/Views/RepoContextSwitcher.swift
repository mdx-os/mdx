import SwiftUI

/// The persistent context anchor in the toolbar: the repo every governed action
/// is aimed at, plus its branch. Switching here changes `selectedRepoID`, which
/// the composer, scout, and start-build flows all read, so context stops being
/// implicit.
///
/// Branch note: the kernel projects no persistent base branch per repo (Forge
/// cuts a fresh branch for each run), so the branch line shows the most recent
/// run's branch for the active repo when there is one, and the honest per-run
/// default otherwise.
struct RepoContextSwitcher: View {
  @Environment(OperatorStore.self) private var store

  var body: some View {
    Group {
      if let active = store.activeRepo {
        menu(active: active)
      }
    }
    .onAppear(perform: applyShotSelection)
  }

  private func menu(active: ForgeRepo) -> some View {
    let branch = branchLine(for: active)
    let real = RepoContext.hasRealBranch(latestRunBranch: store.latestRunBranch(forRepoID: active.id))
    return Menu {
      Text("Anchor work to")
      ForEach(store.repos) { repo in
        Button {
          store.selectRepo(repo.id)
        } label: {
          if repo.id == active.id {
            Label(repo.label, systemImage: "checkmark")
          } else {
            Text(repo.label)
          }
        }
      }
      Divider()
      Button("Open Forge") { store.select(.forge(.overview)) }
    } label: {
      HStack(spacing: 7) {
        Image(systemName: "shippingbox")
          .font(.callout)
          .foregroundStyle(.secondary)
        // One concatenated Text so the toolbar renders the whole anchor (repo and
        // branch) as an atomic unit instead of truncating the trailing views.
        (
          Text(active.label).font(.callout.weight(.medium))
            + Text("    ").font(.callout)
            + Text(Image(systemName: real ? "arrow.triangle.branch" : "point.3.connected.trianglepath.dotted"))
              .font(.caption).foregroundColor(.secondary)
            + Text(" \(branch)").font(.callout).foregroundColor(.secondary)
        )
        .lineLimit(1)
      }
    }
    .menuStyle(.borderlessButton)
    .fixedSize()
    .help("Switch the repo and branch every governed action is anchored to")
    .accessibilityLabel("Active repo \(active.label), branch \(branch). Switch context.")
  }

  private func branchLine(for repo: ForgeRepo) -> String {
    RepoContext.branchLine(latestRunBranch: store.latestRunBranch(forRepoID: repo.id))
  }

  /// Deterministic capture: MDX_SHOT_REPO=<repo_id> pre-selects a repo so a
  /// screenshot can prove the anchor changed context without synthetic clicks.
  private func applyShotSelection() {
    guard let id = ProcessInfo.processInfo.environment["MDX_SHOT_REPO"], !id.isEmpty else { return }
    store.selectRepo(id)
  }
}
