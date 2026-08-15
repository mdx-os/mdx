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
    .package(url: "https://github.com/sparkle-project/Sparkle.git", exact: "2.9.4"),
    .package(url: "https://github.com/supabase/supabase-swift.git", exact: "2.46.0")
  ],
  targets: [
    .executableTarget(
      name: "MDxWorkbench",
      dependencies: [
        .product(name: "MDxOperatorShared", package: "mdx-operator-shared"),
        .product(name: "Sparkle", package: "Sparkle"),
        .product(name: "Supabase", package: "supabase-swift")
      ],
      path: "Sources/MDxOperator",
      linkerSettings: [
        .unsafeFlags(["-Xlinker", "-rpath", "-Xlinker", "@executable_path/../Frameworks"])
      ]
    ),
    .testTarget(
      name: "MDxOperatorTests",
      dependencies: ["MDxWorkbench"],
      path: "Tests",
      exclude: ["Fixtures"]
    )
  ]
)
