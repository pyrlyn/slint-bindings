// Demo app: a SwiftUI pane and an embedded Slint pane sharing one model.

import AppKit
import SlintBindings
import SwiftUI

@MainActor
@Observable
final class DemoModel {
    var name = "Ivan"
    var submissions: [String] = []
}

/// The form the demo shows: the first `.slint` argument, else the repository's
/// `demo.slint`. `swift run` builds from the checkout, as `Package.swift`
/// links the Rust library from it.
@MainActor
private func loadDefinition() -> Result<SlintDefinition, any Error> {
    let path = CommandLine.arguments.dropFirst().first { $0.hasSuffix(".slint") } ?? URL(fileURLWithPath: #filePath)
        .appendingPathComponent("../../../../crates/slint-bindings-core/ui/demo.slint")
        .standardizedFileURL.path
    return Result { try SlintDefinition(path: path) }
}

@main
struct SlintDemoApp: App {
    @State private var model = DemoModel()
    private let definition = loadDefinition()

    init() {
        // `swift run` starts a bare process; make it a regular app with a Dock icon and focus.
        NSApplication.shared.setActivationPolicy(.regular)
        NSApplication.shared.activate()
    }

    var body: some Scene {
        WindowGroup("Slint Bindings demo") {
            switch definition {
            case .success(let definition):
                ContentView(model: model, definition: definition)
            case .failure(let error):
                Text(String(describing: error)).padding()
            }
        }
    }
}

struct ContentView: View {
    @Bindable var model: DemoModel
    let definition: SlintDefinition

    var body: some View {
        HStack(spacing: 0) {
            VStack(alignment: .leading, spacing: 12) {
                Text("SwiftUI").font(.headline)
                TextField("Name pushed into Slint", text: $model.name)
                Text("Submitted from Slint").font(.subheadline)
                List(Array(model.submissions.enumerated()), id: \.offset) { _, name in
                    Text(name)
                }
            }
            .padding()
            .frame(width: 240)

            Divider()

            SlintDemoView(definition: definition, name: model.name) { submitted in
                model.submissions.append(submitted)
            }
            .frame(minWidth: 320, minHeight: 220)
        }
    }
}
