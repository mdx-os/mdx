import AppKit
import SwiftUI

/// Copy text to the general pasteboard (context-menu verb backing).
enum Pasteboard {
  static func copy(_ text: String) {
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(text, forType: .string)
  }
}
