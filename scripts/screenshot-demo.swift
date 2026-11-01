// A demo window for README screenshots: a made-up project dashboard with
// a sidebar, cards and a button, in fixed light colors, so screenshots never
// show anyone's real desktop or data.
//
// Run by scripts/readme_screenshots.sh. Prints each element's frame in
// screen points (top-left origin, like cliclick and screencapture -R) as
// "name x y width height" lines, then "ready".

import AppKit

let contentSize = NSSize(width: 960, height: 600)
let topLeft = NSPoint(x: 160, y: 140)  // screen points from the top-left

struct Element {
    let name: String
    let rect: NSRect  // in the content view (top-left origin)
}

func color(_ hex: UInt32) -> NSColor {
    NSColor(
        srgbRed: CGFloat((hex >> 16) & 0xFF) / 255, green: CGFloat((hex >> 8) & 0xFF) / 255,
        blue: CGFloat(hex & 0xFF) / 255, alpha: 1)
}

let sidebar = NSRect(x: 0, y: 0, width: 210, height: 600)
let cards = (0..<3).map { i in NSRect(x: 250 + CGFloat(i) * 236, y: 150, width: 212, height: 168) }
let button = NSRect(x: 780, y: 48, width: 140, height: 38)
let avatar = NSRect(x: 250, y: 360, width: 56, height: 56)
let elements: [Element] =
    [Element(name: "sidebar", rect: sidebar), Element(name: "button", rect: button),
     Element(name: "avatar", rect: avatar)]
    + cards.enumerated().map { Element(name: "card\($0.offset + 1)", rect: $0.element) }

final class DemoView: NSView {
    override var isFlipped: Bool { true }

    func text(_ s: String, _ p: NSPoint, size: CGFloat, weight: NSFont.Weight = .regular, color c: NSColor) {
        (s as NSString).draw(
            at: p, withAttributes: [.font: NSFont.systemFont(ofSize: size, weight: weight), .foregroundColor: c])
    }

    override func draw(_ dirtyRect: NSRect) {
        color(0xFFFFFF).setFill()
        bounds.fill()

        // Sidebar with navigation.
        color(0xF3F4F6).setFill()
        sidebar.fill()
        text("Acme", NSPoint(x: 24, y: 24), size: 20, weight: .bold, color: color(0x111827))
        for (i, item) in ["Overview", "Projects", "Team", "Reports", "Settings"].enumerated() {
            let row = NSRect(x: 12, y: 80 + CGFloat(i) * 40, width: 186, height: 32)
            if i == 1 {
                color(0xE0E7FF).setFill()
                NSBezierPath(roundedRect: row, xRadius: 8, yRadius: 8).fill()
            }
            text(item, NSPoint(x: 28, y: row.minY + 7), size: 14, weight: i == 1 ? .semibold : .regular,
                 color: i == 1 ? color(0x3730A3) : color(0x374151))
        }

        // Header and the primary button.
        text("Projects", NSPoint(x: 250, y: 46), size: 28, weight: .bold, color: color(0x111827))
        text("12 active · 3 due this week", NSPoint(x: 250, y: 90), size: 14, color: color(0x6B7280))
        color(0x2563EB).setFill()
        NSBezierPath(roundedRect: button, xRadius: 8, yRadius: 8).fill()
        text("New Project", NSPoint(x: button.minX + 26, y: button.minY + 10), size: 14, weight: .semibold,
             color: .white)

        // Cards, each with a colored cover and a title.
        let covers: [UInt32] = [0x6366F1, 0xF59E0B, 0x10B981]
        let titles = ["Website redesign", "Mobile app", "Brand guidelines"]
        for (i, card) in cards.enumerated() {
            let path = NSBezierPath(roundedRect: card, xRadius: 12, yRadius: 12)
            color(0xFFFFFF).setFill()
            path.fill()
            color(0xE5E7EB).setStroke()
            path.lineWidth = 1
            path.stroke()
            color(covers[i]).setFill()
            NSBezierPath(roundedRect: NSRect(x: card.minX + 16, y: card.minY + 16, width: card.width - 32, height: 84),
                         xRadius: 8, yRadius: 8).fill()
            text(titles[i], NSPoint(x: card.minX + 16, y: card.minY + 114), size: 15, weight: .semibold,
                 color: color(0x111827))
            text("Updated 2 days ago", NSPoint(x: card.minX + 16, y: card.minY + 136), size: 12,
                 color: color(0x6B7280))
        }

        // A team member.
        color(0xFCA5A5).setFill()
        NSBezierPath(ovalIn: avatar).fill()
        text("AK", NSPoint(x: avatar.minX + 16, y: avatar.minY + 17), size: 17, weight: .semibold,
             color: color(0x7F1D1D))
        text("Alex Kim", NSPoint(x: 322, y: 368), size: 15, weight: .semibold, color: color(0x111827))
        text("Design lead", NSPoint(x: 322, y: 390), size: 13, color: color(0x6B7280))
    }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
app.appearance = NSAppearance(named: .aqua)
let window = NSWindow(
    contentRect: NSRect(origin: .zero, size: contentSize), styleMask: [.titled, .closable],
    backing: .buffered, defer: false)
window.title = "Acme Projects"
window.contentView = DemoView(frame: NSRect(origin: .zero, size: contentSize))
let screenHeight = NSScreen.screens[0].frame.height
window.setFrameTopLeftPoint(NSPoint(x: topLeft.x, y: screenHeight - topLeft.y))
window.makeKeyAndOrderFront(nil)
app.activate(ignoringOtherApps: true)

// Frames in screen points, top-left origin.
let content = window.convertToScreen(window.contentLayoutRect)
let contentTop = screenHeight - content.maxY
print("content \(Int(content.minX)) \(Int(contentTop)) \(Int(content.width)) \(Int(content.height))")
for e in elements {
    print("\(e.name) \(Int(content.minX + e.rect.minX)) \(Int(contentTop + e.rect.minY)) \(Int(e.rect.width)) \(Int(e.rect.height))")
}
print("ready")
fflush(stdout)
app.run()
