//! Element snapping: the frame of the button, field, image, or other
//! interface element under the pointer, from the Accessibility API. Needs
//! Accessibility permission, so it is opt-in.

use std::ffi::c_void;

use objc2_core_foundation::{CGPoint, CGSize};
use objc2_foundation::{NSDictionary, NSNumber, NSString};

use crate::capture::WindowFrame;

type CFTypeRef = *const c_void;
type AXUIElementRef = *const c_void;
type AXError = i32;

const AX_VALUE_CG_POINT: u32 = 1;
const AX_VALUE_CG_SIZE: u32 = 2;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrustedWithOptions(options: CFTypeRef) -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
    fn AXUIElementCopyElementAtPosition(app: AXUIElementRef, x: f32, y: f32, element: *mut AXUIElementRef) -> AXError;
    fn AXUIElementCopyAttributeValue(element: AXUIElementRef, attribute: CFTypeRef, value: *mut CFTypeRef) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, seconds: f32) -> AXError;
    fn AXValueGetValue(value: CFTypeRef, kind: u32, out: *mut c_void) -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: CFTypeRef);
}

/// Whether PixlSnapz may use the Accessibility API. With `ask`, macOS shows
/// its prompt (once) to allow it in System Settings.
pub fn trusted(ask: bool) -> bool {
    let key = NSString::from_str("AXTrustedCheckOptionPrompt");
    let options = NSDictionary::from_slices(&[&*key], &[&*NSNumber::new_bool(ask)]);
    // SAFETY: an NSDictionary is a CFDictionary; the call does not keep it.
    unsafe { AXIsProcessTrustedWithOptions((&*options as *const NSDictionary<NSString, NSNumber>).cast()) }
}

/// Releases a Core Foundation object when dropped.
struct Owned(CFTypeRef);

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: an object this module got with a Create or Copy call.
            unsafe { CFRelease(self.0) };
        }
    }
}

fn attribute(element: AXUIElementRef, name: &str, kind: u32, out: *mut c_void) -> bool {
    let name = NSString::from_str(name);
    let mut value: CFTypeRef = std::ptr::null();
    // SAFETY: a valid element and attribute name (NSString is a CFString).
    let error = unsafe { AXUIElementCopyAttributeValue(element, (&*name as *const NSString).cast(), &mut value) };
    let value = Owned(value);
    // SAFETY: `out` points to a value of the type `kind` names.
    error == 0 && !value.0.is_null() && unsafe { AXValueGetValue(value.0, kind, out) }
}

/// The frame (global, top-left origin, in points) of the element at `(x, y)`
/// in app `pid`. Asking the app (not the whole system) skips PixlSnapz's own
/// overlay, which is on top. `None` without permission or for elements that
/// fill most of the window anyway.
pub fn element_at(pid: i32, x: f64, y: f64, window: &WindowFrame) -> Option<WindowFrame> {
    if pid <= 0 {
        return None;
    }
    // SAFETY: plain Accessibility calls; every object is released by `Owned`.
    let app = Owned(unsafe { AXUIElementCreateApplication(pid) });
    // A slow or hung app must not stall measuring.
    // SAFETY: as above.
    unsafe { AXUIElementSetMessagingTimeout(app.0, 0.1) };
    let mut element: AXUIElementRef = std::ptr::null();
    // SAFETY: as above.
    let error = unsafe { AXUIElementCopyElementAtPosition(app.0, x as f32, y as f32, &mut element) };
    let element = Owned(element);
    if error != 0 || element.0.is_null() {
        return None;
    }
    let mut origin = CGPoint::default();
    let mut size = CGSize::default();
    if !attribute(element.0, "AXPosition", AX_VALUE_CG_POINT, (&mut origin as *mut CGPoint).cast())
        || !attribute(element.0, "AXSize", AX_VALUE_CG_SIZE, (&mut size as *mut CGSize).cast())
        || size.width < 2.0
        || size.height < 2.0
    {
        return None;
    }
    // The window itself or a pane filling it: the window frame says as much.
    if size.width * size.height >= 0.9 * window.width * window.height {
        return None;
    }
    Some(WindowFrame { x: origin.x, y: origin.y, width: size.width, height: size.height, pid })
}
