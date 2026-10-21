// AppKit view that presents a SlintHost's frames and feeds it input.
// GPU path: a CAMetalLayer sublayer. The view's own layer stays, so a failed
// Metal device can still present CPU frames as CGImage contents.

import AppKit
import CSlintBindings
import QuartzCore

@MainActor
public final class SlintNSView: NSView {
    /// Declared before `host` so it is released after the Rust surface, which retains this layer.
    private var metalLayer: CAMetalLayer?
    public private(set) var host: SlintHost?
    public private(set) var lastError: SlintError?
    private var displayLink: CADisplayLink?
    private var trackingArea: NSTrackingArea?
    /// Marked (preedit) text from the input context. Nil when nothing is composing.
    private var markedText: String?
    /// The key event `handleEvent` is currently interpreting, so `doCommand` can map it.
    private var currentKeyEvent: NSEvent?
    /// Set when `insertText` or `setMarkedText` already consumed the key.
    private var inputClientHandled = false
    /// Key code whose key-up should be dropped because the input method swallowed the press.
    private var suppressKeyUpCode: UInt16?

    /// Shows the host `makeHost` returns, which may come from another library
    /// (`SlintHost(adopting:)`). With `prefersGpu`, `makeHost` first gets a
    /// `CAMetalLayer` to present into; if it throws, or returns a CPU host,
    /// the layer goes away and a throw is retried with nil, the CPU path.
    /// The size the host is made at does not matter: the view resizes it.
    public init(
        frame frameRect: NSRect,
        prefersGpu: Bool = true,
        makeHost: (_ metalLayer: UnsafeMutableRawPointer?) throws -> SlintHost
    ) {
        super.init(frame: frameRect)
        install(prefersGpu: prefersGpu, makeHost: makeHost)
    }

    /// Shows a host made elsewhere. A GPU host needs the layer at creation, so
    /// it goes through `init(frame:prefersGpu:makeHost:)` instead.
    public convenience init(frame frameRect: NSRect, host: SlintHost) {
        self.init(frame: frameRect, prefersGpu: false) { _ in host }
    }

    /// Shows one component of `definition`. `prefersGpu: false` keeps the CPU
    /// `CGImage` path. Snapshot tests use it; the demo leaves the default,
    /// which is the Metal layer.
    public convenience init(frame frameRect: NSRect, definition: SlintDefinition, prefersGpu: Bool = true) {
        self.init(frame: frameRect, prefersGpu: prefersGpu) { metalLayer in
            guard let metalLayer else {
                return try SlintHost(definition: definition, pixelWidth: 1, pixelHeight: 1, scale: 1)
            }
            return try SlintHost(definition: definition, metalLayer: metalLayer, pixelWidth: 1, pixelHeight: 1, scale: 1)
        }
    }

    private func install(prefersGpu: Bool, makeHost: (UnsafeMutableRawPointer?) throws -> SlintHost) {
        wantsLayer = true
        // Top-left keeps a frame that is one size behind the bounds where it is,
        // instead of stretching it across the view until the new pixels arrive.
        layer?.contentsGravity = .topLeft
        layer?.magnificationFilter = .nearest
        if prefersGpu {
            let metal = CAMetalLayer()
            metal.pixelFormat = .bgra8Unorm
            metal.framebufferOnly = true
            metal.isOpaque = false
            metal.frame = bounds
            layer?.addSublayer(metal)
            metalLayer = metal
            let made = try? makeHost(Unmanaged.passUnretained(metal).toOpaque())
            if made?.rendersOnGpu != true {
                metal.removeFromSuperlayer()
                metalLayer = nil
            }
            if let made {
                host = made
                return
            }
        }
        do {
            host = try makeHost(nil)
        } catch let error as SlintError {
            lastError = error
        } catch {
            lastError = SlintError(description: "\(error)")
        }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    // Slint's origin is the top-left corner, so a flipped view needs no y conversion.
    public override var isFlipped: Bool { true }
    public override var acceptsFirstResponder: Bool { true }

    public override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        displayLink?.invalidate()
        displayLink = nil
        guard window != nil else { return }
        // Frames follow the display this view is on, including ProMotion rates.
        let link = displayLink(target: self, selector: #selector(step(_:)))
        link.add(to: .main, forMode: .common)
        displayLink = link
        syncSize()
    }

    public override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        syncSize()
    }

    public override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        syncSize()
    }

    public override func viewWillStartLiveResize() {
        super.viewWillStartLiveResize()
        // Present in the same transaction as the layer resize, so the frame is
        // not stretched or black while the user drags the window edge.
        metalLayer?.presentsWithTransaction = true
    }

    public override func viewDidEndLiveResize() {
        super.viewDidEndLiveResize()
        metalLayer?.presentsWithTransaction = false
        present()
    }

    private func syncSize() {
        let scale = window?.backingScaleFactor ?? 1
        layer?.contentsScale = scale
        let w = Int((bounds.width * scale).rounded())
        let h = Int((bounds.height * scale).rounded())
        if let metalLayer {
            metalLayer.contentsScale = scale
            metalLayer.frame = bounds
            metalLayer.drawableSize = CGSize(width: max(w, 1), height: max(h, 1))
        }
        do {
            try host?.resize(pixelWidth: w, pixelHeight: h, scale: scale)
        } catch let error as SlintError {
            lastError = error
            return
        } catch {
            lastError = SlintError(description: "\(error)")
            return
        }
        // The layer adopts the new bounds in this call. Present before returning so
        // the displayed image matches them; the display link only has to catch animations.
        present()
    }

    @objc private func step(_ link: CADisplayLink) {
        SlintHost.tick()
        present()
    }

    /// Paints and shows a frame if anything changed. The display link calls it
    /// each vsync; tests call it directly.
    func present() {
        guard let host else { return }
        do {
            if host.rendersOnGpu {
                _ = try host.renderGpu()
            } else if let image = try host.renderIfNeeded().image {
                layer?.contents = image
            }
        } catch let error as SlintError {
            lastError = error
        } catch {
            lastError = SlintError(description: "\(error)")
        }
    }

    // MARK: Pointer

    public override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let trackingArea { removeTrackingArea(trackingArea) }
        let area = NSTrackingArea(
            rect: .zero,
            options: [.mouseMoved, .mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self
        )
        addTrackingArea(area)
        trackingArea = area
    }

    private func point(_ event: NSEvent) -> NSPoint {
        convert(event.locationInWindow, from: nil)
    }

    public override func mouseMoved(with event: NSEvent) {
        let p = point(event)
        host?.pointerMoved(x: p.x, y: p.y)
    }

    public override func mouseDragged(with event: NSEvent) { mouseMoved(with: event) }
    public override func rightMouseDragged(with event: NSEvent) { mouseMoved(with: event) }
    public override func otherMouseDragged(with event: NSEvent) { mouseMoved(with: event) }

    public override func mouseDown(with event: NSEvent) {
        window?.makeFirstResponder(self)
        press(event, .left)
    }

    public override func mouseUp(with event: NSEvent) { release(event, .left) }
    public override func rightMouseDown(with event: NSEvent) { press(event, .right) }
    public override func rightMouseUp(with event: NSEvent) { release(event, .right) }
    public override func otherMouseDown(with event: NSEvent) { press(event, .middle) }
    public override func otherMouseUp(with event: NSEvent) { release(event, .middle) }

    public override func mouseExited(with event: NSEvent) {
        host?.pointerExited()
    }

    public override func scrollWheel(with event: NSEvent) {
        let p = point(event)
        // Precise devices report points. A mouse wheel reports lines; Slint wants points.
        let unit: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 16
        host?.pointerScrolled(
            x: p.x, y: p.y,
            dx: event.scrollingDeltaX * unit,
            dy: event.scrollingDeltaY * unit
        )
    }

    private func press(_ event: NSEvent, _ button: SlintPointerButton) {
        let p = point(event)
        host?.pointerPressed(x: p.x, y: p.y, button: button)
    }

    private func release(_ event: NSEvent, _ button: SlintPointerButton) {
        let p = point(event)
        host?.pointerReleased(x: p.x, y: p.y, button: button)
    }

    // MARK: Keyboard

    public override func keyDown(with event: NSEvent) {
        inputClientHandled = false
        currentKeyEvent = event
        // The input context turns dead keys, CJK and the emoji picker into
        // `setMarkedText` / `insertText`, and named bindings (arrows, Tab)
        // into `doCommand`. A false return means neither happened.
        let handled = inputContext?.handleEvent(event) ?? false
        if !handled && !inputClientHandled {
            deliver(event)
        } else if markedText != nil {
            suppressKeyUpCode = event.keyCode
        }
        currentKeyEvent = nil
    }

    public override func keyUp(with event: NSEvent) {
        if suppressKeyUpCode == event.keyCode {
            suppressKeyUpCode = nil
            return
        }
        guard let text = SlintKeys.text(for: event) else { return super.keyUp(with: event) }
        host?.keyReleased(text)
    }

    public override func flagsChanged(with event: NSEvent) {
        guard let text = SlintKeys.text(for: event) else { return }
        if modifierIsDown(event) {
            host?.keyPressed(text)
        } else {
            host?.keyReleased(text)
        }
    }

    public override func doCommand(by selector: Selector) {
        // Key bindings (moveLeft:, insertTab:, …) land here. The original
        // event still carries the key code, so the same map names the key.
        guard !inputClientHandled, let event = currentKeyEvent else {
            super.doCommand(by: selector)
            return
        }
        inputClientHandled = true
        deliver(event)
    }

    public override func becomeFirstResponder() -> Bool {
        host?.focusChanged(true)
        return true
    }

    public override func resignFirstResponder() -> Bool {
        // Leaving the view ends the composition. `unmarkText` commits a
        // pending reading, which is what the input context does on focus loss.
        if markedText != nil { unmarkText() }
        host?.focusChanged(false)
        return true
    }

    private func deliver(_ event: NSEvent) {
        guard let text = SlintKeys.text(for: event) else {
            super.keyDown(with: event)
            return
        }
        if event.isARepeat {
            host?.keyRepeated(text)
        } else {
            sendText(text)
        }
    }

    private func sendText(_ text: String) {
        let accepted = host?.keyPressed(text) ?? false
        // Slint rejected Tab / Backtab: nothing else in the view can take focus.
        if !accepted && sb_last_error() == nil && (text == "\t" || text == "\u{19}") {
            moveFocus(forward: text == "\t")
        }
    }

    private func moveFocus(forward: Bool) {
        guard let window else { return }
        if forward {
            window.selectNextKeyView(self)
        } else {
            window.selectPreviousKeyView(self)
        }
    }

    /// Device-dependent bits inside `NSEvent.modifierFlags` (the NX_* masks).
    /// They say which side is down, so releasing Left Shift while Right Shift
    /// is held does not look like a fresh press of the shared Shift flag.
    private func modifierIsDown(_ event: NSEvent) -> Bool {
        let bit: UInt
        switch event.keyCode {
        case 0x38: bit = 0x0002 // left shift
        case 0x3C: bit = 0x0004 // right shift
        case 0x3B: bit = 0x0001 // left control
        case 0x3E: bit = 0x2000 // right control
        case 0x3A: bit = 0x0020 // left option
        case 0x3D: bit = 0x0040 // right option
        case 0x37: bit = 0x0008 // left command
        case 0x36: bit = 0x0010 // right command
        case 0x39: return event.modifierFlags.contains(.capsLock)
        default: return false
        }
        return event.modifierFlags.rawValue & bit != 0
    }

    isolated deinit {
        displayLink?.invalidate()
    }
}

extension SlintNSView: @MainActor NSTextInputClient {
    public func insertText(_ string: Any, replacementRange: NSRange) {
        inputClientHandled = true
        let text = Self.plain(string)
        if markedText != nil {
            markedText = nil
            // The input method replaced the preedit with the committed string.
            host?.commitComposition(text)
            return
        }
        if text.isEmpty { return }
        // A single key's character, or a multi-scalar insert such as an emoji.
        if text.utf16.count == 1 {
            sendText(text)
        } else {
            host?.commitComposition(text)
        }
    }

    public func setMarkedText(_ string: Any, selectedRange: NSRange, replacementRange: NSRange) {
        // `replacementRange` edits already-committed text. The preedit itself
        // is replaced by the new marked string, which is the case the demo's
        // input method uses (NSNotFound means "the current marked range").
        _ = replacementRange
        inputClientHandled = true
        let text = Self.plain(string)
        if text.isEmpty {
            markedText = nil
            host?.updateComposition("", utf16Start: -1, utf16End: -1)
            return
        }
        markedText = text
        let (start, end) = Self.utf16Bounds(selectedRange, limit: (text as NSString).length)
        host?.updateComposition(text, utf16Start: start, utf16End: end)
    }

    public func unmarkText() {
        let pending = markedText ?? ""
        markedText = nil
        if pending.isEmpty {
            host?.updateComposition("", utf16Start: -1, utf16End: -1)
        } else {
            host?.commitComposition(pending)
        }
    }

    public func selectedRange() -> NSRange {
        NSRange(location: NSNotFound, length: 0)
    }

    public func markedRange() -> NSRange {
        guard let markedText else { return NSRange(location: NSNotFound, length: 0) }
        return NSRange(location: 0, length: (markedText as NSString).length)
    }

    public func hasMarkedText() -> Bool { markedText != nil }

    public func attributedSubstring(forProposedRange range: NSRange, actualRange: NSRangePointer?) -> NSAttributedString? {
        guard let markedText else { return nil }
        let marked = markedText as NSString
        let full = NSRange(location: 0, length: marked.length)
        let clipped = range.location == NSNotFound ? full : NSIntersectionRange(range, full)
        if clipped.length == 0 { return nil }
        actualRange?.pointee = clipped
        return NSAttributedString(string: marked.substring(with: clipped))
    }

    public func firstRect(forCharacterRange range: NSRange, actualRange: NSRangePointer?) -> NSRect {
        actualRange?.pointee = range
        // Slint 1.18 does not tell a custom platform where the caret is, so the
        // candidate window anchors to the text field's place in the demo form.
        guard let window else { return .zero }
        let local = NSRect(x: 16, y: 48, width: 2, height: 24)
        return window.convertToScreen(convert(local, to: nil))
    }

    public func characterIndex(for point: NSPoint) -> Int { NSNotFound }

    public func validAttributesForMarkedText() -> [NSAttributedString.Key] { [] }

    private static func plain(_ string: Any) -> String {
        if let text = string as? String { return text }
        if let text = string as? NSAttributedString { return text.string }
        return ""
    }

    /// `NSNotFound` becomes `-1`, which the core treats as "no selection".
    private static func utf16Bounds(_ range: NSRange, limit: Int) -> (Int32, Int32) {
        guard range.location != NSNotFound else { return (-1, -1) }
        let start = min(range.location, limit)
        let end = min(range.location + range.length, limit)
        return (Int32(start), Int32(end))
    }
}

/// AppKit key events to Slint key text. The table lives in Rust
/// (`appkit_key_text`) so the tests can cover every `slint::platform::Key`
/// without a Mac.
enum SlintKeys {
    static func text(for event: NSEvent) -> String? {
        var buffer = [CChar](repeating: 0, count: 16)
        let mapped = buffer.withUnsafeMutableBufferPointer { raw -> Bool in
            sb_appkit_key_text(
                event.keyCode,
                event.characters ?? "",
                event.charactersIgnoringModifiers ?? "",
                UInt32(truncatingIfNeeded: event.modifierFlags.rawValue),
                raw.baseAddress,
                raw.count
            )
        }
        guard mapped else { return nil }
        return String(cString: buffer)
    }
}
