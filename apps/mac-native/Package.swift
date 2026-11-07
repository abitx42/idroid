// swift-tools-version: 5.9
import Foundation
import PackageDescription

let coreLibraryDirectory = ProcessInfo.processInfo.environment["IDROID_CORE_FFI_DIR"]
    ?? "../../../target/release"

let package = Package(
    name: "idroid-mac-native",
    platforms: [.macOS(.v13)],
    products: [
        .executable(name: "idroid-mac-native", targets: ["IdroidMac"]),
    ],
    targets: [
        .target(
            name: "IdroidCoreFFI",
            path: "Sources/IdroidCoreFFI",
            publicHeadersPath: "include"
        ),
        .executableTarget(
            name: "IdroidMac",
            dependencies: ["IdroidCoreFFI"],
            path: "Sources/IdroidMac",
            linkerSettings: [
                .unsafeFlags(["-L\(coreLibraryDirectory)", "-lidroid_core_ffi"]),
                .linkedFramework("AppKit"),
                .linkedFramework("CoreImage"),
                .linkedFramework("Network"),
                .linkedFramework("Security"),
            ]
        ),
    ]
)
