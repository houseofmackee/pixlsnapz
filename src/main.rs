#[cfg(not(target_os = "macos"))]
compile_error!("PixlSnapz only builds for macOS");

mod app;
mod capture;
mod hotkeys;
mod login_item;
mod overlay;
mod settings_window;
mod updates;

use std::process::ExitCode;

use objc2_foundation::{NSDistributedNotificationCenter, NSString};
use pixlsnapz::{color, items, measure, settings, shortcut};

const USAGE: &str = "\
PixlSnapz: measure anything on your screen

Usage:
  pixlsnapz              Start the menu bar app
  pixlsnapz measure      Start measuring in the running app
  pixlsnapz settings     Open Settings in the running app
  pixlsnapz --version    Print the version
  pixlsnapz --help       Print this help
";

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("PIXLSNAPZ_LOG").unwrap_or_else(|_| "pixlsnapz=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    // Launch Services may pass a `-psn_*` process serial number argument.
    let args: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with("-psn_")).collect();
    match args.first().map(String::as_str) {
        None => match app::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("pixlsnapz: {e:#}");
                ExitCode::FAILURE
            }
        },
        Some(command @ ("measure" | "settings")) => {
            let name = if command == "measure" { app::MEASURE_NOTIFICATION } else { app::SETTINGS_NOTIFICATION };
            // SAFETY: a notification without object or user info.
            unsafe {
                NSDistributedNotificationCenter::defaultCenter()
                    .postNotificationName_object_userInfo_deliverImmediately(
                        &NSString::from_str(name),
                        None,
                        None,
                        true,
                    )
            };
            ExitCode::SUCCESS
        }
        Some("--version" | "-V") => {
            println!("pixlsnapz {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("--help" | "-h") => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("pixlsnapz: unknown argument \"{other}\"\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}
