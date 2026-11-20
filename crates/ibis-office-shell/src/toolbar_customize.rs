// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツールバーのカスタマイズ (GitHub issue #241): the dialog opened from the settings' pages of the formats (ドキュメント,
//! スプレッドシート, プレゼンテーション). It edits a copy of the orders of the format's scenes (`toolbar_layout`); OK keeps them in
//! the settings, キャンセル and Esc drop them. While it is open the settings dialog is behind it (it is not drawn).
//!
//! Left: the commands to choose from (the menus' layout of the format, `menu_layout_data.rs`, folded by menu, with a search that
//! narrows them as it is typed). Right: the order of the scene being edited (`DataTable::reorder_list`). Under the scene's switch: the
//! preview, drawn as the real toolbar is. The history (⌘Z) is the dialog's own.
use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::chrome::data_table::{DataTable, ListState};
use crate::chrome::segment_control::{Mode, SegmentControl, SegmentItem};
use crate::chrome::virtual_table::{TableColumn, TableRow, TableState};
use crate::chrome::{ActionButtonKind, ActionDefinition, SidebarPanel, TEXT, action_button, icon, panel_note, section_heading};
use crate::editor::SlideEditor;
use crate::i18n::text;
use crate::icons::Material;
use crate::palette::rgb;
use crate::toolbar_layout as layout;

/// The id the dialog is kept by (`render_sidebar`, `discard_current_dialog`).
pub(crate) const DIALOG_KEY: &str = "toolbar_customize_dialog";
/// The dialog is larger than the settings it comes from (which hides behind it).
pub(crate) const DIALOG_WIDTH: f32 = 920.;
pub(crate) const DIALOG_HEIGHT: f32 = 640.;
const ORDER_TABLE: &str = "toolbar-customize-order";
const COMMAND_TABLE: &str = "toolbar-customize-commands";

/// A command of the left list.
#[derive(Clone)]
pub(crate) struct Command {
    /// Its key in an order (`cmd:<id>`, `menu:<id>` for a submenu).
    pub key: String,
    pub label: SharedString,
    /// The menu it is under, and the path inside it (for the search's breadcrumb).
    pub category: usize,
    pub path: SharedString,
    pub icon: Option<&'static str>,
    pub submenu: bool,
}

/// The commands of the menus of the format `ext`, by menu: `(menus' names, commands)`. A submenu that has an id is a command of its own
/// (a ▼ button that opens it) and its commands follow it; one command appears once.
pub(crate) fn catalog(ext: &str) -> (Vec<SharedString>, Vec<Command>) {
    if ext != "qat" {
        return format_catalog(ext);
    }
    // The title row's commands are chosen among the menus of the three formats (one list for all): a menu of a format is a category,
    // a submenu is not a command of it, and the buttons and separators of a toolbar are not either.
    let mut names: Vec<SharedString> = Vec::new();
    let mut commands: Vec<Command> = Vec::new();
    for (format, title) in [("docx", "settings.document"), ("xlsx", "settings.spreadsheet"), ("pptx", "settings.presentation")] {
        let (menu_names, menu_commands) = menu_commands(format);
        let first = names.len();
        names.extend(menu_names.iter().map(|n| SharedString::from(format!("{} › {n}", text(title)))));
        for mut c in menu_commands {
            if c.submenu || commands.iter().any(|k| k.key == c.key) {
                continue;
            }
            c.category += first;
            commands.push(c);
        }
    }
    (names, commands)
}

/// The catalog of a format: 区切り線, よく使うコマンド, the toolbar's own buttons, and the commands of the menus.
fn format_catalog(ext: &str) -> (Vec<SharedString>, Vec<Command>) {
    let (names, commands) = menu_commands(ext);
    finish_catalog(ext, names, commands)
}

/// The commands of the menus of the format `ext`, by menu.
fn menu_commands(ext: &str) -> (Vec<SharedString>, Vec<Command>) {
    use crate::menu_layout::LayoutItem;
    let kind = match ext {
        "docx" => ibis_formats::Kind::Document,
        "xlsx" => ibis_formats::Kind::Workbook,
        _ => ibis_formats::Kind::Presentation,
    };
    let mut names = Vec::new();
    let mut commands: Vec<Command> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    #[allow(clippy::too_many_arguments)]
    fn walk(items: &[LayoutItem], category: usize, path: &str, commands: &mut Vec<Command>, seen: &mut HashSet<String>) {
        fn add(key: String, label: SharedString, submenu: bool, category: usize, path: &str, commands: &mut Vec<Command>, seen: &mut HashSet<String>) {
            if seen.insert(key.clone()) {
                commands.push(Command { key, label, category, path: path.to_string().into(), icon: None, submenu });
            }
        }
        for item in items {
            match item {
                LayoutItem::Cmd(id) | LayoutItem::CmdOrPlanned { id, .. } => add(format!("{}{id}", layout::COMMAND), command_label(id), false, category, path, commands, seen),
                LayoutItem::CmdAs { id, label_key } => add(format!("{}{id}", layout::COMMAND), command_label(label_key), false, category, path, commands, seen),
                LayoutItem::Sub { id, label_key, items } => {
                    let label = command_label(label_key);
                    if let Some(id) = id {
                        add(format!("{}{id}", layout::SUBMENU), label.clone(), true, category, path, commands, seen);
                    }
                    let inner = if path.is_empty() { label.to_string() } else { format!("{path} › {label}") };
                    walk(items, category, &inner, commands, seen);
                }
                LayoutItem::Sep | LayoutItem::Dynamic(_) | LayoutItem::Planned { .. } => {}
            }
        }
    }
    for spec in crate::menu_layout::layout(kind) {
        let category = names.len();
        names.push(spec.id.label());
        walk(spec.items, category, "", &mut commands, &mut seen);
    }
    (names, commands)
}

fn finish_catalog(ext: &str, names: Vec<SharedString>, mut commands: Vec<Command>) -> (Vec<SharedString>, Vec<Command>) {
    // In front of the menus' commands: 区切り線, よく使うコマンド (the ones the menus have), and the toolbar's own buttons (seen so far).
    const FRONT: usize = 3;
    for command in &mut commands {
        command.category += FRONT;
    }
    let mut front: Vec<Command> = vec![Command { key: layout::SEPARATOR.to_string(), label: text("toolbar.customize.separator"), category: 0, path: "".into(), icon: Some("more_vert"), submenu: false }];
    for id in POPULAR.iter().chain(match ext {
        "docx" => POPULAR_DOCX.iter(),
        "xlsx" => POPULAR_XLSX.iter(),
        _ => POPULAR_PPTX.iter(),
    }) {
        let key = format!("{}{id}", layout::COMMAND);
        if let Some(found) = commands.iter().find(|c| c.key == key) {
            front.push(Command { category: 1, ..found.clone() });
        }
    }
    let mut buttons: Vec<String> = Vec::new();
    for scene in layout::scenes(ext) {
        for key in layout::seen_default(ext, scene.id).unwrap_or_default() {
            if key != layout::SEPARATOR && !buttons.contains(&key) {
                buttons.push(key);
            }
        }
    }
    for key in buttons {
        let (icon, label) = layout::known(&key).unwrap_or((None, key.clone().into()));
        front.push(Command { key, label, category: 2, path: "".into(), icon, submenu: false });
    }
    let mut all_names = vec![text("toolbar.customize.separator"), text("toolbar.customize.popular"), text("toolbar.customize.toolbar-buttons")];
    all_names.extend(names);
    front.extend(commands);
    (all_names, front)
}

/// The commands that come under よく使うコマンド (those of them the format's menus have).
const POPULAR: &[&str] = &["file.open", "file.save", "file.print", "edit.undo", "edit.redo", "edit.find-replace", "file.export.pdf", "insert.comment"];
const POPULAR_DOCX: &[&str] = &["docx.track.record", "review.accessibility-check"];
const POPULAR_XLSX: &[&str] = &["xlsx.sort", "xlsx.filter", "xlsx.insert-table", "xlsx.fill-down"];
const POPULAR_PPTX: &[&str] = &["pptx.new-slide", "pptx.duplicate-slide"];

/// The name of the command `id`: its words in the tables (the id itself, or the id with `_` before it, as the command tables
/// of the formats spell some), else the id as it is (a command of a format the open document is not has no words here).
fn command_label(id: &str) -> SharedString {
    crate::i18n::lookup_words(id).or_else(|| crate::i18n::lookup_words(&format!("_{id}"))).unwrap_or_else(|| id.to_string().into())
}

/// A key as the search sees it: lower case, full-width letters and digits as half-width, katakana as hiragana.
pub(crate) fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            '\u{3000}' => ' ',
            '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c,
        })
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

/// `label` cut into parts, each with whether it is a match of a word of `query` (as `matches` finds them: folded, so what is marked is
/// what was typed in another script or width too). The folding keeps the number of characters, so the places are the label's.
pub(crate) fn highlight(label: &str, query: &str) -> Vec<(String, bool)> {
    let hay: Vec<char> = fold(label).chars().collect();
    let chars: Vec<char> = label.chars().collect();
    let mut marked = vec![false; chars.len()];
    for word in fold(query).split_whitespace() {
        let word: Vec<char> = word.chars().collect();
        if word.is_empty() || word.len() > hay.len() {
            continue;
        }
        for at in 0..=hay.len() - word.len() {
            if hay[at..at + word.len()] == word[..] {
                marked[at..at + word.len()].iter_mut().for_each(|m| *m = true);
            }
        }
    }
    let mut parts: Vec<(String, bool)> = Vec::new();
    for (c, m) in chars.into_iter().zip(marked) {
        match parts.last_mut() {
            Some((text, on)) if *on == m => text.push(c),
            _ => parts.push((c.to_string(), m)),
        }
    }
    parts
}

/// Whether every word of `query` (split at spaces) is in `haystack` (both folded).
pub(crate) fn matches(haystack: &str, query: &str) -> bool {
    let hay = fold(haystack);
    fold(query).split_whitespace().all(|word| hay.contains(word))
}

/// A command's name with the parts the search found marked (a yellow background).
fn label_text(label: &SharedString, query: &str) -> AnyElement {
    if query.trim().is_empty() {
        return label.clone().into_any_element();
    }
    h_flex()
        .children(highlight(label, query).into_iter().map(|(part, found)| if found { div().bg(rgb(0xfff2a8)).child(part).into_any_element() } else { div().child(part).into_any_element() }))
        .into_any_element()
}

/// What the dialog keeps while it is open.
pub struct ToolbarCustomize {
    /// The format whose toolbar is edited: `docx`, `xlsx` or `pptx`.
    pub(crate) ext: &'static str,
    /// The scene shown (an index in `toolbar_layout::scenes`).
    pub(crate) scene: usize,
    /// The order of each scene as edited: `None` while it is the format's own.
    pub(crate) orders: Vec<Option<Vec<String>>>,
    /// The orders when the dialog opened (to tell whether anything changed).
    original: Vec<Option<Vec<String>>>,
    /// The orders before each change (⌘Z) and the ones undone (⌘⇧Z).
    undo: Vec<Vec<Option<Vec<String>>>>,
    redo: Vec<Vec<Option<Vec<String>>>>,
    /// The row of the order that is selected.
    pub(crate) selected: Option<usize>,
    pub(crate) names: Vec<SharedString>,
    pub(crate) commands: Rc<Vec<Command>>,
    pub(crate) collapsed: HashSet<usize>,
    pub(crate) hide_added: bool,
    /// プレゼンテーション: a command put in is put in every scene (the choice of the checkbox above the list).
    pub(crate) all_scenes: bool,
    pub(crate) search: Entity<InputState>,
    pub(crate) command_state: TableState,
    pub(crate) order_list: ListState,
    pub(crate) preview: crate::toolbar_preview::Preview,
    _subscription: Subscription,
}

impl ToolbarCustomize {
    /// The dialog for the format `ext`, in the scene `scene` (an id; the first one when it is not one of the format's).
    pub(crate) fn new(ext: &'static str, scene: &str, window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        let scenes = layout::scenes(ext);
        let orders: Vec<Option<Vec<String>>> = if ext == "qat" {
            let ids = crate::qat::commands();
            vec![(!ids.is_empty()).then(|| ids.iter().map(|id| format!("{}{id}", layout::COMMAND)).collect())]
        } else {
            scenes.iter().map(|s| layout::saved(ext, s.id)).collect()
        };
        let (names, commands) = catalog(ext);
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(text("toolbar.customize.search")));
        let subscription = cx.subscribe_in(&search, window, |_, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        Self {
            ext,
            scene: scenes.iter().position(|s| s.id == scene).unwrap_or(0),
            original: orders.clone(),
            orders,
            undo: Vec::new(),
            redo: Vec::new(),
            selected: None,
            names,
            commands: Rc::new(commands),
            collapsed: HashSet::new(),
            hide_added: false,
            all_scenes: false,
            search,
            command_state: TableState::default(),
            order_list: ListState::default(),
            preview: Default::default(),
            _subscription: subscription,
        }
    }

    /// Whether the dialog changed any order.
    pub(crate) fn changed(&self) -> bool {
        self.orders != self.original
    }

    /// The id of the scene being edited.
    fn scene_id(&self) -> &'static str {
        layout::scenes(self.ext).get(self.scene).map_or("", |s| s.id)
    }

    /// The order the scene has until it is changed (when the scene has been seen), without the locked buttons.
    pub(crate) fn default_order(&self) -> Option<Vec<String>> {
        if self.ext == "qat" {
            return Some(Vec::new());
        }
        self.default_order_of(self.scene)
    }

    fn default_order_of(&self, scene: usize) -> Option<Vec<String>> {
        layout::seen_default(self.ext, layout::scenes(self.ext).get(scene).map_or("", |s| s.id))
    }

    /// The order of the scene shown: the edited one, else the format's own (empty when it is not known yet).
    pub(crate) fn order(&self) -> Vec<String> {
        self.order_of(self.scene)
    }

    fn order_of(&self, scene: usize) -> Vec<String> {
        self.orders.get(scene).cloned().flatten().or_else(|| if self.ext == "qat" { Some(Vec::new()) } else { self.default_order_of(scene) }).unwrap_or_default()
    }

    /// Whether the scene's order can be changed: its own order is known (an order made of nothing would take its buttons away).
    pub(crate) fn editable(&self) -> bool {
        self.orders.get(self.scene).is_some_and(Option::is_some) || self.default_order().is_some()
    }

    /// Change the order of the scene shown (kept in the history). An order equal to the format's own is the format's own again.
    pub(crate) fn edit(&mut self, change: impl FnOnce(&mut Vec<String>)) {
        if !self.editable() {
            return;
        }
        let before = self.orders.clone();
        let mut order = self.order();
        change(&mut order);
        let own = self.default_order().is_some_and(|d| d == order);
        self.orders[self.scene] = (!own).then_some(order);
        if self.orders != before {
            self.undo.push(before);
            self.redo.clear();
        }
    }

    /// Put `key` in the order after the selected row (at the end when none is). A command is in an order once; a separator any times.
    /// Whether it was put.
    pub(crate) fn add(&mut self, key: &str) -> bool {
        self.add_at(key, self.selected.map(|i| i + 1))
    }

    /// As `add`, at the place `at` of the order (the end when `None`).
    pub(crate) fn add_at(&mut self, key: &str, at: Option<usize>) -> bool {
        if key != layout::SEPARATOR && self.order().iter().any(|k| k == key) {
            return false;
        }
        let key = key.to_string();
        let mut placed = 0;
        self.edit(|order| {
            let at = at.map_or(order.len(), |i| i.min(order.len()));
            order.insert(at, key);
            placed = at;
        });
        self.selected = Some(placed);
        true
    }

    /// A command put in the order of every scene (プレゼンテーション): after the selected row in the scene shown, at the end of the others.
    /// A scene whose own order is not known yet is left as it is (an order made of this command alone would take its buttons away). One step of the history.
    pub(crate) fn add_all(&mut self, key: &str) -> bool {
        let before = self.orders.clone();
        let mut added = false;
        for scene in 0..self.orders.len() {
            if self.orders[scene].is_none() && self.default_order_of(scene).is_none() {
                continue;
            }
            let mut order = self.order_of(scene);
            if key != layout::SEPARATOR && order.iter().any(|k| k == key) {
                continue;
            }
            let at = if scene == self.scene { self.selected.map_or(order.len(), |i| (i + 1).min(order.len())) } else { order.len() };
            order.insert(at, key.to_string());
            let own = self.default_order_of(scene).is_some_and(|d| d == order);
            self.orders[scene] = (!own).then_some(order);
            added = true;
        }
        if added {
            self.undo.push(before);
            self.redo.clear();
        }
        added
    }

    /// The command put in as the dialog is set to (this scene only, or every scene).
    pub(crate) fn add_choice(&mut self, key: &str) -> bool {
        if self.all_scenes && self.ext == "pptx" { self.add_all(key) } else { self.add(key) }
    }

    pub(crate) fn remove(&mut self, index: usize) {
        self.edit(|order| {
            if index < order.len() {
                order.remove(index);
            }
        });
        self.selected = None;
    }

    pub(crate) fn move_row(&mut self, from: usize, to: usize) {
        self.edit(|order| {
            if from < order.len() && to < order.len() && from != to {
                let key = order.remove(from);
                order.insert(to, key);
            }
        });
        self.selected = Some(to);
    }

    pub(crate) fn undo(&mut self) {
        if let Some(before) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.orders, before));
            self.selected = None;
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(after) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.orders, after));
            self.selected = None;
        }
    }

    /// The scene (or every scene) goes back to the format's own order.
    pub(crate) fn reset(&mut self, all: bool) {
        let before = self.orders.clone();
        if all {
            self.orders.iter_mut().for_each(|o| *o = None);
        } else {
            self.orders[self.scene] = None;
        }
        if self.orders != before {
            self.undo.push(before);
            self.redo.clear();
        }
        self.selected = None;
    }

    /// Keep the orders in the settings: the ones that are the format's own are taken away.
    pub(crate) fn save(&self) {
        if self.ext == "qat" {
            let ids: Vec<String> = self.order_of(0).iter().filter_map(|k| k.strip_prefix(layout::COMMAND).map(str::to_string)).collect();
            crate::qat::set(&ids);
            return;
        }
        for (scene, order) in layout::scenes(self.ext).iter().zip(&self.orders) {
            match order {
                Some(keys) => layout::save(self.ext, scene.id, keys),
                None => layout::reset(self.ext, scene.id),
            }
        }
    }

    /// The parts of the preview: the locked buttons and a separator, then the order's (with the place in the order). The buttons do
    /// nothing in the preview (a handler that does nothing keeps them looking as live ones).
    pub(crate) fn preview_entries(&self, order: &[String]) -> Vec<(crate::chrome::ToolItem, Option<usize>)> {
        use crate::chrome::{MenuEntry, ToolButton, ToolItem};
        let nothing: crate::chrome::Handler = Rc::new(|_, _, _| {});
        let mut entries: Vec<(ToolItem, Option<usize>)> = if self.ext == "qat" {
            // The title row: undo and redo come first, fixed.
            [("qat-undo", "undo", "toolbar.customize.undo"), ("qat-redo", "redo", "toolbar.customize.redo")]
                .into_iter()
                .map(|(id, icon, name)| {
                    let mut button = ToolButton::new(id, icon, text(name));
                    button.handler = Some(nothing.clone());
                    (button.into(), None)
                })
                .collect()
        } else {
            let mut locked: Vec<(ToolItem, Option<usize>)> = layout::LOCKED
                .iter()
                .map(|id| {
                    let (icon, label) = layout::known(id).unwrap_or((None, (*id).into()));
                    let mut button = ToolButton::new(id, icon.unwrap_or("help"), label);
                    button.handler = Some(nothing.clone());
                    (button.into(), None)
                })
                .collect();
            locked.push((ToolItem::Separator, None));
            locked
        };
        for (i, key) in order.iter().enumerate() {
            if key == layout::SEPARATOR {
                entries.push((ToolItem::Separator, Some(i)));
                continue;
            }
            let (label, icon_name) = self.describe(key);
            let id = static_id(key);
            let mut button = match icon_name {
                Some(n) => ToolButton::new(id, n, label),
                None => ToolButton::text(id, label.clone(), label),
            };
            button.handler = Some(nothing.clone());
            if key.starts_with(layout::SUBMENU) {
                button.menu = Some(vec![MenuEntry::Separator]);
            }
            entries.push((button.into(), Some(i)));
        }
        entries
    }

    /// The widths of the preview's parts, and where the order's begin.
    pub(crate) fn preview_widths(&self) -> Option<(Vec<f32>, usize, usize)> {
        let entries = self.preview_entries(&self.order());
        let base = entries.iter().position(|(_, o)| o.is_some())?;
        let widths: Vec<f32> = entries.iter().map(|(item, _)| item.width()).collect();
        let shown = crate::toolbar_preview::fit(&widths, f32::from(self.preview.bounds.get().size.width));
        let mut with_more: Vec<f32> = widths[..shown].to_vec();
        let last = shown.saturating_sub(1).max(base);
        if shown < widths.len() {
            with_more.push(crate::toolbar_preview::MORE_WIDTH);
        }
        Some((with_more, base, last))
    }

    /// The name and the icon of a key of an order.
    pub(crate) fn describe(&self, key: &str) -> (SharedString, Option<&'static str>) {
        if key == layout::SEPARATOR {
            return (text("toolbar.customize.separator"), Some("more_vert"));
        }
        if let Some(c) = self.commands.iter().find(|c| c.key == key) {
            return (c.label.clone(), c.icon.or(c.submenu.then_some("arrow_drop_down")));
        }
        match layout::known(key) {
            Some((icon, label)) => (label, icon),
            None => (key.to_string().into(), None),
        }
    }
}

/// A key as the `&'static str` id of a button (each key once).
fn static_id(key: &str) -> &'static str {
    thread_local! {
        static IDS: std::cell::RefCell<std::collections::HashMap<String, &'static str>> = Default::default();
    }
    IDS.with(|ids| *ids.borrow_mut().entry(key.to_string()).or_insert_with(|| Box::leak(key.to_string().into_boxed_str())))
}

/// How the settings page of a format says its toolbar is: the format's own buttons, or customized in some scene.
pub(crate) fn is_customized(ext: &str) -> bool {
    layout::scenes(ext).iter().any(|s| layout::saved(ext, s.id).is_some())
}

/// A row of the left list.
enum Row {
    Group { category: usize, count: usize },
    Command(usize),
}

impl SlideEditor {
    /// Open the dialog for the format `ext` over the settings dialog.
    pub(crate) fn open_toolbar_customize(&mut self, ext: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        // The scene the open document is in, when it is a document of that format.
        let scene = if self.has_document && self.format_info().extension == ext { crate::format::with_format!(self, toolbar_scene(self)) } else { "" };
        let mut state = ToolbarCustomize::new(ext, scene, window, cx);
        // The commands' icons are the open menus', for the format of the open document.
        if self.has_document && (self.format_info().extension == ext || ext == "qat") {
            let menus = self.menus();
            let commands: Vec<Command> = state
                .commands
                .iter()
                .cloned()
                .map(|mut c| {
                    let id = c.key.split_once(':').map_or("", |(_, id)| id);
                    if let Some(item) = crate::chrome::quick_access_item(&menus, id) {
                        c.label = item.label.clone();
                        c.icon = item.icon.filter(|n| !n.is_empty());
                    }
                    c
                })
                .collect();
            state.commands = Rc::new(commands);
        }
        if let Some(s) = self.settings.as_mut() {
            s.customize = Some(state);
        }
        self.dialog.bounds = None;
        self.dialog.drag = None;
        cx.notify();
    }

    /// The keys of the dialog: Escape gives up a button being dragged in the preview; ⌘Z and ⌘⇧Z (Ctrl elsewhere) undo and redo; ⌥↑ ⌥↓
    /// (⌥Home, ⌥End) move the selected row; ⌫ takes it out (not while the search field has the keys). Whether the key was taken.
    pub(crate) fn customize_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(c) = self.customize_mut() else { return false };
        let key = &event.keystroke;
        let typing = c.search.read(cx).focus_handle(cx).is_focused(window);
        let command = if cfg!(target_os = "macos") { key.modifiers.platform } else { key.modifiers.control };
        let handled = if key.key == "escape" && c.preview.held.take().is_some() {
            true
        } else if typing {
            false
        } else if command && key.key == "z" && !key.modifiers.alt {
            if key.modifiers.shift { c.redo() } else { c.undo() }
            true
        } else if key.modifiers.alt && matches!(key.key.as_str(), "up" | "down" | "home" | "end") {
            if let Some(from) = c.selected {
                let last = c.order().len().saturating_sub(1);
                let to = match key.key.as_str() {
                    "up" => from.saturating_sub(1),
                    "down" => (from + 1).min(last),
                    "home" => 0,
                    _ => last,
                };
                c.move_row(from, to);
            }
            true
        } else if matches!(key.key.as_str(), "backspace" | "delete") && !command && !key.modifiers.alt {
            if let Some(index) = c.selected {
                c.remove(index);
            }
            true
        } else {
            false
        };
        if handled {
            cx.notify();
        }
        handled
    }

    fn customize_mut(&mut self) -> Option<&mut ToolbarCustomize> {
        self.settings.as_mut().and_then(|s| s.customize.as_mut())
    }

    /// The dialog's panel while it is open.
    pub(crate) fn toolbar_customize_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.settings.as_ref()?.customize.as_ref()?;
        let editor = cx.weak_entity();
        let scenes = layout::scenes(state.ext);
        let selected_scene = state.scene;
        let scene_items = scenes.iter().enumerate().map(|(i, scene)| {
            let changed = state.orders[i].is_some();
            let name = layout::scene_name(scene);
            let name: SharedString = if changed { format!("{name} ●").into() } else { name };
            SegmentItem::label(("toolbar-customize-scene", i), name, i == selected_scene).on_click(cx.listener(move |e, _, _, cx| {
                if let Some(c) = e.customize_mut() {
                    c.scene = i;
                    c.selected = None;
                }
                cx.notify();
            }))
        });
        let query = state.search.read(cx).value().to_string();
        let order = state.order();
        let known_default = state.default_order().is_some();

        // ---- Left: the commands ----
        let in_order: HashSet<&str> = order.iter().map(String::as_str).collect();
        let searching = !query.trim().is_empty();
        let mut rows: Vec<Row> = Vec::new();
        for category in 0..state.names.len() {
            let mut inside: Vec<usize> = state
                .commands
                .iter()
                .enumerate()
                .filter(|(_, c)| c.category == category)
                .filter(|(_, c)| !(state.hide_added && in_order.contains(c.key.as_str())))
                .filter(|(_, c)| !searching || matches(&format!("{} {} {}", c.label, c.path, c.key), &query))
                .map(|(i, _)| i)
                .collect();
            if inside.is_empty() {
                continue;
            }
            rows.push(Row::Group { category, count: inside.len() });
            if !searching && state.collapsed.contains(&category) {
                continue;
            }
            rows.extend(inside.drain(..).map(Row::Command));
        }
        let rows = Rc::new(rows);
        let command_count = rows.len();
        let commands = state.commands.clone();
        let names = state.names.clone();
        let collapsed = state.collapsed.clone();
        let order_keys: Rc<HashSet<String>> = Rc::new(order.iter().cloned().collect());
        let build_commands = {
            let highlight_query = query.clone();
            let (rows, commands, names, editor, order_keys) = (rows.clone(), commands.clone(), names.clone(), editor.clone(), order_keys.clone());
            move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                range
                    .filter_map(|i| rows.get(i))
                    .map(|row| match row {
                        Row::Group { category, count } => {
                            let category = *category;
                            let editor = editor.clone();
                            TableRow::group(None, names[category].clone()).count(*count).collapsible(collapsed.contains(&category)).on_click(move |_, _, cx| {
                                let _ = editor.update(cx, |e, cx| {
                                    if let Some(c) = e.customize_mut()
                                        && !c.collapsed.remove(&category)
                                    {
                                        c.collapsed.insert(category);
                                    }
                                    cx.notify();
                                });
                            })
                        }
                        Row::Command(index) => {
                            let command = commands[*index].clone();
                            let added = command.key != layout::SEPARATOR && order_keys.contains(&command.key);
                            let mut label = h_flex().gap(px(8.)).items_center().min_w_0();
                            label = label
                                .child(div().w(px(20.)).flex_none().children(command.icon.map(|n| icon(n, 18., rgb(TEXT).into()))))
                                .child(div().flex_1().min_w_0().truncate().child(label_text(&command.label, &highlight_query)));
                            if command.submenu {
                                label = label.child(icon("arrow_drop_down", 16., rgb(TEXT).into()));
                            }
                            if added {
                                label = label.opacity(0.5);
                            }
                            let key = command.key.clone();
                            let (add_editor, add_key) = (editor.clone(), key.clone());
                            let click_editor = editor.clone();
                            TableRow::new(vec![label.into_any_element()])
                                .draggable(*index as u64, command.label.clone())
                                .on_click(move |event, _, cx| {
                                    if event.click_count() >= 2 {
                                        let (editor, key) = (click_editor.clone(), key.clone());
                                        let _ = editor.update(cx, |e, cx| {
                                            if let Some(c) = e.customize_mut() {
                                                c.add_choice(&key);
                                            }
                                            cx.notify();
                                        });
                                    }
                                })
                                .action("toolbar-customize-add", "add", text("toolbar.customize.add"), move |_, _, cx| {
                                    let (editor, key) = (add_editor.clone(), add_key.clone());
                                    let _ = editor.update(cx, |e, cx| {
                                        if let Some(c) = e.customize_mut() {
                                            c.add_choice(&key);
                                        }
                                        cx.notify();
                                    });
                                })
                        }
                    })
                    .collect()
            }
        };
        let command_table = DataTable::virtualized(COMMAND_TABLE, vec![TableColumn::flex("command")], vec![div().into_any_element()], state.command_state.clone(), command_count, build_commands, cx)
            .row_dnd(state.order_list.dnd.clone(), |_, _, _, _| {})
            .row_actions(1)
            .headerless()
            .empty(panel_note(text("toolbar.customize.no-hit").to_string().replace("{q}", query.trim())).p_3().into_any_element());

        // ---- Right: the order ----
        let described: Rc<Vec<(String, SharedString, Option<&'static str>)>> = Rc::new(
            order
                .iter()
                .map(|key| {
                    let (label, icon) = state.describe(key);
                    (key.clone(), label, icon)
                })
                .collect(),
        );
        let selected_row = state.selected;
        let build_order = {
            let (described, editor) = (described.clone(), editor.clone());
            move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                // An empty order is one row that the commands are dropped on (it is not a command and cannot be dragged).
                if described.is_empty() {
                    return vec![TableRow::new(vec![panel_note(text("toolbar.customize.empty")).into_any_element()]).drop_between(0)];
                }
                range
                    .filter_map(|i| described.get(i).map(|d| (i, d)))
                    .map(|(i, (key, label, icon_name))| {
                        let name = h_flex()
                            .gap(px(8.))
                            .items_center()
                            .min_w_0()
                            .child(div().w(px(20.)).flex_none().children(icon_name.map(|n| icon(n, 18., rgb(TEXT).into()))))
                            .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                            .when(key.starts_with(layout::SUBMENU), |d| d.child(icon("arrow_drop_down", 16., rgb(TEXT).into())))
                            .into_any_element();
                        let (delete_editor, select_editor) = (editor.clone(), editor.clone());
                        TableRow::new(vec![name])
                            .selected(selected_row == Some(i))
                            .draggable(i as u64, label.clone())
                            .drop_between(i as u64)
                            .on_click(move |_, _, cx| {
                                let editor = select_editor.clone();
                                let _ = editor.update(cx, |e, cx| {
                                    if let Some(c) = e.customize_mut() {
                                        c.selected = Some(i);
                                    }
                                    cx.notify();
                                });
                            })
                            .action("toolbar-customize-remove", "delete", text("toolbar.customize.remove"), move |_, _, cx| {
                                let editor = delete_editor.clone();
                                let _ = editor.update(cx, |e, cx| {
                                    if let Some(c) = e.customize_mut() {
                                        c.remove(i);
                                    }
                                    cx.notify();
                                });
                            })
                    })
                    .collect()
            }
        };
        let order_table = DataTable::reorder_list_dnd(
            ORDER_TABLE,
            vec![TableColumn::flex("command")],
            vec![div().into_any_element()],
            &state.order_list,
            described.len().max(1),
            build_order,
            |e, ev, _, cx| {
                use crate::chrome::row_dnd::DropPlace;
                if let Some(c) = e.customize_mut() {
                    if ev.from_table == ORDER_TABLE {
                        c.move_row(ev.from as usize, ev.target as usize);
                    } else if let Some(key) = c.commands.get(ev.from as usize).map(|command| command.key.clone()) {
                        let at = ev.target as usize + usize::from(ev.place != DropPlace::Before);
                        if c.all_scenes && c.ext == "pptx" {
                            c.add_all(&key);
                        } else {
                            c.add_at(&key, Some(at));
                        }
                    }
                }
                cx.notify();
            },
            cx,
        )
        .row_actions(1)
        .headerless()
        .empty(panel_note(text("toolbar.customize.empty")).p_3().into_any_element());

        // ---- Header buttons ----
        let is_qat = state.ext == "qat";
        let state_ext_is_pptx = state.ext == "pptx";
        let all_scenes_on = state.all_scenes;
        let can_undo = !state.undo.is_empty();
        let can_redo = !state.redo.is_empty();
        let hide_added = state.hide_added;
        let button = |id: &'static str, label: SharedString, name: &'static str| action_button(id).small().icon(gpui_kit::component::Icon::new(Material(name))).tooltip(label);
        let toolbar_buttons = h_flex()
            .flex_wrap()
            .gap_1()
            .child(button("toolbar-customize-undo", text("toolbar.customize.undo"), "undo").disabled(!can_undo).on_click(cx.listener(|e, _, _, cx| {
                if let Some(c) = e.customize_mut() {
                    c.undo();
                }
                cx.notify();
            })))
            .child(button("toolbar-customize-redo", text("toolbar.customize.redo"), "redo").disabled(!can_redo).on_click(cx.listener(|e, _, _, cx| {
                if let Some(c) = e.customize_mut() {
                    c.redo();
                }
                cx.notify();
            })))
            .when(!is_qat, |d| {
                d.child(action_button("toolbar-customize-separator").small().label(text("toolbar.customize.add-separator")).on_click(cx.listener(|e, _, _, cx| {
                    if let Some(c) = e.customize_mut() {
                        c.add_choice(layout::SEPARATOR);
                    }
                    cx.notify();
                })))
            })
            .child(action_button("toolbar-customize-reset").small().label(text("toolbar.customize.reset-scene")).on_click(cx.listener(|e, _, _, cx| {
                if let Some(c) = e.customize_mut() {
                    c.reset(false);
                }
                cx.notify();
            })))
            .child(action_button("toolbar-customize-reset-all").small().label(text("toolbar.customize.reset-all")).on_click(cx.listener(|e, _, _, cx| {
                if let Some(c) = e.customize_mut() {
                    c.reset(true);
                }
                cx.notify();
            })));
        let preview = self.customize_preview(&order, &editor);
        let left = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_2()
            .child(section_heading(text("toolbar.customize.commands")))
            .child(crate::chrome::text_field(&state.search).full_width())
            .when(state_ext_is_pptx, |d| {
                d.child(action_button("toolbar-customize-all-scenes").small().label(text("toolbar.customize.all-scenes")).selected(all_scenes_on).on_click(cx.listener(|e, _, _, cx| {
                    if let Some(c) = e.customize_mut() {
                        c.all_scenes = !c.all_scenes;
                    }
                    cx.notify();
                })))
            })
            .child(action_button("toolbar-customize-hide-added").small().label(text("toolbar.customize.hide-added")).selected(hide_added).on_click(cx.listener(|e, _, _, cx| {
                if let Some(c) = e.customize_mut() {
                    c.hide_added = !c.hide_added;
                }
                cx.notify();
            })))
            .child(div().id("toolbar-customize-commands-host").w_full().flex_1().min_h_0().child(command_table));
        let right = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_2()
            .child(section_heading(text("toolbar.customize.order")))
            .child(toolbar_buttons)
            .when(!known_default, |d| d.child(panel_note(text("toolbar.customize.unknown-default"))))
            .child(div().id("toolbar-customize-order-host").w_full().flex_1().min_h_0().child(order_table));
        let trash = self.preview_trash();
        let body = v_flex()
            .size_full()
            .gap_2()
            .when(scenes.len() > 1, |d| d.child(h_flex().gap_2().items_center().child(panel_note(text("toolbar.customize.context"))).child(SegmentControl::new(Mode::Intrinsic).children(scene_items))))
            .child(preview)
            .child(h_flex().flex_1().min_h_0().gap_3().items_stretch().child(left).child(right));
        let changed = state.changed();
        let body = div().relative().size_full().child(body).children(trash);
        let actions = crate::chrome::action_bar()
            .action(
                ActionDefinition::new("toolbar-customize-ok", text("shell.action.ok"), ActionButtonKind::Primary).on_click(cx.listener(|e, _, _, cx| {
                    if let Some(s) = e.settings.as_mut()
                        && let Some(c) = s.customize.take()
                    {
                        c.save();
                    }
                    cx.refresh_windows();
                    cx.notify();
                })),
            )
            .action(ActionDefinition::new("toolbar-customize-cancel", text("_shell.cancel"), ActionButtonKind::Normal).on_click(cx.listener(|e, _, _, cx| {
                if let Some(s) = e.settings.as_mut() {
                    s.customize = None;
                }
                cx.notify();
            })));
        let _ = changed;
        Some(SidebarPanel::with_footer(text("settings.toolbar.customize"), body.into_any_element(), actions))
    }
}
