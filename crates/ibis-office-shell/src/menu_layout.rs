// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The menu bar's layout as data (menu bar redesign, stage 2; `tasks/doc/menu-redesign/implementation-spec.md` section 3).
//!
//! Which menus a screen has, and which commands (`commands.rs`) each shows, in order, with the separators and
//! submenus, is the tables of `menu_layout_data.rs` (made from the appendix A of the redesign by
//! `scripts/gen-menu-layout.py`). [`SlideEditor::menu_bar_menus`] puts the commands of the open document into the layout.
//! A command the layout names that the open document does not have (a stage that is not made yet, or a command
//! of another selection) is left out; a command the layout does not name stays reachable by the menu search.
use std::collections::HashSet;

use ibis_pptx::Kind;

use crate::chrome::{prune_entries, MenuEntry, MenuId, MenuItem, Menus, Submenu};
use crate::commands::CommandRegistry;
use crate::editor::SlideEditor;
use crate::i18n;
use crate::menu_context::{visible_contexts, ContextMenu, ObjectKind};

/// A line of a menu of the layout.
pub enum LayoutItem {
    /// The command with this id.
    Cmd(&'static str),
    /// The command `id` under the words of `label_key` in this menu (the design names one command differently in two menus: 校閲 ▸ 新しいコメント
    /// and 挿入 ▸ コメント both run `insert.comment`; the lines whose menu words follow the design, `tasks/doc/menu-redesign/menu-wording-2.tsv`).
    /// Only the words differ; what the line runs, its shortcut and (for a submenu of the old menus) its items are the command's. The toolbar and
    /// the shortcut list keep the command's words; the menu search finds the line under both (`command_search_items`).
    CmdAs { id: &'static str, label_key: &'static str },
    Sep,
    /// A submenu. `id` is the command id of a submenu the old menus had (its words and, when the layout names no
    /// item inside it, its items are the old ones; a `label_key` other than `id` gives it the menu's own words, as `CmdAs`); a submenu with no
    /// `id` is a group the layout adds (`label_key` is its words in the tables).
    Sub { id: Option<&'static str>, label_key: &'static str, items: &'static [LayoutItem] },
    /// Lines that are made when the menu is built (the windows that are open).
    Dynamic(DynamicKind),
    /// A line of the design that is not made yet: its words (`key` is in `i18n/<language>/menu_planned.tsv`, the design's words and
    /// a note that it is in development), always grey, with no icon and nothing behind it.
    Planned { key: &'static str },
    /// The command `id` when this screen has it, else a planned line of `key`: a line the classification of the design's gaps tied to a command
    /// that another screen has.
    CmdOrPlanned { id: &'static str, key: &'static str },
}

/// Whether the lines of the design that are not made yet ([`LayoutItem::Planned`], and [`LayoutItem::CmdOrPlanned`] where the screen has
/// no such command: grey, 「(開発中)」) are shown. **The one place that decides it**: every menu made from the layout (the menu bar, the
/// context menus, the Draw button's menu, the right-click menu of a document's text) goes through [`Resolver::resolve`], which asks here.
/// The toolbar, the menu search and the shortcuts are made from the command table, where a planned line never is.
///
/// - What is distributed hides them: `scripts/dist-profile.sh` (and `bundle-windows.ps1`) set `IBIS_OFFICE_DIST=1` for the build, read
///   here at compile time (`scripts/check-dist-profile.sh` checks that they do). A development build (plain `cargo build --release`,
///   the scenarios) shows them.
/// - `IBIS_OFFICE_PLANNED=1` / `0` in the environment of the running app overrides it without a rebuild (a developer sees them in what is
///   distributed, or sees a development build as it will be released). Not a setting of the settings window.
pub(crate) fn show_planned() -> bool {
    if let Some(forced) = PLANNED_OVERRIDE.with(|o| o.get()) {
        return forced;
    }
    static FROM_ENV: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();
    let from_env = *FROM_ENV.get_or_init(|| match std::env::var("IBIS_OFFICE_PLANNED").ok().as_deref() {
        Some("1") => Some(true),
        Some("0") => Some(false),
        _ => None,
    });
    from_env.unwrap_or(!matches!(option_env!("IBIS_OFFICE_DIST"), Some("1")))
}

thread_local! {
    /// The tests' override of [`show_planned`] (both forms of the menus are checked in one process). Not a state of a window.
    static PLANNED_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Runs `f` with the planned lines shown or hidden, whatever the build and the environment say (for the tests of both forms).
#[allow(dead_code)] // only the tests and the scenarios call it
pub fn with_planned<R>(show: bool, f: impl FnOnce() -> R) -> R {
    let before = PLANNED_OVERRIDE.with(|o| o.replace(Some(show)));
    let out = f();
    PLANNED_OVERRIDE.with(|o| o.set(before));
    out
}

/// What a [`LayoutItem::Dynamic`] line stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicKind {
    /// The open documents, at the bottom of the Window menu.
    OpenWindows,
    /// The Window menu's 「サイドバー ▸」, made from the sidebar table (`sidebars.rs`).
    Sidebars,
}

/// A context menu of the layout (the coloured menus of the bar): its id and its lines.
pub struct ContextSpec {
    pub id: ContextMenu,
    pub items: &'static [LayoutItem],
}

/// The context menus of a screen (the ones it can show; which of them show now is `visible_contexts`).
pub fn layout_context(kind: Kind) -> &'static [ContextSpec] {
    crate::format::info(kind).menus.contexts
}

/// A screen's menus of the layout: the static menus in the order of the bar, and its context menus. A format names its screen's
/// in its `FormatInfo`; the tables themselves are generated into one file (`menu_layout_data.rs`).
pub struct ScreenMenus {
    pub menus: &'static [MenuSpec],
    pub contexts: &'static [ContextSpec],
}

/// The menus of the document screen, the spreadsheet screen and the presentation screen.
pub const DOCUMENT_MENUS: ScreenMenus = ScreenMenus { menus: super::menu_layout_data::DOC, contexts: super::menu_layout_data::DOC_CONTEXT };
pub const SHEET_MENUS: ScreenMenus = ScreenMenus { menus: super::menu_layout_data::SHEET, contexts: super::menu_layout_data::SHEET_CONTEXT };
pub const SLIDE_MENUS: ScreenMenus = ScreenMenus { menus: super::menu_layout_data::SLIDE, contexts: super::menu_layout_data::SLIDE_CONTEXT };

/// A menu that a button of the title row opens (not a menu of the bar): the same definition on the three screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TitleMenu {
    /// The Draw button's (the pens, the eraser, the ruler, converting ink, the Ink panel).
    Draw,
}

/// The lines of the Draw button's menu (appendix A, 「タイトル行の「描画」ボタンのメニュー」). The pens are one group; what a screen
/// cannot do is grey with its reason (`draw_menu.rs` makes the commands).
pub const DRAW_MENU: &[LayoutItem] = &[
    LayoutItem::Cmd("draw.select"),
    LayoutItem::Cmd("draw.lasso"),
    LayoutItem::Sub { id: None, label_key: "draw.pen", items: &[
        LayoutItem::Cmd("draw.pen.black"),
        LayoutItem::Cmd("draw.pen.pencil"),
        LayoutItem::Cmd("draw.pen.highlighter"),
        LayoutItem::Cmd("draw.pen.add"),
    ] },
    LayoutItem::Cmd("draw.eraser"),
    LayoutItem::Cmd("draw.ruler"),
    LayoutItem::Sep,
    LayoutItem::Cmd("draw.to-shape"),
    LayoutItem::Cmd("draw.to-math"),
    LayoutItem::Cmd("draw.canvas"),
    LayoutItem::Sep,
    LayoutItem::Cmd("draw.replay"),
    LayoutItem::Cmd("draw.hide-ink"),
    LayoutItem::Sep,
    LayoutItem::Cmd("draw.ink-panel"),
];

/// The right-click menu of a document's text (Word's: cut, copy, paste and the paste options, the font, the paragraph, the
/// bullets, the link and the comment). Its lines are commands of the table (`commands.rs`), so each runs and is greyed as
/// its menu bar item does. Not generated: it is not a menu of the bar.
pub(crate) const DOC_BODY_POPUP: &[LayoutItem] = &[
    LayoutItem::Cmd("edit.cut"),
    LayoutItem::Cmd("edit.copy"),
    LayoutItem::Cmd("edit.paste"),
    LayoutItem::Cmd("docx.paste-special"),
    LayoutItem::Sep,
    // (The font and paragraph lines are put here by `doc_body_popup_entries`, from `FormatEditor::body_popup_lines`: they are the
    // Home toolbar's buttons' only.)
    LayoutItem::Cmd("docx.bullet.toggle"),
    LayoutItem::Cmd("docx.list.toggle"),
    LayoutItem::Sep,
    LayoutItem::Cmd("insert.link"),
    LayoutItem::Cmd("insert.comment"),
    LayoutItem::Sep,
    LayoutItem::Cmd("edit.select-all"),
];

/// Added to [`DOC_BODY_POPUP`] when the caret is in a table (Word's table lines of the right-click menu).
pub(crate) const DOC_BODY_POPUP_TABLE: &[LayoutItem] = &[
    LayoutItem::Sep,
    LayoutItem::Sub { id: None, label_key: "insert.table", items: &[
        LayoutItem::Cmd("docx.table.row-above"),
        LayoutItem::Cmd("docx.table.row-below"),
        LayoutItem::Cmd("docx.table.column-left"),
        LayoutItem::Cmd("docx.table.column-right"),
        LayoutItem::Sep,
        LayoutItem::Cmd("docx.table.delete-row"),
        LayoutItem::Cmd("docx.table.delete-column"),
        LayoutItem::Cmd("docx.table.delete"),
        LayoutItem::Sep,
        LayoutItem::Cmd("docx.table.equal-columns"),
        LayoutItem::Cmd("docx.table.equal-rows"),
        LayoutItem::Cmd("docx.table.autofit"),
    ] },
    LayoutItem::Cmd("docx.table.select"),
];

/// Added to [`DOC_BODY_POPUP`] when a picture is selected.
pub(crate) const DOC_BODY_POPUP_PICTURE: &[LayoutItem] = &[
    LayoutItem::Sep,
    LayoutItem::Cmd("docx.picture.wrap"),
    LayoutItem::Cmd("docx.picture.alt-text"),
];

pub(crate) fn title_layout(menu: TitleMenu) -> &'static [LayoutItem] {
    match menu {
        TitleMenu::Draw => DRAW_MENU,
    }
}

/// What the menu bar shows now: the static menus, and the context menus of the selection, in the bar's order.
pub struct BarMenus {
    pub statics: Menus,
    pub contexts: Vec<(ContextMenu, Vec<MenuEntry>)>,
    /// The Draw button's menu.
    pub draw: Vec<MenuEntry>,
}

/// A menu of the layout: its id and its lines.
pub struct MenuSpec {
    pub id: MenuId,
    pub items: &'static [LayoutItem],
}

/// The static menus of a screen, in the order of the bar.
pub fn layout(kind: Kind) -> &'static [MenuSpec] {
    crate::format::info(kind).menus.menus
}

/// Commands the layout may show more than once (the redesign allows these: comment, cross-reference, page break; the sheet's protection is in
/// ファイル ▸ ブックの保護 and 校閲, as in Excel, 2026-11-11).
const ALLOWED_TWICE: &[&str] = &["pptx.anim.preview", "pptx.anim.options", "docx.page-break", "docx.xref", "docx.quick-parts.prop.comments", "insert.comment", "xlsx.protect-sheet"];

/// The ids the layout names (so that a submenu taken over whole does not also show them where it is not named).
fn named_ids(items: &[LayoutItem], out: &mut HashSet<&'static str>) {
    for item in items {
        match item {
            LayoutItem::Cmd(id) | LayoutItem::CmdAs { id, .. } | LayoutItem::CmdOrPlanned { id, .. } => {
                out.insert(id);
            }
            LayoutItem::Sub { id, items, .. } => {
                if let Some(id) = id {
                    out.insert(id);
                }
                named_ids(items, out);
            }
            LayoutItem::Sep | LayoutItem::Dynamic(_) | LayoutItem::Planned { .. } => {}
        }
    }
}

/// Ids named by the layout of a screen: its lines, and the commands that only open a sidebar (`LayoutItem::Dynamic(Sidebars)`
/// shows them under their sidebar ids, so no other menu takes them in whole).
fn named_ids_of_screen(kind: Kind, specs: &[MenuSpec]) -> HashSet<&'static str> {
    let mut named = HashSet::new();
    for spec in specs {
        named_ids(spec.items, &mut named);
    }
    named.extend(crate::sidebars::pane_only_commands(kind));
    named
}

fn entry_ids(entries: &[MenuEntry], out: &mut HashSet<&'static str>) {
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => {
                out.insert(item.id);
            }
            MenuEntry::Submenu(sub) => {
                out.insert(sub.id);
                entry_ids(&sub.items, out);
            }
            _ => {}
        }
    }
}

struct Resolver<'a> {
    reg: &'a CommandRegistry,
    named: HashSet<&'static str>,
    placed: HashSet<&'static str>,
    /// The lines of `LayoutItem::Dynamic(OpenWindows)`.
    open_windows: Vec<MenuEntry>,
    /// The lines of `LayoutItem::Dynamic(Sidebars)`.
    sidebars: Vec<MenuEntry>,
    /// The commands of the screen that only open a sidebar (placed by the sidebar lines).
    pane_commands: Vec<&'static str>,
}

impl Resolver<'_> {
    /// `entry` with the items that are already placed (or that the layout names elsewhere, when `skip_named`) left out.
    fn take(&mut self, entry: &MenuEntry, skip_named: bool, own: &HashSet<&'static str>) -> Option<MenuEntry> {
        match entry {
            MenuEntry::Item(item) => {
                // An item the layout puts in this very submenu is placed already: it stays.
                if own.contains(item.id) {
                    return Some(entry.clone());
                }
                if skip_named && self.named.contains(item.id) {
                    return None;
                }
                if self.placed.contains(item.id) && !ALLOWED_TWICE.contains(&item.id) {
                    return None;
                }
                self.placed.insert(item.id);
                Some(entry.clone())
            }
            MenuEntry::Submenu(sub) => {
                let mut kept = Vec::new();
                for inner in &sub.items {
                    // A submenu the layout names elsewhere is not taken whole into this one (its items would be placed here, and the line
                    // that names it would be left empty).
                    if let MenuEntry::Submenu(inner_sub) = inner
                        && self.named.contains(inner_sub.id)
                        && !own.contains(inner_sub.id)
                    {
                        continue;
                    }
                    if let Some(e) = self.take(inner, true, own) {
                        kept.push(e);
                    }
                }
                Some(MenuEntry::Submenu(Submenu { id: sub.id, icon: sub.icon, label: sub.label.clone(), items: kept, access: sub.access.clone() }))
            }
            other => Some(other.clone()),
        }
    }

    fn resolve(&mut self, items: &[LayoutItem]) -> Vec<MenuEntry> {
        let mut out = Vec::new();
        for item in items {
            match item {
                LayoutItem::Sep => out.push(MenuEntry::Separator),
                // A line that is not made yet: grey when planned lines are shown, no line at all when they are hidden (a submenu
                // or a separator that this leaves with nothing is taken out by `prune_entries` and by `Sub` below).
                LayoutItem::Planned { key } => {
                    if show_planned() {
                        out.push(MenuItem::new(key, i18n::text(key)).disabled(true).into());
                    }
                }
                LayoutItem::CmdOrPlanned { id, key } => match self.reg.entry(id) {
                    Some(entry) => {
                        if let Some(e) = self.take(entry, matches!(entry, MenuEntry::Submenu(_)), &HashSet::new()) {
                            out.push(e);
                        }
                    }
                    None => {
                        if show_planned() {
                            out.push(MenuItem::new(key, i18n::text(key)).disabled(true).into());
                        }
                    }
                },
                LayoutItem::Dynamic(DynamicKind::OpenWindows) => {
                    for entry in &self.open_windows {
                        if let MenuEntry::Item(item) = entry {
                            self.placed.insert(item.id);
                        }
                        out.push(entry.clone());
                    }
                }
                LayoutItem::Dynamic(DynamicKind::Sidebars) => {
                    if !self.sidebars.is_empty() {
                        for id in std::mem::take(&mut self.pane_commands) {
                            self.placed.insert(id);
                        }
                        out.push(Submenu { id: "menu.window.sidebar", icon: None, label: i18n::text("menu.window.sidebar"), items: self.sidebars.clone(), access: None }.into());
                    }
                }
                LayoutItem::Cmd(id) => {
                    if let Some(entry) = self.reg.entry(id) {
                        // A submenu named as a whole keeps all its items except those the layout names elsewhere.
                        if let Some(e) = self.take(entry, matches!(entry, MenuEntry::Submenu(_)), &HashSet::new()) {
                            out.push(e);
                        }
                    }
                }
                LayoutItem::CmdAs { id, label_key } => {
                    if let Some(entry) = self.reg.entry(id) {
                        match self.take(entry, matches!(entry, MenuEntry::Submenu(_)), &HashSet::new()) {
                            Some(MenuEntry::Item(mut item)) => {
                                item.label = (*label_key).into();
                                out.push(item.into());
                            }
                            Some(MenuEntry::Submenu(mut sub)) => {
                                sub.label = (*label_key).into();
                                out.push(sub.into());
                            }
                            Some(other) => out.push(other),
                            None => {}
                        }
                    }
                }
                LayoutItem::Sub { id, label_key, items } => {
                    let items_layout: &'static [LayoutItem] = *items;
                    let kids = self.resolve(items);
                    let has_kids = kids.iter().any(|e| !matches!(e, MenuEntry::Separator));
                    let old = id.and_then(|id| self.reg.entry(id));
                    match old {
                        Some(MenuEntry::Submenu(sub)) => {
                            let sub = sub.clone();
                            // The old submenu's items (all the commands it had), then the items the layout adds to it.
                            let mut own = HashSet::new();
                            entry_ids(&kids, &mut own);
                            self.placed.insert(sub.id);
                            let mut items = match self.take(&MenuEntry::Submenu(sub.clone()), true, &own) {
                                Some(MenuEntry::Submenu(s)) => s.items,
                                _ => Vec::new(),
                            };
                            let mut have = HashSet::new();
                            entry_ids(&items, &mut have);
                            items.extend(kids.into_iter().filter(|k| match k {
                                MenuEntry::Item(i) => !have.contains(i.id),
                                MenuEntry::Submenu(s) => !have.contains(s.id),
                                _ => false,
                            }));
                            // A `CmdAs` line of a command that is only inside the old submenu (not in the table on its own) is the old item: it takes
                            // the menu's words there.
                            for layout_item in items_layout {
                                if let LayoutItem::CmdAs { id: as_id, label_key: as_key } = layout_item {
                                    for entry in items.iter_mut() {
                                        if let MenuEntry::Item(item) = entry
                                            && item.id == *as_id
                                        {
                                            item.label = (*as_key).into();
                                        }
                                    }
                                }
                            }
                            let label = if *label_key == sub.id { sub.label } else { (*label_key).into() };
                            out.push(Submenu { id: sub.id, icon: sub.icon, label, items, access: None }.into());
                        }
                        Some(MenuEntry::Item(item)) => {
                            let item: MenuItem = item.clone();
                            if has_kids {
                                self.placed.insert(item.id);
                                let label = if *label_key == item.id { item.label } else { (*label_key).into() };
                                out.push(Submenu { id: item.id, icon: item.icon, label, items: kids, access: None }.into());
                            } else if let Some(e) = self.take(&MenuEntry::Item(item), false, &HashSet::new()) {
                                out.push(e);
                            }
                        }
                        _ => {
                            if has_kids {
                                out.push(Submenu { id: label_key, icon: None, label: i18n::text(label_key), items: kids, access: None }.into());
                            }
                        }
                    }
                }
            }
        }
        out
    }
}

impl MenuId {
    /// The access letter of a menu (Alt+letter on Windows and Linux; implementation-spec.md section 5). Not drawn or
    /// bound yet (the access keys are a later stage); the table is here so that the letters of a screen's bar are
    /// checked not to collide.
    pub fn access_letter(self) -> Option<char> {
        Some(match self {
            MenuId::File => 'f',
            MenuId::Edit => 'e',
            MenuId::Insert => 'n',
            MenuId::Design => 'g',
            MenuId::Layout | MenuId::PageLayout => 'p',
            MenuId::References | MenuId::SlideShow => 's',
            MenuId::Mailings | MenuId::Formulas => 'm',
            MenuId::Review => 'r',
            MenuId::View => 'w',
            MenuId::Window => 'o',
            MenuId::Help => 'h',
            MenuId::Data | MenuId::Animations => 'a',
            MenuId::Transitions => 'k',
            MenuId::Format | MenuId::Slide | MenuId::Arrange | MenuId::Tools => return None,
        })
    }
}

impl SlideEditor {
    /// The menus of the menu bar: the layout of the screen with the commands of the open document in it (empty
    /// submenus and menus are left out). `menus()` still has all the commands (shortcuts and the menu search use it).
    pub fn menu_bar_menus(&self) -> Menus {
        self.menu_bar_menus_and_registry().0
    }

    /// The menu bar's menus and the command table, in which the commands the bar does not show are `menu_hidden`.
    pub fn menu_bar_menus_and_registry(&self) -> (Menus, CommandRegistry) {
        let (bar, reg) = self.build_bar();
        (bar.statics, reg)
    }

    /// The lines of the menu a title row button opens.
    pub fn title_menu_entries(&self, menu: TitleMenu) -> Vec<MenuEntry> {
        match menu {
            TitleMenu::Draw => self.build_bar().0.draw,
        }
    }

    /// The lines of the right-click menu of a document's text, for what is under the caret (a table, a picture).
    pub fn doc_body_popup_entries(&self) -> Vec<MenuEntry> {
        let kind = self.kind();
        let reg = CommandRegistry::from_menus(&self.menus_unpruned(), kind);
        let state = self.context_state();
        let mut items: Vec<&'static LayoutItem> = DOC_BODY_POPUP.iter().collect();
        if state.caret_in_table {
            items.extend(DOC_BODY_POPUP_TABLE);
        }
        if state.objects.contains(&ObjectKind::Picture) {
            items.extend(DOC_BODY_POPUP_PICTURE);
        }
        let mut resolver = Resolver { reg: &reg, named: HashSet::new(), placed: HashSet::new(), open_windows: Vec::new(), sidebars: Vec::new(), pane_commands: Vec::new() };
        let mut entries = Vec::new();
        for item in items {
            entries.extend(resolver.resolve(std::slice::from_ref(item)));
        }
        // The screen's own lines after the first group (フォント… and 段落…: the panels the Home toolbar's font and line-spacing
        // menus reach).
        let at = entries.iter().position(|e| matches!(e, MenuEntry::Separator)).map_or(entries.len(), |i| i + 1);
        entries.splice(at..at, crate::format::with_format!(self, body_popup_lines(self)));
        prune_entries(entries)
    }

    /// The static menus and the context menus of the selection.
    pub fn menu_bar_all(&self) -> BarMenus {
        self.build_bar().0
    }

    /// The title row's Draw button: it opens the screen's Ink panel, as the Draw menu's 「インク パネル」 line does (the user,
    /// K0004, 2026-11-12). The Draw menu itself stays on the menu bar (its commands are in the menu search too).
    pub(crate) fn open_ink_panel_from_title(&mut self, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) {
        let (_, reg) = self.build_bar();
        let state = self.context_state();
        let handler = crate::sidebars::of(self.kind())
            .iter()
            .find(|d| d.id == "sidebar.ink")
            .and_then(|d| d.menu_item(self, &state, &reg).handler);
        if let Some(handler) = handler {
            handler(self, window, cx);
        }
    }

    fn build_bar(&self) -> (BarMenus, CommandRegistry) {
        let kind = self.kind();
        let source = self.menus_unpruned();
        let mut reg = CommandRegistry::from_menus(&source, kind);
        // The Draw button's commands (the title row's menu) are in the table too, so the menu search finds them.
        let draw_commands = self.draw_menu_commands(&reg);
        reg.add_entries(&draw_commands, kind);
        let specs = layout(kind);
        let named = named_ids_of_screen(kind, specs);
        let mut resolver = Resolver {
            reg: &reg,
            named,
            placed: HashSet::new(),
            open_windows: self.open_window_menu_items(),
            sidebars: self.sidebar_menu_entries(&reg),
            pane_commands: crate::sidebars::pane_only_commands(kind),
        };
        let mut menus = Menus::default();
        for spec in specs {
            let entries = menu_entries(&mut resolver, spec);
            let entries = prune_entries(entries);
            if !entries.is_empty() {
                menus.menus.push((spec.id, entries));
            }
        }
        let mut placed = resolver.placed;
        // The Draw button's menu.
        let mut draw_resolver = Resolver { reg: &reg, named: HashSet::new(), placed: HashSet::new(), open_windows: Vec::new(), sidebars: Vec::new(), pane_commands: Vec::new() };
        let draw = prune_entries(draw_resolver.resolve(title_layout(TitleMenu::Draw)));
        placed.extend(draw_resolver.placed);
        // The context menus of the selection. Each shows what it needs, whatever the static menus or another context
        // menu show, so its lines do not count as placed.
        let mut contexts = Vec::new();
        for context in visible_contexts(&self.context_state()) {
            let Some(spec) = layout_context(kind).iter().find(|s| s.id == context) else { continue };
            let mut named = HashSet::new();
            named_ids(spec.items, &mut named);
            let mut resolver = Resolver { reg: &reg, named, placed: HashSet::new(), open_windows: Vec::new(), sidebars: Vec::new(), pane_commands: Vec::new() };
            let mut entries = prune_entries(resolver.resolve(spec.items));
            if entries.is_empty() {
                entries.push(MenuItem::tr("menu.context.empty").disabled(true).into());
            }
            contexts.push((context, entries));
        }
        reg.mark_hidden(&placed);
        (BarMenus { statics: menus, contexts, draw }, reg)
    }
}

/// The lines of one menu. In the browser the Window menu has the sidebars and the full screen only (one document per
/// browser tab, so no windows to switch between).
fn menu_entries(resolver: &mut Resolver, spec: &MenuSpec) -> Vec<MenuEntry> {
    #[cfg(target_family = "wasm")]
    if spec.id == MenuId::Window {
        let web: Vec<&LayoutItem> = spec
            .items
            .iter()
            .filter(|i| matches!(i, LayoutItem::Cmd("view.fullscreen")) || matches!(i, LayoutItem::Dynamic(DynamicKind::Sidebars)))
            .collect();
        let mut out = Vec::new();
        for item in web {
            out.extend(resolver.resolve(std::slice::from_ref(item)));
        }
        return out;
    }
    resolver.resolve(spec.items)
}

/// Where each command is in the menu bar, as the words the menu search shows beside it: 「校閲 ▸ 検索と置換」 (the menu, then
/// the submenus). A command of a submenu is under the submenus' names; one directly in a menu is under the menu's.
/// `words` gets the words a line shows when they are not its command's own (`LayoutItem::CmdAs`), so that the search finds it by them too.
fn locate(
    entries: &[MenuEntry],
    path: &str,
    out: &mut std::collections::HashMap<&'static str, String>,
    words: &mut Vec<(&'static str, gpui_kit::SharedString, String)>,
) {
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => {
                out.entry(item.id).or_insert_with(|| path.to_string());
                words.push((item.id, item.label.clone(), path.to_string()));
            }
            MenuEntry::Submenu(sub) => {
                let name = i18n::text(&sub.label);
                locate(&sub.items, &format!("{path} ▸ {name}"), out, words);
            }
            _ => {}
        }
    }
}

impl SlideEditor {
    /// What the menu search lists: every command of the table (the ones the menu bar shows, the context menus' that
    /// are showing now, the Home commands of the toolbar and whatever else the layout does not show), and the sidebars, each with where it is
    /// (`MenuItem::note`) so that a command that left the menus (bold, line spacing) is found and says where it lives now. Sorted by name.
    pub fn command_search_items(&self) -> Vec<MenuItem> {
        use std::collections::HashMap;
        let (bar, reg) = self.build_bar();
        let kind = self.kind();
        let mut places: HashMap<&'static str, String> = HashMap::new();
        let mut words = Vec::new();
        for (menu, entries) in &bar.statics.menus {
            locate(entries, &i18n::text(menu.key()), &mut places, &mut words);
        }
        for (context, entries) in &bar.contexts {
            locate(entries, &i18n::text(context.key()), &mut places, &mut words);
        }
        locate(&bar.draw, &i18n::text("menu.draw"), &mut places, &mut words);
        let sidebar_place = format!("{} ▸ {}", i18n::text(MenuId::Window.key()), i18n::text("menu.window.sidebar"));
        for id in crate::sidebars::pane_only_commands(kind) {
            places.insert(id, sidebar_place.clone());
        }
        let toolbar_ids: HashSet<&'static str> = self
            .toolbar_items()
            .into_iter()
            .filter_map(|t| match t {
                crate::chrome::ToolItem::Button(b) => Some(b.id),
                _ => None,
            })
            .collect();
        let mut items: Vec<MenuItem> = Vec::new();
        for command in &reg.commands {
            if command.id == "help.search" || command.id.starts_with("font.section.") {
                continue;
            }
            let Some(MenuEntry::Item(item)) = reg.entry(command.id) else { continue };
            if item.handler.is_none() || item.disabled {
                continue;
            }
            let mut found = item.clone();
            found.label = crate::chrome::menu_item_display_label(&found);
            if found.label.is_empty() {
                continue;
            }
            let place = match places.get(command.id) {
                Some(place) => place.clone(),
                None if toolbar_ids.contains(command.id) || crate::commands::HOME_GROUPS.iter().any(|(_, ids)| ids.iter().any(|i| i.trim_start_matches('*') == command.id)) => {
                    i18n::text("menu.search.where-toolbar").to_string()
                }
                None => i18n::text("menu.search.where-other").to_string(),
            };
            found.note = Some(place.into());
            // A menu line that shows the command under the menu's own words (`LayoutItem::CmdAs`) is listed under those words, with
            // that line's place: the words the user sees in the menu bar. One row per command (the same command was listed twice,
            // once under its own words and once under the menu's, with the same place).
            let mut alias = None;
            for (id, raw, path) in &words {
                // (The bar's line carries the key of its words; a line of the command's own words has the command's label.)
                // Only a key of the tables is looked up (a `CmdAs` line's words: `menu.as.…` from the generator, or a hand-named one
                // such as `review.new-comment`): other lines carry words already shown or a document's name (recent files).
                if *id != command.id || *raw == item.label || !i18n::is_known_id(raw) {
                    continue;
                }
                let shown = i18n::text(raw);
                if shown != found.label {
                    let mut as_shown = found.clone();
                    as_shown.label = shown;
                    as_shown.access = None;
                    as_shown.search_alt = Some(found.label.clone());
                    as_shown.note = Some(path.clone().into());
                    alias = Some(as_shown);
                    break;
                }
            }
            items.push(alias.unwrap_or(found));
        }
        // The sidebars, under their own names (an operation that is also in a menu keeps that one: the same name is not listed twice).
        let state = self.context_state();
        for def in crate::sidebars::of(kind) {
            let row = def.menu_item(self, &state, &reg);
            if row.handler.is_none() || row.disabled || items.iter().any(|i| i.label == row.label) {
                continue;
            }
            items.push(row.note(sidebar_place.clone()));
        }
        items.sort_by(|a, b| a.label.as_ref().cmp(b.label.as_ref()));
        items.dedup_by(|a, b| a.label == b.label && a.id == b.id);
        items
    }
}

#[cfg(test)]
mod planned_tests {
    use std::collections::HashSet;

    use super::{LayoutItem, Resolver};
    use crate::chrome::{MenuEntry, Menus};
    use crate::commands::CommandRegistry;
    use ibis_pptx::Kind;

    /// A `Planned` line is a grey item with the words of its key, and it takes no command's place.
    #[test]
    fn a_planned_line_is_a_grey_item_of_its_key() {
        let reg = CommandRegistry::from_menus(&Menus::default(), Kind::Presentation);
        let mut resolver = Resolver { reg: &reg, named: HashSet::new(), placed: HashSet::new(), open_windows: Vec::new(), sidebars: Vec::new(), pane_commands: Vec::new() };
        let layout = [LayoutItem::Planned { key: "menu.planned.slide.animations.1" }, LayoutItem::Sep, LayoutItem::Planned { key: "menu.planned.slide.animations.2" }];
        let out = super::with_planned(true, || resolver.resolve(&layout));
        assert_eq!(out.len(), 3);
        for (entry, id) in [(&out[0], "menu.planned.slide.animations.1"), (&out[2], "menu.planned.slide.animations.2")] {
            let MenuEntry::Item(item) = entry else { panic!("an item") };
            assert_eq!(item.id, id);
            assert!(!item.enabled(), "a planned line is grey");
            assert!(item.handler.is_none(), "nothing behind it");
        }
    }

    /// Hidden (what is distributed): no planned line, a group that had only planned lines is gone, and no separator is left alone.
    #[test]
    fn hidden_planned_lines_leave_no_line_no_empty_group_and_no_lone_separator() {
        let reg = CommandRegistry::from_menus(&Menus::default(), Kind::Presentation);
        let mut resolver = Resolver { reg: &reg, named: HashSet::new(), placed: HashSet::new(), open_windows: Vec::new(), sidebars: Vec::new(), pane_commands: Vec::new() };
        const GROUP: &[LayoutItem] = &[LayoutItem::Planned { key: "menu.planned.slide.animations.1" }, LayoutItem::Sep, LayoutItem::CmdOrPlanned { id: "no.such.command", key: "menu.planned.slide.animations.2" }];
        let layout = [LayoutItem::Planned { key: "menu.planned.slide.animations.1" }, LayoutItem::Sep, LayoutItem::Sub { id: None, label_key: "review.comments", items: GROUP }, LayoutItem::Sep];
        let shown = super::with_planned(true, || crate::chrome::prune_entries(resolver.resolve(&layout)));
        assert_eq!(shown.len(), 3, "shown: the line, the separator and the group");
        let hidden = super::with_planned(false, || crate::chrome::prune_entries(resolver.resolve(&layout)));
        assert!(hidden.is_empty(), "hidden: nothing is left ({} entries)", hidden.len());
    }

    /// The switch: the override of the tests wins, and it is put back.
    #[test]
    fn the_switch_is_one_function() {
        let before = super::show_planned();
        assert!(super::with_planned(true, super::show_planned));
        assert!(!super::with_planned(false, super::show_planned));
        assert_eq!(super::show_planned(), before);
    }

    /// Without the tests' override: `IBIS_OFFICE_PLANNED` of the environment decides when it is 1 or 0, else the build does (a build
    /// with `IBIS_OFFICE_DIST=1`, what is distributed, hides the lines). Run with each to see each (`scripts/check-dist-profile.sh`
    /// says how).
    #[test]
    fn the_environment_then_the_build_decide() {
        let dist = matches!(option_env!("IBIS_OFFICE_DIST"), Some("1"));
        let expected = match std::env::var("IBIS_OFFICE_PLANNED").ok().as_deref() {
            Some("1") => true,
            Some("0") => false,
            _ => !dist,
        };
        assert_eq!(super::show_planned(), expected, "dist build: {dist}");
    }
}
