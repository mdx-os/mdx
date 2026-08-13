// swift-tools-version: 6.0
import PackageDescription

let package = Package(
  name: "MDxOperatorShared",
  platforms: [
    .iOS(.v17),
    .macOS(.v14)
  ],
  products: [
    .library(name: "MDxOperatorShared", targets: ["MDxOperatorShared"]),
    .executable(name: "MDxOperatorSharedChecks", targets: ["MDxOperatorSharedChecks"])
  ],
  targets: [
    .target(name: "MDxOperatorShared"),
    .executableTarget(name: "MDxOperatorSharedChecks", dependencies: ["MDxOperatorShared"]),
    .testTarget(name: "MDxOperatorSharedTests", dependencies: ["MDxOperatorShared"])
  ]
)
