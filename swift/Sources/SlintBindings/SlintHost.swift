// Swift owner of one embedded Slint component (the `sb_*` C ABI).
// Main-actor only: Slint objects are single-threaded and the Rust side refuses
// nothing on its own, so the isolation is what keeps calls on one thread.

import CSlintBindings
import CoreGraphics
import Foundation

/// A failure reported by the Rust side.
public struct SlintError: Error, CustomStringConvertible {
    public let description: String

    static func last() -> SlintError {
        SlintError(description: sb_last_error().map { String(cString: $0) } ?? "unknown error")
    }
}

/// Mouse buttons, in the order the C ABI expects.
public enum SlintPointerButton: UInt8 {
    case left = 0
    case right = 1
    case middle = 2
}

@MainActor
private final class CallbackBox {
    let handler: @MainActor (String) -> Void
    init(_ handler: @escaping @MainActor (String) -> Void) { self.handler = handler }
}

/// A `.slint` file compiled at run time: its main (last exported) component.
@MainActor
public final class SlintDefinition {
    fileprivate let handle: OpaquePointer

    /// Compiles `source`, or the file at `path` when `source` is nil. `path`
    /// also names the file in diagnostics and resolves relative imports.
    public init(path: String, source: String? = nil) throws {
        let compiled = source.map { sb_compile($0, path) } ?? sb_compile(nil, path)
        guard let compiled else { throw SlintError.last() }
        handle = compiled
    }

    isolated deinit {
        sb_definition_free(handle)
    }
}

/// One component of a `SlintDefinition`. CPU frames land in a buffer this
/// object owns; a Metal host presents into the `CAMetalLayer` passed at init.
@MainActor
public final class SlintHost {
    private let handle: OpaquePointer
    private var buffer: [UInt8] = []
    private var callbacks: [String: Unmanaged<CallbackBox>] = [:]
    public private(set) var pixelWidth = 1
    public private(set) var pixelHeight = 1
    public let rendersOnGpu: Bool

    /// Takes ownership of an `SbHost*` that another library made, such as a
    /// Rust crate that links `slint-bindings-ffi` and returns hosts from its
    /// own C functions. Pass that call's result directly: nil means it failed
    /// and left the reason in `sb_last_error`. The host is freed with this object.
    public init(adopting handle: OpaquePointer?) throws {
        guard let handle else { throw SlintError.last() }
        var width: UInt32 = 0
        var height: UInt32 = 0
        guard sb_host_pixel_size(handle, &width, &height) else {
            let error = SlintError.last()
            sb_host_free(handle)
            throw error
        }
        self.handle = handle
        self.pixelWidth = Int(width)
        self.pixelHeight = Int(height)
        self.rendersOnGpu = sb_host_is_gpu(handle)
    }

    /// Creates the component at `width`×`height` physical pixels, on the CPU.
    public convenience init(definition: SlintDefinition, pixelWidth: Int, pixelHeight: Int, scale: CGFloat) throws {
        try self.init(adopting: sb_component_new(
            definition.handle,
            UInt32(max(pixelWidth, 1)),
            UInt32(max(pixelHeight, 1)),
            Float(scale)
        ))
    }

    /// Creates the component presenting into `metalLayer` (`CAMetalLayer`).
    /// The layer must stay alive until this host is released.
    public convenience init(
        definition: SlintDefinition,
        metalLayer: UnsafeMutableRawPointer,
        pixelWidth: Int,
        pixelHeight: Int,
        scale: CGFloat
    ) throws {
        try self.init(adopting: sb_component_new_metal(
            definition.handle,
            metalLayer,
            UInt32(max(pixelWidth, 1)),
            UInt32(max(pixelHeight, 1)),
            Float(scale)
        ))
    }

    isolated deinit {
        // Clear the callbacks first so Rust never calls into a released box.
        for (name, box) in callbacks {
            _ = sb_on_callback(handle, name, nil, nil)
            box.release()
        }
        sb_host_free(handle)
    }

    /// Advances Slint timers and animations for every host in the process.
    public static func tick() {
        sb_tick()
    }

    public func resize(pixelWidth: Int, pixelHeight: Int, scale: CGFloat) throws {
        let (w, h) = (max(pixelWidth, 1), max(pixelHeight, 1))
        guard sb_host_resize(handle, UInt32(w), UInt32(h), Float(scale)) else { throw SlintError.last() }
        self.pixelWidth = w
        self.pixelHeight = h
    }

    /// Renders if the scene changed. `image` is nil when the previous frame is still current.
    public func renderIfNeeded() throws -> (image: CGImage?, animating: Bool) {
        let len = sb_host_frame_len(handle)
        if buffer.count != len {
            buffer = [UInt8](repeating: 0, count: len)
        }
        let frame = buffer.withUnsafeMutableBufferPointer { sb_host_render(handle, $0.baseAddress, $0.count) }
        if frame.failed != 0 { throw SlintError.last() }
        guard frame.redrawn != 0 else { return (nil, frame.animating != 0) }
        return (makeImage(), frame.animating != 0)
    }

    /// Presents one GPU frame into the layer passed at init.
    @discardableResult
    public func renderGpu() throws -> (redrawn: Bool, animating: Bool) {
        let frame = sb_host_gpu_render(handle)
        if frame.failed != 0 { throw SlintError.last() }
        return (frame.redrawn != 0, frame.animating != 0)
    }

    private func makeImage() -> CGImage? {
        // Copy: CoreAnimation may read the image after the next render reuses `buffer`.
        guard let provider = CGDataProvider(data: Data(buffer) as CFData),
              let space = CGColorSpace(name: CGColorSpace.sRGB) else { return nil }
        let bytesPerPixel = 4
        return CGImage(
            width: pixelWidth, height: pixelHeight,
            bitsPerComponent: 8, bitsPerPixel: 8 * bytesPerPixel, bytesPerRow: pixelWidth * bytesPerPixel,
            space: space,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
        )
    }

    // Input. Positions are logical points from the top-left corner, as AppKit
    // reports them in a flipped view.

    public func pointerMoved(x: CGFloat, y: CGFloat) {
        _ = sb_host_pointer_moved(handle, Float(x), Float(y))
    }

    public func pointerPressed(x: CGFloat, y: CGFloat, button: SlintPointerButton) {
        _ = sb_host_pointer_pressed(handle, Float(x), Float(y), button.rawValue)
    }

    public func pointerReleased(x: CGFloat, y: CGFloat, button: SlintPointerButton) {
        _ = sb_host_pointer_released(handle, Float(x), Float(y), button.rawValue)
    }

    public func pointerExited() {
        _ = sb_host_pointer_exited(handle)
    }

    public func pointerScrolled(x: CGFloat, y: CGFloat, dx: CGFloat, dy: CGFloat) {
        _ = sb_host_pointer_scrolled(handle, Float(x), Float(y), Float(dx), Float(dy))
    }

    /// Returns whether Slint accepted the key. `false` with a nil `sb_last_error`
    /// means the scene rejected it (Tab when nothing else inside the view can take focus).
    @discardableResult
    public func keyPressed(_ text: String) -> Bool {
        sb_host_key_pressed(handle, text)
    }

    /// Returns false only when the call failed. A release the scene ignores is still success.
    @discardableResult
    public func keyReleased(_ text: String) -> Bool {
        sb_host_key_released(handle, text)
    }

    /// Shows `preedit` as uncommitted text. Offsets are UTF-16 code units; pass `-1` for no selection.
    public func updateComposition(_ preedit: String, utf16Start: Int32, utf16End: Int32) {
        _ = sb_host_composition_update(handle, preedit, utf16Start, utf16End)
    }

    /// Inserts `text` and clears the preedit.
    public func commitComposition(_ text: String) {
        _ = sb_host_composition_commit(handle, text)
    }

    public func keyRepeated(_ text: String) {
        _ = sb_host_key_repeated(handle, text)
    }

    public func focusChanged(_ focused: Bool) {
        _ = sb_host_focus_changed(handle, focused)
    }

    // Accessibility. The view showing this host forwards its NSAccessibility
    // entry points here; the elements are AccessKit's, built from Slint's item tree.

    /// Exposes the component to VoiceOver through `view` (an `NSView`, held weakly). Once per host.
    public func attachAccessibility(to view: AnyObject) throws {
        guard sb_host_accessibility_attach(handle, Unmanaged.passUnretained(view).toOpaque()) else {
            throw SlintError.last()
        }
    }

    /// The view's `accessibilityChildren`; nil before `attachAccessibility` or on failure.
    public func accessibilityChildren() -> [Any]? {
        guard let array = sb_host_accessibility_children(handle) else { return nil }
        return Unmanaged<NSArray>.fromOpaque(array).takeUnretainedValue() as? [Any]
    }

    /// The element inside the view that has keyboard focus, if any.
    public func accessibilityFocusedElement() -> Any? {
        sb_host_accessibility_focus(handle).map { Unmanaged<AnyObject>.fromOpaque($0).takeUnretainedValue() }
    }

    /// The element at `point`, in screen coordinates.
    public func accessibilityHitTest(_ point: CGPoint) -> Any? {
        sb_host_accessibility_hit_test(handle, point.x, point.y)
            .map { Unmanaged<AnyObject>.fromOpaque($0).takeUnretainedValue() }
    }

    // Properties and callbacks, as JSON values: `"text"`, `42`, `true`,
    // `"#ff0000"`, `"Enum.value"`, arrays for models, objects for structs.

    public func setProperty(_ name: String, json: String) throws {
        guard sb_set_property_json(handle, name, json) else { throw SlintError.last() }
    }

    public func property(_ name: String) throws -> String {
        try Self.take(sb_get_property_json(handle, name))
    }

    /// Calls the callback `name` with a JSON array of arguments; returns its JSON result.
    @discardableResult
    public func invoke(_ name: String, argsJson: String = "[]") throws -> String {
        try Self.take(sb_invoke_json(handle, name, argsJson))
    }

    /// Handles the callback `name`; `handler` gets its arguments as a JSON array.
    public func onCallback(_ name: String, _ handler: @escaping @MainActor (String) -> Void) throws {
        let box = Unmanaged.passRetained(CallbackBox(handler))
        let registered = sb_on_callback(handle, name, { userData, args in
            guard let userData, let args else { return }
            let json = String(cString: args)
            // Slint invokes callbacks synchronously while dispatching input or
            // `invoke`, which only ever happens from the main-actor methods above.
            MainActor.assumeIsolated {
                Unmanaged<CallbackBox>.fromOpaque(userData).takeUnretainedValue().handler(json)
            }
        }, box.toOpaque())
        guard registered else {
            box.release()
            throw SlintError.last()
        }
        // The replaced closure, the only holder of the old box, is gone now.
        callbacks.updateValue(box, forKey: name)?.release()
    }

    private static func take(_ text: UnsafeMutablePointer<CChar>?) throws -> String {
        guard let text else { throw SlintError.last() }
        defer { sb_string_free(text) }
        return String(cString: text)
    }
}
