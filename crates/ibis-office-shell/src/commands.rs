// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The command table (menu bar redesign, stage 1; `tasks/doc/menu-redesign/implementation-spec.md` section 3).
//!
//! One [`Command`] is one thing the user can run from a menu, the menu search or a shortcut: its id (the id the
//! menus have always used), its words, icon, shortcut, whether it can run now, whether it is checked, and what it
//! does. The table is made for the open document from the commands the shell and the format define
//! (`SlideEditor::menus_unpruned`: the screens' own menu builders and the toolbar's buttons of the same operations),
//! so a command is defined once and the menu bar's layout (`menu_layout.rs`) only says where it is shown. A command the
//! layout does not show is `menu_hidden`: it stays in the table, so the menu search and its shortcut still reach it.
use std::collections::{HashMap, HashSet};

use gpui_kit::SharedString;
use ibis_pptx::Kind;

use crate::chrome::{Handler, MenuEntry, MenuItem, Menus, Submenu, ToolButton, ToolItem};
use crate::i18n;

/// The screens a command can be used on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KindMask(u8);

#[allow(dead_code)] // `contains` is for the stages that filter the table by screen (search, window list).
impl KindMask {
    pub const DOCX: KindMask = KindMask(1);
    pub const XLSX: KindMask = KindMask(2);
    pub const PPTX: KindMask = KindMask(4);

    pub(crate) const fn union(a: KindMask, b: KindMask) -> KindMask {
        KindMask(a.0 | b.0)
    }

    pub(crate) fn of(kind: Kind) -> KindMask {
        crate::format::info(kind).mask
    }

    pub(crate) fn contains(self, kind: Kind) -> bool {
        self.0 & Self::of(kind).0 != 0
    }
}

/// A command of the open document.
#[derive(Clone)]
#[allow(dead_code)] // The table's fields are read by the stages that come next (search index, window list, shortcuts); the layout reads `id`, `enabled`, `run`.
pub struct Command {
    /// The id the menus, the scenarios and the tables of words use, such as `"file.save"`.
    pub id: &'static str,
    pub label: SharedString,
    pub icon: Option<&'static str>,
    /// The shortcut shown, in macOS's notation (`"⌘S"`).
    pub shortcut: Option<&'static str>,
    /// It can run now (it has something to do and no dialog is in the way).
    pub enabled: bool,
    pub checked: bool,
    pub run: Option<Handler>,
    pub kinds: KindMask,
    /// Not in the menu bar: the menu search and the shortcut reach it (the Home commands of the toolbar, and
    /// whatever the menu layout does not show).
    pub menu_hidden: bool,
}

/// The commands of the open document, and the menu entries (an item, or a submenu with its items) they came from.
pub struct CommandRegistry {
    pub commands: Vec<Command>,
    entries: HashMap<&'static str, MenuEntry>,
    /// Ids that more than one command definition has (the first is kept; the menus have a few such).
    pub duplicates: Vec<&'static str>,
}

impl CommandRegistry {
    /// The table made of `menus` (what `SlideEditor::menus_unpruned` built), for a document of `kind`.
    pub(crate) fn from_menus(menus: &Menus, kind: Kind) -> Self {
        let mut reg = CommandRegistry { commands: Vec::new(), entries: HashMap::new(), duplicates: Vec::new() };
        for (_, entries) in &menus.menus {
            reg.add_all(entries, kind);
        }
        reg
    }

    /// More commands (the title row's menus make theirs: `draw_menu.rs`).
    pub(crate) fn add_entries(&mut self, entries: &[MenuEntry], kind: Kind) {
        self.add_all(entries, kind);
    }

    fn add_all(&mut self, entries: &[MenuEntry], kind: Kind) {
        for entry in entries {
            match entry {
                MenuEntry::Item(item) => self.add_item(item, entry, kind),
                MenuEntry::Submenu(sub) => {
                    self.entries.entry(sub.id).or_insert_with(|| entry.clone());
                    self.add_all(&sub.items, kind);
                }
                // A palette's squares are not commands of their own; a grid of pictures is made only when a menu is filled.
                MenuEntry::Palette(_) | MenuEntry::Tiles(_) => {}
                MenuEntry::Separator => {}
            }
        }
    }

    fn add_item(&mut self, item: &MenuItem, entry: &MenuEntry, kind: Kind) {
        if self.entries.contains_key(item.id) {
            if !self.duplicates.contains(&item.id) {
                self.duplicates.push(item.id);
            }
            return;
        }
        self.entries.insert(item.id, entry.clone());
        self.commands.push(Command {
            id: item.id,
            label: item.label.clone(),
            icon: item.icon,
            shortcut: item.shortcut,
            enabled: item.enabled(),
            checked: item.checked,
            run: item.handler.clone(),
            kinds: KindMask::of(kind),
            menu_hidden: false,
        });
    }

    /// The menu entry (item or submenu) of command `id`.
    pub fn entry(&self, id: &str) -> Option<&MenuEntry> {
        self.entries.get(id)
    }

    /// Mark the commands that are not in `shown` as `menu_hidden`.
    pub(crate) fn mark_hidden(&mut self, shown: &HashSet<&'static str>) {
        for command in &mut self.commands {
            command.menu_hidden = !shown.contains(command.id);
        }
    }

    pub fn hidden(&self) -> impl Iterator<Item = &Command> {
        self.commands.iter().filter(|c| c.menu_hidden)
    }
}

// ---- The toolbar's Home commands (stage 6; this was `menu_bridge.rs`) ----

/// The Home commands of the toolbar (bold, alignment, line spacing, lists, borders), by the old Format submenu they were
/// in: (submenu id, the toolbar buttons it takes, in order; a `*` before an id
/// means the button's own menu is put in the submenu instead of the button).
pub(crate) const HOME_GROUPS: &[(&str, &[&str])] = &[
    ("format.text", &["bold", "italic", "underline", "strikethrough", "text-shadow", "case"]),
    ("format.align", &["*align", "*halign", "valign", "vertical-align", "text-direction", "wrap", "merge", "indent-less", "indent-more"]),
    ("format.spacing", &["*line-spacing"]),
    ("format.lists", &["checklist", "bullets", "bullet-types", "numbered", "number-types"]),
    ("format.borders", &["*borders", "border-color", "line-weight", "line-style"]),
];

fn disable_all(entries: &mut Vec<MenuEntry>) {
    entries.retain(|e| !matches!(e, MenuEntry::Palette(_)));
    for entry in entries {
        match entry {
            MenuEntry::Item(item) => item.disabled = true,
            MenuEntry::Submenu(sub) => disable_all(&mut sub.items),
            _ => {}
        }
    }
}

/// The entries that stand for `button`: its menu itself (`flatten`), a
/// submenu of its menu, or an item.
fn entries_of(button: &ToolButton, flatten: bool) -> Vec<MenuEntry> {
    let label = i18n::text(&button.tooltip);
    // A split button with the plain action of its own (the toggle of bullets): that action, not its menu.
    if !flatten && button.left_action().is_some() && button.handler.is_some() {
        let mut item = MenuItem::new(button.id, label).checked(button.on).disabled(button.disabled);
        item.icon = button.icon;
        item.handler = button.handler.clone();
        return vec![item.into()];
    }
    let menu = button.menu.clone().filter(|m| !m.is_empty());
    if let Some(mut menu) = menu {
        if flatten {
            if button.disabled {
                disable_all(&mut menu);
            }
            return menu;
        }
        if button.disabled {
            // A grey menu button is no command (it has no action of its own to run).
            return Vec::new();
        }
        return vec![Submenu::new(button.id, button.icon.unwrap_or("tune"), label, menu).into()];
    }
    // A button that does nothing (no handler, no menu) is not a command.
    if button.handler.is_none() {
        return Vec::new();
    }
    let mut item = MenuItem::new(button.id, label).checked(button.on).disabled(button.disabled);
    item.icon = button.icon;
    item.handler = button.handler.clone();
    vec![item.into()]
}

/// The toolbar's Home commands, put in the table of commands (menu bar redesign, stage 6): the format menu's empty submenus
/// that have buttons in `tools` are filled from them, so each button's operation is a command of its own (its handler, its
/// grey state, its check mark). The menu bar's layout does not show them (they are `menu_hidden`: the toolbar, the mini toolbar,
/// the right pane and the shortcuts are their places) and the menu search lists them (its note says "ホーム系 ▸ ツールバー").
pub(crate) fn fill_home_commands(menus: &mut Menus, tools: impl FnOnce() -> Vec<ToolItem>) {
    let present = |menus: &Menus, id: &str| matches!(menus.find_entry(id), Some(MenuEntry::Submenu(_)));
    if !HOME_GROUPS.iter().any(|(id, _)| present(menus, id)) {
        return;
    }
    let buttons: Vec<ToolButton> = tools()
        .into_iter()
        .filter_map(|t| match t {
            ToolItem::Button(b) => Some(b),
            _ => None,
        })
        .collect();
    for (id, wanted) in HOME_GROUPS {
        if !present(menus, id) {
            continue;
        }
        let mut entries: Vec<MenuEntry> = Vec::new();
        for want in *wanted {
            let (flatten, name) = want.strip_prefix('*').map_or((false, *want), |n| (true, n));
            if let Some(button) = buttons.iter().find(|b| b.id == name) {
                if !entries.is_empty() && flatten {
                    entries.push(MenuEntry::Separator);
                }
                entries.extend(entries_of(button, flatten));
            }
        }
        // Not an item the menus already have (the format's own, with its own state).
        let entries = without_known(entries, menus);
        if entries.is_empty() {
            continue;
        }
        for (_, list) in &mut menus.menus {
            if put(list, id, &entries) {
                break;
            }
        }
    }
}

fn put(list: &mut [MenuEntry], id: &str, entries: &[MenuEntry]) -> bool {
    for entry in list {
        if let MenuEntry::Submenu(sub) = entry {
            if sub.id == id {
                // After what the format put in the group itself (a separator between), or alone when it is empty.
                if sub.items.iter().all(|e| matches!(e, MenuEntry::Separator)) {
                    sub.items = entries.to_vec();
                } else {
                    // Not a command the group has under the same name (the format's own, with another id).
                    let have: Vec<SharedString> = sub.items.iter().filter_map(|e| match e {
                        MenuEntry::Item(i) => Some(i.label.clone()),
                        MenuEntry::Submenu(s) => Some(s.label.clone()),
                        _ => None,
                    }).collect();
                    let fresh: Vec<MenuEntry> = entries.iter().filter(|e| match e {
                        MenuEntry::Item(i) => !have.contains(&i.label),
                        MenuEntry::Submenu(s) => !have.contains(&s.label),
                        _ => true,
                    }).cloned().collect();
                    if !fresh.is_empty() {
                        sub.items.push(MenuEntry::Separator);
                        sub.items.extend(fresh);
                    }
                }
                return true;
            }
            if put(&mut sub.items, id, entries) {
                return true;
            }
        }
    }
    false
}

/// `entries` without the items whose ids `menus` already has.
fn without_known(entries: Vec<MenuEntry>, menus: &Menus) -> Vec<MenuEntry> {
    entries
        .into_iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item(item) if menus.find_entry(item.id).is_some() => None,
            MenuEntry::Submenu(mut sub) => {
                sub.items = without_known(std::mem::take(&mut sub.items), menus);
                (!sub.items.is_empty()).then_some(MenuEntry::Submenu(sub))
            }
            other => Some(other),
        })
        .collect()
}
