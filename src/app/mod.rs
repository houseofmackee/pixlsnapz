//! The menu bar app: status item, settings menu, global shortcut, and the
//! measuring overlay.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication, NSApplicationActivationPolicy, NSImage, NSMenu,
    NSMenuDelegate, NSMenuItem, NSRunningApplication, NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
    NSWorkspace, NSWorkspaceApplicationKey, NSWorkspaceDidActivateApplicationNotification,
};
use objc2_foundation::{
    NSAppleEventDescriptor, NSAppleEventManager, NSDistributedNotificationCenter, NSNotification, NSObject,
    NSObjectProtocol, NSString, NSURL,
};

use crate::hotkeys::{HotkeyAction, Hotkeys};
use crate::overlay::{self, StartError};
use crate::settings::Settings;
use crate::shortcut::Shortcut;
use crate::{capture, settings_window};

mod hold;
mod menu;
use menu::*;
pub use menu::{copy_to_clipboard, swatch};

/// Posted by `pixlsnapz measure` to start measuring in the running app.
pub const MEASURE_NOTIFICATION: &str = "io.github.houseofmackee.pixlsnapz.measure";
/// Posted by `pixlsnapz settings` to show the Settings window.
pub const SETTINGS_NOTIFICATION: &str = "io.github.houseofmackee.pixlsnapz.settings";
/// `'GURL'`: the Apple event class and id for opening a URL.
const GET_URL: u32 = u32::from_be_bytes(*b"GURL");
/// `'----'`: the direct object of an Apple event.
const KEY_DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");
const SCREEN_RECORDING_SETTINGS: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PixlSnapzTarget"]
    pub struct Target;

    impl Target {
        #[unsafe(method(menuAction:))]
        fn menu_action(&self, sender: &NSMenuItem) {
            menu_action(sender.tag());
        }

        /// Another app became active (for example with Command-Tab).
        #[unsafe(method(otherAppActivated:))]
        fn other_app_activated(&self, notification: &NSNotification) {
            // Measurements kept on screen stay; measuring ends. PixlSnapz
            // itself becoming active (Settings, alerts) does not count.
            let app = notification
                .userInfo()
                // SAFETY: a static constant.
                .and_then(|info| info.objectForKey(unsafe { NSWorkspaceApplicationKey }))
                .and_then(|app| app.downcast::<NSRunningApplication>().ok());
            let Some(app) = app else { return };
            if app.processIdentifier() == std::process::id() as i32 {
                return;
            }
            let front = front_app_of(&app);
            tracing::debug!("{} became active", front.as_ref().map_or("?", |f| f.0.as_str()));
            APP.with(|a| {
                if let Some(a) = a.borrow_mut().as_mut() {
                    a.front_app = front;
                }
            });
            overlay::end_measuring();
            // The shortcut may be turned off in this app.
            register_hotkey();
        }

        #[unsafe(method(remoteSettings:))]
        fn remote_settings(&self, _notification: &NSNotification) {
            settings_window::show(mtm());
        }

        /// A `pixlsnapz://` link was opened.
        #[unsafe(method(handleURL:withReplyEvent:))]
        fn handle_url(&self, event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
            // SAFETY: `paramDescriptorForKeyword:` takes an `AEKeyword` (u32)
            // and returns a descriptor or nil.
            let param: Option<Retained<NSAppleEventDescriptor>> =
                unsafe { msg_send![event, paramDescriptorForKeyword: KEY_DIRECT_OBJECT] };
            let url = param.and_then(|d| d.stringValue());
            if let Some(url) = url {
                open_pixlsnapz_url(&url.to_string());
            }
        }

        #[unsafe(method(remotePickColor:))]
        fn remote_pick_color(&self, _notification: Option<&NSNotification>) {
            pick_color();
        }

        #[unsafe(method(remoteMeasure:))]
        fn remote_measure(&self, _notification: &NSNotification) {
            // From the Measurement Sets menu: start with that set.
            if let Some(set) = PENDING_SET.with(|p| p.borrow_mut().take()) {
                report_start(overlay::measure_with_set(&set, mtm()));
            } else if !overlay::is_measuring() {
                measure();
            }
        }
    }

    unsafe impl NSObjectProtocol for Target {}

    unsafe impl NSMenuDelegate for Target {
        #[unsafe(method(menuNeedsUpdate:))]
        fn menu_needs_update(&self, menu: &NSMenu) {
            reload_settings();
            build_menu(menu, self);
        }
    }
);

struct App {
    target: Retained<Target>,
    _status_item: Retained<NSStatusItem>,
    hotkeys: Hotkeys,
    settings: Settings,
    registered: Vec<(HotkeyAction, Shortcut)>,
    /// A shortcut is being recorded in Settings: leave the hotkey off.
    suspended: bool,
    /// The app in front (bundle identifier and name), other than PixlSnapz.
    front_app: Option<(String, String)>,
}

/// The bundle identifier and name of `app`.
fn front_app_of(app: &NSRunningApplication) -> Option<(String, String)> {
    let id = app.bundleIdentifier()?.to_string();
    let name = app.localizedName().map_or_else(|| id.clone(), |n| n.to_string());
    Some((id, name))
}

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    /// A set chosen in the menu, loaded once the menu has closed.
    static PENDING_SET: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn mtm() -> MainThreadMarker {
    MainThreadMarker::new().expect("on the main thread")
}

pub fn run() -> anyhow::Result<()> {
    let mtm = mtm();
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);

    let target: Retained<Target> = unsafe { msg_send![Target::alloc(mtm), init] };
    let status_item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
    if let Some(button) = status_item.button(mtm) {
        let image = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str("ruler"),
            Some(&NSString::from_str("PixlSnapz")),
        );
        if let Some(image) = image {
            image.setTemplate(true);
            button.setImage(Some(&image));
        } else {
            button.setTitle(&NSString::from_str("PS"));
        }
        button.setToolTip(Some(&NSString::from_str("PixlSnapz")));
    }
    let menu = NSMenu::new(mtm);
    menu.setDelegate(Some(ProtocolObject::from_ref(&*target)));
    status_item.setMenu(Some(&menu));

    // SAFETY: the target lives for the whole process; selectors match its methods.
    unsafe {
        NSWorkspace::sharedWorkspace().notificationCenter().addObserver_selector_name_object(
            &target,
            sel!(otherAppActivated:),
            Some(NSWorkspaceDidActivateApplicationNotification),
            None,
        );
        NSDistributedNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &target,
            sel!(remoteMeasure:),
            Some(&NSString::from_str(MEASURE_NOTIFICATION)),
            None,
        );
        NSDistributedNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &target,
            sel!(remoteSettings:),
            Some(&NSString::from_str(SETTINGS_NOTIFICATION)),
            None,
        );
    }

    // SAFETY: the target lives for the whole process; the selector matches
    // its method.
    unsafe {
        let manager = NSAppleEventManager::sharedAppleEventManager();
        let _: () = msg_send![
            &manager,
            setEventHandler: &*target,
            andSelector: sel!(handleURL:withReplyEvent:),
            forEventClass: GET_URL,
            andEventID: GET_URL
        ];
    }

    let first_launch = !Settings::path().exists();
    let settings = Settings::load();
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            target,
            _status_item: status_item,
            hotkeys: Hotkeys::install(),
            settings,
            registered: Vec::new(),
            suspended: false,
            front_app: NSWorkspace::sharedWorkspace().frontmostApplication().and_then(|a| front_app_of(&a)),
        })
    });
    register_hotkey();
    hold::start();
    crate::updates::start(mtm);
    // For screenshots and tests: open Settings at a tab right away.
    if let Ok(tab) = std::env::var("PIXLSNAPZ_SHOW_SETTINGS") {
        settings_window::show(mtm);
        settings_window::select_tab(&tab);
    }
    if first_launch {
        welcome();
    }
    app.run();
    Ok(())
}

/// Registers the shortcut from the settings if it changed.
fn register_hotkey() {
    let failed = APP.with(|a| {
        let mut a = a.borrow_mut();
        let a = a.as_mut()?;
        if a.suspended {
            return None;
        }
        let shortcut = a.settings.shortcut();
        // Left to the app in front when it is one the shortcuts are off in.
        let off_here = a.front_app.as_ref().is_some_and(|(id, _)| !a.settings.shortcut_enabled_in(id));
        let mut wanted = Vec::new();
        if !off_here {
            wanted.push((HotkeyAction::Measure, shortcut));
            if let Some(color) = a.settings.color_shortcut().filter(|c| *c != shortcut) {
                wanted.push((HotkeyAction::PickColor, color));
            }
        }
        if a.registered == wanted {
            return None;
        }
        let failed = a.hotkeys.set(&wanted);
        a.registered = wanted;
        // Only the measure shortcut is worth an alert; the color one is
        // optional (a warning is logged).
        failed.into_iter().find(|f| *f == shortcut)
    });
    if let Some(shortcut) = failed {
        alert(
            &t!("The shortcut is not available"),
            &t!(
                "{shortcut} is used by another app. Choose another one in PixlSnapz Settings \
                 (menu bar icon > Settings…); you can still start measuring from the menu bar.",
                shortcut
            ),
            &[n!("OK")],
        );
    }
}

/// Unregisters the shortcut while a new one is recorded.
pub fn suspend_hotkey() {
    APP.with(|a| {
        if let Some(a) = a.borrow_mut().as_mut() {
            a.hotkeys.set(&[]);
            a.registered.clear();
            a.suspended = true;
        }
    });
}

/// Registers the shortcut from the settings again.
pub fn resume_hotkey() {
    APP.with(|a| {
        if let Some(a) = a.borrow_mut().as_mut() {
            a.suspended = false;
        }
    });
    reload_settings();
}

/// The Settings window changed the settings file.
pub fn settings_updated() {
    reload_settings();
}

pub fn show_alert(title: &str, message: &str) {
    alert(title, message, &["OK"]);
}

pub fn open_settings_file() {
    change_settings(|_| {});
    let path = Settings::path();
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    // Open in TextEdit (or the default editor for plain text).
    if !NSWorkspace::sharedWorkspace().openURL(&url) {
        let _ = std::process::Command::new("open").arg("-t").arg(&path).status();
    }
}

fn reload_settings() {
    let settings = Settings::load();
    APP.with(|a| {
        if let Some(a) = a.borrow_mut().as_mut() {
            a.settings = settings;
        }
    });
    register_hotkey();
}

fn settings() -> Settings {
    APP.with(|a| a.borrow().as_ref().map(|a| a.settings.clone()).unwrap_or_default())
}

fn change_settings(f: impl FnOnce(&mut Settings)) {
    let mut s = settings();
    f(&mut s);
    if let Err(e) = s.save() {
        tracing::warn!("saving settings: {e:#}");
    }
    APP.with(|a| {
        if let Some(a) = a.borrow_mut().as_mut() {
            a.settings = s;
        }
    });
    settings_window::refresh();
}

pub fn on_hotkey(action: HotkeyAction) {
    match action {
        HotkeyAction::Measure => measure(),
        HotkeyAction::PickColor => pick_color(),
    }
}

pub fn measure() {
    report_start(overlay::toggle(mtm()));
}

/// Starts picking a color: the loupe, and a click copies the color.
pub fn pick_color() {
    report_start(overlay::pick_color(mtm()));
}

/// For the Shortcuts actions (macos/intents/Intents.swift), which run in
/// this process on the main thread: runs a `pixlsnapz://` link.
///
/// # Safety
/// `url` is null or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pixlsnapz_open_url(url: *const std::ffi::c_char) {
    if url.is_null() || MainThreadMarker::new().is_none() {
        return;
    }
    // SAFETY: the caller passes a NUL-terminated string.
    let url = unsafe { std::ffi::CStr::from_ptr(url) }.to_string_lossy().into_owned();
    open_pixlsnapz_url(&url);
}

/// Runs a `pixlsnapz://measure?set=…` or `pixlsnapz://settings?tab=…` link.
fn open_pixlsnapz_url(url: &str) {
    use crate::integrations::{UrlCommand, parse_url};
    tracing::info!("opening {url}");
    match parse_url(url) {
        Some(UrlCommand::Measure { set: Some(set) }) => report_start(overlay::measure_with_set(&set, mtm())),
        Some(UrlCommand::Measure { set: None }) => {
            if !overlay::is_measuring() {
                measure();
            }
        }
        Some(UrlCommand::Settings { tab }) => {
            overlay::end_measuring();
            settings_window::show(mtm());
            if let Some(tab) = tab {
                settings_window::select_tab(&tab);
            }
        }
        Some(UrlCommand::PickColor) => pick_color(),
        Some(UrlCommand::Ruler { vertical }) => crate::rulers::new_ruler(vertical, mtm()),
        Some(UrlCommand::Colors) => {
            overlay::end_measuring();
            settings_window::show_colors(mtm());
        }
        None => tracing::warn!("unknown link {url}"),
    }
}

/// Explains why measuring could not start, if it could not.
fn report_start(result: Result<(), StartError>) {
    match result {
        Ok(()) => {}
        Err(StartError::NoPermission) => permission_alert(),
        Err(StartError::Capture(e)) => {
            tracing::error!("capture failed: {e:#}");
            if !capture::has_permission(false) {
                permission_alert();
            } else {
                alert(&t!("Could not capture the screen"), &format!("{e:#}"), &[n!("OK")]);
            }
        }
    }
}

fn permission_alert() {
    let choice = alert(
        &t!("PixlSnapz needs Screen Recording permission"),
        &t!("PixlSnapz looks at the pixels on your screen to find edges; nothing is saved or sent anywhere. \
             Turn on PixlSnapz in System Settings > Privacy & Security > Screen & System Audio Recording, \
             then quit and reopen PixlSnapz."),
        &[n!("Open System Settings"), n!("Cancel")],
    );
    if choice == 0 {
        open_url(SCREEN_RECORDING_SETTINGS);
    }
}

fn welcome() {
    let s = settings();
    alert(
        &t!("PixlSnapz is in the menu bar"),
        &t!(
            "Press {shortcut} anywhere to measure: move the pointer to see distances, drag over something to \
             measure it, and press Esc when done. The hints at the bottom of the screen list every shortcut.",
            shortcut = s.shortcut()
        ),
        &[n!("Get Started")],
    );
    change_settings(|_| {});
    if !capture::has_permission(false) {
        capture::has_permission(true);
    }
}

fn open_url(url: &str) {
    if let Some(url) = NSURL::URLWithString(&NSString::from_str(url)) {
        NSWorkspace::sharedWorkspace().openURL(&url);
    }
}

/// Shows a modal alert; returns the index of the chosen button.
fn alert(title: &str, message: &str, buttons: &[&str]) -> usize {
    let mtm = mtm();
    let a = NSAlert::new(mtm);
    a.setMessageText(&NSString::from_str(title));
    a.setInformativeText(&NSString::from_str(message));
    a.setAlertStyle(NSAlertStyle::Informational);
    // Button titles come marked with n!, translated here.
    for b in buttons {
        a.addButtonWithTitle(&NSString::from_str(&crate::i18n::tr(b)));
    }
    NSApplication::sharedApplication(mtm).activate();
    let response = a.runModal();
    (response - NSAlertFirstButtonReturn).max(0) as usize
}
