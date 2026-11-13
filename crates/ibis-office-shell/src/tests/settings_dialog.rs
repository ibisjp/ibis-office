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
        ("shell::settings_dialog_quick_access_table", quick_access_table),
        ("shell::settings_dialog_one_font_size", one_font_size),
        ("shell::settings_dialog_formula_bar_font_default", formula_bar_font_default),
        ("shell::settings_dialog_formula_bar_font_search_space", formula_bar_font_search_space),
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

/// The quick access table: rows dragged to reorder, the trash button, and 追加 → the searchable menu → Enter.
fn quick_access_table() {
    let _fresh = Fresh::new("shell::settings_dialog_quick_access_table");
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 800.);
    crate::qat::set(&["file.save".to_string(), "file.open".to_string(), "edit.copy".to_string(), "xlsx.freeze".to_string()]);
    s.update(|e, window, cx| e.open_settings(dialog::CATEGORY_QAT, window, cx));
    s.settle();
    let list = s.with(|e, _| e.settings.as_ref().unwrap().qat_list.clone());
    s.screenshot("table");
    let mid = |i: usize| {
        let b = list.row_bounds(i).unwrap_or_else(|| panic!("row {i}"));
        (f32::from(b.origin.x) + 60., f32::from(b.center().y), f32::from(b.size.height))
    };
    // Reorder: the first row dragged under the third.
    let (x, y0, h) = mid(0);
    let (_, y2, _) = mid(2);
    s.drag_through(&[(x, y0), (x, y0 + 6.), (x, y0 + h), (x, y2), (x, y2 + h / 2. - 2.)]);
    s.settle();
    assert_eq!(crate::qat::commands(), ["file.open", "edit.copy", "file.save", "xlsx.freeze"], "the row went down by dragging");
    s.screenshot("reordered");
    // Delete: the trash button of the second row ("edit.copy").
    let trash = buttons::bounds("settings-qat-table-settings-qat-delete-1").unwrap_or_else(|| panic!("the trash button: {:?}", buttons::all()));
    s.click((f32::from(trash.center().x), f32::from(trash.center().y)));
    s.settle();
    assert_eq!(crate::qat::commands(), ["file.open", "file.save", "xlsx.freeze"], "the row's trash button deleted it");
    // Add: the button opens the common searchable menu, centred in the dialog, with the commands' icons.
    buttons::click(&mut s, "settings-qat-add");
    s.settle();
    assert_eq!(s.with(|e, _| e.toolbar_search_open), Some("settings-qat-add-menu"), "the add menu is open");
    let dialog_center = s.with(|e, _| e.dialog.bounds.unwrap().center());
    s.screenshot("add-menu");
    let first = buttons::all().into_iter().find(|id| id.contains("searchable-item-settings-qat-add-menu") && id.ends_with(", 0)")).unwrap_or_else(|| panic!("the menu's first row: {:?}", buttons::all()));
    let first = buttons::bounds(&first).unwrap();
    // The list is the search field (above the first row) and ten rows: its middle is the dialog's.
    let top = f32::from(first.origin.y) - 30.;
    let middle = top + (30. + 10. * 28.) / 2.;
    assert!((middle - f32::from(dialog_center.y)).abs() < 12., "centred vertically: the list's middle {middle}, the dialog's {}", f32::from(dialog_center.y));
    // The icons are the menus' (the row's icon is drawn; a command the menus have has one).
    let icons = s.with(|e, _| e.settings_add_items().iter().filter(|i| i.icon.is_some()).count());
    assert!(icons > 0, "the commands the open document's menus have show their icons");
    // Search, Enter: the first match is added at the end of the row, and the menu closes.
    // (As the menu search's scenarios do: the field has the focus the list gave it once the window has drawn.)
    s.update(|e, w, cx| e.ensure_toolbar_search_input("settings-qat-add-menu", w, cx).update(cx, |state, cx| state.focus(w, cx)));
    s.type_text(&crate::i18n::text("edit.paste-plain"));
    s.settle();
    s.screenshot("add-menu-searched");
    s.key("enter");
    s.settle();
    assert_eq!(s.with(|e, _| e.toolbar_search_open), None, "the menu closed");
    assert_eq!(crate::qat::commands(), ["file.open", "file.save", "xlsx.freeze", "edit.paste-plain"], "Enter added the match");
    // Add with the pointer: click an item of the list.
    buttons::click(&mut s, "settings-qat-add");
    s.settle();
    let row = buttons::all().into_iter().find(|id| id.contains("searchable-item-settings-qat-add-menu") && id.ends_with(", 0)")).unwrap();
    let b = buttons::bounds(&row).unwrap();
    let before = crate::qat::commands().len();
    s.click((f32::from(b.center().x), f32::from(b.center().y)));
    s.settle();
    assert_eq!(crate::qat::commands().len(), before + 1, "a click added the clicked command");
    assert_eq!(s.with(|e, _| e.toolbar_search_open), None);
    // Esc with the menu open closes only the menu; the dialog stays.
    buttons::click(&mut s, "settings-qat-add");
    s.settle();
    s.key("escape");
    s.settle();
    assert!(is_open(&mut s), "the first Esc closed only the menu");
    assert_eq!(s.with(|e, _| e.toolbar_search_open), None);
    s.key("escape");
    s.settle();
    assert!(!is_open(&mut s));
    // Kept in the settings file.
    assert!(std::fs::read_to_string(crate::scenario::repo_root().join("target/scenario/shell::settings_dialog_quick_access_table/settings/settings.txt")).unwrap_or_default().contains("qat=file.open,file.save,xlsx.freeze,edit.paste-plain"));
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
    let has_line = || std::fs::read_to_string(&file).unwrap_or_default().lines().any(|l| l.starts_with("sheet.formula-bar-font="));
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
