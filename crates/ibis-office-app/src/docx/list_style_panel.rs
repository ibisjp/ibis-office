// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示形式 > 段落番号 > 新しいリスト スタイルの定義… (Word's Define New List Style):
//! Define a new multilevel numbering style with custom name, number format,
//! prefix/suffix, alignment, indents, and suffix spacing, linking styles.xml
//! (`w:type="numbering"`) and numbering.xml (`w:numStyleLink`).

use gpui_kit::component::input::InputState;
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{FlowAlign, ListLevel, ListSuffix, ParaFormat, RunFormat};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// The panel state for defining a new list style.
pub(crate) struct ListStyleState {
    pub(crate) name: Entity<InputState>,
    pub(crate) level: usize,
    pub(crate) format_idx: usize,
    pub(crate) prefix: Entity<InputState>,
    pub(crate) suffix_text: Entity<InputState>,
    pub(crate) start: Entity<InputState>,
    pub(crate) align: FlowAlign,
    pub(crate) suffix: ListSuffix,
    pub(crate) indent_left: Entity<InputState>,
    pub(crate) indent_hanging: Entity<InputState>,
    #[allow(dead_code)]
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

impl ListStyleState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("docx.list.define-style.name-placeholder"));
        name.update(cx, |i, cx| i.set_value(&crate::shell::i18n::text("docx.list-style.default-name"), window, cx));

        let prefix = cx.new(|cx| InputState::new(window, cx));
        prefix.update(cx, |i, cx| i.set_value("", window, cx));

        let suffix_text = cx.new(|cx| InputState::new(window, cx));
        suffix_text.update(cx, |i, cx| i.set_value(".", window, cx));

        let start = cx.new(|cx| InputState::new(window, cx));
        start.update(cx, |i, cx| i.set_value("1", window, cx));

        let indent_left = cx.new(|cx| InputState::new(window, cx));
        indent_left.update(cx, |i, cx| i.set_value("24", window, cx));

        let indent_hanging = cx.new(|cx| InputState::new(window, cx));
        indent_hanging.update(cx, |i, cx| i.set_value("24", window, cx));

        Self {
            name,
            level: 0,
            format_idx: 0,
            prefix,
            suffix_text,
            start,
            align: FlowAlign::Left,
            suffix: ListSuffix::Tab,
            indent_left,
            indent_hanging,
            ui: Default::default(),
        }
    }
}

impl SlideEditor {
    /// Open the "Define New List Style" sidebar panel.
    pub(crate) fn open_define_list_style_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if let Some(state) = self.doc_state_mut() {
            state.list_style = Some(ListStyleState::new(window, cx));
            self.sidebar_open = true;
            cx.notify();
        }
    }

    /// Close the "Define New List Style" panel.
    pub(crate) fn close_list_style_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.list_style = None;
            cx.notify();
        }
    }

    /// Apply the list style from the panel to the document and close or keep.
    pub(crate) fn apply_list_style_panel(&mut self, close: bool, cx: &mut Context<Self>) {
        let Some(state) = self.doc_state() else { return };
        let Some(ls) = &state.list_style else { return };

        let name = ls.name.read(cx).value().trim().to_string();
        let name = if name.is_empty() { crate::shell::i18n::text("docx.list-style.default-name").to_string() } else { name };

        let prefix = ls.prefix.read(cx).value();
        let suffix_text = ls.suffix_text.read(cx).value();
        let start_num = ls.start.read(cx).value().parse::<u32>().unwrap_or(1);

        let left_pt = ls.indent_left.read(cx).value().parse::<f64>().unwrap_or(24.0);
        let hanging_pt = ls.indent_hanging.read(cx).value().parse::<f64>().unwrap_or(24.0);

        let format = super::list_formats::LIBRARY.get(ls.format_idx).map(|(_, _, f, _)| *f).unwrap_or("decimal");
        let active_level = ls.level;
        let align = ls.align;
        let suffix = ls.suffix;

        // Build 9 levels
        let levels: Vec<ListLevel> = (0..9u32)
            .map(|l| {
                if l as usize == active_level {
                    let lvl_text = format!("{prefix}%{}{suffix_text}", l + 1);
                    ListLevel {
                        start: start_num,
                        format: format.into(),
                        text: lvl_text,
                        suffix,
                        align,
                        para: ParaFormat {
                            left: Some((left_pt * 635.0 * 20.0) as i64),
                            first_line: Some((-(hanging_pt * 635.0 * 20.0)) as i64),
                            ..ParaFormat::default()
                        },
                        run: RunFormat::default(),
                        pic_bullet: None,
                    }
                } else {
                    let indent = (l as i64 + 1) * 440 * 635;
                    ListLevel {
                        start: 1,
                        format: "decimal".into(),
                        text: format!("%{}.", l + 1),
                        suffix: ListSuffix::Tab,
                        align: FlowAlign::Left,
                        para: ParaFormat {
                            left: Some(indent),
                            first_line: Some(-440 * 635),
                            ..ParaFormat::default()
                        },
                        run: RunFormat::default(),
                        pic_bullet: None,
                    }
                }
            })
            .collect();

        self.define_list_style(&name, Some(levels), cx);

        if close {
            self.close_list_style_panel(cx);
        }
    }

    /// Render the "Define New List Style" sidebar panel.
    /// Start number and indents (pt) are applied by OK only, so their ▲▼ only change the number.
    pub(crate) fn list_style_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?;
        let _ = state.list_style.as_ref()?;

        let body = v_flex()
            .gap_3()
            .p_4()
            .text_sm()
            .p_0().child(
                v_flex()
                    .gap_1()
                    .child(div().font_weight(FontWeight::BOLD).child(text("docx.list.define-style.name").to_string()))
                    .child({
                        let state = self.doc_state().unwrap();
                        let ls = state.list_style.as_ref().unwrap();
                        crate::shell::chrome::text_field(&ls.name).full_width()
                    }),
            )
            .child(
                v_flex()
                    .gap_1()
                                        .child(
                        crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic).label(text("docx.list.define-style.level"))
                            .children((0..9).map(|lvl| {
                                let is_active = self.doc_state().unwrap().list_style.as_ref().unwrap().level == lvl;
                                crate::shell::chrome::segment_control::SegmentItem::label(format!("lvl-{lvl}"),format!("{}",lvl+1),is_active)
                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                        if let Some(state) = this.doc_state_mut() {
                                            if let Some(ls) = &mut state.list_style {
                                                ls.level = lvl;
                                                cx.notify();
                                            }
                                        }
                                    }))
                            }))
                    ),
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().font_weight(FontWeight::BOLD).child(text("docx.list.define-style.format").to_string()))
                    .child(
                        {
                            use crate::shell::chrome::{MenuItem,MenuEntry};
                            let current=self.doc_state().unwrap().list_style.as_ref().unwrap().format_idx;
                            let labels=super::list_formats::LIBRARY.iter().take(6).map(|(_,label,_,_)|if label.starts_with("fix."){text(label)}else{(*label).into()}).collect::<Vec<SharedString>>();
                            let entries=labels.iter().enumerate().map(|(idx,label)|MenuEntry::from(MenuItem::new("docx.list.format-choice",label.clone()).on(move |e,_,cx|{if let Some(ls)=e.doc_state_mut().and_then(|s|s.list_style.as_mut()){ls.format_idx=idx;}cx.notify();}))).collect();
                            crate::shell::chrome::Dropdown::from_entries("list-style-format",labels.get(current).cloned().unwrap_or_default(),entries,true,0,&cx.weak_entity()).into_any_element()
                        }
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(div().text_sm().child(text("docx.list.define-style.prefix").to_string()))
                            .child({
                                let state = self.doc_state().unwrap();
                                let ls = state.list_style.as_ref().unwrap();
                                crate::shell::chrome::text_field(&ls.prefix).width(crate::shell::chrome::FIELD_WIDTH * 0.6)
                            })
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(div().text_sm().child(text("docx.list.define-style.suffix-text").to_string()))
                            .child({
                                let state = self.doc_state().unwrap();
                                let ls = state.list_style.as_ref().unwrap();
                                crate::shell::chrome::text_field(&ls.suffix_text).width(crate::shell::chrome::FIELD_WIDTH * 0.6)
                            })
                    )
            .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(div().text_sm().child(text("docx.list.define-style.start-at").to_string()))
                            .child({
                                let state = self.doc_state().unwrap();
                                let ls = state.list_style.as_ref().unwrap();
                                crate::shell::chrome::text_field(&ls.start).full_width().spinner("list-style-start", crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 32767.0).integer_only(), cx, |_, _, _, _| {})
                            })
                    )
            )
            .child(div().text_sm().child(text("docx.list.define-style.text-example")))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(div().text_sm().child(text("docx.list.define-style.align").to_string()))
                            .child(
                                crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                                    .child(
                                        crate::shell::chrome::segment_control::SegmentItem::label("align-left",text("docx.align.left").to_string(),self.doc_state().unwrap().list_style.as_ref().unwrap().align == FlowAlign::Left)
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                if let Some(s) = this.doc_state_mut() {
                                                    if let Some(ls) = &mut s.list_style {
                                                        ls.align = FlowAlign::Left;
                                                        cx.notify();
                                                    }
                                                }
                                            }))
                                    )
                                    .child(
                                        crate::shell::chrome::segment_control::SegmentItem::label("align-center",text("docx.align.center").to_string(),self.doc_state().unwrap().list_style.as_ref().unwrap().align == FlowAlign::Center)
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                if let Some(s) = this.doc_state_mut() {
                                                    if let Some(ls) = &mut s.list_style {
                                                        ls.align = FlowAlign::Center;
                                                        cx.notify();
                                                    }
                                                }
                                            }))
                                    )
                                    .child(
                                        crate::shell::chrome::segment_control::SegmentItem::label("align-right",text("docx.align.right").to_string(),self.doc_state().unwrap().list_style.as_ref().unwrap().align == FlowAlign::Right)
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                if let Some(s) = this.doc_state_mut() {
                                                    if let Some(ls) = &mut s.list_style {
                                                        ls.align = FlowAlign::Right;
                                                        cx.notify();
                                                    }
                                                }
                                            }))
                                    )
                            )
                    )
            )
            .child(
                v_flex()
                    .gap_1()
                    .child(div().text_sm().child(text("docx.list.define-style.follow-number").to_string()))
                    .child(
                        crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                            .child(
                                crate::shell::chrome::segment_control::SegmentItem::label("suff-tab",text("docx.list.suffix-tab").to_string(),self.doc_state().unwrap().list_style.as_ref().unwrap().suffix == ListSuffix::Tab)
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        if let Some(s) = this.doc_state_mut() {
                                            if let Some(ls) = &mut s.list_style {
                                                ls.suffix = ListSuffix::Tab;
                                                cx.notify();
                                            }
                                        }
                                    }))
                            )
                            .child(
                                crate::shell::chrome::segment_control::SegmentItem::label("suff-space",text("docx.list.suffix-space").to_string(),self.doc_state().unwrap().list_style.as_ref().unwrap().suffix == ListSuffix::Space)
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        if let Some(s) = this.doc_state_mut() {
                                            if let Some(ls) = &mut s.list_style {
                                                ls.suffix = ListSuffix::Space;
                                                cx.notify();
                                            }
                                        }
                                    }))
                            )
                            .child(
                                crate::shell::chrome::segment_control::SegmentItem::label("suff-none",text("docx.list.suffix-none").to_string(),self.doc_state().unwrap().list_style.as_ref().unwrap().suffix == ListSuffix::Nothing)
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        if let Some(s) = this.doc_state_mut() {
                                            if let Some(ls) = &mut s.list_style {
                                                ls.suffix = ListSuffix::Nothing;
                                                cx.notify();
                                            }
                                        }
                                    }))
                            )
                    )
            )
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(div().text_sm().child(text("docx.list.define-style.indent-left").to_string()))
                            .child({
                                let state = self.doc_state().unwrap();
                                let ls = state.list_style.as_ref().unwrap();
                                crate::shell::chrome::text_field(&ls.indent_left).spinner("list-style-indent-left", crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 1584.0), cx, |_, _, _, _| {}).unit("pt", px(24.))
                            })
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(div().text_sm().child(text("docx.list.define-style.indent-hanging").to_string()))
                            .child({
                                let state = self.doc_state().unwrap();
                                let ls = state.list_style.as_ref().unwrap();
                                crate::shell::chrome::text_field(&ls.indent_hanging).spinner("list-style-indent-hanging", crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 1584.0), cx, |_, _, _, _| {}).unit("pt", px(24.))
                            })
                    )
            )
            ;

        let panel_terminal_actions = crate::shell::chrome::action_bar()
                    .action(
                        crate::shell::chrome::ActionDefinition::new("apply-list-style", text("docx.list.define-style.add").to_string(), crate::shell::chrome::ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.apply_list_style_panel(true, cx);
                            }))
                    )
                    .action(
                        crate::shell::chrome::ActionDefinition::new("cancel-list-style", text("docx.list.define-style.cancel").to_string(), crate::shell::chrome::ActionButtonKind::Normal)
                            .on_click(cx.listener(|this, _, _window, cx| {
                                this.close_list_style_panel(cx);
                            }))
                    );
        Some(SidebarPanel::with_footer(text("docx.list.define-style.title"), body.into_any_element(), panel_terminal_actions))
    }
}
