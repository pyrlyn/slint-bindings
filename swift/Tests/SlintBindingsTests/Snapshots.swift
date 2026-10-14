// The PNGs next to the Rust e2e are the screenshots. This checks the image
// the macOS host actually presents, not only the buffer the C ABI returns.

import AppKit
import CoreGraphics
import ImageIO
@testable import SlintBindings
import Testing

@MainActor
struct Snapshots {
    @Test func signInViewScreenshotMatchesTheSnapshot() throws {
        // The golden is the CPU frame. The default view presents on Metal, which
        // does not put a CGImage in `layer.contents`.
        let view = SlintNSView(
            frame: NSRect(x: 0, y: 0, width: 320, height: 200),
            definition: try demoDefinition(),
            prefersGpu: false
        )
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 320, height: 200),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.contentView = view
        view.frame = NSRect(x: 0, y: 0, width: 320, height: 200)
        #expect(view.lastError == nil)
        let image = try #require(cgImage(view.layer?.contents))
        let name = snapshotName(width: image.width, height: image.height)
        try expectPixels(image, match: name)
    }

    @Test func namedFieldScreenshotMatchesTheSnapshot() throws {
        let host = try SlintHost(definition: try demoDefinition(), pixelWidth: 320, pixelHeight: 200, scale: 1)
        _ = try host.renderIfNeeded()
        try host.setProperty("name", json: #""Ada""#)
        let image = try #require(try host.renderIfNeeded().image)
        try expectPixels(image, match: "sign-in-ada")
    }

    /// `CALayer.contents` is an untyped CF object. A conditional cast to
    /// `CGImage` is rejected because every CF type would succeed it.
    private func cgImage(_ contents: Any?) -> CGImage? {
        guard let contents else { return nil }
        let object = contents as CFTypeRef
        guard CFGetTypeID(object) == CGImage.typeID else { return nil }
        return (object as! CGImage)
    }

    /// 1× and 2× are the goldens the Rust test writes. Anything else is a
    /// backing scale this suite does not have a picture for.
    private func snapshotName(width: Int, height: Int) -> String {
        if width == 320 && height == 200 { return "sign-in" }
        if width == 640 && height == 400 { return "sign-in@2x" }
        return "sign-in-\(width)x\(height)"
    }

    private func expectPixels(_ image: CGImage, match name: String) throws {
        let golden = try #require(loadPNG(named: name))
        let got = try #require(pixels(of: image))
        let expected = try #require(pixels(of: golden))
        #expect(image.width == golden.width)
        #expect(image.height == golden.height)
        #expect(got.count == expected.count)
        var diffs = 0
        var first: Int?
        let count = min(got.count, expected.count) / 4
        for index in 0..<count {
            let offset = index * 4
            if got[offset..<(offset + 4)] != expected[offset..<(offset + 4)] {
                diffs += 1
                if first == nil { first = index }
            }
        }
        #expect(diffs == 0, "\(name): \(diffs) pixels differ, first at \(first ?? -1)")
    }

    private func loadPNG(named name: String) -> CGImage? {
        let url = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("crates/slint-bindings-ffi/tests/snapshots/\(name).png")
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }

    /// Both images are drawn the same way, so a bottom-left context cannot
    /// make one of them look flipped relative to the other.
    private func pixels(of image: CGImage) -> [UInt8]? {
        let width = image.width
        let height = image.height
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
              let context = CGContext(
                data: &pixels,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: width * 4,
                space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              ) else { return nil }
        context.interpolationQuality = .none
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        return pixels
    }
}
