// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A workbook's sidebars: the screen's rows of the sidebar table (`FormatInfo::sidebars`; what a row says is in `shell/sidebars.rs`,
//! the list the rows are made from is `tasks/doc/menu-redesign/sidebars.md`). The Window menu's 「サイドバー ▸」 lists them in
//! this order. `panels` are the panel functions of this screen's pane chain (`fn sidebar` in `mod.rs`).
use crate::shell::sidebars::SidebarGroup as G;
use crate::shell::sidebars::SidebarKind::{Pane, Task};
use crate::shell::sidebars::SidebarSlot::Right;
use crate::shell::sidebars::{SidebarDef, always, by_format, chart, frame, open_comments, pivot, sheet_table, sidebar_def};

pub(crate) const SIDEBARS: &[SidebarDef] = &[
    // Right pane: format and objects.
    sidebar_def!("sidebar.cell-format", "sidebar.cell-format", Right, G::Object, Pane, panels: &["custom_format_panel"], cmd: Some("format.options"), open: None, start: false, available: always, key: None, toolbar: Some("sidebar")),
    sidebar_def!("sidebar.chart", "sidebar.chart", Right, G::Object, Pane, panels: &["sheet_chart_panel"], cmd: Some("xlsx.chart-format"), open: None, start: false, available: chart, key: None, toolbar: None),
    sidebar_def!("sidebar.frame", "sidebar.frame", Right, G::Object, Pane, panels: &[], cmd: Some("xlsx.frame-settings"), open: None, start: false, available: frame, key: None, toolbar: None),
    sidebar_def!("sidebar.table-design", "sidebar.table-design", Right, G::Object, Pane, panels: &[], cmd: Some("xlsx.table-design"), open: None, start: false, available: sheet_table, key: None, toolbar: None),
    sidebar_def!("sidebar.pivot-fields", "sidebar.pivot-fields", Right, G::Object, Pane, panels: &[], cmd: Some("xlsx.pivot-fields"), open: None, start: false, available: pivot, key: None, toolbar: None),
    sidebar_def!("sidebar.header-footer", "sidebar.header-footer", Right, G::Object, Pane, panels: &["header_footer_edit_panel"], cmd: Some("xlsx.header-footer"), open: None, start: true, available: always, key: None, toolbar: None),
    // Right pane: the document.
    sidebar_def!("sidebar.comments", "sidebar.comments", Right, G::Document, Pane, panels: &[], cmd: None, open: Some(open_comments), start: false, available: always, key: None, toolbar: Some("show-comments")),
    sidebar_def!("sidebar.find", "sidebar.find", Right, G::Document, Pane, panels: &["sheet_find_panel"], cmd: Some("edit.find-replace"), open: None, start: true, available: always, key: Some("⌘F"), toolbar: None),
    sidebar_def!("sidebar.selection", "sidebar.selection", Right, G::Document, Pane, panels: &[], cmd: Some("xlsx.selection-pane"), open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.names", "sidebar.names", Right, G::Document, Pane, panels: &["names_panel"], cmd: Some("xlsx.named-ranges"), open: None, start: true, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.watch", "sidebar.watch", Right, G::Document, Pane, panels: &[], cmd: Some("xlsx.watch-window"), open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.xml-source", "sidebar.xml-source", Right, G::Document, Pane, panels: &[], cmd: Some("xlsx.xml-source"), open: None, start: false, available: always, key: None, toolbar: None),
    // Right pane: review.
    sidebar_def!("sidebar.thesaurus", "sidebar.thesaurus", Right, G::Review, Pane, panels: &[], cmd: Some("xlsx.thesaurus"), open: Some(by_format), start: true, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.accessibility", "sidebar.accessibility", Right, G::Review, Pane, panels: &[], cmd: Some("xlsx.accessibility-check"), open: None, start: true, available: always, key: None, toolbar: None),
    // Right pane: drawing and settings.
    sidebar_def!("sidebar.ink", "sidebar.ink", Right, G::Tools, Pane, panels: &[], cmd: Some("xlsx.ink"), open: Some(by_format), start: false, available: always, key: None, toolbar: None),
    // Settings dialogs the pane chain shows without a dialog wrapper (their own commands open them; the menu leaves them out).
    sidebar_def!("sidebar.sort", "sidebar.sort", Right, G::Tools, Task, panels: &["sort_panel"], cmd: None, open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.edit-basics", "sidebar.edit-basics", Right, G::Tools, Task, panels: &["edit_panel"], cmd: None, open: None, start: false, available: always, key: None, toolbar: None),
];
