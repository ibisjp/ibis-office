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

impl SlideEditor {
    /// 挿入 > 記号と特殊文字…: the panel.
    pub(crate) fn open_symbols_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(state) = self.pptx_mut() {
            state.symbols.get_or_insert(0);
        }
        self.symbol.get_or_insert_with(crate::shell::symbols::SymbolState::default);
        self.sidebar_open = true;
        cx.notify();
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
        let group = self.pptx()?.symbols?;
        let select = Rc::new(|this: &mut SlideEditor, i: usize, cx: &mut Context<SlideEditor>| {
            if let Some(s) = this.pptx_mut() {
                s.symbols = Some(i);
            }
            cx.notify();
        });
        let pick = Rc::new(|this: &mut SlideEditor, c: char, window: &mut Window, cx: &mut Context<SlideEditor>| {
            this.insert_symbol_text(c, window, cx);
        });
        Some(SidebarPanel { title: crate::shell::i18n::text("fix.symbols.symbols-special-characters"), body: crate::shell::symbols::picker(group, select, pick, self.panel_dialog.bounds.map_or(px(520.), |b| b.size.width), cx) })
    }
}
