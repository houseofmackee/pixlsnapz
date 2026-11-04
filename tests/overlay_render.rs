//! Renders kept measurements the way the overlay draws them, over a made-up
//! screenshot, and checks pixels where the lines and labels must be: an
//! area's outline and size label, a guide, a line, and a circle, and that
//! lines get thicker in high contrast. Also what measuring shows live: the
//! protractor arc between two lines, "=" on equal gaps in the spacing view
//! (⌥), and smart guides while drawing a box. Each live check compares with
//! a case where the mark must not appear. And floating rulers: the major
//! tick at 50 points, level, running down, and turned 30°.
//!
//! Runs without the test harness: AppKit needs the main thread.

use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use pixlsnapz::items::Item;
use pixlsnapz::measure::{Frame, PxRect};
use pixlsnapz::overlay::TestPointer;
use pixlsnapz::settings::Settings;

const WHITE: [u8; 3] = [255, 255, 255];
const BLUE: [u8; 3] = [0, 90, 200];

/// How much two colors differ: the largest channel difference.
fn diff(a: [u8; 3], b: [u8; 3]) -> u32 {
    a.iter().zip(b).map(|(x, y)| u32::from(x.abs_diff(y))).max().unwrap_or(0)
}

/// A render with measurements, and the same screen rendered without them.
/// Comparing the two finds what was drawn whatever the display's pixel
/// density (on a 1x display a 1 pt line is half covered across two pixels).
struct Shot {
    drawn: Frame,
    blank: Frame,
    /// Pixels per point.
    scale: usize,
}

impl Shot {
    fn at(&self, x: usize, y: usize) -> [u8; 3] {
        self.drawn.pixel(x * self.scale, y * self.scale)
    }

    fn ink_at(&self, px: usize, py: usize) -> u32 {
        diff(self.drawn.pixel(px, py), self.blank.pixel(px, py))
    }

    /// Whether something was drawn within `r` points of (x, y).
    fn drawn_near(&self, x: usize, y: usize, r: usize) -> bool {
        let s = self.scale;
        (y.saturating_sub(r) * s..=(y + r) * s)
            .any(|py| (x.saturating_sub(r) * s..=(x + r) * s).any(|px| self.ink_at(px, py) > 40))
    }

    /// Pixels drawn on in the row through `y`, from `x0` to `x1` (points).
    fn drawn_in_row(&self, y: usize, x0: usize, x1: usize) -> usize {
        let py = y * self.scale;
        (x0 * self.scale..x1 * self.scale).filter(|&px| self.ink_at(px, py) > 40).count()
    }

    /// Pixels in that row changed strongly (solid labels and lines, not the
    /// light tint of boxes).
    fn strong_in_row(&self, y: usize, x0: usize, x1: usize) -> usize {
        let py = y * self.scale;
        (x0 * self.scale..x1 * self.scale).filter(|&px| self.ink_at(px, py) > 150).count()
    }

    /// How much was drawn in that row: the sum of the changes.
    fn ink_in_row(&self, y: usize, x0: usize, x1: usize) -> u32 {
        let py = y * self.scale;
        (x0 * self.scale..x1 * self.scale).map(|px| self.ink_at(px, py)).sum()
    }
}

fn screen() -> Frame {
    let mut frame = Frame::solid(400, 300, WHITE);
    frame.fill(PxRect { x0: 100, y0: 80, x1: 200, y1: 160 }, BLUE);
    frame
}

fn render(items: &[Item], high_contrast: bool, mtm: MainThreadMarker) -> Shot {
    render_live(items, high_contrast, TestPointer::None, mtm)
}

/// Like `render`, with the pointer hovering or dragging (no hints; the
/// screen frozen, so the screenshot is drawn behind, as in `render`).
fn render_live(items: &[Item], high_contrast: bool, pointer: TestPointer, mtm: MainThreadMarker) -> Shot {
    let settings = Settings { show_hints: false, freeze: true, ..Settings::default() };
    let draw = |items: &[Item], pointer| {
        pixlsnapz::overlay::render_for_tests(screen(), items.to_vec(), settings.clone(), high_contrast, pointer, mtm)
            .expect("the overlay renders")
    };
    // The blank render has neither the items nor the pointer.
    let (drawn, blank) = (draw(items, pointer), draw(&[], TestPointer::None));
    let scale = drawn.width / 400;
    assert!(scale >= 1 && drawn.height == 300 * scale, "unexpected size {}x{}", drawn.width, drawn.height);
    Shot { drawn, blank, scale }
}

fn check(name: &str, ok: bool) {
    if ok {
        println!("ok: {name}");
    } else {
        eprintln!("FAILED: {name}");
        std::process::exit(1);
    }
}

fn main() {
    let home = std::env::temp_dir().join(format!("pixlsnapz-render-test-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    // SAFETY: nothing else runs yet.
    unsafe { std::env::set_var("PIXLSNAPZ_HOME", &home) };
    let mtm = MainThreadMarker::new().expect("runs on the main thread");
    let _app = NSApplication::sharedApplication(mtm);

    let items = [
        Item::Area(PxRect { x0: 100, y0: 80, x1: 200, y1: 160 }),
        Item::VerticalGuide { x: 300 },
        Item::Line { x0: 20.0, y0: 250.0, x1: 180.0, y1: 250.0 },
        Item::Circle { cx: 320.0, cy: 220.0, r: 30.0 },
    ];
    let shot = render(&items, false, mtm);
    check("the area's outline is drawn on its edge", shot.drawn_near(100, 120, 1) && shot.drawn_near(150, 80, 1));
    check("the area is tinted, not hidden", {
        let p = shot.at(150, 120);
        p != BLUE && p[2] > p[1]
    });
    check("the area's size label is below it", shot.drawn_in_row(176, 110, 190) > 10 * shot.scale);
    check("the guide runs the full height", shot.drawn_near(300, 20, 1) && shot.drawn_near(300, 290, 1));
    check("the line is drawn", shot.drawn_near(60, 250, 1));
    check("the circle's outline is drawn", shot.drawn_near(320, 190, 1) && shot.drawn_near(350, 220, 1));
    check("the guide is in the guide color", {
        let s = shot.scale;
        (295 * s..305 * s).map(|px| shot.drawn.pixel(px, 20 * s)).any(|p| p[0] > 200 && p[1] < 200 && p[1] < p[2])
    });
    check("the screen shows where nothing is drawn", shot.at(380, 10) == WHITE && shot.at(60, 30) == WHITE);

    let thin = shot.ink_in_row(20, 290, 310);
    let thick = render(&items, true, mtm).ink_in_row(20, 290, 310);
    check(&format!("lines are thicker in high contrast (ink {thin} -> {thick})"), thick > thin * 3 / 2);

    live_checks(mtm);
    ruler_checks(mtm);
    let _ = std::fs::remove_dir_all(&home);
}

fn area(x0: usize, y0: usize, x1: usize, y1: usize) -> Item {
    Item::Area(PxRect { x0, y0, x1, y1 })
}

fn live_checks(mtm: MainThreadMarker) {
    // Protractor: two lines meeting at (330, 270) at a right angle; the arc,
    // 34 points from the corner, passes (324, 236) at 100° (clear of the
    // lines' own labels).
    let joined = [
        Item::Line { x0: 230.0, y0: 270.0, x1: 330.0, y1: 270.0 },
        Item::Line { x0: 330.0, y0: 270.0, x1: 330.0, y1: 190.0 },
    ];
    let apart = [joined[0], Item::Line { x0: 350.0, y0: 270.0, x1: 350.0, y1: 190.0 }];
    let arc = render(&joined, false, mtm).drawn_near(324, 236, 1);
    let no_arc = render(&apart, false, mtm).drawn_near(324, 236, 1);
    check("the protractor arc shows between lines that meet", arc && !no_arc);

    // Spacing (⌥ over the middle box): its gap labels read "= 40" when both
    // gaps are 40, and just "40 / 60" otherwise; "=" makes the pill wider.
    let hover = TestPointer::Hover { x: 150.0, y: 220.0, option: true };
    let equal = [area(20, 200, 80, 240), area(120, 200, 180, 240), area(220, 200, 280, 240)];
    let unequal = [equal[0], equal[1], area(240, 200, 300, 240)];
    let label_ink = |items: &[Item]| render_live(items, false, hover, mtm).strong_in_row(206, 40, 160);
    let (with_eq, without) = (label_ink(&equal), label_ink(&unequal));
    check(&format!("equal gaps are marked \"=\" in the spacing view ({without} -> {with_eq})"), with_eq > without);

    // Smart guides: a box drawn with its top at y = 30, like the kept box's,
    // gets a dotted line along y = 30 between them; one at y = 40 does not.
    let kept = [area(20, 30, 120, 70)];
    let guide = |top: f64| {
        render_live(&kept, false, TestPointer::DragArea { from: (250.0, top), to: (350.0, top + 40.0) }, mtm)
    };
    check(
        "a dotted smart guide joins boxes whose edges line up",
        guide(30.0).drawn_near(185, 30, 0) && !guide(40.0).drawn_near(185, 30, 0),
    );
    // Same width (100): an "=" inside the top of both boxes.
    let same = guide(40.0).drawn_near(70, 40, 2);
    let wider = render_live(&kept, false, TestPointer::DragArea { from: (230.0, 40.0), to: (350.0, 80.0) }, mtm)
        .drawn_near(70, 40, 2);
    check("boxes of the same width are marked \"=\"", same && !wider);
}

fn ruler_checks(mtm: MainThreadMarker) {
    // Whether a dark tick is drawn within a point of `u` along and `v` across
    // a ruler at `angle` (the start and the image in view points).
    let tick_at = |angle: f64, u: f64, v: f64| {
        let (frame, size, (sx, sy)) =
            pixlsnapz::rulers::render_for_tests(angle, 300.0, mtm).expect("the ruler renders");
        let scale = frame.width as f64 / size.width;
        let a = angle.to_radians();
        // Along and across in view points (y grows down).
        let x = sx + u * a.cos() + v * a.sin();
        let y = sy - u * a.sin() + v * a.cos();
        (-1..=1).any(|dy| {
            (-1..=1).any(|dx| {
                let px = ((x + f64::from(dx)) * scale) as usize;
                let py = ((y + f64::from(dy)) * scale) as usize;
                px < frame.width && py < frame.height && frame.pixel(px, py).iter().all(|c| *c < 120)
            })
        })
    };
    for angle in [0.0, -90.0, 30.0] {
        // The major tick at 50 runs 14 points into the body; none at 52.5.
        check(
            &format!("a ruler at {angle}° has its major tick at 50 points"),
            tick_at(angle, 50.5, 7.0) && !tick_at(angle, 52.5, 7.0),
        );
    }
}
