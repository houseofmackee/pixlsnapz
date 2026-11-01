// Shortcuts actions for PixlSnapz (App Intents). Each one hands a
// pixlsnapz:// link to the app (written in Rust) in this same process, so
// this file holds no logic of its own. build.rs builds it into a static
// library linked into the app, and generates the metadata Shortcuts reads.
// Shortcuts only talks to the app when it is signed with a team ID (such as
// a Developer ID), not ad hoc.

import AppIntents
import Foundation

/// The app's handler for pixlsnapz:// links (src/app/mod.rs).
@_silgen_name("pixlsnapz_open_url")
private func pixlsnapzOpenURL(_ url: UnsafePointer<CChar>)

/// Runs `pixlsnapz://<command>` with `query` items in this app.
@MainActor
private func run(_ command: String, _ query: [String: String] = [:]) {
    var url = URLComponents()
    url.scheme = "pixlsnapz"
    url.host = command
    if !query.isEmpty {
        url.queryItems = query.sorted { $0.key < $1.key }.map { URLQueryItem(name: $0.key, value: $0.value) }
    }
    if let url = url.url {
        url.absoluteString.withCString { pixlsnapzOpenURL($0) }
    }
}

struct MeasureIntent: AppIntent {
    static let title: LocalizedStringResource = "Measure"
    static let description = IntentDescription("Starts measuring on screen, optionally with a saved measurement set.")

    @Parameter(title: "Measurement Set", description: "The name of a set saved with ⇧S while measuring.")
    var set: String?

    @MainActor
    func perform() async throws -> some IntentResult {
        if let set, !set.isEmpty {
            run("measure", ["set": set])
        } else {
            run("measure")
        }
        return .result()
    }
}

struct PickColorIntent: AppIntent {
    static let title: LocalizedStringResource = "Pick Color"
    static let description = IntentDescription("Shows the loupe; a click copies the color under the pointer.")

    @MainActor
    func perform() async throws -> some IntentResult {
        run("pick-color")
        return .result()
    }
}

enum RulerDirection: String, AppEnum {
    case horizontal
    case vertical

    static let typeDisplayRepresentation: TypeDisplayRepresentation = "Direction"
    static let caseDisplayRepresentations: [RulerDirection: DisplayRepresentation] = [
        .horizontal: "Horizontal",
        .vertical: "Vertical",
    ]
}

struct NewRulerIntent: AppIntent {
    static let title: LocalizedStringResource = "New Ruler"
    static let description = IntentDescription("Opens a floating ruler that stays on screen.")

    @Parameter(title: "Direction", default: .horizontal)
    var direction: RulerDirection

    @MainActor
    func perform() async throws -> some IntentResult {
        run("ruler", ["direction": direction.rawValue])
        return .result()
    }
}

struct OpenColorsIntent: AppIntent {
    static let title: LocalizedStringResource = "Show Colors"
    static let description = IntentDescription("Opens the Colors window with recent colors and palettes.")

    @MainActor
    func perform() async throws -> some IntentResult {
        run("colors")
        return .result()
    }
}

struct PixlSnapzShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: MeasureIntent(),
            phrases: ["Measure with \(.applicationName)"],
            shortTitle: "Measure",
            systemImageName: "ruler"
        )
        AppShortcut(
            intent: PickColorIntent(),
            phrases: ["Pick a color with \(.applicationName)"],
            shortTitle: "Pick Color",
            systemImageName: "eyedropper"
        )
    }
}
