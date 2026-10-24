// Loads a compiled AppKit storyboard, lays its initial controller's view out
// at a given size, prints every view whose identifier `--source-tags` set as
// `key x y width height` (top-left origin, in the root view's points), and
// writes the view as a 1x PNG.
//
// Usage: appkit_geometry <dir/Name.storyboardc> <out.png> <width> <height> <ControllerClass>

import AppKit

/// Stands in for the controller class the storyboard names: outlets to
/// properties it does not have are dropped instead of raising.
class TolerantController: NSViewController {
    override func setValue(_ value: Any?, forUndefinedKey key: String) {}
}

let args = CommandLine.arguments
guard args.count == 6, let width = Double(args[3]), let height = Double(args[4]) else {
    FileHandle.standardError.write("usage: appkit_geometry <storyboardc> <png> <w> <h> <class>\n".data(using: .utf8)!)
    exit(2)
}
let compiled = URL(fileURLWithPath: args[1])
if NSClassFromString(args[5]) == nil, let cls = objc_allocateClassPair(TolerantController.self, args[5], 0) {
    objc_registerClassPair(cls)
}

_ = NSApplication.shared
let bundle = Bundle(path: compiled.deletingLastPathComponent().path)!
let storyboard = NSStoryboard(name: compiled.deletingPathExtension().lastPathComponent, bundle: bundle)
guard let controller = storyboard.instantiateInitialController() as? NSViewController else {
    FileHandle.standardError.write("no initial view controller\n".data(using: .utf8)!)
    exit(1)
}
let root = controller.view
let window = NSWindow(
    contentRect: NSRect(x: 0, y: 0, width: width, height: height),
    styleMask: [.borderless], backing: .buffered, defer: false)
window.contentView = root
window.setContentSize(NSSize(width: width, height: height))
root.layoutSubtreeIfNeeded()

// Hidden views are measured too: Slint lays out invisible elements.
func walk(_ view: NSView, _ out: inout [String]) {
    // ibtool names the other views `_NS:<n>`.
    if let id = view.identifier?.rawValue, id.range(of: #"^\d+:\d+$"#, options: .regularExpression) != nil {
        // The alignment rectangle, not the frame: Auto Layout and stack views
        // place controls by it, and the frame adds the bezel's shadow.
        let r = view.superview!.convert(view.alignmentRect(forFrame: view.frame), to: root)
        let top = root.isFlipped ? r.minY : root.bounds.height - r.maxY
        if r.width > 0 && r.height > 0 {
            out.append("\(id) \(r.minX) \(top) \(r.width) \(r.height)")
        }
    }
    for child in view.subviews {
        walk(child, &out)
    }
}

var out: [String] = []
walk(root, &out)
print(out.joined(separator: "\n"))

let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(width), pixelsHigh: Int(height),
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
rep.size = NSSize(width: width, height: height)
root.cacheDisplay(in: root.bounds, to: rep)
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: args[2]))
