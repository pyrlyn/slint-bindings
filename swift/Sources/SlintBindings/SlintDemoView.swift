// SwiftUI wrapper: SwiftUI state in, Slint callbacks out.

import Foundation
import SwiftUI

/// The demo form (`demo.slint`, compiled at run time) as a SwiftUI view.
/// `name` flows SwiftUI → Slint; submissions flow back through `onSubmit`.
/// Two-way binding of edits needs a Slint `changed` callback per property,
/// which the typed wrappers (T13) generate.
public struct SlintDemoView: NSViewRepresentable {
    public var definition: SlintDefinition
    public var name: String
    public var onSubmit: @MainActor (String) -> Void

    public init(definition: SlintDefinition, name: String, onSubmit: @escaping @MainActor (String) -> Void) {
        self.definition = definition
        self.name = name
        self.onSubmit = onSubmit
    }

    public final class Coordinator {
        // Push only real changes, so a SwiftUI re-render does not overwrite
        // what the user just typed inside Slint.
        var lastPushedName: String?
        var onSubmit: (@MainActor (String) -> Void)?
    }

    public func makeCoordinator() -> Coordinator { Coordinator() }

    public func makeNSView(context: Context) -> SlintNSView {
        let view = SlintNSView(frame: .zero, definition: definition)
        try? view.host?.onCallback("submitted") { argsJson in
            guard let name = Self.decode([String].self, argsJson)?.first else { return }
            context.coordinator.onSubmit?(name)
        }
        updateNSView(view, context: context)
        return view
    }

    public func updateNSView(_ view: SlintNSView, context: Context) {
        context.coordinator.onSubmit = onSubmit
        if context.coordinator.lastPushedName != name,
           let json = try? String(decoding: JSONEncoder().encode(name), as: UTF8.self) {
            try? view.host?.setProperty("name", json: json)
            context.coordinator.lastPushedName = name
        }
    }

    private static func decode<T: Decodable>(_ type: T.Type, _ json: String) -> T? {
        try? JSONDecoder().decode(type, from: Data(json.utf8))
    }
}
