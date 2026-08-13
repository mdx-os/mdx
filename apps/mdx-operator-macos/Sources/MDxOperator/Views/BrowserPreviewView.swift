import AppKit
import SwiftUI
import WebKit

struct BrowserPreviewView: View {
  @Environment(OperatorStore.self) private var store
  let run: ForgeRun

  @AppStorage("MDxBrowserPreviewURL") private var previewURL = "http://127.0.0.1:5173"
  @AppStorage("MDxBrowserPreviewPath") private var previewPath = "apps/mdx-host"
  @State private var viewport = BrowserViewport.desktop
  @State private var reloadToken = 0
  @State private var snapshotToken = 0
  @State private var nativeSnapshot: NSImage?
  @State private var navigationStatus = "Enter a local preview URL or let Forge render the review branch."
  @State private var showLivePreview = false

  var body: some View {
    VStack(alignment: .leading, spacing: 18) {
      auditControls
      if let result = store.browserAudit {
        auditResult(result)
      } else if let error = store.browserAuditError {
        browserNotice(title: "Forge could not review this render", detail: error)
      } else {
        browserNotice(
          title: "Review the branch as a customer would",
          detail: "Forge will render the selected package from this run's isolated branch and keep the screenshot, accessibility, console, network, and trace evidence together.",
          systemImage: "eye",
          tint: .accentColor
        )
      }
      DisclosureGroup(isExpanded: $showLivePreview) {
        livePreview
          .padding(.top, 12)
      } label: {
        VStack(alignment: .leading, spacing: 3) {
          Text("Open a running local URL")
            .font(.headline)
          Text("Optional interactive WebKit preview for a server you started yourself")
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      }
      .padding(16)
      .background(Color.primary.opacity(0.025), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
    .frame(maxWidth: 940, alignment: .leading)
    .task(id: run.id) {
      // External repos commonly expose a root index instead of MDx's host
      // package. Keep an explicit user choice, but repair the old global
      // default when opening a run from another connected repository.
      if run.repo != "mdx", previewPath == "apps/mdx-host" {
        previewPath = "."
      }
    }
  }

  private var livePreview: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .firstTextBaseline) {
        VStack(alignment: .leading, spacing: 4) {
          Text("Local live URL")
            .font(.title3.weight(.semibold))
          Text("Optional: point this at a running loopback server for an interactive WebKit preview.")
            .font(.callout)
            .foregroundStyle(.secondary)
        }
        Spacer()
        StatusPill(status: "Optional", tone: .neutral)
      }

      HStack(spacing: 8) {
        TextField("http://127.0.0.1:5173", text: $previewURL)
          .textFieldStyle(.roundedBorder)
          .font(.system(.callout, design: .monospaced))
          .onSubmit { reloadToken += 1 }
        Button("Open") { reloadToken += 1 }
          .buttonStyle(.borderedProminent)
        Button { reloadToken += 1 } label: {
          Image(systemName: "arrow.clockwise")
        }
        .buttonStyle(.bordered)
        .help("Reload preview")
        Button { snapshotToken += 1 } label: {
          Label("Capture", systemImage: "camera")
        }
        .buttonStyle(.bordered)
      }

      ZStack {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
          .fill(Color(nsColor: .windowBackgroundColor))
        LocalWebPreview(
          urlString: previewURL,
          reloadToken: reloadToken,
          snapshotToken: snapshotToken,
          onStatus: { navigationStatus = $0 },
          onSnapshot: { nativeSnapshot = $0 }
        )
        .clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
      }
      .overlay {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
          .stroke(Color.primary.opacity(0.12), lineWidth: 1)
      }
      .frame(minHeight: 470)

      HStack {
        Image(systemName: "lock.shield")
        Text(navigationStatus)
        Spacer()
        Text("Loopback page and resources · ephemeral cookies")
      }
      .font(.caption)
      .foregroundStyle(.secondary)

      if let nativeSnapshot {
        DisclosureGroup("Latest native capture") {
          Image(nsImage: nativeSnapshot)
            .resizable()
            .scaledToFit()
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .padding(.top, 8)
        }
        .font(.callout.weight(.medium))
      }
    }
    .padding(.top, 2)
  }

  private var auditControls: some View {
    VStack(alignment: .leading, spacing: 12) {
      HStack(alignment: .firstTextBaseline) {
        VStack(alignment: .leading, spacing: 4) {
          Text("Let Forge inspect what it built")
            .font(.title3.weight(.semibold))
          Text("Forge checks out this run's branch, renders the package in Chromium, and preserves the screenshot, accessibility tree, console, network, and trace evidence.")
            .font(.callout)
            .foregroundStyle(.secondary)
        }
        Spacer()
        if store.browserAuditInFlight {
          ProgressView()
            .controlSize(.small)
        }
      }

      HStack(spacing: 12) {
        VStack(alignment: .leading, spacing: 4) {
          Text("Package to review")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
          TextField("Repository root", text: previewPathDisplay)
            .textFieldStyle(.roundedBorder)
            .font(.system(.callout, design: .monospaced))
            .frame(minWidth: 220)
          Text(previewPath == "." ? "Repository root" : previewPath)
            .font(.caption2)
            .foregroundStyle(.tertiary)
        }
        VStack(alignment: .leading, spacing: 4) {
          Text("Viewport")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.secondary)
          Picker("Viewport", selection: $viewport) {
            ForEach(BrowserViewport.allCases) { item in
              Text(item.label).tag(item)
            }
          }
          .labelsHidden()
          .frame(width: 130)
        }
        Spacer()
      }

      HStack(spacing: 10) {
        Button {
          Task { await store.auditSelectedRun(previewPath: previewPath, viewport: viewport, autoRevise: false) }
        } label: {
          Label("Review render", systemImage: "eye")
        }
        .buttonStyle(.borderedProminent)
        .disabled(!run.hasBranch || store.browserAuditInFlight)

        if store.browserAudit?.approved == false || store.browserEvolutionActive {
          Button {
            store.startBrowserEvolution(previewPath: previewPath, viewport: viewport)
          } label: {
            Label("Repair and re-review", systemImage: "wand.and.sparkles")
          }
          .buttonStyle(.bordered)
          .disabled(!run.hasBranch || store.browserAuditInFlight)
        }

        if store.browserEvolutionActive {
          Button("Stop") { store.stopBrowserEvolution() }
            .buttonStyle(.bordered)
          Text("Cycle \(store.browserEvolutionCycle) · \(store.browserEvolutionRunID)")
            .font(.caption.monospaced())
            .foregroundStyle(.secondary)
        }

        Text(store.browserAudit?.approved == false
          ? "Up to two governed repairs. Shipping still requires your decision."
          : "Review first. Repair only appears when the measured render needs it.")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
    .padding(18)
    .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
  }

  private func auditResult(_ result: BrowserAuditResult) -> some View {
    VStack(alignment: .leading, spacing: 14) {
      HStack(alignment: .top) {
        VStack(alignment: .leading, spacing: 5) {
          Text(result.approved ? "Forge approves this render" : "Forge found a revision")
            .font(.title3.weight(.semibold))
          Text(result.critique)
            .font(.callout)
            .textSelection(.enabled)
        }
        Spacer()
        StatusPill(status: result.verdict.capitalized, tone: result.approved ? .positive : .locked)
      }

      if let data = result.screenshot, let image = NSImage(data: data) {
        Image(nsImage: image)
          .resizable()
          .scaledToFit()
          .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
          .overlay {
            RoundedRectangle(cornerRadius: 10, style: .continuous)
              .stroke(Color.primary.opacity(0.12), lineWidth: 1)
          }
      }

      if !result.findings.isEmpty {
        VStack(alignment: .leading, spacing: 8) {
          Text("Measured findings")
            .font(.headline)
          ForEach(result.findings) { finding in
            HStack(alignment: .top, spacing: 9) {
              Image(systemName: finding.severity == "high" ? "exclamationmark.triangle.fill" : "circle.fill")
                .font(finding.severity == "high" ? .callout : .system(size: 7))
                .foregroundStyle(finding.severity == "high" ? Color.orange : Color.secondary)
                .frame(width: 16, height: 18)
              VStack(alignment: .leading, spacing: 2) {
                Text(finding.title).font(.callout.weight(.semibold))
                Text(finding.detail).font(.caption).foregroundStyle(.secondary)
              }
            }
          }
        }
      }

      HStack(spacing: 14) {
        Label("\(result.consoleErrorCount) runtime errors", systemImage: "terminal")
        Label("\(result.networkFailureCount) failed resources", systemImage: "network")
        Label(result.critiqueModel, systemImage: "brain")
        Spacer()
        if result.revisionStarted {
          Text("Repair run \(result.revisionRunID)")
            .foregroundStyle(.orange)
        }
      }
      .font(.caption)
      .foregroundStyle(.secondary)
    }
    .padding(18)
    .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 16, style: .continuous))
  }

  private func browserNotice(title: String, detail: String) -> some View {
    browserNotice(title: title, detail: detail, systemImage: "exclamationmark.triangle", tint: .orange)
  }

  private func browserNotice(title: String, detail: String, systemImage: String, tint: Color) -> some View {
    HStack(alignment: .top, spacing: 10) {
      Image(systemName: systemImage)
        .foregroundStyle(tint)
      VStack(alignment: .leading, spacing: 3) {
        Text(title).font(.callout.weight(.semibold))
        Text(detail).font(.caption).foregroundStyle(.secondary)
      }
    }
    .padding(14)
    .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
  }

  private var previewPathDisplay: Binding<String> {
    Binding(
      get: { previewPath == "." ? "" : previewPath },
      set: { value in
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        previewPath = trimmed.isEmpty ? "." : trimmed
      }
    )
  }
}

private struct LocalWebPreview: NSViewRepresentable {
  let urlString: String
  let reloadToken: Int
  let snapshotToken: Int
  let onStatus: (String) -> Void
  let onSnapshot: (NSImage) -> Void

  func makeCoordinator() -> Coordinator {
    Coordinator(onStatus: onStatus, onSnapshot: onSnapshot)
  }

  func makeNSView(context: Context) -> WKWebView {
    let configuration = WKWebViewConfiguration()
    configuration.websiteDataStore = .nonPersistent()
    configuration.defaultWebpagePreferences.allowsContentJavaScript = true
    let webView = WKWebView(frame: .zero, configuration: configuration)
    webView.navigationDelegate = context.coordinator
    webView.isInspectable = true
    context.coordinator.webView = webView
    context.coordinator.configureNetworkBoundaryAndLoad(urlString, in: webView)
    return webView
  }

  func updateNSView(_ webView: WKWebView, context: Context) {
    context.coordinator.onStatus = onStatus
    context.coordinator.onSnapshot = onSnapshot
    if context.coordinator.loadedURL != urlString || context.coordinator.reloadToken != reloadToken {
      context.coordinator.reloadToken = reloadToken
      context.coordinator.load(urlString, in: webView)
    }
    if context.coordinator.snapshotToken != snapshotToken {
      context.coordinator.snapshotToken = snapshotToken
      Task { @MainActor in
        do {
          let image = try await webView.takeSnapshot(configuration: nil)
          context.coordinator.onSnapshot(image)
          context.coordinator.onStatus("Native WebKit capture complete.")
        } catch {
          context.coordinator.onStatus("Capture failed: \(error.localizedDescription)")
        }
      }
    }
  }

  final class Coordinator: NSObject, WKNavigationDelegate {
    weak var webView: WKWebView?
    var loadedURL = ""
    var reloadToken = -1
    var snapshotToken = -1
    var networkBoundaryReady = false
    var pendingURL = ""
    var onStatus: (String) -> Void
    var onSnapshot: (NSImage) -> Void

    init(onStatus: @escaping (String) -> Void, onSnapshot: @escaping (NSImage) -> Void) {
      self.onStatus = onStatus
      self.onSnapshot = onSnapshot
    }

    func configureNetworkBoundaryAndLoad(_ raw: String, in webView: WKWebView) {
      pendingURL = raw
      let rules = #"[{"trigger":{"url-filter":"^https?://","unless-domain":["localhost","127.0.0.1"]},"action":{"type":"block"}},{"trigger":{"url-filter":"^wss?://","unless-domain":["localhost","127.0.0.1"]},"action":{"type":"block"}}]"#
      WKContentRuleListStore.default().compileContentRuleList(
        forIdentifier: "com.mdx.forge.loopback-only",
        encodedContentRuleList: rules
      ) { [weak self, weak webView] ruleList, error in
        Task { @MainActor in
          guard let self, let webView else { return }
          guard let ruleList else {
            self.onStatus("Preview stayed closed because its loopback boundary could not load: \(error?.localizedDescription ?? "unknown error")")
            return
          }
          webView.configuration.userContentController.add(ruleList)
          self.networkBoundaryReady = true
          self.load(self.pendingURL, in: webView)
        }
      }
    }

    func load(_ raw: String, in webView: WKWebView) {
      pendingURL = raw
      guard networkBoundaryReady else {
        onStatus("Preparing the loopback-only browser boundary.")
        return
      }
      loadedURL = raw
      guard let url = URL(string: raw), isLoopback(url) else {
        onStatus("Preview stays on http://localhost or http://127.0.0.1.")
        return
      }
      onStatus("Loading \(url.absoluteString)")
      webView.load(URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 20))
    }

    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction) async -> WKNavigationActionPolicy {
      guard let url = navigationAction.request.url, isLoopback(url) else {
        onStatus("Blocked navigation outside the local preview boundary.")
        return .cancel
      }
      return .allow
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
      onStatus("Rendered \(webView.url?.absoluteString ?? loadedURL)")
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
      onStatus("Preview failed: \(error.localizedDescription)")
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
      onStatus("Preview is not running yet: \(error.localizedDescription)")
    }

    private func isLoopback(_ url: URL) -> Bool {
      guard ["http", "https"].contains(url.scheme?.lowercased() ?? "") else { return false }
      return ["localhost", "127.0.0.1"].contains(url.host?.lowercased() ?? "")
    }
  }
}
