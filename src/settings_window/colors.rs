//! The Colors window: the recent colors or a saved palette, each with its
//! codes; a click copies one and the last two clicked show their contrast.
//! Recent colors can be saved as a palette, and palettes exported as CSS,
//! JSON, or Adobe swatches. Right-click a color to add it to a palette, or
//! to remove or move it within one.

use super::*;

use objc2_app_kit::{
    NSAlert, NSAlertFirstButtonReturn, NSCellImagePosition, NSFont, NSModalResponseOK, NSSavePanel, NSScrollView,
};

use crate::color::{ColorFormat, ColorHistory, Export, Palettes, contrast_text};

const W: f64 = 560.0;
const H: f64 = 440.0;
const ROW_H: f64 = 30.0;
/// Palettes and the recent colors hold at most this many rows.
const MAX_ROWS: usize = 100;

pub(super) struct ColorsWindow {
    window: Retained<NSWindow>,
    source: Retained<NSPopUpButton>,
    format: Retained<NSPopUpButton>,
    scroll: Retained<NSScrollView>,
    contrast: Retained<NSTextField>,
    save: Retained<NSButton>,
    export: Retained<NSButton>,
    delete: Retained<NSButton>,
    rename: Retained<NSButton>,
    clear: Retained<NSButton>,
    /// The colors in the list, top to bottom.
    shown: Vec<[u8; 3]>,
    /// The last two colors clicked, newest last.
    picked: Vec<[u8; 3]>,
}

thread_local! {
    static COLORS: RefCell<Option<ColorsWindow>> = const { RefCell::new(None) };
}

/// Shows the Colors window, creating it the first time.
pub fn show_colors(mtm: MainThreadMarker) {
    // The window's buttons send to the Settings window's target.
    let target = target(mtm);
    if COLORS.with(|c| c.borrow().is_none()) {
        let built = build_colors(&target, mtm);
        COLORS.with(|c| *c.borrow_mut() = Some(built));
    }
    refresh_colors();
    let window = COLORS.with(|c| c.borrow().as_ref().map(|c| c.window.clone()));
    if let Some(window) = window {
        NSApplication::sharedApplication(mtm).activate();
        window.makeKeyAndOrderFront(None);
        window.orderFrontRegardless();
    }
}

fn build_colors(t: &Target, mtm: MainThreadMarker) -> ColorsWindow {
    // SAFETY: NSWindow's designated initializer.
    let window: Retained<NSWindow> = unsafe {
        msg_send![NSWindow::alloc(mtm), initWithContentRect: rect(0.0, 0.0, W, H),
            styleMask: NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable,
            backing: NSBackingStoreType::Buffered, defer: false]
    };
    // SAFETY: the module keeps the window.
    unsafe { window.setReleasedWhenClosed(false) };
    window.setTitle(&ns(&t!("Colors")));
    window.center();
    let content = window.contentView().expect("a window has a content view");

    let source = popup(&[], t, mtm);
    // SAFETY: the target outlives the window; the action exists on it.
    unsafe { source.setAction(Some(sel!(colorsChanged:))) };
    source.setFrame(rect(16.0, H - 42.0, 240.0, 26.0));
    source.setAccessibilityLabel(Some(&ns(&t!("Show"))));
    content.addSubview(&source);

    let formats: Vec<&str> = ColorFormat::ALL.iter().map(|f| f.name()).collect();
    let format = popup(&formats, t, mtm);
    // SAFETY: as above.
    unsafe { format.setAction(Some(sel!(colorsChanged:))) };
    let label = NSTextField::labelWithString(&ns(&t!("Copy as:")), mtm);
    label.setAlignment(NSTextAlignment::Right);
    let fw = format.fittingSize().width.max(130.0);
    format.setFrame(rect(W - 16.0 - fw, H - 42.0, fw, 26.0));
    let lw = label.fittingSize().width;
    label.setFrame(rect(W - 22.0 - fw - lw, H - 37.0, lw, 18.0));
    format.setAccessibilityLabel(Some(&ns(&t!("Copy as"))));
    content.addSubview(&label);
    content.addSubview(&format);

    let scroll = NSScrollView::initWithFrame(NSScrollView::alloc(mtm), rect(16.0, 96.0, W - 32.0, H - 150.0));
    scroll.setHasVerticalScroller(true);
    scroll.setBorderType(objc2_app_kit::NSBorderType::BezelBorder);
    content.addSubview(&scroll);

    let contrast = NSTextField::labelWithString(&ns(""), mtm);
    contrast.setFrame(rect(18.0, 64.0, W - 36.0, 20.0));
    content.addSubview(&contrast);

    let save = button("Save as Palette…", sel!(savePalette:), t, mtm);
    let export = button("Export…", sel!(exportPalette:), t, mtm);
    let delete = button("Delete Palette", sel!(deletePalette:), t, mtm);
    let rename = button("Rename…", sel!(renamePalette:), t, mtm);
    let clear = button("Clear Recent Colors", sel!(clearColors:), t, mtm);
    for b in [&save, &export, &delete, &rename, &clear] {
        content.addSubview(b);
    }
    ColorsWindow {
        window,
        source,
        format,
        scroll,
        contrast,
        save,
        export,
        delete,
        rename,
        clear,
        shown: Vec::new(),
        picked: Vec::new(),
    }
}

/// Which list is shown: `None` for the recent colors, or a palette's name.
fn selected_palette(c: &ColorsWindow) -> Option<String> {
    let i = c.source.indexOfSelectedItem();
    (i > 0).then(|| c.source.titleOfSelectedItem().map(|t| t.to_string())).flatten()
}

/// Fills the window from the saved colors and palettes (if it is open).
pub fn refresh_colors() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let target = target(mtm);
    COLORS.with(|c| {
        let mut c = c.borrow_mut();
        let Some(c) = c.as_mut() else { return };
        let settings = Settings::load();
        let palettes = Palettes::load_from(&Palettes::path());
        let current = selected_palette(c);

        // The source list: Recent Colors, then the palettes.
        c.source.removeAllItems();
        c.source.addItemWithTitle(&ns(&t!("Recent Colors")));
        for p in &palettes.palettes {
            c.source.addItemWithTitle(&ns(&p.name));
        }
        let palette = current.and_then(|name| palettes.palettes.iter().find(|p| p.name == name));
        match palette {
            Some(p) => c.source.selectItemWithTitle(&ns(&p.name)),
            None => c.source.selectItemAtIndex(0),
        }
        let i = ColorFormat::ALL.iter().position(|f| *f == settings.color_format).unwrap_or(0);
        c.format.selectItemAtIndex(i as isize);

        c.shown = match palette {
            Some(p) => p.colors.clone(),
            None => ColorHistory::load_from(&ColorHistory::path()).colors,
        };
        c.shown.truncate(MAX_ROWS);
        c.delete.setHidden(palette.is_none());
        c.rename.setHidden(palette.is_none());
        c.clear.setHidden(palette.is_some());
        // The buttons shown, side by side.
        let mut x = 12.0;
        for b in [&c.save, &c.export, &c.delete, &c.rename, &c.clear].into_iter().filter(|b| !b.isHidden()) {
            let w = b.fittingSize().width.max(90.0);
            b.setFrame(rect(x, 16.0, w, 32.0));
            x += w + 4.0;
        }
        c.save.setEnabled(palette.is_none() && !c.shown.is_empty());
        c.export.setEnabled(!c.shown.is_empty());
        c.picked.retain(|p| c.shown.contains(p));

        // One row per color: its swatch and codes; a click copies it.
        let clip = c.scroll.contentSize();
        let height = (c.shown.len() as f64 * ROW_H).max(clip.height);
        let list = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, clip.width, height));
        for (n, rgb) in c.shown.iter().enumerate() {
            let title = format!(
                "{}     {}     {}",
                ColorFormat::Hex.format(*rgb),
                ColorFormat::Rgb.format(*rgb),
                ColorFormat::Hsl.format(*rgb)
            );
            let row = button(&title, sel!(colorClicked:), &target, mtm);
            row.setTitle(&ns(&title));
            row.setBordered(false);
            row.setContentTintColor(Some(&objc2_app_kit::NSColor::labelColor()));
            row.setImage(Some(&crate::app::swatch(*rgb, 20.0)));
            row.setImagePosition(NSCellImagePosition::ImageLeft);
            row.setAlignment(NSTextAlignment::Left);
            row.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(12.0, 0.0)));
            row.setTag(n as isize);
            row.setFrame(rect(8.0, height - (n as f64 + 1.0) * ROW_H, clip.width - 16.0, ROW_H));
            row.setAccessibilityLabel(Some(&ns(&t!("Copy {color}", color = ColorFormat::Hex.format(*rgb)))));
            let menu = row_menu(n, c.shown.len(), palette.is_some(), &palettes, &target, mtm);
            // SAFETY: the row keeps its menu; its items target the shared target.
            unsafe { row.setMenu(Some(&menu)) };
            list.addSubview(&row);
        }
        if c.shown.is_empty() {
            let empty = NSTextField::labelWithString(
                &ns(&t!("No colors yet: pick one with the Pick Color shortcut, or P while measuring.")),
                mtm,
            );
            empty.setFrame(rect(10.0, height - 30.0, clip.width - 20.0, 18.0));
            list.addSubview(&empty);
        }
        c.scroll.setDocumentView(Some(&list));
        // Top of the list first.
        list.scrollPoint(NSPoint::new(0.0, height));
        show_contrast(c);
    });
}

// What a color row's menu items do: the row times ROW, plus one of these.
const ROW: isize = 1000;
const REMOVE: isize = 1;
const MOVE_UP: isize = 2;
const MOVE_DOWN: isize = 3;
const NEW_PALETTE: isize = 4;
/// Plus a palette's index: add the color to that palette.
const ADD_TO: isize = 100;

/// The right-click menu of color row `n` (of `count`).
fn row_menu(
    n: usize,
    count: usize,
    in_palette: bool,
    palettes: &Palettes,
    target: &Target,
    mtm: MainThreadMarker,
) -> Retained<objc2_app_kit::NSMenu> {
    use objc2_app_kit::{NSMenu, NSMenuItem};
    let item = |menu: &NSMenu, title: &str, code: isize, enabled: bool| {
        // SAFETY: the action exists on the target, which outlives the menu.
        let it = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(mtm),
                &ns(title),
                Some(sel!(colorMenu:)),
                &ns(""),
            )
        };
        it.setTag(n as isize * ROW + code);
        // SAFETY: as above.
        unsafe { it.setTarget(Some(target)) };
        it.setEnabled(enabled);
        menu.addItem(&it);
    };
    let menu = NSMenu::new(mtm);
    menu.setAutoenablesItems(false);
    let add = NSMenu::new(mtm);
    add.setAutoenablesItems(false);
    for (i, p) in palettes.palettes.iter().enumerate().take((ROW - ADD_TO) as usize) {
        item(&add, &p.name, ADD_TO + i as isize, true);
    }
    if !palettes.palettes.is_empty() {
        add.addItem(&NSMenuItem::separatorItem(mtm));
    }
    item(&add, &t!("New Palette…"), NEW_PALETTE, true);
    let parent = NSMenuItem::new(mtm);
    parent.setTitle(&ns(&t!("Add to Palette")));
    parent.setSubmenu(Some(&add));
    menu.addItem(&parent);
    if in_palette {
        menu.addItem(&NSMenuItem::separatorItem(mtm));
        item(&menu, &t!("Remove from Palette"), REMOVE, true);
        item(&menu, &t!("Move Up"), MOVE_UP, n > 0);
        item(&menu, &t!("Move Down"), MOVE_DOWN, n + 1 < count);
    }
    menu
}

/// A color row's menu item was chosen.
pub(super) fn color_menu(tag: isize) {
    let (row, code) = (tag / ROW, tag % ROW);
    let Some((rgb, current)) = COLORS.with(|c| {
        let c = c.borrow();
        let c = c.as_ref()?;
        Some((*c.shown.get(usize::try_from(row).ok()?)?, selected_palette(c)))
    }) else {
        return;
    };
    let path = Palettes::path();
    let mut palettes = Palettes::load_from(&path);
    match code {
        NEW_PALETTE => {
            let suggestion = palettes.unused_name(&t!("Palette"));
            let Some(name) = ask_name(&t!("Add the color to a new palette named"), &suggestion) else { return };
            palettes.add_color(&name, rgb);
        }
        c if c >= ADD_TO => {
            let Some(name) = palettes.palettes.get((c - ADD_TO) as usize).map(|p| p.name.clone()) else { return };
            palettes.add_color(&name, rgb);
        }
        REMOVE | MOVE_UP | MOVE_DOWN => {
            let Some(p) = current.as_deref().and_then(|name| palettes.get_mut(name)) else { return };
            let row = row as usize;
            match code {
                REMOVE if row < p.colors.len() => {
                    p.colors.remove(row);
                }
                MOVE_UP => p.move_color(row, -1),
                MOVE_DOWN => p.move_color(row, 1),
                _ => {}
            }
        }
        _ => return,
    }
    if let Err(e) = palettes.save_to(&path) {
        crate::app::show_alert(&t!("Could not save the palette"), &format!("{e:#}"));
    }
    refresh_colors();
}

/// Renames the palette shown.
pub(super) fn rename_palette() {
    let Some(old) = COLORS.with(|c| c.borrow().as_ref().and_then(selected_palette)) else { return };
    let Some(new) = ask_name(&t!("Rename the palette to"), &old) else { return };
    let new = new.trim().to_string();
    let path = Palettes::path();
    let mut palettes = Palettes::load_from(&path);
    if !palettes.rename(&old, &new) {
        crate::app::show_alert(&t!("Could not rename the palette"), &t!("Another palette is called {new}.", new));
        return;
    }
    if let Err(e) = palettes.save_to(&path) {
        crate::app::show_alert(&t!("Could not save the palette"), &format!("{e:#}"));
        return;
    }
    // Show it under its new name.
    COLORS.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            c.source.addItemWithTitle(&ns(&new));
            c.source.selectItemWithTitle(&ns(&new));
        }
    });
    refresh_colors();
}

fn show_contrast(c: &ColorsWindow) {
    let text = match c.picked.as_slice() {
        [a, b] => t!(
            "{a} and {b}: contrast {ratio}",
            a = ColorFormat::Hex.format(*a),
            b = ColorFormat::Hex.format(*b),
            ratio = contrast_text(*a, *b)
        ),
        _ => t!("Click a color to copy it; the last two clicked show their contrast."),
    };
    c.contrast.setStringValue(&ns(&text));
}

/// A row was clicked: copies its color.
pub(super) fn color_clicked(tag: isize) {
    let copied = COLORS.with(|c| {
        let mut c = c.borrow_mut();
        let c = c.as_mut()?;
        let rgb = *c.shown.get(usize::try_from(tag).ok()?)?;
        c.picked.retain(|p| *p != rgb);
        c.picked.push(rgb);
        if c.picked.len() > 2 {
            c.picked.remove(0);
        }
        show_contrast(c);
        Some(Settings::load().color_format.format(rgb))
    });
    if let Some(text) = copied {
        crate::app::copy_to_clipboard(&text);
    }
}

/// The list or the copy format changed.
pub(super) fn colors_changed() {
    let format = COLORS.with(|c| c.borrow().as_ref().map(|c| c.format.indexOfSelectedItem()));
    if let Some(i) = format.and_then(|i| usize::try_from(i).ok()).filter(|i| *i < ColorFormat::ALL.len()) {
        let mut s = Settings::load();
        if s.color_format != ColorFormat::ALL[i] {
            s.color_format = ColorFormat::ALL[i];
            save_and_refresh(&s);
        }
    }
    refresh_colors();
}

/// Asks for a name in a small alert. `None` when cancelled or empty.
fn ask_name(title: &str, suggestion: &str) -> Option<String> {
    let mtm = MainThreadMarker::new()?;
    let alert = NSAlert::new(mtm);
    alert.setMessageText(&ns(title));
    alert.addButtonWithTitle(&ns(&t!("Save")));
    alert.addButtonWithTitle(&ns(&tr("Cancel")));
    let field = NSTextField::textFieldWithString(&ns(suggestion), mtm);
    field.setFrame(rect(0.0, 0.0, 240.0, 24.0));
    alert.setAccessoryView(Some(&field));
    alert.window().setInitialFirstResponder(Some(&field));
    let saved = alert.runModal() == NSAlertFirstButtonReturn;
    let name = field.stringValue().to_string().trim().to_string();
    (saved && !name.is_empty()).then_some(name)
}

/// Saves the recent colors as a palette.
pub(super) fn save_palette() {
    let shown = COLORS.with(|c| c.borrow().as_ref().map(|c| c.shown.clone())).unwrap_or_default();
    if shown.is_empty() {
        return;
    }
    let path = Palettes::path();
    let mut palettes = Palettes::load_from(&path);
    let suggestion = palettes.unused_name(&t!("Palette"));
    let Some(name) = ask_name(&t!("Save the recent colors as a palette named"), &suggestion) else { return };
    palettes.put(&name, shown);
    if let Err(e) = palettes.save_to(&path) {
        crate::app::show_alert(&t!("Could not save the palette"), &format!("{e:#}"));
        return;
    }
    COLORS.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            c.source.addItemWithTitle(&ns(&name));
            c.source.selectItemWithTitle(&ns(&name));
        }
    });
    refresh_colors();
}

/// Deletes the palette shown.
pub(super) fn delete_palette() {
    let Some(name) = COLORS.with(|c| c.borrow().as_ref().and_then(selected_palette)) else { return };
    let path = Palettes::path();
    let mut palettes = Palettes::load_from(&path);
    palettes.remove(&name);
    if let Err(e) = palettes.save_to(&path) {
        tracing::warn!("deleting palette {name}: {e:#}");
    }
    COLORS.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            c.source.selectItemAtIndex(0);
        }
    });
    refresh_colors();
}

pub(super) fn clear_recent_colors() {
    if let Err(e) = ColorHistory::default().save_to(&ColorHistory::path()) {
        tracing::warn!("clearing the color history: {e:#}");
    }
    refresh_colors();
}

/// Exports the colors shown, in a format chosen in the save panel.
pub(super) fn export_palette() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some((name, colors)) = COLORS.with(|c| {
        c.borrow().as_ref().map(|c| (selected_palette(c).unwrap_or_else(|| t!("Recent Colors")), c.shown.clone()))
    }) else {
        return;
    };
    if colors.is_empty() {
        return;
    }
    let panel = NSSavePanel::savePanel(mtm);
    let names: Vec<&str> = Export::ALL.iter().map(|e| e.name()).collect();
    let target = target(mtm);
    let formats = popup(&names, &target, mtm);
    // SAFETY: a plain popup; no action needed while the panel is open.
    unsafe { formats.setAction(None) };
    let label = NSTextField::labelWithString(&ns(&t!("Format:")), mtm);
    let accessory = NSView::initWithFrame(NSView::alloc(mtm), rect(0.0, 0.0, 360.0, 40.0));
    label.setFrame(rect(10.0, 12.0, 80.0, 18.0));
    label.setAlignment(NSTextAlignment::Right);
    formats.setFrame(rect(96.0, 8.0, 240.0, 26.0));
    accessory.addSubview(&label);
    accessory.addSubview(&formats);
    panel.setAccessoryView(Some(&accessory));
    panel.setNameFieldStringValue(&ns(&name));
    panel.setMessage(Some(&ns(&t!("Export {name}", name))));
    if panel.runModal() != NSModalResponseOK {
        return;
    }
    let Some(path) = panel.URL().and_then(|u| u.path()) else { return };
    let export = Export::ALL[usize::try_from(formats.indexOfSelectedItem()).unwrap_or(0).min(Export::ALL.len() - 1)];
    let path = std::path::PathBuf::from(path.to_string()).with_extension(export.extension());
    if let Err(e) = std::fs::write(&path, export.render(&name, &colors)) {
        crate::app::show_alert(&t!("Could not export the colors"), &format!("{}: {e}", path.display()));
    }
}
