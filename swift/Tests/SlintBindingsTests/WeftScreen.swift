// A screen weft's `weft-slint` generated (fixtures/weft/login.slint), shown in
// a SwiftUI window the way weft's editor shows it: the host comes from a C
// constructor (`SlintHost(adopting:)`), and input arrives as AppKit events.
// The events go to the view's responder methods: a test process is not the
// active app, so its window never becomes key, and AppKit keeps a click in a
// window that is not key for activating it.
// Hit targets in logical points at 480 wide: the email field at y 84, the
// submit button at y 184, "Create an account" at (349, 220).

import AppKit
import CSlintBindings
@testable import SlintBindings
import SwiftUI
import Testing

@MainActor
struct WeftScreenInSwiftUI {
    private let email = CGPoint(x: 100, y: 84)
    private let submit = CGPoint(x: 100, y: 184)
    private let signup = CGPoint(x: 349, y: 220)

    private var fixturePath: String {
        URL(fileURLWithPath: #filePath)
            .appendingPathComponent("../../../../fixtures/weft/login.slint")
            .standardizedFileURL.path
    }

    @Test func generatedScreenShowsAndTakesInputInASwiftUIWindow() throws {
        // Stands in for a library such as weft's `ws_surface_new`: raw `sb_*`
        // calls hand Swift an `SbHost*` it did not create.
        let definition = try #require(sb_compile(nil, fixturePath), "\(SlintError.last())")
        defer { sb_definition_free(definition) }
        var made: SlintHost?
        let root = SlintView(prefersGpu: false) { _ in
            let host = try SlintHost(adopting: sb_component_new(definition, 1, 1, 1))
            made = host
            return host
        }
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 480, height: 800),
            styleMask: [.titled],
            backing: .buffered,
            defer: false
        )
        window.contentView = NSHostingView(rootView: root.frame(width: 480, height: 800))
        window.contentView?.layoutSubtreeIfNeeded()

        let view = try #require(slintView(in: window.contentView))
        let host = try #require(made)
        #expect(view.host === host)
        #expect(view.lastError == nil)
        #expect(host.pixelWidth == Int((480 * window.backingScaleFactor).rounded()))
        let frame = try #require(view.layer?.contents.map { $0 as! CGImage })
        #expect(!isOneColour(frame))

        var performed: [String] = []
        try host.onCallback("perform") { performed.append($0) }

        // Disabled while the email is empty.
        click(window, view, submit)
        #expect(performed.isEmpty)

        click(window, view, email)
        #expect(window.firstResponder === view)
        type(window, view, "ada@example.com")
        #expect(try host.property("email") == #""ada@example.com""#)

        click(window, view, submit)
        click(window, view, signup)
        #expect(performed == [#"["auth.submit","form"]"#, #"["nav.signup","signup"]"#])
    }

    @Test func factoryFallsBackToTheCpuWhenTheLayerIsNotUsed() throws {
        let definition = try #require(sb_compile(nil, fixturePath), "\(SlintError.last())")
        defer { sb_definition_free(definition) }
        let frame = NSRect(x: 0, y: 0, width: 32, height: 32)

        // A CPU host made while a layer was offered: the layer goes.
        var offered: [Bool] = []
        let cpu = SlintNSView(frame: frame) { layer in
            offered.append(layer != nil)
            return try SlintHost(adopting: sb_component_new(definition, 1, 1, 1))
        }
        #expect(offered == [true])
        #expect(cpu.host?.rendersOnGpu == false)
        #expect(cpu.layer?.sublayers?.isEmpty ?? true)

        // A factory that throws for the layer is asked again without one.
        offered = []
        let retried = SlintNSView(frame: frame) { layer in
            offered.append(layer != nil)
            if layer != nil { throw SlintError(description: "no GPU here") }
            return try SlintHost(adopting: sb_component_new(definition, 1, 1, 1))
        }
        #expect(offered == [true, false])
        #expect(retried.host != nil && retried.lastError == nil)
    }

    @Test func adoptingAFailedConstructorThrowsItsError() {
        #expect(throws: SlintError.self) { try SlintHost(adopting: sb_component_new(nil, 1, 1, 1)) }
    }

    private func slintView(in root: NSView?) -> SlintNSView? {
        guard let root else { return nil }
        if let view = root as? SlintNSView { return view }
        return root.subviews.lazy.compactMap { slintView(in: $0) }.first
    }

    private func click(_ window: NSWindow, _ view: NSView, _ point: CGPoint) {
        let location = view.convert(point, to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = NSEvent.mouseEvent(
                with: type, location: location, modifierFlags: [], timestamp: 0,
                windowNumber: window.windowNumber, context: nil,
                eventNumber: 0, clickCount: 1, pressure: type == .leftMouseDown ? 1 : 0
            )
            guard let event else { continue }
            if type == .leftMouseDown { view.mouseDown(with: event) } else { view.mouseUp(with: event) }
        }
    }

    private func type(_ window: NSWindow, _ view: NSView, _ text: String) {
        for character in text {
            for type in [NSEvent.EventType.keyDown, .keyUp] {
                let event = NSEvent.keyEvent(
                    with: type, location: .zero, modifierFlags: [], timestamp: 0,
                    windowNumber: window.windowNumber, context: nil,
                    characters: String(character), charactersIgnoringModifiers: String(character),
                    isARepeat: false, keyCode: 0
                )
                guard let event else { continue }
                if type == .keyDown { view.keyDown(with: event) } else { view.keyUp(with: event) }
            }
        }
    }

    private func isOneColour(_ image: CGImage) -> Bool {
        let width = image.width, height = image.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let drawn = pixels.withUnsafeMutableBytes { raw -> Bool in
            guard let context = CGContext(
                data: raw.baseAddress, width: width, height: height, bitsPerComponent: 8,
                bytesPerRow: width * 4, space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
            ) else { return false }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { return true }
        let first = pixels[0..<4]
        return stride(from: 0, to: pixels.count, by: 4).allSatisfy { pixels[$0..<$0 + 4] == first }
    }
}
