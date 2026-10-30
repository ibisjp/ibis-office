// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ツール > スペル > 類義語辞典 (and the shape's context menu): the selected words looked up in the thesaurus the
//! Word side has (`docx/thesaurus.rs`), the synonyms listed under their kinds and meanings, as on the .docx and .xlsx
//! sides. A synonym chosen while text is selected in a shape takes the place of the selection.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;

use crate::docx::thesaurus::{ThesaurusResult, lookup};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};

pub(crate) struct PptxThesaurusPanel {
    pub(crate) query: Entity<InputState>,
    pub(crate) result: Option<ThesaurusResult>,
    pub(crate) searched: String,
    pub(crate) message: String,
    _subscription: Subscription,
}

impl SlideEditor {
    /// The text selected in the shape being edited (empty when nothing is selected).
    fn pptx_selected_words(&self) -> String {
        let (Some(edit), Some(all)) = (self.text_editing(), self.edited_text()) else { return String::new() };
        let range = edit.range();
        all.chars().skip(range.start).take(range.len()).collect::<String>().trim().to_string()
    }

    pub(crate) fn open_pptx_thesaurus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let word = self.pptx_selected_words();
        let query = cx.new(|cx| {
            let mut input = InputState::new(window, cx).placeholder(text("docx.thesaurus.search-placeholder"));
            if !word.is_empty() {
                input.set_value(&word, window, cx);
            }
            input.focus(window, cx);
            input
        });
        let subscription = cx.subscribe_in(&query, window, |this, state, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                let word = state.read(cx).value().to_string();
                this.pptx_thesaurus_search(&word, cx);
            }
        });
        let result = if word.is_empty() { None } else { lookup(&word) };
        if let Some(s) = self.pptx_mut() {
            s.find = None;
            s.spelling = None;
            s.thesaurus = Some(PptxThesaurusPanel { query, result, searched: word, message: String::new(), _subscription: subscription });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn pptx_thesaurus_search(&mut self, word: &str, cx: &mut Context<Self>) {
        let word = word.trim().to_string();
        if let Some(p) = self.pptx_mut().and_then(|s| s.thesaurus.as_mut()) {
            p.result = lookup(&word);
            p.searched = word;
            p.message.clear();
        }
        cx.notify();
    }

    /// A synonym chosen: it takes the place of the text selected in the shape being edited.
    pub(crate) fn pptx_thesaurus_replace(&mut self, synonym: &str, cx: &mut Context<Self>) {
        let message = if self.pptx_selected_words().is_empty() {
            text("pptx.thesaurus.select-word").to_string()
        } else {
            self.shape_type_text(synonym, cx);
            format("docx.thesaurus.replaced-msg", &[("word", &synonym)]).to_string()
        };
        if let Some(p) = self.pptx_mut().and_then(|s| s.thesaurus.as_mut()) {
            p.message = message;
        }
        cx.notify();
    }

    pub(crate) fn pptx_thesaurus_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.thesaurus.as_ref()?;
        let theme = cx.theme().clone();
        let query = p.query.clone();
        let searched = p.searched.clone();
        let message = p.message.clone();
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0().child(crate::shell::chrome::text_field(&query).full_width());
        match &p.result {
            None if searched.is_empty() => {
                body = body.child(div().text_sm().text_color(theme.muted_foreground).child(text("docx.thesaurus.empty-prompt")));
            }
            None => {
                body = body
                    .child(div().text_sm().child(format("docx.thesaurus.no-results", &[("word", &searched)])))
                    .child(div().text_sm().text_color(theme.muted_foreground).child(text("docx.thesaurus.hint")));
            }
            Some(result) => {
                for (g, group) in result.groups.iter().enumerate() {
                    body = body.child(div().pt_1().text_sm().font_weight(FontWeight::SEMIBOLD).child(SharedString::from(format!("{} — {}", group.category, group.meaning))));
                    let mut row = h_flex().flex_wrap().gap_1();
                    for (i, synonym) in group.synonyms.iter().enumerate() {
                        let word = synonym.clone();
                        row = row.child(
                            crate::shell::chrome::action_button(SharedString::from(format!("pptx-synonym-{g}-{i}")))
                                .xsmall()
                                .label(SharedString::from(synonym.clone()))
                                .on_click(cx.listener(move |this, _, _, cx| this.pptx_thesaurus_replace(&word, cx))),
                        );
                    }
                    body = body.child(row);
                }
            }
        }
        body = body.child(div().text_sm().child(SharedString::from(message)));
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(
                crate::shell::chrome::ActionDefinition::new("pptx-thesaurus-close", text("_shell.close"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|this, _, _, cx| {
                    if let Some(s) = this.pptx_mut() {
                        s.thesaurus = None;
                    }
                    cx.notify();
                })),
            );
        Some(SidebarPanel::with_footer(text("docx.thesaurus.title"), body.into_any_element(), panel_terminal_actions))
    }
}
