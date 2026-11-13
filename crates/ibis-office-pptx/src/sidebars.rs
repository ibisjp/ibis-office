// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A presentation's sidebars: the screen's rows of the sidebar table (`FormatInfo::sidebars`; what a row says is in `shell/sidebars.rs`,
//! the list the rows are made from is `tasks/doc/menu-redesign/sidebars.md`). The Window menu's 「サイドバー ▸」 lists them in
//! this order. `panels` are the panel functions of this screen's pane chain (`fn sidebar` in `mod.rs`).
use crate::shell::sidebars::SidebarGroup as G;
use crate::shell::sidebars::SidebarKind::Pane;
use crate::shell::sidebars::SidebarSlot::{Bottom, Left, Right};
use crate::shell::sidebars::{SidebarDef, always, by_format, master, open_comments, smartart, sidebar_def};

pub(crate) const SIDEBARS: &[SidebarDef] = &[
    // Left and bottom panes.
    sidebar_def!("sidebar.thumbnails", "sidebar.thumbnails", Left, G::Document, Pane, panels: &[], cmd: None, open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.notes", "sidebar.notes", Bottom, G::Document, Pane, panels: &[], cmd: Some("pptx.notes"), open: None, start: false, available: always, key: None, toolbar: Some("notes-pane")),
    // Right pane: format and objects.
    sidebar_def!("sidebar.format", "sidebar.format", Right, G::Object, Pane, panels: &[], cmd: Some("format.options"), open: None, start: false, available: always, key: None, toolbar: Some("sidebar")),
    sidebar_def!("sidebar.3d-model", "sidebar.3d-model", Right, G::Object, Pane, panels: &["threed_model_panel"], cmd: Some("pptx.3d-model.panel"), open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.smartart-text", "sidebar.smartart-text", Right, G::Object, Pane, panels: &["smartart_text_pane"], cmd: Some("pptx.smartart.text-pane"), open: None, start: false, available: smartart, key: None, toolbar: None),
    sidebar_def!("sidebar.master", "sidebar.master", Right, G::Object, Pane, panels: &["master_view_panel"], cmd: None, open: None, start: false, available: master, key: None, toolbar: None),
    sidebar_def!("sidebar.notes-format", "sidebar.notes-format", Right, G::Object, Pane, panels: &["notes_format_panel"], cmd: Some("pptx.notes-format"), open: None, start: false, available: always, key: None, toolbar: None),
    // Right pane: the document.
    sidebar_def!("sidebar.comments", "sidebar.comments", Right, G::Document, Pane, panels: &["slide_comments_panel"], cmd: None, open: Some(open_comments), start: false, available: always, key: None, toolbar: Some("show-comments")),
    sidebar_def!("sidebar.find", "sidebar.find", Right, G::Document, Pane, panels: &["pptx_find_panel"], cmd: Some("edit.find-replace"), open: None, start: true, available: always, key: Some("⌘F"), toolbar: None),
    sidebar_def!("sidebar.selection", "sidebar.selection", Right, G::Document, Pane, panels: &["selection_pane"], cmd: Some("pptx.selection-pane"), open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.reading-order", "sidebar.reading-order", Right, G::Document, Pane, panels: &["reading_order_pane"], cmd: Some("pptx.reading-order"), open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.animation", "sidebar.animation", Right, G::Document, Pane, panels: &["animation_pane"], cmd: Some("pptx.anim.pane"), open: None, start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.record-preview", "sidebar.record-preview", Right, G::Document, Pane, panels: &["record_preview_panel"], cmd: Some("pptx.record-preview"), open: None, start: true, available: always, key: None, toolbar: None),
    // Right pane: review.
    sidebar_def!("sidebar.spelling", "sidebar.spelling", Right, G::Review, Pane, panels: &["spelling_panel"], cmd: Some("pptx.spelling.check"), open: Some(by_format), start: true, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.thesaurus", "sidebar.thesaurus", Right, G::Review, Pane, panels: &["pptx_thesaurus_panel"], cmd: Some("pptx.thesaurus"), open: Some(by_format), start: true, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.accessibility", "sidebar.accessibility", Right, G::Review, Pane, panels: &["pptx_accessibility_panel"], cmd: Some("pptx.check-accessibility"), open: None, start: true, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.screen-reader", "sidebar.screen-reader", Right, G::Review, Pane, panels: &["pptx_screen_reader_panel"], cmd: Some("pptx.screen-reader"), open: Some(by_format), start: false, available: always, key: None, toolbar: None),
    // Right pane: drawing and settings.
    sidebar_def!("sidebar.ink", "sidebar.ink", Right, G::Tools, Pane, panels: &["pptx_ink_panel"], cmd: Some("pptx.ink"), open: Some(by_format), start: false, available: always, key: None, toolbar: None),
    sidebar_def!("sidebar.customize-toolbar", "sidebar.customize-toolbar", Right, G::Tools, Pane, panels: &["pptx_ribbon_qat_panel"], cmd: Some("pptx.customize-ribbon"), open: Some(by_format), start: false, available: always, key: None, toolbar: None),
];
