//! Keyboard commands while measuring.

use super::*;

use objc2_app_kit::{NSEvent, NSEventModifierFlags};
use objc2_foundation::NSPoint;

use crate::items::Item;
use crate::measure::Direction;
use crate::settings::{Grid, Settings};

pub(super) const KEY_RETURN: u16 = 36;
pub(super) const KEY_ENTER: u16 = 76;
pub(super) const KEY_ESCAPE: u16 = 53;
pub(super) const KEY_DELETE: u16 = 51;
pub(super) const KEY_FORWARD_DELETE: u16 = 117;
pub(super) const KEY_SPACE: u16 = 49;
pub(super) const KEY_LEFT: u16 = 123;
pub(super) const KEY_RIGHT: u16 = 124;
pub(super) const KEY_DOWN: u16 = 125;
pub(super) const KEY_UP: u16 = 126;

pub(super) fn key_down(event: &NSEvent) {
    let code = event.keyCode();
    let flags = event.modifierFlags();
    let command = flags.contains(NSEventModifierFlags::Command);
    let shift = flags.contains(NSEventModifierFlags::Shift);
    let option = flags.contains(NSEventModifierFlags::Option);
    let chars = event.charactersIgnoringModifiers().map(|c| c.to_string().to_lowercase()).unwrap_or_default();

    match code {
        KEY_ESCAPE => {
            let dragging = SESSION.with(|s| s.borrow().as_ref().is_some_and(|s| s.drag.is_some()));
            if dragging {
                update(|s| s.drag = None)
            } else {
                end()
            }
            return;
        }
        KEY_SPACE => {
            update(|s| s.moving = s.drag.is_some());
            return;
        }
        KEY_RETURN | KEY_ENTER => {
            if copy_size() {
                end();
            }
            return;
        }
        KEY_DELETE | KEY_FORWARD_DELETE => {
            update(|s| {
                s.items.pop();
            });
            return;
        }
        KEY_LEFT | KEY_RIGHT | KEY_UP | KEY_DOWN if option => {
            let dir = match code {
                KEY_LEFT => Direction::Left,
                KEY_RIGHT => Direction::Right,
                KEY_UP => Direction::Up,
                _ => Direction::Down,
            };
            update(|s| s.jump(dir));
            return;
        }
        KEY_LEFT | KEY_RIGHT | KEY_UP | KEY_DOWN => {
            update(|s| nudge(s, code, shift));
            return;
        }
        _ => {}
    }

    match (command, chars.as_str()) {
        (true, "c") if shift => screenshot(false, option),
        (true, "c") => {
            copy_size();
        }
        (true, "s") => screenshot(true, option),
        (true, "z") => update(|s| {
            s.items.pop();
        }),
        (true, "w") | (true, "q") | (true, ".") => end(),
        (false, "v") => update(|s| s.add_guide(true)),
        (false, "h") => update(|s| s.add_guide(false)),
        (false, "g") => update(|s| {
            s.items.retain(|(_, item)| !item.is_guide());
            s.toast("Guides cleared");
        }),
        (false, "c") => update(|s| {
            s.items.clear();
            s.toast("Cleared");
        }),
        (false, "r") => update(|s| {
            let n = s.restore();
            s.toast(if n == 0 { "Nothing to restore".to_string() } else { format!("Restored {n} items") });
        }),
        (false, "p") if shift => update(|s| {
            s.settings.color_format = s.settings.color_format.next();
            let name = s.settings.color_format.name();
            s.toast(format!("Color format: {name}"));
            s.save_settings();
        }),
        (false, "p") if option => copy_average_color(),
        (false, "p") => copy_color(),
        (false, "l") => update(|s| {
            s.line_tool = !s.line_tool;
            s.toast(if s.line_tool { "Line tool: drag to measure length and angle" } else { "Area tool" });
        }),
        (false, "x") => update(|s| {
            s.settings.axes = s.settings.axes.next();
            let name = s.settings.axes.name();
            s.toast(format!("Measure: {name}"));
            s.save_settings();
        }),
        (false, "-") | (false, "=") | (false, "+") => update(|s| {
            let zoom_in = chars != "-";
            if s.settings.loupe {
                s.settings.step_loupe_zoom(zoom_in);
            }
            s.settings.loupe = true;
            s.toast(format!("Loupe: {}×", s.settings.loupe_zoom));
            s.save_settings();
        }),
        (false, "u") => update(|s| {
            s.settings.units = s.settings.units.next();
            let name = s.settings.units.name();
            s.toast(format!("Units: {name}"));
            s.save_settings();
        }),
        (false, "k") => pick_contrast(),
        (false, "o") => update(|s| {
            s.settings.grid = s.settings.grid.next();
            let name = s.settings.grid.name();
            if s.settings.grid == Grid::Columns && !s.columns_fit() {
                s.toast(format!("Grid: {name} (the columns do not fit in the last area; see Settings)"));
            } else {
                s.toast(format!("Grid: {name}"));
            }
            s.save_settings();
        }),
        (false, "b") => keep_on_screen(),
        (false, "[") | (false, "]") => update(|s| {
            s.settings.step_tolerance(chars == "]");
            s.toast(format!("Edge tolerance: {}", s.settings.tolerance_name()));
            s.save_settings();
        }),
        (false, "s") => toggle_setting("Snap areas to content", |st| &mut st.snap_areas),
        (false, "z") => toggle_setting("Loupe", |st| &mut st.loupe),
        (false, "f") => toggle_setting("Freeze screen", |st| &mut st.freeze),
        (false, "a") => toggle_setting("Aspect ratio", |st| &mut st.show_aspect_ratio),
        (false, "/") | (false, "?") => update(|s| {
            s.settings.show_hints = !s.settings.show_hints;
            s.save_settings();
        }),
        _ => {}
    }
}

pub(super) fn toggle_setting(name: &str, field: impl Fn(&mut Settings) -> &mut bool) {
    update(|s| {
        let value = field(&mut s.settings);
        *value = !*value;
        let on = *value;
        s.toast(format!("{name}: {}", if on { "on" } else { "off" }));
        s.save_settings();
    });
}

/// Arrow keys: move the cursor by one pixel (ten points with Shift), or
/// the last guide when one was just added.
pub(super) fn nudge(s: &mut Session, code: u16, shift: bool) {
    let Some(o) = s.overlays.get(s.active) else { return };
    let step_pt = if shift { 10.0 } else { 1.0 / o.scale };
    let step_px = (step_pt * o.scale).round().max(1.0) as isize;
    let (dx, dy) = match code {
        KEY_LEFT => (-1.0, 0.0),
        KEY_RIGHT => (1.0, 0.0),
        KEY_UP => (0.0, -1.0),
        _ => (0.0, 1.0),
    };
    let (w, h) = (o.shot.frame.width as isize, o.shot.frame.height as isize);
    let (size, active) = (o.size, s.active);
    let moved_guide = match s.items.last_mut() {
        Some((i, Item::VerticalGuide { x })) if *i == active && dx != 0.0 => {
            *x = (*x as isize + dx as isize * step_px).clamp(0, w) as usize;
            true
        }
        Some((i, Item::HorizontalGuide { y })) if *i == active && dy != 0.0 => {
            *y = (*y as isize + dy as isize * step_px).clamp(0, h) as usize;
            true
        }
        _ => false,
    };
    if !moved_guide && let Some(p) = s.hover {
        let x = (p.x + dx * step_pt).clamp(0.0, size.width - 0.01);
        let y = (p.y + dy * step_pt).clamp(0.0, size.height - 0.01);
        s.hover = Some(NSPoint::new(x, y));
    }
}
