//! Keyboard commands while measuring.

use super::*;

use objc2_app_kit::{NSEvent, NSEventModifierFlags};
use objc2_foundation::NSPoint;

use crate::i18n::tr;
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
            update(Session::remove_item);
            return;
        }
        // ⌘ arrows move the design overlay.
        KEY_LEFT | KEY_RIGHT | KEY_UP | KEY_DOWN if command => {
            let step = if shift { 10.0 } else { 1.0 };
            let (dx, dy) = match code {
                KEY_LEFT => (-step, 0.0),
                KEY_RIGHT => (step, 0.0),
                KEY_UP => (0.0, -step),
                _ => (0.0, step),
            };
            update(|s| {
                if s.settings.overlay_visible && s.overlay_image.is_some() {
                    s.settings.overlay_x += dx;
                    s.settings.overlay_y += dy;
                    s.settings_dirty = true;
                }
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
        (true, "e") => send_to_cleanshotz(),
        (true, "i") => update(|s| {
            if let Some((x, y)) = s.hover_px() {
                (s.settings.overlay_x, s.settings.overlay_y) = (x as f64, y as f64);
                s.save_settings();
            }
        }),
        (false, "i") if option => choose_overlay_image(),
        (false, "i") if shift => update(|s| {
            s.settings.step_overlay_look();
            let look = if s.settings.overlay_difference {
                t!("difference")
            } else {
                format!("{}%", (s.settings.overlay_opacity * 100.0).round())
            };
            s.toast(t!("Design overlay: {look}", look));
            s.save_settings();
        }),
        (false, "i") => {
            let has_image = SESSION.with(|s| s.borrow().as_ref().is_some_and(|s| !s.settings.overlay_image.is_empty()));
            if has_image {
                update(|s| {
                    if s.overlay_image.is_none() {
                        s.overlay_image = load_overlay_image(&s.settings.overlay_image);
                    }
                    s.settings.overlay_visible = !s.settings.overlay_visible;
                    let on = s.settings.overlay_visible;
                    s.toast(if on { t!("Design overlay: on") } else { t!("Design overlay: off") });
                    s.save_settings();
                });
            } else {
                choose_overlay_image();
            }
        }
        (false, "t") => update(|s| {
            s.settings.size_format = s.settings.size_format.next();
            let name = tr(s.settings.size_format.name());
            s.toast(t!("Size format: {name}", name));
            s.save_settings();
        }),
        (false, "v") => update(|s| s.add_guide(true)),
        (false, "h") => update(|s| s.add_guide(false)),
        (false, "g") => update(|s| {
            s.items.retain(|(_, item)| !item.is_guide());
            s.toast(t!("Guides cleared"));
        }),
        (false, "c") => update(|s| {
            s.items.clear();
            s.toast(t!("Cleared"));
        }),
        (false, "r") => update(|s| {
            let n = s.restore();
            s.toast(if n == 0 { t!("Nothing to restore") } else { t!("Restored {n} items", n) });
        }),
        (false, "p") if shift => update(|s| {
            s.settings.color_format = s.settings.color_format.next();
            let name = s.settings.color_format.name();
            s.toast(t!("Color format: {name}", name));
            s.save_settings();
        }),
        (false, "p") if option => copy_average_color(),
        (false, "p") => copy_color(),
        (false, "l") => update(|s| {
            s.tool = if s.tool == Tool::Line { Tool::Area } else { Tool::Line };
            s.toast(if s.tool == Tool::Line {
                t!("Line tool: drag to measure length and angle")
            } else {
                t!("Area tool")
            });
        }),
        (false, "e") => update(|s| {
            s.tool = if s.tool == Tool::Circle { Tool::Area } else { Tool::Circle };
            s.toast(if s.tool == Tool::Circle { t!("Circle tool: drag from the center") } else { t!("Area tool") });
        }),
        (false, "z") if shift => update(|s| {
            s.settings.vision = s.settings.vision.next();
            s.settings.loupe = true;
            let name = s.settings.vision.name();
            s.toast(t!("Loupe: {name}", name = tr(name)));
            s.save_settings();
        }),
        (false, "m") => toggle_setting(n!("Measurements in screenshots"), |st| &mut st.stamp_screenshots),
        (false, "n") => toggle_setting(n!("Smooth edges (photos and gradients)"), |st| &mut st.smooth_edges),
        (false, "d") => update(|s| {
            s.settings.step_design_zoom(!shift);
            let zoom = s.settings.design_zoom;
            for o in &mut s.overlays {
                o.measure.zoom = f64::from(zoom) / 100.0;
            }
            s.toast(t!("Design zoom: {zoom}%", zoom));
            s.save_settings();
        }),
        (false, "x") => update(|s| {
            s.settings.axes = s.settings.axes.next();
            let name = s.settings.axes.name();
            s.toast(t!("Measure: {name}", name = tr(name)));
            s.save_settings();
        }),
        (false, "-") | (false, "=") | (false, "+") => update(|s| {
            let zoom_in = chars != "-";
            if s.settings.loupe {
                s.settings.step_loupe_zoom(zoom_in);
            }
            s.settings.loupe = true;
            s.toast(t!("Loupe: {zoom}×", zoom = s.settings.loupe_zoom));
            s.save_settings();
        }),
        (false, "u") => update(|s| {
            s.settings.units = s.settings.units.next();
            let name = s.settings.units.name();
            s.toast(t!("Units: {name}", name = tr(name)));
            s.save_settings();
        }),
        (false, "k") => pick_contrast(),
        (false, "o") => update(|s| {
            s.settings.grid = s.settings.grid.next();
            let name = s.settings.grid.name();
            if s.settings.grid == Grid::Columns && !s.columns_fit() {
                s.toast(t!("Grid: {name} (the columns do not fit in the last area; see Settings)", name = tr(name)));
            } else {
                s.toast(t!("Grid: {name}", name = tr(name)));
            }
            s.save_settings();
        }),
        (false, "b") => keep_on_screen(),
        (false, "[") | (false, "]") => update(|s| {
            s.settings.step_tolerance(chars == "]");
            s.toast(t!("Edge tolerance: {name}", name = tr(&s.settings.tolerance_name())));
            s.save_settings();
        }),
        (false, "s") => toggle_setting(n!("Snap areas to content"), |st| &mut st.snap_areas),
        (false, "z") => toggle_setting(n!("Loupe"), |st| &mut st.loupe),
        (false, "f") => toggle_setting(n!("Freeze screen"), |st| &mut st.freeze),
        (false, "a") => toggle_setting(n!("Aspect ratio"), |st| &mut st.show_aspect_ratio),
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
        s.toast(if on { t!("{name}: on", name = tr(name)) } else { t!("{name}: off", name = tr(name)) });
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
