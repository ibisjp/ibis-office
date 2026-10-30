// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The window's chrome, as the UI mock has it (`tasks/doc/design/toolbar.html`):
//! the title row (36 px), the menu bar (28 px), the one-row toolbar (44 px)
//! and the sidebar on the right; the status bar is in `status_bar.rs`.
//!
//! What the menus, the toolbar and the sidebar hold is registered, not
//! written here. The shell builds each menu with the items every format
//! shares (and placeholders for the rest), then hands the menus to the
//! format (`FormatEditor::menus`), which adds items or connects them. The
//! toolbar's right half and the sidebar are the format's
//! (`FormatEditor::toolbar_right`, `FormatEditor::sidebar`); until a format
//! gives its own, `chrome_defaults.rs` has one per format made of what the
//! editor can already do.
//!
//! Registration, from a format's `FormatEditor` implementation:
//!
//! ```ignore
//! fn menus(editor: &SlideEditor, menus: &mut Menus) {
//!     // Connect a placeholder the shell put in (see `chrome_defaults.rs` for the ids)…
//!     menus.connect("xlsx.insert-row-above", |editor, window, cx| editor.insert_rows(true, window, cx));
//!     // …or add an item.
//!     menus.items_mut(MenuId::Data).push(MenuItem::tr("xlsx.sort").icon("sort").on(…).into());
//! }
//! fn toolbar_right(editor: &SlideEditor) -> Option<Vec<ToolItem>> {
//!     Some(vec![ToolItem::Separator, ToolButton::new("bold", "format_bold", "_shell.bold-b").on(bold).click(…).into()])
//! }
//! ```
//!
//! An item with no handler is shown disabled, so a placeholder is visible
//! before it works.

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;

mod form;
pub(crate) mod font_size;
pub(crate) const TOOLBAR_CARET_WIDTH:f32=16.;
pub(crate) const CONTROL_INDENT:f32=14.;
pub(crate) use form::FORM_PADDING;
pub(crate) mod dropdown_list;
pub(crate) mod dropdown;
pub(crate) use dropdown::{Dropdown,DropdownItem,FontDropdown,ColorPaletteDropdown};
mod scrollbar;
pub(crate) mod action_bar;
pub(crate) use action_bar::{ActionBar, ActionDefinition};
pub(crate) mod preview_dropdown;
pub(crate) mod gradient_slider;
pub(crate) mod collapsible_section;
pub(crate) use collapsible_section::collapsible_section;
pub(crate) mod control_row;
pub(crate) use control_row::{SegmentControlRow, control_row, text_field_row, color_field_row, color_text_field_row, dropdown_row, segment_control_row};
mod tab_control;
pub(crate) mod segment_control;
pub(crate) use tab_control::{tab_control,tab_control_with_mode,TabMode};
pub(crate) mod panel;
pub(crate) mod data_table;
mod sheet_preview;
pub(crate) use sheet_preview::sheet_preview;
pub(crate) use form::{table_list,table_fill,panel_form, panel_message, table, table_row,table_row_with_actions};

use crate::shell::editor::SlideEditor;
use crate::shell::format::with_format;
use crate::shell::icons::Material;

// ----- Colours and sizes of the mock ---------------------------------------

pub(crate) const APP_BG: u32 = 0xf8fafd;
/// The background of the primary button (the main action of a panel or dialog: 適用, OK, 挿入, 保存…), one colour for the
/// whole app. The text on it is white. A button is made primary with `.primary()`; this sets what that looks like.
pub(crate) const PRIMARY_BUTTON_BG: u32 = 0x2d599f;
/// The primary button's background under the pointer and while pressed.
const PRIMARY_BUTTON_HOVER_BG: u32 = 0x3a6bb8;
const PRIMARY_BUTTON_ACTIVE_BG: u32 = 0x234a85;
/// The icon in a menu row is drawn this many times larger than the menu's own icon size (12 px).
pub(crate) const MENU_ICON_SCALE: f32 = 1.4;
pub(crate) const TOOLBAR_BG: u32 = 0xedf2f7;
const ICON: u32 = 0x444746;
pub(super) const TEXT: u32 = 0x1f1f1f;
pub(crate) const TEXT_SECONDARY: u32 = 0x5f6368;
const DIVIDER: u32 = 0xc7c7c7;
pub(crate) const BORDER: u32 = 0xdadce0;
pub(crate) const SIDEBAR_WIDTH: f32 = 300.0;

/// Padding at the left of the title row. On macOS, leaves space for the native
/// traffic light buttons (close, minimize, fullscreen). On other platforms, standard padding.
#[cfg(target_os = "macos")]
pub(crate) const TITLE_ROW_PADDING_LEFT: f32 = 78.0;
#[cfg(not(target_os = "macos"))]
pub(crate) const TITLE_ROW_PADDING_LEFT: f32 = 14.0;

/// How far from the window's left edge the toolbar starts (points). 8 everywhere, except on a Mac with the title row and the menus hidden
/// (`compact`): then the toolbar is at the window's top, under the traffic lights (gpui puts them at 9 pt from the left and the top, three
/// buttons of 12 pt with 8 pt between: they end at 61 pt), so it starts where the title row's content does (`TITLE_ROW_PADDING_LEFT`,
/// 78 pt). Full screen hides the buttons.
pub(crate) fn toolbar_left_inset(compact: bool, fullscreen: bool) -> f32 {
    if cfg!(target_os = "macos") && compact && !fullscreen { TITLE_ROW_PADDING_LEFT } else { 8.0 }
}

fn disabled_color() -> Hsla {
    crate::shell::palette::shade(0.12, 0.38)
}

// ----- What is registered --------------------------------------------------

/// What a menu item or toolbar button does.
pub(crate) type Handler = Rc<dyn Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>)>;

/// The menus of the menu bar, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MenuId {
    File,
    Edit,
    View,
    Insert,
    Format,
    Data,
    Slide,
    Arrange,
    Tools,
    Window,
    Help,
}

impl MenuId {
    /// Its key in the tables of the UI's words.
    pub(crate) fn key(self) -> &'static str {
        match self {
            MenuId::File => "menu.file",
            MenuId::Edit => "menu.edit",
            MenuId::View => "menu.view",
            MenuId::Insert => "menu.insert",
            MenuId::Format => "menu.format",
            MenuId::Data => "menu.data",
            MenuId::Slide => "menu.slide",
            MenuId::Arrange => "menu.arrange",
            MenuId::Tools => "menu.tools",
            MenuId::Window => "menu.window",
            MenuId::Help => "menu.help",
        }
    }

    /// Its name in the UI's language.
    pub(crate) fn label(self) -> SharedString {
        crate::shell::i18n::text(self.key())
    }
}

/// A line of a menu.
#[derive(Clone)]
pub(crate) enum MenuEntry {
    Item(MenuItem),
    Separator,
    Submenu(Submenu),
    /// The squares of a colour palette (`color_palette.rs`).
    Palette(crate::shell::color_palette::Palette),
}

/// A menu item. It is enabled when it has a handler and is not disabled.
#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct MenuItem {
    /// For finding it (`Menus::connect`), such as `"xlsx.insert-row-above"`.
    pub id: &'static str,
    pub icon: Option<&'static str>,
    /// Optional colour swatch drawn at the left of the row (e.g. sheet tab colour in #98).
    pub swatch: Option<ibis_doc_model::Rgb>,
    pub label: SharedString,
    /// The shortcut shown, such as `"⌘S"`.
    pub shortcut: Option<&'static str>,
    pub checked: bool,
    pub disabled: bool,
    pub handler: Option<Handler>,
    pub dropdown_content: Option<dropdown::ItemContent>,
}

impl MenuItem {
    pub(crate) fn new(id: &'static str, label: impl Into<SharedString>) -> Self {
        Self { id, icon: None, swatch: None, label: label.into(), shortcut: None, checked: false, disabled: false, handler: None, dropdown_content: None }
    }

    /// An item labelled in the UI's language by its id (`i18n/*.tsv`).
    pub(crate) fn tr(id: &'static str) -> Self {
        Self::new(id, crate::shell::i18n::text(id))
    }

    pub(crate) fn icon(mut self, icon: &'static str) -> Self {
        self.icon = Some(icon);
        self
    }

    pub(crate) fn swatch(mut self, swatch: ibis_doc_model::Rgb) -> Self {
        self.swatch = Some(swatch);
        self
    }

    pub(crate) fn swatch_opt(mut self, swatch: Option<ibis_doc_model::Rgb>) -> Self {
        self.swatch = swatch;
        self
    }

    pub(crate) fn shortcut(mut self, shortcut: &'static str) -> Self {
        self.shortcut = Some(shortcut);
        self
    }

    pub(crate) fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn on(mut self, f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Self {
        self.handler = Some(Rc::new(f));
        self
    }

    pub(crate) fn enabled(&self) -> bool {
        self.handler.is_some() && !self.disabled
    }
}

impl From<MenuItem> for MenuEntry {
    fn from(item: MenuItem) -> Self {
        MenuEntry::Item(item)
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) struct Submenu {
    pub id: &'static str,
    pub icon: Option<&'static str>,
    pub label: SharedString,
    pub items: Vec<MenuEntry>,
}

impl Submenu {
    pub(crate) fn new(id: &'static str, icon: &'static str, label: impl Into<SharedString>, items: Vec<MenuEntry>) -> Self {
        Self { id, icon: Some(icon), label: label.into(), items }
    }

    /// A submenu labelled in the UI's language by its id.
    pub(crate) fn tr(id: &'static str, icon: &'static str, items: Vec<MenuEntry>) -> Self {
        Self::new(id, icon, crate::shell::i18n::text(id), items)
    }
}

impl From<Submenu> for MenuEntry {
    fn from(submenu: Submenu) -> Self {
        MenuEntry::Submenu(submenu)
    }
}

#[allow(dead_code)] // The registration API: used by the formats' code.
fn canonical_menu_id(id: &str) -> &str {
    match id {
        "xlsx.insert-textbox" => "insert.text-box",
        "xlsx.insert-shape" => "insert.shape",
        "xlsx.insert-picture-menu" => "insert.image",
        "xlsx.insert-link" => "insert.link",
        "xlsx.insert-symbol" => "insert.symbol",
        "xlsx.recommended-pivot-tables" => "xlsx.recommended-pivots",
        _ => id,
    }
}

/// The menu bar's menus, as built for the open document.
#[derive(Clone, Default)]
pub(crate) struct Menus {
    pub menus: Vec<(MenuId, Vec<MenuEntry>)>,
}

#[allow(dead_code)] // The registration API: used by the formats' code.
impl Menus {
    /// The items of menu `id` (an empty menu is added if it is not there).
    pub(crate) fn items_mut(&mut self, id: MenuId) -> &mut Vec<MenuEntry> {
        if let Some(i) = self.menus.iter().position(|(m, _)| *m == id) {
            return &mut self.menus[i].1;
        }
        self.menus.push((id, Vec::new()));
        &mut self.menus.last_mut().expect("just pushed").1
    }

    /// A copy of the entry (an item or a submenu) with id `id`, in any menu or submenu.
    pub(crate) fn find_entry(&self, id: &str) -> Option<MenuEntry> {
        let id = canonical_menu_id(id);
        fn find(entries: &[MenuEntry], id: &str) -> Option<MenuEntry> {
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) if item.id == id => return Some(entry.clone()),
                    MenuEntry::Palette(p) if p.find(id).is_some() => return p.find(id).cloned().map(MenuEntry::Item),
                    MenuEntry::Submenu(sub) => {
                        if sub.id == id {
                            return Some(entry.clone());
                        }
                        if let Some(found) = find(&sub.items, id) {
                            return Some(found);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        self.menus.iter().find_map(|(_, entries)| find(entries, id))
    }

    /// The item with id `id`, in any menu or submenu.
    pub(crate) fn find_mut(&mut self, id: &str) -> Option<&mut MenuItem> {
        let id = canonical_menu_id(id);
        fn find<'a>(entries: &'a mut [MenuEntry], id: &str) -> Option<&'a mut MenuItem> {
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) if item.id == id => return Some(item),
                    MenuEntry::Palette(p) => {
                        if let Some(found) = p.find_mut(id) {
                            return Some(found);
                        }
                    }
                    MenuEntry::Submenu(sub) => {
                        if let Some(found) = find(&mut sub.items, id) {
                            return Some(found);
                        }
                    }
                    _ => {}
                }
            }
            None
        }
        self.menus.iter_mut().find_map(|(_, entries)| find(entries, id))
    }

    /// Put `items` in the submenu with id `id`, wherever it is (nothing if there is none).
    pub(crate) fn fill_submenu(&mut self, id: &str, items: Vec<MenuEntry>) {
        fn find<'a>(entries: &'a mut [MenuEntry], id: &str) -> Option<&'a mut Submenu> {
            for entry in entries {
                if let MenuEntry::Submenu(sub) = entry {
                    if sub.id == id {
                        return Some(sub);
                    }
                    if let Some(found) = find(&mut sub.items, id) {
                        return Some(found);
                    }
                }
            }
            None
        }
        if let Some(sub) = self.menus.iter_mut().find_map(|(_, entries)| find(entries, id)) {
            sub.items = items;
        }
    }

    /// Give the item with id `id` its handler (it becomes enabled).
    /// Whether the item was found.
    pub(crate) fn connect(&mut self, id: &str, f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> bool {
        match self.find_mut(id) {
            Some(item) => {
                item.handler = Some(Rc::new(f));
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum SegmentRounding {
    #[default]
    All,
    Left,
    Middle,
    Right,
}

/// A button of the toolbar.
#[derive(Clone)]
pub(crate) struct ToolButton {
    pub id: &'static str,
    pub icon: Option<&'static str>,
    /// A text button (`背景`, `¥`) instead of, or after, the icon.
    pub text: Option<SharedString>,
    pub tooltip: SharedString,
    /// Pressed (a toggle that is on, or the tool in use).
    pub on: bool,
    pub disabled: bool,
    /// A colour bar under the icon (text colour, fill colour).
    pub color: Option<Hsla>,
    /// A menu opened by the button (a ▼ is drawn after the icon).
    pub menu: Option<Vec<MenuEntry>>,
    pub handler: Option<Handler>,
    /// A split button (as Office's): what the left half does at once (the last used choice), the ▼ on the
    /// right having the menu.
    pub quick: Option<Handler>,
    /// Not drawn in the toolbar (its menu is for the Format menu, `menu_bridge.rs`, only).
    pub(crate) pub_hidden_marker: (),
    pub hidden: bool,
    pub segment: SegmentRounding,
    /// When true, the dropdown menu has a search TextField at the top that filters items by name (#98, #103).
    pub searchable: bool,
    /// When true, this searchable dropdown represents a single-selection list (one checked item highlighted in light blue).
    pub single_select: bool,
}

impl ToolButton {
    pub(crate) fn new(id: &'static str, icon: &'static str, tooltip: impl Into<SharedString>) -> Self {
        Self { id, icon: Some(icon), text: None, tooltip: tooltip.into(), on: false, disabled: false, color: None, menu: None, handler: None, quick: None, pub_hidden_marker: (), hidden: false, segment: SegmentRounding::All, searchable: false, single_select: false }
    }

    /// A text button.
    pub(crate) fn text(id: &'static str, text: impl Into<SharedString>, tooltip: impl Into<SharedString>) -> Self {
        Self { id, icon: None, text: Some(text.into()), tooltip: tooltip.into(), on: false, disabled: false, color: None, menu: None, handler: None, quick: None, pub_hidden_marker: (), hidden: false, segment: SegmentRounding::All, searchable: false, single_select: false }
    }

    /// A drop-down button that has only a ▼ arrow (no separate icon).
    pub(crate) fn dropdown(id: &'static str, tooltip: impl Into<SharedString>) -> Self {
        Self { id, icon: None, text: None, tooltip: tooltip.into(), on: false, disabled: false, color: None, menu: None, handler: None, quick: None, pub_hidden_marker: (), hidden: false, segment: SegmentRounding::All, searchable: false, single_select: false }
    }

    /// Add a search box at the top of this button's dropdown menu (#98, #103).
    pub(crate) fn searchable(mut self) -> Self {
        self.searchable = true;
        self
    }

    /// Mark this searchable dropdown as a single-selection list (selected item gets checkmark + light blue background).
    pub(crate) fn single_select(mut self) -> Self {
        self.single_select = true;
        self
    }

    pub(crate) fn segment(mut self, segment: SegmentRounding) -> Self {
        self.segment = segment;
        self
    }

    pub(crate) fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub(crate) fn color(mut self, color: Hsla) -> Self {
        self.color = Some(color);
        self
    }

    /// A button of a palette (`color_palette.rs`): its left half sets the colour last picked from it in this window
    /// (else `default`) and shows it as the bar under the icon; the ▼ opens the palette. `id` names the button's
    /// last colour.
    pub(crate) fn palette(mut self, editor: &SlideEditor, id: &'static str, default: u32, mut items: Vec<MenuEntry>) -> Self {
        let last = editor.last_colors.get(id).copied().unwrap_or(ibis_doc_model::Rgb(default));
        let mut pick = None;
        for entry in &mut items {
            if let MenuEntry::Palette(p) = entry {
                p.button = Some(id);
                pick = p.pick.clone();
                // Picking a square (here or by the scenarios) is the button's last colour from then on.
                for swatch in p.theme.iter_mut().flatten().chain(p.standard.iter_mut()) {
                    let (rgb, inner) = (swatch.rgb, swatch.item.handler.clone());
                    swatch.item.handler = inner.map(|inner| -> Handler {
                        Rc::new(move |e, w, cx| {
                            e.last_colors.insert(id, rgb);
                            inner(e, w, cx)
                        })
                    });
                }
            }
        }
        self.color = Some(rgb(last.0).into());
        self.quick = pick.map(|pick| -> Handler { Rc::new(move |e, w, cx| pick(e, Some(last), w, cx)) });
        self.menu = Some(items);
        self
    }

    /// A split button of the kind that remembers the choice (Office's borders, bullets, numbering, underline, shading):
    /// the left half does at once what was last chosen from the menu in this window (else the first item `default`
    /// accepts), the ▼ opens the menu. Only the items `record` accepts are remembered; with `follow_icon` the button
    /// shows the icon of that item. Call it after `menu`.
    pub(crate) fn last_used(mut self, editor: &SlideEditor, id: &'static str, record: fn(&MenuItem) -> bool, default: fn(&MenuItem) -> bool, follow_icon: bool) -> Self {
        fn key(item: &MenuItem) -> String {
            format!("{}\t{}", item.id, item.label)
        }
        fn wrap(entries: &mut [MenuEntry], id: &'static str, record: fn(&MenuItem) -> bool) {
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) if record(item) => {
                        let (k, inner) = (key(item), item.handler.clone());
                        item.handler = inner.map(|inner| -> Handler {
                            Rc::new(move |e, w, cx| {
                                e.last_choices.insert(id, k.clone());
                                inner(e, w, cx)
                            })
                        });
                    }
                    MenuEntry::Submenu(sub) => wrap(&mut sub.items, id, record),
                    _ => {}
                }
            }
        }
        fn leaves(entries: &[MenuEntry], record: fn(&MenuItem) -> bool, out: &mut Vec<MenuItem>) {
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) if record(item) && item.enabled() => out.push(item.clone()),
                    MenuEntry::Submenu(sub) => leaves(&sub.items, record, out),
                    _ => {}
                }
            }
        }
        let Some(menu) = self.menu.as_mut() else { return self };
        wrap(menu, id, record);
        let mut all = Vec::new();
        leaves(menu, record, &mut all);
        let last = editor.last_choices.get(id);
        let chosen = last.and_then(|k| all.iter().find(|i| &key(i) == k)).or_else(|| all.iter().find(|i| default(i))).cloned();
        if let Some(item) = chosen {
            if follow_icon {
                self.icon = item.icon.or(self.icon);
            }
            self.quick = item.handler;
        }
        self
    }

    /// The left half of a split button with its own action.
    pub(crate) fn quick(mut self, f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Self {
        self.quick = Some(Rc::new(f));
        self
    }

    /// The left half of a split button that is also a switch: while it is `on`, it does `off`; else the last choice
    /// (or `off` itself, when nothing was chosen yet, the switch being what the button did before the menu).
    pub(crate) fn quick_off(mut self, on: bool, off: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Self {
        let inner = self.quick.take();
        self.quick = Some(Rc::new(move |e, w, cx| match &inner {
            Some(inner) if !on => inner(e, w, cx),
            _ => off(e, w, cx),
        }));
        self
    }

    /// Not drawn in the toolbar.
    pub(crate) fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// What the left half does: the last choice of a split button.
    pub(crate) fn left_action(&self) -> Option<&Handler> {
        self.menu.as_ref().and(self.quick.as_ref())
    }

    pub(crate) fn menu(mut self, items: Vec<MenuEntry>) -> Self {
        self.menu = Some(items);
        self
    }

    pub(crate) fn click(mut self, f: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Self {
        self.handler = Some(Rc::new(f));
        self
    }

    pub(crate) fn enabled(&self) -> bool {
        !self.disabled && (self.searchable || self.handler.is_some() || self.menu.as_ref().is_some_and(|m| !m.is_empty()))
    }

    /// The width it takes in the toolbar, in pixels (for folding into ︙).
    fn width(&self) -> f32 {
        let text = self.text.as_ref().map_or(0.0, |t| text_width(t, 14.0) + 12.0);
        let icon = if self.icon.is_some() { 26.0 } else { 0.0 };
        let caret = if self.menu.is_some() && self.id != "menu-search" { 14.0 } else { 0.0 };
        // A split button is the icon's button and the ▼'s side by side.
        let split = if self.left_action().is_some() { 4.0 } else { 0.0 };
        (icon + text + caret + split).max(28.0)
    }
}

impl From<ToolButton> for ToolItem {
    fn from(button: ToolButton) -> Self {
        ToolItem::Button(button)
    }
}

/// A part of the toolbar.
#[derive(Clone)]
pub(crate) enum ToolItem {
    Button(ToolButton),
    Separator,
    /// A drop-down box with its value shown (フォント, スタイル).
    Select { id: &'static str, value: SharedString, width: f32, tooltip: SharedString, menu: Vec<MenuEntry>, disabled: bool },
    /// − [size] + : the font size.
    FontSize { value: Option<SharedString>, smaller: Option<Handler>, larger: Option<Handler> },
}

impl ToolItem {
    fn width(&self) -> f32 {
        match self {
            ToolItem::Button(b) => b.width(),
            ToolItem::Separator => 9.0,
            ToolItem::Select { width, .. } => *width,
            ToolItem::FontSize { .. } => 120.0,
        }
    }
}

/// A tab of the sidebar's top (a format's panels that stand side by side):
/// its label, the title of the panel it shows (to tell which is selected)
/// and what pressing it does.
pub(crate) struct SidebarTab {
    pub label: &'static str,
    pub panel_title: &'static str,
    pub open: Handler,
}

/// The right pane's state for its buttons (the title row's, 表示 > サイドバー), found by `render_sidebar`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct RightPane {
    /// The toggle can show something: the format has a pane on request, or a panel is open.
    pub available: bool,
    /// The pane is on screen.
    pub shown: bool,
}

/// The sidebar's content.
pub(crate) struct SidebarPanel {
    pub title: SharedString,
    pub body: AnyElement,
}

// The opt-in gallery renders each real panel independently, without stale panel precedence.
#[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
pub(crate) type CapturedPanel = (fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) -> Option<SidebarPanel>, &'static str, super::panel_dialog::PanelMode);
#[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
thread_local! {
    pub(crate) static FORCED_SIDEBAR: std::cell::Cell<Option<CapturedPanel>> = const { std::cell::Cell::new(None) };
    pub(crate) static LAST_SIDEBAR_TITLE: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

thread_local! {
    static PANEL_MODE: std::cell::Cell<super::panel_dialog::PanelMode> = const { std::cell::Cell::new(super::panel_dialog::PanelMode::Sidebar) };
    static PANEL_KEY: std::cell::Cell<Option<&'static str>> = const { std::cell::Cell::new(None) };
    static SIDEBAR_FOOTER: std::cell::RefCell<Option<ActionBar>> = const { std::cell::RefCell::new(None) };
}

impl SidebarPanel {
    pub(crate) fn submit_type(self,submit:super::panel_dialog::SubmitType)->Self {super::panel_dialog::form::OPTIONS.with(|o|o.set(super::panel_dialog::form::Options{submit,..o.get()}));self}
    pub(crate) fn auto_focus(self,auto_focus:bool)->Self {super::panel_dialog::form::OPTIONS.with(|o|o.set(super::panel_dialog::form::Options{auto_focus,..o.get()}));self}

    pub(crate) fn modal_named(self, key: &'static str) -> Self {
        PANEL_KEY.with(|current| current.set(Some(key)));
        self.modal()
    }
    pub(crate) fn modeless_named(self, key: &'static str) -> Self {
        PANEL_KEY.with(|current| current.set(Some(key)));
        self.modeless()
    }
    pub(crate) fn modal(self) -> Self {
        PANEL_MODE.with(|mode| mode.set(super::panel_dialog::PanelMode::Modal)); self
    }
    pub(crate) fn modeless(self) -> Self {
        PANEL_MODE.with(|mode| mode.set(super::panel_dialog::PanelMode::Modeless)); self
    }

    /// Construct a `SidebarPanel` with a sticky bottom footer rendered below the scrollable body (#151).
    pub(crate) fn with_footer(title: impl Into<SharedString>, body: AnyElement, footer: ActionBar) -> Self {
        SIDEBAR_FOOTER.with(|cell| *cell.borrow_mut() = Some(footer));
        Self {
            title: title.into(),
            body,
        }
    }
}

/// A rough width of `text` at `size` px: full-width characters are square.
fn text_width(text: &str, size: f32) -> f32 {
    text.chars().map(|c| if c.is_ascii() { size * 0.58 } else { size }).sum()
}

// ----- Drawing -------------------------------------------------------------

/// Make the theme's primary button the app's one colour ([`PRIMARY_BUTTON_BG`]).
pub(crate) fn apply_primary_button_colors(theme: &mut gpui_kit::component::Theme) {
    let (normal, hover, active, text): (Hsla, Hsla, Hsla, Hsla) = (rgb(PRIMARY_BUTTON_BG).into(), rgb(PRIMARY_BUTTON_HOVER_BG).into(), rgb(PRIMARY_BUTTON_ACTIVE_BG).into(), rgb(0xffffff).into());
    // The colours, and the tokens the buttons read (made from the colours when the theme was chosen).
    theme.primary = normal;
    theme.primary_hover = hover;
    theme.primary_active = active;
    theme.primary_foreground = text;
    theme.button_primary = normal;
    theme.button_primary_hover = hover;
    theme.button_primary_active = active;
    theme.button_primary_foreground = text;
    theme.tokens.primary = normal.into();
    theme.tokens.primary_hover = hover.into();
    theme.tokens.primary_active = active.into();
    theme.tokens.primary_foreground = text.into();
    theme.tokens.button_primary = normal.into();
    theme.tokens.button_primary_hover = hover.into();
    theme.tokens.button_primary_active = active.into();
    theme.tokens.button_primary_foreground = text.into();
}

/// The icon of a menu row: the menu draws it at its own small size, so it is scaled up about its centre.
fn menu_icon(name: &'static str) -> Icon {
    Icon::new(Material(name)).transform(gpui::Transformation::scale(gpui::size(MENU_ICON_SCALE, MENU_ICON_SCALE)))
}

/// A handler as a click listener: it reaches the editor through a weak
/// handle, so it works in popups too.
fn listener(editor: &WeakEntity<SlideEditor>, handler: &Handler) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let (editor, handler) = (editor.clone(), handler.clone());
    move |event, window, cx| {
        editor.update(cx, |this, cx| {
            // PopupMenu invokes commands with a default event, including keyboard confirmation.
            // Only actual pointer positions can be checked against the title row.
            if this.panel_dialog_is_modal() && event.position() != point(px(0.), px(0.)) && event.position().y < px(36.) {
                return;
            }
            this.ruler_settings_open = false;
            handler(this, window, cx)
        }).ok();
    }
}

/// Resolve the display label for `item`: UI keys go through `i18n::text`, installed font names
/// go through `i18n::font_label`, and document-specific / user-defined names are returned as-is.
pub(crate) fn menu_item_display_label(item: &MenuItem) -> SharedString {
    const NAMES: [&str; 12] = [
        "file.recent.item",
        "xlsx.unhide-sheet",
        "merge.import.sheet",
        "xlsx.validation-item",
        "xlsx.sheet-list.item",
        "pptx.new-slide.layout",
        "pptx.layout.apply",
        "pptx.clip.bookmark-jump",
        "pptx.clip.caption-item",
        "pptx.custom-show-item",
        "docx.style.update",
        "xlsx.function",
    ];
    let is_dynamic_id = item.id.starts_with("window.open.")
        || item.id.starts_with("font.section.")
        || item.id.starts_with("docx.style.doc.")
        || item.id.starts_with("docx.style.char.")
        || item.id.starts_with("docx.track.reviewers.author:")
        || (item.id.starts_with("docx.xref.") && item.id != "docx.xref.dialog" && item.id != "docx.xref.none")
        || (item.id.starts_with("docx.date.") && item.id != "docx.date.update");
    if dropdown::is_font_item_id(item.id) {
        crate::shell::i18n::font_label(&item.label)
    } else if NAMES.contains(&item.id) || is_dynamic_id {
        item.label.clone()
    } else if crate::shell::i18n::is_font_name(&item.label) {
        crate::shell::i18n::font_label(&item.label)
    } else {
        crate::shell::i18n::text(&item.label)
    }
}

/// Format a shortcut string for `is_macos` (`true`: keep macOS symbols `⌘⇧⌥⌃`; `false`: convert to `Ctrl+`, `Alt+`, `Shift+`, `Backspace`, `Enter`, etc.).
pub(crate) fn format_shortcut_for_target(shortcut: &str, is_macos: bool) -> String {
    if is_macos || shortcut.is_empty() {
        return shortcut.to_string();
    }
    let mut has_ctrl = false;
    let mut has_alt = false;
    let mut has_shift = false;
    let mut key_part = String::new();
    for ch in shortcut.chars() {
        match ch {
            '⌘' | '⌃' => has_ctrl = true,
            '⌥' => has_alt = true,
            '⇧' => has_shift = true,
            '⌫' => key_part.push_str("Backspace"),
            '⌦' => key_part.push_str("Del"),
            '↩' => key_part.push_str("Enter"),
            '⎋' => key_part.push_str("Esc"),
            '⇥' => key_part.push_str("Tab"),
            other => key_part.push(other),
        }
    }
    let mut out = String::new();
    if has_ctrl {
        out.push_str("Ctrl+");
    }
    if has_alt {
        out.push_str("Alt+");
    }
    if has_shift {
        out.push_str("Shift+");
    }
    out.push_str(&key_part);
    out
}

pub(crate) fn format_shortcut_for_platform(shortcut: &str) -> String {
    format_shortcut_for_target(shortcut, cfg!(target_os = "macos"))
}

pub(crate) fn menu_item_row(item: &MenuItem, editor: &WeakEntity<SlideEditor>) -> PopupMenuItem {menu_item_row_for_width(item,editor,MENU_ITEM_MAX_WIDTH)}
fn menu_item_row_for_width(item:&MenuItem,editor:&WeakEntity<SlideEditor>,width:f32)->PopupMenuItem {
                // Icon (or ✓) in the menu's icon column, the label, the shortcut.
                // The code's words in the UI's language; a document's names (sheets,
                // layouts, styles, fonts, files) as they are.
                let is_font_item = dropdown::is_font_item_id(item.id);
                let is_font_section = item.id.starts_with("font.section.");

                if is_font_section {
                    let label = item.label.clone();
                    return PopupMenuItem::element(move |_, cx| {
                        div()
                            .w_full()
                            .px(px(6.))
                            .py(px(3.))
                            .text_size(px(12.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(cx.theme().muted_foreground)
                            .child(label.clone())
                    })
                    .disabled(true);
                }

                let full_label=menu_item_display_label(item);
                let shortcut_width=item.shortcut.map_or(0.,|key|text_width(&format_shortcut_for_platform(key),13.)+24.);
                let label=fit_menu_label(&full_label,(width-72.-shortcut_width-if item.swatch.is_some(){18.}else{0.}).max(0.));
                let (shortcut, enabled, swatch) = (item.shortcut, item.enabled(), item.swatch);
                let valid_icon = item.icon.filter(|i| !i.is_empty() && !item.checked && crate::shell::icons::icon_svg_bytes(i).is_some());
                let raw_font_name = item.label.to_string();
                let display_label = label.clone();
                #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
                let probe_id = item.id;

                let preview=item.dropdown_content.clone();
                let tooltip_label=item.label.clone();
                let mut row = PopupMenuItem::element(move |_, cx| {
                    let fg: Hsla = if enabled { rgb(TEXT).into() } else { disabled_color() };
                    let content = if let Some(dropdown::ItemContent::Palette(colors))=&preview {
                        div().id("menu-palette-preview").child(dropdown::color_palette_item(colors)).tooltip({let label=tooltip_label.clone();move|w,cx|gpui_kit::component::tooltip::Tooltip::new(label.clone()).build(w,cx)}).into_any_element()
                    } else if is_font_item {
                        div().pl(px(14.)).child(dropdown::font_item(&raw_font_name,display_label.clone(),fg)).into_any_element()
                    } else {
                        h_flex()
                            .items_center()
                            .gap(px(6.))
                            .when_some(swatch, |d, c| {
                                d.child(div().w(px(10.)).h(px(10.)).rounded(px(2.)).bg(rgb(c.0)).border_1().border_color(crate::shell::palette::shade(0.0, 0.2)))
                            })
                            .child(div().flex_1().min_w_0().truncate().text_color(fg).child(display_label.clone())).into_any_element()
                    };

                    let row = h_flex()
                        .w_full()
                        .gap(px(16.))
                        .items_center()
                        .child(div().flex_1().min_w_0().overflow_hidden().child(content))
                        .when_some(shortcut, |d, key| d.child(div().text_color(if enabled { cx.theme().muted_foreground } else { disabled_color() }).child(format_shortcut_for_platform(key))));
                    #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
                    let row = {
                        let id = SharedString::from(format!("menu-item-{probe_id}"));
                        super::scenario::button_probe::observe_control(row.id(id.clone()), id.into())
                    };
                    row
                })
                .checked(item.checked)
                .disabled(!enabled);
                if let Some(name) = valid_icon {
                    row = row.icon(menu_icon(name));
                }
                if let Some(handler) = &item.handler {
                    row = row.on_click(listener(editor, handler));
                }
    row
}

/// `entries` without the submenus that have nothing to show (no items, or
/// only separators), and without the separators that would be left at the
/// start, at the end or twice in a row. Applied to every submenu too.
pub(crate) fn prune_entries(entries: Vec<MenuEntry>) -> Vec<MenuEntry> {
    let mut out: Vec<MenuEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        match entry {
            MenuEntry::Submenu(mut sub) => {
                sub.items = prune_entries(std::mem::take(&mut sub.items));
                if !sub.items.is_empty() {
                    out.push(MenuEntry::Submenu(sub));
                }
            }
            MenuEntry::Separator => {
                if !matches!(out.last(), None | Some(MenuEntry::Separator)) {
                    out.push(MenuEntry::Separator);
                }
            }
            other => out.push(other),
        }
    }
    while matches!(out.last(), Some(MenuEntry::Separator)) {
        out.pop();
    }
    out
}

/// Maximum width of a menu item, including icon and shortcut columns.
fn fit_menu_label(label:&str,width:f32)->SharedString {
    if text_width(label,14.)<=width{return label.to_owned().into();}
    let ellipsis="…";
    if text_width(ellipsis,14.)>width{return "".into();}
    let boundaries=label.char_indices().map(|(i,_)|i).chain(std::iter::once(label.len())).collect::<Vec<_>>();
    let (mut low,mut high)=(0,boundaries.len());
    while low+1<high {let mid=(low+high)/2;let candidate=format!("{}{}",&label[..boundaries[mid]],ellipsis);if text_width(&candidate,14.)<=width{low=mid;}else{high=mid;}}
    format!("{}{}",&label[..boundaries[low]],ellipsis).into()
}

pub(crate) const MENU_ITEM_MAX_WIDTH:f32=600.;

/// Compute the fixed width required to display all `entries` without changing width while scrolling (#108).
pub(crate) fn menu_width_for_entries(entries: &[MenuEntry]) -> f32 {
    let palette_only=!entries.is_empty()&&entries.iter().all(|e|matches!(e,MenuEntry::Item(item) if matches!(item.dropdown_content,Some(dropdown::ItemContent::Palette(_)))));
    let mut max_w: f32 = if palette_only{0.}else{280.};
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => {
                if let Some(dropdown::ItemContent::Palette(colors))=&item.dropdown_content {
                    max_w=max_w.max(colors.len() as f32*16.+colors.len().saturating_sub(1) as f32*3.+48.);continue;
                }
                let label = menu_item_display_label(item);
                let shortcut_w = item.shortcut.map_or(0.0, |s| text_width(&format_shortcut_for_platform(s), 13.0) + 24.0);
                let swatch_w = if item.swatch.is_some() { 18.0 } else { 0.0 };
                let w = 48.0 + swatch_w + text_width(&label, 14.0) + shortcut_w + 24.0;
                max_w = max_w.max(w);
            }
            MenuEntry::Submenu(sub) => {
                let label = crate::shell::i18n::text(&sub.label);
                let w = 48.0 + text_width(&label, 14.0) + 36.0;
                max_w = max_w.max(w);
            }
            _ => {}
        }
    }
    max_w.min(MENU_ITEM_MAX_WIDTH)
}

/// Add `entries` directly into `menu` without re-pruning or applying `PopupMenu`'s internal scroll cap (#105).
pub(crate) fn fill_menu_entries(mut menu: PopupMenu, entries: &[MenuEntry], width:f32, editor: &WeakEntity<SlideEditor>, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    for entry in entries {
        menu = match entry {
            MenuEntry::Separator => menu.separator(),
            MenuEntry::Submenu(sub) => {
                let (items, editor) = (sub.items.clone(), editor.clone());
                let icon = sub.icon.filter(|i| !i.is_empty() && crate::shell::icons::icon_svg_bytes(i).is_some()).map(menu_icon);
                let caption=fit_menu_label(&crate::shell::i18n::text(&sub.label),(width-84.).max(0.));
                menu.submenu_with_icon(icon, caption, window, cx, move |m, window, cx| fill_menu(m, &items, &editor, window, cx))
            }
            MenuEntry::Palette(palette) => crate::shell::color_palette::fill_palette(menu, palette, editor, cx),
            MenuEntry::Item(item) => menu.item(menu_item_row_for_width(item, editor,width)),
        };
    }
    menu
}

/// `entries` into `menu`.
pub(crate) fn fill_menu(menu: PopupMenu, entries: &[MenuEntry], editor: &WeakEntity<SlideEditor>, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
    let entries = prune_entries(entries.to_vec());
    let width = menu_width_for_entries(&entries);
    crate::shell::menu_bar::fill_scrollable_popup_menu(menu, entries, width, editor, window, cx)
}


/// Row overflow actions: hover visibility and compact borderless styling live here.
/// The start history explicitly uses this light-grey overflow affordance.
pub(crate) fn hover_row_actions(index: usize, entries: Vec<MenuEntry>, editor: &WeakEntity<SlideEditor>) -> AnyElement {
    let id=ElementId::Name(format!("row-actions-{index}").into());
    let button=Button::new(id.clone()).ghost().small()
        .icon(Icon::new(Material("more_vert")).transform(Transformation::rotate(Radians(std::f32::consts::FRAC_PI_2)))).w(px(30.)).h(px(26.)).rounded(px(5.))
        .bg(rgb(0xf0f0f0)).tooltip(crate::shell::i18n::text("shell.row.actions"));
    #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
    let button=super::scenario::button_probe::observe(button,id);
    div().id(("row-actions-wrap",index))
        .on_mouse_down(MouseButton::Left,|_,_,cx|cx.stop_propagation())
        .on_click(|_,_,cx|cx.stop_propagation())
        .child(Dropdown::with_button("row-actions",button,entries,false,index,editor).reveal_on_hover("row-actions")).into_any_element()
}

/// One logo renderer for the start panel and all editing menu bars.
pub(crate) fn brand_logo(id:&'static str,height:f32,full_width:bool)->Stateful<Div>{
    let root=div().id(id).relative().flex_none();
    let root=if full_width{root.w_full().aspect_ratio(LOGO_ASPECT)}else{root.w(px(height*LOGO_ASPECT)).h(px(height))};
    let root=root.child(img(app_logo_for_height(height)).absolute().inset_0().size_full().object_fit(ObjectFit::Contain));
    #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
    let root=super::scenario::button_probe::observe_control(root,id.into());
    root
}

/// The ibisOffice logo (the mark and the name, logo/ibisOffice_logo.svg) as
/// the start view's title row shows it: its height in pixels, and its width
/// over its height. The SVG's own size is this size (99.25×22): GPUI draws
/// an SVG picture at twice its own size and scales that to the element, and
/// scaled down from its 600×133 it came out jagged.
pub(crate) const LOGO_HEIGHT: f32 = 22.0;
pub(crate) const LOGO_ASPECT: f32 = 600.0 / 133.0;

pub(crate) fn app_logo() -> std::sync::Arc<Image> { app_logo_for_height(LOGO_HEIGHT) }
/// Size the SVG raster source to its use, so a large start logo stays sharp.
pub(crate) fn app_logo_for_height(height:f32)->std::sync::Arc<Image>{
    static CACHE:std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<(u32,bool),std::sync::Arc<Image>>>>=std::sync::OnceLock::new();
    let height=height.ceil().max(1.)as u32;let dark=crate::shell::palette::is_dark();
    let mut cache=CACHE.get_or_init(Default::default).lock().unwrap();
    cache.entry((height,dark)).or_insert_with(||{
        let mut svg=include_str!("../../logo/ibisOffice_logo.svg").replacen("width=\"99.25\" height=\"22\"",&format!("width=\"{}\" height=\"{height}\"",height as f32*LOGO_ASPECT),1);
        if dark {svg=svg.replace("rgb(0,40,76)","rgb(232,238,246)").replace("rgb(0,40,77)","rgb(232,238,246)");}
        std::sync::Arc::new(Image::from_bytes(ImageFormat::Svg,svg.into_bytes()))
    }).clone()
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn office_app_logo(kind: ibis_pptx::Kind) -> std::sync::Arc<Image> {
    static WORD: std::sync::OnceLock<std::sync::Arc<Image>> = std::sync::OnceLock::new();
    static EXCEL: std::sync::OnceLock<std::sync::Arc<Image>> = std::sync::OnceLock::new();
    static POWERPOINT: std::sync::OnceLock<std::sync::Arc<Image>> = std::sync::OnceLock::new();
    match kind {
        ibis_pptx::Kind::Document => WORD
            .get_or_init(|| std::sync::Arc::new(Image::from_bytes(ImageFormat::Svg, include_bytes!("../../logo/open_in_word.svg").to_vec())))
            .clone(),
        ibis_pptx::Kind::Workbook => EXCEL
            .get_or_init(|| std::sync::Arc::new(Image::from_bytes(ImageFormat::Svg, include_bytes!("../../logo/open_in_excel.svg").to_vec())))
            .clone(),
        ibis_pptx::Kind::Presentation => POWERPOINT
            .get_or_init(|| std::sync::Arc::new(Image::from_bytes(ImageFormat::Svg, include_bytes!("../../logo/open_in_powerpoint.svg").to_vec())))
            .clone(),
    }
}

pub(crate) fn icon(name: &'static str, size: f32, color: Hsla) -> Icon {
    Icon::new(Material(name)).size(px(size)).text_color(color)
}

/// Returns true if this platform requires client-drawn window control buttons
/// (Minimize, Maximize/Restore, Close).
pub(crate) fn should_render_window_controls(window: &Window) -> bool {
    #[cfg(feature = "scenarios")]
    if SCENARIO_CLIENT_WINDOW_CONTROLS.load(std::sync::atomic::Ordering::Relaxed) {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = window;
        false
    }
    #[cfg(target_os = "windows")]
    {
        let _ = window;
        true
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        matches!(window.window_decorations(), Decorations::Client { .. })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux", target_os = "freebsd")))]
    {
        let _ = window;
        false
    }
}

// Scenarios exercise the actual client-drawn controls on platforms whose
// native title bar would otherwise hide them. Restore even if a scenario panics.
#[cfg(feature = "scenarios")]
static SCENARIO_CLIENT_WINDOW_CONTROLS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(feature = "scenarios")]
pub(crate) struct ScenarioClientWindowControls(bool);

#[cfg(feature = "scenarios")]
impl Drop for ScenarioClientWindowControls {
    fn drop(&mut self) {
        SCENARIO_CLIENT_WINDOW_CONTROLS.store(self.0, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(feature = "scenarios")]
pub(crate) fn scenario_client_window_controls() -> ScenarioClientWindowControls {
    ScenarioClientWindowControls(SCENARIO_CLIENT_WINDOW_CONTROLS.swap(true, std::sync::atomic::Ordering::Relaxed))
}

/// Configures a div as a window titlebar drag area.
/// On Windows, registers it as a native caption drag area.
/// On Linux (CSD), starts window dragging on left mouse press, toggles maximized state
/// on double click, and shows the native window menu on right click.
pub(crate) fn apply_title_drag(div: Div, window: &Window) -> Div {
    #[cfg(target_os = "windows")]
    {
        let _ = window;
        div.window_control_area(WindowControlArea::Drag)
    }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    {
        if matches!(window.window_decorations(), Decorations::Client { .. }) {
            div.on_mouse_down(MouseButton::Left, |e, window, _| {
                eprintln!("[DEBUG-WAYLAND] titlebar mouse down! click_count={}", e.click_count);
                if e.click_count == 2 {
                    eprintln!("[DEBUG-WAYLAND] titlebar double-click -> zoom_window()");
                    window.zoom_window();
                } else {
                    eprintln!("[DEBUG-WAYLAND] titlebar single-click -> start_window_move()");
                    window.start_window_move();
                }
            })
            .on_mouse_down(MouseButton::Right, |e, window, _| {
                eprintln!("[DEBUG-WAYLAND] titlebar right-click -> show_window_menu()");
                window.show_window_menu(e.position);
            })
        } else {
            eprintln!("[DEBUG-WAYLAND] apply_title_drag: NOT client decorations! {:?}", window.window_decorations());
            div
        }
    }
    #[cfg(target_os = "macos")]
    {
        // The title row is drawn by the app (the system's title bar is transparent): a double click does what the
        // system's setting says (System Settings > Desktop & Dock > "Double-click a window's title bar to"), zoom by default.
        let _ = window;
        div.on_mouse_down(MouseButton::Left, |e, window, _| {
            if e.click_count == 2 {
                window.titlebar_double_click();
            }
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "freebsd", target_os = "macos")))]
    {
        let _ = window;
        div
    }
}

/// The app's icon at the left end of the title row (Windows): a click shows
/// the window's system menu below it, as the icon of a native title bar does.
#[cfg(target_os = "windows")]
fn system_menu_icon() -> AnyElement {
    static ICON: std::sync::OnceLock<std::sync::Arc<Image>> = std::sync::OnceLock::new();
    let image = ICON.get_or_init(|| std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, include_bytes!("../../windows/png/ico/ibisOffice_icon_win_48px_transparent.png").to_vec()))).clone();
    div()
        .id("system-menu")
        .w(px(28.))
        .h(px(28.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(4.))
        .hover(|s| s.bg(crate::shell::palette::shade(0.0, 0.08)))
        .child(img(image).w(px(20.)).h(px(20.)))
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            // Below the icon, in the window's client pixels.
            let scale = window.scale_factor();
            let (x, y) = ((TITLE_ROW_PADDING_LEFT * scale) as i32, (36.0 * scale) as i32);
            // Out of this handler: TrackPopupMenu runs a message loop until the
            // menu closes, and the tasks it runs would find the app still
            // borrowed by this event ("RefCell already borrowed", GitHub issue #74).
            cx.spawn(async move |_| show_system_menu(x, y)).detach();
            cx.stop_propagation();
        })
        .into_any_element()
}

/// Shows the active window's system menu (元のサイズに戻す・移動・サイズ変更・
/// 最小化・最大化・閉じる) at `x`, `y` of its client area, and carries out
/// the command chosen. GPUI's Windows platform has no `show_window_menu`.
#[cfg(target_os = "windows")]
fn show_system_menu(x: i32, y: i32) {
    use std::ffi::c_void;
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    const MF_BYCOMMAND: u32 = 0x0000;
    const MF_ENABLED: u32 = 0x0000;
    const MF_GRAYED: u32 = 0x0001;
    const TPM_RETURNCMD: u32 = 0x0100;
    const TPM_RIGHTBUTTON: u32 = 0x0002;
    const WM_SYSCOMMAND: u32 = 0x0112;
    const SC_SIZE: u32 = 0xF000;
    const SC_MOVE: u32 = 0xF010;
    const SC_MAXIMIZE: u32 = 0xF030;
    const SC_RESTORE: u32 = 0xF120;
    const SC_CLOSE: u32 = 0xF060;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetActiveWindow() -> *mut c_void;
        fn GetSystemMenu(hwnd: *mut c_void, revert: i32) -> *mut c_void;
        fn IsZoomed(hwnd: *mut c_void) -> i32;
        fn EnableMenuItem(menu: *mut c_void, id: u32, enable: u32) -> i32;
        fn SetMenuDefaultItem(menu: *mut c_void, item: u32, by_position: u32) -> i32;
        fn ClientToScreen(hwnd: *mut c_void, point: *mut Point) -> i32;
        fn TrackPopupMenu(menu: *mut c_void, flags: u32, x: i32, y: i32, reserved: i32, hwnd: *mut c_void, rect: *const c_void) -> i32;
        fn PostMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> i32;
    }
    // SAFETY: the calls take the window and menu handles Windows gave here,
    // and `point` is a POINT that ClientToScreen writes.
    unsafe {
        let hwnd = GetActiveWindow();
        if hwnd.is_null() {
            return;
        }
        let menu = GetSystemMenu(hwnd, 0);
        if menu.is_null() {
            return;
        }
        // DefWindowProc sets these when it shows the menu itself; here, by hand.
        let zoomed = IsZoomed(hwnd) != 0;
        let state = |enabled: bool| MF_BYCOMMAND | if enabled { MF_ENABLED } else { MF_GRAYED };
        EnableMenuItem(menu, SC_RESTORE, state(zoomed));
        EnableMenuItem(menu, SC_MOVE, state(!zoomed));
        EnableMenuItem(menu, SC_SIZE, state(!zoomed));
        EnableMenuItem(menu, SC_MAXIMIZE, state(!zoomed));
        SetMenuDefaultItem(menu, SC_CLOSE, 0);
        let mut point = Point { x, y };
        ClientToScreen(hwnd, &mut point);
        let command = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, point.x, point.y, 0, hwnd, std::ptr::null());
        if command != 0 {
            PostMessageW(hwnd, WM_SYSCOMMAND, command as usize, 0);
        }
    }
}

/// Renders the Minimize, Maximize/Restore, and Close buttons on platforms requiring CSD.
pub(crate) fn render_window_controls(window: &Window, editor: &WeakEntity<SlideEditor>) -> Option<AnyElement> {
    if !should_render_window_controls(window) {
        return None;
    }

    let is_maximized = window.is_maximized();

    // Minimize button (―)
    let min_btn = div()
        .id("window-control-minimize")
        .w(px(46.))
        .h(px(36.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(crate::shell::palette::shade(0.0, 0.08)))
        .child(Icon::new(Material("remove")).size(px(18.)))
        .on_click(|_, window, _| {
            eprintln!("[DEBUG-WAYLAND] minimize button clicked! calling window.minimize_window()");
            window.minimize_window();
        })
        .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text("_shell.minimize")).build(window, cx));

    #[cfg(target_os = "windows")]
    let min_btn = min_btn.window_control_area(WindowControlArea::Min);

    // Maximize / Restore button (□ / ❐)
    let max_icon = if is_maximized { "filter_none" } else { "crop_square" };
    let max_tip = if is_maximized { "_shell.restore" } else { "_shell.maximize" };
    let max_btn = div()
        .id("window-control-maximize")
        .w(px(46.))
        .h(px(36.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(|s| s.bg(crate::shell::palette::shade(0.0, 0.08)))
        .child(Icon::new(Material(max_icon)).size(px(16.)))
        .on_click(|_, window, _| {
            eprintln!("[DEBUG-WAYLAND] maximize button clicked! calling window.zoom_window()");
            window.zoom_window();
        })
        .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text(max_tip)).build(window, cx));

    #[cfg(target_os = "windows")]
    let max_btn = max_btn.window_control_area(WindowControlArea::Max);

    // Close button (✕, red on hover)
    let editor = editor.clone();
    let close_btn = div()
        .id("window-control-close")
        .w(px(46.))
        .h(px(36.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(rgb(ICON))
        .hover(|s| s.bg(rgb(0xe81123)).text_color(crate::shell::palette::white()))
        .child(Icon::new(Material("close")).size(px(18.)))
        .on_click(move |_, window, cx| {
            // Removing a window directly bypasses the platform close callback.
            // Use the same save guard as its native close button and Alt+F4.
            let _ = editor.update(cx, |editor, cx| {
                if editor.window_should_close(cx) {
                    window.remove_window();
                }
            });
        })
        .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text("_shell.close")).build(window, cx));

    #[cfg(target_os = "windows")]
    let close_btn = close_btn.window_control_area(WindowControlArea::Close);

    Some(
        h_flex()
            .flex_none()
            .items_center()
            .gap(px(0.))
            .child(min_btn)
            .child(max_btn)
            .child(close_btn)
            .into_any_element(),
    )
}

impl SlideEditor {
    /// The menus for the open document: the shell's, then the format's additions.
    pub(crate) fn menus(&self) -> Menus {
        let mut menus = self.menus_unpruned();
        for (_, entries) in &mut menus.menus {
            *entries = prune_entries(std::mem::take(entries));
        }
        menus
    }

    /// The menus as the shell and the format built them, before the empty
    /// submenus are left out (for the scenario that lists them).
    pub(crate) fn menus_unpruned(&self) -> Menus {
        let mut menus = crate::shell::chrome_defaults::menus(self);
        with_format!(self, menus(self, &mut menus));
        crate::shell::menu_bridge::fill(&mut menus, || {
            let mut tools = self.toolbar_items();
            if self.kind() == ibis_pptx::Kind::Presentation {
                tools.extend(crate::pptx::chrome::menu_buttons(self));
            }
            tools
        });
        fn wrap(entries: &mut [MenuEntry]) {
            for entry in entries {
                match entry {
                    MenuEntry::Item(item) => {
                        if let Some(handler) = item.handler.take() {
                            let ruler = item.id == "docx.ruler";
                            item.handler = Some(Rc::new(move |this, window, cx| {
                                if !ruler { this.ruler_settings_open = false; }
                                handler(this, window, cx);
                            }));
                        }
                    }
                    MenuEntry::Submenu(submenu) => wrap(&mut submenu.items),
                    _ => {}
                }
            }
        }
        for (_, entries) in &mut menus.menus { wrap(entries); }
        if self.panel_dialog_is_modal() {
            fn disable(entries: &mut [MenuEntry]) {
                for entry in entries {
                    match entry {
                        MenuEntry::Item(item) => item.disabled = true,
                        MenuEntry::Submenu(submenu) => disable(&mut submenu.items),
                        _ => {}
                    }
                }
            }
            for (_, entries) in &mut menus.menus { disable(entries); }
        }
        menus
    }

    /// The toolbar's parts, in order: the common left part, the format's
    /// left additions, and the right half for what is selected.
    pub(crate) fn toolbar_items(&self) -> Vec<ToolItem> {
        let mut items = crate::shell::chrome_defaults::toolbar_left(self);
        items.extend(with_format!(self, toolbar_left(self)));
        let right = with_format!(self, toolbar_right(self)).unwrap_or_else(|| crate::shell::chrome_defaults::toolbar_right(self));
        if !right.is_empty() {
            items.push(ToolItem::Separator);
            items.extend(right);
        }
        items
    }

    /// The title row: the app's icon for the system menu (Windows), the
    /// start window's button, undo and redo, the slide show (a
    /// presentation), the file name in the middle, comments, the outline
    /// (a document) and the sidebar; on Linux (CSD) and Windows, also includes
    /// the window drag region and control buttons (minimize, maximize/restore, close).
    pub(crate) fn render_title_row(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = cx.entity().downgrade();
        let controls = render_window_controls(window, &editor);
        let pr = if controls.is_some() { px(0.) } else { px(12.) };
        let row = h_flex().h(px(36.)).flex_shrink_0().pl(px(TITLE_ROW_PADDING_LEFT)).pr(pr).gap(px(8.)).items_center().bg(rgb(APP_BG)).text_color(rgb(TEXT));
        if !self.has_document {
            // The start view: the app's logo (開く and 新規作成 are in the view).
            return row
                .child(apply_title_drag(div().flex_1().h_full(), window))
                .child(title_button("help", "help", "_shell.help", None, &editor))
                .children(self.embed_buttons(&editor))
                .children(controls)
                .into_any_element();
        }
        let fill_icon = |name: &'static str, size: f32, color: Hsla| Icon::new(Material::fill(name)).size(px(size)).text_color(color);
        let can_undo = self.document.can_undo() && !self.loading;
        let can_redo = self.document.can_redo() && !self.loading;
        let name = if self.is_workbook() {
            self.display_title_formatted().into()
        } else {
            self.file_name.clone().unwrap_or_default()
        };
        let add_comment = crate::shell::format::with_format!(self, add_comment(self));
        let show_comments = crate::shell::format::with_format!(self, show_comments(self));
        let buttons = |d: Div| {
            // The formats connect them (`FormatEditor::add_comment`,
            // `show_comments`).
            let d = d.child(title_button("add-comment", "add_comment", "_shell.new-comment-m", add_comment.clone(), &editor))
                .child(match &show_comments {
                    Some((true, h)) => title_button_icon("show-comments", fill_icon("comment", 20.0, rgb(ICON).into()), "_shell.hide-comments", Some(h.clone()), &editor),
                    Some((false, h)) => title_button("show-comments", "comment", "_shell.show-comments", Some(h.clone()), &editor),
                    None => title_button("show-comments", "comment", "_shell.show-comments", None, &editor),
                })
                .when(self.kind() == ibis_pptx::Kind::Document, |d| {
                    let open = self.outline_open;
                    d.child(
                        title_button_icon("outline", if open { fill_icon("dock_to_right", 20.0, rgb(ICON).into()) } else { icon("dock_to_right", 20.0, rgb(ICON).into()) }, "docx.navigation.title", Some(Rc::new(|this: &mut SlideEditor, _, cx| {
                            this.outline_open = !this.outline_open;
                            cx.notify();
                        })), &editor),
                    )
                })
                .when(self.pages_are_slides(), |d| {
                    // The notes pane (below the slide): its button stands left of the sidebar's.
                    let open = self.notes_shown();
                    d.child(title_button_icon(
                        "notes-pane",
                        if open { fill_icon("dock_to_bottom", 20.0, rgb(ICON).into()) } else { icon("dock_to_bottom", 20.0, rgb(ICON).into()) },
                        if open { "_shell.hide-notes-pane" } else { "_shell.show-notes-pane" },
                        Some(Rc::new(|this: &mut SlideEditor, window, cx| this.toggle_notes(window, cx))),
                        &editor,
                    ))
                })
                .child({
                    // On and enabled as the pane is (`right_pane`, from `render_sidebar` this frame).
                    let pane = self.right_pane;
                    let color: Hsla = if pane.available { rgb(ICON).into() } else { disabled_color() };
                    title_button_icon(
                        "sidebar",
                        if pane.shown { fill_icon("dock_to_left", 20.0, color) } else { icon("dock_to_left", 20.0, color) },
                        if pane.shown { "_shell.hide-sidebar" } else { "view.sidebar" },
                        pane.available.then(|| -> Handler { Rc::new(|this: &mut SlideEditor, window, cx| this.toggle_sidebar(window, cx)) }),
                        &editor,
                    )
                });
            #[cfg(any(target_os = "macos", target_os = "windows"))]
            let d = {
                let kind = self.kind();
                let tip = match kind {
                    ibis_pptx::Kind::Document => "window.open-in-word",
                    ibis_pptx::Kind::Workbook => "window.open-in-excel",
                    ibis_pptx::Kind::Presentation => "window.open-in-powerpoint",
                };
                let can_open_in_office = self.has_file_on_disk();
                let handler: Handler = Rc::new(|this: &mut SlideEditor, window, cx| this.open_in_office_action(window, cx));
                d.child(
                    div()
                        .id("open-in-office")
                        .size(px(32.))
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .when(can_open_in_office, |d| {
                            d.cursor_pointer()
                                .hover(|s| s.bg(crate::shell::palette::shade(0.27, 0.08)))
                                .on_click(listener(&editor, &handler))
                        })
                        .when(!can_open_in_office, |d| d.opacity(0.4))
                        .child(img(office_app_logo(kind)).size(px(20.)))
                        .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text(tip)).build(window, cx)),
                )
            };
            d
        };
        let slideshow = self.pages_are_slides().then(|| self.render_slideshow_button(&editor, false));
        #[cfg(target_os = "windows")]
        let row = row.child(system_menu_icon());
        #[cfg(not(target_arch = "wasm32"))]
        let row = row.child(self.start_window_button(&editor));
        #[cfg(target_arch = "wasm32")]
        let row = {
            let (app_icon, app_color) = match self.kind() {
                ibis_pptx::Kind::Document => ("description", 0x2f6fd6),
                ibis_pptx::Kind::Workbook => ("table_chart", 0x1e8e3e),
                ibis_pptx::Kind::Presentation => ("slideshow", 0xe8a100),
            };
            row.child(div().w(px(28.)).h(px(28.)).flex().items_center().justify_center().child(fill_icon(app_icon, 24.0, rgb(app_color).into())))
        };
        row.child(
            h_flex()
                .child(title_button("undo", "undo", "_shell.undo-z", can_undo.then(|| -> Handler { Rc::new(|this: &mut SlideEditor, window, cx| this.undo(&crate::Undo, window, cx)) }), &editor))
                .child(title_button("redo", "redo", "_shell.redo-z", can_redo.then(|| -> Handler { Rc::new(|this: &mut SlideEditor, window, cx| this.redo(&crate::Redo, window, cx)) }), &editor))
                .children(self.quick_access_buttons(&editor)),
        )
            .children(slideshow)
            .child(
                apply_title_drag(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .justify_center()
                        .items_center(),
                    window,
                )
                .child(div().text_size(px(15.)).px(px(6.)).truncate().child(name))
                .when(self.is_dirty(), |d| d.child(div().id("title-dirty-mark").text_size(px(12.)).ml(px(-2.)).text_color(rgb(ICON)).child("●"))),
            )
            .child(buttons(h_flex()))
            .children(self.embed_buttons(&editor))
            .children(controls)
            .into_any_element()
    }

    /// The buttons the embedding page asked for at the top right of the browser version: download, then close.
    fn embed_buttons(&self, editor: &WeakEntity<SlideEditor>) -> Vec<AnyElement> {
        #[cfg(target_arch = "wasm32")]
        {
            let mut buttons = Vec::new();
            if crate::shell::embed::show_download() {
                let handler: Handler = Rc::new(|this: &mut SlideEditor, window, cx| this.embed_download(window, cx));
                buttons.push(title_button("embed-download", "download", "shell.embed.download", Some(handler), editor));
            }
            if crate::shell::embed::show_close() {
                let handler: Handler = Rc::new(|this: &mut SlideEditor, window, cx| this.embed_close(window, cx));
                buttons.push(title_button("embed-close", "close", "shell.embed.close", Some(handler), editor));
            }
            buttons
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = editor;
            Vec::new()
        }
    }

    /// The format's icon in the title bar (blue document, green workbook, orange presentation). Only a mark, not a button:
    /// the start window opens from the menu bar's logo and ファイル > スタートウィンドウ.
    #[cfg(not(target_arch = "wasm32"))]
    fn start_window_button(&self, _editor: &WeakEntity<SlideEditor>) -> AnyElement {
        let (app_icon, app_color) = match self.kind() {
            ibis_pptx::Kind::Document => ("description", 0x2f6fd6),
            ibis_pptx::Kind::Workbook => ("table_chart", 0x1e8e3e),
            ibis_pptx::Kind::Presentation => ("slideshow", 0xe8a100),
        };
        div().w(px(32.)).h(px(32.)).flex_none().flex().items_center().justify_center()
            .child(Icon::new(Material::fill(app_icon)).size(px(24.)).text_color(rgb(app_color)))
            .into_any_element()
    }

    /// The commands of the クイック アクセス ツール バー the open document has, as buttons beside undo and redo.
    pub(crate) fn quick_access_buttons(&self, editor: &WeakEntity<SlideEditor>) -> Vec<AnyElement> {
        let ids = crate::shell::qat::commands();
        if ids.is_empty() {
            return Vec::new();
        }
        let menus = self.menus();
        ids.iter()
            .filter_map(|id| {
                // A submenu whose first command has the submenu's own id (ウィンドウ枠の固定) is
                // shown as that command, with the submenu's icon.
                match menus.find_entry(id)? {
                    MenuEntry::Submenu(sub) => {
                        let mut inner = sub.items.iter().find_map(|e| match e {
                            MenuEntry::Item(i) if i.id == id.as_str() => Some(i.clone()),
                            _ => None,
                        })?;
                        if inner.icon.is_none_or(str::is_empty) {
                            inner.icon = sub.icon;
                        }
                        Some(MenuEntry::Item(inner))
                    }
                    other => Some(other),
                }
            })
            .filter_map(|entry| match entry {
                MenuEntry::Item(item) => {
                    let icon_name = item.icon.filter(|n| !n.is_empty())?;
                    let handler = if item.disabled { None } else { item.handler.clone() };
                    let color: Hsla = if handler.is_some() { rgb(ICON).into() } else { disabled_color() };
                    Some(title_button_icon(item.id, icon(icon_name, 20.0, color), item.id, handler, editor))
                }
                _ => None,
            })
            .collect()
    }

    /// スライドショー, with ▼ for how to start; `icon_only` leaves out its
    /// label (in the toolbar's title row).
    fn render_slideshow_button(&self, editor: &WeakEntity<SlideEditor>, icon_only: bool) -> AnyElement {
        let disabled = self.loading;
        let start: Handler = Rc::new(|this: &mut SlideEditor, window, cx| this.start_slideshow(window, cx));
        let from_start: Handler = Rc::new(|this: &mut SlideEditor, window, cx| {
            this.show_slide_number(1, window, cx);
            this.start_slideshow(window, cx);
        });
        let mut items: Vec<MenuEntry> = vec![
            MenuItem::tr("slideshow.from-start").icon("play_arrow").shortcut("⇧⌘↩").on(move |e, w, c| from_start(e, w, c)).into(),
            MenuItem::tr("slideshow.from-current").icon("slideshow").shortcut("⌘↩").on({
                let start = start.clone();
                move |e, w, c| start(e, w, c)
            })
            .into(),
            MenuItem::tr("slideshow.presenter").icon("co_present").shortcut("⌥⌘↩").on(|e, w, c| e.start_presenter_view(w, c)).into(),
        ];
        let custom_shows = self.presentation().custom_shows.clone();
        if !custom_shows.is_empty() {
            let mut sub_items: Vec<MenuEntry> = Vec::new();
            for cs in custom_shows {
                let id = cs.id;
                sub_items.push(MenuItem::new("slideshow.custom-show-item", cs.name).icon("filter_none").on(move |e, w, c| e.start_custom_slideshow(id, w, c)).into());
            }
            items.push(Submenu::tr("slideshow.custom-shows", "filter_none", sub_items).into());
        }
        items.push(MenuEntry::Separator);
        items.push(MenuItem::tr("slideshow.rehearse-timings").icon("schedule").on(|e, w, c| e.start_rehearsal(w, c)).into());
        items.push(MenuItem::tr("slideshow.settings").icon("settings").on(|e, w, c| e.open_show_panel(w, c)).into());
        let theme_border = rgb(0x747775);
        let caret_editor = editor.clone();
        h_flex()
            .h(px(30.))
            .rounded(px(15.))
            .border_1()
            .border_color(theme_border)
            .bg(crate::shell::palette::white())
            .when(disabled, |d| d.opacity(0.5))
            .child(
                div()
                    .id("slideshow")
                    .h_full()
                    .pl(px(if icon_only { 8. } else { 10. }))
                    .pr(px(if icon_only { 6. } else { 8. }))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded_l(px(15.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|s| s.bg(crate::shell::palette::shade(0.0, 0.04)))
                    .child(icon("slideshow", 18.0, rgb(ICON).into()))
                    .when(!icon_only, |d| d.child(crate::shell::i18n::text("_shell.slide-show")))
                    .when(!disabled, |d| d.on_click(listener(editor, &start)))
                    .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text("_shell.play-from-current-slide")).build(window, cx)),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(crate::shell::palette::shade(0.0, 0.15)))
            .child(
                Button::new("slideshow-caret")
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(Material("arrow_drop_down")))
                    .tooltip(crate::shell::i18n::text("_shell.choose-how-to-start"))
                    .disabled(disabled)
                    .rounded_r(px(15.))
                    .dropdown_menu(move |menu, window, cx| fill_menu(menu.min_w(px(300.)), &items, &caret_editor, window, cx)),
            )
            .into_any_element()
    }

    /// The toolbar: the common left part, the format's right part, the
    /// buttons that do not fit folded into ︙, and ^ to hide the menus.
    pub(crate) fn render_toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        set_current_toolbar_open(self.font_size_menu_open, self.toolbar_search_open);
        let editor = cx.entity().downgrade();
        let mut items = self.toolbar_items();
        items.retain(|item| !matches!(item, ToolItem::Button(b) if b.hidden));
        // On Windows, and on Linux where the window's buttons are drawn here
        // (CSD), with the menus hidden, the toolbar is the title row: between
        // the system menu's icon (Windows) and the start window's button and
        // the window's buttons.
        let in_title_row = self.chrome_compact && should_render_window_controls(window);
        // What fits: the bar is the window less 8 px each side, 6 px padding
        // each side, and ︙ and ^ on the right (and in the title row, the
        // icons and the three window buttons).
        let system_menu_icon_width = if cfg!(target_os = "windows") { 28.0 + 8.0 } else { 0.0 };
        // The slide show's button, its icon and ▾ (a presentation).
        let slideshow_width = if self.pages_are_slides() { 8.0 + 64.0 } else { 0.0 };
        // On a Mac, with the title row and the menus hidden the toolbar is at the window's top, where the window's three buttons are
        // (the traffic lights): the bar starts right of them (not with the window full screen, which hides them).
        let mac_inset = toolbar_left_inset(self.chrome_compact, window.is_fullscreen());
        let title_parts = if in_title_row { TITLE_ROW_PADDING_LEFT + system_menu_icon_width + 28.0 + slideshow_width + 3.0 * 46.0 } else { 0.0 };
        // Linux (CSD) draws the window's shadow inside the viewport, at its sides.
        let paddings = gpui_kit::component::window_paddings(window);
        let available = f32::from(window.viewport_size().width - paddings.left - paddings.right) - 16.0 - 12.0 - 2.0 * 29.0 - title_parts - (mac_inset - 8.0).max(0.0);
        let mut used = 0.0;
        let mut shown = Vec::new();
        let mut folded = Vec::new();
        for item in items {
            let w = item.width() + 1.0;
            if folded.is_empty() && used + w <= available {
                used += w;
                shown.push(item);
            } else {
                folded.push(item);
            }
        }
        while matches!(shown.last(), Some(ToolItem::Separator)) {
            shown.pop();
        }
        while matches!(folded.first(), Some(ToolItem::Separator)) {
            folded.remove(0);
        }
        let compact = self.chrome_compact || !self.show_menu_bar;
        let more = (!folded.is_empty()).then(|| {
            let folded = Rc::new(folded);
            let editor = editor.clone();
            Popover::new("toolbar-more")
                .anchor(Anchor::TopRight)
                .trigger(Button::new("toolbar-more-button").ghost().xsmall().icon(Icon::new(Material("more_vert"))).tooltip(crate::shell::i18n::text("_shell.more")).h(px(28.)).w(px(28.)).rounded(px(4.)))
                .content(move |_, _, _| {
                    // The rest of the toolbar in one row, as the mock's ︙.
                    h_flex()
                        .h(px(36.))
                        .px(px(6.))
                        .gap(px(1.))
                        .items_center()
                        .children(folded.iter().enumerate().map(|(i, item)| tool_item(item, i + 1000, &editor)))
                })
        });
        // In the title row, the bar's empty middle moves the window.
        let gap = if in_title_row { apply_title_drag(div().flex_1().h_full(), window) } else { div().flex_1() };
        let toolbar_bounds = self.toolbar_bounds.clone();
        let bar = h_flex()
            .relative()
            .h(px(36.))
            .px(px(6.))
            .gap(px(1.))
            .items_center()
            .rounded(px(18.))
            .bg(rgb(TOOLBAR_BG))
            .child(canvas(|_, _, _| {}, move |bounds, _, _, _| toolbar_bounds.set(bounds)).absolute().top_0().left_0().size_full())
            .children(shown.iter().enumerate().map(|(i, item)| tool_item(item, i, &editor)))
            .child(gap)
            .children(more)
            .child(tool_item(
                &ToolButton::new("toggle-menus", if compact { "keyboard_arrow_down" } else { "keyboard_arrow_up" }, if compact { "_shell.show-menus-f" } else { "_shell.hide-menus-f" })
                    .click(|this, _, cx| {
                        if !this.show_menu_bar {
                            this.show_menu_bar = true;
                            this.chrome_compact = false;
                        } else {
                            this.chrome_compact = !this.chrome_compact;
                        }
                        cx.notify();
                    })
                    .into(),
                9999,
                &editor,
            ));
        #[cfg(any(target_os = "windows", target_os = "linux", target_os = "freebsd", feature = "scenarios"))]
        if in_title_row {
            let icons = h_flex().flex_none().gap(px(8.)).pl(px(TITLE_ROW_PADDING_LEFT)).pt(px(8.));
            #[cfg(target_os = "windows")]
            let icons = icons.child(system_menu_icon());
            // The icons centred in the row's 44 px; the window's buttons at its top, as Windows' own.
            return h_flex()
                .flex_shrink_0()
                .items_start()
                .bg(rgb(APP_BG))
                .child(icons.child(self.start_window_button(&editor)).children(self.pages_are_slides().then(|| self.render_slideshow_button(&editor, true))))
                .child(div().flex_1().min_w_0().px(px(8.)).py(px(4.)).child(bar))
                .children(render_window_controls(window, &editor))
                .into_any_element();
        }
        div().pl(px(mac_inset)).pr(px(8.)).pt(px(2.)).pb(px(6.)).flex_shrink_0().bg(rgb(APP_BG)).child(bar).into_any_element()
    }

    /// The sidebar, right of the document: the panel, under the tabs the
    /// format has for it. Also records `right_pane` (shown, and whether the toggle can show it), which the title row's
    /// button and 表示 > サイドバー read.
    pub(crate) fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.panel_dialog.active {
            let restore_focus = self.panel_dialog.focus.as_ref().is_some_and(|focus| focus.contains_focused(window, cx));
            let editor = cx.weak_entity();
            // A dialog can disappear by replacing its panel state, including after Apply.
            // Restore document keys centrally instead of leaving focus in a removed input.
            window.defer(cx, move |window, cx| {
                let _ = editor.update(cx, |e, cx| {
                    if !e.panel_dialog.active {
                        e.panel_dialog.form_controls.clear();
                        e.panel_dialog.submit_handler = None;
                        e.panel_dialog.has_submit_action = false;
                        if restore_focus { e.focus_handle.focus(window, cx); }
                    }
                });
            });
        }
        if !self.panel_dialog.active { self.panel_dialog.bounds = None; }
        self.panel_dialog.overlay = None;
        self.panel_dialog.active = false;
        self.panel_dialog.key = None;
        PANEL_KEY.with(|key| key.set(None));
        super::panel_dialog::form::OPTIONS.with(|o|o.set(Default::default()));
        PANEL_MODE.with(|mode| mode.set(super::panel_dialog::PanelMode::Sidebar));
        let editor = cx.entity().downgrade();
        let tabs_of = with_format!(self, sidebar_tabs());
        let on_request = with_format!(self, sidebar_on_request()) || !tabs_of.is_empty();
        // Closed, a format with nothing to show on request is asked whether a panel is open (the toggle can show
        // only that); one that always has something is not asked (its panel would be made for nothing).
        #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
        super::scenario::button_probe::input_scope(true);
        let panel = if self.sidebar_open || !on_request {
            SIDEBAR_FOOTER.with(|cell| *cell.borrow_mut() = None);
            #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
            let forced = FORCED_SIDEBAR.with(|slot| slot.get());
            #[cfg(not(all(feature = "scenarios", any(target_os = "macos", target_os = "linux"))))]
            let forced: Option<(fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) -> Option<SidebarPanel>, &'static str, super::panel_dialog::PanelMode)> = None;
            if let Some((render, key, mode)) = forced {
                render(self, window, cx).map(|p| match mode {
                    super::panel_dialog::PanelMode::Modal => p.modal_named(key),
                    super::panel_dialog::PanelMode::Modeless => p.modeless_named(key),
                    super::panel_dialog::PanelMode::Sidebar => p,
                })
            } else if let Some(panel)=self.color_picker_dialog(cx) {
                Some(panel.modal_named("color_picker_dialog"))
            } else if self.ruler_settings_open && self.is_document() {
                self.ruler_panel(cx)
            } else {
                with_format!(self, sidebar(self, window, cx)).or_else(|| crate::shell::chrome_defaults::sidebar(self, window, cx))
            }
        } else {
            None
        };
        #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
        super::scenario::button_probe::input_scope(false);
        let has_content = panel.is_some() || !tabs_of.is_empty();
        self.right_pane = RightPane { available: on_request || panel.is_some(), shown: self.sidebar_open && has_content };
        if !self.right_pane.shown {
            self.panel_dialog.form_controls.clear();self.panel_dialog.submit_handler=None;self.panel_dialog.has_submit_action=false;
            return None;
        }
        let mut panel = match panel {
            Some(panel) => panel,
            // Nothing open: the tabs alone, for a format that has them.
            None => SidebarPanel { title: "".into(), body: div().into_any_element() },
        };
        #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
        LAST_SIDEBAR_TITLE.with(|title| *title.borrow_mut() = panel::panel_title(&panel.title).to_string());
        let mut footer = SIDEBAR_FOOTER.with(|cell| cell.borrow_mut().take());
        let mode = PANEL_MODE.with(|mode| mode.get());
        self.panel_dialog.key = PANEL_KEY.with(|key| key.get());
        if mode != super::panel_dialog::PanelMode::Sidebar {
            let child_key = self.panel_dialog.key;
            let retain_chart = self.is_workbook() && self.sheet_state().is_some_and(|state| state.chart_edit.is_some())
                && matches!(child_key, Some("move_chart_panel" | "chart_template_panel" | "chart_series_editor"));
            let retain_sort = self.is_workbook() && child_key == Some("sort_level_editor");
            let background = if retain_chart { self.sheet_chart_panel(cx) } else if retain_sort { self.sort_background_panel(cx) } else { None };
            let background_footer = SIDEBAR_FOOTER.with(|cell| cell.borrow_mut().take());
            self.panel_dialog.key = child_key;
            self.panel_dialog.overlay = Some(self.render_panel_dialog(panel, footer, mode, window, cx));
            if let Some(background) = background {
                panel = background;
                footer = background_footer;
                PANEL_MODE.with(|mode| mode.set(super::panel_dialog::PanelMode::Sidebar));
                PANEL_KEY.with(|key| key.set(None));
            } else {
                self.right_pane.shown = false;
                return None;
            }
        }
        if self.is_document() {
            if let Some((child,key))=self.merge_workflow_dialog(cx) {
                let child_footer=SIDEBAR_FOOTER.with(|cell|cell.borrow_mut().take());
                self.panel_dialog.key=Some(key);
                self.panel_dialog.overlay=Some(self.render_panel_dialog(child,child_footer,super::panel_dialog::PanelMode::Modal,window,cx));
            }
        }
        if self.is_document() {
            if let Some(child)=self.tab_row_dialog(cx) {
                let child_footer=SIDEBAR_FOOTER.with(|cell|cell.borrow_mut().take());
                self.panel_dialog.key=Some("tab_stop_editor");
                self.panel_dialog.overlay=Some(self.render_panel_dialog(child,child_footer,super::panel_dialog::PanelMode::Modal,window,cx));
            }
        }
        if self.is_workbook() && !self.panel_dialog.active {
            if let Some(child) = self.chart_series_editor_panel(cx) {
                let child_footer = SIDEBAR_FOOTER.with(|cell| cell.borrow_mut().take());
                self.panel_dialog.key = Some("chart_series_editor");
                let child_mode = if self.sheet_range_pick_active() {
                    super::panel_dialog::PanelMode::Modeless
                } else {
                    super::panel_dialog::PanelMode::Modal
                };
                self.panel_dialog.overlay = Some(self.render_panel_dialog(child, child_footer, child_mode, window, cx));
                PANEL_MODE.with(|mode| mode.set(super::panel_dialog::PanelMode::Sidebar));
                PANEL_KEY.with(|key| key.set(None));
            }
        }
        if self.is_workbook() && self.sheet_range_pick_active() && !self.panel_dialog.active {
            if let Some(picker)=self.generic_range_pick_panel(cx) {
                let footer=take_sidebar_footer();
                self.panel_dialog.key=Some("generic_range_picker");
                self.panel_dialog.overlay=Some(self.render_panel_dialog(picker,footer,super::panel_dialog::PanelMode::Modeless,window,cx));
            }
        }
        let pane_blocked = self.is_workbook() && self.panel_dialog.active
            && matches!(self.panel_dialog.key, Some("chart_series_editor" | "move_chart_panel" | "chart_template_panel" | "sort_level_editor"));
        // The tabs show with their panels, or with none open; another
        // panel (the name manager, ...) has its own title only.
        let selected = tabs_of.iter().position(|t| crate::shell::i18n::text(t.panel_title) == crate::shell::i18n::text(&panel.title));
        let tabs = (selected.is_some() || panel.title.is_empty()).then(|| {
            h_flex().h(px(40.)).px(px(6.)).gap(px(1.)).flex_shrink_0().border_b_1().border_color(rgb(BORDER)).children(tabs_of.iter().enumerate().map(|(i, tab)| {
                let on = selected == Some(i);
                let handler = tab.open.clone();
                div()
                    .id(("sidebar-tab", i))
                    .h_full()
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .whitespace_nowrap()
                    .text_color(if on { rgb(0x0b57d0) } else { rgb(TEXT_SECONDARY) })
                    .when(on, |d| d.font_weight(FontWeight::MEDIUM).border_b_2().border_color(rgb(0x0b57d0)))
                    .hover(|s| s.bg(crate::shell::palette::shade(0.27, 0.06)))
                    .child(crate::shell::i18n::text(tab.label))
                    .on_click(listener(&editor, &handler))
            }))
        });
        let close: Handler = Rc::new(|this: &mut SlideEditor, window, cx| {
            this.sidebar_open = false;
            cx.notify();
            window.refresh();
        });
        let sidebar_scroll = self.sidebar_scroll.clone();
        Some(
            v_flex()
                .relative()
                .w(px(SIDEBAR_WIDTH))
                .h_full()
                .flex_shrink_0()
                .bg(crate::shell::palette::white())
                .border_l_1()
                .border_color(rgb(BORDER))
                .rounded_tl(px(16.))
                .child(
                    h_flex()
                        .h(px(52.))
                        .pl(px(18.))
                        .pr(px(10.))
                        .items_center()
                        .justify_between()
                        .child(div().text_size(px(16.)).font_weight(FontWeight::MEDIUM).text_color(rgb(TEXT)).child(panel::panel_title(&panel.title)))
                        .child(title_button("sidebar-close", "close", "_shell.close", Some(close), &editor)),
                )
                .children(tabs)
                .child(panel::panel_contents(panel.body,footer,sidebar_scroll,self.sidebar_layout.clone(),editor.clone()))
                .when(pane_blocked, |d| d.child(div().absolute().inset_0().occlude()
                    .on_mouse_down(MouseButton::Left, |_,_,cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_,_,cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, |_,_,cx| cx.stop_propagation())
                    .on_scroll_wheel(|_,_,cx| cx.stop_propagation())))
                .into_any_element(),
        )
    }
}

/// Shared height for labeled command buttons across all document formats.
pub(crate) const BUTTON_HEIGHT: f32 = 28.;

#[derive(Clone, Copy, Default)]
pub(crate) enum ActionButtonKind { Primary, #[default] Normal, Destructive }
#[derive(Clone, Copy, Default)]
pub(crate) struct ActionButtonStyle { pub kind: ActionButtonKind }
impl ActionButtonStyle {
    fn apply(self, button: Button, cx: Option<&App>) -> Button {
        match self.kind {
            ActionButtonKind::Primary => button.primary(),
            ActionButtonKind::Normal => button,
            ActionButtonKind::Destructive => button.custom(gpui_kit::component::button::ButtonCustomVariant::new(cx.expect("destructive style requires application context"))
                .color(rgb(0xd93025).into()).foreground(rgb(0xd93025).into()).hover(cx.unwrap().theme().button_hover).active(cx.unwrap().theme().button_active))
                .border_1().bg(rgb(0xffffff)).text_color(rgb(0xd93025)).border_color(rgb(0xd93025)),
        }
    }
}
pub(crate) const SELECTED_ROW_BG: u32 = 0xe8f0fe;
pub(crate) fn styled_action_button(id: impl Into<ElementId>, style: ActionButtonStyle, cx: Option<&App>) -> Button {
    let id=id.into();
    let button=style.apply(Button::new(id.clone()).h(px(BUTTON_HEIGHT)).min_h(px(BUTTON_HEIGHT)).max_h(px(BUTTON_HEIGHT)).flex_none(),cx);
    #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
    let button=super::scenario::button_probe::observe(button,id);
    button
}
pub(crate) fn action_button(id: impl Into<ElementId>) -> Button { styled_action_button(id, ActionButtonStyle::default(),None) }
pub(crate) fn primary_button(id: impl Into<ElementId>) -> Button { styled_action_button(id, ActionButtonStyle {kind:ActionButtonKind::Primary},None) }
pub(crate) fn destructive_button(id: impl Into<ElementId>, cx: &App) -> Button { styled_action_button(id, ActionButtonStyle {kind:ActionButtonKind::Destructive},Some(cx)) }
pub(crate) fn dialog_close_button(cx: &Context<SlideEditor>) -> Button {
    action_button("dialog-close").label(crate::shell::i18n::text("_shell.close"))
        .on_click(cx.listener(|this, _, window, cx| this.close_panel_dialog(window, cx)))
}

pub(crate) fn dialog_cancel_button(cx: &Context<SlideEditor>) -> Button {
    action_button("dialog-cancel").label(crate::shell::i18n::text("_shell.cancel"))
        .on_click(cx.listener(|this, _, window, cx| this.close_panel_dialog(window, cx)))
}

pub(crate) fn dialog_cancel_definition(cx: &Context<SlideEditor>) -> ActionDefinition {
    ActionDefinition::new("dialog-cancel", crate::shell::i18n::text("_shell.cancel"), ActionButtonKind::Normal)
        .on_click(cx.listener(|this, _, window, cx| this.close_panel_dialog(window, cx)))
}
pub(crate) fn dialog_close_definition(cx: &Context<SlideEditor>) -> ActionDefinition {
    ActionDefinition::new("dialog-close", crate::shell::i18n::text("_shell.close"), ActionButtonKind::Normal)
        .on_click(cx.listener(|this, _, window, cx| this.close_panel_dialog(window, cx)))
}

/// The shared terminal action area, in a pane or a dialog. No controls follow this area.
pub(crate) fn action_bar() -> ActionBar { ActionBar::new() }

/// The outer content inset is owned by the pane/dialog host, never a screen-specific form.
pub(crate) fn panel_body(body:AnyElement)->Div {
 div().w_full().min_w_0().min_h_0().p(px(form::FORM_PADDING)).child(body)
}

pub(crate) const ACTION_BAR_PADDING_Y: f32 = 12.;

/// Every registered footer receives its insets here, including legacy custom button rows.
pub(crate) fn action_bar_host(footer:ActionBar)->Div {
 h_flex().relative().w_full().flex_none().justify_center().items_center().py(px(ACTION_BAR_PADDING_Y)).border_t_1().border_color(rgb(BORDER)).bg(rgb(0xffffff)).child(footer)
}

/// Wizards progress left-to-right: Back first, primary last at the right edge.
pub(crate) fn wizard_action_bar() -> ActionBar {
    action_bar().justify_end()
}

/// A round icon button of the title row (32 px).
pub(crate) fn title_button(id: &'static str, name: &'static str, tooltip: &'static str, handler: Option<Handler>, editor: &WeakEntity<SlideEditor>) -> AnyElement {
    let color: Hsla = if handler.is_some() { rgb(ICON).into() } else { disabled_color() };
    title_button_icon(id, icon(name, 20.0, color), tooltip, handler, editor)
}

fn title_button_icon(id: &'static str, icon: Icon, tooltip: &'static str, handler: Option<Handler>, editor: &WeakEntity<SlideEditor>) -> AnyElement {
    let enabled = handler.is_some();
    div()
        .id(id)
        .size(px(32.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(crate::shell::palette::shade(0.27, 0.08))))
        .child(icon)
        .when_some(handler, |d, h| {
            let guarded: Handler = Rc::new(move |this, window, cx| {
                if !this.panel_dialog_is_modal() { h(this, window, cx); }
            });
            d.on_click(listener(editor, &guarded))
        })
        .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text(&tooltip)).build(window, cx))
        .into_any_element()
}

thread_local! {
    static CURRENT_TOOLBAR_OPEN: std::cell::Cell<(bool, Option<&'static str>)> = const { std::cell::Cell::new((false, None)) };
}

pub(crate) fn set_current_toolbar_open(font_size_open: bool, search_open: Option<&'static str>) {
    CURRENT_TOOLBAR_OPEN.with(|c| c.set((font_size_open, search_open)));
}

pub(crate) const FONT_SIZE_CANDIDATES: &[f32] = &[
    6.0, 7.0, 8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0,
];

pub(crate) fn font_size_candidate_label(pt: f32) -> String {
    if (pt - pt.round()).abs() < 1e-4 {
        format!("{}", pt.round() as i32)
    } else {
        format!("{pt:.1}")
    }
}

/// Return the font size candidates matching `query` when `filtering` is active, or all candidates otherwise (#127, #149).
pub(crate) fn filtered_font_size_candidates(query: &str, filtering: bool) -> Vec<f32> {
    let trimmed = query.trim();
    if !filtering || trimmed.is_empty() {
        return FONT_SIZE_CANDIDATES.to_vec();
    }
    let q = trimmed.to_lowercase();
    FONT_SIZE_CANDIDATES
        .iter()
        .copied()
        .filter(|&pt| font_size_candidate_label(pt).to_lowercase().contains(&q))
        .collect()
}

/// Step to the next/previous enabled `MenuItem` index by `delta` (skipping section headers and disabled items) (#127).
pub(crate) fn step_enabled_menu_index(items: &[MenuItem], current: Option<usize>, delta: isize) -> Option<usize> {
    let enabled_indices: Vec<usize> = items
        .iter()
        .enumerate()
        .filter_map(|(i, it)| it.enabled().then_some(i))
        .collect();
    if enabled_indices.is_empty() {
        return None;
    }
    let cur_pos = current.and_then(|c| {
        enabled_indices
            .iter()
            .position(|&i| i >= c)
            .or_else(|| Some(enabled_indices.len() - 1))
    });
    let next_pos = match cur_pos {
        Some(pos) => (pos as isize + delta).clamp(0, enabled_indices.len() as isize - 1) as usize,
        None => {
            if delta >= 0 {
                0
            } else {
                enabled_indices.len() - 1
            }
        }
    };
    Some(enabled_indices[next_pos])
}

pub(crate) fn scroll_row_into_view(handle: &ScrollHandle, row_idx: usize, row_h: f32, viewport_h: f32) {
    let top = row_idx as f32 * row_h;
    let bottom = top + row_h;
    let current_scroll = (-f32::from(handle.offset().y)).max(0.0);
    let new_scroll = if top < current_scroll {
        top
    } else if bottom > current_scroll + viewport_h {
        (bottom - viewport_h).max(0.0)
    } else {
        current_scroll
    };
    handle.set_offset(point(px(0.), px(-new_scroll)));
}

/// Renders an always-visible vertical scrollbar thumb overlay for `handle` whenever content overflows (#125, #149, #151).
pub(crate) fn always_visible_scrollbar_overlay(
    id: impl Into<ElementId>, handle: ScrollHandle, known_heights: Option<(f32,f32)>,
) -> scrollbar::ScrollbarOverlay {
    scrollbar::ScrollbarOverlay { id:id.into(), handle, known_heights, horizontal:false }
}

/// Persistent horizontal thumb, painted only when content exceeds its viewport width.
pub(crate) fn horizontal_scrollbar_overlay(id: impl Into<ElementId>, handle: ScrollHandle) -> scrollbar::ScrollbarOverlay {
    scrollbar::ScrollbarOverlay { id:id.into(), handle, known_heights:None, horizontal:true }
}

/// One part of the toolbar.
pub(crate) fn tool_item(item: &ToolItem, index: usize, editor: &WeakEntity<SlideEditor>) -> AnyElement {
    match item {
        ToolItem::Separator => div().w(px(1.)).h(px(20.)).mx(px(4.)).bg(rgb(DIVIDER)).flex_none().into_any_element(),
        ToolItem::Button(b) => tool_button(b, index, editor),
        ToolItem::Select { id, value, width, tooltip, menu, disabled } => {
            let enabled = !disabled && !menu.is_empty();
            let color: Hsla = if enabled { rgb(TEXT).into() } else { disabled_color() };
            let is_font = *id == "font";
            let label=if is_font {crate::shell::i18n::font_label(value)}else{value.clone()};
            let button=dropdown_button(SharedString::from(format!("{id}-{index}")),label)
                .h(px(28.)).w(px(*width)).tooltip(crate::shell::i18n::text(tooltip)).disabled(!enabled);
            if enabled {
                if is_font { FontDropdown::new(id,value,menu.clone(),index,editor).button(button).into_any_element() }
                else { Dropdown::with_button(id,button,menu.clone(),false,index,editor).into_any_element() }
            } else {
                button.into_any_element()
            }
        }
        ToolItem::FontSize { value, smaller, larger } => font_size::FontSizeControl {search:false,value:value.clone(),smaller:smaller.clone(),larger:larger.clone(),index,editor:editor.clone()}.into_any_element(),
    }
}

fn tool_button(b: &ToolButton, index: usize, editor: &WeakEntity<SlideEditor>) -> AnyElement {
    let enabled = b.enabled();
    let color: Hsla = if !enabled {
        disabled_color()
    } else if b.on {
        rgb(0x041e49).into()
    } else {
        rgb(ICON).into()
    };
    let content = h_flex()
        .items_center()
        .justify_center()
        .when_some(b.icon, |d, name| {
            match b.color {
                // The icon over a colour bar.
                Some(bar) => d.child(v_flex().items_center().pt(px(2.)).child(icon(name, 18.0, color)).child(div().w(px(18.)).h(px(3.)).mt(px(-1.)).rounded(px(1.)).bg(bar))),
                None => d.child(icon(name, 20.0, color)),
            }
        })
        .when_some(b.text.clone(), |d, text| d.child(div().px(px(3.)).text_size(px(14.)).text_color(if enabled { rgb(TEXT).into() } else { disabled_color() }).child(crate::shell::i18n::text(&text))))
        .when(b.menu.is_some() && b.id != "menu-search", |d| d.child(div().ml(px(-2.)).child(icon("arrow_drop_down", 16.0, color))));
    // A split button: the icon (what was last used is done at once) and the ▼ (the menu) are two buttons side by side,
    // each with its own hover and click.
    if let (Some(menu), Some(quick), true) = (&b.menu, b.left_action(), enabled) {
        let left = Button::new(SharedString::from(format!("{}-{index}", b.id)))
            .ghost()
            .xsmall()
            .h(px(28.))
            .min_w(px(28.))
            .px(px(3.))
            .rounded_l(px(4.))
            .rounded_r(px(0.))
            .tooltip(crate::shell::i18n::text(&b.tooltip))
            .selected(b.on)
            .child(
                h_flex()
                    .items_center()
                    .justify_center()
                    .when_some(b.icon, |d, name| match b.color {
                        Some(bar) => d.child(v_flex().items_center().pt(px(2.)).child(icon(name, 18.0, color)).child(div().w(px(18.)).h(px(3.)).mt(px(-1.)).rounded(px(1.)).bg(bar))),
                        None => d.child(icon(name, 20.0, color)),
                    })
                    .when_some(b.text.clone(), |d, text| d.child(div().px(px(3.)).text_size(px(14.)).text_color(rgb(TEXT)).child(crate::shell::i18n::text(&text)))),
            )
            .on_click(listener(editor, quick));
        let (menu, editor) = (Rc::new(menu.clone()), editor.clone());
        let right = action_button(SharedString::from(format!("{}-{index}-menu", b.id)))
            .ghost()
            .xsmall()
            .h(px(28.))
            .w(px(TOOLBAR_CARET_WIDTH))
            .px(px(0.))
            .rounded_r(px(4.))
            .rounded_l(px(0.))
            .tooltip(crate::shell::i18n::text(&b.tooltip))
            .child(icon("arrow_drop_down", 16.0, color));
        let right=if menu.iter().any(|e|matches!(e,MenuEntry::Palette(_))){ColorPaletteDropdown::from_palette(b.id,right,(*menu).clone(),&editor).into_any_element()}else{right.dropdown_menu(move|m,w,cx|fill_menu(m,&menu,&editor,w,cx)).into_any_element()};
        return h_flex().items_center().child(left).child(right).into_any_element();
    }
    let button = Button::new(SharedString::from(format!("{}-{index}", b.id)))
        .ghost()
        .xsmall()
        .h(px(28.))
        .min_w(px(28.))
        .px(px(3.))
        .tooltip(crate::shell::i18n::text(&b.tooltip))
        .selected(b.on)
        .disabled(!enabled)
        .child(content);
    let button = match b.segment {
        SegmentRounding::All => button.rounded(px(4.)),
        SegmentRounding::Left => button.rounded_l(px(4.)).rounded_r(px(0.)),
        SegmentRounding::Middle => button.rounded(px(0.)),
        SegmentRounding::Right => button.rounded_r(px(4.)).rounded_l(px(0.)),
    };
    if enabled && b.searchable {
        return Dropdown::with_button(b.id, button, b.menu.clone().unwrap_or_default(), b.single_select, index, editor).into_any_element();
    }
    match (&b.menu, &b.handler) {
        (Some(menu), _) if enabled => {
            let items=flatten_menu_entries(menu);
            if items.iter().any(|i|dropdown::is_font_item_id(i.id)) {
                FontDropdown::new(b.id,"",menu.clone(),index,editor).button(button).into_any_element()
            } else if b.single_select || (menu.iter().all(|entry|matches!(entry,MenuEntry::Item(_))) && items.iter().any(|i|i.checked)) {
                Dropdown::with_button(b.id,button,menu.clone(),b.single_select,index,editor).into_any_element()
            } else {
                let (menu,editor)=(Rc::new(menu.clone()),editor.clone());
                button.dropdown_menu(move|m,w,cx|fill_menu(m.min_w(px(220.)),&menu,&editor,w,cx)).into_any_element()
            }
        }
        (_, Some(handler)) if enabled => button.on_click(listener(editor, handler)).into_any_element(),
        _ => button.into_any_element(),
    }
}

trait MouseDownStopExt: InteractiveElement {
    fn on_mouse_down_stop(self) -> Self
    where
        Self: Sized,
    {
        self.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
    }
}
impl<E: InteractiveElement> MouseDownStopExt for E {}

/// Flatten `MenuEntry` tree into leaf `MenuItem`s (#98, #103).
pub(crate) fn flatten_menu_entries(entries: &[MenuEntry]) -> Vec<MenuItem> {
    let mut out = Vec::new();
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => out.push(item.clone()),
            MenuEntry::Submenu(sub) => out.extend(flatten_menu_entries(&sub.items)),
            _ => {}
        }
    }
    out
}

/// Collect all executable leaf `MenuItem`s from `menus`, sorted in ascending order of their display label (#103).
pub(crate) fn collect_searchable_menu_items(menus: &Menus) -> Vec<MenuItem> {
    let mut items = Vec::new();
    for (_, entries) in &menus.menus {
        for item in flatten_menu_entries(entries) {
            if item.id == "help.search" || item.id.starts_with("font.section.") {
                continue;
            }
            if item.handler.is_some() && !item.disabled {
                let mut resolved = item;
                resolved.label = menu_item_display_label(&resolved);
                if !resolved.label.is_empty() {
                    items.push(resolved);
                }
            }
        }
    }
    items.sort_by(|a, b| a.label.as_ref().cmp(b.label.as_ref()));
    items.dedup_by(|a, b| a.label == b.label && a.id == b.id);
    items
}

/// Filter `items` by `query` (case-insensitive substring match on the display label) (#98, #103, #127).
pub(crate) fn filter_menu_items(items: &[MenuItem], query: &str) -> Vec<MenuItem> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return items.to_vec();
    }
    let q = trimmed.to_lowercase();
    items
        .iter()
        .filter(|item| {
            if item.id.starts_with("font.section.") {
                return false;
            }
            let label = menu_item_display_label(item);
            label.to_lowercase().contains(&q) || item.label.to_lowercase().contains(&q)
        })
        .cloned()
        .collect()
}

/// The width of a field in the right pane (a number with its ▲▼, a colour code, a dropdown of line styles…): the
/// boxes of a row line up when they all use it. [`text_field`]'s default width.
pub(crate) const FIELD_WIDTH: Pixels = px(124.);
/// Standard font selector width in dialogs and side panes (1.5 × the input width).
pub(crate) const FONT_DROPDOWN_WIDTH: Pixels = px(186.);

/// The one text field of the app (the right-pane panels and the popups): small, with a ✕ button to clear it, and
/// [`FIELD_WIDTH`] wide. Set what differs with the builder (#98, #127, #149, #150):
///
/// - `.no_clear()`: no ✕. Colour codes (6-digit hex): in the narrow colour fields the ✕ hid the end of the code
///   (2026-10-23 ユーザー).
/// - `.width(px)` / `.full_width()`: another width, or fill the parent (a name, a range, a search box…).
/// - `.spinner(id, spec, cx, on_step)`: ▲▼ buttons for a number. A click steps the value by `spec`, writes it into the
///   field and calls `on_step`, which applies it as typing it and pressing Enter does.
/// - `.disabled(bool)`.
///
/// It is an element: `.child(text_field(&input))`, or `.into_any_element()`.
pub(crate) fn text_field(input: &Entity<gpui_kit::component::input::InputState>) -> TextField {
    #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
    super::scenario::button_probe::record_input(input);
    TextField { input: input.clone(), clear: true, width: Some(FIELD_WIDTH), disabled: false, spinner: None, compact_spinner: false, unit: None, picker: None, trailing: None }
}

type StepHandler = Rc<dyn Fn(&mut SlideEditor, f32, &mut Window, &mut Context<SlideEditor>)>;

struct FieldSpinner {
    id: SharedString,
    spec: SpinnerSpec,
    editor: WeakEntity<SlideEditor>,
    on_step: StepHandler,
}

/// See [`text_field`].
#[derive(IntoElement)]
pub(crate) struct TextField {
    input: Entity<gpui_kit::component::input::InputState>,
    clear: bool,
    /// `None`: fill the parent.
    width: Option<Pixels>,
    disabled: bool,
    spinner: Option<FieldSpinner>,
    compact_spinner: bool,
    unit: Option<(SharedString, Pixels)>,
    picker: Option<FieldPathPicker>,
    trailing: Option<FieldTrailingIcon>,
}

struct FieldTrailingIcon {
    id: SharedString,
    icon: &'static str,
    tooltip: SharedString,
    editor: WeakEntity<SlideEditor>,
    clicked: Handler,
}

#[derive(Clone, Copy)]
pub(crate) enum PathMode { File, Folder }
struct FieldPathPicker {
    id: SharedString,
    mode: PathMode,
    extensions: Vec<String>,
    editor: WeakEntity<SlideEditor>,
    selected: Rc<dyn Fn(&mut SlideEditor, std::path::PathBuf, &mut Window, &mut Context<SlideEditor>)>,
}

impl TextField {
    /// An action icon inside the input's trailing edge (for example a sheet range picker).
    pub(crate) fn trailing_icon(mut self, id: impl Into<SharedString>, name: &'static str, tooltip: impl Into<SharedString>, cx: &Context<SlideEditor>, clicked: impl Fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>) + 'static) -> Self {
        self.trailing = Some(FieldTrailingIcon { id: id.into(), icon: name, tooltip: tooltip.into(), editor: cx.weak_entity(), clicked: Rc::new(clicked) });
        self
    }

    /// A path input with a standard Browse button; selection changes the draft only.
    pub(crate) fn path_picker(mut self, id: impl Into<SharedString>, mode: PathMode, extensions: &[&str], cx: &Context<SlideEditor>, selected: impl Fn(&mut SlideEditor, std::path::PathBuf, &mut Window, &mut Context<SlideEditor>) + 'static) -> Self {
        self.picker = Some(FieldPathPicker { id: id.into(), mode, extensions: extensions.iter().map(|s| s.to_string()).collect(), editor: cx.weak_entity(), selected: Rc::new(selected) }); self
    }
    pub(crate) fn unit(mut self, label: impl Into<SharedString>, width: Pixels) -> Self { self.unit=Some((label.into(),width)); self }
    /// Compact three-digit color channel input; other numeric fields retain the standard width.
    pub(crate) fn compact_spinner(mut self) -> Self { self.compact_spinner = true; self }

    /// No ✕ to clear the field.
    pub(crate) fn no_clear(mut self) -> Self {
        self.clear = false;
        self
    }

    /// The field's width (with its ▲▼ when it has them) instead of [`FIELD_WIDTH`].
    pub(crate) fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    /// The field fills its parent's width.
    pub(crate) fn full_width(mut self) -> Self {
        self.width = None;
        self
    }

    pub(crate) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// ▲▼ buttons that step the number in the field by `spec` and then call `on_step` with the new value.
    pub(crate) fn spinner(
        mut self,
        id: impl Into<SharedString>,
        spec: SpinnerSpec,
        cx: &Context<SlideEditor>,
        on_step: impl Fn(&mut SlideEditor, f32, &mut Window, &mut Context<SlideEditor>) + 'static,
    ) -> Self {
        self.spinner = Some(FieldSpinner { id: id.into(), spec, editor: cx.weak_entity(), on_step: Rc::new(on_step) });
        self
    }
}

impl RenderOnce for TextField {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let spinner_width = if self.compact_spinner { px(80.) } else { FIELD_WIDTH };
        let mut field = gpui_kit::component::input::Input::new(&self.input).small().cleanable(self.clear).disabled(self.disabled);
        #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
        let probe_id = self.trailing.as_ref().map(|t| format!("{}-field", t.id));
        if let Some(trailing) = self.trailing {
            let button = action_button(trailing.id).ghost().xsmall().w(px(24.)).h(px(24.)).px(px(0.))
                .disabled(self.disabled).tooltip(trailing.tooltip)
                .child(icon(trailing.icon, 18., rgb(ICON).into()))
                .on_click(move |_, window, cx| {
                    let clicked = trailing.clicked.clone();
                    let _ = trailing.editor.update(cx, |this, cx| clicked(this, window, cx));
                });
            field = field.suffix(button);
        }
        if let Some(picker) = self.picker {
            let input = self.input.clone();
            let browse = action_button(picker.id).label(crate::shell::i18n::text("field.browse"))
                .disabled(self.disabled || cfg!(target_family = "wasm"))
                .on_click(move |_, window, cx| {
                    let input = input.clone(); let extensions = picker.extensions.clone(); let selected = picker.selected.clone();
                    let _ = picker.editor.update(cx, |_, cx| {
                        let picked = cx.prompt_for_paths(PathPromptOptions { files: matches!(picker.mode, PathMode::File), directories: matches!(picker.mode, PathMode::Folder), extensions, multiple: false, prompt: None });
                        cx.spawn_in(window, async move |this, cx| {
                            let Ok(Ok(Some(paths))) = picked.await else { return };
                            let Some(path) = paths.into_iter().next() else { return };
                            let _ = this.update_in(cx, |this, window, cx| {
                                input.update(cx, |i, cx| i.set_value(path.to_string_lossy().into_owned(), window, cx));
                                selected(this, path, window, cx); cx.notify();
                            });
                        }).detach();
                    });
                });
            return h_flex().gap_2().items_center().w_full().child(div().flex_1().min_w_0().child(field)).child(browse).into_any_element();
        }
        let Some(spinner) = self.spinner else {
            let element = match self.width {
                Some(w) => field.w(w).into_any_element(),
                None => field.into_any_element(),
            };
            // Scenario measurement of the box's outer edge (compare with a dropdown's right edge).
            #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
            if let Some(key) = probe_id {
                let wrapper = div().relative().when_some(self.width, |d, w| d.w(w)).when(self.width.is_none(), |d| d.w_full());
                return wrapper.child(element).child(canvas(|_, _, _| (), move |b, _, _, _| super::scenario::button_probe::record_bounds(&key, b)).absolute().inset_0()).into_any_element();
            }
            return element;
        };
        let FieldSpinner { id, spec, editor, on_step } = spinner;
        let disabled = self.disabled;
        let step_btn = |up: bool| {
            let input = self.input.clone();
            let editor = editor.clone();
            let on_step = on_step.clone();
            let (suffix, icon_name) = if up { ("up", "keyboard_arrow_up") } else { ("down", "keyboard_arrow_down") };
            Button::new(SharedString::from(format!("{id}-{suffix}")))
                .ghost()
                .xsmall()
                .w(px(18.))
                .h(px(12.))
                .px(px(0.))
                .disabled(disabled)
                .child(icon(icon_name, 12.0, rgb(ICON).into()))
                .on_click(move |_, window, cx| {
                    let raw = input.read(cx).value().to_string();
                    let (current, unit) = split_number(&raw).unwrap_or((spec.empty_value(), ""));
                    let next = spec.step_value(current, up);
                    let formatted = format!("{}{unit}", spec.format_value(next));
                    input.update(cx, |s, cx| s.set_value(formatted, window, cx));
                    let on_step = on_step.clone();
                    editor.update(cx, |this, cx| on_step(this, next, window, cx)).ok();
                })
        };
        let (up_btn, down_btn) = (step_btn(true), step_btn(false));
        let numeric = h_flex()
            .gap_1()
            .items_center()
            .w(spinner_width).min_w(spinner_width).max_w(spinner_width).flex_none()
            .child(div().flex_1().min_w(px(48.)).child(field))
            .child(v_flex().items_center().justify_center().child(up_btn).child(down_btn))
            .into_any_element();
        if let Some((label,width))=self.unit {
            h_flex().items_center().gap_2().flex_none().child(numeric)
                .child(div().w(width).min_w(width).flex_none().text_sm().child(label)).into_any_element()
        } else { numeric }
    }
}

/// Shared right-pane row with left-aligned label, right-aligned `text_field`, and fixed-width unit label
/// so input left/right edges align vertically across rows (#150, #151).
pub(crate) fn labeled_input_unit(
    label: impl Into<SharedString>,
    input: &Entity<gpui_kit::component::input::InputState>,
    unit: impl Into<SharedString>,
) -> Div {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(8.))
        .child(div().text_size(px(12.)).text_color(rgb(TEXT)).child(label.into()))
        .child(
            h_flex()
                .flex_none()
                .items_center()
                .gap(px(4.))
                .child(text_field(input).width(px(96.)))
                .child(div().w(px(36.)).text_size(px(12.)).text_color(rgb(TEXT_SECONDARY)).child(unit.into())),
        )
}

/// Content attribute for the single dropdown button: a standard text label or custom preview.
pub(crate) enum DropdownButtonContent {
    Label(SharedString),
    Custom(AnyElement),
}
impl From<SharedString> for DropdownButtonContent {
    fn from(value: SharedString) -> Self { Self::Label(value) }
}
impl From<String> for DropdownButtonContent {
    fn from(value: String) -> Self { Self::Label(value.into()) }
}
impl From<&str> for DropdownButtonContent {
    fn from(value: &str) -> Self { Self::Label(value.to_owned().into()) }
}
impl From<AnyElement> for DropdownButtonContent {
    fn from(value: AnyElement) -> Self { Self::Custom(value) }
}
/// The closed button shared by text, font and owner-drawn preview dropdowns.
pub(crate) fn dropdown_button(id: impl Into<ElementId>, content: impl Into<DropdownButtonContent>) -> Button {
    let content = match content.into() {
        DropdownButtonContent::Label(label) => div().truncate().text_sm().child(label).into_any_element(),
        DropdownButtonContent::Custom(preview) => preview,
    };
    action_button(id).small().h(px(BUTTON_HEIGHT)).w_full().bg(rgb(0xffffff))
        .child(h_flex().w_full().min_w_0().items_center().justify_between().gap_2()
            .child(div().flex_1().min_w_0().overflow_hidden().child(content))
            .child(icon("arrow_drop_down",18.,rgb(ICON).into())))
}

pub(crate) fn font_dropdown(
    id: &'static str, current: &str, entries: Vec<MenuEntry>, index: usize,
    editor: &WeakEntity<SlideEditor>,
) -> AnyElement {
    FontDropdown::new(id,current,entries,index,editor).into_any_element()
}

#[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
thread_local! {pub(crate) static DROPDOWN_LAYOUT: std::cell::RefCell<std::collections::HashMap<&'static str,(f32,bool)>> = Default::default();}
thread_local! {
    static DROPDOWN_BUTTON_WIDTHS: std::cell::RefCell<std::collections::HashMap<&'static str,f32>> = Default::default();
}
/// Render a dropdown button whose popup menu has a `✕`-clearable search TextField at the top,
/// keeps a fixed width and height (`x, y, w, h` unchanged when filtering), shows `"みつかりません"`
/// when 0 items match, and supports keyboard navigation (`↑`, `↓`, `PageUp`, `PageDown`, `Enter`)
/// plus an always-visible scrollbar when scrollable (#98, #103, #125, #126, #127).

thread_local! {static DROPDOWN_INSTANCE:std::cell::Cell<Option<(&'static str,usize)>>=const{std::cell::Cell::new(None)};}
#[derive(IntoElement)]
struct HoverDropdownTrigger{button:Button,group:&'static str,selected:bool}
impl gpui_kit::component::Selectable for HoverDropdownTrigger{fn selected(mut self,value:bool)->Self{self.selected=value;self.button=self.button.selected(value);self}fn is_selected(&self)->bool{self.selected}}
impl RenderOnce for HoverDropdownTrigger{fn render(self,_:&mut Window,_:&mut App)->impl IntoElement{div().opacity(if self.selected{1.}else{0.}).group_hover(self.group,|d|d.opacity(1.)).child(self.button)}}
fn render_dropdown(
    id: &'static str,
    button: Button,
    entries: Vec<MenuEntry>,
    single_select: bool,
    index: usize,
    editor: &WeakEntity<SlideEditor>,
    options: dropdown::Options,
) -> AnyElement {
    let static_items = Rc::new(flatten_menu_entries(&entries));
    let editor_open = editor.clone();
    let items_open = static_items.clone();
    let editor_content = editor.clone();
    let is_open = CURRENT_TOOLBAR_OPEN.with(|c| c.get().1 == Some(id)) && DROPDOWN_INSTANCE.with(|c|c.get().is_none_or(|(open_id,open_index)|open_id!=id || open_index==index));
    // Toolbar buttons (like `menu-search`) open downward (`Anchor::TopLeft` places the popup's top-left at the button's bottom-left);
    // bottom-bar buttons (like `sheet-list`) open upward (`Anchor::BottomLeft` places the popup's bottom-left at the button's top-left).
    let anchor = if id == "sheet-list" { Anchor::BottomLeft } else { Anchor::TopLeft };

    let popup = Popover::new(SharedString::from(format!("searchable-menu-{id}-{index}")))
        .appearance(false).p_0()
        .anchor(anchor)
        .open(is_open);
    let popup=if let Some(group)=options.hover_reveal{popup.trigger(HoverDropdownTrigger{button,group,selected:is_open})}else{popup.trigger(button)};
    let popup=popup.on_open_change(move |open, window, cx| {
            if *open {DROPDOWN_INSTANCE.with(|c|c.set(Some((id,index))));}
            else {DROPDOWN_INSTANCE.with(|c|{if c.get()==Some((id,index)){c.set(None);}});}
            editor_open
                .update(cx, |this, cx| {
                    let input = this.ensure_toolbar_search_input(id, window, cx);
                    let scroll = this.ensure_toolbar_search_scroll(id);
                    if *open {
                        this.font_size_menu_open = false;
                        this.font_size_filtering = false;
                        this.font_size_selected = None;
                        this.toolbar_search_open = Some(id);
                        let items = if items_open.is_empty() { this.searchable_menu_items_for(id) } else { (*items_open).clone() };
                        let initial=items.iter().position(|item|item.checked && item.enabled())
                            .or_else(||items.iter().position(|item|item.enabled()));
                        this.toolbar_search_popup_items = Some((id, items));
                        this.toolbar_search_selected.insert(id, initial);
                        scroll.set_offset(Point::default());
                        input.update(cx, |state, cx| {
                            state.set_value("", window, cx);
                            state.focus(window, cx);
                        });
                    } else if this.toolbar_search_open == Some(id) {
                        this.toolbar_search_open = None;
                        this.toolbar_search_popup_items = None;
                        this.toolbar_search_selected.insert(id, None);
                    }
                    set_current_toolbar_open(this.font_size_menu_open, this.toolbar_search_open);
                    cx.notify();
                })
                .ok();
        })
        .content(move |_, window, cx| {
            let Some(ed) = editor_content.upgrade() else {
                return div().into_any_element();
            };
            let (input, scroll_handle, selected_idx) = ed.update(cx, |this, cx| {
                let input = this.ensure_toolbar_search_input(id, window, cx);
                let scroll = this.ensure_toolbar_search_scroll(id);
                let sel = this.toolbar_search_selected.get(id).copied().flatten();
                (input, scroll, sel)
            });
            let all_items: Vec<MenuItem> = if static_items.is_empty() {
                ed.read(cx).searchable_menu_items_for(id)
            } else {
                (*static_items).clone()
            };
            let button_width = DROPDOWN_BUTTON_WIDTHS.with(|widths| widths.borrow().get(id).copied()).unwrap_or(240.);
            let fixed_width = match options.width {
                dropdown::ListWidth::Button=>button_width.min(dropdown_list::MAX_WIDTH),
                dropdown::ListWidth::Fixed(w)=>f32::from(w).min(dropdown_list::MAX_WIDTH),
                dropdown::ListWidth::Content=>dropdown_list::content_width(&all_items,|item|menu_item_display_label(item).to_string(),|item| {
                    let label=menu_item_display_label(item);
                    let text_width=match &item.dropdown_content {
                        Some(dropdown::ItemContent::Font(family))=>crate::shell::font_preview::get_or_render_preview(family,&label).map(|p|p.width).unwrap_or_else(||dropdown_list::label_width(window,&label,13.)),
                        _=>dropdown_list::label_width(window,&label,13.),
                    };
                    text_width+68.+item.shortcut.map_or(0.,|sc|dropdown_list::label_width(window,&format_shortcut_for_platform(sc),12.)+20.)
                },button_width),
            };
            let row_h = dropdown_list::ROW_HEIGHT;
            let avail_list_h = (f32::from(window.viewport_size().height) - 214.0).max(224.0);
            let max_rows = ((avail_list_h / row_h).floor() as usize).max(8);
            let visible_rows = all_items.len().clamp(1, max_rows);
            let fixed_list_height = (visible_rows as f32) * row_h;
            let show_search = options.search && all_items.len() > 20;
            #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
            DROPDOWN_LAYOUT.with(|m|{m.borrow_mut().insert(id,(fixed_width,show_search));});
            let query = if show_search {input.read(cx).value().to_string()} else {String::new()};
            let has_query = !query.trim().is_empty();
            let matched = filter_menu_items(&all_items, &query);
            let reserve_left=options.checkmarks || all_items.iter().any(|i|i.icon.is_some()||i.swatch.is_some());
            let matched_len = matched.len();
            let first_enabled_idx = if has_query {
                matched.iter().position(|it| it.enabled())
            } else {
                matched.iter().position(|it| it.checked && it.enabled())
            };
            let active_highlight_idx = selected_idx.map(|i| i.min(matched_len.saturating_sub(1))).or(first_enabled_idx);
            let editor_for_rows = editor_content.clone();
            let input_for_rows = input.clone();

            let menu_focus=input.read(cx).focus_handle(cx);
            dropdown_list::surface().relative().left(options.offset_x).border_1().border_color(rgb(BORDER)).rounded(px(6.)).shadow_md()
                .when(!show_search, |d|d.track_focus(&menu_focus))
                .w(px(fixed_width))
                .on_mouse_down_stop()
                .when(show_search, |d| d.child(text_field(&input).full_width()))
                .child(
                    div()
                        .relative()
                        .w_full()
                        .h(px(fixed_list_height))
                        .child(
                            dropdown_list::list(SharedString::from(format!("searchable-menu-list-{id}")), &scroll_handle)
                                .children(if matched.is_empty() {
                                    vec![
                                        div()
                                            .w_full()
                                            .h(px(28.))
                                            .px(px(8.))
                                            .flex()
                                            .items_center()
                                            .text_size(px(13.))
                                            .text_color(rgb(TEXT_SECONDARY))
                                            .child(crate::shell::i18n::text("fix.menu.not-found"))
                                            .into_any_element(),
                                    ]
                                } else {
                                    matched
                                        .into_iter()
                                        .enumerate()
                                        .map(|(row_idx, item)| {
                                            if item.id.starts_with("font.section.") {
                                                return div()
                                                    .id((SharedString::from(format!("searchable-item-{id}")), row_idx))
                                                    .w_full()
                                                    .h(px(28.))
                                                    .px(px(6.))
                                                    .flex()
                                                    .flex_none()
                                                    .items_center()
                                                    .text_size(px(12.))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(TEXT_SECONDARY))
                                                    .child(item.label.clone())
                                                    .on_mouse_down_stop()
                                                    .into_any_element();
                                            }
                                            let is_highlighted = active_highlight_idx == Some(row_idx);
                                            let enabled = item.enabled();
                                            let fg: Hsla = if enabled { rgb(TEXT).into() } else { disabled_color() };
                                            let label = menu_item_display_label(&item);
                                            let handler = item.handler.clone();
                                            let ed_click = editor_for_rows.clone();
                                            let input_click = input_for_rows.clone();

                                            let check_fg: Hsla = if let Some(c) = item.swatch {
                                                let r = ((c.0 >> 16) & 0xff) as f32 / 255.0;
                                                let g = ((c.0 >> 8) & 0xff) as f32 / 255.0;
                                                let b = (c.0 & 0xff) as f32 / 255.0;
                                                let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                                                if luma < 0.45 { rgb(0xffffff).into() } else { rgb(0x1f1f1f).into() }
                                            } else {
                                                fg
                                            };
                                            let left_slot = div()
                                                .w(px(20.))
                                                .h(px(row_h))
                                                .flex_none()
                                                .rounded(px(0.))
                                                .when_some(item.swatch, |d, c| d.bg(rgb(c.0)))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .children(dropdown_list::checkmark(item.checked,options.checkmarks,check_fg))
                                                .when(!options.checkmarks || !item.checked, |d| {
                                                    if let Some(ic) = item.icon.filter(|i| !i.is_empty()) {
                                                        d.child(icon(ic, 16.0, check_fg))
                                                    } else {
                                                        d
                                                    }
                                                });

                                            let selected_row_bg = (single_select || options.checkmarks) && item.checked;
                                            let is_preview=matches!(&item.dropdown_content,Some(dropdown::ItemContent::Palette(_)|dropdown::ItemContent::Custom(_)));
                                            let label_el=match &item.dropdown_content {
                                                Some(dropdown::ItemContent::Font(family))=>dropdown::font_item(family,label.clone(),fg),
                                                Some(dropdown::ItemContent::Palette(colors))=>dropdown::color_palette_item(colors),
                                                Some(dropdown::ItemContent::Custom(render))=>render(),
                                                None=>div().flex_1().truncate().text_size(px(13.)).text_color(fg).child(label.clone()).into_any_element(),
                                            };

                                            let row_id=ElementId::from((SharedString::from(format!("searchable-item-{id}")),row_idx));
                                            let activate=move |_: &ClickEvent,window:&mut Window,cx:&mut App| {
                                                if let Some(h)=handler.as_ref().filter(|_|enabled) {
                                                    input_click.update(cx,|s,cx|s.set_value("",window,cx));
                                                    ed_click.update(cx,|this,cx| {
                                                        this.toolbar_search_open=None;this.toolbar_search_popup_items=None;
                                                        this.toolbar_search_selected.insert(id,None);set_current_toolbar_open(this.font_size_menu_open,None);cx.notify();
                                                    }).ok();
                                                    let owner=ed_click.clone();let h=h.clone();window.defer(cx,move|w,cx|{owner.update(cx,|e,cx|h(e,w,cx)).ok();});
                                                }
                                            };
                                            let has_left=reserve_left;
                                            if is_preview {
                                                let tooltip=item.label.clone();
                                                let row=dropdown_list::option(row_id.clone(),selected_row_bg,is_highlighted,enabled)
                                                    .tooltip(move|w,cx|gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(w,cx))
                                                    .child(h_flex().w_full().min_w_0().items_center().gap(px(6.)).when(has_left,|d|d.child(left_slot)).child(label_el))
                                                    .on_mouse_down_stop().on_click(activate);
                                                #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
                                                let row=super::scenario::button_probe::observe_control(row,row_id);
                                                row.into_any_element()
                                            } else {
                                                let row=dropdown_list::option(row_id.clone(),selected_row_bg,is_highlighted,enabled)
                                                    .when(has_left,|d|d.child(left_slot)).child(label_el)
                                                    .when_some(item.shortcut,|d,sc|d.child(div().text_size(px(12.)).text_color(rgb(TEXT_SECONDARY)).child(format_shortcut_for_platform(sc))))
                                                    .on_mouse_down_stop().on_click(activate);
                                                #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
                                                let row=super::scenario::button_probe::observe_control(row,row_id);
                                                row.into_any_element()
                                            }
                                        })
                                        .collect()
                                }),
                        )
                        .child(always_visible_scrollbar_overlay(
                            SharedString::from(format!("searchable-menu-scrollbar-{id}")),
                            scroll_handle,
                            Some((fixed_list_height, (matched_len as f32) * row_h)),
                        )),
                )
                .into_any_element()
        })
        .into_any_element();
    div().relative().child(popup).child(canvas(|_,_,_|(),move |bounds,_,_,_| {
        DROPDOWN_BUTTON_WIDTHS.with(|widths| {widths.borrow_mut().insert(id,f32::from(bounds.size.width));});
    }).absolute().inset_0()).into_any_element()
}

/// The shared radio indicator, matching the Go To Special panel.
pub(crate) fn radio_dot(selected: bool) -> Div {
    div().size(px(14.)).rounded(px(7.)).border_1()
        .border_color(if selected { rgb(PRIMARY_BUTTON_BG) } else { rgb(0x888888) })
        .flex().flex_none().items_center().justify_center()
        .when(selected, |d| d.child(div().size(px(8.)).rounded(px(4.)).bg(rgb(PRIMARY_BUTTON_BG))))
}

#[derive(IntoElement)]
struct FormControlMarker(bool);
impl RenderOnce for FormControlMarker {fn render(self,_:&mut Window,_:&mut App)->impl IntoElement {gpui_kit::component::form_controls::other(self.0,false);div().absolute().size_0()}}

/// A radio indicator and its label are one indivisible item, including in a flow group.
/// The group owns placement; the button owns appearance and the caller owns selection.
pub(crate) fn radio_button(
    id: impl Into<ElementId>,
    selected: bool,
    disabled: bool,
    label: impl Into<SharedString>,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let label = label.into();
    let id: ElementId = id.into();
    let control = h_flex().id(id.clone()).role(gpui_kit::Role::RadioButton).aria_label(label.clone())
        .aria_toggled(if selected { gpui_kit::Toggled::True } else { gpui_kit::Toggled::False }).flex_none().items_center().gap_2()
        .py(px(3.)).px_1().rounded(px(3.)).whitespace_nowrap()
        .when(disabled, |d| d.opacity(0.45))
        .when(!disabled, |d| d.cursor_pointer().hover(|s| s.bg(rgb(0xedf2f7))).on_click(on_click))
        .child(FormControlMarker(disabled))
        .child(radio_dot(selected))
        .child(div().text_sm().child(label));
    // The scenario tests press it by its rendered bounds.
    #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
    let control = super::scenario::button_probe::observe_control(control, id);
    control
}

/// Layout of a shared radio group. Flow wraps between buttons, never inside a label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RadioGroupLayout {
    Flow,
    #[default]
    Column,
}

/// Radio group container. Use `.layout(RadioGroupLayout::Flow)` for wrapping rows;
/// the default is one button per row. Children may include panel bounds markers.
#[derive(IntoElement)]
pub(crate) struct RadioGroup {
    base: Div,
    layout: RadioGroupLayout,
}

pub(crate) fn radio_group() -> RadioGroup {
    RadioGroup { base: div(), layout: RadioGroupLayout::Column }
}

impl RadioGroup {
    pub(crate) fn layout(mut self, layout: RadioGroupLayout) -> Self {
        self.layout = layout;
        self
    }
}

impl ParentElement for RadioGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.base.extend(elements);
    }
}

impl RenderOnce for RadioGroup {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        self.base.flex().gap_0p5().map(|d| match self.layout {
            RadioGroupLayout::Flow => d.flex_row().flex_wrap().items_center(),
            RadioGroupLayout::Column => d.flex_col().items_stretch(),
        })
    }
}

/// App-wide checkbox: the primary-blue graphical indicator and a left-aligned label.
/// Keeps the familiar builder used by the panels; the change callback receives the next value.
#[derive(IntoElement)]
pub(crate) struct Checkbox {
    base: Stateful<Div>,
    id: ElementId,
    indeterminate: bool,
    label: SharedString,
    accessibility_label: Option<SharedString>,
    checked: bool,
    disabled: bool,
    on_change: Option<Rc<dyn Fn(&bool, &mut Window, &mut App)>>,
}

impl Checkbox {
    pub(crate) fn new(id: impl Into<ElementId>) -> Self {
        let id = id.into();
        Self { base: div().id(id.clone()), id, indeterminate: false, label: "".into(), accessibility_label:None, checked: false, disabled: false, on_change: None }
    }
    pub(crate) fn label(mut self, label: impl Into<SharedString>) -> Self { self.label = label.into(); self }
    pub(crate) fn accessibility_label(mut self,label:impl Into<SharedString>)->Self {self.accessibility_label=Some(label.into());self}
    pub(crate) fn indeterminate(mut self, mixed: bool) -> Self { self.indeterminate = mixed; self }
    pub(crate) fn checked(mut self, checked: bool) -> Self { self.checked = checked; self }
    pub(crate) fn disabled(mut self, disabled: bool) -> Self { self.disabled = disabled; self }
    pub(crate) fn on_click(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler)); self
    }
    pub(crate) fn small(self) -> Self { self }
}

impl Styled for Checkbox {
    fn style(&mut self) -> &mut StyleRefinement { self.base.style() }
}

impl RenderOnce for Checkbox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let checked = self.checked;
        let focus = window.use_keyed_state(self.id, cx, |_, cx| cx.focus_handle().tab_stop(true)).read(cx).clone().tab_stop(!self.disabled);
        let focus_click = focus.clone();
        let keyboard_handler = self.on_change.clone();
        let disabled = self.disabled;
        let indicator = if self.indeterminate {
            checkbox_box(false).child(div().w(px(8.)).h(px(2.)).bg(rgb(PRIMARY_BUTTON_BG)))
        } else { checkbox_box(checked) };
        self.base.role(gpui_kit::Role::CheckBox).aria_label(self.accessibility_label.unwrap_or_else(||self.label.clone()))
            .aria_toggled(if self.indeterminate { gpui_kit::Toggled::Mixed } else if checked { gpui_kit::Toggled::True } else { gpui_kit::Toggled::False })
            .track_focus(&focus).flex().items_center().justify_start().gap_2().py(px(3.)).px_1()
            .when(self.disabled, |d| d.opacity(0.45))
            .when(!self.disabled, |d| d.on_mouse_down(MouseButton::Left, move |_, window, cx| focus_click.focus(window, cx))
                .on_key_down(move |e: &KeyDownEvent, window, cx| {
                    if !disabled && matches!(e.keystroke.key.as_str(), "space" | "enter") {
                        if let Some(handler) = &keyboard_handler { handler(&!checked, window, cx); }
                        cx.stop_propagation();
                    }
                }).cursor_pointer().hover(|s| s.bg(rgb(0xedf2f7))))
            .when_some(self.on_change.filter(|_| !self.disabled), |d, handler| {
                d.on_click(move |_, window, cx| handler(&!checked, window, cx))
            })
            .child(indicator)
            .child(div().min_w_0().text_sm().child(self.label))
    }
}

/// Shared checkbox indicator square with primary button blue (`PRIMARY_BUTTON_BG`) border and fill when checked (#106, #150).
#[allow(dead_code)]
pub(crate) fn checkbox_box(checked: bool) -> Div {
    div()
        .w(px(14.))
        .h(px(14.))
        .rounded(px(3.))
        .border_1()
        .border_color(rgb(PRIMARY_BUTTON_BG))
        .bg(if checked { rgb(PRIMARY_BUTTON_BG) } else { rgb(0xffffff) })
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .when(checked, |d| d.child(icon("check", 12.0, rgb(0xffffff).into())))
}

/// Shared left-aligned borderless checkbox row (#106).
#[allow(dead_code)]
pub(crate) fn checkbox_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> Stateful<Div> {
    let label = label.into();
    let id=id.into();let probe_id=id.clone();
    let control=h_flex()
        .id(id).role(gpui_kit::Role::CheckBox).aria_label(label.clone())
        .aria_toggled(if checked { gpui_kit::Toggled::True } else { gpui_kit::Toggled::False })
        .gap(px(8.))
        .py(px(3.))
        .px(px(4.))
        .rounded(px(4.))
        .items_center()
        .when(!disabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(crate::shell::palette::shade(0.0, 0.05)))
                .on_click(on_click)
        })
        .when(disabled, |d| d.opacity(0.5))
        .child(checkbox_box(checked))
        .child(div().text_sm().text_color(rgb(TEXT)).child(label));
    #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
    let control=super::scenario::button_probe::observe_control(control,probe_id);
    control
}

/// The colour of the chrome's text, for panels.
pub(crate) fn text_secondary() -> Hsla {
    rgb(TEXT_SECONDARY).into()
}

/// A pressed-state background, for panels.
#[allow(dead_code)]
pub(crate) fn pressed() -> Hsla {
    rgb(0xd3e3fd).into()
}

/// Shared 4-button horizontal alignment group for toolbars (`[Left | Center | Right | ▼]`).
pub(crate) fn align_h_toolbar_items(
    current: Option<ibis_doc_model::TextAlign>,
    enabled: bool,
    on_left: Handler,
    on_center: Handler,
    on_right: Handler,
    menu: Vec<MenuEntry>,
) -> [ToolItem; 4] {
    align_h_toolbar_items_with_id("align", current, enabled, on_left, on_center, on_right, menu)
}

/// Shared 4-button horizontal alignment group for toolbars (`[Left | Center | Right | ▼]`) with custom dropdown id.
pub(crate) fn align_h_toolbar_items_with_id(
    dropdown_id: &'static str,
    current: Option<ibis_doc_model::TextAlign>,
    enabled: bool,
    on_left: Handler,
    on_center: Handler,
    on_right: Handler,
    menu: Vec<MenuEntry>,
) -> [ToolItem; 4] {
    let mut left = ToolButton::new("align-left", "format_align_left", "_shell.align-left")
        .segment(SegmentRounding::Left)
        .on(current == Some(ibis_doc_model::TextAlign::Left))
        .disabled(!enabled);
    left.handler = enabled.then_some(on_left);
    let mut center = ToolButton::new("align-center", "format_align_center", "_shell.align-center")
        .segment(SegmentRounding::Middle)
        .on(current == Some(ibis_doc_model::TextAlign::Center))
        .disabled(!enabled);
    center.handler = enabled.then_some(on_center);
    let mut right = ToolButton::new("align-right", "format_align_right", "_shell.align-right")
        .segment(SegmentRounding::Middle)
        .on(current == Some(ibis_doc_model::TextAlign::Right))
        .disabled(!enabled);
    right.handler = enabled.then_some(on_right);
    [
        left.into(),
        center.into(),
        right.into(),
        ToolButton::dropdown(dropdown_id, "_shell.horizontal-alignment")
            .segment(SegmentRounding::Right)
            .disabled(!enabled)
            .menu(menu)
            .into(),
    ]
}

/// Shared 4-button vertical alignment group for toolbars (`[Top | Middle | Bottom | ▼]`).
pub(crate) fn align_v_toolbar_items(
    current: Option<ibis_doc_model::VerticalAnchor>,
    enabled: bool,
    on_top: Handler,
    on_middle: Handler,
    on_bottom: Handler,
    menu: Vec<MenuEntry>,
) -> [ToolItem; 4] {
    align_v_toolbar_items_with_ids(["valign-top", "valign-center", "valign-bottom", "vertical-align"], current, enabled, on_top, on_middle, on_bottom, menu)
}

/// Shared 4-button vertical alignment group for toolbars (`[Top | Middle | Bottom | ▼]`) with custom dropdown id.
pub(crate) fn align_v_toolbar_items_with_id(
    dropdown_id: &'static str,
    current: Option<ibis_doc_model::VerticalAnchor>,
    enabled: bool,
    on_top: Handler,
    on_middle: Handler,
    on_bottom: Handler,
    menu: Vec<MenuEntry>,
) -> [ToolItem; 4] {
    align_v_toolbar_items_with_ids(["valign-top", "valign-center", "valign-bottom", dropdown_id], current, enabled, on_top, on_middle, on_bottom, menu)
}

fn align_v_toolbar_items_with_ids(
    ids: [&'static str; 4],
    current: Option<ibis_doc_model::VerticalAnchor>,
    enabled: bool,
    on_top: Handler,
    on_middle: Handler,
    on_bottom: Handler,
    menu: Vec<MenuEntry>,
) -> [ToolItem; 4] {
    let mut top = ToolButton::new(ids[0], "vertical_align_top", "docx.cell.valign-top")
        .segment(SegmentRounding::Left)
        .on(current == Some(ibis_doc_model::VerticalAnchor::Top))
        .disabled(!enabled);
    top.handler = enabled.then_some(on_top);
    let mut middle = ToolButton::new(ids[1], "vertical_align_center", "docx.cell.valign-center")
        .segment(SegmentRounding::Middle)
        .on(current == Some(ibis_doc_model::VerticalAnchor::Middle))
        .disabled(!enabled);
    middle.handler = enabled.then_some(on_middle);
    let mut bottom = ToolButton::new(ids[2], "vertical_align_bottom", "docx.cell.valign-bottom")
        .segment(SegmentRounding::Middle)
        .on(current == Some(ibis_doc_model::VerticalAnchor::Bottom))
        .disabled(!enabled);
    bottom.handler = enabled.then_some(on_bottom);
    [
        top.into(),
        middle.into(),
        bottom.into(),
        ToolButton::dropdown(ids[3], "_shell.vertical-alignment")
            .segment(SegmentRounding::Right)
            .disabled(!enabled)
            .menu(menu)
            .into(),
    ]
}

/// Renders the shared 4-button vertical alignment group (`[Top | Middle | Bottom | ▼]`) inside a panel.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_align_v_group(
    current: Option<ibis_doc_model::VerticalAnchor>,
    enabled: bool,
    on_top: Handler,
    on_middle: Handler,
    on_bottom: Handler,
    menu: Vec<MenuEntry>,
    base_index: usize,
    editor: &WeakEntity<SlideEditor>,
) -> AnyElement {
    let items = align_v_toolbar_items_with_ids(
        ["panel-valign-top", "panel-valign-center", "panel-valign-bottom", "panel-vertical-align"],
        current,
        enabled,
        on_top,
        on_middle,
        on_bottom,
        menu,
    );
    let mut row = h_flex().items_center();
    for (offset, item) in items.iter().enumerate() {
        row = row.child(tool_item(item, base_index + offset, editor));
    }
    row.into_any_element()
}

pub(crate) fn panel_font_dropdown(id: &'static str, input: &Entity<gpui_kit::component::input::InputState>, cx: &Context<SlideEditor>) -> AnyElement {
    #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
    super::scenario::button_probe::record_input(input);
    let current=input.read(cx).value().to_string(); let field=input.clone();
    let entries=crate::shell::font_list::build_font_menu_entries(id,Vec::new(),Some(&current),move |name,_,w,cx| {
        field.update(cx,|i,cx|i.set_value(name.to_string(),w,cx));cx.notify();
    });
    div().w(FONT_DROPDOWN_WIDTH).flex_none().child(font_dropdown(id,&current,entries,0,&cx.weak_entity())).into_any_element()
}
pub(crate) fn panel_color_field(id: &'static str, input: &Entity<gpui_kit::component::input::InputState>, cx: &Context<SlideEditor>) -> AnyElement {
    #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
    super::scenario::button_probe::record_input(input);
    let current=u32::from_str_radix(input.read(cx).value().trim_start_matches('#'),16).ok().map(ibis_doc_model::Rgb);
    let field=input.clone();
    let pick: crate::shell::color_palette::PickColor=Rc::new(move |e,color,w,cx| {
        if let Some(c)=color {field.update(cx,|i,cx|{i.set_value(format!("{:06X}",c.0),w,cx);cx.emit(gpui_kit::component::input::InputEvent::Change);});e.note_recent_color(c);}cx.notify();
    });
    let colors:Vec<_>=crate::shell::color_editor::palette_colors().into_iter().map(|c|(SharedString::from(format!("#{:06X}",c.0)),c,"color.swatch")).collect();
    let entries=crate::shell::color_palette::simple_palette(None,None,&colors,current,pick);
    div().w(FIELD_WIDTH).flex_none().child(color_text_field(id,input,current,entries,&cx.weak_entity())).into_any_element()
}
pub(crate) fn panel_color_field_enabled(id:&'static str,input:&Entity<gpui_kit::component::input::InputState>,enabled:bool,cx:&Context<SlideEditor>)->AnyElement {
 if enabled{return panel_color_field(id,input,cx);}
 let current=u32::from_str_radix(input.read(cx).value().trim_start_matches('#'),16).unwrap_or(0);
 h_flex().gap_1().w(FIELD_WIDTH).items_center().child(action_button(id).disabled(true).w(px(32.)).px_1().child(div().size(px(14.)).bg(rgb(current))))
 .child(div().flex_1().min_w_0().child(text_field(input).no_clear().disabled(true).full_width())).into_any_element()
}
pub(crate) fn color_name_label(color:u32,label:impl Into<SharedString>) -> Div {
    h_flex().items_center().gap_2().child(div().size(px(14.)).rounded(px(2.)).border_1().border_color(rgb(BORDER)).bg(rgb(color)))
        .child(div().text_sm().child(label.into()))
}
pub(crate) fn color_radio_button(id:impl Into<ElementId>, selected:bool,color:u32,label:impl Into<SharedString>, on_click:impl Fn(&ClickEvent,&mut Window,&mut App)+'static)->Stateful<Div> {
    let label=label.into();
    h_flex().id(id).role(gpui_kit::Role::RadioButton).aria_label(label.clone()).aria_toggled(if selected {Toggled::True} else {Toggled::False})
        .gap_2().items_center().px_1().py(px(3.)).cursor_pointer().child(radio_dot(selected)).child(color_name_label(color,label)).on_click(on_click)
}
pub(crate) fn take_sidebar_footer()->Option<ActionBar>{SIDEBAR_FOOTER.with(|c|c.borrow_mut().take())}
pub(crate) fn input_dropdown(id: &'static str, input: &Entity<gpui_kit::component::input::InputState>, choices:Vec<String>, cx:&Context<SlideEditor>) -> AnyElement {
    #[cfg(all(feature="scenarios",any(target_os="macos",target_os="linux")))]
    super::scenario::button_probe::record_input(input);
    let current=input.read(cx).value().to_string();
    let entries=choices.into_iter().map(|name| {let field=input.clone();let value=name.clone(); MenuItem::new("docx.style.doc.choice",name).on(move |_,w,cx|{field.update(cx,|i,cx|i.set_value(value.clone(),w,cx));cx.notify();}).into()}).collect();
    div().w(FIELD_WIDTH).flex_none().child(Dropdown::with_button(id,dropdown_button(id,current),entries,false,0,&cx.weak_entity()).into_any_element()).into_any_element()
}

/// A color text input paired with a color swatch button that opens a color palette popup.
pub(crate) fn color_text_field(id:&'static str,input:&Entity<gpui_kit::component::input::InputState>,current_rgb:Option<ibis_doc_model::Rgb>,palette_entries:Vec<MenuEntry>,editor:&WeakEntity<SlideEditor>)->ColorTextField {
    ColorTextField{id,input:input.clone(),current_rgb,palette_entries,editor:editor.clone(),allow_none:None}
}
#[derive(IntoElement)]
pub(crate) struct ColorTextField {id:&'static str,input:Entity<gpui_kit::component::input::InputState>,current_rgb:Option<ibis_doc_model::Rgb>,palette_entries:Vec<MenuEntry>,editor:WeakEntity<SlideEditor>,allow_none:Option<bool>}
impl ColorTextField {pub(crate) fn allow_none(mut self,value:bool)->Self{self.allow_none=Some(value);self}}
impl RenderOnce for ColorTextField {
 fn render(self,_:&mut Window,cx:&mut App)->impl IntoElement {
    let id=self.id;let input=&self.input;let current_rgb=self.current_rgb;let editor=&self.editor;
    let swatch = match current_rgb {
        Some(c) => div()
            .w(px(14.))
            .h(px(14.))
            .rounded(px(2.))
            .bg(rgb(c.0))
            .border_1()
            .border_color(rgb(BORDER)),
        None => div()
            .w(px(14.))
            .h(px(14.))
            .rounded(px(2.))
            .bg(rgb(0xffffff))
            .border_1()
            .border_color(rgb(BORDER))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(10.))
            .text_color(rgb(0xd93025))
            .child("×"),
    };
    let mut palette_entries=self.palette_entries;
    for entry in &mut palette_entries {
        if let MenuEntry::Palette(p)=entry {
            p.current=current_rgb;
            if let Some(pick)=&p.pick {p.theme=crate::shell::color_palette::theme_rows(&editor.upgrade().map(|e|e.read(cx).current_theme_palette_colors()).unwrap_or([ibis_doc_model::Rgb(0);10]),pick,current_rgb);}
            if self.allow_none==Some(false) {p.none=None;}
            else if self.allow_none == Some(true) && p.none.is_none() {
                if let Some(pick) = p.pick.clone() {
                    p.none = Some(MenuItem::tr("_shell.no-fill").on(move |e, w, cx| pick(e, None, w, cx)));
                }
            }
        }
    }
    let entries = Rc::new(palette_entries);
    let ed = editor.clone();
    let picker_btn = action_button(SharedString::from(format!("{id}-swatch")))
        .ghost()
        .xsmall()
        .h(px(24.)).min_h(px(24.)).max_h(px(24.))
        .px(px(4.))
        .border_1()
        .border_color(rgb(BORDER))
        .rounded(px(4.))
        .child(
            h_flex()
                .items_center()
                .gap(px(2.))
                .child(swatch)
                .child(icon("arrow_drop_down", 14.0, rgb(ICON).into())),
        )
        ;
    let picker_btn=ColorPaletteDropdown::from_palette(id,picker_btn,(*entries).clone(),&ed);

    h_flex()
        .gap_1()
        .items_center()
        .w_full()
        .child(picker_btn)
        .child(div().flex_1().min_w(px(64.)).child(text_field(input).no_clear().full_width()))
        .into_any_element()
 }
}

/// The number at the start of a field's text and what follows it (a unit such as " cm", "pt", "%", "°", kept when a
/// spinner writes the stepped value back). `None` when the text does not start with a number.
pub(crate) fn split_number(raw: &str) -> Option<(f32, &str)> {
    let text = raw.trim();
    let end = text
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0)))
        .map_or(text.len(), |(i, _)| i);
    let value = text[..end].parse::<f32>().ok()?;
    let unit = text[end..].trim_end_matches('+');
    Some((value, unit))
}

/// How a [`SpinnerSpec`] steps when no explicit `steps` table is provided or outside its range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpinnerMode {
    Add,
    Multiply,
}

/// Specification for a numeric input with `▲` / `▼` step buttons.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SpinnerSpec {
    pub step: f32,
    pub mode: SpinnerMode,
    pub steps: Option<&'static [f32]>,
    pub min: Option<f32>,
    pub max: Option<f32>,
    pub integer_only: bool,
}

impl SpinnerSpec {
    pub const fn new(step: f32) -> Self {
        Self {
            step,
            mode: SpinnerMode::Add,
            steps: None,
            min: None,
            max: None,
            integer_only: false,
        }
    }

    pub const fn steps(mut self, steps: &'static [f32]) -> Self {
        self.steps = Some(steps);
        self
    }

    #[allow(dead_code)]
    pub const fn mode(mut self, mode: SpinnerMode) -> Self {
        self.mode = mode;
        self
    }

    pub const fn integer_only(mut self) -> Self {
        self.integer_only = true;
        self
    }

    pub const fn min(mut self, min: f32) -> Self {
        self.min = Some(min);
        self
    }

    #[allow(dead_code)]
    pub const fn max(mut self, max: f32) -> Self {
        self.max = Some(max);
        self
    }

    pub const fn range(mut self, min: f32, max: f32) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self
    }

    /// The value an empty (or not numeric) field steps from: 0, or the nearest end of the range when 0 is outside it.
    pub fn empty_value(&self) -> f32 {
        let lo = self.min.unwrap_or(f32::MIN);
        let hi = self.max.unwrap_or(f32::MAX).max(lo);
        0.0_f32.clamp(lo, hi)
    }

    pub fn step_value(&self, current: f32, up: bool) -> f32 {
        let eps = 1e-4_f32;
        let next = if let Some(table) = self.steps {
            if up {
                table.iter().copied().find(|&v| v > current + eps).unwrap_or_else(|| {
                    let base = table.last().copied().unwrap_or(current).max(current);
                    match self.mode {
                        SpinnerMode::Add => base + self.step,
                        SpinnerMode::Multiply => base * self.step,
                    }
                })
            } else {
                table.iter().rev().copied().find(|&v| v < current - eps).unwrap_or_else(|| {
                    let base = table.first().copied().unwrap_or(current).min(current);
                    match self.mode {
                        SpinnerMode::Add => base - self.step,
                        SpinnerMode::Multiply => {
                            if self.step.abs() > eps {
                                base / self.step
                            } else {
                                base
                            }
                        }
                    }
                })
            }
        } else {
            match self.mode {
                SpinnerMode::Add => {
                    if up {
                        current + self.step
                    } else {
                        current - self.step
                    }
                }
                SpinnerMode::Multiply => {
                    if up {
                        current * self.step
                    } else if self.step.abs() > eps {
                        current / self.step
                    } else {
                        current
                    }
                }
            }
        };
        let clamped = match (self.min, self.max) {
            (Some(lo), Some(hi)) => next.clamp(lo, hi),
            (Some(lo), None) => next.max(lo),
            (None, Some(hi)) => next.min(hi),
            (None, None) => next,
        };
        if self.integer_only {
            clamped.round()
        } else {
            (clamped * 100.0).round() / 100.0
        }
    }

    pub fn format_value(&self, val: f32) -> String {
        if self.integer_only || (val - val.round()).abs() < 1e-4 {
            format!("{}", val.round() as i64)
        } else if ((val * 10.0) - (val * 10.0).round()).abs() < 1e-3 {
            format!("{val:.1}")
        } else {
            format!("{val:.2}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MenuEntry, MenuId, MenuItem, Menus, SlideEditor, Submenu, ToolButton, ToolItem};
    use gpui_kit::{Context, Window};

    fn sample() -> Menus {
        Menus {
            menus: vec![(
                MenuId::Insert,
                vec![
                    MenuItem::new("a", "A").into(),
                    MenuEntry::Separator,
                    Submenu::new("sub", "add", "Sub", vec![MenuItem::new("b", "B").into()]).into(),
                ],
            )],
        }
    }

    #[test]
    fn placeholders_are_disabled_until_connected() {
        let mut menus = sample();
        assert!(!menus.find_mut("b").unwrap().enabled());
        assert!(menus.connect("b", |_: &mut SlideEditor, _: &mut Window, _: &mut Context<SlideEditor>| {}));
        assert!(menus.find_mut("b").unwrap().enabled());
        assert!(!menus.connect("missing", |_: &mut SlideEditor, _: &mut Window, _: &mut Context<SlideEditor>| {}));
    }

    #[test]
    fn empty_submenus_and_stray_separators_are_pruned() {
        let empty = |id| Submenu::new(id, "add", "E", vec![MenuEntry::Separator]);
        let entries = vec![
            MenuEntry::Separator,
            MenuItem::new("a", "A").into(),
            MenuEntry::Separator,
            empty("e1").into(),
            MenuEntry::Separator,
            Submenu::new("outer", "add", "O", vec![empty("e2").into(), MenuItem::new("b", "B").into(), empty("e3").into()]).into(),
            MenuEntry::Separator,
            empty("e4").into(),
        ];
        let out = super::prune_entries(entries);
        assert_eq!(out.len(), 3);
        assert!(matches!(out[0], MenuEntry::Item(_)) && matches!(out[1], MenuEntry::Separator));
        let MenuEntry::Submenu(outer) = &out[2] else { panic!("outer") };
        assert_eq!(outer.items.len(), 1);
    }

    #[test]
    fn a_format_can_add_a_menu() {
        let mut menus = sample();
        menus.items_mut(MenuId::Data).push(MenuItem::new("c", "C").into());
        assert_eq!(menus.menus.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [MenuId::Insert, MenuId::Data]);
    }

    #[test]
    fn toolbar_widths_follow_the_mock() {
        assert_eq!(ToolItem::Separator.width(), 9.0);
        assert_eq!(ToolItem::from(ToolButton::new("b", "format_bold", "太字")).width(), 28.0);
        assert!(ToolItem::from(ToolButton::new("b", "shapes", "図形").menu(Vec::new())).width() > 28.0);
    }

    #[test]
    fn every_icon_used_by_the_chrome_is_bundled() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/src/shell/");
        let source = ["chrome.rs", "chrome_defaults.rs"].iter().map(|f| std::fs::read_to_string(format!("{dir}{f}")).unwrap()).collect::<String>();
        let mut missing = Vec::new();
        for pattern in ["icon(\"", "Material(\"", "Material::fill(\"", "ToolButton::new(\""] {
            let mut at = 0;
            while let Some(i) = source[at..].find(pattern) {
                let start = at + i;
                at = start + pattern.len();
                // `title_button_icon("id", …)` names an element, not an icon.
                if source[..start].ends_with(|c: char| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let part = &source[at..];
                let rest = if pattern.starts_with("ToolButton") { part.split_once(", \"").map(|(_, r)| r).unwrap_or("") } else { part };
                let name: String = rest.chars().take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_').collect();
                if !name.is_empty() && !crate::shell::icons::has(&name) {
                    missing.push(name);
                }
            }
        }
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn window_control_labels_and_icons_are_present() {
        for key in ["_shell.minimize", "_shell.maximize", "_shell.restore", "_shell.close"] {
            let label = crate::shell::i18n::text(key);
            assert!(!label.is_empty() && label != key, "Missing i18n label for {key}");
        }
        for icon in ["remove", "crop_square", "filter_none", "close"] {
            assert!(crate::shell::icons::has(icon), "Missing window control icon {icon}");
        }
    }

    #[test]
    fn title_row_left_padding_leaves_space_for_traffic_lights_on_macos() {
        #[cfg(target_os = "macos")]
        assert!(super::TITLE_ROW_PADDING_LEFT >= 78.0);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(super::TITLE_ROW_PADDING_LEFT, 14.0);
    }

    #[test]
    fn split_number_keeps_the_unit() {
        assert_eq!(super::split_number(" 2.5 cm"), Some((2.5, " cm")));
        assert_eq!(super::split_number("-3"), Some((-3.0, "")));
        assert_eq!(super::split_number("12+"), Some((12.0, "")));
        assert_eq!(super::split_number("100%"), Some((100.0, "%")));
        assert_eq!(super::split_number("abc"), None);
    }

    #[test]
    fn spinner_spec_steps_and_clamps() {
        let spec = super::SpinnerSpec::new(1.0).range(0.0, 10.0).integer_only();
        assert_eq!(spec.step_value(5.0, true), 6.0);
        assert_eq!(spec.step_value(10.0, true), 10.0);
        assert_eq!(spec.step_value(0.0, false), 0.0);
        assert_eq!(spec.empty_value(), 0.0);
        assert_eq!(super::SpinnerSpec::new(1.0).range(1.0, 9.0).empty_value(), 1.0);
        assert_eq!(super::SpinnerSpec::new(1.0).range(-90.0, 90.0).empty_value(), 0.0);

        const TABLE: &[f32] = &[0.25, 0.5, 0.75, 1.0, 1.5, 2.25, 3.0];
        let table_spec = super::SpinnerSpec::new(0.5).steps(TABLE).min(0.0);
        assert_eq!(table_spec.step_value(0.6, true), 0.75);
        assert_eq!(table_spec.step_value(0.75, true), 1.0);
        assert_eq!(table_spec.step_value(0.75, false), 0.5);
    }

    #[test]
    fn searchable_menu_sorting_filtering_and_window_menu_width() {
        assert_eq!(MenuId::Window.key(), "menu.window");
        let mut menus = Menus::default();
        menus.items_mut(MenuId::File).push(
            MenuItem::new("b", "コピー")
                .on(|_, _, _| {})
                .into(),
        );
        menus.items_mut(MenuId::Edit).push(
            MenuItem::new("a", "切り取り")
                .on(|_, _, _| {})
                .into(),
        );
        let collected = super::collect_searchable_menu_items(&menus);
        assert_eq!(collected.len(), 2);
        assert!(collected[0].label.as_ref() <= collected[1].label.as_ref());

        let filtered = super::filter_menu_items(&collected, "切り");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "a");
        assert!(super::filter_menu_items(&collected, "存在しない").is_empty());

        let w = super::menu_width_for_entries(&[
            MenuItem::new("short", "短い").into(),
            MenuItem::new("long", "とても長いメニュー項目のラベル表示テスト").shortcut("⌥⇧⌘W").into(),
        ]);
        assert!(w > 280.0);
    }

    #[test]
    fn shortcuts_dirty_state_and_single_select_work() {
        assert_eq!(super::format_shortcut_for_target("⌘O", true), "⌘O");
        assert_eq!(super::format_shortcut_for_target("⌘O", false), "Ctrl+O");
        assert_eq!(super::format_shortcut_for_target("⌘⇧S", false), "Ctrl+Shift+S");
        assert_eq!(super::format_shortcut_for_target("⌥⌘S", false), "Ctrl+Alt+S");
        assert_eq!(super::format_shortcut_for_target("⌘⌫", false), "Ctrl+Backspace");
        assert_eq!(super::format_shortcut_for_target("⌘↩", false), "Ctrl+Enter");

        let btn = ToolButton::new("sheet-list", "menu", "").searchable().single_select();
        assert!(btn.searchable && btn.single_select);

        let mut doc = ibis_doc_model::Document::new(ibis_doc_model::Presentation::new());
        assert!(!doc.is_dirty());
        let slide = doc.presentation().slides[0].id;
        let shape = doc.ids().new_shape_id();
        doc.execute(vec![ibis_doc_model::Command::InsertShape {
            slide,
            index: 0,
            shape: ibis_doc_model::Shape::text_box(shape, ibis_doc_model::EmuRect::new(0, 0, 100, 50), "a"),
        }])
        .unwrap();
        assert!(doc.is_dirty());
        doc.mark_clean();
        assert!(!doc.is_dirty());
        assert!(doc.undo());
        assert!(doc.is_dirty());
        assert!(doc.redo());
        assert!(!doc.is_dirty());
    }

    #[test]
    fn font_and_font_size_filtering_and_keyboard_stepping_work() {
        let all_sizes = super::filtered_font_size_candidates("11", false);
        assert_eq!(all_sizes.len(), super::FONT_SIZE_CANDIDATES.len());

        let cleared_sizes = super::filtered_font_size_candidates("", true);
        assert_eq!(cleared_sizes.len(), super::FONT_SIZE_CANDIDATES.len());

        let filtered_10 = super::filtered_font_size_candidates("10", true);
        assert_eq!(filtered_10, vec![10.0, 10.5]);

        let items = vec![
            MenuItem::new("font.section.recent", "履歴").disabled(true),
            MenuItem::new("font.recent.item", "Arial").on(|_, _, _| {}),
            MenuItem::new("font.section.system", "システムフォント").disabled(true),
            MenuItem::new("xlsx.font", "Calibri").on(|_, _, _| {}),
            MenuItem::new("xlsx.font", "Times New Roman").on(|_, _, _| {}),
        ];
        let unfiltered = super::filter_menu_items(&items, "");
        assert_eq!(unfiltered.len(), 5);

        let filtered_font = super::filter_menu_items(&items, "cal");
        assert_eq!(filtered_font.len(), 1);
        assert_eq!(filtered_font[0].label.as_ref(), "Calibri");

        assert_eq!(super::step_enabled_menu_index(&items, None, 1), Some(1));
        assert_eq!(super::step_enabled_menu_index(&items, Some(1), 1), Some(3));
        assert_eq!(super::step_enabled_menu_index(&items, Some(3), 1), Some(4));
        assert_eq!(super::step_enabled_menu_index(&items, Some(4), -1), Some(3));
        assert_eq!(super::step_enabled_menu_index(&items, Some(3), -1), Some(1));
    }
}

