import SwiftUI

struct ContentView: View {
  @Environment(OperatorStore.self) private var store
  @Environment(\.scenePhase) private var scenePhase
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @AppStorage(OperatorStore.showInspectorDefaultsKey) private var showInspector = false
  @State private var searchText = ""
  @State private var columnVisibility: NavigationSplitViewVisibility = .all

  var body: some View {
    ZStack {
      shellWithSheets
        // While a full-window cover (welcome or settings) is up, keep the shell
        // behind it inert so its menu commands, Cmd-K palette, and default-action
        // shortcuts cannot fire under the cover. Each cover is its own enabled
        // layer above.
        .disabled(store.welcomeShown || store.settingsShown)
      if store.settingsShown {
        SettingsCoverView(initialSection: captureSettingsSection ?? .account)
          .environment(store)
          .transition(reduceMotion ? .opacity : .opacity.combined(with: .scale(scale: 1.02)))
          .zIndex(1)
      }
      if store.welcomeShown {
        WelcomeFlowView()
          .environment(store)
          .transition(reduceMotion ? .opacity : .opacity.combined(with: .scale(scale: 1.02)))
          .zIndex(2)
      }
    }
    .animation(reduceMotion ? nil : .easeInOut(duration: 0.28), value: store.welcomeShown)
    .animation(reduceMotion ? nil : .easeInOut(duration: 0.28), value: store.settingsShown)
    .task {
      if captureSettingsSection != nil {
        store.showSettings()
      }
    }
  }

  private var captureSettingsSection: SettingsSection? {
    SettingsSection.screenshotSection()
  }

  @ViewBuilder
  private var shellWithSheets: some View {
    @Bindable var store = store
    appShell
    .task {
      CompanionPanelController.shared.configure(store: store)
      await store.refresh()
      await store.beginCanarySession()
    }
    .onChange(of: scenePhase) { _, phase in
      store.setForeground(phase == .active)
      if phase == .active { Task { await store.beginCanarySession() } }
    }
    .onChange(of: store.selectedAppRoute, initial: true) { _, route in
      // Twin, Pages, Message, and Marketplace each own a navigation rail of
      // their own. Opening them with the global shell sidebar still visible
      // produces a cramped three or four column workspace, especially in a
      // restored window. Enter those apps in focused mode and leave the native
      // sidebar toolbar control available when someone wants global navigation
      // back. Home, Forge, and You keep the app-wide sidebar visible.
      withAnimation(reduceMotion ? nil : .easeOut(duration: 0.18)) {
        switch route {
        case .twin, .pages, .message, .memory, .marketplace:
          columnVisibility = .detailOnly
        case .home, .forge, .you:
          columnVisibility = .all
        }
      }
      Task { await store.recordCanarySurfaceVisit(route) }
    }
    .sheet(isPresented: Binding(
      get: { store.commandPaletteShown },
      set: { if !$0 { store.hideCommandPalette() } }
    )) {
      CommandPaletteView()
        .environment(store)
    }
    .sheet(isPresented: $store.showDiagnostics) {
      DiagnosticsDetailView()
        .environment(store)
    }
    .sheet(isPresented: $store.showFeedback) {
      FeedbackSheet()
        .environment(store)
    }
    .sheet(isPresented: $store.showPRHandoff) {
      PRHandoffSheet()
        .environment(store)
    }
  }

  // The toolbar search drives the CURRENT surface's real query, and only
  // appears on surfaces that actually have something to search - no dead box.
  private var searchableSurface: Bool {
    switch store.selectedAppRoute {
    case .forge, .pages, .twin, .message, .memory, .marketplace: return true
    case .home, .you: return false
    }
  }

  private var surfaceSearch: Binding<String> {
    switch store.selectedAppRoute {
    case .marketplace:
      return Binding(get: { store.marketplaceSearch }, set: { store.marketplaceSearch = $0 })
    case .pages:
      return Binding(get: { store.pageSearch }, set: { store.pageSearch = $0; store.runPageSearch($0) })
    case .twin:
      return Binding(get: { store.twinConvoSearch }, set: { store.twinConvoSearch = $0 })
    case .message:
      return Binding(get: { store.messageSearch }, set: { store.messageSearch = $0 })
    case .memory:
      return Binding(get: { store.memorySearch }, set: { store.memorySearch = $0 })
    default:
      return $searchText
    }
  }

  private var searchPrompt: String {
    switch store.selectedAppRoute {
    case .pages: return "Search pages"
    case .twin: return "Search conversations"
    case .message: return "Search Message"
    case .memory: return "Search Memory"
    case .marketplace: return "Search capabilities"
    case .forge(.fleet): return "Search fleets"
    case .forge(.machines): return "Search machines"
    case .forge(.missions): return "Search missions"
    case .forge(.review): return "Search review"
    case .forge(.evidence): return "Search evidence"
    case .forge(.overview): return "Search Forge"
    default: return "Search runs"
    }
  }

  @ViewBuilder
  private var appShell: some View {
    let base = NavigationSplitView(columnVisibility: $columnVisibility) {
      SidebarView()
        .navigationSplitViewColumnWidth(min: 230, ideal: 260, max: 310)
    } detail: {
      DetailView(searchText: $searchText)
        .frame(minWidth: 500)
        .inspector(isPresented: $showInspector) {
          InspectorView()
            .inspectorColumnWidth(min: 300, ideal: 340, max: 420)
        }
    }
    .navigationSplitViewStyle(.balanced)

    Group {
      if searchableSurface {
        base.searchable(text: surfaceSearch, placement: .toolbar, prompt: searchPrompt)
      } else {
        base
      }
    }
    .toolbar {
      ToolbarItemGroup(placement: .navigation) {
        Button {
          store.navigateBack()
        } label: {
          Label("Back", systemImage: "chevron.left")
        }
        .disabled(!store.canNavigateBack)
        .help("Go back")

        Button {
          store.navigateForward()
        } label: {
          Label("Forward", systemImage: "chevron.right")
        }
        .disabled(!store.canNavigateForward)
        .help("Go forward")
      }

      ToolbarItem(placement: .principal) {
        RepoContextSwitcher()
      }

      if #available(macOS 26.0, *) {
        ToolbarSpacer(.fixed)
      }

      ToolbarItemGroup {
        Button {
          showInspector.toggle()
        } label: {
          Label(showInspector ? "Hide Context" : "Show Context", systemImage: "sidebar.right")
        }
        .keyboardShortcut("i", modifiers: [.command, .option])
        .help(showInspector ? "Hide context panel (⌘⌥I)" : "Show context panel (⌘⌥I)")

        // Toolbar restraint: the palette (Cmd-K, View menu) and refresh (Cmd-R,
        // View menu, plus auto-refresh) no longer take permanent toolbar slots.
        Button {
          store.showSettings()
        } label: {
          Label("Settings", systemImage: "gearshape")
        }
        .help("Open settings")
      }
    }
  }
}
