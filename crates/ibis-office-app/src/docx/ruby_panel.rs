// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ルビ (Word's ルビ, Phonetic Guide): the sidebar panel that puts ruby
//! over the selection (in one paragraph), or changes the ruby of the
//! ruby selected; empty, it takes the ruby away (the base stays).

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{FlowCommand, FlowPos, FlowRunKind, RubyAlign};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The panel's field.
pub(crate) struct RubyState {
    pub(crate) text: Entity<InputState>,
    target:(FlowPos,FlowPos),
    _subscription: Subscription,
}

impl SlideEditor {
    /// The selection in one paragraph: its paragraph, offsets and text, and
    /// the ruby run it is (if it is exactly one).
    fn ruby_selection(&self) -> Option<(u32, usize, usize, String, Option<FlowRunKind>)> {
        let (start, end) = self.doc_state().and_then(|s|s.ruby.as_ref()).map(|r|r.target).or_else(||self.ordered_selection())?;
        if start.para != end.para || start == end {
            return None;
        }
        let p = ibis_doc_model::flow_command::paragraph(self.flow()?, start.para)?;
        let mut at = 0;
        let mut text = String::new();
        let mut ruby = None;
        for run in &p.runs {
            let len = run.len();
            let (a, b) = (at.max(start.offset), (at + len).min(end.offset));
            if a < b {
                match &run.kind {
                    FlowRunKind::Text(t) => text.extend(t.chars().skip(a - at).take(b - a)),
                    kind @ FlowRunKind::Ruby { base, .. } if start.offset == at && end.offset == at + len => {
                        text.push_str(base);
                        ruby = Some(kind.clone());
                    }
                    kind @ FlowRunKind::Field { instruction, .. } if start.offset == at && end.offset == at + len => {
                        if let Some((_, inner)) = ibis_doc_model::field_format::enclosed(instruction) {
                            text.push_str(&inner);
                            ruby = Some(kind.clone());
                        } else {
                            return None;
                        }
                    }
                    // Only text (or one ruby / enclosed field) takes ruby.
                    _ => return None,
                }
            }
            at += len;
        }
        Some((start.para, start.offset, end.offset, text, ruby))
    }

    /// Put `ruby` over the selection (as Word sets it: half the base's
    /// size, its baseline the base's size less a point above), or take the
    /// ruby away (an empty `ruby`).
    pub(crate) fn set_ruby(&mut self, ruby: &str, cx: &mut Context<Self>) {
        let Some((para, from, to, base, _)) = self.ruby_selection() else { return };
        let size = self.flow().and_then(|d| ibis_doc_model::flow_command::paragraph(d, para)).map_or(1050, |p| p.format_at(from + 1).2.size);
        let ruby = ruby.trim();
        let mut commands = vec![FlowCommand::Delete { from: FlowPos::new(para, from), to: FlowPos::new(para, to) }];
        let len = if ruby.is_empty() {
            commands.push(FlowCommand::InsertText { at: FlowPos::new(para, from), text: base.clone(), format: None });
            base.chars().count()
        } else {
            let kind = FlowRunKind::Ruby { base, text: ruby.to_string(), size: size / 2, raise: size.saturating_sub(100), align: RubyAlign::Center };
            commands.push(FlowCommand::InsertField { at: FlowPos::new(para, from), kind });
            1
        };
        if self.run_flow(commands, cx)
            && let Some(state) = self.doc_state_mut()
        {
            state.anchor = Some(FlowPos::new(para, from));
            state.caret = Some(FlowPos::new(para, from + len));
            if let Some(ruby)=state.ruby.as_mut(){ruby.target=(FlowPos::new(para,from),FlowPos::new(para,from+len));}
        }
        cx.notify();
    }

    /// 囲い文字: the 1–2 characters selected in `enclosure` (○ □ △ ◇), as
    /// Word writes it (an `EQ \o\ac` field).
    pub(crate) fn enclose_character(&mut self, enclosure: char, cx: &mut Context<Self>) {
        let Some((para, from, to, text, _)) = self.ruby_selection() else { return };
        let count = text.chars().count();
        if count == 0 || count > 2 {
            self.status = Some(crate::shell::i18n::text("docx.ruby-panel.select-1-2-characters-enclose"));
            cx.notify();
            return;
        }
        let kind = FlowRunKind::Field { instruction: format!("EQ \\o\\ac({enclosure},{text})"), shown: String::new() };
        let commands = vec![FlowCommand::Delete { from: FlowPos::new(para, from), to: FlowPos::new(para, to) }, FlowCommand::InsertField { at: FlowPos::new(para, from), kind }];
        if self.run_flow(commands, cx)
            && let Some(state) = self.doc_state_mut()
        {
            state.anchor = Some(FlowPos::new(para, from));
            state.caret = Some(FlowPos::new(para, from + 1));
        }
        cx.notify();
    }

    /// 囲い文字の解除: clear the enclosed character back to plain text.
    pub(crate) fn clear_enclosed_character(&mut self, cx: &mut Context<Self>) {
        let Some((para, from, to, text, Some(FlowRunKind::Field { instruction, .. }))) = self.ruby_selection() else { return };
        if ibis_doc_model::field_format::enclosed(&instruction).is_none() {
            return;
        }
        let len = text.chars().count();
        let commands = vec![
            FlowCommand::Delete { from: FlowPos::new(para, from), to: FlowPos::new(para, to) },
            FlowCommand::InsertText { at: FlowPos::new(para, from), text, format: None },
        ];
        if self.run_flow(commands, cx)
            && let Some(state) = self.doc_state_mut()
        {
            state.anchor = Some(FlowPos::new(para, from));
            state.caret = Some(FlowPos::new(para, from + len));
        }
        cx.notify();
    }

    /// 表示形式 > テキスト > ルビ…: the panel, with the selection's ruby.
    pub(crate) fn open_ruby(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let current = match self.ruby_selection() {
            Some((.., Some(FlowRunKind::Ruby { text, .. }))) => text,
            Some(_) => String::new(),
            None => {
                self.status = Some(crate::shell::i18n::text("docx.ruby-panel.select-text-add-phonetic-guide"));
                cx.notify();
                return;
            }
        };
        if self.doc_state().is_some_and(|s| s.ruby.is_none()) {
            let Some(target)=self.ordered_selection() else{return;};
            let text = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("docx.ruby-panel.ruby-text")));
            let subscription = cx.subscribe_in(&text, window, |this, input, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    let text = input.read(cx).value().to_string();
                    this.set_ruby(&text, cx);
                    this.close_panel_dialog(window,cx);
                }
            });
            if let Some(state) = self.doc_state_mut() {
                state.ruby = Some(RubyState { text, target, _subscription: subscription });
            }
        }
        if let Some(input) = self.doc_state().and_then(|s| s.ruby.as_ref()).map(|r| r.text.clone()) {
            input.update(cx, |a, cx| {
                a.set_value(current, window, cx);
                a.focus(window, cx);
            });
        }
        if let Some(state) = self.doc_state_mut() {
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.text_box = None;
            state.merge = None;
            state.table_size = None;
            state.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn ruby_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let input = self.doc_state()?.ruby.as_ref()?.text.clone();
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(crate::shell::chrome::text_field(&input).full_width())
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("ruby-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|this, _, window, cx| {
                        let text = this.doc_state().and_then(|s| s.ruby.as_ref()).map(|r| r.text.read(cx).value().to_string()).unwrap_or_default();
                        this.set_ruby(&text, cx);
                        this.close_panel_dialog(window,cx);
                    })))
            .action(crate::shell::chrome::dialog_cancel_definition(cx))
                    .action(crate::shell::chrome::ActionDefinition::new("ruby-remove", crate::shell::i18n::text("docx.ruby-panel.remove-ruby"), crate::shell::chrome::ActionButtonKind::Destructive).on_click(cx.listener(|this, _, _, cx| this.set_ruby("", cx))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("docx.ruby-panel.phonetic-guide"), body.into_any_element(), panel_actions))
    }
}
