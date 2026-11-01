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

/// The default guide color, #FF2D78, allowing for antialiasing.
fn is_accent(p: [u8; 3]) -> bool {
    p[0] > 200 && p[1] < 130 && (60..200).contains(&p[2])
}

struct Shot {
    frame: Frame,
    /// Pixels per point.
    scale: usize,
}

impl Shot {
    fn at(&self, x: usize, y: usize) -> [u8; 3] {
        self.frame.pixel(x * self.scale, y * self.scale)
    }

    /// Whether an accent pixel is within `r` points of (x, y).
    fn accent_near(&self, x: usize, y: usize, r: usize) -> bool {
        let s = self.scale;
        (y.saturating_sub(r) * s..=(y + r) * s)
            .any(|py| (x.saturating_sub(r) * s..=(x + r) * s).any(|px| is_accent(self.frame.pixel(px, py))))
    }

    /// Accent pixels in the row through `y`, from `x0` to `x1` (points).
    fn accent_in_row(&self, y: usize, x0: usize, x1: usize) -> usize {
        let py = y * self.scale;
        (x0 * self.scale..x1 * self.scale).filter(|&px| is_accent(self.frame.pixel(px, py))).count()
    }
}

fn render(items: &[Item], high_contrast: bool, mtm: MainThreadMarker) -> Shot {
    let mut frame = Frame::solid(400, 300, WHITE);
    frame.fill(PxRect { x0: 100, y0: 80, x1: 200, y1: 160 }, BLUE);
    let out = pixlsnapz::overlay::render_for_tests(frame, items.to_vec(), Settings::default(), high_contrast, mtm)
        .expect("the overlay renders");
    let scale = out.width / 400;
    assert!(scale >= 1 && out.height == 300 * scale, "unexpected size {}x{}", out.width, out.height);
    Shot { frame: out, scale }
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
    check("the area's outline is drawn on its edge", shot.accent_near(100, 120, 1) && shot.accent_near(150, 80, 1));
    check("the area is tinted, not hidden", {
        let p = shot.at(150, 120);
        p != BLUE && p[2] > p[1]
    });
    check("the area's size label is below it", shot.accent_in_row(176, 110, 190) > 10);
    check("the guide runs the full height", shot.accent_near(300, 20, 1) && shot.accent_near(300, 290, 1));
    check("the line is drawn", shot.accent_near(60, 250, 1));
    check("the circle's outline is drawn", shot.accent_near(320, 190, 1) && shot.accent_near(350, 220, 1));
    check("the screen shows where nothing is drawn", shot.at(380, 10) == WHITE && shot.at(60, 30) == WHITE);

    let thin = shot.accent_in_row(20, 290, 310);
    let thick = render(&items, true, mtm).accent_in_row(20, 290, 310);
    check(&format!("lines are thicker in high contrast ({thin} -> {thick} pixels)"), thick > thin);

    let _ = std::fs::remove_dir_all(&home);
}
