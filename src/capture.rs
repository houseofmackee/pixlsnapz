//! Screenshots of every display through ScreenCaptureKit, taken right before
//! the overlay appears, so edges are found in exactly what is on screen.
//!
//! ScreenCaptureKit reports results on its own dispatch queues, so waiting
//! for them on the main thread cannot deadlock. A capture takes tens of
//! milliseconds per display.

use std::ptr::NonNull;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use block2::RcBlock;
use objc2::AllocAnyThread;
use objc2::rc::Retained;
use objc2_core_foundation::{CFRetained, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGDisplayScreenSize, CGImage, CGImageAlphaInfo,
    CGImageByteOrderInfo, CGPreflightScreenCaptureAccess, CGRequestScreenCaptureAccess, kCGColorSpaceSRGB,
};
use objc2_foundation::{NSArray, NSError};
use objc2_screen_capture_kit::{
    SCContentFilter, SCDisplay, SCRunningApplication, SCScreenshotManager, SCShareableContent, SCStreamConfiguration,
};

use crate::measure::Frame;

const TIMEOUT: Duration = Duration::from_secs(5);

/// Moves a value into a completion handler's thread. The wrapped
/// Objective-C objects are immutable snapshots, safe to hand over.
struct AssertSend<T>(T);
// SAFETY: see the type's documentation.
unsafe impl<T> Send for AssertSend<T> {}

/// One display's screenshot.
pub struct Shot {
    pub display_id: u32,
    pub frame: Frame,
    /// The same pixels as an image, for the magnifier.
    pub image: CFRetained<CGImage>,
    /// The display's physical width in millimeters (0 when unknown).
    pub width_mm: f64,
}

/// Whether the app may record the screen. Asks macOS for permission (and
/// adds the app to the list in System Settings) when `prompt` is set.
pub fn has_permission(prompt: bool) -> bool {
    CGPreflightScreenCaptureAccess() || (prompt && CGRequestScreenCaptureAccess())
}

fn error_message(error: *mut NSError) -> String {
    // SAFETY: ScreenCaptureKit passes either nil or a valid NSError.
    match unsafe { error.as_ref() } {
        Some(error) => error.localizedDescription().to_string(),
        None => "unknown ScreenCaptureKit error".to_string(),
    }
}

fn shareable_content() -> Result<Retained<SCShareableContent>> {
    let (tx, rx) = mpsc::sync_channel(1);
    let handler = RcBlock::new(move |content: *mut SCShareableContent, error: *mut NSError| {
        // SAFETY: on success `content` is a valid object; retain it because
        // the handler only borrows it.
        let result = match unsafe { Retained::retain(content) } {
            Some(content) => Ok(content),
            None => Err(error_message(error)),
        };
        let _ = tx.send(AssertSend(result));
    });
    // SAFETY: the handler has the expected signature.
    unsafe {
        SCShareableContent::getShareableContentExcludingDesktopWindows_onScreenWindowsOnly_completionHandler(
            false, true, &handler,
        );
    }
    let result = rx.recv_timeout(TIMEOUT).map_err(|_| anyhow!("ScreenCaptureKit did not respond in time"))?.0;
    result.map_err(|e| anyhow!(e))
}

/// Captures every display. Displays that fail are logged and left out.
pub fn capture_all() -> Result<Vec<Shot>> {
    let content = shareable_content()?;
    // Leave out PixlSnapz's own windows, such as measurements kept on screen.
    let own_pid = std::process::id() as i32;
    // SAFETY: plain property reads.
    let own: Vec<Retained<SCRunningApplication>> =
        unsafe { content.applications().iter().filter(|a| a.processID() == own_pid).collect() };
    let own = NSArray::from_retained_slice(&own);
    // SAFETY: plain property read.
    let displays = unsafe { content.displays() };
    let mut shots = Vec::new();
    for display in displays.iter() {
        match capture_display(&display, &own) {
            Ok(shot) => shots.push(shot),
            Err(e) => {
                // SAFETY: plain property read.
                let id = unsafe { display.displayID() };
                tracing::warn!("capturing display {id}: {e:#}");
            }
        }
    }
    if shots.is_empty() {
        bail!("no display could be captured");
    }
    Ok(shots)
}

fn capture_display(display: &SCDisplay, excluded: &NSArray<SCRunningApplication>) -> Result<Shot> {
    // SAFETY: a valid display and exclusion list; plain property reads.
    let (filter, width, height, display_id) = unsafe {
        let filter = SCContentFilter::initWithDisplay_excludingApplications_exceptingWindows(
            SCContentFilter::alloc(),
            display,
            excluded,
            &NSArray::new(),
        );
        let scale = f64::from(filter.pointPixelScale());
        let width = (display.width() as f64 * scale).round() as usize;
        let height = (display.height() as f64 * scale).round() as usize;
        (filter, width, height, display.displayID())
    };
    // SAFETY: setting plain configuration properties.
    let config = unsafe {
        let config = SCStreamConfiguration::new();
        config.setWidth(width);
        config.setHeight(height);
        config.setShowsCursor(false);
        config.setScalesToFit(false);
        config
    };
    let (tx, rx) = mpsc::sync_channel(1);
    let handler = RcBlock::new(move |image: *mut CGImage, error: *mut NSError| {
        let result = match NonNull::new(image) {
            // SAFETY: a valid image that the handler only borrows; retain it.
            Some(image) => Ok(unsafe { CFRetained::retain(image) }),
            None => Err(error_message(error)),
        };
        let _ = tx.send(AssertSend(result));
    });
    // SAFETY: valid filter and configuration; the handler has the expected signature.
    unsafe {
        SCScreenshotManager::captureImageWithFilter_configuration_completionHandler(&filter, &config, Some(&handler));
    }
    let image = rx.recv_timeout(TIMEOUT).map_err(|_| anyhow!("ScreenCaptureKit did not respond in time"))?.0;
    let image = image.map_err(|e| anyhow!(e))?;
    let frame = to_frame(&image)?;
    let width_mm = CGDisplayScreenSize(display_id).width;
    Ok(Shot { display_id, frame, image, width_mm })
}

/// A screenshot made from `frame`, as if display `display_id` had shown it
/// (for tests).
pub fn shot_from_frame(display_id: u32, frame: Frame) -> Option<Shot> {
    let mut pixels = frame.pixels.clone();
    // SAFETY: a static constant.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: `pixels` holds `height` rows of `width * 4` bytes and outlives
    // the context; the image copies them.
    let context = unsafe {
        CGBitmapContextCreate(
            pixels.as_mut_ptr().cast(),
            frame.width,
            frame.height,
            8,
            frame.width * 4,
            Some(&space),
            info,
        )
    }?;
    let image = objc2_core_graphics::CGBitmapContextCreateImage(Some(&context))?;
    Some(Shot { display_id, frame, image, width_mm: 0.0 })
}

/// Draws `image` into an sRGB RGBA8 buffer.
pub fn to_frame(image: &CGImage) -> Result<Frame> {
    let width = CGImage::width(Some(image));
    let height = CGImage::height(Some(image));
    if width == 0 || height == 0 {
        bail!("empty image");
    }
    let mut pixels = vec![0u8; width * height * 4];
    // SAFETY: a static constant.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB })).ok_or_else(|| anyhow!("no sRGB"))?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: `pixels` holds `height` rows of `width * 4` bytes and outlives
    // the context, which is dropped at the end of this function.
    let context =
        unsafe { CGBitmapContextCreate(pixels.as_mut_ptr().cast(), width, height, 8, width * 4, Some(&space), info) }
            .ok_or_else(|| anyhow!("could not create a bitmap context"))?;
    let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as f64, height as f64));
    CGContext::draw_image(Some(&context), rect, Some(image));
    drop(context);
    Ok(Frame::new(width, height, pixels))
}
