// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 読み上げ順序ペイン (校閲 > 読み上げ順序 / アクセシビリティ チェック: PPTX-ACCESS-004):
//! スライド上のオブジェクトがスクリーンリーダーによって読み上げられる順序（上から下へ）を
//! 表示・確認・並べ替え可能にする。装飾用オブジェクトのスキップ表示や、
//! 視覚的配置（タイトル最優先、上から下・左から右）に基づく自動整列もサポート。

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, ShapeId, ShapeKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;

/// The Reading Order pane state.
#[derive(Clone, Default)]
pub(crate) struct ReadingOrderPane {
    pub(crate) reorder: std::rc::Rc<std::cell::RefCell<crate::shell::chrome::data_table::RowReorder>>,
}

impl SlideEditor {
    /// 読み上げ順序ペインの開閉トグル
    pub(crate) fn toggle_reading_order_pane(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.reading_order_pane = match s.reading_order_pane.take() {
                Some(_) => None,
                None => Some(ReadingOrderPane::default()),
            };
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 読み上げ順序ペインを開く
    pub(crate) fn open_reading_order_pane(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.reading_order_pane = Some(ReadingOrderPane::default());
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// 読み上げ順序（インデックス）を 1 つ移動する
    /// `earlier == true`: より早く読まれる（上へ移動、インデックス - 1）
    /// `earlier == false`: より遅く読まれる（下へ移動、インデックス + 1）
    pub(crate) fn move_reading_order(&mut self, shape: ShapeId, earlier: bool, cx: &mut Context<Self>) {
        let slide_id = self.current_slide;
        let slide = self.slide();
        let Some(curr_idx) = slide.shape_index(shape) else { return };
        let new_idx = if earlier {
            if curr_idx == 0 { return; }
            curr_idx - 1
        } else {
            if curr_idx + 1 >= slide.shapes.len() { return; }
            curr_idx + 1
        };
        self.set_selection(vec![shape]);
        self.run(vec![Command::MoveShape { slide: slide_id, shape, index: new_idx }], cx);
    }

    /// 視覚的配置に基づいて論理的な読み上げ順序に自動整列する
    /// 1. タイトルプレースホルダーが最初
    /// 2. サブタイトル / プレースホルダー
    /// 3. 一般図形・コンテンツ: Y座標昇順（上から下）、おおむね同一行ならX座標昇順（左から右）
    /// 4. 装飾用オブジェクト（読み上げ不要）は末尾へ
    pub(crate) fn auto_reading_order(&mut self, cx: &mut Context<Self>) {
        let slide_id = self.current_slide;
        let slide = self.slide().clone();
        if slide.shapes.len() <= 1 { return; }

        let mut ranked: Vec<(usize, ShapeId)> = slide
            .shapes
            .iter()
            .enumerate()
            .map(|(original_idx, sh)| {
                let is_decorative = slide.decorative_shapes.contains(&sh.id);
                let ph = slide.placeholders.get(&sh.id).map(|p| p.placeholder.normalized());
                let is_title = ph == Some("title") || ph == Some("ctrTitle");
                let is_sub_title = ph == Some("subTitle");

                // スコア計算:
                // 装飾用: 1_000_000_000 + original_idx
                // タイトル: 0
                // サブタイトル: 10_000
                // 通常: 100_000 + (Y / 200_000) * 10_000 + (X / 200_000)
                let score: i64 = if is_decorative {
                    1_000_000_000 + original_idx as i64
                } else if is_title {
                    0
                } else if is_sub_title {
                    10_000
                } else {
                    let y = (sh.frame.y / 200_000).max(0);
                    let x = (sh.frame.x / 200_000).max(0);
                    100_000 + y * 10_000 + x
                };
                (score as usize, sh.id)
            })
            .collect();

        // 安定ソート
        ranked.sort_by_key(|&(score, _)| score);

        let target_order: Vec<ShapeId> = ranked.into_iter().map(|(_, id)| id).collect();
        let current_order: Vec<ShapeId> = slide.shapes.iter().map(|s| s.id).collect();
        if target_order == current_order { return; }

        // 現在の shapes から target_order の順序へ並べ替える MoveShape の列を生成
        let mut sim = current_order;
        let mut commands = Vec::new();
        for (target_idx, &id) in target_order.iter().enumerate() {
            let current_pos = sim.iter().position(|&x| x == id).unwrap();
            if current_pos != target_idx {
                sim.remove(current_pos);
                sim.insert(target_idx, id);
                commands.push(Command::MoveShape { slide: slide_id, shape: id, index: target_idx });
            }
        }

        if !commands.is_empty() {
            self.run(commands, cx);
        }
    }

    /// 図形の「装飾としてマーク（読み上げスキップ）」フラグをトグル切り替えする
    pub(crate) fn toggle_shape_decorative(&mut self, shape: ShapeId, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let is_decorative = self.slide().decorative_shapes.contains(&shape);
        self.set_selection(vec![shape]);
        self.run(vec![Command::SetShapeDecorative { slide, shape, decorative: !is_decorative }], cx);
    }

    /// Context menu entries for a shape row in the Reading Order Pane.
    pub(crate) fn reading_order_context_menu_entries(&self, id: ShapeId) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let slide = self.slide();
        let idx = slide.shape_index(id).unwrap_or(0);
        let count = slide.shapes.len();
        let is_decorative = slide.decorative_shapes.contains(&id);
        vec![
            MenuItem::tr("pptx.reading-order-pane.move-up-read-earlier")
                .icon("arrow_upward")
                .disabled(idx == 0)
                .on(move |e, _, cx| e.move_reading_order(id, true, cx))
                .into(),
            MenuItem::tr("pptx.reading-order-pane.move-down-read-later")
                .icon("arrow_downward")
                .disabled(idx + 1 >= count)
                .on(move |e, _, cx| e.move_reading_order(id, false, cx))
                .into(),
            MenuItem::tr("pptx.reading-order-pane.auto-order-top-bottom")
                .icon("format_list_numbered")
                .disabled(count <= 1)
                .on(|e, _, cx| e.auto_reading_order(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.alt-text.decorative")
                .icon("visibility_off")
                .checked(is_decorative)
                .on(move |e, _, cx| e.toggle_shape_decorative(id, cx))
                .into(),
            MenuItem::tr("pptx.alt-text")
                .icon("accessibility")
                .on(move |e, w, cx| {
                    e.set_selection(vec![id]);
                    e.open_alt_text_panel(w, cx);
                })
                .into(),
        ]
    }

    /// Open context menu for shape `id` in the Reading Order Pane at pointer `position`.
    pub(crate) fn open_reading_order_context_menu(&mut self, id: ShapeId, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if !self.selected_ids().contains(&id) {
            self.set_selection(vec![id]);
            cx.notify();
        }
        let entries = self.reading_order_context_menu_entries(id);
        self.open_popup_entries(entries, position, window, cx);
    }

    /// 読み上げ順序サイドバーパネルの描画
    pub(crate) fn reading_order_pane(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let reorder = self.pptx()?.reading_order_pane.as_ref()?.reorder.clone();
        let t = crate::shell::i18n::text;
        let slide = self.slide().clone();
        let selected = self.selected_ids();
        let shapes = &slide.shapes;
        let mut body = v_flex().gap_2()
            .child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.reading-order-pane.order-which-objects-will-read")));
        if shapes.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.selection-pane.no-shapes-slide")));
        }
        let mut rows = Vec::new();
        for (i, shape) in shapes.iter().enumerate() {
            let id = shape.id;
            let n = id.0 as usize;
            let is_decorative = slide.decorative_shapes.contains(&id);
            let is_selected = selected.contains(&id);
            let eye = crate::shell::chrome::action_button(("ro-decorative", n)).small().w(px(28.))
                .icon(Icon::new(Material(if is_decorative { "visibility_off" } else { "visibility" })))
                .tooltip(t("pptx.alt-text.decorative"))
                .on_click(cx.listener(move |e, _, _, cx| {cx.stop_propagation(); e.toggle_shape_decorative(id, cx);}));
            let row = h_flex().id(("reading-order-row", n)).w_full().h_full().min_w_0().items_center().gap_1().px_1()
                .on_click(cx.listener(move |e, _, _, cx| {e.set_selection(vec![id]); cx.notify();}))
                .on_mouse_down(MouseButton::Right, cx.listener(move |e, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation(); e.open_reading_order_context_menu(id, event.position, window, cx);
                }))
                .child(crate::shell::chrome::data_table::row_controls(reorder.clone(), n, 0, eye))
                .child(div().w(px(22.)).h(px(22.)).flex_none().rounded_full().bg(rgb(0xe8eaed)).text_sm().flex().items_center().justify_center().child((i + 1).to_string()))
                .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_sm()
                    .when(is_decorative, |d| d.text_color(rgb(0x9aa0a6)).line_through()).child(self.shape_label(id)))
                .child(crate::shell::chrome::icon("menu", 18., rgb(0x5f6368).into()).cursor_grab());
            rows.push((n, row.into_any_element(), is_selected));
        }
        let table = crate::shell::chrome::data_table::DataTable::new("pptx-reading-order-table", None, rows, cx)
            .reorderable(reorder, |e, from, to, _, cx| {
                let shape = ShapeId(from as u64); let target = ShapeId(to as u64);
                if let Some(index) = e.slide().shape_index(target) {
                    let slide = e.current_slide;
                    e.set_selection(vec![shape]);
                    e.run(vec![Command::MoveShape {slide, shape, index}], cx);
                }
            });
        body = body.child(table);

        // アクションバー
        body = body.child(
            h_flex()
                .gap_1()
                .pt_2()
                .child(
                    crate::shell::chrome::action_button("ro-auto-order")
                        .small()
                        .label(t("pptx.reading-order-pane.auto-order-top-bottom"))
                        .tooltip(t("pptx.reading-order-pane.automatically-orders-reading-order"))
                        .on_click(cx.listener(|e, _, _, cx| e.auto_reading_order(cx))),
                )
                .child(
                    crate::shell::chrome::action_button("ro-to-selection-pane")
                        .small()

                        .label(t("pptx.reading-order-pane.selection-pane"))
                        .tooltip(t("pptx.reading-order-pane.open-selection-pane-manage-z"))
                        .on_click(cx.listener(|e, _, _, cx| {
                            e.toggle_selection_pane(cx);
                        })),
                ),
        );

        Some(SidebarPanel { title: t("pptx.reading-order-pane.reading-order"), body: body.into_any_element() })
    }
}
