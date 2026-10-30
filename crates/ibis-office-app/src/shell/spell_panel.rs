// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The side panel of ツール > スペル > スペルチェック, shared by the formats: the
//! word not known, its suggestions and the buttons 変更, すべて変更, 無視,
//! すべて無視, 辞書に追加. What a format does with a button is given in
//! [`SpellActions`]; where the words are and how they are changed stays with the format.
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

type Act = fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>);

/// What the buttons of the panel do.
#[derive(Clone, Copy)]
pub(crate) struct SpellActions {
    /// The word replaced by the text (true: every one of the same spelling).
    pub(crate) change: fn(&mut SlideEditor, &str, bool, &mut Window, &mut Context<SlideEditor>),
    /// Not told again (true: this spelling everywhere).
    pub(crate) ignore: fn(&mut SlideEditor, bool, &mut Window, &mut Context<SlideEditor>),
    pub(crate) learn: Act,
    /// The next word (false: the previous).
    pub(crate) step: fn(&mut SlideEditor, bool, &mut Window, &mut Context<SlideEditor>),
    pub(crate) close: Act,
}

/// What the panel shows.
pub(crate) struct SpellView<'a> {
    /// Button ids start with this (`pptx-spell`).
    pub(crate) prefix: &'static str,
    /// The word number shown and the number of words; `None` word: none found.
    pub(crate) current: usize,
    pub(crate) total: usize,
    pub(crate) word: Option<&'a str>,
    pub(crate) suggestions: &'a [String],
}

pub(crate) fn sidebar(view: SpellView, actions: SpellActions, cx: &mut Context<SlideEditor>) -> SidebarPanel {
    let t = crate::shell::i18n::text;
    let heading = |label: &str| div().text_sm().font_weight(FontWeight::BOLD).child(t(label));
    let id = |name: &str| ElementId::Name(format!("{}-{name}", view.prefix).into());
    let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
    if !crate::spelling::available() && view.total == 0 {
        body = body.child(div().text_sm().child(t("pptx.spelling-panel.spell-checking-not-available")));
    } else if let (Some(word), true) = (view.word, view.total > 0) {
        body = body
            .child(div().text_sm().text_color(rgb(0x888888)).child(crate::shell::i18n::format("_pptx.n-of-total", &[("n", &(view.current + 1)), ("total", &view.total)])))
            .child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).text_color(rgb(0xb3261e)).child(word.to_string()))
            .child(heading("pptx.spelling-panel.suggestions"));
        if view.suggestions.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0x888888)).child(t("pptx.spelling-panel.no-suggestions")));
        }
        for (i, s) in view.suggestions.iter().enumerate() {
            let text = s.clone();
            body = body.child(crate::shell::chrome::action_button(id(&format!("suggestion-{i}"))).small().label(s.clone()).on_click(cx.listener(move |e, _, w, cx| (actions.change)(e, &text, false, w, cx))));
        }
        let first = view.suggestions.first().cloned();
        let (first_one, first_all) = (first.clone(), first);
        body = body
            .child(h_flex().gap_1().flex_wrap()
                .child(crate::shell::chrome::action_button(id("change")).small().label(t("_xlsx.change")).on_click(cx.listener(move |e, _, w, cx| if let Some(s) = &first_one { (actions.change)(e, s, false, w, cx) })))
                .child(crate::shell::chrome::action_button(id("change-all")).small().label(t("pptx.spelling-panel.change-all")).on_click(cx.listener(move |e, _, w, cx| if let Some(s) = &first_all { (actions.change)(e, s, true, w, cx) }))))
            .child(h_flex().gap_1().flex_wrap()
                .child(crate::shell::chrome::action_button(id("ignore")).small().label(t("pptx.spelling-panel.ignore")).on_click(cx.listener(move |e, _, w, cx| (actions.ignore)(e, false, w, cx))))
                .child(crate::shell::chrome::action_button(id("ignore-all")).small().label(t("pptx.spelling-panel.ignore-all")).on_click(cx.listener(move |e, _, w, cx| (actions.ignore)(e, true, w, cx))))
                .child(crate::shell::chrome::action_button(id("learn")).small().label(t("pptx.spelling-panel.add-dictionary")).on_click(cx.listener(move |e, _, w, cx| (actions.learn)(e, w, cx)))))
            .child(h_flex().gap_1()
                .child(crate::shell::chrome::action_button(id("previous")).small().label(t("_shell.previous")).on_click(cx.listener(move |e, _, w, cx| (actions.step)(e, false, w, cx))))
                .child(crate::shell::chrome::action_button(id("next")).small().label(t("_shell.next")).on_click(cx.listener(move |e, _, w, cx| (actions.step)(e, true, w, cx)))));
    } else {
        body = body.child(div().text_sm().child(t("pptx.spelling-panel.no-spelling-errors-found")));
    }
    let panel_terminal_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new(id("close"), t("_shell.close"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(move |e, _, w, cx| (actions.close)(e, w, cx))));
        SidebarPanel::with_footer(t("pptx.spelling-panel.spelling"), body.into_any_element(), panel_terminal_actions)
}
