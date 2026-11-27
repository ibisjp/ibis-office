// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツール > 設定… (⌘,): the app's settings, a modal dialog inside the window the user is in (the common dialog: the panel of
//! `SidebarPanel::modal_named("settings_dialog")`, not a window of its own), master–detail as the system's settings are: the
//! categories on the left, the selected one's settings on the right, with the splitter between them dragged by the pointer (the
//! left pane's width is kept in the app's settings file: `settings.nav-width`). 一般 > 言語 chooses the UI's language (the
//! system's, English or Japanese); every window shows it at once. The クイック アクセス ツール バー's commands are a reorderable
//! table (rows dragged, a trash button per row) with an add button that opens the common searchable menu, centred in the dialog.
use std::rc::Rc;

use gpui_kit::component::scroll::ScrollbarMode;
use gpui_kit::component::theme::Theme;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::chrome::data_table::{DataTable, ListState};
use crate::chrome::dropdown::{ListSpec, ListWidth};
use crate::chrome::virtual_table::{TableColumn, TableRow};
use crate::chrome::{
    BORDER, Dropdown, DropdownItem, FontDropdown, MenuEntry, MenuItem, SELECTED_ROW_BG, SidebarPanel, TEXT, action_button, checkbox_button, dropdown_row, icon, panel_note, section_heading,
};
use crate::chrome::control_row;
use crate::editor::SlideEditor;
use crate::i18n::{self, Language, LanguageSetting, text};
use crate::icons::Material;
use crate::palette::rgb;

/// The categories on the left (the ids of their names).
const CATEGORIES: [&str; 8] =
    ["settings.general", "settings.save", "settings.trust", "settings.qat", "settings.cmyk", "settings.document", "settings.spreadsheet", "settings.presentation"];
/// The categories from this one on are the formats' (the others are the whole app's): the list leaves a gap before them.
const FIRST_FORMAT_CATEGORY: usize = 5;
/// The index of クイック アクセス ツール バー in [`CATEGORIES`].
pub(crate) const CATEGORY_QAT: usize = 3;
/// The index of CMYK のプロファイル in [`CATEGORIES`].
pub const CATEGORY_CMYK: usize = 4;
/// The index of スプレッドシート in [`CATEGORIES`].
pub const CATEGORY_SPREADSHEET: usize = 6;
/// The index of ドキュメント and of プレゼンテーション.
pub(crate) const CATEGORY_DOCUMENT: usize = 5;
pub(crate) const CATEGORY_PRESENTATION: usize = 7;

/// The key of the left pane's width (px) in the app's settings file; an absent or unreadable value is [`NAV_WIDTH_DEFAULT`].
pub(crate) const NAV_WIDTH_KEY: &str = "settings.nav-width";
pub(crate) const NAV_WIDTH_DEFAULT: f32 = 180.;
pub(crate) const NAV_WIDTH_MIN: f32 = 120.;
pub(crate) const NAV_WIDTH_MAX: f32 = 320.;
/// The dialog's size when it opens (it can be moved and resized as the common dialogs are).
pub(crate) const DIALOG_WIDTH: f32 = 720.;
pub(crate) const DIALOG_HEIGHT: f32 = 480.;

/// The left pane's width kept in the settings file, within its limits.
pub(crate) fn nav_width() -> f32 {
    crate::settings::get(NAV_WIDTH_KEY).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).map_or(NAV_WIDTH_DEFAULT, |v| v.clamp(NAV_WIDTH_MIN, NAV_WIDTH_MAX))
}

/// Keep the left pane's width (whole pixels) in the settings file.
pub(crate) fn set_nav_width(width: f32) {
    crate::settings::set(NAV_WIDTH_KEY, &format!("{}", width.clamp(NAV_WIDTH_MIN, NAV_WIDTH_MAX).round() as i32));
}

/// What the dialog keeps while it is open (the editor's `settings`).
pub struct SettingsDialog {
    /// The category shown (0: 一般).
    pub(crate) category: usize,
    /// The left pane's width: read from the settings file when the dialog opens, written when the splitter is let go.
    pub(crate) nav_width: f32,
    /// The splitter being dragged: where the pointer was (x) and the width then.
    pub(crate) nav_drag: Option<(f32, f32)>,
    /// ツールバーのカスタマイズ while it is open (it is drawn instead of the settings).
    pub(crate) customize: Option<crate::toolbar_customize::ToolbarCustomize>,
    /// The CMYK profiles found in the OS's places (scanned when the category is first shown), and what the last choice said.
    pub cmyk_found: Option<Vec<crate::cmyk_profile::ProfileInfo>>,
    pub(crate) cmyk_message: Option<SharedString>,
}

/// ツール > 設定… (and the app menu's 設定…): open the dialog in the editor window the user is in (the active one, else any editor
/// window; with none open, a start window is opened for it). Deferred: the action comes while the window with the keys is being
/// updated, when its editor cannot be updated.
pub fn open(cx: &mut App) {
    open_at(0, cx)
}

/// As [`open`], with the category `category` shown.
pub(crate) fn open_at(category: usize, cx: &mut App) {
    cx.defer(move |cx| {
        let mut found = editor_window(cx);
        if found.is_none() {
            crate::app::open_editor(None, None, cx);
            found = editor_window(cx);
        }
        if let Some((handle, editor)) = found {
            let _ = handle.update(cx, |_, window, cx| {
                editor.update(cx, |editor, cx| editor.open_settings(category, window, cx));
                window.activate_window();
            });
        }
    });
}

/// The editor window to show the dialog in: the active window when it is one, else the first.
fn editor_window(cx: &mut App) -> Option<(AnyWindowHandle, Entity<SlideEditor>)> {
    use gpui_kit::component::Root;
    let of = |handle: AnyWindowHandle, cx: &mut App| -> Option<(AnyWindowHandle, Entity<SlideEditor>)> {
        let root = handle.downcast::<Root>()?;
        let editor = root.update(cx, |root, _, _| root.view().clone().downcast::<SlideEditor>().ok()).ok().flatten()?;
        Some((handle, editor))
    };
    if let Some(found) = cx.active_window().and_then(|w| of(w, cx)) {
        return Some(found);
    }
    cx.windows().into_iter().find_map(|w| of(w, cx))
}

/// Choose the UI's language: saved, and every window drawn again in it (the
/// app's own menu bar too).
pub fn choose_language(setting: LanguageSetting, cx: &mut App) {
    i18n::set_setting(setting);
    crate::app::set_app_menus(cx);
    cx.refresh_windows();
}

/// The name of a choice of language, in the UI's language.
fn choice_label(setting: LanguageSetting) -> SharedString {
    match setting {
        LanguageSetting::System => i18n::text("settings.language.system"),
        // A language's own name, as systems list them.
        LanguageSetting::Fixed(Language::English) => "English".into(),
        LanguageSetting::Fixed(Language::Japanese) => "日本語".into(), // i18n-ok（言語の自称（母語での表記）。English と並べて自称のまま。迷った：母語表記）
    }
}

/// The key of 起動時のリンクの確認 (the warning bar of a workbook that links others): off when `0`.
pub const LINKS_WARNING_KEY: &str = "trust.links-warning";

/// Whether a workbook with links to others shows the warning bar when it is opened.
pub fn links_warning_enabled() -> bool {
    crate::settings::get(LINKS_WARNING_KEY).as_deref() != Some("0")
}

/// The key of 数式バーの英数フォント (GitHub issue #149): the family the formula bar's text is drawn in; absent or empty is the app's font.
pub(crate) const FORMULA_BAR_FONT_KEY: &str = "xlsx.formula-bar.font";

/// The family chosen for the formula bar's text, if one is.
pub fn formula_bar_font() -> Option<String> {
    crate::settings::get(FORMULA_BAR_FONT_KEY).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// The font list of 数式バーの英数フォント (the common font dropdown) and the text its button shows. Choosing a font keeps it at once and
/// draws every window again. The first item, 既定（アプリの書体）, takes the setting off (the formula bar is drawn in the app's font, `app_font`),
/// and is what the button shows while no font is chosen (not the app font's internal name) (GitHub issue #149).
pub(crate) fn formula_bar_font_menu(app_font: String) -> (String, Vec<MenuEntry>) {
    font_setting_menu(FORMULA_BAR_FONT_KEY, "settings-formula-bar-font", "settings-formula-bar-font-default", text("settings.spreadsheet.formula-bar-font-default"), app_font)
}

/// The font list of a font setting kept under `key` (see [`formula_bar_font_menu`]); `default_label` is the first item, which takes the
/// setting off, and `app_font` the family that also means "off".
pub(crate) fn font_setting_menu(key: &'static str, id: &'static str, default_id: &'static str, default_label: SharedString, app_font: String) -> (String, Vec<MenuEntry>) {
    let chosen = crate::settings::get(key).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let current = chosen.clone().unwrap_or_else(|| default_label.to_string());
    let default_item: MenuEntry = MenuItem::new(default_id, default_label)
        .checked(chosen.is_none())
        .on(move |_, _, cx| {
            crate::settings::remove(key);
            cx.refresh_windows();
            cx.notify();
        })
        .into();
    let entries = crate::font_list::build_font_menu_entries(id, vec![default_item], chosen.as_deref(), move |name, _, _, cx| {
        if name == app_font {
            crate::settings::remove(key);
        } else {
            crate::settings::set(key, name);
        }
        cx.refresh_windows();
        cx.notify();
    });
    (current, entries)
}

/// The keys of the font a text file (.csv, .tsv, .txt) is shown in when it is opened (GitHub issue #243): the family and the size in points.
/// Absent is the usual (游ゴシック, 12 pt).
pub(crate) const CSV_FONT_KEY: &str = "xlsx.csv.font";
pub(crate) const CSV_FONT_SIZE_KEY: &str = "xlsx.csv.font-size";
/// The usual size of a text file's cells, and the limits of the size that can be chosen.
pub(crate) const CSV_FONT_SIZE_DEFAULT: f32 = 12.;
const CSV_FONT_SIZE_RANGE: (f32, f32) = (1., 409.);

/// The size chosen for a text file's cells (points), within its limits.
pub fn csv_cell_pt() -> f32 {
    crate::settings::get(CSV_FONT_SIZE_KEY).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).map_or(CSV_FONT_SIZE_DEFAULT, |v| v.clamp(CSV_FONT_SIZE_RANGE.0, CSV_FONT_SIZE_RANGE.1))
}

/// The font a text file is opened in: `None` while neither the family nor the size is chosen (the usual font, no extra work on open).
pub fn csv_font() -> Option<ibis_formats::csv::CsvFont> {
    let name = crate::settings::get(CSV_FONT_KEY).map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
    let size = crate::settings::get(CSV_FONT_SIZE_KEY).is_some();
    if name.is_none() && !size {
        return None;
    }
    Some(ibis_formats::csv::CsvFont { name: name.unwrap_or_else(|| ibis_formats::csv::DEFAULT_FONT.to_string()), size_pt: csv_cell_pt() })
}

/// 編集記号 (GitHub issue #262): the marks a document window always shows, as Word's オプション > 表示 > 常に画面に表示する編集記号 lists them. Each is
/// kept as `1` or `0` under `docx.marks.<name>`; the third is the default when the key is absent (the marks are shown at first: the
/// paragraph mark is what tells where a paragraph ends; 隠し文字 is not, as in Word, because it changes what the page shows).
pub const DOCX_MARKS: [(&str, &str, bool); 7] = [
    ("docx.marks.tabs", "docx.view.marks.tabs", true),
    ("docx.marks.spaces", "docx.view.marks.spaces", true),
    ("docx.marks.paragraph", "docx.view.marks.paragraph", true),
    ("docx.marks.line-breaks", "docx.view.marks.line-breaks", true),
    ("docx.marks.hidden-text", "docx.view.marks.hidden", false),
    ("docx.marks.optional-hyphens", "docx.view.marks.optional-hyphens", true),
    ("docx.marks.anchors", "docx.view.marks.anchors", true),
];

/// Whether the mark `key` (one of [`DOCX_MARKS`]) is always shown.
pub fn docx_mark_shown(key: &str) -> bool {
    let default = DOCX_MARKS.iter().find(|(k, _, _)| *k == key).is_some_and(|(_, _, d)| *d);
    match crate::settings::get(key).as_deref() {
        Some("1") => true,
        Some("0") => false,
        _ => default,
    }
}

/// How many times the marks' settings were changed here: the document windows read their marks again when it is not the number they
/// last saw (`docx_marks_generation`).
static DOCX_MARKS_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The marks' settings were changed (the dialog does this itself): the document windows read them again.
pub fn note_docx_marks_changed() {
    DOCX_MARKS_GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// The number of changes of the marks' settings so far.
pub fn docx_marks_generation() -> u64 {
    DOCX_MARKS_GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

/// The key of 限られたグリッド表示 (GitHub issue #273): on when `1`; off by default. On, a sheet shows its used range and a margin, and the grid grows as the selection nears its edge.
pub const LIMITED_GRID_KEY: &str = "xlsx.grid.limited";

/// What the setting was when last read: 0 not read, 1 off, 2 on (the sheets ask on every frame, so the file is read once until the dialog changes it).
static LIMITED_GRID_CACHE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

/// Whether the sheets show the limited grid.
pub fn limited_grid() -> bool {
    use std::sync::atomic::Ordering::Relaxed;
    match LIMITED_GRID_CACHE.load(Relaxed) {
        0 => {
            let on = crate::settings::get(LIMITED_GRID_KEY).as_deref() == Some("1");
            LIMITED_GRID_CACHE.store(if on { 2 } else { 1 }, Relaxed);
            on
        }
        v => v == 2,
    }
}

/// Turn the limited grid on or off (kept, and shown at once).
pub fn set_limited_grid(on: bool, cx: &mut App) {
    crate::settings::set(LIMITED_GRID_KEY, if on { "1" } else { "0" });
    note_limited_grid_changed();
    cx.refresh_windows();
}

/// The setting was changed outside `set_limited_grid`: read it again.
pub fn note_limited_grid_changed() {
    LIMITED_GRID_CACHE.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// The key of the number of sheets from which a workbook's sheet list has its search field (GitHub issue #262). Absent is
/// [`SHEET_LIST_SEARCH_MIN_DEFAULT`] (a list of more than 20 sheets).
pub const SHEET_LIST_SEARCH_MIN_KEY: &str = "xlsx.sheet-list.search-min";
pub const SHEET_LIST_SEARCH_MIN_DEFAULT: usize = 21;
const SHEET_LIST_SEARCH_MIN_RANGE: (f32, f32) = (1., 1000.);

/// The number of sheets from which the sheet list has a search field.
pub fn sheet_list_search_min() -> usize {
    crate::settings::get(SHEET_LIST_SEARCH_MIN_KEY)
        .and_then(|v| v.trim().parse::<usize>().ok())
        .map_or(SHEET_LIST_SEARCH_MIN_DEFAULT, |n| n.clamp(SHEET_LIST_SEARCH_MIN_RANGE.0 as usize, SHEET_LIST_SEARCH_MIN_RANGE.1 as usize))
}

/// Whether a dropdown list `id` of `count` rows has its search field: more than 20 rows, except the sheet list (the number is a setting).
pub fn list_has_search(id: &str, count: usize) -> bool {
    if id == "sheet-list" { count >= sheet_list_search_min() } else { count > 20 }
}

/// The key of スクロール バーを常に表示: on when `1` (on by default; an explicitly saved 0 still means hover).
pub const SCROLLBARS_ALWAYS_KEY: &str = "view.scrollbars-always";

/// Whether the scrollbars of the windows are always shown.
pub fn scrollbars_always() -> bool {
    crate::settings::get(SCROLLBARS_ALWAYS_KEY).as_deref() != Some("0")
}

/// The scrollbars' mode of every window as the setting says (at start, and when it is changed).
pub fn apply_scrollbar_mode(cx: &mut App) {
    apply_scrollbar_mode_only(cx);
    cx.refresh_windows();
}

/// As `apply_scrollbar_mode`, without redrawing (the theme's change redraws).
pub(crate) fn apply_scrollbar_mode_only(cx: &mut App) {
    // Hidden until the view scrolls or the pointer is over one (not the system's setting, which may show them always), unless asked to stay.
    Theme::set_scrollbar_mode(if scrollbars_always() { ScrollbarMode::Always } else { ScrollbarMode::Hover }, cx);
}



impl SlideEditor {
    /// Show the settings dialog (modal) with the category `category` selected.
    pub fn open_settings(&mut self, category: usize, _window: &mut Window, cx: &mut Context<Self>) {
        crate::render::shape_view::ensure_system_font_names(cx);
        self.settings = Some(SettingsDialog { category: category.min(CATEGORIES.len() - 1), nav_width: nav_width(), nav_drag: None, customize: None, cmyk_found: None, cmyk_message: None });
        self.sidebar_open = true;
        self.dialog.bounds = None;
        self.dialog.drag = None;
        self.toolbar_search_open = None;
        cx.notify();
    }

    /// The dialog's panel while it is open (the common dialog draws its title, close button, 閉じる and Esc).
    pub(crate) fn settings_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        if self.settings.as_ref().is_some_and(|s| s.category == CATEGORY_CMYK) {
            self.scan_for_settings(cx);
        }
        let state = self.settings.as_ref()?;
        let category = state.category.min(CATEGORIES.len() - 1);
        let nav_width = state.nav_width;
        let editor = cx.weak_entity();
        let nav = v_flex().w(px(nav_width)).flex_none().h_full().gap(px(2.)).children(CATEGORIES.iter().enumerate().map(|(i, key)| {
            let selected = i == category;
            div()
                .id(("settings-category", i))
                .when(i == FIRST_FORMAT_CATEGORY, |d| d.mt(px(10.)))
                .px(px(8.))
                .py(px(5.))
                .rounded(px(4.))
                .cursor_pointer()
                .when(selected, |d| d.bg(rgb(SELECTED_ROW_BG)))
                .when(!selected, |d| d.hover(|s| s.bg(crate::palette::shade(0.0, 0.05))))
                .child(text(key))
                .on_click(cx.listener(move |e, _, _, cx| {
                    if let Some(s) = e.settings.as_mut() {
                        s.category = i;
                    }
                    cx.notify();
                }))
        }));
        let detail = match category {
            0 => self.settings_scrolled(self.settings_general(&editor, cx)),
            1 => self.settings_scrolled(self.settings_saving(cx)),
            2 => self.settings_scrolled(self.settings_trust(cx)),
            CATEGORY_DOCUMENT => self.settings_scrolled(self.settings_toolbar_page(CATEGORY_DOCUMENT, "docx", cx)),
            CATEGORY_PRESENTATION => self.settings_scrolled(self.settings_toolbar_page(CATEGORY_PRESENTATION, "pptx", cx)),
            CATEGORY_SPREADSHEET => self.settings_scrolled(self.settings_spreadsheet(cx)),
            CATEGORY_CMYK => {
                let (found, message) = self.settings.as_ref().map(|s| (s.cmyk_found.clone().unwrap_or_default(), s.cmyk_message.clone())).unwrap_or_default();
                self.settings_scrolled(self.cmyk_profile_controls(&found, message, cx))
            }
            _ => self.settings_scrolled(self.settings_qat_page(cx)),
        };
        let body = div()
            .relative()
            .size_full()
            .text_sm()
            .text_color(rgb(TEXT))
            .child(h_flex().size_full().items_stretch().child(nav).child(settings_splitter(&editor, self.splitter_mark(), cx)).child(v_flex().flex_1().min_w_0().h_full().child(detail)))
;
        Some(SidebarPanel { title: text("settings.title"), body: body.into_any_element() })
    }

    /// A category's controls, scrolling inside the detail pane when they do not fit.
    fn settings_scrolled(&self, body: Div) -> AnyElement {
        div().id("settings-detail-scroll").flex_1().min_h_0().overflow_y_scroll().child(body).into_any_element()
    }

    /// 一般: the language, the scroll bars, the update check.
    fn settings_general(&self, editor: &WeakEntity<SlideEditor>, cx: &mut Context<Self>) -> Div {
        let current = i18n::setting();
        let choices = [LanguageSetting::System, LanguageSetting::Fixed(Language::English), LanguageSetting::Fixed(Language::Japanese)];
        let items = choices
            .into_iter()
            .map(|choice| DropdownItem::new(choice_label(choice)).checked(choice == current).on_select(Rc::new(move |_, _, cx| choose_language(choice, cx))))
            .collect();
        let language = dropdown_row(text("settings.language"), Dropdown::new("settings-language", choice_label(current), items, editor).fill_width());
        let body = v_flex()
            .gap_2()
            .child(language)
            .child(panel_note(text("settings.language.note")))
            .child(checkbox_button("settings-scrollbars-always", text("settings.view.scrollbars-always"), scrollbars_always(), false, cx.listener(|_, _, _, cx| {
                crate::settings::set(SCROLLBARS_ALWAYS_KEY, if scrollbars_always() { "0" } else { "1" });
                apply_scrollbar_mode(cx);
                cx.notify();
            })));
        // 起動時に更新を確かめる (no update in the browser).
        #[cfg(not(target_family = "wasm"))]
        let body = body
            .child(checkbox_button("settings-check-updates", text("fix.update.setting"), crate::update::check_enabled(), false, cx.listener(|_, _, _, cx| {
                crate::update::set_check_enabled(!crate::update::check_enabled());
                cx.notify();
            })))
            .child(panel_note(text("fix.update.setting.note")));
        body
    }

    /// スプレッドシート: the formula bar's font.
    fn settings_spreadsheet(&self, cx: &mut Context<Self>) -> Div {
        let (current, entries) = formula_bar_font_menu(cx.theme().font_family.to_string());
        let (csv_current, csv_entries) = font_setting_menu(CSV_FONT_KEY, "settings-csv-font", "settings-csv-font-default", text("settings.spreadsheet.csv-font-default"), String::new());
        v_flex()
            .gap_2()
            .child(dropdown_row(text("settings.spreadsheet.formula-bar-font"), FontDropdown::new("settings-formula-bar-font", &current, entries, 0, &cx.weak_entity()).fill_width()))
            .child(panel_note(text("settings.spreadsheet.formula-bar-font-note")))
            .child(section_heading(text("settings.spreadsheet.csv")))
            .child(dropdown_row(text("settings.spreadsheet.csv-font"), FontDropdown::new("settings-csv-font", &csv_current, csv_entries, 0, &cx.weak_entity()).fill_width()))
            .child(control_row(
                text("settings.spreadsheet.csv-font-size"),
                crate::chrome::number_stepper("settings-csv-font-size", csv_cell_pt(), cx, |_, size, _, cx| {
                    crate::settings::set(CSV_FONT_SIZE_KEY, &format!("{}", (size * 10.).round() / 10.));
                    cx.notify();
                })
                .range(CSV_FONT_SIZE_RANGE.0, CSV_FONT_SIZE_RANGE.1)
                .step(1.)
                .decimals(1)
                .unit(text("settings.spreadsheet.csv-font-size-unit")),
            ))
            .child(panel_note(text("settings.spreadsheet.csv-note")))
            .child(section_heading(text("settings.spreadsheet.sheet-list")))
            .child(control_row(
                text("settings.spreadsheet.sheet-list-search-min"),
                crate::chrome::number_stepper("settings-sheet-list-search-min", sheet_list_search_min() as f32, cx, |_, n, _, cx| {
                    crate::settings::set(SHEET_LIST_SEARCH_MIN_KEY, &format!("{}", n.round() as usize));
                    cx.notify();
                })
                .range(SHEET_LIST_SEARCH_MIN_RANGE.0, SHEET_LIST_SEARCH_MIN_RANGE.1)
                .step(1.)
                .decimals(0)
                .unit(text("settings.spreadsheet.sheet-list-search-min-unit")),
            ))
            .child(panel_note(text("settings.spreadsheet.sheet-list-note")))
            .child(section_heading(text("settings.spreadsheet.grid")))
            .child(checkbox_button("settings-limited-grid", text("settings.spreadsheet.limited-grid"), limited_grid(), false, cx.listener(|_, _, _, cx| {
                set_limited_grid(!limited_grid(), cx);
                cx.notify();
            })))
            .child(panel_note(text("settings.spreadsheet.limited-grid-note")))
            .child(self.settings_toolbar_section("xlsx", cx))
    }

    /// ドキュメント and プレゼンテーション: the format's settings, so far the toolbar's.
    fn settings_toolbar_page(&self, _category: usize, ext: &'static str, cx: &mut Context<Self>) -> Div {
        let page = v_flex().gap_2();
        let page = if ext == "docx" { page.child(self.settings_marks_section(cx)) } else { page };
        page.child(self.settings_toolbar_section(ext, cx))
    }

    /// 編集記号 of the document's page: which marks are always shown (Word's オプション > 表示), kept for every document window.
    fn settings_marks_section(&self, cx: &mut Context<Self>) -> Div {
        let section = v_flex().gap_2().child(section_heading(text("settings.document.marks"))).child(panel_note(text("settings.document.marks-note")));
        DOCX_MARKS.iter().enumerate().fold(section, |section, (i, (key, label, _))| {
            let key: &'static str = key;
            section.child(checkbox_button(SharedString::from(format!("settings-marks-{i}")), text(label), docx_mark_shown(key), false, cx.listener(move |_, _, _, cx| {
                crate::settings::set(key, if docx_mark_shown(key) { "0" } else { "1" });
                note_docx_marks_changed();
                cx.refresh_windows();
                cx.notify();
            })))
        })
    }

    /// The toolbar's section of a format's page: the button that opens ツールバーのカスタマイズ, and whether the toolbar is the
    /// format's own or customized.
    fn settings_toolbar_section(&self, ext: &'static str, cx: &mut Context<Self>) -> Div {
        let state = if crate::toolbar_customize::is_customized(ext) { text("settings.toolbar.customized") } else { text("settings.toolbar.default") };
        v_flex()
            .gap_2()
            .mt_2()
            .child(section_heading(text("settings.toolbar")))
            .child(panel_note(text("settings.toolbar.note")))
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(action_button(SharedString::from(format!("settings-toolbar-customize-{ext}"))).label(text("settings.toolbar.customize")).on_click(cx.listener(move |e, _, window, cx| e.open_toolbar_customize(ext, window, cx))))
                    .child(panel_note(state)),
            )
    }

    /// 保存: AutoRecover, and the backup copy of a workbook saved over.
    fn settings_saving(&self, cx: &mut Context<Self>) -> Div {
        #[cfg(not(target_family = "wasm"))]
        {
            use crate::autorecover::{load_autorecover_config, save_autorecover_config};
            let config = load_autorecover_config();
            let backup = crate::backup_save::load_backup_save_config();
            return v_flex()
                .gap_2()
                .child(checkbox_button("settings-autorecover", text("settings.save.autorecover"), config.enabled, false, cx.listener(|_, _, _, cx| {
                    let mut c = load_autorecover_config();
                    c.enabled = !c.enabled;
                    save_autorecover_config(&c);
                    cx.notify();
                })))
                .child(control_row(
                    text("settings.save.interval"),
                    crate::chrome::number_stepper("settings-interval", config.interval_minutes as f32, cx, |_, minutes, _, cx| {
                        let mut c = load_autorecover_config();
                        c.interval_minutes = (minutes.round() as u32).clamp(1, 120);
                        save_autorecover_config(&c);
                        cx.notify();
                    })
                    .range(1., 120.)
                    .step(1.)
                    .decimals(0)
                    .unit(text("settings.save.minutes")),
                ))
                .child(checkbox_button("settings-keep-unsaved", text("settings.save.keep-unsaved"), config.keep_last_unsaved, false, cx.listener(|_, _, _, cx| {
                    let mut c = load_autorecover_config();
                    c.keep_last_unsaved = !c.keep_last_unsaved;
                    save_autorecover_config(&c);
                    cx.notify();
                })))
                .child(checkbox_button("settings-backup", text("settings.save.backup"), backup.always_create_backup, false, cx.listener(|_, _, _, cx| {
                    let mut c = crate::backup_save::load_backup_save_config();
                    c.always_create_backup = !c.always_create_backup;
                    let _ = crate::backup_save::save_backup_save_config(&c);
                    cx.notify();
                })))
                .child(panel_note(text("settings.save.note")));
        }
        #[cfg(target_family = "wasm")]
        {
            let _ = cx;
            v_flex().gap_2().child(panel_note(text("settings.save.web")))
        }
    }

    /// セキュリティ センター.
    fn settings_trust(&self, cx: &mut Context<Self>) -> Div {
        v_flex()
            .gap_2()
            .child(checkbox_button("settings-links-warning", text("settings.trust.links-warning"), links_warning_enabled(), false, cx.listener(|_, _, _, cx| {
                crate::settings::set(LINKS_WARNING_KEY, if links_warning_enabled() { "0" } else { "1" });
                cx.notify();
            })))
            .child(panel_note(text("settings.trust.links-note")))
            .child(div().mt_2().child(section_heading(text("settings.trust.macros"))))
            .child(panel_note(text("settings.trust.macros-note")))
    }

    /// クイック アクセス ツール バー: the commands of the title row, chosen in ツールバーのカスタマイズ (one order for every format).
    fn settings_qat_page(&self, cx: &mut Context<Self>) -> Div {
        let state = if crate::qat::commands().is_empty() { text("settings.toolbar.default") } else { text("settings.toolbar.customized") };
        v_flex()
            .gap_2()
            .child(panel_note(text("settings.qat.note")))
            .child(
                h_flex()
                    .gap_3()
                    .items_center()
                    .child(action_button("settings-qat-customize").label(text("settings.qat.customize")).on_click(cx.listener(|e, _, window, cx| e.open_toolbar_customize("qat", window, cx))))
                    .child(panel_note(state)),
            )
    }
}

/// The splitter between the categories and the detail: the pointer drags it to change the left pane's width (within
/// [`NAV_WIDTH_MIN`] and [`NAV_WIDTH_MAX`]); letting go keeps the width in the settings file.
fn settings_splitter(editor: &WeakEntity<SlideEditor>, mark: impl IntoElement, cx: &mut Context<SlideEditor>) -> impl IntoElement {
    let tracker = editor.clone();
    let splitter = div()
        .id("settings-splitter")
        .relative()
        .w(px(9.))
        .h_full()
        .flex_none()
        .cursor_col_resize()
        .flex()
        .justify_center()
        .child(div().w(px(1.)).h_full().bg(rgb(BORDER)))
        .on_mouse_down(MouseButton::Left, cx.listener(|e, event: &MouseDownEvent, _, cx| {
            if let Some(s) = e.settings.as_mut() {
                s.nav_drag = Some((f32::from(event.position.x), s.nav_width));
            }
            cx.stop_propagation();
        }))
        // (A finger within reach of it takes hold of it too: K0084.)
        .child(mark)
        .child(
            canvas(
                |_, _, _| (),
                move |_, _, window, _| {
                    let moving = tracker.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = moving.update(cx, |e, cx| {
                                if let Some(s) = e.settings.as_mut()
                                    && let Some((x0, w0)) = s.nav_drag
                                {
                                    s.nav_width = (w0 + f32::from(event.position.x) - x0).clamp(NAV_WIDTH_MIN, NAV_WIDTH_MAX);
                                    cx.notify();
                                }
                            });
                        }
                    });
                    let released = tracker.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = released.update(cx, |e, cx| {
                                if let Some(s) = e.settings.as_mut()
                                    && s.nav_drag.take().is_some()
                                {
                                    set_nav_width(s.nav_width);
                                    cx.notify();
                                }
                            });
                        }
                    });
                },
            )
            .absolute()
            .size_0(),
        );
    // The scenario tests (K0084) read where it is drawn.
    #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux", windows)))]
    let splitter = splitter.child(canvas(|_, _, _| (), |bounds, _, _, _| crate::scenario::button_probe::record_bounds("settings-splitter", bounds)).absolute().inset_0());
    splitter
}

#[cfg(test)]
mod tests_262 {
    // Not `use super::*`: it brings gpui's own `test` attribute in, which does not expand here.
    use super::{DOCX_MARKS, SHEET_LIST_SEARCH_MIN_KEY, docx_mark_shown, list_has_search, sheet_list_search_min};
    use crate::chrome::MenuItem;

    /// The two tests set settings in a settings file of their own (not the user's), one at a time.
    fn isolated() -> std::sync::MutexGuard<'static, ()> {
        static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = ONE.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/test-settings-262"));
        let _ = std::fs::create_dir_all(&dir);
        // SAFETY: only these tests read this variable, and they hold the lock above.
        unsafe { std::env::set_var("IBIS_SETTINGS_DIR", &dir) };
        guard
    }

    /// GitHub issue #262: the sheet list has its search box from the number of sheets the setting says (21 when it is not set), and a search by
    /// full-width letters and digits finds the half-width name (and the other way round, and in the other case).
    #[test]
    fn the_sheet_list_search_follows_its_setting_and_ignores_width_and_case() {
        let _one = isolated();
        let key = SHEET_LIST_SEARCH_MIN_KEY;
        crate::settings::remove(key);
        assert_eq!(sheet_list_search_min(), 21);
        assert!(!list_has_search("sheet-list", 20) && list_has_search("sheet-list", 21));
        crate::settings::set(key, "1");
        assert!(list_has_search("sheet-list", 1), "1 shows it always");
        crate::settings::set(key, "5");
        assert!(!list_has_search("sheet-list", 4) && list_has_search("sheet-list", 5));
        crate::settings::set(key, "0");
        assert_eq!(sheet_list_search_min(), 1, "at least 1");
        crate::settings::set(key, "abc");
        assert_eq!(sheet_list_search_min(), 21, "unreadable: the default");
        crate::settings::remove(key);
        // The other lists keep their rule: more than 20 rows.
        assert!(!list_has_search("other", 20) && list_has_search("other", 21));

        let items: Vec<MenuItem> = ["Sheet1", "売上Ａ", "Report 2026", "ＫＥＹ"].iter().enumerate().map(|(i, name)| MenuItem::new(["a", "b", "c", "d"][i], *name)).collect();
        let found = |query: &str| crate::chrome::filter_menu_items(&items, query).iter().map(|i| i.label.to_string()).collect::<Vec<_>>();
        assert_eq!(found("ｓｈｅｅｔ１"), ["Sheet1"], "full-width query, half-width name");
        assert_eq!(found("ＳＨＥＥＴ"), ["Sheet1"], "and upper case");
        assert_eq!(found("売上A"), ["売上Ａ"], "half-width query, full-width name");
        assert_eq!(found("key"), ["ＫＥＹ"]);
        assert_eq!(found("２０２６"), ["Report 2026"]);
        assert_eq!(found("report　2026"), ["Report 2026"], "the ideographic space");
        assert!(found("zzz").is_empty());
    }

    /// The marks' settings: the defaults, and each key's own value.
    #[test]
    fn the_marks_settings_have_defaults_and_each_its_own_key() {
        let _one = isolated();
        for (key, _, default) in DOCX_MARKS {
            crate::settings::remove(key);
            assert_eq!(docx_mark_shown(key), default, "{key} by default");
            crate::settings::set(key, if default { "0" } else { "1" });
            assert_eq!(docx_mark_shown(key), !default, "{key} set");
            crate::settings::remove(key);
        }
        assert!(!docx_mark_shown("docx.marks.hidden-text"), "hidden text is not shown by default");
        assert!(docx_mark_shown("docx.marks.paragraph"));
    }
}
