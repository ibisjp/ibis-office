// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > 記号と特殊文字… (PPTX-TEXT-024): the shared characters panel
//! (`shell/symbols.rs`) in the sidebar; pressing a character puts it at the
//! caret of the text being edited (over the selection), or at the end of the
//! text of the shape selected, in the text's own format.

use std::rc::Rc;

use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(Symbols)]
impl SlideEditor {
    /// 挿入 > 記号と特殊文字…: the panel.
    pub(crate) fn open_symbols_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.pptx().is_some() {
            self.shape_edit.symbols.get_or_insert(0);
        }
        self.symbol.get_or_insert_with(crate::shell::symbols::SymbolState::default);
        self.sidebar_open = true;
        cx.notify();
    }

    /// A character picked in the shared panel (`FormatEditor::insert_symbol`): into the text being edited, else the text of
    /// the first shape selected, else a new text box in the middle of the slide.
    pub(crate) fn insert_symbol_in_slide(&mut self, c: char, window: &mut Window, cx: &mut Context<Self>) {
        if self.text_editing().is_some() {
            self.shape_type_text(&c.to_string(), cx);
        } else if let Some(&shape) = self.selected_ids().first() {
            self.begin_text_edit(shape, false, false, window, cx);
            self.shape_type_text(&c.to_string(), cx);
        } else {
            let slide_sz = self.slide().size.unwrap_or(ibis_doc_model::EmuSize::new(9_144_000, 6_858_000));
            let center = ibis_doc_model::EmuPoint::new(slide_sz.width / 2 - 914_400, slide_sz.height / 2 - 457_200);
            let frame = self.presentation().default_text_box_frame(center);
            let id = self.document.ids().new_shape_id();
            let shape = ibis_doc_model::Shape::text_box(id, frame, "");
            self.insert_shape(shape, cx);
            self.begin_text_edit(id, true, false, window, cx);
            self.shape_type_text(&c.to_string(), cx);
        }
    }

    /// Put `c` at the caret; with a shape selected and no text edited, the
    /// text is edited from the end first.
    pub(crate) fn insert_symbol_text(&mut self, c: char, window: &mut Window, cx: &mut Context<Self>) {
        if self.text_editing().is_none() {
            let target = match self.selected_ids().as_slice() {
                [one] => self.slide().shape(*one).filter(|s| s.text_body().is_some()).map(|s| s.id),
                _ => None,
            };
            let Some(id) = target else { return };
            self.begin_text_edit(id, false, false, window, cx);
        }
        if self.text_editing().is_some() {
            self.input_replace(None, &c.to_string(), cx);
            self.focus_handle.focus(window, cx);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn symbols_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let group = self.shape_edit.symbols?;
        let select = Rc::new(|this: &mut SlideEditor, i: usize, cx: &mut Context<SlideEditor>| {
            if this.pptx().is_some() {
                this.shape_edit.symbols = Some(i);
            }
            cx.notify();
        });
        let pick = Rc::new(|this: &mut SlideEditor, c: char, window: &mut Window, cx: &mut Context<SlideEditor>| {
            this.insert_symbol_text(c, window, cx);
        });
        Some(SidebarPanel { title: crate::shell::i18n::text("fix.symbols.symbols-special-characters"), body: crate::shell::symbols::picker(group, select, pick, self.dialog.bounds.map_or(px(520.), |b| b.size.width), cx) })
    }
}
