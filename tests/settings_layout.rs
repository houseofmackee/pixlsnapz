//! Builds the real Settings window (without showing it) and checks every
//! tab: no control overlaps another or sticks out of its tab, VoiceOver can
//! name every control, and the content matches tests/snapshots/settings_layout.txt.
//!
//! After an intended change, review the diff and run
//! `UPDATE_SNAPSHOTS=1 cargo test --test settings_layout`.
//!
//! Runs without the test harness: AppKit needs the main thread.

use std::path::Path;

use objc2::MainThreadMarker;
use objc2_app_kit::NSApplication;

fn main() {
    // Settings of its own, with a screenshot folder that is the same on
    // every Mac.
    let home = std::env::temp_dir().join(format!("pixlsnapz-settings-test-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::fs::write(home.join("settings.toml"), "screenshot_folder = \"/tmp/pixlsnapz-screenshots\"\n").unwrap();
    // SAFETY: nothing else runs yet.
    unsafe { std::env::set_var("PIXLSNAPZ_HOME", &home) };

    let mtm = MainThreadMarker::new().expect("runs on the main thread");
    let _app = NSApplication::sharedApplication(mtm);
    let (report, problems) = pixlsnapz::settings_window::layout_report_for_tests(mtm);
    let _ = std::fs::remove_dir_all(&home);

    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/settings_layout.txt");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &report).unwrap();
        println!("updated {}", path.display());
    } else {
        let expected = std::fs::read_to_string(&path).unwrap_or_default();
        if expected != report {
            eprintln!("Settings layout differs from {}:\n", path.display());
            for line in diff(&expected, &report) {
                eprintln!("{line}");
            }
            eprintln!("\nIf the change is intended: UPDATE_SNAPSHOTS=1 cargo test --test settings_layout");
            std::process::exit(1);
        }
    }
    if !problems.is_empty() {
        for p in &problems {
            eprintln!("problem: {p}");
        }
        std::process::exit(1);
    }
    println!("settings layout: {} rows, no problems", report.lines().count());
}

/// Lines only in `old` (-) or only in `new` (+).
fn diff(old: &str, new: &str) -> Vec<String> {
    let old: Vec<&str> = old.lines().collect();
    let new: Vec<&str> = new.lines().collect();
    let mut out: Vec<String> = old.iter().filter(|l| !new.contains(l)).map(|l| format!("- {l}")).collect();
    out.extend(new.iter().filter(|l| !old.contains(l)).map(|l| format!("+ {l}")));
    out
}
