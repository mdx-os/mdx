import AppKit

/// Escape hatches: MDx never traps the work. Detects installed editors by
/// their URL schemes and opens repos/files in Finder, Terminal, Cursor,
/// VS Code, or Zed. Trust to leave the IDE comes from knowing the way back.
@MainActor
enum EditorOpener {
  struct Destination: Identifiable {
    let id: String
    let label: String
    let icon: String
    let open: (_ path: String, _ line: Int?) -> Void
  }

  /// Editors present on this Mac (scheme-detected), plus Finder and Terminal.
  static func available() -> [Destination] {
    var destinations: [Destination] = [
      Destination(id: "finder", label: "Finder", icon: "folder") { path, _ in
        NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)])
      },
      Destination(id: "terminal", label: "Terminal", icon: "terminal") { path, _ in
        let dir = directory(of: path)
        NSWorkspace.shared.open(
          [URL(fileURLWithPath: dir)],
          withApplicationAt: URL(fileURLWithPath: "/System/Applications/Utilities/Terminal.app"),
          configuration: NSWorkspace.OpenConfiguration()
        )
      },
    ]
    let editors: [(id: String, label: String, scheme: String, urlBuilder: (String, Int?) -> String)] = [
      ("cursor", "Cursor", "cursor", { path, line in "cursor://file\(path)\(line.map { ":\($0)" } ?? "")" }),
      ("vscode", "VS Code", "vscode", { path, line in "vscode://file\(path)\(line.map { ":\($0)" } ?? "")" }),
      ("zed", "Zed", "zed", { path, line in "zed://file\(path)\(line.map { ":\($0)" } ?? "")" }),
    ]
    for editor in editors {
      guard let probe = URL(string: "\(editor.scheme)://probe"),
            NSWorkspace.shared.urlForApplication(toOpen: probe) != nil else { continue }
      destinations.append(Destination(id: editor.id, label: editor.label, icon: "chevron.left.forwardslash.chevron.right") { path, line in
        if let url = URL(string: editor.urlBuilder(path, line)) {
          NSWorkspace.shared.open(url)
        }
      })
    }
    return destinations
  }

  private static func directory(of path: String) -> String {
    var isDirectory: ObjCBool = false
    FileManager.default.fileExists(atPath: path, isDirectory: &isDirectory)
    return isDirectory.boolValue ? path : (path as NSString).deletingLastPathComponent
  }
}
