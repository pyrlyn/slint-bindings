// swift-tools-version: 6.0
// SwiftPM package for the macOS host: a C module over the Rust static library,
// an AppKit/SwiftUI wrapper, and a demo app.

import Foundation
import PackageDescription

// The library target links no Rust library: the app does. An app may link its
// own Rust library that carries `slint-bindings-ffi` (weft's editor does), and a
// second copy of Slint would not see the platform the first one installed.
// This package's demo and tests link Cargo's debug output (`just swift-build`
// builds it first). M4 adds an XCFramework binary target for plain consumers.
let packageDir = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
let rustStaticLib = packageDir
    .appendingPathComponent("../target/debug/libslint_bindings_ffi.a")
    .standardizedFileURL.path

let package = Package(
    name: "SlintBindings",
    platforms: [.macOS(.v14)],
    products: [
        .library(name: "SlintBindings", targets: ["SlintBindings"]),
        .executable(name: "SlintDemo", targets: ["SlintDemo"]),
    ],
    targets: [
        .systemLibrary(name: "CSlintBindings", path: "Sources/CSlintBindings"),
        .target(
            name: "SlintBindings",
            dependencies: ["CSlintBindings"],
            linkerSettings: [
                // What the Rust side (fontique, Slint's image and text stack) links against.
                .linkedFramework("AppKit"),
                .linkedFramework("CoreFoundation"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("CoreText"),
                .linkedFramework("Foundation"),
            ]
        ),
        .executableTarget(
            name: "SlintDemo",
            dependencies: ["SlintBindings"],
            linkerSettings: [.unsafeFlags([rustStaticLib])]
        ),
        .testTarget(
            name: "SlintBindingsTests",
            dependencies: ["SlintBindings"],
            linkerSettings: [.unsafeFlags([rustStaticLib])]
        ),
    ]
)
