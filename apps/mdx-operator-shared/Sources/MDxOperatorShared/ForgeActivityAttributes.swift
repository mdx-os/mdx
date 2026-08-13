#if os(iOS)
  import ActivityKit
  import Foundation

  public struct ForgeActivityAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable, Sendable {
      public let stage: String
      public let safeSummary: String
      public let needsUser: Bool

      public init(stage: String, safeSummary: String, needsUser: Bool) {
        self.stage = stage
        self.safeSummary = safeSummary
        self.needsUser = needsUser
      }
    }

    public let opaqueSessionRef: String
    public let repositoryLabel: String

    public init(opaqueSessionRef: String, repositoryLabel: String) {
      self.opaqueSessionRef = opaqueSessionRef
      self.repositoryLabel = repositoryLabel
    }
  }
#endif
