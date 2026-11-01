//! The loupe: a magnified, optionally color-blindness simulated view of the
//! pixels around the pointer, with the color and coordinates under it.

use super::*;

use objc2_app_kit::{NSBezierPath, NSColor, NSCompositingOperation, NSGraphicsContext, NSImageInterpolation};
use objc2_foundation::{NSPoint, NSRect, NSSize};

use crate::color::Vision;
use crate::measure::format_length;

pub(super) fn draw_loupe(s: &Session, o: &Overlay, x: usize, y: usize, cursor: NSPoint, rgb: [f64; 3]) {
    let zoom = f64::from(s.settings.loupe_zoom.max(1));
    // An odd number of pixels, so the one under the pointer is in the middle.
    let radius = ((LOUPE_SIZE / zoom / 2.0).floor() as usize).max(1);
    let n = radius * 2 + 1;
    let side = n as f64 * zoom;
    let mut origin = NSPoint::new(cursor.x + 24.0, cursor.y + 36.0);
    if origin.x + side > o.size.width - 4.0 {
        origin.x = cursor.x - 24.0 - side;
    }
    if origin.y + side + 30.0 > o.size.height - 4.0 {
        origin.y = cursor.y - 24.0 - side - 30.0;
    }
    let dest = NSRect::new(origin, NSSize::new(side, side));

    // Source rectangle in the image's points (NSImage: bottom-left origin).
    let src_x = x as f64 - radius as f64;
    let src_y = y as f64 - radius as f64;
    let src = NSRect::new(
        NSPoint::new(src_x / o.scale, o.size.height - (src_y + n as f64) / o.scale),
        NSSize::new(n as f64 / o.scale, n as f64 / o.scale),
    );

    let Some(ctx) = NSGraphicsContext::currentContext() else { return };
    ctx.saveGraphicsState();
    let clip = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(dest, 10.0, 10.0);
    NSColor::blackColor().setFill();
    clip.fill();
    clip.addClip();
    ctx.setImageInterpolation(NSImageInterpolation::None);
    let vision = s.settings.vision;
    if vision == Vision::Normal {
        // SAFETY: plain drawing; `respectFlipped` keeps the image upright in the flipped view.
        unsafe {
            o.image.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                dest,
                src,
                NSCompositingOperation::Copy,
                1.0,
                true,
                None,
            )
        };
    } else {
        // Each pixel as it looks with the simulated color vision.
        let f = &o.shot.frame;
        for j in 0..n {
            for i in 0..n {
                let (px, py) = (x as isize + i as isize - radius as isize, y as isize + j as isize - radius as isize);
                if px < 0 || py < 0 || px as usize >= f.width || py as usize >= f.height {
                    continue;
                }
                let [r, g, b] = vision.simulate(f.pixel(px as usize, py as usize));
                NSColor::colorWithSRGBRed_green_blue_alpha(
                    f64::from(r) / 255.0,
                    f64::from(g) / 255.0,
                    f64::from(b) / 255.0,
                    1.0,
                )
                .setFill();
                NSBezierPath::fillRect(NSRect::new(
                    NSPoint::new(origin.x + i as f64 * zoom, origin.y + j as f64 * zoom),
                    NSSize::new(zoom, zoom),
                ));
            }
        }
    }
    // Pixel grid, when the pixels are big enough to see it.
    let grid = NSBezierPath::bezierPath();
    grid.setLineWidth(lw(0.5));
    for i in (1..n).filter(|_| zoom >= 6.0) {
        let d = i as f64 * zoom;
        line(&grid, (origin.x + d, origin.y), (origin.x + d, origin.y + side));
        line(&grid, (origin.x, origin.y + d), (origin.x + side, origin.y + d));
    }
    NSColor::colorWithWhite_alpha(0.5, 0.25).setStroke();
    grid.stroke();
    // The pixel under the cursor.
    let at = radius as f64 * zoom;
    let mark = NSBezierPath::bezierPathWithRect(NSRect::new(
        NSPoint::new(origin.x + at, origin.y + at),
        NSSize::new(zoom, zoom),
    ));
    mark.setLineWidth(lw(1.5));
    color(rgb, 1.0).setStroke();
    mark.stroke();
    ctx.restoreGraphicsState();
    let border = NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(dest, 10.0, 10.0);
    border.setLineWidth(lw(1.0));
    NSColor::colorWithWhite_alpha(1.0, 0.75).setStroke();
    border.stroke();

    // The color in the chosen format, and where the pointer is.
    let at = |px: usize| format_length(px, o.scale, s.in_pixels());
    let color_text = s.settings.color_format.short(o.shot.frame.pixel(x, y));
    let mut text = format!("{color_text}   {}, {}", at(x), at(y));
    if s.settings.vision != Vision::Normal {
        let name = &crate::i18n::tr(s.settings.vision.name());
        text.push_str("   ");
        text.push_str(name.split(' ').next().unwrap_or(name));
    }
    pill(&text, NSPoint::new(origin.x, origin.y + side + 5.0), o.size, 11.0, &dark());
}
