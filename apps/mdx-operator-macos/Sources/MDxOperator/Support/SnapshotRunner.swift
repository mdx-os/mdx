import AppKit
import SwiftUI

/// Dev-only visual snapshot harness. Renders the top product surfaces to PNGs
/// via ImageRenderer (macOS 13+) so we can SEE surfaces during design review
/// without a full-screen capture (which would grab the reviewer's private
/// windows). It drives the app's REAL SwiftUI views and the REAL OperatorStore,
/// loading live data from the local kernel (override with MDX_API_URL), so the
/// PNGs show the same views the shipping app renders, not a mock.
///
/// Runs headless when the binary is launched with `--snapshot`, writes PNGs to
/// `<package>/.snapshots/`, prints their paths, and exits. Inert otherwise, so
/// the real app entry is undisturbed. Built into the app binary for the same
/// reason as MapperChecks/ParityProbe: the Command Line Tools toolchain ships
/// no XCTest/swift-testing, and SwiftPM cannot link one executable target
/// against another, so a separate MDxSnapshots target could not reach these
/// views. Invoke with: `swift run MDxWorkbench --snapshot`.
@MainActor
enum SnapshotRunner {
  private static let renderSize = CGSize(width: 1280, height: 840)
  private static let sidebarWidth: CGFloat = 260

  private final class ResultBox { var code: Int32 = 0; var done = false }

  static func runIfRequested() {
    guard CommandLine.arguments.contains("--snapshot") else { return }
    setvbuf(stdout, nil, _IONBF, 0)
    // Pump RunLoop.main (not dispatchMain): this both services the MainActor
    // executor so store loads + ImageRenderer run on the main actor, and gives
    // AppKit a live runloop for off-screen rasterization. dispatchMain() does
    // not reliably drive the MainActor cooperative executor for this workload.
    let box = ResultBox()
    Task { @MainActor in
      box.code = await runAll()
      box.done = true
    }
    while !box.done {
      RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.02))
    }
    exit(box.code)
  }

  private static func runAll() async -> Int32 {
    let outDir = snapshotsDirectory()
    do {
      try FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)
    } catch {
      print("[snapshot] cannot create \(outDir.path): \(error)")
      return 1
    }

    let scheme: ColorScheme = ProcessInfo.processInfo.environment["MDX_SNAPSHOT_LIGHT"] != nil ? .light : .dark
    var written: [String] = []

    func shot(_ name: String, route: AppRoute, store: OperatorStore) {
      store.selectedAppRoute = route
      let view = shell(store: store)
        .environment(store)
        .frame(width: renderSize.width, height: renderSize.height)
        .preferredColorScheme(scheme)
        .background(Color(nsColor: .windowBackgroundColor))
      if let path = write(view, name: name, outDir: outDir) {
        written.append(path)
        print("[snapshot] wrote \(path)")
      } else {
        print("[snapshot] FAILED to render \(name)")
      }
    }

    // The per-surface loaders (loadTwin/loadMessage/loadPages/loadMarketplace)
    // populate their own arrays and complete cleanly headlessly. store.refresh()
    // is the exception: it assigns the Home snapshot and THEN starts a
    // never-ending foreground watch loop that monopolizes the main actor, so we
    // do Home LAST on its own store and never await refresh (poll for the
    // snapshot, render, exit). The flagship surfaces use a store that never
    // calls refresh, so nothing starves their loaders.
    let configuredBaseURL = OperatorStore.configuredBaseURL()
    print("[snapshot] loading live data from \(configuredBaseURL.absoluteString) ...")
    let flagship = OperatorStore()
    await flagship.loadTwin(); print("[snapshot] twin loaded (\(flagship.twinMessages.count) msgs)")
    await flagship.loadTwinConversations()
    await flagship.loadMessage(); print("[snapshot] message loaded")
    await flagship.loadPages(); print("[snapshot] pages loaded (\(flagship.pages.count))")
    await flagship.loadMarketplace(); print("[snapshot] marketplace loaded (\(flagship.capabilities.count))")

    if let firstPage = flagship.visiblePages.first ?? flagship.pages.first {
      flagship.selectPage(firstPage.id)
    }
    if let firstCap = flagship.visibleCapabilities.first ?? flagship.capabilities.first {
      flagship.selectCapability(firstCap.id)
    }
    if let firstPack = flagship.marketplacePacks.first {
      flagship.selectPack(firstPack.id)
    }

    shot("02-twin", route: .twin, store: flagship)
    shot("03-message", route: .message, store: flagship)
    shot("04-pages", route: .pages, store: flagship)
    shot("05-marketplace", route: .marketplace, store: flagship)

    // Offline / first-connect state: a fresh store that never loaded is honestly
    // in the .offline snapshot, which is the connect-hero surface a new user
    // sees before the kernel is up. No UserDefaults mutation.
    let offline = OperatorStore()
    shot("06-offline-home", route: .home, store: offline)
    // The honest "Not connected" empty states on the flagship reader surfaces:
    // these full-pane placeholders are direct children (not ScrollView bodies),
    // so ImageRenderer rasterizes them faithfully.
    shot("07-offline-pages", route: .pages, store: offline)
    shot("08-offline-marketplace", route: .marketplace, store: offline)
    shot("09-offline-message", route: .message, store: offline)

    // Connected Home last, on its own store, via unawaited refresh + poll.
    let homeStore = OperatorStore()
    let refreshTask = Task { await homeStore.refresh() }
    var homeConnected = false
    for _ in 0..<80 {
      if homeStore.snapshot.connectionStatus == .ok { homeConnected = true; break }
      try? await Task.sleep(nanoseconds: 250_000_000)
    }
    print("[snapshot] home snapshot status=\(homeStore.snapshot.connectionStatus) runs=\(homeStore.forgeRuns.count)")
    if homeConnected {
      shot("01-home", route: .home, store: homeStore)
      if ProcessInfo.processInfo.environment["MDX_SNAPSHOT_FLEET"] == "1" {
        let fleetView = FleetProofSnapshot(store: homeStore)
          .environment(homeStore)
          .frame(width: renderSize.width, height: renderSize.height)
          .preferredColorScheme(scheme)
          .background(Color(nsColor: .windowBackgroundColor))
        if let path = write(fleetView, name: "12-fleet", outDir: outDir) {
          written.append(path)
          print("[snapshot] wrote \(path)")
        } else {
          print("[snapshot] FAILED to render 12-fleet")
        }
      }
    } else {
      print("[snapshot] home snapshot did not connect; skipped 01-home")
    }
    refreshTask.cancel()

    let connected = homeConnected

    // Welcome / first-run cover. Rendered on a connected store with the
    // setup-router loaded so the connect step shows the REAL owner and model.
    // ImageRenderer draws these static step views faithfully (large type, cards,
    // check rows, accent). It does NOT rasterize the offline connect step's
    // base-URL TextField (renders as a placeholder glyph), which is why we
    // snapshot the CONNECTED connect step; if the kernel is down we still get a
    // faithful meet/path/set and an honest offline connect frame.
    let welcomeStore = homeConnected ? homeStore : OperatorStore()
    await welcomeStore.loadSetupRouter()
    print("[snapshot] setup-router reachable=\(welcomeStore.setupRouter.reachable) owner=\(welcomeStore.setupRouter.ownerName) model=\(welcomeStore.setupRouter.modelID)")

    func welcomeShot(_ name: String, step: WelcomeFlowView.Step) {
      let view = WelcomeFlowView(initialStep: step, animateEntrance: false)
        .environment(welcomeStore)
        .frame(width: renderSize.width, height: renderSize.height)
        .preferredColorScheme(scheme)
        .background(Color(nsColor: .windowBackgroundColor))
      if let path = write(view, name: name, outDir: outDir) {
        written.append(path)
        print("[snapshot] wrote \(path)")
      } else {
        print("[snapshot] FAILED to render \(name)")
      }
    }

    welcomeShot("07-welcome-meet", step: .meet)
    welcomeShot("08-welcome-connect", step: .connect)
    welcomeShot("09-welcome-path", step: .path)
    welcomeShot("10-welcome-set", step: .set)

    // AppKit-backed settings controls are not authoritative under
    // ImageRenderer. The real-window gallery captures those shipping surfaces
    // through capture_gallery.sh without touching the founder app.

    // Manifest so a reviewer knows what each PNG is.
    writeManifest(written: written, outDir: outDir, connected: connected, scheme: scheme)

    print("[snapshot] done. \(written.count) PNG(s) under \(outDir.path)")
    return written.isEmpty ? 1 : 0
  }

  // MARK: - Shell composition

  /// A realistic full-shell frame (sidebar + detail) without NavigationSplitView,
  /// which ImageRenderer rasterizes unreliably. The detail is the app's real
  /// DetailView, so every surface renders through the shipping view code.
  @MainActor
  private static func shell(store: OperatorStore) -> some View {
    HStack(spacing: 0) {
      SidebarView()
        .frame(width: sidebarWidth)
        .background(Color(nsColor: .underPageBackgroundColor))
      Divider()
      DetailView(searchText: .constant(""))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
  }

  /// ImageRenderer cannot rasterize macOS 26's GlassEffectContainer. This
  /// proof frame composes the shipping fleet rows directly on a plain window
  /// surface, preserving the real mapped state and recovery controls without
  /// changing or imitating the live app's Glass chrome.
  private struct FleetProofSnapshot: View {
    let store: OperatorStore

    var body: some View {
      VStack(alignment: .leading, spacing: 18) {
        HStack(alignment: .firstTextBaseline) {
          VStack(alignment: .leading, spacing: 6) {
            Text("Fleets")
              .font(.largeTitle.weight(.semibold))
            Text("Governed lane outcomes, integration, and recovery from the live local kernel.")
              .font(.callout)
              .foregroundStyle(.secondary)
          }
          Spacer()
          StatusPill(status: "Connected", tone: .positive)
        }

        Text("\(store.snapshot.fleetRunCount) fleet runs at \(store.snapshot.baseURL.absoluteString)")
          .font(.caption)
          .foregroundStyle(.secondary)

        VStack(alignment: .leading, spacing: 12) {
          ForEach(store.snapshot.fleetRuns) { run in
            FleetRunRow(
              run: run,
              openRun: { _ in },
              recover: { }
            )
          }
        }
      }
      .padding(28)
      .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
  }

  // MARK: - Rendering

  @MainActor
  private static func write(_ view: some View, name: String, outDir: URL) -> String? {
    let renderer = ImageRenderer(content: view)
    renderer.scale = 2.0
    guard let cgImage = renderer.cgImage else { return nil }
    let rep = NSBitmapImageRep(cgImage: cgImage)
    rep.size = renderSize
    guard let data = rep.representation(using: .png, properties: [:]) else { return nil }
    let url = outDir.appendingPathComponent("\(name).png")
    do {
      try data.write(to: url)
      return url.path
    } catch {
      print("[snapshot] write error for \(name): \(error)")
      return nil
    }
  }

  // MARK: - Output location

  private static func snapshotsDirectory() -> URL {
    if let override = ProcessInfo.processInfo.environment["MDX_SNAPSHOT_DIR"] {
      return URL(fileURLWithPath: override)
    }
    var dir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
    for _ in 0..<10 {
      if FileManager.default.fileExists(atPath: dir.appendingPathComponent("Package.swift").path) {
        return dir.appendingPathComponent(".snapshots")
      }
      dir = dir.deletingLastPathComponent()
    }
    return URL(fileURLWithPath: FileManager.default.currentDirectoryPath).appendingPathComponent(".snapshots")
  }

  private static func writeManifest(written: [String], outDir: URL, connected: Bool, scheme: ColorScheme) {
    let doc: [String: Any] = [
      "generated_at": ISO8601DateFormatter().string(from: Date()),
      "base_url": OperatorStore.configuredBaseURL().absoluteString,
      "kernel_connected": connected,
      "color_scheme": scheme == .light ? "light" : "dark",
      "render_size": ["width": renderSize.width, "height": renderSize.height],
      "note": "Rendered off-screen via ImageRenderer over the real DetailView + SidebarView; no full-screen capture.",
      "files": written.map { URL(fileURLWithPath: $0).lastPathComponent }.sorted()
    ]
    if let data = try? JSONSerialization.data(withJSONObject: doc, options: [.prettyPrinted, .sortedKeys]) {
      try? data.write(to: outDir.appendingPathComponent("manifest.json"))
    }
  }
}
