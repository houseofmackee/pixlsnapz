//! Renders kept measurements the way the overlay draws them, over a made-up
//! screenshot, and checks pixels where the lines and labels must be: an
//! area's outline and size label, a guide, a line, and a circle, and that
//! lines get thicker in high contrast.
//!
//! Runs without the test harness: AppKit needs the main thread.

use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;
use pixlsnapz::items::Item;
use pixlsnapz::measure::{Frame, PxRect};
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
    let draw = |items: &[Item]| {
        pixlsnapz::overlay::render_for_tests(screen(), items.to_vec(), Settings::default(), high_contrast, mtm)
            .expect("the overlay renders")
    };
    let (drawn, blank) = (draw(items), draw(&[]));
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

    let _ = std::fs::remove_dir_all(&home);
}
