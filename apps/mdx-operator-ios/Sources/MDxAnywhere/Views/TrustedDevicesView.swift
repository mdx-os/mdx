import MDxOperatorShared
import SwiftUI

struct TrustedDevicesView: View {
  @StateObject private var trust: MobileTrustStore
  @State private var scannerPresented = false

  init(accessTokenProvider: MobileAccessTokenProvider? = nil) {
    _trust = StateObject(
      wrappedValue: MobileTrustStore(accessTokenProvider: accessTokenProvider)
    )
  }

  var body: some View {
    List {
      statusSection
      devicesSection
      hostsSection
      securitySection
    }
    .navigationTitle("Trusted devices")
    .navigationBarTitleDisplayMode(.inline)
    .mobileFocusedDestination()
    .toolbar {
      ToolbarItem(placement: .primaryAction) {
        Button("Pair Mac", systemImage: "qrcode.viewfinder") {
          scannerPresented = true
        }
        .accessibilityIdentifier("trust.pair-mac")
      }
    }
    .sheet(isPresented: $scannerPresented) {
      NavigationStack {
        PairingScannerView(
          onCode: { value in
            scannerPresented = false
            Task { await trust.redeem(payload: value) }
          }
        )
        .navigationTitle("Pair your Mac")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
          ToolbarItem(placement: .cancellationAction) {
            Button("Cancel") { scannerPresented = false }
          }
        }
      }
    }
    .task { await trust.prepare() }
    .refreshable { try? await trust.refresh() }
  }

  @ViewBuilder
  private var statusSection: some View {
    Section {
      switch trust.state {
      case .loading:
        HStack {
          ProgressView()
          Text("Securing this iPhone")
        }
      case .pairing:
        HStack {
          ProgressView()
          Text("Verifying the one-time code")
        }
      case .paired:
        Label("Mac paired", systemImage: "checkmark.seal.fill")
          .foregroundStyle(.green)
      case .failed(let message):
        Label(message, systemImage: "exclamationmark.triangle.fill")
          .foregroundStyle(.orange)
      case .ready:
        Label("Ready to pair", systemImage: "lock.shield.fill")
      }
    } footer: {
      Text("Codes expire after 60 seconds and work once. MDx never opens a port on your Mac.")
    }
  }

  @ViewBuilder
  private var devicesSection: some View {
    Section("iPhones") {
      if let devices = trust.projection?.devices, !devices.isEmpty {
        ForEach(devices) { device in
          TrustRow(
            title: device.displayName,
            detail: device.deviceID == trust.thisDeviceID ? "This iPhone" : device.platform,
            state: device.status,
            icon: "iphone.gen3"
          )
          .swipeActions {
            if device.deviceID != trust.thisDeviceID, device.status == "active" {
              Button("Revoke", role: .destructive) {
                Task { await trust.revokeDevice(device.deviceID) }
              }
            }
          }
        }
      } else {
        Text("No registered iPhones")
          .foregroundStyle(.secondary)
      }
    }
  }

  @ViewBuilder
  private var hostsSection: some View {
    Section("Macs") {
      if let hosts = trust.projection?.hosts, !hosts.isEmpty {
        ForEach(hosts) { host in
          TrustRow(
            title: host.displayName,
            detail: host.connectorStatus == "online" ? "Available now" : "Offline",
            state: host.status,
            icon: "macbook"
          )
          .swipeActions {
            if host.status == "active" {
              Button("Revoke", role: .destructive) {
                Task { await trust.revokeHost(host.hostID) }
              }
            }
          }
        }
      } else {
        Text("Open MDx Settings on your Mac to make it available here.")
          .foregroundStyle(.secondary)
      }
    }
  }

  private var securitySection: some View {
    Section("Security") {
      LabeledContent(
        "App integrity", value: trust.attestationPosture.replacingOccurrences(of: "_", with: " "))
      LabeledContent("Host connection", value: "Outbound only")
      LabeledContent("Secrets on iPhone", value: "None")
    }
  }
}

private struct TrustRow: View {
  let title: String
  let detail: String
  let state: String
  let icon: String

  var body: some View {
    Label {
      HStack {
        VStack(alignment: .leading) {
          Text(title)
          Text(detail).font(.caption).foregroundStyle(.secondary)
        }
        Spacer()
        Text(state.capitalized)
          .font(.caption.weight(.semibold))
          .foregroundStyle(state == "active" ? .green : .secondary)
      }
    } icon: {
      Image(systemName: icon)
    }
  }
}
