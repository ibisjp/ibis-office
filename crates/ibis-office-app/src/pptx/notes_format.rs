// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ノートの文字の書式 (表示 > ノートの書式編集): the notes page of the slide shown
//! edited as a slide (`ibis_pptx::notes_page`): its text can be made bold,
//! given a size, a colour, a bullet and the like with the tools of any text.
//! When the view is closed the text goes into the notes page of the file; the
//! file is then read again from what the program holds (what was not saved
//! is kept), and the history of undoing starts over.
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// Which page the view edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageKind {
    /// The notes page of the slide shown.
    Notes,
    /// 配布資料マスター: where a handout's header, date, footer and number go (`ibis_pptx::handout_master`).
    Handout,
    /// ノート マスター: the page's image, the notes, the header, date, footer and number places (`master_page::build_notes_master`).
    NotesMaster,
}

/// The view, while it is open.
pub(crate) struct NotesFormat {
    kind: PageKind,
    /// The file as it was when the page was made.
    base: Vec<u8>,
    slide_index: usize,
    #[cfg(not(target_family = "wasm"))]
    path: Option<std::path::PathBuf>,
    file_name: Option<SharedString>,
    #[cfg(not(target_family = "wasm"))]
    password: Option<String>,
}

impl SlideEditor {
    /// The notes page's view is open.
    pub(crate) fn is_notes_format(&self) -> bool {
        self.notes_format.as_ref().is_some_and(|v| v.kind == PageKind::Notes)
    }

    /// The notes page's or the handout master's view is open.
    pub(crate) fn is_page_view(&self) -> bool {
        self.notes_format.is_some()
    }

    pub(crate) fn is_handout_master(&self) -> bool {
        self.notes_format.as_ref().is_some_and(|v| v.kind == PageKind::Handout)
    }

    /// 表示 > ノートの書式編集: the notes page of the slide shown, as a slide.
    pub(crate) fn open_notes_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_page_view(PageKind::Notes, window, cx);
    }

    pub(crate) fn is_notes_master(&self) -> bool {
        self.notes_format.as_ref().is_some_and(|v| v.kind == PageKind::NotesMaster)
    }

    /// 表示 > ノート マスター: the notes master, as a slide.
    pub(crate) fn open_notes_master(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_page_view(PageKind::NotesMaster, window, cx);
    }

    /// 表示 > 配布資料マスター: the places of the handout master, as a slide.
    pub(crate) fn open_handout_master(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_page_view(PageKind::Handout, window, cx);
    }

    fn open_page_view(&mut self, kind: PageKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_page_view() || self.is_master_view() || !self.has_document || !self.is_presentation() || self.slideshow.is_some() {
            return;
        }
        self.end_text_edit(window, cx);
        self.commit_notes(window, cx);
        let Some(source) = self.source.clone() else {
            return self.set_status(crate::shell::i18n::text("pptx.notes-format.notes-document-cannot-formatted").to_string(), cx);
        };
        let bytes = match source.save(self.presentation(), ibis_pptx::SaveOptions::default()) {
            Ok(bytes) => bytes,
            Err(err) => return self.set_status(crate::shell::i18n::format("file.save-failed", &[("error", &err)]), cx),
        };
        let slide_index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let built = match kind {
            PageKind::Notes => ibis_pptx::notes_page::build(&bytes, slide_index),
            PageKind::Handout => ibis_pptx::handout_master::build(&bytes),
            PageKind::NotesMaster => ibis_pptx::master_page::build_notes_master(&bytes),
        };
        let opened = built.map_err(|e| e.to_string()).and_then(|temp| ibis_pptx::open(temp).map_err(|e| e.to_string()));
        let opened = match opened {
            Ok(opened) => opened,
            Err(err) => return self.set_status(err, cx),
        };
        let view = NotesFormat {
            kind,
            base: bytes,
            slide_index,
            #[cfg(not(target_family = "wasm"))]
            path: self.path.clone(),
            file_name: self.file_name.clone(),
            #[cfg(not(target_family = "wasm"))]
            password: self.protection_password.clone(),
        };
        let name = view.file_name.clone().unwrap_or_default();
        self.load(opened.presentation, name, window, cx);
        self.source = Some(std::sync::Arc::new(opened.source));
        self.notes_format = Some(view);
        let message = match kind {
            PageKind::Notes => "pptx.notes-format.notes-formatting-undo-history",
            PageKind::Handout => "pptx.notes-format.handout-master-undo-history",
            PageKind::NotesMaster => "pptx.notes-format.notes-master-undo-history",
        };
        self.set_status(crate::shell::i18n::text(message).to_string(), cx);
        cx.notify();
    }

    /// ノートの書式編集を閉じる: the text put into the notes page, the file read again.
    pub(crate) fn close_notes_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.notes_format.take() else { return };
        self.end_text_edit(window, cx);
        let merged = self
            .source
            .clone()
            .ok_or_else(|| crate::shell::i18n::text("_pptx.can-t-save").to_string())
            .and_then(|source| source.save(self.presentation(), ibis_pptx::SaveOptions::default()).map_err(|e| e.to_string()))
            .and_then(|edited| match view.kind {
                PageKind::Notes => ibis_pptx::notes_page::apply(&view.base, &edited, view.slide_index),
                PageKind::Handout => ibis_pptx::handout_master::apply(&view.base, &edited),
                PageKind::NotesMaster => ibis_pptx::master_page::apply_notes_master(&view.base, &edited),
            }.map_err(|e| e.to_string()))
            .and_then(|bytes| ibis_pptx::open(bytes).map_err(|e| e.to_string()));
        let opened = match merged {
            Ok(opened) => opened,
            Err(err) => {
                self.notes_format = Some(view);
                return self.set_status(crate::shell::i18n::format("pptx.notes-format.notes-could-not-saved-error", &[("error", &err)]), cx);
            }
        };
        let name = view.file_name.clone().unwrap_or_default();
        self.load(opened.presentation, name, window, cx);
        self.source = Some(std::sync::Arc::new(opened.source));
        self.file_name = view.file_name.clone();
        #[cfg(not(target_family = "wasm"))]
        {
            self.path = view.path.clone();
            self.protection_password = view.password.clone();
        }
        if let Some(id) = self.presentation().slides.get(view.slide_index).map(|s| s.id) {
            self.select_slide(id, window, cx);
        }
        let message = match view.kind {
            PageKind::Notes => "pptx.notes-format.notes-formatting-closed-undo",
            PageKind::Handout => "pptx.notes-format.handout-master-closed-undo-history",
            PageKind::NotesMaster => "pptx.notes-format.notes-master-closed-undo-history",
        };
        self.set_status(crate::shell::i18n::text(message).to_string(), cx);
        cx.notify();
    }

    /// Context menu entries for Notes Format / Notes Master / Handout Master view (`PPTX-NOTES-007..010`, `PPTX-MASTER-010`, `PPTX-MASTER-011`).
    pub(crate) fn notes_format_context_menu_entries(&self) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
        let Some(view) = self.notes_format.as_ref() else {
            return Vec::new();
        };
        let close_key = match view.kind {
            PageKind::Notes => "pptx.notes-format.close-notes-formatting",
            PageKind::Handout => "pptx.notes-format.close-handout-master",
            PageKind::NotesMaster => "pptx.notes-format.close-notes-master",
        };
        vec![
            MenuItem::tr("pptx.notes-header-footer")
                .icon("view_quilt")
                .on(|e, w, cx| e.open_notes_marks(w, cx))
                .into(),
            Submenu::tr("pptx.background", "wallpaper", crate::pptx::chrome::background_menu(self)).into(),
            MenuItem::tr("pptx.theme-colors")
                .icon("palette")
                .on(|e, w, cx| e.open_theme_panel(w, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr(close_key)
                .icon("close")
                .on(|e, w, cx| e.close_notes_format(w, cx))
                .into(),
        ]
    }

    /// Open context menu for Notes Format / Notes Master / Handout Master view at pointer `position`.
    pub(crate) fn open_notes_format_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entries = self.notes_format_context_menu_entries();
        if !entries.is_empty() {
            self.open_popup_entries(entries, position, window, cx);
        }
    }

    pub(crate) fn notes_format_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let kind = self.notes_format.as_ref()?.kind;
        let t = crate::shell::i18n::text;
        let (help, close, title) = match kind {
            PageKind::Notes => ("pptx.notes-format.select-notes-text-set-bold", "pptx.notes-format.close-notes-formatting", "pptx.notes-format.notes-formatting"),
            PageKind::Handout => ("pptx.notes-format.move-resize-boxes-set-size", "pptx.notes-format.close-handout-master", "pptx.notes-format.handout-master"),
            PageKind::NotesMaster => ("pptx.notes-format.notes-master-help", "pptx.notes-format.close-notes-master", "pptx.notes-format.notes-master"),
        };
        let body = v_flex()
            .id("pptx-notes-format-panel")
            .gap_2()
            .px(px(12.))
            .py(px(8.))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, w, cx| {
                    cx.stop_propagation();
                    this.open_notes_format_context_menu(event.position, w, cx);
                }),
            )
            .child(div().text_sm().child(t(help)))
            ;
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("pptx-notes-format-close", t(close), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_notes_format(w, cx))));
        Some(SidebarPanel::with_footer(t(title), body.into_any_element(), panel_terminal_actions))
    }
}
