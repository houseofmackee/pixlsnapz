//! Copying sizes and colors, the contrast checker, and screenshots.

use super::*;

use std::path::PathBuf;

use objc2::AllocAnyThread;
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardTypePNG, NSPasteboardTypeString,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGImage;
use objc2_foundation::{NSDictionary, NSString};

use crate::color::{ColorHistory, average, contrast_text};
use crate::measure::PxRect;

pub(super) fn set_clipboard_string(text: &str) {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    // SAFETY: a static constant.
    pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
}

/// Copies the current measurement as "W × H". Returns whether there was one.
pub(super) fn copy_size() -> bool {
    let text = SESSION.with(|s| s.borrow().as_ref().and_then(Session::clipboard_text));
    let Some(text) = text else { return false };
    set_clipboard_string(&text);
    update(|s| s.toast(t!("Copied {text}", text)));
    true
}

/// Copies the color under the cursor in the chosen format.
pub(super) fn copy_color() {
    let text = SESSION.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        let (x, y) = s.hover_px()?;
        let rgb = s.overlays[s.active].shot.frame.pixel(x, y);
        Some((s.settings.color_format.format(rgb), rgb))
    });
    if let Some((text, rgb)) = text {
        ColorHistory::remember(rgb);
        set_clipboard_string(&text);
        update(|s| s.toast(t!("Copied {text}", text)));
    }
}

/// K: the first press picks a color, the second compares the color under
/// the pointer with it and copies the WCAG contrast ratio.
pub(super) fn pick_contrast() {
    let mut copied = None;
    update(|s| {
        let Some((x, y)) = s.hover_px() else { return };
        let here = s.overlays[s.active].shot.frame.pixel(x, y);
        match s.contrast.take() {
            None => {
                s.contrast = Some(here);
                s.toast(t!("Contrast: point at the other color and press K"));
            }
            Some(first) => {
                ColorHistory::remember(first);
                ColorHistory::remember(here);
                let text = contrast_text(first, here);
                s.toast(t!("Copied contrast {text}", text = &text));
                copied = Some(text);
            }
        }
    });
    if let Some(text) = copied {
        set_clipboard_string(&text);
    }
}

/// Copies the average color of the last area in the chosen format.
pub(super) fn copy_average_color() {
    let text = SESSION.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        let (i, r) = s.last_area()?;
        average(&s.overlays[i].shot.frame, r).map(|c| (s.settings.color_format.format(c), c))
    });
    match text {
        Some((text, rgb)) => {
            ColorHistory::remember(rgb);
            set_clipboard_string(&text);
            update(|s| s.toast(t!("Copied average {text}", text = &text)));
        }
        None => update(|s| s.toast(t!("Drag over an area first"))),
    }
}

/// Room around an area for its labels when measurements are drawn into a
/// screenshot (points).
const STAMP_MARGIN: f64 = 64.0;

/// A PNG of the last area, or of the display when there is none, with the
/// measurements drawn in when `stamp` (and there are any). `padded` adds
/// room around the area.
fn area_png(padded: bool, stamp: bool) -> Option<Vec<u8>> {
    let plan = SESSION.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        let (i, r) = s.screenshot_area()?;
        let o = &s.overlays[i];
        let stamp = stamp && !s.items.is_empty();
        let mut pad = if padded { SCREENSHOT_PADDING } else { 0.0 };
        if stamp {
            pad = pad.max(STAMP_MARGIN);
        }
        let pad = (pad * o.scale) as usize;
        let r = o.shot.frame.clamp(PxRect {
            x0: r.x0.saturating_sub(pad),
            y0: r.y0.saturating_sub(pad),
            x1: r.x1 + pad,
            y1: r.y1 + pad,
        });
        Some((i, r, stamp, o.view.clone(), o.rect_pt(r)))
    })?;
    let (i, r, stamp, view, rect) = plan;
    if stamp {
        stamped_png(&view, rect)
    } else {
        SESSION.with(|s| png_of(&s.borrow().as_ref()?.overlays[i].shot.image, r))
    }
}

/// Saves (or copies) a PNG of the last area, or of the display when there is
/// none, with the measurements drawn in unless that is turned off (M).
/// Option toggles padding around the area.
pub(super) fn screenshot(save: bool, padded: bool) {
    let (stamp, dir) = SESSION.with(|s| {
        s.borrow()
            .as_ref()
            .map_or((true, PathBuf::new()), |s| (s.settings.stamp_screenshots, s.settings.screenshot_dir()))
    });
    let Some(png) = area_png(padded, stamp) else {
        update(|s| s.toast(t!("Could not take the screenshot")));
        return;
    };
    if save {
        let name = format!("PixlSnapz {}.png", chrono_like_now());
        let path = dir.join(name);
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &png)) {
            Ok(()) => update(|s| s.toast(t!("Saved to {folder}", folder = dir.display()))),
            Err(e) => {
                tracing::warn!("saving {}: {e}", path.display());
                update(|s| s.toast(t!("Could not save the screenshot")));
            }
        }
    } else {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let data = objc2_foundation::NSData::with_bytes(&png);
        // SAFETY: a static constant.
        pasteboard.setData_forType(Some(&data), unsafe { NSPasteboardTypePNG });
        update(|s| s.toast(t!("Screenshot copied")));
    }
}

/// ⌘E: the last area (or the display) with its measurements, opened in
/// CleanShotZ's editor to annotate. Measuring ends, so the editor comes to
/// the front.
pub(super) fn send_to_cleanshotz() {
    let workspace = objc2_app_kit::NSWorkspace::sharedWorkspace();
    let installed = workspace
        .URLForApplicationWithBundleIdentifier(&NSString::from_str(crate::integrations::CLEANSHOTZ_BUNDLE_ID))
        .is_some();
    if !installed {
        update(|s| s.toast(t!("CleanShotZ is not installed")));
        return;
    }
    let Some(png) = area_png(false, true) else {
        update(|s| s.toast(t!("Could not take the screenshot")));
        return;
    };
    let dir = dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("PixlSnapz");
    let path = dir.join(format!("PixlSnapz {}.png", chrono_like_now()));
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &png)) {
        tracing::warn!("writing {}: {e}", path.display());
        update(|s| s.toast(t!("Could not save the screenshot")));
        return;
    }
    end();
    let url = crate::integrations::cleanshotz_annotate_url(&path.to_string_lossy());
    match objc2_foundation::NSURL::URLWithString(&NSString::from_str(&url)) {
        Some(url) => {
            workspace.openURL(&url);
        }
        None => tracing::warn!("not a URL: {url}"),
    }
}

/// Asks for a name in a small prompt above the overlay (the pointer shows
/// while it is open). `None` when cancelled or left empty.
/// Runs a modal panel over the overlay. `runModal` puts the panel at the
/// modal panel level, so the overlay steps down below it meanwhile; then the
/// overlay gets its level and the keyboard back.
fn prompt<T>(run: impl FnOnce() -> T) -> T {
    let windows =
        SESSION.with(|s| s.borrow().as_ref().map(|s| s.overlays.iter().map(|o| o.window.clone()).collect::<Vec<_>>()));
    let windows = windows.unwrap_or_default();
    for w in &windows {
        w.setLevel(objc2_app_kit::NSModalPanelWindowLevel - 1);
    }
    NSCursor::unhide();
    let result = run();
    NSCursor::hide();
    blank_cursor().set();
    for w in &windows {
        w.setLevel(LEVEL);
    }
    let key = SESSION.with(|s| {
        s.borrow().as_ref().map(|s| (s.overlays[s.active].window.clone(), s.overlays[s.active].view.clone()))
    });
    if let Some((window, view)) = key {
        window.makeKeyAndOrderFront(None);
        window.makeFirstResponder(Some(&view));
    }
    result
}

/// ⇧S: asks for a name (typed on the overlay itself: a separate window
/// would not reliably get the keyboard, as PixlSnapz is not the active app)
/// to save what is on screen as a set.
pub(super) fn save_set() {
    let sets = crate::items::SavedSets::load_from(&crate::items::SavedSets::path());
    update(|s| {
        if s.items.is_empty() {
            s.toast(t!("Measure something first"));
            return;
        }
        let suggestion = s.current_set.clone().unwrap_or_else(|| sets.unused_name(&t!("Set")));
        announce(&format!("{} {suggestion}", t!("Save these measurements and guides as")));
        s.naming = Some((suggestion, true));
    });
}

/// Return while naming a set: saves it.
pub(super) fn commit_set() {
    let Some(name) = SESSION.with(|s| s.borrow_mut().as_mut().and_then(|s| s.naming.take())).map(|(n, _)| n) else {
        return;
    };
    let name = name.trim().to_string();
    if name.is_empty() {
        redraw_all();
        return;
    }
    let path = crate::items::SavedSets::path();
    let mut sets = crate::items::SavedSets::load_from(&path);
    let items = SESSION.with(|s| s.borrow().as_ref().map(|s| s.saved().items)).unwrap_or_default();
    sets.put(&name, items);
    match sets.save_to(&path) {
        Ok(()) => update(|s| {
            s.current_set = Some(name.clone());
            s.toast(t!("Saved set {name}", name));
        }),
        Err(e) => {
            tracing::warn!("saving sets: {e:#}");
            update(|s| s.toast(t!("Could not save the set")));
        }
    }
}

/// Loads the design overlay image at `path`: the image and its size in
/// pixels (for vector images, at 2 pixels per point).
pub(super) fn load_overlay_image(path: &str) -> Option<(Retained<NSImage>, f64, f64)> {
    if path.is_empty() {
        return None;
    }
    let image = NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(path))?;
    let reps = image.representations();
    let rep = reps.firstObject()?;
    let (mut w, mut h) = (rep.pixelsWide() as f64, rep.pixelsHigh() as f64);
    if w <= 0.0 || h <= 0.0 {
        let size = image.size();
        (w, h) = (size.width * 2.0, size.height * 2.0);
    }
    Some((image, w, h))
}

/// Asks for a design image (above the overlay) and shows it, with its
/// top-left corner at the last kept box, or the screen's top-left.
pub(super) fn choose_overlay_image() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let panel = objc2_app_kit::NSOpenPanel::openPanel(mtm);
    panel.setCanChooseFiles(true);
    panel.setCanChooseDirectories(false);
    panel.setMessage(Some(&NSString::from_str(&t!("Choose a design image to lay over the screen"))));
    let chosen = prompt(|| {
        if panel.runModal() == objc2_app_kit::NSModalResponseOK {
            panel.URL().and_then(|u| u.path()).map(|p| p.to_string())
        } else {
            None
        }
    });
    let Some(path) = chosen else { return };
    match load_overlay_image(&path) {
        Some(image) => update(|s| {
            s.overlay_image = Some(image);
            let corner = s.last_area().map_or((0.0, 0.0), |(_, r)| (r.x0 as f64, r.y0 as f64));
            (s.settings.overlay_x, s.settings.overlay_y) = corner;
            s.settings.overlay_image = path;
            s.settings.overlay_visible = true;
            s.save_settings();
            s.toast(t!("Design overlay: ⇧I opacity, ⌘ arrows move, ⌘I to the pointer"));
        }),
        None => update(|s| s.toast(t!("Not an image"))),
    }
}

/// Renders `rect` (points) of `view` in export mode: the screenshot with the
/// kept measurements on it, without the pointer, hints, or messages.
fn stamped_png(view: &OverlayView, rect: NSRect) -> Option<Vec<u8>> {
    SESSION.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.exporting = true;
        }
    });
    // Drawing happens right here, through drawRect: (no session borrow held).
    let rep = view.bitmapImageRepForCachingDisplayInRect(rect);
    if let Some(rep) = &rep {
        view.cacheDisplayInRect_toBitmapImageRep(rect, rep);
    }
    SESSION.with(|s| {
        if let Some(s) = s.borrow_mut().as_mut() {
            s.exporting = false;
        }
    });
    // SAFETY: an empty properties dictionary.
    let data = unsafe { rep?.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    Some(data.to_vec())
}

pub(super) fn png_of(image: &CGImage, r: PxRect) -> Option<Vec<u8>> {
    let rect = CGRect::new(CGPoint::new(r.x0 as f64, r.y0 as f64), CGSize::new(r.width() as f64, r.height() as f64));
    let cropped = CGImage::with_image_in_rect(Some(image), rect)?;
    let rep = NSBitmapImageRep::initWithCGImage(NSBitmapImageRep::alloc(), &cropped);
    // SAFETY: an empty properties dictionary.
    let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
    Some(data.to_vec())
}

/// Local date and time as "2026-11-01 at 16.42.05", like macOS screenshots.
pub(super) fn chrono_like_now() -> String {
    // SAFETY: localtime_r fills the given struct.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}-{:02}-{:02} at {:02}.{:02}.{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec
        )
    }
}
