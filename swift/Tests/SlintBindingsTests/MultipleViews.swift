// Several views on one thread: two in one window and one in a second window,
// each with its own popups (`popups.slint`; top combo box center 80×32).

import AppKit
@testable import SlintBindings
import Testing

@MainActor
func popupsDefinition() throws -> SlintDefinition {
    let path = URL(fileURLWithPath: #filePath)
        .appendingPathComponent("../../../../crates/slint-bindings-core/ui/popups.slint")
        .standardizedFileURL.path
    return try SlintDefinition(path: path)
}

@MainActor
struct MultipleViews {
    private let topCombo = NSPoint(x: 80, y: 32)
    private let down = "\u{F701}"
    private let enter = "\n"

    @Test func viewsInTwoWindowsKeepTheirOwnPopupsAndState() throws {
        let definition = try popupsDefinition()
        let size = NSSize(width: 320, height: 200)
        let left = SlintNSView(frame: NSRect(origin: .zero, size: size), definition: definition, prefersGpu: false)
        let right = SlintNSView(frame: NSRect(origin: NSPoint(x: 320, y: 0), size: size), definition: definition, prefersGpu: false)
        let other = SlintNSView(frame: NSRect(origin: .zero, size: size), definition: definition, prefersGpu: false)
        let pair = window(width: 640, height: 200)
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 640, height: 200))
        container.addSubview(left)
        container.addSubview(right)
        pair.contentView = container
        let single = window(width: 320, height: 200)
        single.contentView = other
        other.frame = NSRect(origin: .zero, size: size)
        let views = [left, right, other]
        for view in views {
            #expect(view.lastError == nil)
            #expect(view.layer?.contents != nil)
        }
        let windows = NSApplication.shared.windows.count
        let shown = views.map { $0.layer?.contents as AnyObject? }

        click(left, topCombo)
        // The popup is drawn into the view's own frame, not a new window.
        #expect(left.layer?.contents as AnyObject? !== shown[0])
        #expect(right.layer?.contents as AnyObject? === shown[1])
        #expect(other.layer?.contents as AnyObject? === shown[2])
        #expect(NSApplication.shared.windows.count == windows)

        let host = try #require(left.host)
        for key in [down, enter] {
            _ = host.keyPressed(key)
            _ = host.keyReleased(key)
        }
        #expect(try host.property("fruit") == #""Banana""#)
        #expect(try right.host?.property("fruit") == #""Apple""#)
        #expect(try other.host?.property("fruit") == #""Apple""#)
    }

    @Test func clickingAnotherViewClosesThePopup() throws {
        let definition = try popupsDefinition()
        let size = NSSize(width: 320, height: 200)
        let left = SlintNSView(frame: NSRect(origin: .zero, size: size), definition: definition, prefersGpu: false)
        let right = SlintNSView(frame: NSRect(origin: NSPoint(x: 320, y: 0), size: size), definition: definition, prefersGpu: false)
        let pair = window(width: 640, height: 200)
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 640, height: 200))
        container.addSubview(left)
        container.addSubview(right)
        pair.contentView = container
        click(left, topCombo)
        let host = try #require(left.host)
        #expect(try host.renderIfNeeded().image == nil)

        click(right, topCombo)
        #expect(pair.firstResponder === right)
        // Where the open list showed "Cherry"; with the popup closed it is background.
        click(left, NSPoint(x: 80, y: 120))
        #expect(try host.property("fruit") == #""Apple""#)
    }

    @Test func aClickInsideTheOpenListPicksAnItem() throws {
        let view = SlintNSView(frame: NSRect(x: 0, y: 0, width: 320, height: 200), definition: try popupsDefinition(), prefersGpu: false)
        let single = window(width: 320, height: 200)
        single.contentView = view
        view.frame = NSRect(x: 0, y: 0, width: 320, height: 200)
        click(view, topCombo)
        click(view, NSPoint(x: 80, y: 120))
        #expect(try view.host?.property("fruit") != #""Apple""#)
    }

    private func window(width: CGFloat, height: CGFloat) -> NSWindow {
        NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: width, height: height),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
    }

    /// A left click through the view's own event handlers, then a frame.
    private func click(_ view: SlintNSView, _ point: NSPoint) {
        let location = view.convert(point, to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            guard let event = NSEvent.mouseEvent(
                with: type,
                location: location,
                modifierFlags: [],
                timestamp: 0,
                windowNumber: view.window?.windowNumber ?? 0,
                context: nil,
                eventNumber: 0,
                clickCount: 1,
                pressure: 1
            ) else { continue }
            if type == .leftMouseDown {
                view.mouseDown(with: event)
            } else {
                view.mouseUp(with: event)
            }
        }
        view.present()
    }
}
