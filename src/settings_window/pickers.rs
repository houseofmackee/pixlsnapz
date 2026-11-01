//! Choosing an app for the shortcut-off list, and the screenshot folder.

use super::*;

use objc2::MainThreadMarker;
use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};

use crate::settings::Settings;

/// An app's name from its bundle identifier, or the identifier itself.
pub(super) fn app_name(bundle_id: &str) -> String {
    objc2_app_kit::NSWorkspace::sharedWorkspace()
        .URLForApplicationWithBundleIdentifier(&ns(bundle_id))
        .and_then(|url| url.lastPathComponent())
        .map(|n| n.to_string().trim_end_matches(".app").to_string())
        .unwrap_or_else(|| bundle_id.to_string())
}

pub(super) fn save_and_refresh(s: &Settings) {
    if let Err(e) = s.save() {
        tracing::warn!("saving settings: {e:#}");
    }
    crate::app::settings_updated();
    refresh();
}

/// Adds an app, chosen in Applications, to those the shortcut is off in.
pub(super) fn add_disabled_app() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(false);
    panel.setCanChooseFiles(true);
    panel.setTreatsFilePackagesAsDirectories(false);
    panel.setDirectoryURL(Some(&objc2_foundation::NSURL::fileURLWithPath(&ns("/Applications"))));
    panel.setPrompt(Some(&ns("Turn Off Shortcut")));
    panel.setMessage(Some(&ns("Choose an app in which the measure shortcut is left to the app.")));
    if panel.runModal() != NSModalResponseOK {
        return;
    }
    let Some(path) = panel.URL().and_then(|u| u.path()) else { return };
    let id = objc2_foundation::NSBundle::bundleWithPath(&path).and_then(|b| b.bundleIdentifier());
    match id {
        Some(id) => {
            let mut s = Settings::load();
            let id = id.to_string();
            if s.shortcut_enabled_in(&id) {
                s.toggle_disabled_app(&id);
            }
            save_and_refresh(&s);
        }
        None => crate::app::show_alert("Not an app", &format!("{path} has no bundle identifier.")),
    }
}

pub(super) fn choose_folder() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setCanCreateDirectories(true);
    panel.setPrompt(Some(&ns("Choose")));
    if panel.runModal() == NSModalResponseOK
        && let Some(path) = panel.URL().and_then(|u| u.path())
    {
        let mut s = Settings::load();
        s.screenshot_folder = path.to_string();
        if let Err(e) = s.save() {
            tracing::warn!("saving settings: {e:#}");
        }
        crate::app::settings_updated();
        refresh();
    }
}
