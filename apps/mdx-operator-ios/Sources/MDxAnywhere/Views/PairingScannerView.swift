import SwiftUI
import UIKit
import VisionKit

struct PairingScannerView: View {
  let onCode: (String) -> Void

  @State private var pasteError: String?

  private var cameraAvailable: Bool {
    DataScannerViewController.isSupported && DataScannerViewController.isAvailable
  }

  var body: some View {
    Group {
      if cameraAvailable {
        PairingCameraView(onCode: onCode)
          .ignoresSafeArea()
      } else {
        VStack(spacing: 14) {
          Image(systemName: "qrcode.viewfinder")
            .font(.system(size: 42))
            .foregroundStyle(.secondary)
          Text("Camera pairing is unavailable")
            .font(.title3.weight(.semibold))
          Text("Copy the one-time pairing code from MDx on your Mac, then paste it here.")
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
        }
        .padding(32)
      }
    }
    .safeAreaInset(edge: .bottom) {
      VStack(spacing: 8) {
        if cameraAvailable {
          Text("Scan the code shown in MDx Settings on your Mac.")
            .font(.footnote)
            .foregroundStyle(.secondary)
        }
        if let pasteError {
          Text(pasteError)
            .font(.footnote)
            .foregroundStyle(.orange)
        }
        Button {
          pastePairingCode()
        } label: {
          Label("Paste pairing code", systemImage: "doc.on.clipboard")
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .accessibilityIdentifier("pairing.paste-code")
      }
      .padding(.horizontal, 20)
      .padding(.vertical, 14)
      .background(.regularMaterial)
    }
  }

  private func pastePairingCode() {
    guard let value = UIPasteboard.general.string,
      !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    else {
      pasteError = "Copy a fresh pairing code from your Mac first."
      return
    }
    pasteError = nil
    onCode(value)
  }
}

private struct PairingCameraView: UIViewControllerRepresentable {
  let onCode: (String) -> Void

  func makeCoordinator() -> Coordinator {
    Coordinator(onCode: onCode)
  }

  func makeUIViewController(context: Context) -> UIViewController {
    let scanner = DataScannerViewController(
      recognizedDataTypes: [.barcode(symbologies: [.qr])],
      qualityLevel: .balanced,
      recognizesMultipleItems: false,
      isHighFrameRateTrackingEnabled: false,
      isPinchToZoomEnabled: true,
      isGuidanceEnabled: true,
      isHighlightingEnabled: true
    )
    scanner.delegate = context.coordinator
    try? scanner.startScanning()
    return scanner
  }

  func updateUIViewController(_ uiViewController: UIViewController, context: Context) {}

  final class Coordinator: NSObject, DataScannerViewControllerDelegate {
    private let onCode: (String) -> Void
    private var delivered = false

    init(onCode: @escaping (String) -> Void) {
      self.onCode = onCode
    }

    func dataScanner(
      _ dataScanner: DataScannerViewController,
      didAdd addedItems: [RecognizedItem],
      allItems: [RecognizedItem]
    ) {
      guard !delivered else { return }
      for item in addedItems {
        guard case .barcode(let barcode) = item,
          let value = barcode.payloadStringValue
        else { continue }
        delivered = true
        dataScanner.stopScanning()
        onCode(value)
        return
      }
    }
  }
}
