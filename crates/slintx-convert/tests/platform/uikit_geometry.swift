// An iOS Simulator app that loads a compiled UIKit storyboard, lays its
// initial controller's view out at a given size, and writes every view whose
// accessibility identifier `--source-tags` set as `key x y width height`
// (in the root view's points), then each stack view's
// `layoutMarginsRelativeArrangement` and margins as
// `stack <key or index> YES|NO top leading bottom trailing`, into a text
// file. It writes the view as a 1x PNG and exits.
//
// Arguments come through the environment, since `simctl launch` passes
// `SIMCTL_CHILD_*` variables on: SLINTX_STORYBOARD (the .storyboardc path),
// SLINTX_OUT, SLINTX_PNG, SLINTX_WIDTH, SLINTX_HEIGHT and SLINTX_CLASS.

import UIKit

/// Stands in for the controller class the storyboard names: outlets to
/// properties it does not have are dropped instead of raising.
class TolerantController: UIViewController {
    override func setValue(_ value: Any?, forUndefinedKey key: String) {}
}

func fail(_ message: String) -> Never {
    FileHandle.standardError.write("\(message)\n".data(using: .utf8)!)
    exit(1)
}

func measure() {
    let env = ProcessInfo.processInfo.environment
    guard let compiled = env["SLINTX_STORYBOARD"], let text = env["SLINTX_OUT"], let png = env["SLINTX_PNG"],
        let width = env["SLINTX_WIDTH"].flatMap(Double.init),
        let height = env["SLINTX_HEIGHT"].flatMap(Double.init), let name = env["SLINTX_CLASS"]
    else { fail("missing SLINTX_* environment") }
    if NSClassFromString(name) == nil, let cls = objc_allocateClassPair(TolerantController.self, name, 0) {
        objc_registerClassPair(cls)
    }
    let url = URL(fileURLWithPath: compiled)
    guard let bundle = Bundle(path: url.deletingLastPathComponent().path) else { fail("no bundle") }
    let storyboard = UIStoryboard(name: url.deletingPathExtension().lastPathComponent, bundle: bundle)
    guard let controller = storyboard.instantiateInitialViewController() else {
        fail("no initial view controller")
    }
    // Outside a window: no safe area, no status bar, exactly the reference size.
    let root = controller.view!
    root.translatesAutoresizingMaskIntoConstraints = true
    root.frame = CGRect(x: 0, y: 0, width: width, height: height)
    root.setNeedsLayout()
    root.layoutIfNeeded()

    var out: [String] = []
    var stacks: [String] = []
    func walk(_ view: UIView) {
        if let id = view.accessibilityIdentifier, id.range(of: #"^\d+:\d+$"#, options: .regularExpression) != nil {
            let r = view.superview!.convert(view.frame, to: root)
            if r.width > 0 && r.height > 0 {
                out.append("\(id) \(r.minX) \(r.minY) \(r.width) \(r.height)")
            }
        }
        if let stack = view as? UIStackView {
            let key = stack.accessibilityIdentifier ?? "#\(stacks.count)"
            let m = stack.directionalLayoutMargins
            stacks.append(
                "stack \(key) \(stack.isLayoutMarginsRelativeArrangement ? "YES" : "NO") "
                    + "\(m.top) \(m.leading) \(m.bottom) \(m.trailing)")
        }
        for child in view.subviews {
            walk(child)
        }
    }
    walk(root)
    do {
        try (out + stacks).joined(separator: "\n").write(toFile: text, atomically: true, encoding: .utf8)
    } catch {
        fail("\(text): \(error)")
    }

    let format = UIGraphicsImageRendererFormat()
    format.scale = 1
    let image = UIGraphicsImageRenderer(bounds: root.bounds, format: format).image { context in
        root.layer.render(in: context.cgContext)
    }
    do {
        try image.pngData()!.write(to: URL(fileURLWithPath: png))
    } catch {
        fail("\(png): \(error)")
    }
}

class AppDelegate: UIResponder, UIApplicationDelegate {}

/// iOS 27 requires the scene life cycle; Info.plist names this class.
@objc(SlintxSceneDelegate)
class SceneDelegate: UIResponder, UIWindowSceneDelegate {
    func scene(
        _ scene: UIScene, willConnectTo session: UISceneSession, options connectionOptions: UIScene.ConnectionOptions
    ) {
        measure()
        exit(0)
    }
}

UIApplicationMain(CommandLine.argc, CommandLine.unsafeArgv, nil, NSStringFromClass(AppDelegate.self))
