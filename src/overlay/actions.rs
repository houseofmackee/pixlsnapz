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

use crate::color::{average, contrast_text};
use crate::measure::PxRect;

pub(super) fn set_clipboard_string(text: &str) {
    let pasteboard = NSPasteboard::generalPasteboard();
    pasteboard.clearContents();
    // SAFETY: a static constant.
    pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
}

/// Copies the current measurement as "W × H". Returns whether there was one.
pub(super) fn copy_size() -> bool {
    let text = SESSION.with(|s| s.borrow().as_ref().and_then(Session::copy_text));
    let Some(text) = text else { return false };
    set_clipboard_string(&text);
    update(|s| s.toast(format!("Copied {text}")));
    true
}

/// Copies the color under the cursor in the chosen format.
pub(super) fn copy_color() {
    let text = SESSION.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        let (x, y) = s.hover_px()?;
        Some(s.settings.color_format.format(s.overlays[s.active].shot.frame.pixel(x, y)))
    });
    if let Some(text) = text {
        set_clipboard_string(&text);
        update(|s| s.toast(format!("Copied {text}")));
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
                s.toast("Contrast: point at the other color and press K");
            }
            Some(first) => {
                let text = contrast_text(first, here);
                s.toast(format!("Copied contrast {text}"));
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
        average(&s.overlays[i].shot.frame, r).map(|c| s.settings.color_format.format(c))
    });
    match text {
        Some(text) => {
            set_clipboard_string(&text);
            update(|s| s.toast(format!("Copied average {text}")));
        }
        None => update(|s| s.toast("Drag over an area first")),
    }
}

/// Saves (or copies) a PNG of the last area, or of the display when there is
/// none. Option toggles padding around the area.
pub(super) fn screenshot(save: bool, padded: bool) {
    let result: Option<(Vec<u8>, PathBuf)> = SESSION.with(|s| {
        let s = s.borrow();
        let s = s.as_ref()?;
        let (i, r) = s.screenshot_area()?;
        let o = &s.overlays[i];
        let pad = if padded { (SCREENSHOT_PADDING * o.scale) as usize } else { 0 };
        let r = o.shot.frame.clamp(PxRect {
            x0: r.x0.saturating_sub(pad),
            y0: r.y0.saturating_sub(pad),
            x1: r.x1 + pad,
            y1: r.y1 + pad,
        });
        let png = png_of(&o.shot.image, r)?;
        Some((png, s.settings.screenshot_dir()))
    });
    let Some((png, dir)) = result else {
        update(|s| s.toast("Could not take the screenshot"));
        return;
    };
    if save {
        let name = format!("PixlSnapz {}.png", chrono_like_now());
        let path = dir.join(name);
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &png)) {
            Ok(()) => update(|s| s.toast(format!("Saved to {}", dir.display()))),
            Err(e) => {
                tracing::warn!("saving {}: {e}", path.display());
                update(|s| s.toast("Could not save the screenshot"));
            }
        }
    } else {
        let pasteboard = NSPasteboard::generalPasteboard();
        pasteboard.clearContents();
        let data = objc2_foundation::NSData::with_bytes(&png);
        // SAFETY: a static constant.
        pasteboard.setData_forType(Some(&data), unsafe { NSPasteboardTypePNG });
        update(|s| s.toast("Screenshot copied"));
    }
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
