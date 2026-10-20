// What VoiceOver sees in the demo form (T14). The test asks the view the same
// NSAccessibility questions VoiceOver does, so it needs no screen reader.

import AppKit
@testable import SlintBindings
import Testing

@MainActor
struct Accessibility {
    /// The demo form on the CPU path, in a window: AccessKit converts frames
    /// through the view's window, as VoiceOver's own queries do.
    private func shownForm() throws -> (NSWindow, SlintNSView) {
        let rect = NSRect(x: 0, y: 0, width: 320, height: 200)
        let view = SlintNSView(frame: rect, definition: try demoDefinition(), prefersGpu: false)
        let window = NSWindow(contentRect: rect, styleMask: [.borderless], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = view
        return (window, view)
    }

    /// Every element below `children`, depth first, as VoiceOver walks them.
    private func walk(_ children: [Any]?) -> [AnyObject] {
        (children ?? []).flatMap { child -> [AnyObject] in
            let element = child as AnyObject
            return [element] + walk(element.accessibilityChildren?())
        }
    }

    private func role(_ element: AnyObject) -> NSAccessibility.Role? {
        element.accessibilityRole?()
    }

    /// For getters Swift sees more than one declaration of.
    private func ask(_ element: AnyObject?, _ getter: String) -> Any? {
        (element as? NSObject)?.perform(NSSelectorFromString(getter))?.takeUnretainedValue()
    }

    @Test func voiceOverFindsTheHeadingFieldAndButton() throws {
        let (window, view) = try shownForm()
        defer { window.close() }
        #expect(view.lastError == nil)

        let elements = walk(view.accessibilityChildren())
        let heading = elements.first { role($0) == .staticText }
        #expect(ask(heading, "accessibilityValue") as? String == "Sign in")

        let field = elements.first { role($0) == .textField }
        #expect(field?.accessibilityTitle?() == "Name")
        #expect(field?.accessibilityPlaceholderValue?() == "Name")

        let button = elements.first { role($0) == .button }
        #expect(button?.accessibilityTitle?() == "Continue")

        // The button sits where demo.slint puts it: 16, 96 from the view's top-left.
        let frame = button?.accessibilityFrame?() ?? .zero
        let inView = view.convert(window.convertFromScreen(frame), from: nil)
        #expect(abs(inView.minX - 16) < 1 && abs(inView.minY - 96) < 1)
        #expect(abs(inView.width - 128) < 1 && abs(inView.height - 32) < 1)
    }

    @Test func pressingTheButtonSubmitsAndTypingReachesTheField() throws {
        let (window, view) = try shownForm()
        defer { window.close() }
        let elements = walk(view.accessibilityChildren())

        let field = elements.first { role($0) == .textField }
        field?.setAccessibilityValue?("Ada")
        let button = elements.first { role($0) == .button }
        #expect(button?.accessibilityPerformPress?() == true)

        // Actions run on the next tick, outside AppKit's accessibility call.
        SlintHost.tick()
        #expect(try view.host?.property("name") == #""Ada""#)
        #expect(try view.host?.property("submit-count") == "1")
    }

    @Test func focusFollowsSlintAfterAFrame() throws {
        let (window, view) = try shownForm()
        defer { window.close() }
        let elements = walk(view.accessibilityChildren())
        let field = elements.first { role($0) == .textField }

        // AccessKit reports focus only while the host view has it.
        window.makeFirstResponder(view)
        field?.setAccessibilityFocused?(true)
        SlintHost.tick()
        _ = try view.host?.renderIfNeeded()

        let focused = view.accessibilityFocusedUIElement as AnyObject?
        #expect(focused.map(role) == .textField)
    }
}
