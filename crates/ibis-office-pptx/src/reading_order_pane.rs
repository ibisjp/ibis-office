// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 読み上げ順序ペイン (校閲 > 読み上げ順序 / アクセシビリティ チェック: PPTX-ACCESS-004):
//! スライド上のオブジェクトがスクリーンリーダーによって読み上げられる順序（上から下へ）を
//! 表示・確認・並べ替え可能にする。装飾用オブジェクトのスキップ表示や、
//! 視覚的配置（タイトル最優先、上から下・左から右）に基づく自動整列もサポート。

use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Command, ShapeId};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;
use crate::ext::*;

/// The Reading Order pane state.
#[derive(Clone, Default)]
pub(crate) struct ReadingOrderPane {
    pub(crate) reorder: crate::shell::chrome::data_table::ListState,
}

#[ibis_office_macros::editor_ext(ReadingOrderPaneExt)]
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
    pub fn open_reading_order_pane(&mut self, cx: &mut Context<Self>) {
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
    pub fn reading_order_pane(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let reorder = self.pptx()?.reading_order_pane.as_ref()?.reorder.clone();
        let t = crate::shell::i18n::text;
        let slide = self.slide().clone();
        let selected = self.selected_ids();
        let shapes = &slide.shapes;
        let mut body = v_flex().size_full().gap_2()
            .child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.reading-order-pane.order-which-objects-will-read")));
        if shapes.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.selection-pane.no-shapes-slide")));
        } else {
            use crate::shell::chrome::data_table::{DataTable, no_drag};
            use crate::shell::chrome::virtual_table::{TableColumn, TableRow};
            let editor = cx.entity().downgrade();
            let ids: std::rc::Rc<Vec<ShapeId>> = std::rc::Rc::new(shapes.iter().map(|s| s.id).collect());
            let labels: std::rc::Rc<Vec<String>> = std::rc::Rc::new(ids.iter().map(|id| self.shape_label(*id)).collect());
            let decorative: std::rc::Rc<Vec<bool>> = std::rc::Rc::new(ids.iter().map(|id| slide.decorative_shapes.contains(id)).collect());
            let selected_of: std::rc::Rc<Vec<bool>> = std::rc::Rc::new(ids.iter().map(|id| selected.contains(id)).collect());
            let count = ids.len();
            let build = {
                let (ids, labels, decorative, selected_of, editor) = (ids.clone(), labels.clone(), decorative.clone(), selected_of.clone(), editor.clone());
                move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                    range.filter_map(|i| ids.get(i).copied().map(|id| (i, id))).map(|(i, id)| {
                        let t = crate::shell::i18n::text;
                        let (n, is_decorative) = (id.0 as usize, decorative[i]);
                        let toggle = editor.clone();
                        let eye = no_drag(crate::shell::chrome::action_button(("ro-decorative", n)).small().w(px(28.))
                            .icon(Icon::new(Material(if is_decorative { "visibility_off" } else { "visibility" })))
                            .tooltip(t("pptx.alt-text.decorative"))
                            .on_click(move |_, _, cx| { cx.stop_propagation(); let _ = toggle.update(cx, |e, cx| e.toggle_shape_decorative(id, cx)); })).into_any_element();
                        let number = div().w(px(22.)).h(px(22.)).flex_none().rounded_full().bg(rgb(0xe8eaed)).text_sm().flex().items_center().justify_center().child((i + 1).to_string()).into_any_element();
                        let name = div().w_full().min_w_0().overflow_hidden().whitespace_nowrap().text_sm()
                            .when(is_decorative, |d| d.text_color(rgb(0x9aa0a6)).line_through()).child(SharedString::from(labels[i].clone())).into_any_element();
                        let select = editor.clone();
                        TableRow::new(vec![eye, number, name])
                            .selected(selected_of[i])
                            .on_click(move |_, _, cx| { let _ = select.update(cx, |e, cx| { e.set_selection(vec![id]); cx.notify(); }); })
                            .draggable(id.0, SharedString::from(labels[i].clone()))
                            .drop_between(id.0)
                    }).collect()
                }
            };
            let table = DataTable::reorder_list("pptx-reading-order-table", vec![TableColumn::fixed("eye", 28.), TableColumn::fixed("number", 26.), TableColumn::flex("name")], vec![div().into_any_element(), div().into_any_element(), div().into_any_element()], &reorder, count, build, |e, from, to, _, cx| {
                let shape = ShapeId(from as u64); let target = ShapeId(to as u64);
                if let Some(index) = e.slide().shape_index(target) {
                    let slide = e.current_slide;
                    e.set_selection(vec![shape]);
                    e.run(vec![Command::MoveShape {slide, shape, index}], cx);
                }
            }, cx).headerless();
            // The right click of a row opens its menu.
            let (right, dnd, ids_for_menu) = (editor.clone(), reorder.clone(), ids.clone());
            body = body.child(div().id("ro-table-host").w_full().flex_1().min_h_0().child(table)
                .on_mouse_down(MouseButton::Right, move |ev: &MouseDownEvent, window, cx| {
                    if let Some(id) = (0..count).find(|i| dnd.row_bounds(*i).is_some_and(|b| b.contains(&ev.position))).map(|i| ids_for_menu[i]) {
                        cx.stop_propagation();
                        let _ = right.update(cx, |e, cx| e.open_reading_order_context_menu(id, ev.position, window, cx));
                    }
                }));
        }

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

        Some(SidebarPanel { title: t("pptx.reading-order-pane.reading-order"), body: body.into_any_element() }.fill())
    }
}
