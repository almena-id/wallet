// swift-tools-version:5.5

import PackageDescription

let package = Package(
  name: "almena-call",
  platforms: [
    .iOS(.v15),
  ],
  products: [
    .library(
      name: "almena-call",
      type: .static,
      targets: ["almena-call"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "almena-call",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
