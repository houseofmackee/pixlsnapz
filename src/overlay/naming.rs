//! ⇧S: naming a set in a text field on the overlay itself (a separate
//! window would not reliably get the keyboard, as PixlSnapz is not the
//! active app). Being a real text field, it takes paste, input methods, and
//! the usual editing keys; Return saves and Esc cancels.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::{NSObjectProtocol, ProtocolObject, Sel};
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSControl, NSControlTextEditingDelegate, NSFont, NSTextAlignment, NSTextField, NSTextFieldDelegate, NSTextView,
};
use objc2_foundation::{NSObject, NSPoint, NSRect, NSSize, NSString};

use super::*;

const WIDTH: f64 = 300.0;
const HEIGHT: f64 = 30.0;

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PixlSnapzNamingDelegate"]
    struct NamingDelegate;

    unsafe impl NSObjectProtocol for NamingDelegate {}

    unsafe impl NSControlTextEditingDelegate for NamingDelegate {
        #[unsafe(method(control:textView:doCommandBySelector:))]
        fn do_command(&self, _control: &NSControl, _view: &NSTextView, command: Sel) -> bool {
            if command == sel!(insertNewline:) {
                commit();
                true
            } else if command == sel!(cancelOperation:) {
                cancel();
                true
            } else {
                false
            }
        }
    }

    unsafe impl NSTextFieldDelegate for NamingDelegate {}
);

thread_local! {
    static FIELD: RefCell<Option<(Retained<NSTextField>, Retained<NamingDelegate>)>> = const { RefCell::new(None) };
}

/// The editing action for a key equivalent in the field: cut, copy, paste,
/// select all, undo, or redo.
pub(super) fn edit_action(event: &objc2_app_kit::NSEvent) -> Option<Sel> {
    use objc2_app_kit::NSEventModifierFlags as Flags;
    let flags = event.modifierFlags() & (Flags::Command | Flags::Shift | Flags::Option | Flags::Control);
    let key = event.charactersIgnoringModifiers()?.to_string().to_lowercase();
    match (flags, key.as_str()) {
        (f, "x") if f == Flags::Command => Some(sel!(cut:)),
        (f, "c") if f == Flags::Command => Some(sel!(copy:)),
        (f, "v") if f == Flags::Command => Some(sel!(paste:)),
        (f, "a") if f == Flags::Command => Some(sel!(selectAll:)),
        (f, "z") if f == Flags::Command => Some(sel!(undo:)),
        (f, "z") if f == Flags::Command | Flags::Shift => Some(sel!(redo:)),
        _ => None,
    }
}

/// Shows the field with `suggestion` selected, in the active overlay.
pub(super) fn start(suggestion: &str) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    if FIELD.with(|f| f.borrow().is_some()) {
        return;
    }
    let Some((window, view, size)) = SESSION.with(|s| {
        s.borrow().as_ref().map(|s| {
            let o = &s.overlays[s.active];
            (o.window.clone(), o.view.clone(), o.size)
        })
    }) else {
        return;
    };
    let field = NSTextField::textFieldWithString(&NSString::from_str(suggestion), mtm);
    field.setFont(Some(&NSFont::systemFontOfSize(17.0)));
    field.setAlignment(NSTextAlignment::Center);
    field.setFrame(NSRect::new(
        NSPoint::new((size.width - WIDTH) / 2.0, (size.height - HEIGHT) / 2.0),
        NSSize::new(WIDTH, HEIGHT),
    ));
    field.setAccessibilityLabel(Some(&NSString::from_str(&t!("Save these measurements and guides as"))));
    // SAFETY: a plain NSObject subclass.
    let delegate: Retained<NamingDelegate> = unsafe { msg_send![NamingDelegate::alloc(mtm), init] };
    // SAFETY: the module keeps the delegate as long as the field.
    unsafe { field.setDelegate(Some(ProtocolObject::from_ref(&*delegate))) };
    view.addSubview(&field);
    window.makeFirstResponder(Some(&field));
    // SAFETY: selecting the field's own text.
    unsafe { field.selectText(None) };
    FIELD.with(|f| *f.borrow_mut() = Some((field, delegate)));
    update(|s| s.naming = true);
}

/// Removes the field and gives the keyboard back to the overlay. The name
/// typed, if there was a field.
fn finish() -> Option<String> {
    let (field, _delegate) = FIELD.with(|f| f.borrow_mut().take())?;
    let name = field.stringValue().to_string();
    field.removeFromSuperview();
    let key = SESSION.with(|s| {
        s.borrow_mut().as_mut().map(|s| {
            s.naming = false;
            (s.overlays[s.active].window.clone(), s.overlays[s.active].view.clone())
        })
    });
    if let Some((window, view)) = key {
        window.makeFirstResponder(Some(&view));
    }
    redraw_all();
    Some(name)
}

/// Esc, or measuring ended: no set is saved.
pub(super) fn cancel() {
    finish();
}

/// Return: saves the set under the name typed.
fn commit() {
    let Some(name) = finish() else { return };
    actions::save_set_as(name.trim());
}
