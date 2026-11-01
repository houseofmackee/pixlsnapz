//! Launch at login through `SMAppService` (macOS 13+). Only works when running
//! from `PixlSnapz.app`, not from `cargo run`.

use objc2_foundation::NSBundle;
use objc2_service_management::{SMAppService, SMAppServiceStatus};

fn in_app_bundle() -> bool {
    NSBundle::mainBundle().bundlePath().to_string().ends_with(".app")
}

pub fn is_enabled() -> bool {
    // SAFETY: a plain ServiceManagement call for the main app bundle.
    in_app_bundle() && unsafe { SMAppService::mainAppService().status() } == SMAppServiceStatus::Enabled
}

/// Turns launch at login on or off. Returns a message when it could not.
pub fn set(enabled: bool) -> Option<String> {
    if !in_app_bundle() {
        return Some("Launch at login only works when running PixlSnapz.app.".to_string());
    }
    // SAFETY: plain ServiceManagement calls for the main app bundle.
    let result = unsafe {
        let service = SMAppService::mainAppService();
        if enabled { service.registerAndReturnError() } else { service.unregisterAndReturnError() }
    };
    result.err().map(|e| e.localizedDescription().to_string())
}
