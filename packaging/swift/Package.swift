// swift-tools-version:5.9
// SwiftPM manifest for the UniFFI Swift bindings of nova-ta.
// `cargo xtask build -t swift` places the generated sources in Sources/NovaTa and,
// on macOS, builds NovaTaFFI.xcframework next to this file.
// Swift packages are versioned by git tag, so there is no version field to sync.

import PackageDescription

let package = Package(
    name: "NovaTa",
    platforms: [.macOS(.v12), .iOS(.v15)],
    products: [
        .library(name: "NovaTa", targets: ["NovaTa"]),
    ],
    targets: [
        .binaryTarget(name: "NovaTaFFI", path: "NovaTaFFI.xcframework"),
        .target(
            name: "NovaTa",
            dependencies: ["NovaTaFFI"],
            path: "Sources/NovaTa",
            exclude: ["nova_taFFI.h", "nova_taFFI.modulemap"]
        ),
    ]
)
