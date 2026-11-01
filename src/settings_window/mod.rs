//! The Settings window: every setting in five tabs (General, Measuring,
//! Colors & Loupe, Grid, Screenshots), including a shortcut recorder. Each
//! change is saved right away.

use std::cell::RefCell;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibility, NSApplication, NSBackingStoreType, NSButton, NSControlStateValueOff, NSControlStateValueOn,
    NSPopUpButton, NSSlider, NSTabView, NSTabViewItem, NSTextAlignment, NSTextField, NSView, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{NSArray, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString};

use crate::color::{ColorFormat, Vision};
use crate::i18n::tr;
use crate::settings::{Axes, COLORS, DESIGN_ZOOMS, Grid, LOUPE_ZOOMS, Settings, Units};
use crate::units::SizeFormat;

mod pickers;
mod recorder;
use pickers::*;
use recorder::*;

const WIDTH: f64 = 580.0;
/// Fits the tallest tab (Measuring) on any screen.
const HEIGHT: f64 = 380.0;
const LABEL_WIDTH: f64 = 190.0;
const ROW: f64 = 30.0;
const KEY_ESCAPE: u16 = 53;
/// English tab titles (also what `select_tab` matches), translated when shown.
const TAB_TITLES: [&str; 5] = [n!("General"), n!("Measuring"), n!("Colors & Loupe"), n!("Grid"), n!("Screenshots")];
const AXES: [Axes; 3] = [Axes::Both, Axes::Horizontal, Axes::Vertical];

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "PixlSnapzSettingsTarget"]
    struct Target;

    impl Target {
        #[unsafe(method(changed:))]
        fn changed(&self, _sender: Option<&AnyObject>) {
            controls_changed();
        }

        #[unsafe(method(recordShortcut:))]
        fn record_shortcut(&self, _sender: Option<&AnyObject>) {
            start_recording();
        }

        #[unsafe(method(chooseFolder:))]
        fn choose_folder(&self, _sender: Option<&AnyObject>) {
            choose_folder();
        }

        #[unsafe(method(addApp:))]
        fn add_app(&self, _sender: Option<&AnyObject>) {
            add_disabled_app();
        }

        #[unsafe(method(clearApps:))]
        fn clear_apps(&self, _sender: Option<&AnyObject>) {
            let mut s = Settings::load();
            s.disabled_apps.clear();
            save_and_refresh(&s);
        }

        #[unsafe(method(openFile:))]
        fn open_file(&self, _sender: Option<&AnyObject>) {
            crate::app::open_settings_file();
        }
    }

    unsafe impl NSObjectProtocol for Target {}
);

struct Controls {
    shortcut: Retained<NSButton>,
    login: Retained<NSButton>,
    hints: Retained<NSButton>,
    units: Retained<NSPopUpButton>,
    show_units: Retained<NSButton>,
    design_zoom: Retained<NSPopUpButton>,
    ppi: Retained<NSTextField>,
    tolerance: Retained<NSSlider>,
    tolerance_value: Retained<NSTextField>,
    snap: Retained<NSButton>,
    axes: Retained<NSPopUpButton>,
    aspect: Retained<NSButton>,
    freeze: Retained<NSButton>,
    color: Retained<NSPopUpButton>,
    color_format: Retained<NSPopUpButton>,
    size_format: Retained<NSPopUpButton>,
    loupe: Retained<NSButton>,
    loupe_zoom: Retained<NSPopUpButton>,
    grid: Retained<NSPopUpButton>,
    grid_size: Retained<NSTextField>,
    grid_columns: Retained<NSTextField>,
    grid_gutter: Retained<NSTextField>,
    grid_margin: Retained<NSTextField>,
    folder: Retained<NSTextField>,
    disabled_apps: Retained<NSTextField>,
    vision: Retained<NSPopUpButton>,
    stamp: Retained<NSButton>,
    check_updates: Retained<NSButton>,
    smooth: Retained<NSButton>,
}

struct SettingsWindow {
    window: Retained<NSWindow>,
    tabs: Retained<NSTabView>,
    _target: Retained<Target>,
    c: Controls,
}

thread_local! {
    static WINDOW: RefCell<Option<SettingsWindow>> = const { RefCell::new(None) };
    /// The key monitor while a shortcut is being recorded.
    static MONITOR: RefCell<Option<Retained<AnyObject>>> = const { RefCell::new(None) };
}

fn ns(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> NSRect {
    NSRect::new(NSPoint::new(x, y), NSSize::new(w, h))
}

/// Lays out rows from the top of a (non-flipped) tab view.
struct Layout {
    view: Retained<NSView>,
    top: f64,
    mtm: MainThreadMarker,
}

impl Layout {
    fn y(&self) -> f64 {
        self.top - ROW
    }

    /// A label on the left and `control` (with its width) on the right.
    fn row(&mut self, title: &str, control: &NSView, width: f64) {
        let title = tr(title);
        let label = NSTextField::labelWithString(&ns(&title), self.mtm);
        label.setAlignment(NSTextAlignment::Right);
        label.setFrame(rect(20.0, self.y() + 5.0, LABEL_WIDTH, 18.0));
        self.view.addSubview(&label);
        let h = control.fittingSize().height.clamp(18.0, 26.0);
        let width = self.fit_width(control, width);
        control.setFrame(rect(CONTROL_X, self.y() + (ROW - h) / 2.0, width, h));
        // VoiceOver reads the row's title for the control next to it.
        control.setAccessibilityLabel(Some(&ns(title.trim_end_matches(':').trim_end())));
        self.view.addSubview(control);
        self.top -= ROW;
    }

    /// A control with no label (checkboxes, which carry their own).
    fn plain(&mut self, control: &NSView, width: f64) {
        let h = control.fittingSize().height.clamp(18.0, 26.0);
        let width = self.fit_width(control, width);
        control.setFrame(rect(CONTROL_X, self.y() + (ROW - h) / 2.0, width, h));
        self.view.addSubview(control);
        self.top -= ROW - 6.0;
    }

    /// Buttons side by side in the control column, each as wide as its title.
    fn buttons(&mut self, buttons: &[&NSButton]) {
        let mut x = CONTROL_X - 6.0;
        for b in buttons {
            let w = b.fittingSize().width.max(70.0);
            b.setFrame(rect(x, self.y(), w, ROW));
            self.view.addSubview(b);
            x += w + 4.0;
        }
        self.top -= ROW;
    }

    /// At least `width`, wide enough for the control's text (which may be a
    /// longer translation), but not past the edge of the tab.
    fn fit_width(&self, control: &NSView, width: f64) -> f64 {
        let available = self.view.frame().size.width - CONTROL_X - 12.0;
        width.max(control.fittingSize().width).min(available)
    }
}

/// Where controls start, right of the row titles.
const CONTROL_X: f64 = LABEL_WIDTH + 30.0;

fn popup(titles: &[&str], target: &Target, mtm: MainThreadMarker) -> Retained<NSPopUpButton> {
    let p = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), rect(0.0, 0.0, 200.0, 26.0), false);
    let titles: Vec<Retained<NSString>> = titles.iter().map(|t| ns(&tr(t))).collect();
    p.addItemsWithTitles(&NSArray::from_retained_slice(&titles));
    // SAFETY: the target outlives the window; the action exists on it.
    unsafe {
        p.setTarget(Some(target));
        p.setAction(Some(sel!(changed:)));
    }
    p
}

fn checkbox(title: &str, target: &Target, mtm: MainThreadMarker) -> Retained<NSButton> {
    // SAFETY: the target outlives the window; the action exists on it.
    unsafe { NSButton::checkboxWithTitle_target_action(&ns(&tr(title)), Some(target), Some(sel!(changed:)), mtm) }
}

fn button(title: &str, action: objc2::runtime::Sel, target: &Target, mtm: MainThreadMarker) -> Retained<NSButton> {
    // SAFETY: the target outlives the window; the action exists on it.
    unsafe { NSButton::buttonWithTitle_target_action(&ns(&tr(title)), Some(target), Some(action), mtm) }
}

fn number_field(target: &Target, mtm: MainThreadMarker) -> Retained<NSTextField> {
    let f = NSTextField::textFieldWithString(&ns(""), mtm);
    f.setAlignment(NSTextAlignment::Right);
    // SAFETY: the target outlives the window; the action exists on it.
    unsafe {
        f.setTarget(Some(target));
        f.setAction(Some(sel!(changed:)));
    }
    if let Some(cell) = f.cell() {
        cell.setSendsActionOnEndEditing(true);
    }
    f
}

fn set_check(b: &NSButton, on: bool) {
    b.setState(if on { NSControlStateValueOn } else { NSControlStateValueOff });
}

fn is_on(b: &NSButton) -> bool {
    b.state() == NSControlStateValueOn
}

fn number_text(v: f64) -> String {
    if v.fract() == 0.0 { format!("{}", v as i64) } else { format!("{v}") }
}

/// Shows the Settings window, creating it the first time.
pub fn show(mtm: MainThreadMarker) {
    let exists = WINDOW.with(|w| w.borrow().is_some());
    if !exists {
        let built = build(mtm);
        WINDOW.with(|w| *w.borrow_mut() = Some(built));
    }
    refresh();
    let window = WINDOW.with(|w| w.borrow().as_ref().map(|w| w.window.clone()));
    if let Some(window) = window {
        let app = NSApplication::sharedApplication(mtm);
        app.activate();
        window.makeKeyAndOrderFront(None);
        // In front even when macOS does not activate the app (a script asked).
        window.orderFrontRegardless();
    }
}

fn build(mtm: MainThreadMarker) -> SettingsWindow {
    let target: Retained<Target> = unsafe { msg_send![Target::alloc(mtm), init] };
    let t = &*target;
    // SAFETY: NSWindow's designated initializer.
    let window: Retained<NSWindow> = unsafe {
        msg_send![NSWindow::alloc(mtm), initWithContentRect: rect(0.0, 0.0, WIDTH, HEIGHT),
            styleMask: NSWindowStyleMask::Titled | NSWindowStyleMask::Closable,
            backing: NSBackingStoreType::Buffered, defer: false]
    };
    // SAFETY: the module keeps the window.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&ns(&t!("PixlSnapz Settings")));
    let tabs = NSTabView::initWithFrame(NSTabView::alloc(mtm), rect(12.0, 10.0, WIDTH - 24.0, HEIGHT - 16.0));
    let content = tabs.contentRect().size;
    // A view for each tab, with rows laid out from its top.
    let page = |title: &str| {
        let view = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, content.width, content.height));
        let item = NSTabViewItem::new();
        item.setLabel(&ns(&tr(title)));
        item.setView(Some(&view));
        tabs.addTabViewItem(&item);
        Layout { view, top: content.height - 14.0, mtm }
    };

    let units: Vec<&str> = Units::ALL.iter().map(|u| u.name()).collect();
    let zooms: Vec<String> = DESIGN_ZOOMS.iter().map(|z| format!("{z}%")).collect();
    let zooms: Vec<&str> = zooms.iter().map(String::as_str).collect();
    let axes: Vec<&str> = AXES.iter().map(|a| a.name()).collect();
    let colors: Vec<&str> = COLORS.iter().map(|c| c.0).collect();
    let formats: Vec<&str> = ColorFormat::ALL.iter().map(|f| f.name()).collect();
    let loupe_zooms: Vec<String> = LOUPE_ZOOMS.iter().map(|z| format!("{z}×")).collect();
    let loupe_zooms: Vec<&str> = loupe_zooms.iter().map(String::as_str).collect();
    let grids: Vec<&str> = Grid::ALL.iter().map(|g| g.name()).collect();
    let visions: Vec<&str> = Vision::ALL.iter().map(|v| v.name()).collect();

    let c = Controls {
        shortcut: button("", sel!(recordShortcut:), t, mtm),
        login: checkbox("Launch at login", t, mtm),
        hints: checkbox("Show shortcut hints while measuring", t, mtm),
        units: popup(&units, t, mtm),
        show_units: checkbox("Show units (pt, px, mm, in)", t, mtm),
        design_zoom: popup(&zooms, t, mtm),
        ppi: number_field(t, mtm),
        // SAFETY: the target outlives the window; the action exists on it.
        tolerance: unsafe {
            NSSlider::sliderWithValue_minValue_maxValue_target_action(
                8.0,
                0.0,
                128.0,
                Some(t),
                Some(sel!(changed:)),
                mtm,
            )
        },
        tolerance_value: NSTextField::labelWithString(&ns(""), mtm),
        snap: checkbox("Snap areas to their content", t, mtm),
        axes: popup(&axes, t, mtm),
        aspect: checkbox("Show the aspect ratio of areas", t, mtm),
        freeze: checkbox("Freeze the screen while measuring", t, mtm),
        color: popup(&colors, t, mtm),
        color_format: popup(&formats, t, mtm),
        size_format: popup(&SizeFormat::ALL.iter().map(|f| f.name()).collect::<Vec<_>>(), t, mtm),
        loupe: checkbox("Show the loupe", t, mtm),
        loupe_zoom: popup(&loupe_zooms, t, mtm),
        grid: popup(&grids, t, mtm),
        grid_size: number_field(t, mtm),
        grid_columns: number_field(t, mtm),
        grid_gutter: number_field(t, mtm),
        grid_margin: number_field(t, mtm),
        folder: NSTextField::labelWithString(&ns(""), mtm),
        disabled_apps: NSTextField::labelWithString(&ns(""), mtm),
        vision: popup(&visions, t, mtm),
        stamp: checkbox("Draw the measurements into screenshots", t, mtm),
        check_updates: checkbox("Check for updates automatically", t, mtm),
        smooth: checkbox("Smooth edges for photos and gradients (N)", t, mtm),
    };
    c.ppi.setPlaceholderString(Some(&ns(&t!("Automatic"))));
    // The slider describes its own value (below); the label repeats it.
    c.tolerance_value.setAccessibilityElement(false);
    c.folder.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByTruncatingMiddle);
    c.disabled_apps.setLineBreakMode(objc2_app_kit::NSLineBreakMode::ByTruncatingTail);

    let mut l = page(TAB_TITLES[0]);
    l.row("Measure shortcut:", &c.shortcut, 220.0);
    l.plain(&c.login, 280.0);
    l.plain(&c.hints, 280.0);
    l.plain(&c.check_updates, 280.0);
    l.row("Shortcut off in:", &c.disabled_apps, 300.0);
    let add = button("Add App…", sel!(addApp:), t, mtm);
    let clear = button("Clear", sel!(clearApps:), t, mtm);
    l.buttons(&[&add, &clear]);
    let open = button("Open Settings File", sel!(openFile:), t, mtm);
    let open_width = open.fittingSize().width.max(170.0);
    open.setFrame(rect(content.width - open_width - 14.0, 14.0, open_width, 30.0));
    l.view.addSubview(&open);

    let mut l = page(TAB_TITLES[1]);
    l.row("Units:", &c.units, 160.0);
    l.plain(&c.show_units, 280.0);
    l.row("Design zoom:", &c.design_zoom, 160.0);
    l.row("Screen PPI (mm, in):", &c.ppi, 100.0);
    l.row("Edge tolerance:", &c.tolerance, 200.0);
    c.tolerance_value.setFrame(rect(LABEL_WIDTH + 240.0, l.top + 6.0, content.width - LABEL_WIDTH - 252.0, 18.0));
    l.view.addSubview(&c.tolerance_value);
    l.plain(&c.snap, 280.0);
    l.plain(&c.smooth, 300.0);
    l.row("Measure:", &c.axes, 160.0);
    l.plain(&c.aspect, 280.0);
    l.plain(&c.freeze, 280.0);

    let mut l = page(TAB_TITLES[2]);
    l.row("Guide color:", &c.color, 160.0);
    l.row("Copy colors as:", &c.color_format, 160.0);
    l.row("Copy sizes as:", &c.size_format, 160.0);
    l.plain(&c.loupe, 280.0);
    l.row("Loupe zoom:", &c.loupe_zoom, 100.0);
    l.row("Loupe vision:", &c.vision, 230.0);

    let mut l = page(TAB_TITLES[3]);
    l.row("Grid (O while measuring):", &c.grid, 160.0);
    l.row("Square size (pt):", &c.grid_size, 80.0);
    l.row("Columns:", &c.grid_columns, 80.0);
    l.row("Gutter (pt):", &c.grid_gutter, 80.0);
    l.row("Margin (pt):", &c.grid_margin, 80.0);

    let mut l = page(TAB_TITLES[4]);
    l.row("Save to:", &c.folder, 300.0);
    let choose = button("Choose…", sel!(chooseFolder:), t, mtm);
    l.buttons(&[&choose]);
    l.plain(&c.stamp, 300.0);

    let root = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, WIDTH, HEIGHT));
    root.addSubview(&tabs);
    window.setContentView(Some(&root));
    window.center();
    SettingsWindow { window, tabs, _target: target, c }
}

/// Shows the tab whose title starts with `name` (ignoring case).
pub fn select_tab(name: &str) {
    let name = name.to_lowercase();
    WINDOW.with(|w| {
        let w = w.borrow();
        let Some(w) = w.as_ref() else { return };
        let index = TAB_TITLES.iter().position(|t| t.to_lowercase().starts_with(&name));
        if let Some(i) = index {
            w.tabs.selectTabViewItemAtIndex(i as isize);
        }
    });
}

/// Shows the current settings in the window (if it is open).
pub fn refresh() {
    let s = Settings::load();
    let login = crate::login_item::is_enabled();
    let recording = MONITOR.with(|m| m.borrow().is_some());
    WINDOW.with(|w| {
        let w = w.borrow();
        let Some(w) = w.as_ref() else { return };
        let c = &w.c;
        if !recording {
            c.shortcut.setTitle(&ns(&s.shortcut().to_string()));
        }
        set_check(&c.login, login);
        set_check(&c.hints, s.show_hints);
        c.units.selectItemAtIndex(Units::ALL.iter().position(|u| *u == s.units).unwrap_or(0) as isize);
        set_check(&c.show_units, s.show_units);
        let zoom = DESIGN_ZOOMS.iter().position(|z| *z == s.design_zoom).unwrap_or(2);
        c.design_zoom.selectItemAtIndex(zoom as isize);
        c.ppi.setStringValue(&ns(&if s.screen_ppi > 0.0 { number_text(s.screen_ppi) } else { String::new() }));
        c.tolerance.setDoubleValue(f64::from(s.tolerance));
        c.tolerance_value.setStringValue(&ns(&tr(&s.tolerance_name())));
        c.tolerance.setAccessibilityValueDescription(Some(&ns(&format!(
            "{} ({})",
            tr(&s.tolerance_name()),
            s.tolerance
        ))));
        set_check(&c.snap, s.snap_areas);
        c.axes.selectItemAtIndex(AXES.iter().position(|a| *a == s.axes).unwrap_or(0) as isize);
        set_check(&c.aspect, s.show_aspect_ratio);
        set_check(&c.freeze, s.freeze);
        let color = COLORS.iter().position(|(_, hex)| hex.eq_ignore_ascii_case(&s.color));
        c.color.selectItemAtIndex(color.map_or(-1, |i| i as isize));
        let size_format = SizeFormat::ALL.iter().position(|f| *f == s.size_format).unwrap_or(0);
        c.size_format.selectItemAtIndex(size_format as isize);
        let format = ColorFormat::ALL.iter().position(|f| *f == s.color_format).unwrap_or(0);
        c.color_format.selectItemAtIndex(format as isize);
        set_check(&c.loupe, s.loupe);
        let lz = LOUPE_ZOOMS.iter().position(|z| *z == s.loupe_zoom).unwrap_or(2);
        c.loupe_zoom.selectItemAtIndex(lz as isize);
        c.grid.selectItemAtIndex(Grid::ALL.iter().position(|g| *g == s.grid).unwrap_or(0) as isize);
        c.grid_size.setStringValue(&ns(&number_text(s.grid_size)));
        c.grid_columns.setStringValue(&ns(&s.grid_columns.to_string()));
        c.grid_gutter.setStringValue(&ns(&number_text(s.grid_gutter)));
        c.grid_margin.setStringValue(&ns(&number_text(s.grid_margin)));
        c.folder.setStringValue(&ns(&s.screenshot_dir().display().to_string()));
        let apps: Vec<String> = s.disabled_apps.iter().map(|id| app_name(id)).collect();
        let apps = if apps.is_empty() { t!("No apps") } else { apps.join(", ") };
        c.disabled_apps.setStringValue(&ns(&apps));
        c.disabled_apps.setToolTip(Some(&ns(&s.disabled_apps.join("\n"))));
        c.vision.selectItemAtIndex(Vision::ALL.iter().position(|v| *v == s.vision).unwrap_or(0) as isize);
        set_check(&c.stamp, s.stamp_screenshots);
        set_check(&c.smooth, s.smooth_edges);
        // Sparkle keeps this setting; builds without Sparkle cannot update.
        set_check(&c.check_updates, crate::updates::checks_automatically());
        c.check_updates.setEnabled(crate::updates::available());
    });
}

fn selected<T: Copy>(p: &NSPopUpButton, values: &[T], current: T) -> T {
    usize::try_from(p.indexOfSelectedItem()).ok().and_then(|i| values.get(i).copied()).unwrap_or(current)
}

fn positive(field: &NSTextField, current: f64) -> f64 {
    let v = field.stringValue().to_string().trim().parse::<f64>().unwrap_or(current);
    if v.is_finite() && v >= 0.0 { v } else { current }
}

/// Reads every control into the settings and saves them.
fn controls_changed() {
    let mut s = Settings::load();
    let login = WINDOW.with(|w| {
        let w = w.borrow();
        let c = &w.as_ref()?.c;
        s.show_hints = is_on(&c.hints);
        s.units = selected(&c.units, &Units::ALL, s.units);
        s.show_units = is_on(&c.show_units);
        s.design_zoom = selected(&c.design_zoom, DESIGN_ZOOMS, s.design_zoom);
        s.screen_ppi = positive(&c.ppi, 0.0);
        s.tolerance = c.tolerance.doubleValue().round().clamp(0.0, 255.0) as u8;
        s.snap_areas = is_on(&c.snap);
        s.axes = selected(&c.axes, &AXES, s.axes);
        s.show_aspect_ratio = is_on(&c.aspect);
        s.freeze = is_on(&c.freeze);
        if let Ok(i) = usize::try_from(c.color.indexOfSelectedItem())
            && let Some((_, hex)) = COLORS.get(i)
        {
            s.color = hex.to_string();
        }
        s.color_format = selected(&c.color_format, &ColorFormat::ALL, s.color_format);
        s.size_format = selected(&c.size_format, &SizeFormat::ALL, s.size_format);
        s.loupe = is_on(&c.loupe);
        s.loupe_zoom = selected(&c.loupe_zoom, LOUPE_ZOOMS, s.loupe_zoom);
        s.grid = selected(&c.grid, &Grid::ALL, s.grid);
        s.grid_size = positive(&c.grid_size, s.grid_size).max(1.0);
        s.grid_columns = (positive(&c.grid_columns, f64::from(s.grid_columns)).round() as u32).clamp(1, 48);
        s.grid_gutter = positive(&c.grid_gutter, s.grid_gutter);
        s.grid_margin = positive(&c.grid_margin, s.grid_margin);
        s.vision = selected(&c.vision, &Vision::ALL, s.vision);
        s.stamp_screenshots = is_on(&c.stamp);
        s.smooth_edges = is_on(&c.smooth);
        if crate::updates::available() {
            crate::updates::set_checks_automatically(is_on(&c.check_updates));
        }
        Some(is_on(&c.login))
    });
    let Some(login) = login else { return };
    if let Err(e) = s.save() {
        tracing::warn!("saving settings: {e:#}");
    }
    if login != crate::login_item::is_enabled()
        && let Some(message) = crate::login_item::set(login)
    {
        crate::app::show_alert(&t!("Launch at Login"), &message);
    }
    crate::app::settings_updated();
    refresh();
}

/// For the UI tests: builds the Settings window (without showing it) and
/// describes each tab, top to bottom, as "tab | kind | text | VoiceOver
/// label" lines, plus a list of layout problems: controls overlapping or
/// outside their tab, and controls VoiceOver could not name.
/// A control in the layout report.
struct ReportRow {
    /// The vertical center, negated so the top sorts first.
    center: f64,
    x: f64,
    line: String,
    frame: NSRect,
    /// A control rather than a label.
    control: bool,
}

/// How far apart (in points) the vertical centers of controls in one row
/// may be.
const ROW_TOLERANCE: f64 = 8.0;

#[doc(hidden)]
pub fn layout_report_for_tests(mtm: MainThreadMarker) -> (String, Vec<String>) {
    let built = build(mtm);
    WINDOW.with(|w| *w.borrow_mut() = Some(built));
    refresh();
    let mut report = String::new();
    let mut problems = Vec::new();
    WINDOW.with(|w| {
        let w = w.borrow();
        let Some(w) = w.as_ref() else { return };
        for (i, tab) in TAB_TITLES.iter().enumerate() {
            let Some(view) = w.tabs.tabViewItemAtIndex(i as isize).view(mtm) else { continue };
            let bounds = view.bounds();
            let mut rows: Vec<ReportRow> = Vec::new();
            for sub in view.subviews().iter() {
                let any: &AnyObject = sub.as_ref();
                let (kind, text, needs_label) = if let Some(p) = any.downcast_ref::<NSPopUpButton>() {
                    ("popup", p.titleOfSelectedItem().map(|t| t.to_string()).unwrap_or_default(), true)
                } else if let Some(b) = any.downcast_ref::<NSButton>() {
                    let on = if b.state() == NSControlStateValueOn { " [on]" } else { "" };
                    ("button", format!("{}{on}", b.title()), false)
                } else if let Some(s) = any.downcast_ref::<NSSlider>() {
                    ("slider", format!("{}", s.doubleValue()), true)
                } else if let Some(f) = any.downcast_ref::<NSTextField>() {
                    let kind = if f.isEditable() { "field" } else { "label" };
                    (kind, f.stringValue().to_string(), f.isEditable())
                } else {
                    ("view", String::new(), false)
                };
                // Text that does not fit is cut off, unless the control is
                // meant to truncate (the folder path, the list of apps).
                let truncates = any.downcast_ref::<NSTextField>().is_some_and(|f| {
                    matches!(
                        f.lineBreakMode(),
                        objc2_app_kit::NSLineBreakMode::ByTruncatingMiddle
                            | objc2_app_kit::NSLineBreakMode::ByTruncatingTail
                    )
                });
                let frame = sub.frame();
                if kind != "view" && kind != "field" && kind != "slider" && !truncates {
                    let needed = sub.fittingSize().width;
                    if needed > frame.size.width + 1.0 {
                        problems.push(format!(
                            "{tab}: cut off ({needed:.0} of {:.0} points): {kind} {text:?}",
                            frame.size.width
                        ));
                    }
                }
                let label = sub.accessibilityLabel().map(|l| l.to_string()).unwrap_or_default();
                if needs_label && label.is_empty() {
                    problems.push(format!("{tab}: a {kind} has no VoiceOver label"));
                }
                let f = sub.frame();
                let line = format!("{tab} | {kind} | {text} | {label}");
                // Sorted by the vertical center (negated: top first).
                rows.push(ReportRow {
                    center: -(f.origin.y + f.size.height / 2.0),
                    x: f.origin.x,
                    line,
                    frame: f,
                    control: kind != "label",
                });
            }
            // Visual rows, top to bottom: controls whose vertical centers are
            // within a few points share a row, read left to right. This
            // does not depend on the exact heights macOS gives controls.
            rows.sort_by(|a, b| a.center.total_cmp(&b.center));
            let mut grouped: Vec<Vec<ReportRow>> = Vec::new();
            for row in rows {
                match grouped.last_mut() {
                    Some(group) if (row.center - group[0].center).abs() <= ROW_TOLERANCE => group.push(row),
                    _ => grouped.push(vec![row]),
                }
            }
            let rows: Vec<ReportRow> = grouped
                .into_iter()
                .flat_map(|mut group| {
                    group.sort_by(|a, b| a.x.total_cmp(&b.x));
                    group
                })
                .collect();
            for ReportRow { line, frame: f, .. } in &rows {
                report.push_str(line.trim_end());
                report.push('\n');
                let inside = f.origin.x >= 0.0
                    && f.origin.y >= 0.0
                    && f.origin.x + f.size.width <= bounds.size.width + 0.5
                    && f.origin.y + f.size.height <= bounds.size.height + 0.5;
                if !inside {
                    problems.push(format!("{tab}: outside the tab: {line}"));
                }
            }
            // Controls (not labels) must not overlap each other.
            let controls: Vec<&ReportRow> = rows.iter().filter(|r| r.control).collect();
            for (a, ra) in controls.iter().enumerate() {
                for rb in &controls[a + 1..] {
                    let (fa, fb) = (ra.frame, rb.frame);
                    let overlap_x =
                        fa.origin.x < fb.origin.x + fb.size.width && fb.origin.x < fa.origin.x + fa.size.width;
                    let overlap_y =
                        fa.origin.y < fb.origin.y + fb.size.height && fb.origin.y < fa.origin.y + fa.size.height;
                    if overlap_x && overlap_y {
                        problems.push(format!("{tab}: overlapping: {} and {}", ra.line, rb.line));
                    }
                }
            }
        }
    });
    (report, problems)
}
