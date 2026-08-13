// Window-id helper for the visual-QA capture rail.
//
// Prints the CoreGraphics window number of the MDx app's main window so the
// capture script can do a WINDOW-SCOPED `screencapture -l<id>` and never a
// full-screen grab (which would capture the reviewer's private windows).
//
// Selection matches the app's real window: owner name "MDx" (the bundle display
// name, not the product "MDxWorkbench"), window layer 0 (normal windows, not
// menu-bar/panel chrome), and width > 400 (the main workspace, not the compact
// run-monitor tear-off or the menu-bar popover). If several qualify, the largest
// by area wins. Prints nothing and exits non-zero when no window matches, so the
// caller can wait and retry rather than capture the wrong surface.
//
// Usage: winid [ownerName]   (ownerName defaults to "MDx")
// Compile once: swiftc -O -o <dir>/winid script/winid.swift

import CoreGraphics
import Foundation

let owner = CommandLine.arguments.dropFirst().first ?? "MDx"

guard let windows = CGWindowListCopyWindowInfo(
  [.optionOnScreenOnly, .excludeDesktopElements],
  kCGNullWindowID
) as? [[String: Any]] else {
  FileHandle.standardError.write(Data("winid: could not read the window list\n".utf8))
  exit(1)
}

var best: (id: Int, area: CGFloat)?
for window in windows {
  guard let ownerName = window[kCGWindowOwnerName as String] as? String, ownerName == owner else { continue }
  guard let layer = window[kCGWindowLayer as String] as? Int, layer == 0 else { continue }
  guard let bounds = window[kCGWindowBounds as String] as? [String: CGFloat],
        let width = bounds["Width"], let height = bounds["Height"], width > 400 else { continue }
  guard let id = window[kCGWindowNumber as String] as? Int else { continue }
  let area = width * height
  if best == nil || area > best!.area { best = (id, area) }
}

guard let best else {
  FileHandle.standardError.write(Data("winid: no on-screen window for owner \"\(owner)\"\n".utf8))
  exit(2)
}
print(best.id)
