// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The sidebar table (menu bar redesign, stage 7; `tasks/doc/menu-redesign/implementation-spec.md` section 3 and
//! `sidebars.md`): what a row of the table says, and what is made from a screen's table. Each screen has its own table
//! (`docx/sidebars.rs`, `xlsx/sidebars.rs`, `pptx/sidebars.rs`; `FormatInfo::sidebars`): the one place that says which
//! sidebars (the panes that stay open beside or under the page) the screen has. The Window menu's 「サイドバー ▸」, the menu
//! search and the sidebar commands are made from it, so a sidebar added there shows in all of them and nobody has to remember
//! each place. This file names no screen: a sidebar two screens have is a row with the same id in each table.
//!
//! A [`SidebarDef`] does not open its pane by itself: it names the screen's command that already does (`cmd`), or a
//! function (`open`) for a pane that has no command (`by_format`: the screen opens it, `FormatEditor::open_sidebar`). The row of the menu is that command under the sidebar's own id
//! (`sidebar.*`): its handler, its check mark and its shortcut. A pane that opens by itself when something is selected (a
//! picture's) has neither, and its row is grey with the words 「選択すると自動で開きます」. Whether the row can be used now is
//! decided from the selection as data ([`ContextState`]), and a row that cannot be used is grey with the reason at its right.
//!
//! `panels` are the names of the panel functions of the screen's pane chain (`fn sidebar` of the screen) that show the pane: the
//! test `sidebars_cover_pane_chain` reads the chains' source and fails for a resident panel that no definition names.
use ibis_pptx::Kind;

use crate::chrome::{Handler, MenuEntry, MenuItem};
use crate::commands::CommandRegistry;
use crate::editor::SlideEditor;
use crate::i18n;
use crate::menu_context::{ContextState, ObjectKind};

/// `Pane` is resident (the Window menu lists it); `Task` is a settings dialog that happens to be shown in the pane
/// (its own command opens it; it is in the table so that the chain is covered, and the menu leaves it out).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarKind {
    Pane,
    Task,
}

/// Where the pane is. The right pane shows one at a time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidebarSlot {
    Left,
    Bottom,
    Right,
}

/// The order within a slot (the menu puts a separator between groups).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SidebarGroup {
    /// Format and objects.
    Object,
    Document,
    Review,
    /// Drawing and settings.
    Tools,
}

/// Whether a sidebar can be used with this selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Availability {
    Enabled,
    /// The words (key of the table of words) that say what is needed.
    Disabled(&'static str),
}

type Available = fn(&ContextState) -> Availability;
/// Given the definition's id.
type Open = fn(&mut SlideEditor, &'static str, &mut gpui_kit::Window, &mut gpui_kit::Context<SlideEditor>);

pub struct SidebarDef {
    /// Also the id of the command that stands for it in the Window menu and the menu search.
    pub id: &'static str,
    pub label_key: &'static str,
    pub slot: SidebarSlot,
    pub group: SidebarGroup,
    pub kind: SidebarKind,
    /// The panel functions of the screen's pane chain that show it.
    pub panels: &'static [&'static str],
    /// The command the screen already has that opens (or toggles) it.
    pub cmd: Option<&'static str>,
    /// How to open it when there is no such command (the screen decides inside).
    pub open: Option<Open>,
    /// It starts an operation (spelling, find…): the command stays in its own menu too, and the sidebar row is another name for it.
    pub start_command: bool,
    pub available: Available,
    pub shortcut: Option<&'static str>,
    /// The id of the title row's button that stands for it (comments, the sidebar toggle), the one the title row makes
    /// from this table.
    #[allow(dead_code)] // Read by the scenario `sidebars_toolbar_buttons` (the title row's buttons are still made by `render_title_row`; the field says which sidebar each stands for).
    pub toolbar: Option<&'static str>,
}

// What a row needs of the selection (`SidebarDef::available`).
pub fn always(_: &ContextState) -> Availability {
    Availability::Enabled
}
pub fn picture(s: &ContextState) -> Availability {
    if s.objects.contains(&ObjectKind::Picture) { Availability::Enabled } else { Availability::Disabled("sidebar.reason.picture") }
}
pub fn shape(s: &ContextState) -> Availability {
    if s.objects.contains(&ObjectKind::Shape) { Availability::Enabled } else { Availability::Disabled("sidebar.reason.shape") }
}
pub fn frame(s: &ContextState) -> Availability {
    if s.objects.iter().any(|o| matches!(o, ObjectKind::Shape | ObjectKind::Picture)) { Availability::Enabled } else { Availability::Disabled("sidebar.reason.frame") }
}
pub fn chart(s: &ContextState) -> Availability {
    if s.objects.contains(&ObjectKind::Chart) { Availability::Enabled } else { Availability::Disabled("sidebar.reason.chart") }
}
pub fn sheet_table(s: &ContextState) -> Availability {
    if s.in_sheet_table { Availability::Enabled } else { Availability::Disabled("sidebar.reason.sheet-table") }
}
pub fn pivot(s: &ContextState) -> Availability {
    if s.in_pivot { Availability::Enabled } else { Availability::Disabled("sidebar.reason.pivot") }
}
pub fn outline(s: &ContextState) -> Availability {
    if s.outline_view { Availability::Enabled } else { Availability::Disabled("sidebar.reason.outline") }
}
pub fn smartart(s: &ContextState) -> Availability {
    if s.objects.contains(&ObjectKind::SmartArt) { Availability::Enabled } else { Availability::Disabled("sidebar.reason.smartart") }
}
pub fn master(s: &ContextState) -> Availability {
    if s.master_view { Availability::Enabled } else { Availability::Disabled("sidebar.reason.master") }
}

/// The screen opens the pane itself (`FormatEditor::open_sidebar`): a pane without a command on that screen.
pub fn by_format(this: &mut SlideEditor, id: &'static str, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<SlideEditor>) {
    crate::format::with_format!(this, open_sidebar(this, id, window, cx));
}
/// The comments pane is the one the title row's コメントを表示 button toggles (each screen's own).
pub fn open_comments(this: &mut SlideEditor, _: &'static str, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<SlideEditor>) {
    if let Some((_, handler)) = crate::format::with_format!(this, show_comments(this)) {
        handler(this, window, cx);
    }
}

use SidebarSlot::{Bottom, Left, Right};

/// A row of a screen's table, on one line.
// Exported for the formats (ibis-office-app); named from here, `crate::sidebars::sidebar_def`, through the `pub use` below.
#[macro_export]
#[doc(hidden)]
macro_rules! sidebar_def {
    ($id:literal, $label:literal, $slot:expr, $group:expr, $kind:expr, panels: $panels:expr, cmd: $cmd:expr, open: $open:expr, start: $start:expr, available: $avail:expr, key: $key:expr, toolbar: $toolbar:expr) => {
        $crate::sidebars::SidebarDef { id: $id, label_key: $label, slot: $slot, group: $group, kind: $kind, panels: $panels, cmd: $cmd, open: $open, start_command: $start, available: $avail, shortcut: $key, toolbar: $toolbar }
    };
}
pub use crate::sidebar_def;

/// The sidebars of the screen `kind`, in the order of its table (`FormatInfo::sidebars`).
pub fn of(kind: Kind) -> &'static [SidebarDef] {
    crate::format::info(kind).sidebars
}

impl SidebarDef {
    /// The pane is one of the Window menu's rows.
    pub fn listed(&self) -> bool {
        self.kind == SidebarKind::Pane
    }

    /// The menu row that stands for the sidebar, made from the command that opens it (its handler, check mark and
    /// shortcut) under the sidebar's own id.
    pub(crate) fn menu_item(&self, editor: &SlideEditor, state: &ContextState, reg: &CommandRegistry) -> MenuItem {
        let base = self.cmd.and_then(|id| reg.entry(id)).and_then(|e| match e {
            MenuEntry::Item(item) => Some(item),
            _ => None,
        });
        let mut item = MenuItem::new(self.id, i18n::text(self.label_key));
        if let Some(base) = base {
            item.icon = base.icon;
            item.checked = base.checked;
            item.disabled = base.disabled;
            item.shortcut = base.shortcut;
            item.handler = base.handler.clone();
        }
        if item.handler.is_none() {
            if let Some(open) = self.open {
                let id = self.id;
                let handler: Handler = std::rc::Rc::new(move |this, window, cx| open(this, id, window, cx));
                item.handler = Some(handler);
            }
        }
        if item.shortcut.is_none() {
            item.shortcut = self.shortcut;
        }
        if self.id == "sidebar.comments" {
            item.checked = crate::format::with_format!(editor, show_comments(editor)).is_some_and(|(shown, _)| shown);
        }
        match (self.available)(state) {
            Availability::Disabled(reason) => {
                item.disabled = true;
                item.hint = Some(i18n::text(reason));
            }
            Availability::Enabled => {
                if item.handler.is_none() {
                    // It opens by itself (a pane that follows the selection) or is always there: nothing to run.
                    item.disabled = true;
                    item.hint = Some(i18n::text("sidebar.reason.automatic"));
                } else if item.disabled {
                    item.hint = Some(i18n::text("sidebar.reason.unavailable"));
                }
            }
        }
        item
    }
}

impl SlideEditor {
    /// The right pane's button / 表示 > サイドバー: show or hide the pane. What it is for is the format's choice
    /// (`FormatEditor::sidebar_toggled`).
    pub fn toggle_sidebar(&mut self, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) {
        self.sidebar_open = !self.sidebar_open;
        crate::format::with_format!(self, sidebar_toggled(self));
        cx.notify();
        window.refresh();
    }

    /// The lines of 「サイドバー ▸」 of the Window menu for this screen: the sidebar toggle, then the left, bottom and right
    /// panes, each under its heading, in the table's order, a separator between groups.
    pub(crate) fn sidebar_menu_entries(&self, reg: &CommandRegistry) -> Vec<MenuEntry> {
        let kind = self.kind();
        let state = self.context_state();
        let mut out: Vec<MenuEntry> = Vec::new();
        // The toggle of the right pane: a document has none (its line was grey for ever; the user took it out of the menu, 2026-11-10).
        if crate::format::driver(kind).sidebar_on_request()
            && let Some(entry @ MenuEntry::Item(_)) = reg.entry("view.sidebar")
        {
            out.push(entry.clone());
        }
        for (slot, heading) in [(Left, "sidebar.heading.left"), (Bottom, "sidebar.heading.bottom"), (Right, "sidebar.heading.right")] {
            let mut defs: Vec<&SidebarDef> = of(kind).iter().filter(|d| d.slot == slot && d.listed()).collect();
            if defs.is_empty() {
                continue;
            }
            defs.sort_by_key(|d| d.group);
            out.push(MenuEntry::Separator);
            out.push(MenuItem::new(heading_id(heading), i18n::text(heading)).disabled(true).into());
            let mut group = defs[0].group;
            for def in defs {
                if def.group != group {
                    out.push(MenuEntry::Separator);
                    group = def.group;
                }
                out.push(def.menu_item(self, &state, reg).into());
            }
        }
        out
    }
}

fn heading_id(key: &'static str) -> &'static str {
    key
}

/// The ids of the commands that only open a sidebar on `kind` (they are shown under 「サイドバー ▸」 and nowhere else).
pub fn pane_only_commands(kind: Kind) -> Vec<&'static str> {
    // `view.sidebar` (the toggle of the right pane) is the first line of the submenu.
    let mut ids: Vec<&'static str> = vec!["view.sidebar"];
    ids.extend(of(kind).iter().filter(|d| d.listed() && !d.start_command).filter_map(|d| d.cmd));
    ids
}
