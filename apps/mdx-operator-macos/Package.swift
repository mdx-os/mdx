// swift-tools-version: 6.0
import PackageDescription

let package = Package(
  name: "MDxWorkbench",
  platforms: [
    .macOS(.v14)
  ],
  products: [
    .executable(name: "MDxWorkbench", targets: ["MDxWorkbench"])
  ],
  dependencies: [
    .package(path: "../mdx-operator-shared"),
    .package(url: "https://github.com/supabase/supabase-swift.git", exact: "2.46.0")
  ],
  targets: [
    .executableTarget(
      name: "MDxWorkbench",
      dependencies: [
        .product(name: "MDxOperatorShared", package: "mdx-operator-shared"),
        .product(name: "Supabase", package: "supabase-swift")
      ],
      path: "Sources/MDxOperator"
    ),
    .testTarget(
      name: "MDxOperatorTests",
      dependencies: ["MDxWorkbench"],
      path: "Tests",
      exclude: ["Fixtures"]
    )
  ]
)
