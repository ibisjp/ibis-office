// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示形式 > 禁則文字の設定…: the characters a line may not start with and may not end with (Word's 禁則文字の設定),
//! for the whole document; the standard ones until they are changed.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::FlowCommand;

use super::help_panels::HelpPanels;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct KinsokuPanel {
    pub(crate) strict: bool,
    pub(crate) start: Entity<InputState>,
    pub(crate) end: Entity<InputState>,
}

impl SlideEditor {
    /// 禁則処理の方式が「厳密」かどうか（`w:strictFirstAndLastChars`）。
    pub(crate) fn strict_kinsoku(&self) -> bool {
        self.flow().map_or(true, |d| d.strict_first_and_last_chars)
    }

    /// 禁則処理の方式を切り替える（1手の元に戻す）。
    #[allow(dead_code)]
    pub(crate) fn set_strict_kinsoku(&mut self, strict: bool, cx: &mut Context<Self>) {
        self.run_flow(vec![FlowCommand::SetStrictFirstAndLastChars(strict)], cx);
    }

    /// 禁則プリセット（標準または厳密）を一括適用する（1手の元に戻す）。
    pub(crate) fn set_kinsoku_preset(&mut self, strict: bool, cx: &mut Context<Self>) {
        self.run_flow(
            vec![
                FlowCommand::SetStrictFirstAndLastChars(strict),
                FlowCommand::SetKinsokuLists { start: None, end: None },
            ],
            cx,
        );
    }

    /// The lists now: what the document sets, else the standard or strict ones.
    pub(crate) fn kinsoku_lists(&self) -> (String, String) {
        let doc = self.flow();
        let strict = doc.map_or(true, |d| d.strict_first_and_last_chars);
        let default_start = if strict {
            super::flow_layout::STRICT_NO_LINE_START
        } else {
            super::flow_layout::NORMAL_NO_LINE_START
        };
        (
            doc.and_then(|d| d.no_line_start.clone()).unwrap_or_else(|| default_start.to_string()),
            doc.and_then(|d| d.no_line_end.clone()).unwrap_or_else(|| super::flow_layout::NO_LINE_END.to_string()),
        )
    }

    /// Set the lists (`None`: the standard/strict ones) in one step of undo.
    pub(crate) fn set_kinsoku_lists(&mut self, start: Option<String>, end: Option<String>, cx: &mut Context<Self>) {
        let strict = self.strict_kinsoku();
        let default_start = if strict {
            super::flow_layout::STRICT_NO_LINE_START
        } else {
            super::flow_layout::NORMAL_NO_LINE_START
        };
        // A list equal to the default one is not kept.
        let start = start.filter(|s| s != default_start);
        let end = end.filter(|s| s != super::flow_layout::NO_LINE_END);
        self.run_flow(vec![FlowCommand::SetKinsokuLists { start, end }], cx);
    }

    pub(crate) fn open_kinsoku_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let (start, end) = self.kinsoku_lists();
        let panel = KinsokuPanel {
            strict: self.strict_kinsoku(),
            start: cx.new(|cx| InputState::new(window, cx).default_value(start)),
            end: cx.new(|cx| InputState::new(window, cx).default_value(end)),
        };
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = HelpPanels::default();
            s.helper.kinsoku = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_kinsoku_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.helper.kinsoku = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// 適用: the two lists as typed. If matching strict/normal default, saves as preset.
    pub(crate) fn apply_kinsoku_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.kinsoku.as_ref()) else { return };
        let (start, end) = (p.start.read(cx).value().to_string(), p.end.read(cx).value().to_string());
        let clean = |v: String| v.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        let start_cleaned = clean(start);
        let end_cleaned = clean(end);

        let strict=p.strict;
        let default=if strict {super::flow_layout::STRICT_NO_LINE_START}else{super::flow_layout::NORMAL_NO_LINE_START};
        let commands=vec![FlowCommand::SetStrictFirstAndLastChars(strict),FlowCommand::SetKinsokuLists {
            start:if start_cleaned.is_empty()||start_cleaned==default{None}else{Some(start_cleaned)},
            end:if end_cleaned.is_empty()||end_cleaned==super::flow_layout::NO_LINE_END{None}else{Some(end_cleaned)},
        }];
        self.run_flow(commands, cx);
        self.close_kinsoku_panel(cx);
    }

    /// 「標準」プリセットを入力欄に反映し適用.
    pub(crate) fn select_normal_kinsoku_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p)=self.doc_state_mut().and_then(|s|s.helper.kinsoku.as_mut()){p.strict=false;}
        let (start, end) = (super::flow_layout::NORMAL_NO_LINE_START.to_string(), super::flow_layout::NO_LINE_END.to_string());
        if let Some(p) = self.doc_state().and_then(|s| s.helper.kinsoku.as_ref()) {
            p.start.update(cx, |i, cx| i.set_value(&start, window, cx));
            p.end.update(cx, |i, cx| i.set_value(&end, window, cx));
        }
        cx.notify();
    }

    /// 「厳密」プリセットを入力欄に反映し適用.
    pub(crate) fn select_strict_kinsoku_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p)=self.doc_state_mut().and_then(|s|s.helper.kinsoku.as_mut()){p.strict=true;}
        let (start, end) = (super::flow_layout::STRICT_NO_LINE_START.to_string(), super::flow_layout::NO_LINE_END.to_string());
        if let Some(p) = self.doc_state().and_then(|s| s.helper.kinsoku.as_ref()) {
            p.start.update(cx, |i, cx| i.set_value(&start, window, cx));
            p.end.update(cx, |i, cx| i.set_value(&end, window, cx));
        }
        cx.notify();
    }

    /// 標準に戻す.
    pub(crate) fn reset_kinsoku_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.select_strict_kinsoku_panel(window, cx);
    }

    pub(crate) fn kinsoku_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.kinsoku.as_ref()?;
        let theme = cx.theme();
        let strict = p.strict;
        let expected=if strict{super::flow_layout::STRICT_NO_LINE_START}else{super::flow_layout::NORMAL_NO_LINE_START};
        let is_custom=p.start.read(cx).value().as_ref()!=expected || p.end.read(cx).value().as_ref()!=super::flow_layout::NO_LINE_END;
        let editor=cx.entity().downgrade();
        let label = |key: &'static str| div().text_sm().text_color(theme.muted_foreground).child(text(key));
        let body = v_flex()
            .gap_3()
            .p_3()
            .p_0().child(label("docx.kinsoku.description"))
            .child(
                v_flex()
                    .gap_1()
                    .child(label("docx.kinsoku.rule_label"))
                    .child(crate::shell::chrome::Dropdown::new("kinsoku-preset",text(if is_custom{"docx.kinsoku.rule_custom"}else if strict{"docx.kinsoku.rule_strict"}else{"docx.kinsoku.rule_normal"}),vec![
                        crate::shell::chrome::DropdownItem::new(text("docx.kinsoku.rule_normal")).checked(!is_custom&&!strict).on_select(std::rc::Rc::new(|e,w,cx|e.select_normal_kinsoku_panel(w,cx))),
                        crate::shell::chrome::DropdownItem::new(text("docx.kinsoku.rule_strict")).checked(!is_custom&&strict).on_select(std::rc::Rc::new(|e,w,cx|e.select_strict_kinsoku_panel(w,cx))),
                    ],&cx.weak_entity())),
            )
            .child(v_flex().gap_1().child(label("docx.kinsoku.start")).child(crate::shell::chrome::text_field(&p.start).full_width()))
            .child(v_flex().gap_1().child(label("docx.kinsoku.end")).child(crate::shell::chrome::text_field(&p.end).full_width()))

            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("kinsoku-apply", text("docx.kinsoku.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|this, _, _, cx| this.apply_kinsoku_panel(cx))))
                    .action(crate::shell::chrome::dialog_cancel_definition(cx))
                    .action(crate::shell::chrome::ActionDefinition::new("kinsoku-reset", text("docx.kinsoku.reset"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|this, _, window, cx| this.reset_kinsoku_panel(window, cx))))
                    ;
        Some(SidebarPanel::with_footer(text("docx.kinsoku.title"), body.into_any_element(), panel_actions))
    }
}

#[cfg(test)]
mod tests {
    use super::super::flow_layout::may_break;
    use ibis_doc_model::FlowDocument;

    #[test]
    fn the_documents_lists_replace_the_standard_ones() {
        let mut doc = FlowDocument::default();
        // Standard: a line may not start with 、 nor end with 「.
        assert!(!may_break(Some('あ'), Some('、'), true, true, &doc));
        assert!(!may_break(Some('「'), Some('あ'), true, true, &doc));
        assert!(may_break(Some('あ'), Some('い'), true, true, &doc));
        doc.no_line_start = Some("あ".to_string());
        doc.no_line_end = Some("い".to_string());
        // The document's lists: 、 may start a line now, あ may not; い may not end one, 「 may.
        assert!(may_break(Some('う'), Some('、'), true, true, &doc));
        assert!(!may_break(Some('う'), Some('あ'), true, true, &doc));
        assert!(!may_break(Some('い'), Some('う'), true, true, &doc));
        assert!(may_break(Some('「'), Some('う'), true, true, &doc));
        // Without 禁則処理 nothing is held.
        assert!(may_break(Some('う'), Some('あ'), false, true, &doc));
    }

    #[test]
    fn strict_and_normal_kinsoku_rules() {
        let mut doc = FlowDocument::default();
        doc.strict_first_and_last_chars = true;

        // Strict: small kana and prolonged sound mark cannot start a line
        assert!(!may_break(Some('あ'), Some('っ'), true, true, &doc));
        assert!(!may_break(Some('あ'), Some('ぁ'), true, true, &doc));
        assert!(!may_break(Some('あ'), Some('ー'), true, true, &doc));
        // Punctuation cannot start
        assert!(!may_break(Some('あ'), Some('、'), true, true, &doc));

        // Normal: small kana and prolonged sound mark CAN start a line
        doc.strict_first_and_last_chars = false;
        assert!(may_break(Some('あ'), Some('っ'), true, true, &doc));
        assert!(may_break(Some('あ'), Some('ぁ'), true, true, &doc));
        assert!(may_break(Some('あ'), Some('ー'), true, true, &doc));
        // But punctuation still CANNOT start
        assert!(!may_break(Some('あ'), Some('、'), true, true, &doc));
    }
}
