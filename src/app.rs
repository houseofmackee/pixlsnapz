//! The menu bar app: status item, settings menu, global shortcut, and the
//! measuring overlay.

use std::cell::RefCell;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool, ProtocolObject};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSAlertStyle, NSApplication, NSApplicationActivationPolicy,
    NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSImage, NSMenu, NSMenuDelegate, NSMenuItem,
    NSRunningApplication, NSStatusBar, NSStatusItem, NSVariableStatusItemLength, NSWorkspace,
    NSWorkspaceApplicationKey, NSWorkspaceDidActivateApplicationNotification,
};
use objc2_foundation::{
    NSDistributedNotificationCenter, NSNotification, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
    NSURL,
};

use crate::color::{ColorFormat, ColorHistory};
use crate::hotkeys::{HotkeyAction, Hotkeys};
use crate::overlay::{self, StartError};
use crate::settings::{Axes, COLORS, DESIGN_ZOOMS, Grid, Settings, TOLERANCE_STEPS, Units};
use crate::shortcut::{CMD, CONTROL, OPTION, SHIFT, Shortcut};
use crate::{capture, login_item, settings_window};

/// Posted by `pixlsnapz measure` to start measuring in the running app.
pub const MEASURE_NOTIFICATION: &str = "io.github.houseofmackee.pixlsnapz.measure";
/// Posted by `pixlsnapz settings` to show the Settings window.
pub const SETTINGS_NOTIFICATION: &str = "io.github.houseofmackee.pixlsnapz.settings";
const SCREEN_RECORDING_SETTINGS: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture";

// Menu item tags. Submenu choices add their index to a base.
const MEASURE: isize = 1;
const SNAP: isize = 2;
const LOUPE: isize = 3;
const HINTS: isize = 4;
const SHOW_UNITS: isize = 5;
const ASPECT: isize = 6;
const FREEZE: isize = 7;
const LOGIN: isize = 8;
const OPEN_SETTINGS: isize = 9;
const PERMISSION: isize = 10;
const ABOUT: isize = 11;
const QUIT: isize = 12;
const SETTINGS: isize = 13;
const CLEAR_KEPT: isize = 14;
const TOGGLE_APP: isize = 15;
const CLEAR_COLORS: isize = 16;
const UNITS_BASE: isize = 100;
const TOLERANCE_BASE: isize = 200;
const COLOR_BASE: isize = 300;
const FORMAT_BASE: isize = 400;
const AXES_BASE: isize = 500;
const ZOOM_BASE: isize = 600;
const GRID_BASE: isize = 700;
const RECENT_BASE: isize = 800;
const AXES: [Axes; 3] = [Axes::Both, Axes::Horizontal, Axes::Vertical];

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

        #[unsafe(method(remoteMeasure:))]
        fn remote_measure(&self, _notification: &NSNotification) {
            if !overlay::is_measuring() {
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
    registered: Option<Shortcut>,
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

    let first_launch = !Settings::path().exists();
    let settings = Settings::load();
    APP.with(|a| {
        *a.borrow_mut() = Some(App {
            target,
            _status_item: status_item,
            hotkeys: Hotkeys::install(),
            settings,
            registered: None,
            suspended: false,
            front_app: NSWorkspace::sharedWorkspace().frontmostApplication().and_then(|a| front_app_of(&a)),
        })
    });
    register_hotkey();
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
        // Left to the app in front when it is one the shortcut is off in.
        let off_here = a.front_app.as_ref().is_some_and(|(id, _)| !a.settings.shortcut_enabled_in(id));
        let wanted = (!off_here).then_some(shortcut);
        if a.registered == wanted {
            return None;
        }
        a.registered = wanted;
        match wanted {
            Some(shortcut) => a.hotkeys.set(&[(HotkeyAction::Measure, shortcut)]).first().copied(),
            None => {
                a.hotkeys.set(&[]);
                None
            }
        }
    });
    if let Some(shortcut) = failed {
        alert(
            "The shortcut is not available",
            &format!(
                "{shortcut} is used by another app. Choose another one in PixlSnapz Settings \
                 (menu bar icon > Settings…); you can still start measuring from the menu bar."
            ),
            &["OK"],
        );
    }
}

/// Unregisters the shortcut while a new one is recorded.
pub fn suspend_hotkey() {
    APP.with(|a| {
        if let Some(a) = a.borrow_mut().as_mut() {
            a.hotkeys.set(&[]);
            a.registered = None;
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
    }
}

pub fn measure() {
    match overlay::toggle(mtm()) {
        Ok(()) => {}
        Err(StartError::NoPermission) => permission_alert(),
        Err(StartError::Capture(e)) => {
            tracing::error!("capture failed: {e:#}");
            if !capture::has_permission(false) {
                permission_alert();
            } else {
                alert("Could not capture the screen", &format!("{e:#}"), &["OK"]);
            }
        }
    }
}

fn permission_alert() {
    let choice = alert(
        "PixlSnapz needs Screen Recording permission",
        "PixlSnapz looks at the pixels on your screen to find edges; nothing is saved or sent anywhere. \
         Turn on PixlSnapz in System Settings > Privacy & Security > Screen & System Audio Recording, \
         then quit and reopen PixlSnapz.",
        &["Open System Settings", "Cancel"],
    );
    if choice == 0 {
        open_url(SCREEN_RECORDING_SETTINGS);
    }
}

fn welcome() {
    let s = settings();
    alert(
        "PixlSnapz is in the menu bar",
        &format!(
            "Press {} anywhere to measure: move the pointer to see distances, drag over something to measure it, \
             and press Esc when done. The hints at the bottom of the screen list every shortcut.",
            s.shortcut()
        ),
        &["Get Started"],
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
    for b in buttons {
        a.addButtonWithTitle(&NSString::from_str(b));
    }
    NSApplication::sharedApplication(mtm).activate();
    let response = a.runModal();
    (response - NSAlertFirstButtonReturn).max(0) as usize
}

fn item(menu: &NSMenu, title: &str, tag: isize, target: &Target) -> Retained<NSMenuItem> {
    let mtm = mtm();
    // SAFETY: the action exists on the target.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(sel!(menuAction:)),
            &NSString::from_str(""),
        )
    };
    item.setTag(tag);
    // SAFETY: the target outlives the menu.
    unsafe { item.setTarget(Some(target)) };
    menu.addItem(&item);
    item
}

fn check(menu: &NSMenu, title: &str, tag: isize, on: bool, target: &Target) {
    let i = item(menu, title, tag, target);
    i.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
}

fn submenu(menu: &NSMenu, title: &str) -> Retained<NSMenu> {
    let mtm = mtm();
    let parent = NSMenuItem::new(mtm);
    parent.setTitle(&NSString::from_str(title));
    let sub = NSMenu::new(mtm);
    parent.setSubmenu(Some(&sub));
    menu.addItem(&parent);
    sub
}

fn build_menu(menu: &NSMenu, target: &Target) {
    let mtm = mtm();
    menu.removeAllItems();
    let s = settings();

    let shortcut = s.shortcut();
    let measure = item(menu, "Measure", MEASURE, target);
    measure.setKeyEquivalent(&NSString::from_str(shortcut.key_name()));
    let mut mask = NSEventModifierFlags::empty();
    for (bit, flag) in [
        (CMD, NSEventModifierFlags::Command),
        (SHIFT, NSEventModifierFlags::Shift),
        (OPTION, NSEventModifierFlags::Option),
        (CONTROL, NSEventModifierFlags::Control),
    ] {
        if shortcut.modifiers & bit != 0 {
            mask |= flag;
        }
    }
    measure.setKeyEquivalentModifierMask(mask);
    if overlay::has_kept() {
        item(menu, "Clear Kept Measurements", CLEAR_KEPT, target);
    }
    let front = APP.with(|a| a.borrow().as_ref().and_then(|a| a.front_app.clone()));
    if let Some((id, name)) = front {
        let title = if s.shortcut_enabled_in(&id) {
            format!("Turn Off Shortcut in {name}")
        } else {
            format!("Turn On Shortcut in {name}")
        };
        item(menu, &title, TOGGLE_APP, target);
    }
    let recent = submenu(menu, "Recent Colors");
    let history = ColorHistory::load_from(&ColorHistory::path());
    if history.colors.is_empty() {
        // No action: shown disabled.
        recent.addItem(&NSMenuItem::new(mtm));
        if let Some(empty) = recent.itemAtIndex(0) {
            empty.setTitle(&NSString::from_str("No Colors Yet"));
        }
    } else {
        for (i, rgb) in history.colors.iter().enumerate() {
            let it = item(&recent, &s.color_format.format(*rgb), RECENT_BASE + i as isize, target);
            it.setImage(Some(&swatch(*rgb)));
        }
        recent.addItem(&NSMenuItem::separatorItem(mtm));
        item(&recent, "Clear Recent Colors", CLEAR_COLORS, target);
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));

    let units = submenu(menu, "Units");
    for (i, u) in Units::ALL.iter().enumerate() {
        check(&units, u.name(), UNITS_BASE + i as isize, s.units == *u, target);
    }
    let zoom = submenu(menu, "Design Zoom");
    for (i, z) in DESIGN_ZOOMS.iter().enumerate() {
        check(&zoom, &format!("{z}%"), ZOOM_BASE + i as isize, s.design_zoom == *z, target);
    }
    let grid = submenu(menu, "Grid");
    for (i, g) in Grid::ALL.iter().enumerate() {
        check(&grid, g.name(), GRID_BASE + i as isize, s.grid == *g, target);
    }
    let tolerance = submenu(menu, "Edge Tolerance");
    for (i, (value, name)) in TOLERANCE_STEPS.iter().enumerate() {
        check(&tolerance, name, TOLERANCE_BASE + i as isize, s.tolerance == *value, target);
    }
    let colors = submenu(menu, "Color");
    for (i, (name, hex)) in COLORS.iter().enumerate() {
        check(&colors, name, COLOR_BASE + i as isize, s.color.eq_ignore_ascii_case(hex), target);
    }
    let formats = submenu(menu, "Color Format");
    for (i, f) in ColorFormat::ALL.iter().enumerate() {
        check(&formats, f.name(), FORMAT_BASE + i as isize, s.color_format == *f, target);
    }
    let directions = submenu(menu, "Measure Directions");
    for (i, a) in AXES.iter().enumerate() {
        check(&directions, a.name(), AXES_BASE + i as isize, s.axes == *a, target);
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    check(menu, "Snap Areas to Content", SNAP, s.snap_areas, target);
    check(menu, "Freeze Screen While Measuring", FREEZE, s.freeze, target);
    check(menu, "Show Loupe", LOUPE, s.loupe, target);
    check(menu, "Show Units", SHOW_UNITS, s.show_units, target);
    check(menu, "Show Aspect Ratio", ASPECT, s.show_aspect_ratio, target);
    check(menu, "Show Shortcut Hints", HINTS, s.show_hints, target);
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    let settings_item = item(menu, "Settings…", SETTINGS, target);
    settings_item.setKeyEquivalent(&NSString::from_str(","));
    check(menu, "Launch at Login", LOGIN, login_item::is_enabled(), target);
    if !capture::has_permission(false) {
        item(menu, "Allow Screen Recording…", PERMISSION, target);
    }
    menu.addItem(&NSMenuItem::separatorItem(mtm));
    item(menu, "About PixlSnapz", ABOUT, target);
    let quit = item(menu, "Quit PixlSnapz", QUIT, target);
    quit.setKeyEquivalent(&NSString::from_str("q"));
}

/// A small rounded square of `rgb`, for menus.
fn swatch(rgb: [u8; 3]) -> Retained<NSImage> {
    let draw = RcBlock::new(move |rect: NSRect| {
        let inner = NSRect::new(
            NSPoint::new(rect.origin.x + 0.5, rect.origin.y + 0.5),
            NSSize::new(rect.size.width - 1.0, rect.size.height - 1.0),
        );
        let path = objc2_app_kit::NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(inner, 3.0, 3.0);
        objc2_app_kit::NSColor::colorWithSRGBRed_green_blue_alpha(
            f64::from(rgb[0]) / 255.0,
            f64::from(rgb[1]) / 255.0,
            f64::from(rgb[2]) / 255.0,
            1.0,
        )
        .setFill();
        path.fill();
        objc2_app_kit::NSColor::colorWithWhite_alpha(0.5, 0.5).setStroke();
        path.setLineWidth(0.5);
        path.stroke();
        Bool::YES
    });
    NSImage::imageWithSize_flipped_drawingHandler(NSSize::new(14.0, 14.0), false, &draw)
}

fn copy_to_clipboard(text: &str) {
    let pasteboard = objc2_app_kit::NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    // SAFETY: a static constant.
    pasteboard.setString_forType(&NSString::from_str(text), unsafe { objc2_app_kit::NSPasteboardTypeString });
}

fn menu_action(tag: isize) {
    let mtm = mtm();
    match tag {
        // Let the menu close before the screen is captured.
        MEASURE => {
            APP.with(|a| {
                if let Some(a) = a.borrow().as_ref() {
                    // SAFETY: the selector exists on the target.
                    let _: () = unsafe {
                        msg_send![&*a.target, performSelector: sel!(remoteMeasure:), withObject: None::<&AnyObject>,
                            afterDelay: 0.15f64]
                    };
                }
            });
        }
        SNAP => change_settings(|s| s.snap_areas = !s.snap_areas),
        LOUPE => change_settings(|s| s.loupe = !s.loupe),
        HINTS => change_settings(|s| s.show_hints = !s.show_hints),
        SHOW_UNITS => change_settings(|s| s.show_units = !s.show_units),
        ASPECT => change_settings(|s| s.show_aspect_ratio = !s.show_aspect_ratio),
        FREEZE => change_settings(|s| s.freeze = !s.freeze),
        LOGIN => {
            if let Some(message) = login_item::set(!login_item::is_enabled()) {
                alert("Launch at Login", &message, &["OK"]);
            }
        }
        OPEN_SETTINGS => open_settings_file(),
        SETTINGS => settings_window::show(mtm),
        CLEAR_KEPT => overlay::end(),
        TOGGLE_APP => {
            let front = APP.with(|a| a.borrow().as_ref().and_then(|a| a.front_app.clone()));
            if let Some((id, _)) = front {
                change_settings(|s| s.toggle_disabled_app(&id));
                register_hotkey();
            }
        }
        CLEAR_COLORS => {
            if let Err(e) = ColorHistory::default().save_to(&ColorHistory::path()) {
                tracing::warn!("clearing the color history: {e:#}");
            }
        }
        t if (RECENT_BASE..RECENT_BASE + ColorHistory::MAX as isize).contains(&t) => {
            let history = ColorHistory::load_from(&ColorHistory::path());
            if let Some(rgb) = history.colors.get((t - RECENT_BASE) as usize) {
                copy_to_clipboard(&settings().color_format.format(*rgb));
                ColorHistory::remember(*rgb);
            }
        }
        PERMISSION => {
            if !capture::has_permission(true) {
                open_url(SCREEN_RECORDING_SETTINGS);
            }
        }
        ABOUT => {
            let app = NSApplication::sharedApplication(mtm);
            app.activate();
            app.orderFrontStandardAboutPanel(None);
        }
        QUIT => NSApplication::sharedApplication(mtm).terminate(None),
        t if (UNITS_BASE..UNITS_BASE + Units::ALL.len() as isize).contains(&t) => {
            change_settings(|s| s.units = Units::ALL[(t - UNITS_BASE) as usize])
        }
        t if (ZOOM_BASE..ZOOM_BASE + DESIGN_ZOOMS.len() as isize).contains(&t) => {
            change_settings(|s| s.design_zoom = DESIGN_ZOOMS[(t - ZOOM_BASE) as usize])
        }
        t if (GRID_BASE..GRID_BASE + Grid::ALL.len() as isize).contains(&t) => {
            change_settings(|s| s.grid = Grid::ALL[(t - GRID_BASE) as usize])
        }
        t if (TOLERANCE_BASE..TOLERANCE_BASE + TOLERANCE_STEPS.len() as isize).contains(&t) => {
            change_settings(|s| s.tolerance = TOLERANCE_STEPS[(t - TOLERANCE_BASE) as usize].0)
        }
        t if (COLOR_BASE..COLOR_BASE + COLORS.len() as isize).contains(&t) => {
            change_settings(|s| s.color = COLORS[(t - COLOR_BASE) as usize].1.to_string())
        }
        t if (FORMAT_BASE..FORMAT_BASE + ColorFormat::ALL.len() as isize).contains(&t) => {
            change_settings(|s| s.color_format = ColorFormat::ALL[(t - FORMAT_BASE) as usize])
        }
        t if (AXES_BASE..AXES_BASE + AXES.len() as isize).contains(&t) => {
            change_settings(|s| s.axes = AXES[(t - AXES_BASE) as usize])
        }
        _ => {}
    }
}
