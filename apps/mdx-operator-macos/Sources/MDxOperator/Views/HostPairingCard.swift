import AppKit
import CoreImage
import CoreImage.CIFilterBuiltins
import MDxOperatorShared
import SwiftUI

struct HostPairingCard: View {
  @StateObject private var model: HostPairingStore
  @State private var copiedPayload: String?
  private let autoPrepare: Bool

  init(
    baseURL: URL,
    accessTokenProvider: MobileAccessTokenProvider? = nil,
    autoPrepare: Bool = true
  ) {
    self.autoPrepare = autoPrepare
    _model = StateObject(
      wrappedValue: HostPairingStore(
        baseURL: baseURL,
        accessTokenProvider: accessTokenProvider
      )
    )
  }

  var body: some View {
    SettingsGroup {
      VStack(alignment: .leading, spacing: 14) {
        HStack {
          Label("Forge Anywhere", systemImage: "iphone.and.arrow.forward")
            .font(.headline)
          Spacer()
          connectionBadge
        }

        Text(
          "Pair your iPhone to start, steer, and review work while this Mac stays the execution host."
        )
        .font(.subheadline)
        .foregroundStyle(.secondary)

        switch model.state {
        case .loading:
          HStack {
            ProgressView()
            Text("Preparing this Mac")
          }
        case .ready:
          readyContent
        case .code(let payload, let expiresAt):
          pairingCode(payload: payload, expiresAt: expiresAt)
        case .paired:
          pairedContent
        case .failed(let message):
          Label(message, systemImage: "exclamationmark.triangle.fill")
            .foregroundStyle(.orange)
          Button("Try again") { Task { await model.prepare() } }
        }
      }
      .padding(.vertical, 8)
    }
    .task {
      if autoPrepare { await model.prepare() }
    }
  }

  @ViewBuilder
  private var connectionBadge: some View {
    switch model.state {
    case .paired:
      Text("Paired").foregroundStyle(.green)
    case .code:
      Text("Waiting for iPhone").foregroundStyle(.orange)
    default:
      Text("Outbound only").foregroundStyle(.secondary)
    }
  }

  private var readyContent: some View {
    VStack(alignment: .leading, spacing: 12) {
      if model.availableDevices.isEmpty {
        Text("Sign in to MDx on your iPhone, then refresh.")
          .foregroundStyle(.secondary)
      } else {
        Picker("Pair with", selection: $model.selectedDeviceID) {
          ForEach(model.availableDevices) { device in
            Text(device.displayName).tag(Optional(device.deviceID))
          }
        }
        Button("Show pairing code") { Task { await model.makePairingCode() } }
          .mdxPrimaryButtonStyle()
      }
      Button("Refresh devices") { Task { await model.prepare() } }
        .buttonStyle(.link)
    }
  }

  private func pairingCode(payload: String, expiresAt: Date) -> some View {
    HStack(alignment: .top, spacing: 18) {
      if let image = QRCode.image(for: payload) {
        Image(nsImage: image)
          .interpolation(.none)
          .resizable()
          .frame(width: 168, height: 168)
          .accessibilityLabel("One-time iPhone pairing code")
      }
      VStack(alignment: .leading, spacing: 10) {
        Text("Pair with MDx on iPhone")
          .font(.headline)
        Text("You > Trusted devices > Pair Mac")
          .foregroundStyle(.secondary)
        Text("Expires \(expiresAt, style: .relative)")
          .font(.caption.monospacedDigit())
        Text("The code works once and is bound to your account, tenant, iPhone, and this Mac.")
          .font(.caption)
          .foregroundStyle(.secondary)
        HStack(spacing: 8) {
          Button {
            Pasteboard.copy(payload)
            copiedPayload = payload
          } label: {
            Label(
              copiedPayload == payload ? "Copied" : "Copy pairing code",
              systemImage: copiedPayload == payload ? "checkmark" : "doc.on.doc"
            )
          }
          .accessibilityIdentifier("trust.copy-pairing-code")

          Button("Make a new code") { Task { await model.makePairingCode() } }
        }
        Text("Copy and paste the code when the iPhone camera is unavailable.")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
    }
  }

  private var pairedContent: some View {
    VStack(alignment: .leading, spacing: 10) {
      Label("This Mac is available to your iPhone", systemImage: "checkmark.seal.fill")
        .foregroundStyle(.green)
      Text("MDx uses one outbound encrypted connection. No inbound listener is opened.")
        .font(.subheadline)
        .foregroundStyle(.secondary)
      Button("Rotate host key") { Task { await model.rotateHostKey() } }
        .buttonStyle(.link)
    }
  }
}

private enum QRCode {
  static func image(for value: String) -> NSImage? {
    let filter = CIFilter.qrCodeGenerator()
    filter.message = Data(value.utf8)
    filter.correctionLevel = "M"
    guard let output = filter.outputImage?.transformed(by: CGAffineTransform(scaleX: 10, y: 10))
    else { return nil }
    let representation = CIContext().createCGImage(output, from: output.extent)
    guard let representation else { return nil }
    return NSImage(cgImage: representation, size: NSSize(width: 168, height: 168))
  }
}
