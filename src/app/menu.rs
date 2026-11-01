//! The menu bar icon's menu: building it each time it opens, and its actions.

use super::*;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Bool};
use objc2::{MainThreadOnly, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSControlStateValueOff, NSControlStateValueOn, NSEventModifierFlags, NSImage, NSMenu, NSMenuItem,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};

use crate::color::{ColorFormat, ColorHistory};
use crate::items::SavedSets;
use crate::overlay::{self};
use crate::settings::{Axes, COLORS, DESIGN_ZOOMS, Grid, TOLERANCE_STEPS, Units};
use crate::shortcut::{CMD, CONTROL, OPTION, SHIFT};
use crate::units::SizeFormat;
use crate::{capture, login_item, settings_window};

// Menu item tags. Submenu choices add their index to a base.
pub(super) const MEASURE: isize = 1;
pub(super) const SNAP: isize = 2;
pub(super) const LOUPE: isize = 3;
pub(super) const HINTS: isize = 4;
pub(super) const SHOW_UNITS: isize = 5;
pub(super) const ASPECT: isize = 6;
pub(super) const FREEZE: isize = 7;
pub(super) const LOGIN: isize = 8;
pub(super) const OPEN_SETTINGS: isize = 9;
pub(super) const PERMISSION: isize = 10;
pub(super) const ABOUT: isize = 11;
pub(super) const QUIT: isize = 12;
pub(super) const SETTINGS: isize = 13;
pub(super) const CLEAR_KEPT: isize = 14;
pub(super) const TOGGLE_APP: isize = 15;
pub(super) const CLEAR_COLORS: isize = 16;
pub(super) const UNITS_BASE: isize = 100;
pub(super) const TOLERANCE_BASE: isize = 200;
pub(super) const COLOR_BASE: isize = 300;
pub(super) const FORMAT_BASE: isize = 400;
pub(super) const AXES_BASE: isize = 500;
pub(super) const ZOOM_BASE: isize = 600;
pub(super) const GRID_BASE: isize = 700;
pub(super) const RECENT_BASE: isize = 800;
pub(super) const CHECK_UPDATES: isize = 17;
pub(super) const OVERLAY_CHOOSE: isize = 18;
pub(super) const OVERLAY_SHOW: isize = 19;
pub(super) const OVERLAY_REMOVE: isize = 20;
pub(super) const SIZE_FORMAT_BASE: isize = 900;
pub(super) const SET_BASE: isize = 1000;
pub(super) const DELETE_SET_BASE: isize = 1100;
/// Sets listed in the menu (each base leaves room for this many).
const MAX_SETS: usize = 100;
pub(super) const AXES: [Axes; 3] = [Axes::Both, Axes::Horizontal, Axes::Vertical];

pub(super) fn item(menu: &NSMenu, title: &str, tag: isize, target: &Target) -> Retained<NSMenuItem> {
    let mtm = mtm();
    // SAFETY: the action exists on the target.
    let item = unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(&crate::i18n::tr(title)),
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

pub(super) fn check(menu: &NSMenu, title: &str, tag: isize, on: bool, target: &Target) {
    let i = item(menu, title, tag, target);
    i.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
}

pub(super) fn submenu(menu: &NSMenu, title: &str) -> Retained<NSMenu> {
    let mtm = mtm();
    let parent = NSMenuItem::new(mtm);
    parent.setTitle(&NSString::from_str(&crate::i18n::tr(title)));
    let sub = NSMenu::new(mtm);
    parent.setSubmenu(Some(&sub));
    menu.addItem(&parent);
    sub
}

pub(super) fn build_menu(menu: &NSMenu, target: &Target) {
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
            t!("Turn Off Shortcut in {name}", name)
        } else {
            t!("Turn On Shortcut in {name}", name)
        };
        item(menu, &title, TOGGLE_APP, target);
    }
    let recent = submenu(menu, "Recent Colors");
    let history = ColorHistory::load_from(&ColorHistory::path());
    if history.colors.is_empty() {
        // No action: shown disabled.
        recent.addItem(&NSMenuItem::new(mtm));
        if let Some(empty) = recent.itemAtIndex(0) {
            empty.setTitle(&NSString::from_str(&t!("No Colors Yet")));
        }
    } else {
        // Hex titles keep the menu narrow; clicking copies the chosen format.
        recent.addItem(&NSMenuItem::new(mtm));
        if let Some(header) = recent.itemAtIndex(0) {
            header.setTitle(&NSString::from_str(&t!("Click to Copy as {format}", format = s.color_format.name())));
        }
        for (i, rgb) in history.colors.iter().enumerate() {
            let it = item(&recent, &ColorFormat::Hex.format(*rgb), RECENT_BASE + i as isize, target);
            it.setImage(Some(&swatch(*rgb)));
        }
        recent.addItem(&NSMenuItem::separatorItem(mtm));
        item(&recent, "Clear Recent Colors", CLEAR_COLORS, target);
    }
    let sets = SavedSets::load_from(&SavedSets::path());
    let saved = submenu(menu, "Measurement Sets");
    if sets.sets.is_empty() {
        saved.addItem(&NSMenuItem::new(mtm));
        if let Some(empty) = saved.itemAtIndex(0) {
            empty.setTitle(&NSString::from_str(&t!("No Sets Yet: ⇧S Saves One While Measuring")));
        }
    } else {
        saved.addItem(&NSMenuItem::new(mtm));
        if let Some(header) = saved.itemAtIndex(0) {
            header.setTitle(&NSString::from_str(&t!("Measure With")));
        }
        for (i, set) in sets.sets.iter().take(MAX_SETS).enumerate() {
            item(&saved, &set.name, SET_BASE + i as isize, target);
        }
        saved.addItem(&NSMenuItem::separatorItem(mtm));
        let delete = submenu(&saved, "Delete");
        for (i, set) in sets.sets.iter().take(MAX_SETS).enumerate() {
            item(&delete, &set.name, DELETE_SET_BASE + i as isize, target);
        }
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
    let sizes = submenu(menu, "Size Format");
    for (i, f) in SizeFormat::ALL.iter().enumerate() {
        check(&sizes, f.name(), SIZE_FORMAT_BASE + i as isize, s.size_format == *f, target);
    }
    let overlay = submenu(menu, "Design Overlay");
    item(&overlay, "Choose Image…", OVERLAY_CHOOSE, target);
    if !s.overlay_image.is_empty() {
        check(&overlay, "Show While Measuring", OVERLAY_SHOW, s.overlay_visible, target);
        item(&overlay, "Remove Image", OVERLAY_REMOVE, target);
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
    if crate::updates::available() {
        let title = match crate::updates::found() {
            Some(version) => t!("Update to PixlSnapz {version}…", version),
            None => t!("Check for Updates…"),
        };
        item(menu, &title, CHECK_UPDATES, target);
    }
    item(menu, "About PixlSnapz", ABOUT, target);
    let quit = item(menu, "Quit PixlSnapz", QUIT, target);
    quit.setKeyEquivalent(&NSString::from_str("q"));
}

/// A small rounded square of `rgb`, for menus.
pub(super) fn swatch(rgb: [u8; 3]) -> Retained<NSImage> {
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

pub(super) fn copy_to_clipboard(text: &str) {
    let pasteboard = objc2_app_kit::NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    // SAFETY: a static constant.
    pasteboard.setString_forType(&NSString::from_str(text), unsafe { objc2_app_kit::NSPasteboardTypeString });
}

pub(super) fn menu_action(tag: isize) {
    let mtm = mtm();
    match tag {
        t if (SET_BASE..SET_BASE + MAX_SETS as isize).contains(&t) => {
            let sets = SavedSets::load_from(&SavedSets::path());
            if let Some(set) = sets.sets.get((t - SET_BASE) as usize) {
                PENDING_SET.with(|p| *p.borrow_mut() = Some(set.name.clone()));
                menu_action(MEASURE);
            }
        }
        t if (DELETE_SET_BASE..DELETE_SET_BASE + MAX_SETS as isize).contains(&t) => {
            let path = SavedSets::path();
            let mut sets = SavedSets::load_from(&path);
            if let Some(name) = sets.sets.get((t - DELETE_SET_BASE) as usize).map(|s| s.name.clone()) {
                sets.remove(&name);
                if let Err(e) = sets.save_to(&path) {
                    tracing::warn!("deleting set {name}: {e:#}");
                }
            }
        }
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
                alert(&t!("Launch at Login"), &message, &[n!("OK")]);
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
        CHECK_UPDATES => crate::updates::check(mtm),
        OVERLAY_CHOOSE => {
            let app = NSApplication::sharedApplication(mtm);
            app.activate();
            let panel = objc2_app_kit::NSOpenPanel::openPanel(mtm);
            panel.setCanChooseFiles(true);
            panel.setCanChooseDirectories(false);
            panel.setMessage(Some(&NSString::from_str(&t!("Choose a design image to lay over the screen"))));
            if panel.runModal() == objc2_app_kit::NSModalResponseOK
                && let Some(path) = panel.URL().and_then(|u| u.path())
            {
                change_settings(|s| {
                    s.overlay_image = path.to_string();
                    s.overlay_visible = true;
                });
            }
        }
        OVERLAY_SHOW => change_settings(|s| s.overlay_visible = !s.overlay_visible),
        OVERLAY_REMOVE => change_settings(|s| {
            s.overlay_image.clear();
            s.overlay_visible = false;
        }),
        t if (SIZE_FORMAT_BASE..SIZE_FORMAT_BASE + SizeFormat::ALL.len() as isize).contains(&t) => {
            change_settings(|s| s.size_format = SizeFormat::ALL[(t - SIZE_FORMAT_BASE) as usize])
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
