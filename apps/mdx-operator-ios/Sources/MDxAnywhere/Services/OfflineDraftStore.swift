import Foundation
import MDxOperatorShared

struct OfflineDraftStore {
  private let fileManager: FileManager
  private let baseURL: URL?

  init(fileManager: FileManager = .default, baseURL: URL? = nil) {
    self.fileManager = fileManager
    self.baseURL = baseURL
  }

  func load() -> [MobileOfflineDraft] {
    guard let data = try? Data(contentsOf: fileURL) else { return [] }
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .iso8601
    return (try? decoder.decode([MobileOfflineDraft].self, from: data)) ?? []
  }

  func save(_ drafts: [MobileOfflineDraft]) throws {
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    let data = try encoder.encode(Array(drafts.suffix(50)))
    let directory = fileURL.deletingLastPathComponent()
    try fileManager.createDirectory(at: directory, withIntermediateDirectories: true)
    try data.write(to: fileURL, options: [.atomic, .completeFileProtection])
  }

  func deleteAll() throws {
    guard fileManager.fileExists(atPath: fileURL.path) else { return }
    try fileManager.removeItem(at: fileURL)
  }

  private var fileURL: URL {
    let base =
      baseURL
      ?? fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
      ?? fileManager.temporaryDirectory
    return base.appending(path: "MDxAnywhere/offline-drafts.json")
  }
}
