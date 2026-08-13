import AppKit
import Carbon.HIToolbox
import SwiftUI

/// The always-reachable layer: Option+Space summons a small floating panel
/// over whatever app is frontmost — glance at running work, ask Twin, or
/// dispatch a build — without activating or switching to MDx (the ChatGPT
/// companion-window pattern). The panel is a non-activating NSPanel so the
/// user's current app keeps focus context; the global hotkey uses Carbon's
/// RegisterEventHotKey, which needs no accessibility permission.
@MainActor
final class CompanionPanelController: NSObject, NSWindowDelegate {
  static let shared = CompanionPanelController()

  private var panel: NSPanel?
  private weak var store: OperatorStore?
  private var hotKeyRef: EventHotKeyRef?
  private var configured = false

  func configure(store: OperatorStore) {
    self.store = store
    guard !configured else { return }
    configured = true
    registerHotKey()
  }

  // MARK: - Panel lifecycle

  func toggle() {
    if let panel, panel.isVisible {
      hide()
    } else {
      show()
    }
  }

  func show() {
    guard let store else { return }
    let panel: NSPanel
    if let existing = self.panel {
      panel = existing
    } else {
      panel = NSPanel(
        contentRect: NSRect(x: 0, y: 0, width: 560, height: 150),
        styleMask: [.borderless, .nonactivatingPanel],
        backing: .buffered,
        defer: false
      )
      panel.level = .floating
      panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
      panel.isOpaque = false
      panel.backgroundColor = .clear
      panel.hasShadow = true
      panel.hidesOnDeactivate = false
      panel.isReleasedWhenClosed = false
      panel.animationBehavior = .utilityWindow
      panel.delegate = self
      panel.contentView = NSHostingView(
        rootView: CompanionView(dismiss: { [weak self] in self?.hide() })
          .environment(store)
      )
      self.panel = panel
    }

    // Center horizontally on the screen with the mouse, in the upper third.
    let mouse = NSEvent.mouseLocation
    let screen = NSScreen.screens.first { NSMouseInRect(mouse, $0.frame, false) } ?? NSScreen.main
    if let screen {
      let frame = screen.visibleFrame
      panel.setFrameOrigin(NSPoint(
        x: frame.midX - panel.frame.width / 2,
        y: frame.minY + frame.height * 0.68
      ))
    }
    panel.makeKeyAndOrderFront(nil)
  }

  func hide() {
    panel?.orderOut(nil)
  }

  nonisolated func windowDidResignKey(_ notification: Notification) {
    // Clicking away dismisses, like Spotlight.
    Task { @MainActor in self.hide() }
  }

  // MARK: - Global hotkey (Option+Space)

  private func registerHotKey() {
    var eventType = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
    InstallEventHandler(GetApplicationEventTarget(), { _, _, _ in
      Task { @MainActor in CompanionPanelController.shared.toggle() }
      return noErr
    }, 1, &eventType, nil, nil)

    let hotKeyID = EventHotKeyID(signature: OSType(0x4D44_5850), id: 1) // "MDXP"
    RegisterEventHotKey(
      UInt32(kVK_Space),
      UInt32(optionKey),
      hotKeyID,
      GetApplicationEventTarget(),
      0,
      &hotKeyRef
    )
  }
}

/// The panel's face: a glance line, one field, two verbs. Chrome material is
/// correct here — this is a transient HUD, exactly where glass belongs.
struct CompanionView: View {
  let dismiss: () -> Void
  @Environment(OperatorStore.self) private var store
  @State private var ask = ""
  @FocusState private var askFocused: Bool

  var body: some View {
    VStack(alignment: .leading, spacing: 12) {
      glanceRow

      HStack(spacing: 10) {
        Image(systemName: "sparkles")
          .foregroundStyle(Color.accentColor)
        TextField("Ask Twin, or describe a build…", text: $ask)
          .textFieldStyle(.plain)
          .font(.title3)
          .focused($askFocused)
          .onSubmit { submitTwin() }
      }
      .padding(.horizontal, 14)
      .padding(.vertical, 10)
      .background(
        RoundedRectangle(cornerRadius: 10, style: .continuous)
          .fill(Color.primary.opacity(0.06))
      )

      HStack(spacing: 12) {
        Text("Return asks Twin")
          .font(.caption)
          .foregroundStyle(.tertiary)
        Text("⌘Return starts a build")
          .font(.caption)
          .foregroundStyle(.tertiary)
        Text("Esc closes")
          .font(.caption)
          .foregroundStyle(.tertiary)
        Spacer(minLength: 0)
        Button {
          openApp()
        } label: {
          Label("Open MDx", systemImage: "arrow.up.forward.app")
            .font(.caption)
        }
        .controlSize(.small)
      }
    }
    .padding(16)
    .frame(width: 560)
    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
    .overlay(
      RoundedRectangle(cornerRadius: 14, style: .continuous)
        .stroke(Color.primary.opacity(0.12), lineWidth: 1)
    )
    .onAppear {
      ask = ""
      askFocused = true
    }
    .onKeyPress(.escape) {
      dismiss()
      return .handled
    }
    .background(hiddenShortcuts)
  }

  private var glanceRow: some View {
    HStack(spacing: 8) {
      Image(systemName: store.runningRunCount > 0 ? "hammer.fill" : "hammer")
        .foregroundStyle(store.runningRunCount > 0 ? Color.accentColor : Color.secondary)
        .symbolEffect(.pulse, isActive: store.runningRunCount > 0)
      Text(glanceText)
        .font(.callout)
        .foregroundStyle(.secondary)
      Spacer(minLength: 8)
      Circle()
        .fill(store.snapshot.connectionStatus == .ok ? Color.green : Color.orange)
        .frame(width: 7, height: 7)
        .accessibilityLabel(store.snapshot.connectionStatus.displayName)
    }
  }

  private var glanceText: String {
    var parts: [String] = []
    if store.runningRunCount > 0 { parts.append("\(store.runningRunCount) running") }
    if store.reviewReadyCount > 0 { parts.append("\(store.reviewReadyCount) ready to review") }
    if parts.isEmpty { parts.append("MDx is quiet right now") }
    return parts.joined(separator: " · ")
  }

  private var hiddenShortcuts: some View {
    Button("") { submitBuild() }
      .keyboardShortcut(.return, modifiers: [.command])
      .opacity(0)
      .frame(width: 0, height: 0)
      .accessibilityHidden(true)
  }

  private func submitTwin() {
    let trimmed = ask.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    openApp()
    store.select(.twin)
    store.sendTwin(trimmed)
    finish()
  }

  private func submitBuild() {
    let trimmed = ask.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty else { return }
    openApp()
    store.pendingBuildIntent = trimmed
    store.startBuildFlow()
    finish()
  }

  private func openApp() {
    NSApp.activate(ignoringOtherApps: true)
  }

  private func finish() {
    ask = ""
    dismiss()
  }
}
