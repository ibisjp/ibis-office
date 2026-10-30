// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! カスタム ヘッダー・フッターの編集 UI (XLSX-SEC-016):
//! 左・中・右セクションへの文字列・フィールドコード入力、
//! ページ種別（通常・奇数 / 偶数 / 先頭）切り替え、
//! ページ設定（ヘッダー・フッターのオプション）との連携。

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;

use super::header_footer::{join_sections, split_sections};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 編集対象（ヘッダー または フッター）
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HfTarget {
    Header,
    Footer,
}

/// 編集対象のページ種別
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum HfPageKind {
    #[default]
    Odd, // 通常・奇数ページ
    Even, // 偶数ページ
    First, // 先頭ページ
}

/// アクティブな入力セクション（フィールドコード挿入先）
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum HfActiveSection {
    Left,
    #[default]
    Center,
    Right,
}

/// ヘッダー・フッター編集パネルの状態
pub(crate) struct HeaderFooterEditPanel {
    pub(crate) target: HfTarget,
    pub(crate) page_kind: HfPageKind,
    pub(crate) active_section: HfActiveSection,
    pub(crate) left: Entity<InputState>,
    pub(crate) center: Entity<InputState>,
    pub(crate) right: Entity<InputState>,
    pub(crate) code: Entity<InputState>,
    /// The page layout view's page whose header or footer is edited in place on the paper.
    pub(crate) inline_page: Option<usize>,
    /// The undo depth right after the last real-time reflection: the next one replaces that step while nothing
    /// else was done since, so a run of typing is one Undo step (as the sort panel's real-time update).
    live: Option<(usize, ibis_doc_model::SheetPrint)>,
    /// The 「コード」 section is folded away until opened.
    pub(crate) code_open: bool,
    code_state: std::rc::Rc<crate::shell::chrome::collapsible_section::SectionState>,
    _subscriptions: Vec<Subscription>,
}

const FIELD_BUTTONS: &[(&str, &str, &str)] = &[
    ("hf-field-page", "xlsx.hf.insert-page", "&P"),
    ("hf-field-pages", "xlsx.hf.insert-pages", "&N"),
    ("hf-field-date", "xlsx.hf.insert-date", "&D"),
    ("hf-field-time", "xlsx.hf.insert-time", "&T"),
    ("hf-field-file", "xlsx.hf.insert-file", "&F"),
    ("hf-field-sheet", "xlsx.hf.insert-sheet", "&A"),
];

impl SlideEditor {
    /// カスタム ヘッダー／フッター編集画面を開く
    pub(crate) fn open_header_footer_edit(&mut self, target: HfTarget, window: &mut Window, cx: &mut Context<Self>) {
        self.open_header_footer_edit_at(target, None, HfActiveSection::Center, window, cx);
    }

    /// The editor opened from a press on a page's header or footer in the page layout view: the section
    /// under the press is the one to type in, in place on the paper (`page`).
    pub(crate) fn open_header_footer_edit_at(&mut self, target: HfTarget, page: Option<usize>, section: HfActiveSection, window: &mut Window, cx: &mut Context<Self>) {
        let print = self.current_print().unwrap_or_default();
        let hf = &print.header_footer;
        let raw_code = match target {
            HfTarget::Header => hf.odd_header.as_deref().unwrap_or(""),
            HfTarget::Footer => hf.odd_footer.as_deref().unwrap_or(""),
        };
        let (left_str, center_str, right_str) = split_sections(raw_code);
        let left = cx.new(|cx| InputState::new(window, cx).default_value(left_str));
        let center = cx.new(|cx| InputState::new(window, cx).default_value(center_str));
        let right = cx.new(|cx| InputState::new(window, cx).default_value(right_str));
        let code = cx.new(|cx| InputState::new(window, cx).default_value(raw_code.to_string()));

        // Every change is put in the sheet as it is typed. The section last focused is where a field is inserted.
        let mut subscriptions: Vec<Subscription> = [(&left, HfActiveSection::Left), (&center, HfActiveSection::Center), (&right, HfActiveSection::Right)]
            .into_iter()
            .map(|(input, which)| {
                cx.subscribe_in(input, window, move |this, _, event: &InputEvent, window, cx| match event {
                    InputEvent::Focus => {
                        if let Some(p) = this.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
                            p.active_section = which;
                        }
                        cx.notify();
                    }
                    InputEvent::Change | InputEvent::PressEnter { .. } => {
                        // What is typed in a section makes it the one the next field goes in.
                        if let Some(p) = this.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
                            p.active_section = which;
                        }
                        this.apply_header_footer_edit(cx);
                        this.sync_hf_code_input(window, cx);
                    }
                    InputEvent::Blur => {}
                })
            })
            .collect();
        // The code typed directly is split into the sections as it is typed (only while it is the field being typed in).
        subscriptions.push(cx.subscribe_in(&code, window, |this, input, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::Change) && input.focus_handle(cx).is_focused(window) {
                this.apply_hf_code_direct(window, cx);
            }
        }));
        let focus = match section {
            HfActiveSection::Left => left.clone(),
            HfActiveSection::Center => center.clone(),
            HfActiveSection::Right => right.clone(),
        };
        let panel = HeaderFooterEditPanel {
            target,
            page_kind: HfPageKind::Odd,
            active_section: section,
            left,
            center,
            right,
            code,
            inline_page: page,
            live: None,
            code_open: false,
            code_state: Default::default(),
            _subscriptions: subscriptions,
        };
        if page.is_some() {
            focus.update(cx, |input, cx| input.focus(window, cx));
        }
        // The editor is a pane beside the sheet (not a dialog over it): the page setup dialog it may have been opened
        // from is closed, so there is nothing to go back to.
        if let Some(s) = self.sheet_state_mut() {
            s.close_settings();
            s.header_footer_edit = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 編集中のヘッダー／フッターのコード文字列をモデルに適用する
    pub(crate) fn apply_header_footer_edit(&mut self, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()) else { return };
        let target = panel.target;
        let kind = panel.page_kind;
        let l = panel.left.read(cx).value().to_string();
        let c = panel.center.read(cx).value().to_string();
        let r = panel.right.read(cx).value().to_string();
        let new_code = join_sections(&l, &c, &r);
        let live = panel.live.clone();

        // Only while the sheet is as this panel left it (an Undo, or an option changed, since is not replaced).
        if live.as_ref().is_some_and(|(len, print)| *len == self.document.undo_len() && self.current_print().as_ref() == Some(print)) {
            self.document.retract_last();
        }
        let before = self.document.undo_len();
        self.set_print(
            move |p| {
                let hf = &mut p.header_footer;
                match (target, kind) {
                    (HfTarget::Header, HfPageKind::Odd) => hf.odd_header = new_code,
                    (HfTarget::Header, HfPageKind::Even) => hf.even_header = new_code,
                    (HfTarget::Header, HfPageKind::First) => hf.first_header = new_code,
                    (HfTarget::Footer, HfPageKind::Odd) => hf.odd_footer = new_code,
                    (HfTarget::Footer, HfPageKind::Even) => hf.even_footer = new_code,
                    (HfTarget::Footer, HfPageKind::First) => hf.first_footer = new_code,
                }
            },
            None,
            cx,
        );
        let undo_len = self.document.undo_len();
        let print = self.current_print();
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
            p.live = print.filter(|_| undo_len > before).map(|print| (undo_len, print));
        }
    }

    /// コード表示 input を現在の左右中の内容で同期
    fn sync_hf_code_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()) else { return };
        let l = panel.left.read(cx).value().to_string();
        let c = panel.center.read(cx).value().to_string();
        let r = panel.right.read(cx).value().to_string();
        let new_code = join_sections(&l, &c, &r).unwrap_or_default();
        let code_input = panel.code.clone();
        if code_input.read(cx).value().as_ref() != new_code {
            code_input.update(cx, |i, cx| i.set_value(new_code, window, cx));
        }
    }

    /// アクティブな入力セクションにフィールドコードを挿入する
    pub(crate) fn insert_hf_field(&mut self, field_code: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()) else { return };
        // The section being typed in, or else the one last focused.
        let focused = [&panel.left, &panel.center, &panel.right].into_iter().find(|i| i.focus_handle(cx).is_focused(window)).cloned();
        let target_input = focused.unwrap_or_else(|| match panel.active_section {
            HfActiveSection::Left => panel.left.clone(),
            HfActiveSection::Center => panel.center.clone(),
            HfActiveSection::Right => panel.right.clone(),
        });
        // At the caret of the section last focused, as Excel's header editor does.
        target_input.update(cx, |i, cx| {
            i.insert(field_code.to_string(), window, cx);
            i.focus(window, cx);
        });
        self.apply_header_footer_edit(cx);
        self.sync_hf_code_input(window, cx);
    }

    /// 直接書かれたコード文字列から左右中セクションを再構成して反映
    pub(crate) fn apply_hf_code_direct(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()) else { return };
        let raw = panel.code.read(cx).value().to_string();
        let (l, c, r) = split_sections(&raw);
        let left_input = panel.left.clone();
        let center_input = panel.center.clone();
        let right_input = panel.right.clone();
        for (input, value) in [(left_input, l), (center_input, c), (right_input, r)] {
            if input.read(cx).value().as_ref() != value {
                input.update(cx, |i, cx| i.set_value(value, window, cx));
            }
        }
        self.apply_header_footer_edit(cx);
    }

    /// ページ種別（通常・偶数・先頭）の切り替え
    pub(crate) fn set_hf_page_kind(&mut self, kind: HfPageKind, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_header_footer_edit(cx);
        let Some(target) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()).map(|p| p.target) else { return };
        let print = self.current_print().unwrap_or_default();
        let hf = &print.header_footer;
        let raw_code = match (target, kind) {
            (HfTarget::Header, HfPageKind::Odd) => hf.odd_header.as_deref().unwrap_or(""),
            (HfTarget::Header, HfPageKind::Even) => hf.even_header.as_deref().unwrap_or(""),
            (HfTarget::Header, HfPageKind::First) => hf.first_header.as_deref().unwrap_or(""),
            (HfTarget::Footer, HfPageKind::Odd) => hf.odd_footer.as_deref().unwrap_or(""),
            (HfTarget::Footer, HfPageKind::Even) => hf.even_footer.as_deref().unwrap_or(""),
            (HfTarget::Footer, HfPageKind::First) => hf.first_footer.as_deref().unwrap_or(""),
        };
        let (l, c, r) = split_sections(raw_code);
        let Some(panel) = self.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) else { return };
        panel.page_kind = kind;
        panel.live = None;
        let left_input = panel.left.clone();
        let center_input = panel.center.clone();
        let right_input = panel.right.clone();
        let code_input = panel.code.clone();
        left_input.update(cx, |i, cx| i.set_value(l, window, cx));
        center_input.update(cx, |i, cx| i.set_value(c, window, cx));
        right_input.update(cx, |i, cx| i.set_value(r, window, cx));
        code_input.update(cx, |i, cx| i.set_value(raw_code.to_string(), window, cx));
        cx.notify();
    }

    /// ヘッダー／フッター編集パネルを閉じる（入力は入力のたびにシートへ反映済み）
    pub(crate) fn close_header_footer_edit(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.header_footer_edit = None;
        }
        cx.notify();
    }

    /// ヘッダー・フッター編集サイドバーパネル: every change is put in the sheet as it is made.
    pub(crate) fn header_footer_edit_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.header_footer_edit.as_ref()?;
        let target = panel.target;
        let page_kind = panel.page_kind;
        let left_state = panel.left.clone();
        let center_state = panel.center.clone();
        let right_state = panel.right.clone();
        let code_state = panel.code.clone();
        let (code_open, code_section) = (panel.code_open, panel.code_state.clone());

        let print = self.current_print()?;
        let hf = &print.header_footer;

        let title = match target {
            HfTarget::Header => text("xlsx.header-edit.title"),
            HfTarget::Footer => text("xlsx.footer-edit.title"),
        };

        // The pages the header or footer is for are chosen only when the options make them differ.
        let kinds: Vec<(&'static str, &'static str, HfPageKind)> = [
            ("hf-kind-odd", "xlsx.hf.odd-page", HfPageKind::Odd, true),
            ("hf-kind-even", "xlsx.hf.even-page", HfPageKind::Even, hf.different_odd_even),
            ("hf-kind-first", "xlsx.hf.first-page", HfPageKind::First, hf.different_first),
        ].into_iter().filter(|k| k.3).map(|k| (k.0, k.1, k.2)).collect();
        let page_kinds = (kinds.len() > 1).then(|| {
            v_flex().gap_1()
                .child(div().text_sm().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("_xlsx.apply-to-pages")))
                .child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic).children(
                    kinds.into_iter().map(|(id, label, kind)| crate::shell::chrome::segment_control::SegmentItem::label(id, text(label), page_kind == kind).on_click(cx.listener(move |e, _, w, cx| e.set_hf_page_kind(kind, w, cx))))))
        });
        // An option that takes a kind of page away puts the editor back on the standard pages.
        let option = |id: &'static str, label: &'static str, on: bool, change: fn(&mut ibis_doc_model::SheetHeaderFooter, bool), cx: &mut Context<Self>| {
            crate::shell::chrome::checkbox_button(id, text(label), on, false, cx.listener(move |e, _, w, cx| {
                e.set_print(|p| change(&mut p.header_footer, !on), None, cx);
                let hf = e.current_print().map(|p| p.header_footer).unwrap_or_default();
                let page_kind = e.sheet_state().and_then(|s| s.header_footer_edit.as_ref()).map(|p| p.page_kind);
                if (page_kind == Some(HfPageKind::First) && !hf.different_first) || (page_kind == Some(HfPageKind::Even) && !hf.different_odd_even) {
                    e.set_hf_page_kind(HfPageKind::Odd, w, cx);
                }
                cx.notify();
            }))
        };
        let options_row = v_flex().gap_1()
            .child(option("hf-opt-diff-first", "xlsx.hf.diff-first", hf.different_first, |h, v| h.different_first = v, cx))
            .child(option("hf-opt-diff-odd-even", "xlsx.hf.diff-odd-even", hf.different_odd_even, |h, v| h.different_odd_even = v, cx))
            .child(option("hf-opt-scale-doc", "xlsx.hf.scale-with-doc", hf.scale_with_doc, |h, v| h.scale_with_doc = v, cx))
            .child(option("hf-opt-align-margins", "xlsx.hf.align-margins", hf.align_with_margins, |h, v| h.align_with_margins = v, cx));

        // フィールド挿入ボタン群: they go in at the caret of the section last focused.
        let field_buttons = h_flex().flex_wrap().gap_1().children(FIELD_BUTTONS.iter().map(|&(id, label, code)| {
            crate::shell::chrome::action_button(id).small().label(text(label)).on_click(cx.listener(move |this, _, window, cx| {
                this.insert_hf_field(code, window, cx);
            }))
        }));

        let section_block = |label: &'static str, state: &Entity<InputState>| {
            v_flex().gap_1()
                .child(div().text_sm().child(text(label)))
                .child(crate::shell::chrome::text_field(state).full_width())
        };

        let direct_code_row = div().mx(px(-18.)).child(crate::shell::chrome::collapsible_section(
            "hf-code-section", text("xlsx.hf.code-preview"), code_open,
            div().px(px(18.)).child(crate::shell::chrome::text_field(&code_state).full_width()).into_any_element(),
            code_section, self.sidebar_scroll.clone(),
            cx.listener(move |e, _, _, cx| {
                if let Some(p) = e.sheet_state_mut().and_then(|s| s.header_footer_edit.as_mut()) {
                    p.code_open = !code_open;
                }
                cx.notify();
            })));

        let body = v_flex()
            .gap_2()
            .p_0()
            .children(page_kinds)
            .child(div().text_sm().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("_xlsx.options")))
            .child(options_row)
            .child(div().text_sm().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("_xlsx.insert-field-into-section")))
            .child(field_buttons)
            .child(section_block("xlsx.hf.left-section", &left_state))
            .child(section_block("xlsx.hf.center-section", &center_state))
            .child(section_block("xlsx.hf.right-section", &right_state))
            .child(direct_code_row);

        Some(SidebarPanel { title: title.into(), body: body.into_any_element() })
    }
}
