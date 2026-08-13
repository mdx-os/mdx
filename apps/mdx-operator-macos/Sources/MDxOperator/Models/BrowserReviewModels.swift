import Foundation

struct BrowserFinding: Identifiable, Equatable {
  let id: String
  let severity: String
  let title: String
  let detail: String
  let selector: String
}

struct BrowserAuditResult: Equatable {
  let auditID: String
  let runID: String
  let branch: String
  let previewPath: String
  let url: String
  let screenshot: Data?
  let screenshotSHA256: String
  let tracePath: String
  let verdict: String
  let critique: String
  let critiqueModel: String
  let findings: [BrowserFinding]
  let consoleErrorCount: Int
  let networkFailureCount: Int
  let revisionStarted: Bool
  let revisionRunID: String

  var approved: Bool { verdict.caseInsensitiveCompare("APPROVE") == .orderedSame }
  var needsRevision: Bool { !approved }
}

enum BrowserViewport: String, CaseIterable, Identifiable {
  case desktop
  case laptop
  case tablet
  case phone

  var id: String { rawValue }

  var label: String {
    switch self {
    case .desktop: "Desktop"
    case .laptop: "Laptop"
    case .tablet: "Tablet"
    case .phone: "Phone"
    }
  }

  var size: CGSize {
    switch self {
    case .desktop: CGSize(width: 1440, height: 900)
    case .laptop: CGSize(width: 1280, height: 800)
    case .tablet: CGSize(width: 820, height: 1180)
    case .phone: CGSize(width: 390, height: 844)
    }
  }
}
