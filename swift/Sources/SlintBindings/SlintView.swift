// SwiftUI wrapper for any host, including one another library made.

import AppKit
import SwiftUI

/// A `SlintNSView` as a SwiftUI view. The host is made once, when SwiftUI
/// creates the view; reach it afterwards through the closure that made it.
public struct SlintView: NSViewRepresentable {
    private let make: @MainActor () -> SlintNSView

    /// `makeHost` as in `SlintNSView.init(frame:prefersGpu:makeHost:)`: it
    /// gets a `CAMetalLayer` pointer to present into, or nil for the CPU path.
    public init(
        prefersGpu: Bool = true,
        makeHost: @escaping @MainActor (_ metalLayer: UnsafeMutableRawPointer?) throws -> SlintHost
    ) {
        make = { SlintNSView(frame: .zero, prefersGpu: prefersGpu, makeHost: makeHost) }
    }

    /// One component of `definition`.
    public init(definition: SlintDefinition, prefersGpu: Bool = true) {
        make = { SlintNSView(frame: .zero, definition: definition, prefersGpu: prefersGpu) }
    }

    public func makeNSView(context: Context) -> SlintNSView { make() }

    public func updateNSView(_ view: SlintNSView, context: Context) {}
}
