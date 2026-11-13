// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A document's sidebars: the screen's rows of the sidebar table (`FormatInfo::sidebars`; what a row says is in `shell/sidebars.rs`,
//! the list the rows are made from is `tasks/doc/menu-redesign/sidebars.md`). The Window menu's 「サイドバー ▸」 lists them in
//! this order. `panels` are the panel functions of this screen's pane chain (`fn sidebar` in `mod.rs`).
use crate::shell::sidebars::SidebarGroup as G;
use crate::shell::sidebars::SidebarKind::Pane;
use crate::shell::sidebars::SidebarSlot::{Left, Right};
use crate::shell::sidebars::{SidebarDef, always, by_format, chart, open_comments, outline, picture, shape, sidebar_def};

pub(crate) const SIDEBARS: &[SidebarDef] = &[
    // Left and bottom panes.
    sidebar_def!("sidebar.navigation", "sidebar.navigation", Left, G::Document, Pane, panels: &[], cmd: Some("docx.navigation-pane"), open: None, start: false, available: always, key: None, toolbar: Some("outline")),
    // Right pane: format and objects.
    sidebar_def!("sidebar.picture", "sidebar.picture", Right, G::Object, Pane, panels: &["picture_panel"], cmd: None, open: None, start: false, available: picture, key: None, toolbar: None),
    // The text box's settings (#14): the line runs 挿入 ▸ テキスト ボックスの設定 (`docx.textbox.edit`, grey while the caret is in no text box).
    sidebar_def!("sidebar.text-box", "sidebar.text-box", Right, G::Object, Pane, panels: &["text_box_panel"], cmd: Some("docx.textbox.edit"), open: None, start: false, available: shape, key: None, toolbar: None),
    sidebar_def!("sidebar.chart", "sidebar.chart", Right, G::Object, Pane, panels: &["doc_chart_panel"], cmd: None, open: None, start: false, available: chart, key: None, toolbar: None),
    sidebar_def!("sidebar.tabs", "sidebar.tabs", Right, G::Object, Pane, panels: &["tab_panel"], cmd: Some("docx.tabs"), open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.master-doc", "sidebar.master-doc", Right, G::Object, Pane, panels: &["master_doc_panel"], cmd: None, open: None, start: false, available: outline, key: None, toolbar: None),
    // Right pane: the document.
    sidebar_def!("sidebar.comments", "sidebar.comments", Right, G::Document, Pane, panels: &["comments_panel"], cmd: None, open: Some(open_comments), start: false, available: always, key: None, toolbar: Some("show-comments")),
    sidebar_def!("sidebar.find", "sidebar.find", Right, G::Document, Pane, panels: &["find_panel"], cmd: Some("edit.find-replace"), open: None, start: true, available: always, key: Some("⌘F"), toolbar: None),
    sidebar_def!("sidebar.selection", "sidebar.selection", Right, G::Document, Pane, panels: &["selection_pane_panel"], cmd: Some("docx.selection-pane"), open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.quick-parts", "sidebar.quick-parts", Right, G::Document, Pane, panels: &["quick_parts_panel"], cmd: Some("docx.quick-parts.panel"), open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.merge", "sidebar.merge", Right, G::Document, Pane, panels: &["merge_panel"], cmd: None, open: None, start: false, available: always, key: None, toolbar: None),
    // Right pane: review.
    sidebar_def!("sidebar.spelling", "sidebar.spelling", Right, G::Review, Pane, panels: &["doc_spelling_panel"], cmd: None, open: Some(by_format), start: true, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.thesaurus", "sidebar.thesaurus", Right, G::Review, Pane, panels: &["doc_thesaurus_panel"], cmd: None, open: Some(by_format), start: true, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.word-count", "sidebar.word-count", Right, G::Review, Pane, panels: &["word_count_panel"], cmd: Some("tools.word-count"), open: None, start: true, available: always, key: Some("⇧⌘C"), toolbar: None),
    sidebar_def!("sidebar.accessibility", "sidebar.accessibility", Right, G::Review, Pane, panels: &["accessibility_panel"], cmd: Some("docx.accessibility-check"), open: None, start: true, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.screen-reader", "sidebar.screen-reader", Right, G::Review, Pane, panels: &["docx_screen_reader_panel"], cmd: None, open: Some(by_format), start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.read-aloud", "sidebar.read-aloud", Right, G::Review, Pane, panels: &["docx_read_aloud_panel"], cmd: None, open: Some(by_format), start: false, available: always, key: None, toolbar: None),
    // Right pane: drawing and settings.
    sidebar_def!("sidebar.ink", "sidebar.ink", Right, G::Tools, Pane, panels: &["docx_ink_panel"], cmd: None, open: Some(by_format), start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.customize-toolbar", "sidebar.customize-toolbar", Right, G::Tools, Pane, panels: &["docx_customize_toolbar_panel"], cmd: None, open: Some(by_format), start: false, available: always, key: None, toolbar: None),
];
