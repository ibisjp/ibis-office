// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツール > 設定: the settings are a modal dialog inside the window (the common dialog, no window of their own), Esc closes it, the
//! splitter between the categories and the detail is dragged and its width is kept in the app's settings file, and the quick access
//! table reorders by dragging its rows, deletes with the trash button and adds through the common searchable menu (search, Enter).
use super::{Scenario, button_probe as buttons};
use crate::settings_dialog as dialog;

pub fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::settings_dialog_modal_open_and_esc", modal_open_and_esc),
        ("shell::settings_dialog_splitter_kept", splitter_kept),
        ("shell::toolbar_customize_quick_access_and_all_scenes", toolbar_customize_quick_access_and_all_scenes),
        ("shell::settings_dialog_one_font_size", one_font_size),
        ("shell::settings_dialog_formula_bar_font_default", formula_bar_font_default),
        ("shell::settings_dialog_formula_bar_font_search_space", formula_bar_font_search_space),
        ("shell::settings_dialog_csv_font", csv_font),
        ("shell::issue237_window_bounds_fit_the_display", window_bounds_fit_the_display),
        ("shell::issue237_zoom_follows_the_file_else_the_last_one", zoom_follows_the_file_else_the_last_one),
        ("shell::toolbar_layout_follows_the_saved_order", toolbar_layout_follows_the_saved_order),
        ("shell::toolbar_customize_opens_and_keeps_the_orders", toolbar_customize_opens_and_keeps_the_orders),
        ("shell::toolbar_customize_edits_the_order", toolbar_customize_edits_the_order),
        ("shell::toolbar_customize_highlight_and_fold", toolbar_customize_highlight_and_fold),
        ("shell::k0038_old_menu_ids_in_the_settings_are_read", old_menu_ids_in_the_settings),
        ("shell::toolbar_customize_preview_drag_and_trash", toolbar_customize_preview_drag_and_trash),
    ]
}

/// The settings of the scenario in a folder of its own, and the quick access row empty; undone when dropped.
struct Fresh;
impl Fresh {
    fn new(name: &str) -> Self {
        let dir = crate::scenario::repo_root().join("target/scenario").join(name).join("settings");
        let _ = std::fs::remove_dir_all(&dir);
        // SAFETY: the scenarios run one at a time on the main thread; nothing else reads the environment meanwhile.
        unsafe { std::env::set_var("IBIS_SETTINGS_DIR", &dir) };
        crate::qat::reload();
        Fresh
    }
}
impl Drop for Fresh {
    fn drop(&mut self) {
        // SAFETY: as in `new`.
        unsafe { std::env::remove_var("IBIS_SETTINGS_DIR") };
        crate::qat::reload();
        crate::i18n::show(crate::i18n::Language::Japanese);
    }
}

fn is_open(s: &mut Scenario) -> bool {
    s.with(|e, _| e.settings.is_some())
}

/// ツール > 設定 opens a modal dialog in the window (on a document and on the start window alike), centred, and Esc closes it.
fn modal_open_and_esc() {
    let _fresh = Fresh::new("shell::settings_dialog_modal_open_and_esc");
    for start in [false, true] {
        let mut s = if start { Scenario::start_view() } else { Scenario::open("test/editing/xlsx-sales.xlsx") };
        s.size(1200., 800.);
        assert!(!is_open(&mut s));
        s.menu("tools.settings");
        s.settle();
        assert!(is_open(&mut s), "the dialog is open (start window: {start})");
        assert_eq!(s.window_count(), 1, "no window of its own (start window: {start})");
        assert!(s.with(|e, _| e.dialog_is_modal()), "modal (start window: {start})");
        let (bounds, viewport) = s.update(|e, window, _| (e.dialog.bounds.expect("the dialog's place"), window.viewport_size()));
        let (dx, dy) = (f32::from(bounds.center().x) - f32::from(viewport.width) / 2., f32::from(bounds.center().y) - f32::from(viewport.height) / 2.);
        assert!(dx.abs() < 2. && dy.abs() < 2., "centred in the window: {bounds:?} in {viewport:?}");
        s.screenshot(if start { "start-open" } else { "open" });
        s.key("escape");
        s.settle();
        assert!(!is_open(&mut s), "Esc closes the dialog (start window: {start})");
        assert!(!s.with(|e, _| e.dialog_is_modal()));
    }
}

/// The splitter is dragged by the pointer within its limits and the width is kept in the settings file and read back by the next dialog.
fn splitter_kept() {
    let _fresh = Fresh::new("shell::settings_dialog_splitter_kept");
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    assert_eq!(dialog::nav_width(), dialog::NAV_WIDTH_DEFAULT, "no width kept yet");
    s.menu("tools.settings");
    s.settle();
    let splitter = |s: &mut Scenario| -> (f32, f32) {
        let (b, w) = s.with(|e, _| (e.dialog.bounds.unwrap(), e.settings.as_ref().unwrap().nav_width));
        // Inside the frame (1) and the body's padding (12), the left pane, then the splitter's 9 px.
        (f32::from(b.origin.x) + 13. + w + 4.5, f32::from(b.center().y))
    };
    let width = |s: &mut Scenario| s.with(|e, _| e.settings.as_ref().unwrap().nav_width);
    assert_eq!(width(&mut s), dialog::NAV_WIDTH_DEFAULT);
    let from = splitter(&mut s);
    s.drag(from, (from.0 + 60., from.1));
    s.settle();
    assert_eq!(width(&mut s), dialog::NAV_WIDTH_DEFAULT + 60., "dragged right by 60");
    assert!(std::fs::read_to_string(crate::scenario::repo_root().join("target/scenario/shell::settings_dialog_splitter_kept/settings/settings.txt")).unwrap_or_default().contains("settings.nav-width=240"), "kept in the settings file");
    s.screenshot("wider");
    // Beyond the limits it stops at them.
    let from = splitter(&mut s);
    s.drag(from, (from.0 + 400., from.1));
    s.settle();
    assert_eq!(width(&mut s), dialog::NAV_WIDTH_MAX, "the widest");
    let from = splitter(&mut s);
    s.drag(from, (from.0 - 600., from.1));
    s.settle();
    assert_eq!(width(&mut s), dialog::NAV_WIDTH_MIN, "the narrowest");
    assert_eq!(dialog::nav_width(), dialog::NAV_WIDTH_MIN);
    // Closed and opened again (as at the next start), the kept width is back.
    s.key("escape");
    s.settle();
    assert!(!is_open(&mut s));
    s.menu("tools.settings");
    s.settle();
    assert_eq!(width(&mut s), dialog::NAV_WIDTH_MIN, "the width is restored");
    // A damaged value is the default.
    crate::settings::set(dialog::NAV_WIDTH_KEY, "wide");
    assert_eq!(dialog::nav_width(), dialog::NAV_WIDTH_DEFAULT);
    crate::settings::set(dialog::NAV_WIDTH_KEY, "9999");
    assert_eq!(dialog::nav_width(), dialog::NAV_WIDTH_MAX);
}


/// Every pane of the dialog is in the one standard font size: the left pane, the headings and the notes use the panels' size (no
/// larger heading, no smaller note of their own), and the pictures of both languages are taken to look at.
fn one_font_size() {
    let _fresh = Fresh::new("shell::settings_dialog_one_font_size");
    let source = std::fs::read_to_string(crate::scenario::repo_root().join("crates/ibis-office-shell/src/settings_dialog.rs")).unwrap();
    let ui = &source[source.find("impl SlideEditor").unwrap()..];
    for size in ["text_lg", "text_xs", "text_size(", "text_xl", "font_size("] {
        assert!(!ui.contains(size), "the settings dialog draws its own font size: {size}");
    }
    for (language, name) in [(crate::i18n::Language::Japanese, "ja"), (crate::i18n::Language::English, "en")] {
        let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
        s.size(1200., 800.);
        crate::qat::set(&["file.save".to_string(), "edit.copy".to_string(), "xlsx.freeze".to_string()]);
        crate::i18n::show(language);
        for category in 0..4 {
            s.update(move |e, window, cx| e.open_settings(category, window, cx));
            s.settle();
            s.screenshot(&format!("{name}-{category}"));
        }
        crate::qat::reset();
    }
}

/// GitHub issue #149 (#M90): 数式バーの英数フォント. The list's first item is 既定（アプリの書体）, which is what the button shows and what is
/// checked while no font is chosen; choosing a font keeps it in the settings file; choosing 既定 takes the line away again.
fn formula_bar_font_default() {
    use crate::chrome::MenuEntry;
    let _fresh = Fresh::new("shell::settings_dialog_formula_bar_font_default");
    let file = crate::scenario::repo_root().join("target/scenario/shell::settings_dialog_formula_bar_font_default/settings/settings.txt");
    let has_line = || std::fs::read_to_string(&file).unwrap_or_default().lines().any(|l| l.starts_with("xlsx.formula-bar.font="));
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    s.update(|e, window, cx| e.open_settings(dialog::CATEGORY_SPREADSHEET, window, cx));
    s.settle();
    s.screenshot("default");
    let default_label = crate::i18n::text("settings.spreadsheet.formula-bar-font-default").to_string();
    // The handler of the item with the id (`first` of the font items is the first system font).
    let handler = |s: &mut Scenario, id: &'static str| {
        let (_, entries) = dialog::formula_bar_font_menu(".SystemUIFont".to_string());
        let item = entries.into_iter().find_map(|m| if let MenuEntry::Item(i) = m { (i.id == id).then_some(i) } else { None }).unwrap_or_else(|| panic!("no item {id}"));
        let handler = item.handler.clone().expect("the item's handler");
        s.update(|e, window, cx| handler(e, window, cx));
    };
    // No font chosen: the button shows 既定, not the app font's internal name; the first item is 既定 and checked.
    let (current, entries) = dialog::formula_bar_font_menu(".SystemUIFont".to_string());
    assert_eq!(current, default_label);
    assert!(!has_line());
    let Some(MenuEntry::Item(first)) = entries.first() else { panic!("the first entry") };
    assert_eq!((first.id, first.label.to_string(), first.checked), ("settings-formula-bar-font-default", default_label.clone(), true));
    // Choose a font: kept, and the button shows it; 既定 is no longer checked.
    let font = entries.iter().find_map(|m| if let MenuEntry::Item(i) = m { (i.id == "settings-formula-bar-font").then(|| i.label.to_string()) } else { None }).expect("a system font in the list");
    handler(&mut s, "settings-formula-bar-font");
    s.settle();
    assert_eq!(dialog::formula_bar_font().as_deref(), Some(font.as_str()));
    assert!(has_line(), "the line is in the settings file");
    let (current, entries) = dialog::formula_bar_font_menu(".SystemUIFont".to_string());
    assert_eq!(current, font);
    let Some(MenuEntry::Item(first)) = entries.first() else { panic!("the first entry") };
    assert!(!first.checked, "the default item is not checked while a font is chosen");
    s.update(|e, window, cx| e.open_settings(dialog::CATEGORY_SPREADSHEET, window, cx));
    s.settle();
    s.screenshot("chosen");
    // 既定: the line is gone, the formula bar is in the app's font again.
    handler(&mut s, "settings-formula-bar-font-default");
    s.settle();
    assert_eq!(dialog::formula_bar_font(), None);
    assert!(!has_line(), "the line is taken out of the settings file");
    let (current, _) = dialog::formula_bar_font_menu(".SystemUIFont".to_string());
    assert_eq!(current, default_label);
    s.screenshot("restored");
}

/// GitHub issue #243: the font a text file is opened in. Nothing chosen is the usual font (None: no work on open); a family chosen from the
/// list and a size are kept in the settings and read back; the page is drawn with them.
fn csv_font() {
    let _fresh = Fresh::new("shell::settings_dialog_csv_font");
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    assert_eq!(dialog::csv_font(), None);
    let (current, entries) = dialog::font_setting_menu(dialog::CSV_FONT_KEY, "settings-csv-font", "settings-csv-font-default", crate::i18n::text("settings.spreadsheet.csv-font-default"), String::new());
    assert_eq!(current, crate::i18n::text("settings.spreadsheet.csv-font-default").to_string());
    assert!(entries.len() > 1, "the list has the system's fonts");
    crate::settings::set(dialog::CSV_FONT_KEY, "Arial");
    assert_eq!(dialog::csv_font(), Some(ibis_formats::csv::CsvFont { name: "Arial".into(), size_pt: 12.0 }));
    crate::settings::set(dialog::CSV_FONT_SIZE_KEY, "9.5");
    assert_eq!(dialog::csv_font().map(|f| f.size_pt), Some(9.5));
    s.update(|e, window, cx| e.open_settings(dialog::CATEGORY_SPREADSHEET, window, cx));
    s.settle();
    s.screenshot("csv-font");
}

/// GitHub issue #149 (#M87/#M90, the Windows check): a space typed in the search field of the font list is a character of the search, not
/// a key of the button that opened the list (`Courier New` could not be typed: the list closed at the space).
fn formula_bar_font_search_space() {
    let _fresh = Fresh::new("shell::settings_dialog_formula_bar_font_search_space");
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    s.update(|e, window, cx| e.open_settings(dialog::CATEGORY_SPREADSHEET, window, cx));
    s.settle();
    buttons::click(&mut s, "settings-formula-bar-font");
    s.settle();
    assert_eq!(s.with(|e, _| e.toolbar_search_open), Some("settings-formula-bar-font"), "the font list is open");
    s.type_text("Courier");
    s.key("space");
    assert_eq!(s.with(|e, _| e.toolbar_search_open), Some("settings-formula-bar-font"), "the list stays open at the space");
    s.type_text("New");
    assert_eq!(s.with(|e, cx| e.toolbar_search_inputs["settings-formula-bar-font"].read(cx).value().to_string()), "Courier New");
    s.screenshot("search-space");
}

/// GitHub issue #241: the toolbar is drawn in the order saved for its scene (the buttons named, a separator, a menu command as a
/// button of its own, a submenu as a button with a ▼), the locked buttons first; taking the setting away gives the format's own back.
fn toolbar_layout_follows_the_saved_order() {
    use crate::chrome::ToolItem;
    use crate::toolbar_layout as layout;
    let _fresh = Fresh::new("shell::toolbar_layout_follows_the_saved_order");
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    s.settle();
    let keys = |s: &mut Scenario| s.update(|e, _, _| e.toolbar_items().iter().map(|i| layout::item_key(i).to_string()).collect::<Vec<_>>());
    let own = keys(&mut s);
    assert!(own.len() > 6, "the workbook's own toolbar has buttons: {own:?}");
    let scene = s.update(|e, _, _| crate::format::with_format!(e, toolbar_scene(e)));
    assert_eq!(scene, "cell");
    assert!(layout::saved("xlsx", "cell").is_none());
    // The order is the buttons `italic` and `bold` (a plain order of the workbook's buttons), a separator, and a menu command.
    let pick: Vec<String> = own.iter().filter(|k| !layout::LOCKED.contains(&k.as_str()) && k.as_str() != "|").take(2).cloned().collect();
    let mut order = vec![pick[1].clone(), pick[0].clone(), "|".to_string()];
    order.push("cmd:file.save".to_string());
    layout::save("xlsx", "cell", &order);
    let drawn = keys(&mut s);
    assert_eq!(drawn[..2], ["menu-search".to_string(), "format-paint".to_string()], "the locked buttons are first: {drawn:?}");
    assert_eq!(drawn[2], "|");
    // What follows is the buttons that are not drawn (hidden), which stay.
    assert_eq!(drawn[3..7], [pick[1].clone(), pick[0].clone(), "|".to_string(), "file.save".to_string()], "{drawn:?}");
    let save = s.update(|e, _, _| e.toolbar_items().into_iter().find_map(|i| if let ToolItem::Button(b) = i { (b.id == "file.save").then_some(b) } else { None }));
    assert!(save.is_some_and(|b| b.handler.is_some()), "a menu command is a button that runs the command");
    s.screenshot("custom");
    layout::reset("xlsx", "cell");
    assert_eq!(keys(&mut s), own, "no setting: the format's own buttons");
}

/// GitHub issue #241: the settings have a page for each format with the button that opens ツールバーのカスタマイズ over the settings;
/// OK keeps the orders (a scene back to the format's own is taken out of the settings), Esc and キャンセル keep nothing.
fn toolbar_customize_opens_and_keeps_the_orders() {
    use crate::toolbar_layout as layout;
    let _fresh = Fresh::new("shell::toolbar_customize_opens_and_keeps_the_orders");
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    for category in [dialog::CATEGORY_DOCUMENT, dialog::CATEGORY_SPREADSHEET, dialog::CATEGORY_PRESENTATION] {
        s.update(move |e, window, cx| e.open_settings(category, window, cx));
        s.settle();
        s.screenshot(&format!("page-{category}"));
    }
    s.update(|e, window, cx| e.open_settings(dialog::CATEGORY_SPREADSHEET, window, cx));
    s.update(|e, window, cx| e.open_toolbar_customize("xlsx", window, cx));
    s.settle();
    s.screenshot("open");
    let open = |s: &mut Scenario| s.update(|e, _, _| e.settings.as_ref().is_some_and(|d| d.customize.is_some()));
    assert!(open(&mut s));
    // The scene is the one the workbook is in.
    assert_eq!(s.update(|e, _, _| e.settings.as_ref().and_then(|d| d.customize.as_ref()).map(|c| c.scene)), Some(0));
    // An order made in the dialog is kept by OK only.
    s.update(|e, _, _| e.settings.as_mut().and_then(|d| d.customize.as_mut()).unwrap().orders[0] = Some(vec!["bold".to_string()]));
    s.update(|e, _, _| {
        e.dialog.key = Some(crate::toolbar_customize::DIALOG_KEY);
        e.discard_current_dialog();
    });
    assert!(!open(&mut s));
    assert!(layout::saved("xlsx", "cell").is_none(), "Esc keeps nothing");
    s.update(|e, window, cx| e.open_toolbar_customize("xlsx", window, cx));
    s.update(|e, _, _| e.settings.as_mut().and_then(|d| d.customize.as_mut()).unwrap().orders[0] = Some(vec!["bold".to_string()]));
    s.update(|e, _, _| e.settings.as_mut().and_then(|d| d.customize.take()).unwrap().save());
    assert_eq!(layout::saved("xlsx", "cell"), Some(vec!["bold".to_string()]));
}

/// GitHub issue #241: the dialog's list of commands (the menus' layout of the format, folded by menu, found by a search that does not
/// tell hiragana from katakana, full-width from half-width, or capitals from small letters), and the order's edits: add after the
/// selected row, a command once and a separator any times, move, remove, undo and redo, reset a scene; a scene put back to the format's
/// own order is no order at all.
fn toolbar_customize_edits_the_order() {
    use crate::toolbar_customize as customize;
    let _fresh = Fresh::new("shell::toolbar_customize_edits_the_order");
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    s.settle();
    s.update(|e, window, cx| e.open_settings(dialog::CATEGORY_SPREADSHEET, window, cx));
    s.update(|e, window, cx| e.open_toolbar_customize("xlsx", window, cx));
    s.settle();
    s.screenshot("dialog");
    let (names, commands) = customize::catalog("xlsx");
    assert!(!names.is_empty() && commands.iter().any(|c| c.key == "cmd:file.save"), "the commands of the workbook's menus");
    assert!(commands.iter().any(|c| c.submenu && c.key.starts_with("menu:")), "a submenu is a command with a ▼");
    assert!(customize::matches("ＢＯＬＤ ｆｉｌｅ ひらがな", "bold FILE ヒラガナ")); // i18n-ok（検索の正規化の試験データ（かなの区別なし）。迷った：試験用の入力）
    assert!(!customize::matches("bold", "bold italic"));
    let own = s.update(|e, _, _| e.settings.as_ref().unwrap().customize.as_ref().unwrap().default_order()).expect("the workbook's own order was seen");
    assert!(own.len() > 3);
    fn run(s: &mut Scenario, f: &dyn Fn(&mut customize::ToolbarCustomize)) {
        s.update(|e, _, _| f(e.settings.as_mut().unwrap().customize.as_mut().unwrap()));
    }
    let order = |s: &mut Scenario| s.update(|e, _, _| e.settings.as_ref().unwrap().customize.as_ref().unwrap().order());
    assert!(!s.update(|e, _, _| e.settings.as_ref().unwrap().customize.as_ref().unwrap().changed()));
    run(&mut s, &|c| {
        assert!(c.add("cmd:file.save"));
        assert!(!c.add("cmd:file.save"), "a command is in the order once");
        assert!(c.add("|") && c.add("|"), "a separator any times");
    });
    let after = order(&mut s);
    assert_eq!(after.len(), own.len() + 3);
    assert_eq!(after[own.len()], "cmd:file.save");
    run(&mut s, &|c| c.move_row(own.len(), 0));
    assert_eq!(order(&mut s)[0], "cmd:file.save");
    run(&mut s, &|c| c.remove(0));
    run(&mut s, &|c| c.undo());
    assert_eq!(order(&mut s)[0], "cmd:file.save");
    run(&mut s, &|c| c.redo());
    assert_eq!(order(&mut s)[0], own[0]);
    run(&mut s, &|c| c.reset(false));
    assert_eq!(order(&mut s), own);
    assert!(!s.update(|e, _, _| e.settings.as_ref().unwrap().customize.as_ref().unwrap().changed()), "back to the format's own: nothing changed");
    // Taking out what was put leaves the format's own order, which is no order saved.
    run(&mut s, &|c| {
        c.add("cmd:file.save");
        c.remove(c.selected.unwrap());
    });
    assert!(s.update(|e, _, _| e.settings.as_ref().unwrap().customize.as_ref().unwrap().orders[0].is_none()));
}

/// GitHub issue #241: the preview's buttons are dragged left and right (the order follows where they are let go), and a button let go in
/// the trash circle, which shows while one is dragged, is taken out. A command of the list dropped on the order is put where the
/// insertion line was.
fn toolbar_customize_preview_drag_and_trash() {
    use gpui_kit::{point, px};
    let _fresh = Fresh::new("shell::toolbar_customize_preview_drag_and_trash");
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    s.settle();
    s.update(|e, window, cx| e.open_settings(dialog::CATEGORY_SPREADSHEET, window, cx));
    s.update(|e, window, cx| e.open_toolbar_customize("xlsx", window, cx));
    s.settle();
    let order = |s: &mut Scenario| s.update(|e, _, _| e.settings.as_ref().unwrap().customize.as_ref().unwrap().order());
    let before = order(&mut s);
    assert!(before.len() > 3);
    // The first button of the order, held and carried far to the right: it ends last.
    let (bounds, widths_base) = s.update(|e, _, _| {
        let c = e.settings.as_ref().unwrap().customize.as_ref().unwrap();
        (c.preview.bounds.get(), c.preview_widths().unwrap())
    });
    let (widths, base, _) = widths_base;
    let xs = crate::toolbar_preview::positions(&widths);
    let grab = point(px(10.), px(10.));
    let at = |x: f32, y: f32| point(bounds.origin.x + px(x), bounds.origin.y + px(y));
    s.update(|e, _, cx| e.preview_press(0, grab, at(xs[base] + 10., 10.), cx));
    s.update(|e, _, cx| e.preview_pointer(at(5_000., 10.), cx));
    s.settle();
    s.screenshot("dragging");
    s.update(|e, _, cx| e.preview_release(cx));
    let after = order(&mut s);
    // It goes as far as the last button the preview shows (the rest are in ︙), not to the end of the order.
    assert_ne!(after.first(), before.first(), "{after:?}");
    assert!(after.contains(&before[0]), "{after:?}");
    assert_eq!(after.len(), before.len());
    // Held and let go in the trash circle: taken out.
    s.update(|e, _, cx| e.preview_press(0, grab, at(xs[base] + 10., 10.), cx));
    s.update(|e, _, cx| e.preview_pointer(at(xs[base] + 40., -60.), cx));
    s.settle();
    let trash = s.update(|e, _, _| e.settings.as_ref().unwrap().customize.as_ref().unwrap().preview.trash.get());
    assert!(trash.size.width > px(0.), "the trash circle is shown while a button is dragged");
    s.update(|e, _, cx| e.preview_pointer(trash.center(), cx));
    assert!(s.update(|e, _, _| e.settings.as_ref().unwrap().customize.as_ref().unwrap().preview.over_trash()));
    s.screenshot("trash");
    s.update(|e, _, cx| e.preview_release(cx));
    let removed = order(&mut s);
    assert_eq!(removed.len(), before.len() - 1);
    assert!(!removed.contains(&after[0]), "the button taken out is gone");
}

/// GitHub issue #237: a window remembered at a size or place the display cannot show is made to fit it (no larger than the display's usable
/// area, none of it off the screen), and the size and place are kept in the settings as `x,y,width,height`.
fn window_bounds_fit_the_display() {
    use crate::app::fit_to_display;
    let screen = (0., 25., 1440., 875.);
    // Fits as it was.
    assert_eq!(fit_to_display((100., 100., 1000., 700.), screen), (100., 100., 1000., 700.));
    // Too large: the display's size, at its corner.
    assert_eq!(fit_to_display((-300., -50., 4000., 3000.), screen), (0., 25., 1440., 875.));
    // Off to the right and below (a screen that is gone): moved in.
    assert_eq!(fit_to_display((3000., 2000., 800., 600.), screen), (640., 300., 800., 600.));
    // Too small to use: made usable.
    let (_, _, w, h) = fit_to_display((10., 40., 20., 20.), screen);
    assert!(w >= 480. && h >= 320., "{w} x {h}");
    // Kept and read back.
    let _fresh = Fresh::new("shell::issue237_window_bounds_fit_the_display");
    crate::app::keep_bounds(gpui_kit::Bounds::new(gpui_kit::point(gpui_kit::px(120.4), gpui_kit::px(80.)), gpui_kit::size(gpui_kit::px(900.), gpui_kit::px(640.6))));
    assert_eq!(crate::settings::get(crate::app::WINDOW_BOUNDS_KEY).as_deref(), Some("120,80,900,641"));
}

/// GitHub issue #237: a document or a presentation opens at the zoom its file has (Word's `w:zoom`, PowerPoint's slide view scale); a file with
/// none takes the zoom the last window was at; the zoom of the window is the model's `view_zoom` (the saving writes it).
fn zoom_follows_the_file_else_the_last_one() {
    let _fresh = Fresh::new("shell::issue237_zoom_follows_the_file_else_the_last_one");
    let root = crate::scenario::repo_root();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    let open = |s: &mut Scenario, file: &str, zoom: Option<u16>| {
        let mut presentation = ibis_formats::read(&std::fs::read(root.join(file)).unwrap()).unwrap();
        presentation.view_zoom = zoom;
        s.update(move |e, window, cx| e.load(presentation, "file".into(), window, cx));
        s.settle();
    };
    // The file's own zoom.
    open(&mut s, "test/editing/pptx-plan.pptx", Some(175));
    assert_eq!(s.with(|e, _| e.zoom), Some(1.75));
    // Changing it is the file's zoom (written at the save) and the next file's.
    s.update(|e, window, cx| e.set_zoom(Some(0.8), window, cx));
    assert_eq!(s.with(|e, _| e.presentation().view_zoom), Some(80));
    assert_eq!(crate::editor::last_zoom(), Some(Some(0.8)));
    // A document with none: the last one. (A presentation with none is fitted: see below.)
    open(&mut s, "test/editing/autospace-letters.docx", None);
    assert_eq!(s.with(|e, _| e.zoom), Some(0.8));
    // Fit the page is kept too.
    s.update(|e, window, cx| e.set_zoom(None, window, cx));
    assert_eq!(crate::editor::last_zoom(), Some(None));
    open(&mut s, "test/editing/autospace-letters.docx", None);
    assert_eq!(s.with(|e, _| e.zoom), None);
    // A presentation opens fitted to the window, whatever the last window was at and whatever its slide view says (`varScale` 1, 0 or none, `scale` 119 % or 60 %:
    // PowerPoint for Windows fits all of them, `tasks/doc/requests/2026-11-17-237-varscale-result.md`).
    s.update(|e, window, cx| e.set_zoom(Some(0.8), window, cx));
    let read = |file: &str| ibis_formats::read(&std::fs::read(root.join(file)).unwrap()).unwrap();
    let power_point = read("test/editing/pptx-plan.pptx");
    assert_eq!(power_point.view_zoom, None, "the scale is not the zoom");
    s.update(move |e, window, cx| e.load(power_point, "file".into(), window, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.zoom), None, "a PowerPoint file opens fitted");
    // (A zoom the editor was given, as after the user's own: the presentation's `view_zoom` is that.)
    let mut fixed = read("test/editing/pptx-plan.pptx");
    fixed.view_zoom = Some(119);
    s.update(move |e, window, cx| e.load(fixed, "file".into(), window, cx));
    s.settle();
    assert_eq!(s.with(|e, _| e.zoom), Some(1.19));
    // A presentation with none (no viewProps) is fitted too, not at the last zoom.
    open(&mut s, "test/editing/pptx-plan.pptx", None);
    assert_eq!(s.with(|e, _| e.zoom), None);
}

/// GitHub issue #241 (stage 4): クイック アクセス ツール バー is chosen in the same dialog (one order for the three formats, no scenes, no
/// separators), OK keeps it in `app.quick-access-toolbar`; and in a presentation a command put in with すべての場面に is put in every scene
/// whose own order is known.
fn toolbar_customize_quick_access_and_all_scenes() {
    use crate::toolbar_customize as customize;
    let _fresh = Fresh::new("shell::toolbar_customize_quick_access_and_all_scenes");
    let mut s = Scenario::open("test/editing/88-review-order.pptx");
    s.size(1200., 800.);
    s.settle();
    crate::qat::set(&["file.save".to_string()]);
    s.update(|e, window, cx| e.open_settings(dialog::CATEGORY_QAT, window, cx));
    s.settle();
    s.screenshot("page");
    s.update(|e, window, cx| e.open_toolbar_customize("qat", window, cx));
    s.settle();
    s.screenshot("qat-dialog");
    let (names, commands) = customize::catalog("qat");
    assert!(names.len() > 10 && commands.iter().all(|c| !c.submenu && c.key.starts_with("cmd:")), "the commands of the three formats' menus, no submenu");
    s.update(|e, _, _| {
        let c = e.settings.as_mut().unwrap().customize.as_mut().unwrap();
        assert_eq!(c.order(), ["cmd:file.save"], "the order is the one kept");
        assert!(c.add("cmd:file.print"));
        c.move_row(1, 0);
    });
    s.update(|e, _, _| e.settings.as_mut().and_then(|d| d.customize.take()).unwrap().save());
    assert_eq!(crate::qat::commands(), ["file.print", "file.save"]);
    // すべての場面に (a presentation).
    s.update(|e, window, cx| e.open_toolbar_customize("pptx", window, cx));
    s.settle();
    s.update(|e, _, _| {
        let c = e.settings.as_mut().unwrap().customize.as_mut().unwrap();
        c.all_scenes = true;
        let scenes = crate::toolbar_layout::scenes("pptx");
        let known: Vec<usize> = (0..scenes.len()).filter(|i| crate::toolbar_layout::seen_default("pptx", scenes[*i].id).is_some()).collect();
        assert!(known.len() >= 2, "several scenes have a known own order: {known:?}");
        assert!(c.add_choice("cmd:file.save"));
        for i in &known {
            assert!(c.orders[*i].as_ref().is_some_and(|o| o.iter().any(|k| k == "cmd:file.save")), "scene {i} has it");
        }
        // One step of the history takes it out of every scene.
        c.undo();
        assert!(c.orders.iter().all(Option::is_none));
    });
}

/// GitHub issue #241: what the search found is marked in the name (also what was typed in another script or width), and the preview folds
/// what does not fit into ︙.
fn toolbar_customize_highlight_and_fold() {
    use crate::toolbar_customize::highlight;
    assert_eq!(highlight("Hello World", "WORLD"), [("Hello ".to_string(), false), ("World".to_string(), true)]);
    assert_eq!(highlight("ひらがなカタカナ", "ヒラ カタ"), [("ひら".to_string(), true), ("がな".to_string(), false), ("カタ".to_string(), true), ("カナ".to_string(), false)]); // i18n-ok（検索の強調の試験データ（かなの区別なし）。迷った：試験用の入力）
    let widths = [28.; 10];
    assert_eq!(crate::toolbar_preview::fit(&widths, 0.), 10, "not drawn yet: all");
    assert_eq!(crate::toolbar_preview::fit(&widths, 2000.), 10, "all fit");
    let shown = crate::toolbar_preview::fit(&widths, 200.);
    assert!(shown < 10 && shown > 0, "some are in ︙: {shown}");
}

/// K0038: the menu ids that were renamed (merged into shared ones, abbreviations spelled out) are read from the settings of a person who
/// saved them under the old name: the クイック アクセス ツール バー's commands and a toolbar order's commands and submenus.
fn old_menu_ids_in_the_settings() {
    let _fresh = Fresh::new("shell::k0038_old_menu_ids_in_the_settings");
    crate::settings::set("app.quick-access-toolbar", "docx.accessibility-check, file.save,pptx.anim.options");
    crate::qat::reload();
    assert_eq!(crate::qat::commands(), ["review.accessibility-check", "file.save", "pptx.animation.options"]);
    crate::settings::set("toolbar.xlsx.cell", "cmd:xlsx.order-front,|,bold,menu:docx.xref,cmd:file.open");
    assert_eq!(crate::toolbar_layout::saved("xlsx", "cell"), Some(["cmd:arrange.bring-to-front", "|", "bold", "menu:docx.cross-reference", "cmd:file.open"].map(str::to_string).to_vec()));
}
