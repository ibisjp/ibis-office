// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 類義語辞典パネル（文書画面。校閲 > 類義語辞典 / Shift+F7 / docx.review.thesaurus）の状態と操作。辞書と検索は `shell/thesaurus.rs`。

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::FlowPos;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
// What was here before the dictionary moved to shell/thesaurus.rs stays reachable under this name (the tests use it).
#[allow(unused_imports)]
pub use crate::shell::thesaurus::{SynonymEntry, THESAURUS_ENTRIES, ThesaurusGroup, ThesaurusResult, lookup};
use crate::ext::*;

/// 類義語辞典パネル状態
pub(crate) struct ThesaurusState {
    pub(crate) query_input: Entity<InputState>,
    pub(crate) query: String,
    pub(crate) result: Option<ThesaurusResult>,
    pub(crate) target_range: Option<(FlowPos, FlowPos)>,
    pub(crate) history: Vec<String>,
    pub(crate) history_index: usize,
    pub(crate) message: Option<String>,
    _subscriptions: Vec<Subscription>,
}

#[ibis_office_macros::editor_ext(Thesaurus)]
impl SlideEditor {
    /// 校閲 > 類義語辞典 (Shift+F7 / docx.review.thesaurus) を開く
    pub fn open_thesaurus_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.end_text_edit(window, cx);

        // 1. 選択中のテキストがあるか調べる
        let mut initial_query = self.selected_text().trim().to_string();
        let mut target_range = None;

        if !initial_query.is_empty() {
            if let Some((start, end)) = self.ordered_selection() {
                target_range = Some((start, end));
            }
        } else if let Some(caret) = self.doc_state().and_then(|s| s.caret) {
            // キャレット位置の単語を取得
            let (start, end) = self.word_at(caret);
            let word = self.range_text(start, end).trim().to_string();
            if !word.is_empty() {
                initial_query = word;
                target_range = Some((start, end));
            }
        }

        let query_input = cx.new(|cx| {
            let mut inp = InputState::new(window, cx).placeholder(text("docx.thesaurus.search-placeholder"));
            if !initial_query.is_empty() {
                inp.set_value(&initial_query, window, cx);
            }
            inp.focus(window, cx);
            inp
        });

        let mut subscriptions = Vec::new();
        let sub = cx.subscribe_in(&query_input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let q = this.doc_state()
                    .and_then(|s| s.thesaurus.as_ref())
                    .map(|ts| ts.query_input.read(cx).value().trim().to_string())
                    .unwrap_or_default();
                if !q.is_empty() {
                    this.thesaurus_search(&q, window, cx);
                }
            }
        });
        subscriptions.push(sub);

        let result = if !initial_query.is_empty() {
            lookup(&initial_query)
        } else {
            None
        };

        let history = if !initial_query.is_empty() {
            vec![initial_query.clone()]
        } else {
            Vec::new()
        };

        if let Some(state) = self.doc_state_mut() {
            state.thesaurus = Some(ThesaurusState {
                query_input,
                query: initial_query,
                result,
                target_range,
                history,
                history_index: 0,
                message: None,
                _subscriptions: subscriptions,
            });
            // 他のサイドバーを閉じる
            state.find = None;
            state.link = None;
            state.style_edit = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.picture_edit = None;
            state.table_size = None;
            state.table_formula = None;
            state.comments = None;
            state.ruby = None;
            state.text_box = None;
            state.merge = None;
            state.helper = crate::help_panels::HelpPanels::default();
            state.docx_math = None;
            state.quick_parts = None;
            state.source_manager = None;
            state.print_panel = None;
            state.inspect_panel = None;
            state.compatibility_panel = None;
        }

        self.sidebar_open = true;
        cx.notify();
    }

    /// 類義語辞典パネルを閉じる
    pub(crate) fn close_thesaurus_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.thesaurus = None;
        }
        self.sidebar_open = false;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// 指定されたクエリで検索を実行
    pub(crate) fn thesaurus_search(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let q = query.trim().to_string();
        if q.is_empty() {
            return;
        }

        let result = lookup(&q);

        if let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) {
            ts.query_input.update(cx, |inp, cx| {
                inp.set_value(&q, window, cx);
            });
            ts.query = q.clone();
            ts.result = result;
            ts.message = None;

            // 履歴に追加
            if ts.history.last().map(|s| s.as_str()) != Some(&q) {
                ts.history.push(q);
                ts.history_index = ts.history.len().saturating_sub(1);
            }
        }
        cx.notify();
    }

    /// 検索履歴を1つ戻る
    pub(crate) fn thesaurus_history_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prev_query = {
            let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) else { return };
            if ts.history_index > 0 {
                ts.history_index -= 1;
                ts.history.get(ts.history_index).cloned()
            } else {
                None
            }
        };

        if let Some(q) = prev_query {
            let result = lookup(&q);
            if let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) {
                ts.query_input.update(cx, |inp, cx| {
                    inp.set_value(&q, window, cx);
                });
                ts.query = q;
                ts.result = result;
                ts.message = None;
            }
            cx.notify();
        }
    }

    /// 選択中の単語を類義語で置換する
    pub(crate) fn thesaurus_replace(&mut self, replacement: &str, cx: &mut Context<Self>) {
        let Some(doc) = self.flow().cloned() else { return };

        // 置換対象の範囲を決定
        let range = if let Some((start, end)) = self.ordered_selection() {
            Some((start, end))
        } else if let Some(target) = self.doc_state().and_then(|s| s.thesaurus.as_ref()).and_then(|ts| ts.target_range) {
            Some(target)
        } else if let Some(caret) = self.doc_state().and_then(|s| s.caret) {
            let (start, end) = self.word_at(caret);
            Some((start, end))
        } else {
            None
        };

        let Some((start, end)) = range else {
            if let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) {
                ts.message = Some(text("docx.thesaurus.no-selection-error").to_string());
            }
            cx.notify();
            return;
        };

        // 1ステップの Undo/Redo で置換を実行
        let commands = Self::replacement_commands(&self.resources.markup_view, &doc, start, end, replacement, None).0;
        if !commands.is_empty() {
            self.run_flow(commands, cx);

            // 置換後のテキストを選択状態にする
            let new_end = FlowPos::new(start.para, start.offset + replacement.chars().count());
            self.set_caret(start, false, cx);
            self.set_caret(new_end, true, cx);

            if let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) {
                ts.target_range = Some((start, new_end));
                ts.message = Some(crate::shell::i18n::format(
                    "docx.thesaurus.replaced-msg",
                    &[("word", &replacement)],
                ));
            }
        }
        cx.notify();
    }

    /// クリップボードへ単語をコピー
    pub(crate) fn thesaurus_copy(&mut self, word: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(word.to_string()));
        if let Some(ts) = self.doc_state_mut().and_then(|s| s.thesaurus.as_mut()) {
            ts.message = Some(crate::shell::i18n::format(
                "docx.thesaurus.copied-msg",
                &[("word", &word)],
            ));
        }
        cx.notify();
    }

    /// 類義語辞典サイドバーパネルの描画
    pub fn doc_thesaurus_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?.thesaurus.as_ref()?;
        let query_input = state.query_input.clone();
        let query = state.query.clone();
        let result = state.result.clone();
        let message = state.message.clone();
        let can_back = state.history_index > 0;

        let theme = cx.theme();
        let border_color = theme.border;
        let surface_color = theme.background;

        // ヘッダー（タイトル・戻るボタン・閉じるボタン）
        let header = h_flex()
            .justify_between()
            .items_center()
            .pb(px(8.))
            .border_b_1()
            .border_color(border_color)
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        crate::shell::chrome::action_button("thesaurus-back")

                            .xsmall()
                            .disabled(!can_back)
                            .label("←")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.thesaurus_history_back(window, cx);
                            })),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::BOLD)
                            .child(text("docx.thesaurus.title")),
                    ),
            )
            .child(
                crate::shell::chrome::action_button("thesaurus-close")
                    .xsmall()
                    .label("✕")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_thesaurus_panel(window, cx);
                    })),
            );

        // 検索バー
        let search_bar = h_flex()
            .gap_2()
            .items_center()
            .pt(px(8.))
            .child(div().flex_1().child(crate::shell::chrome::text_field(&query_input).full_width()))
            .child(
                crate::shell::chrome::primary_button("thesaurus-search-btn")
                    .small()
                    .label(text("docx.thesaurus.search-btn"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        let q = this.doc_state()
                            .and_then(|s| s.thesaurus.as_ref())
                            .map(|ts| ts.query_input.read(cx).value().trim().to_string())
                            .unwrap_or_default();
                        this.thesaurus_search(&q, window, cx);
                    })),
            );

        // メッセージ通知
        let msg_view = message.map(|m| {
            div()
                .px(px(8.))
                .py(px(4.))
                .mt(px(4.))
                .rounded_sm()
                .bg(rgb(0xe6f4ea))
                .text_color(rgb(0x137333))
                .text_sm()
                .child(m)
        });

        // 結果一覧または空状態
        let content = if let Some(res) = result {
            let groups_len = res.groups.len();
            let total_words: usize = res.groups.iter().map(|g| g.synonyms.len()).sum();

            let summary = h_flex()
                .justify_between()
                .items_center()
                .py(px(4.))
                .child(
                    div()
                        .text_sm()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("「{}」", res.query)), // i18n-ok（類語辞書のデータ）
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x666666))
                        .child(format!("{groups_len} 分類 / {total_words} 語")), // i18n-ok（類語辞書のデータ）
                );

            let group_elements: Vec<AnyElement> = res.groups.into_iter().enumerate().map(|(g_idx, group)| {
                let category_badge = div()
                    .px(px(6.))
                    .py(px(2.))
                    .rounded_sm()
                    .bg(rgb(0xe8f0fe))
                    .text_color(rgb(0x1a73e8))
                    .text_size(px(10.))
                    .font_weight(FontWeight::BOLD)
                    .child(group.category);

                let meaning_text = div()
                    .text_sm()
                    .text_color(rgb(0x555555))
                    .child(group.meaning);

                let word_buttons: Vec<AnyElement> = group.synonyms.into_iter().enumerate().map(|(w_idx, word)| {
                    let w_insert1 = word.clone();
                    let w_insert2 = word.clone();
                    let w_search = word.clone();
                    let w_copy = word.clone();
                    let item_id = (g_idx * 1000 + w_idx) as u32;

                    h_flex()
                        .justify_between()
                        .items_center()
                        .px(px(8.))
                        .py(px(4.))
                        .rounded_sm()
                        .hover(|s| s.bg(rgb(0xf1f3f4)))
                        .child(
                            crate::shell::chrome::action_button(("thesaurus-w", item_id))

                                .small()
                                .label(word)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.thesaurus_replace(&w_insert1, cx);
                                })),
                        )
                        .child(
                            h_flex()
                                .gap_1()
                                .items_center()
                                .child(
                                    crate::shell::chrome::action_button(("thesaurus-ins", item_id))

                                        .xsmall()
                                        .tooltip(text("docx.thesaurus.insert-tooltip"))
                                        .label(text("docx.thesaurus.insert"))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.thesaurus_replace(&w_insert2, cx);
                                        })),
                                )
                                .child(
                                    crate::shell::chrome::action_button(("thesaurus-copy", item_id))

                                        .xsmall()
                                        .tooltip(text("docx.thesaurus.copy-tooltip"))
                                        .label("📋")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.thesaurus_copy(&w_copy, cx);
                                        })),
                                )
                                .child(
                                    crate::shell::chrome::action_button(("thesaurus-lookup", item_id))

                                        .xsmall()
                                        .tooltip(text("docx.thesaurus.lookup-tooltip"))
                                        .label("🔍")
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.thesaurus_search(&w_search, window, cx);
                                        })),
                                ),
                        )
                        .into_any_element()
                }).collect();

                v_flex()
                    .gap_1()
                    .p(px(8.))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(0xeeeeee))
                    .bg(surface_color)
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(category_badge)
                            .child(meaning_text),
                    )
                    .child(v_flex().gap_0().children(word_buttons))
                    .into_any_element()
            }).collect();

            v_flex()
                .gap_2()
                .pt(px(8.))
                .child(summary)
                .children(group_elements)
                .into_any_element()
        } else if !query.is_empty() {
            v_flex()
                .pt(px(16.))
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x777777))
                        .child(crate::shell::i18n::format(
                            "docx.thesaurus.no-results",
                            &[("word", &query)],
                        )),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x999999))
                        .child(text("docx.thesaurus.hint")),
                )
                .into_any_element()
        } else {
            v_flex()
                .pt(px(16.))
                .gap_2()
                .items_center()
                .child(
                    div()
                        .text_sm()
                        .text_color(rgb(0x777777))
                        .child(text("docx.thesaurus.empty-prompt")),
                )
                .into_any_element()
        };

        let view = v_flex()
            .p(px(12.))
            .gap_2()
            .p_0().child(header)
            .child(search_bar)
            .children(msg_view)
            .child(
                div()
                    .id("thesaurus-scroll-content")
                    .flex_1()
                    .overflow_y_scroll()
                    .child(content),
            );

        Some(SidebarPanel {
            title: text("docx.thesaurus.title").into(),
            body: view.into_any_element(),
        })
    }
}

