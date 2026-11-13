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
#[cfg(not(target_family = "wasm"))]
use crate::chrome::control_row;
use crate::editor::SlideEditor;
use crate::i18n::{self, Language, LanguageSetting, text};
use crate::icons::Material;
use crate::palette::rgb;

/// The categories on the left (the ids of their names).
const CATEGORIES: [&str; 6] = ["settings.general", "settings.save", "settings.trust", "settings.qat", "settings.cmyk", "settings.spreadsheet"];
/// The index of クイック アクセス ツール バー in [`CATEGORIES`].
pub(crate) const CATEGORY_QAT: usize = 3;
/// The index of CMYK のプロファイル in [`CATEGORIES`].
pub const CATEGORY_CMYK: usize = 4;
/// The index of スプレッドシート in [`CATEGORIES`].
pub(crate) const CATEGORY_SPREADSHEET: usize = 5;

/// The key of the left pane's width (px) in the app's settings file; an absent or unreadable value is [`NAV_WIDTH_DEFAULT`].
pub(crate) const NAV_WIDTH_KEY: &str = "settings.nav-width";
pub(crate) const NAV_WIDTH_DEFAULT: f32 = 180.;
pub(crate) const NAV_WIDTH_MIN: f32 = 120.;
pub(crate) const NAV_WIDTH_MAX: f32 = 320.;
/// The dialog's size when it opens (it can be moved and resized as the common dialogs are).
pub(crate) const DIALOG_WIDTH: f32 = 720.;
pub(crate) const DIALOG_HEIGHT: f32 = 480.;
/// The id of the add menu's list (the common searchable list), its width and the most rows it shows.
const ADD_MENU_ID: &str = "settings-qat-add-menu";
const ADD_MENU_WIDTH: f32 = 320.;
const ADD_MENU_ROWS: usize = 10;
/// The gap the common popover leaves between its anchor and the popup.
const POPOVER_OFFSET: f32 = 6.;

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
    /// The scroll and the row drag of the quick access table.
    pub(crate) qat_list: ListState,
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
pub(crate) const FORMULA_BAR_FONT_KEY: &str = "sheet.formula-bar-font";

/// The family chosen for the formula bar's text, if one is.
pub fn formula_bar_font() -> Option<String> {
    crate::settings::get(FORMULA_BAR_FONT_KEY).map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// The font list of 数式バーの英数フォント (the common font dropdown) and the text its button shows. Choosing a font keeps it at once and
/// draws every window again. The first item, 既定（アプリの書体）, takes the setting off (the formula bar is drawn in the app's font, `app_font`),
/// and is what the button shows while no font is chosen (not the app font's internal name) (GitHub issue #149).
pub(crate) fn formula_bar_font_menu(app_font: String) -> (String, Vec<MenuEntry>) {
    let chosen = formula_bar_font();
    let default_label = text("settings.spreadsheet.formula-bar-font-default");
    let current = chosen.clone().unwrap_or_else(|| default_label.to_string());
    let default_item: MenuEntry = MenuItem::new("settings-formula-bar-font-default", default_label)
        .checked(chosen.is_none())
        .on(|_, _, cx| {
            crate::settings::remove(FORMULA_BAR_FONT_KEY);
            cx.refresh_windows();
            cx.notify();
        })
        .into();
    let entries = crate::font_list::build_font_menu_entries("settings-formula-bar-font", vec![default_item], chosen.as_deref(), move |name, _, _, cx| {
        if name == app_font {
            crate::settings::remove(FORMULA_BAR_FONT_KEY);
        } else {
            crate::settings::set(FORMULA_BAR_FONT_KEY, name);
        }
        cx.refresh_windows();
        cx.notify();
    });
    (current, entries)
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
        self.settings = Some(SettingsDialog { category: category.min(CATEGORIES.len() - 1), nav_width: nav_width(), nav_drag: None, qat_list: ListState::default(), cmyk_found: None, cmyk_message: None });
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
            CATEGORY_SPREADSHEET => self.settings_scrolled(self.settings_spreadsheet(cx)),
            CATEGORY_CMYK => {
                let (found, message) = self.settings.as_ref().map(|s| (s.cmyk_found.clone().unwrap_or_default(), s.cmyk_message.clone())).unwrap_or_default();
                self.settings_scrolled(self.cmyk_profile_controls(&found, message, cx))
            }
            _ => self.settings_quick_access(&editor, cx),
        };
        let body = div()
            .relative()
            .size_full()
            .text_sm()
            .text_color(rgb(TEXT))
            .child(h_flex().size_full().items_stretch().child(nav).child(settings_splitter(&editor, cx)).child(v_flex().flex_1().min_w_0().h_full().child(detail)))
            .when(category == CATEGORY_QAT, |d| d.child(self.settings_add_menu(&editor, cx)));
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
        v_flex()
            .gap_2()
            .child(dropdown_row(text("settings.spreadsheet.formula-bar-font"), FontDropdown::new("settings-formula-bar-font", &current, entries, 0, &cx.weak_entity()).fill_width()))
            .child(panel_note(text("settings.spreadsheet.formula-bar-font-note")))
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

    /// クイック アクセス ツール バー: the commands of the title row, a table dragged to reorder, with 追加 at its top right.
    fn settings_quick_access(&mut self, editor: &WeakEntity<SlideEditor>, cx: &mut Context<Self>) -> AnyElement {
        let chosen = crate::qat::commands();
        let menus = self.menus();
        let list = self.settings.as_ref().map(|s| s.qat_list.clone()).unwrap_or_default();
        // The name and the icon of a command are the menus' (the registry of the open document's commands); a command only another
        // kind of document has is shown by its id's name, with room for the icon kept.
        let rows: Rc<Vec<(String, SharedString, Option<&'static str>)>> = Rc::new(
            chosen
                .iter()
                .map(|id| match crate::chrome::quick_access_item(&menus, id) {
                    Some(item) => (id.clone(), item.label.clone(), item.icon.filter(|n| !n.is_empty())),
                    None => (id.clone(), text(id), None),
                })
                .collect(),
        );
        let count = rows.len();
        let build = {
            let (rows, editor) = (rows.clone(), editor.clone());
            move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range
                    .filter_map(|i| rows.get(i).map(|r| (i, r)))
                    .map(|(i, (id, label, icon_name))| {
                        let name = h_flex()
                            .gap(px(8.))
                            .items_center()
                            .min_w_0()
                            .child(div().w(px(20.)).flex_none().children(icon_name.map(|n| icon(n, 18., rgb(TEXT).into()))))
                            .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                            .into_any_element();
                        let (editor, id) = (editor.clone(), id.clone());
                        TableRow::new(vec![name])
                            .draggable(i as u64, label.clone())
                            .drop_between(i as u64)
                            .action("settings-qat-delete", "delete", text("settings.qat.remove"), move |_, _, cx| {
                                crate::qat::remove(&id);
                                cx.refresh_windows();
                                let _ = editor.update(cx, |_, cx| cx.notify());
                            })
                    })
                    .collect()
            }
        };
        let table = DataTable::reorder_list("settings-qat-table", vec![TableColumn::flex("command")], vec![div().into_any_element()], &list, count, build, |_, from, to, _, cx| {
            if crate::qat::move_to(from, to) {
                cx.refresh_windows();
            }
            cx.notify();
        }, cx)
        .row_actions(1)
        .headerless()
        .empty(panel_note(text("settings.qat.empty")).p_3().into_any_element());
        let add = action_button("settings-qat-add").small().icon(Icon::new(Material("add"))).label(text("settings.qat.add")).on_click(cx.listener(|e, _, window, cx| e.open_settings_add_menu(window, cx)));
        let reset = action_button("settings-qat-reset").small().label(text("settings.qat.reset")).on_click(cx.listener(|_, _, _, cx| {
            crate::qat::reset();
            cx.refresh_windows();
            cx.notify();
        }));
        v_flex()
            .size_full()
            .gap_2()
            .child(panel_note(text("settings.qat.note")))
            .child(h_flex().items_center().justify_between().child(section_heading(text("settings.qat.order"))).child(h_flex().gap_2().child(add).child(reset)))
            .child(div().id("settings-qat-table-host").w_full().flex_1().min_h_0().child(table))
            .into_any_element()
    }

    /// The commands the add menu offers: every candidate not in the row yet, named and drawn as the menus have it (the icon is the
    /// menus' too), added at the end of the row by the click or by Enter.
    pub(crate) fn settings_add_items(&self) -> Vec<MenuItem> {
        let chosen = crate::qat::commands();
        let menus = self.menus();
        crate::qat::CANDIDATES
            .iter()
            .filter(|id| !chosen.iter().any(|c| c == *id))
            .map(|id| {
                let id: &'static str = id;
                let mut item = match crate::chrome::quick_access_item(&menus, id) {
                    Some(found) => {
                        let item = MenuItem::new(id, found.label.clone());
                        match found.icon.filter(|n| !n.is_empty()) {
                            Some(name) => item.icon(name),
                            None => item,
                        }
                    }
                    None => MenuItem::new(id, text(id)),
                };
                item.handler = Some(Rc::new(move |_, _, cx| {
                    crate::qat::add(id);
                    cx.refresh_windows();
                    cx.notify();
                }));
                item
            })
            .collect()
    }

    /// 追加: open the common searchable menu over the commands not in the row.
    fn open_settings_add_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.settings_add_items();
        self.open_searchable_list(ADD_MENU_ID, items, ListSpec { always_search: true, rows: Some(ADD_MENU_ROWS) }, window, cx);
    }

    /// The common searchable list (`Dropdown`, the menu search's list) of the add menu, with its anchor in the dialog's middle: the
    /// list opens below its anchor, which is put so that the list (the search field and `ADD_MENU_ROWS` rows at most) is centred
    /// vertically and horizontally in the dialog. The dialog's own button is the add button; the anchor is nothing to click.
    fn settings_add_menu(&self, editor: &WeakEntity<SlideEditor>, _cx: &mut Context<Self>) -> Div {
        let items = self.settings_add_items();
        let rows = items.len().clamp(1, ADD_MENU_ROWS) as f32;
        // The search field (a button's height and its frame) and the rows, in a frame.
        let height = crate::chrome::BUTTON_HEIGHT + 2. + rows * crate::chrome::dropdown_row_height(ADD_MENU_ID) + 2.;
        let bounds = self.dialog.bounds.map_or((DIALOG_WIDTH, DIALOG_HEIGHT), |b| (f32::from(b.size.width), f32::from(b.size.height)));
        // The body starts under the title (44 px), inside the frame (1 px) and the body's padding.
        let (inset_x, inset_y) = (1. + crate::chrome::FORM_PADDING, 1. + 44. + crate::chrome::FORM_PADDING);
        let left = bounds.0 / 2. - inset_x - ADD_MENU_WIDTH / 2.;
        // The popup stands 6 px below its anchor (the common popover's offset, forks/gpui-component/src/popover.rs).
        let top = bounds.1 / 2. - inset_y - height / 2. - POPOVER_OFFSET;
        let anchor = gpui_kit::component::button::Button::new("settings-qat-add-anchor").w(px(0.)).h(px(0.)).p_0();
        let entries: Vec<MenuEntry> = items.into_iter().map(MenuEntry::from).collect();
        div().absolute().left(px(left)).top(px(top)).child(Dropdown::with_button(ADD_MENU_ID, anchor, entries, false, 0, editor).list_width(ListWidth::Fixed(px(ADD_MENU_WIDTH))))
    }
}

/// The splitter between the categories and the detail: the pointer drags it to change the left pane's width (within
/// [`NAV_WIDTH_MIN`] and [`NAV_WIDTH_MAX`]); letting go keeps the width in the settings file.
fn settings_splitter(editor: &WeakEntity<SlideEditor>, cx: &mut Context<SlideEditor>) -> impl IntoElement {
    let tracker = editor.clone();
    div()
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
        )
}
