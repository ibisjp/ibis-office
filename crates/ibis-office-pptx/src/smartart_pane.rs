// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! SmartArt のテキスト ウィンドウ (PPTX-SMART-009): the sidebar pane of a presentation that lists a diagram's items, with
//! their order, layout and colours, and its menu. The diagrams themselves (insert, layout, colours, the items' edits) are
//! every format's and are in `shell/smartart.rs`.

use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::ShapeId;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::smartart::{SmartArtColorStyle, SmartArtKind};
use crate::ext::*;

/// The colour dropdown shows 5 square swatches; its width is that of the 5 swatches and their gaps (plus the button's own).
const SWATCH_SIDE: f32 = 18.;
const SWATCH_GAP: f32 = 4.;
const SWATCH_ROW_WIDTH: f32 = SWATCH_SIDE * 5. + SWATCH_GAP * 4.;

#[ibis_office_macros::editor_ext(SmartartPane)]
impl SlideEditor {
    /// テキスト ウィンドウ (PPTX-SMART-009): SmartArt テキスト ウィンドウの表示・非表示を切り替え。
    pub fn toggle_smartart_text_pane(&mut self, cx: &mut Context<Self>) {
        let shown = self.pptx().is_some_and(|s| s.smartart_text_pane);
        if let Some(s) = self.pptx_mut() {
            s.smartart_text_pane = !shown;
            s.smartart_reorder = Default::default();
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Select the SmartArt node shape corresponding to item `index` (PPTX-SMART-009).
    pub(crate) fn select_smartart_item(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((_, _, _, leaves, _, _)) = self.selected_smartart_data() else { return };
        let slide = self.slide();
        let item_shapes: Vec<ShapeId> = leaves
            .into_iter()
            .filter(|id| {
                slide.shape(*id).and_then(|s| s.text_body()).is_some_and(|b| {
                    let t = b.text().trim().to_string();
                    !t.is_empty() && !t.starts_with('📷')
                })
            })
            .collect();
        if let Some(&target_shape_id) = item_shapes.get(index) {
            self.set_selection(vec![target_shape_id]);
            cx.notify();
        }
    }

    /// Context menu entries for row `index` in the SmartArt Text Pane (PPTX-SMART-009..014).
    pub(crate) fn smartart_text_pane_context_menu_entries(&self, index: usize) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
        let Some((_, kind, style, _, items, _)) = self.selected_smartart_data() else { return Vec::new() };
        let n = items.len();
        let mut entries: Vec<MenuEntry> = vec![
            MenuItem::tr("pptx.smartart.add-shape").icon("add_box").on(|e, _, cx| e.smartart_add_shape(cx)).into(),
        ];
        if kind == SmartArtKind::OrganizationChart {
            entries.push(MenuItem::tr("pptx.smartart.add-assistant").icon("person_add").on(|e, _, cx| e.smartart_add_assistant(cx)).into());
        }
        entries.push(
            MenuItem::tr("pptx.smartart.remove-shape")
                .icon("indeterminate_check_box")
                .disabled(n <= 1 || index >= n)
                .on(move |e, _, cx| {
                    if let Some((_, kind, style, leaves, mut it, frame)) = e.selected_smartart_data() {
                        if it.len() > 1 && index < it.len() {
                            it.remove(index);
                            let r: Vec<&str> = it.iter().map(String::as_str).collect();
                            let next_sel = index.min(r.len().saturating_sub(1));
                            e.replace_smartart_shapes_with_selection(&leaves, kind, style, &r, frame, Some(next_sel), cx);
                        }
                    }
                })
                .into(),
        );
        entries.push(MenuEntry::Separator);
        entries.push(
            MenuItem::tr("pptx.smartart.promote")
                .icon("format_indent_decrease")
                .disabled(n <= 1 || index == 0 || index >= n)
                .on(move |e, _, cx| {
                    e.select_smartart_item(index, cx);
                    e.smartart_promote(cx);
                })
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.smartart.demote")
                .icon("format_indent_increase")
                .disabled(n <= 1 || index + 1 >= n)
                .on(move |e, _, cx| {
                    e.select_smartart_item(index, cx);
                    e.smartart_demote(cx);
                })
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.smartart.move-up")
                .icon("arrow_upward")
                .disabled(n <= 1 || index == 0 || index >= n)
                .on(move |e, _, cx| {
                    e.select_smartart_item(index, cx);
                    e.smartart_move_item_up(cx);
                })
                .into(),
        );
        entries.push(
            MenuItem::tr("pptx.smartart.move-down")
                .icon("arrow_downward")
                .disabled(n <= 1 || index + 1 >= n)
                .on(move |e, _, cx| {
                    e.select_smartart_item(index, cx);
                    e.smartart_move_item_down(cx);
                })
                .into(),
        );
        entries.push(MenuEntry::Separator);
        let layout_items: Vec<MenuEntry> = [
            (SmartArtKind::BasicBlockList, "pptx.smartart.layout.list", "view_Agenda"),
            (SmartArtKind::VerticalList, "pptx.smartart.layout.vlist", "format_list_bulleted"),
            (SmartArtKind::BasicProcess, "pptx.smartart.layout.process", "trending_flat"),
            (SmartArtKind::ContinuousChevronProcess, "pptx.smartart.layout.chevron", "double_arrow"),
            (SmartArtKind::BasicCycle, "pptx.smartart.layout.cycle", "autorenew"),
            (SmartArtKind::OrganizationChart, "pptx.smartart.layout.hierarchy", "account_tree"),
            (SmartArtKind::BasicRelationship, "pptx.smartart.layout.relationship", "compare_arrows"),
            (SmartArtKind::BasicMatrix, "pptx.smartart.layout.matrix", "grid_view"),
            (SmartArtKind::BasicPyramid, "pptx.smartart.layout.pyramid", "change_history"),
            (SmartArtKind::PictureList, "pptx.smartart.layout.picture", "perm_media"),
        ]
        .into_iter()
        .map(|(k, id, icon)| {
            MenuItem::tr(id)
                .icon(icon)
                .checked(kind == k)
                .on(move |e, _, cx| e.change_smartart_layout(k, cx))
                .into()
        })
        .collect();
        entries.push(Submenu::tr("pptx.smartart.change-layout", "account_tree", layout_items).into());

        let color_items: Vec<MenuEntry> = [
            (SmartArtColorStyle::Colorful, "pptx.smartart.color.colorful"),
            (SmartArtColorStyle::Accent1, "pptx.smartart.color.accent1"),
            (SmartArtColorStyle::Accent2, "pptx.smartart.color.accent2"),
            (SmartArtColorStyle::Accent3, "pptx.smartart.color.accent3"),
            (SmartArtColorStyle::Outline, "pptx.smartart.color.outline"),
        ]
        .into_iter()
        .map(|(st, id)| {
            MenuItem::tr(id)
                .icon("palette")
                .checked(style == st)
                .on(move |e, _, cx| e.set_smartart_color_style(st, cx))
                .into()
        })
        .collect();
        entries.push(Submenu::tr("pptx.smartart.color", "palette", color_items).into());
        entries.push(MenuEntry::Separator);
        entries.push(MenuItem::tr("pptx.smartart-to-shapes").icon("widgets").on(|e, _, cx| e.convert_diagram_to_shapes(cx)).into());
        entries
    }

    /// Open context menu for row `index` in the SmartArt Text Pane at pointer `position`.
    pub(crate) fn open_smartart_text_pane_context_menu(&mut self, index: usize, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.select_smartart_item(index, cx);
        let entries = self.smartart_text_pane_context_menu_entries(index);
        if !entries.is_empty() {
            self.open_popup_entries(entries, position, window, cx);
        }
    }

    /// テキスト ウィンドウ サイドバー パネル (PPTX-SMART-009).
    pub fn smartart_text_pane(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        if !self.pptx()?.smartart_text_pane {
            return None;
        }
        let t = |k| crate::shell::i18n::text(k);
        let title = t("pptx.smartart.text-pane");

        let data = self.selected_smartart_data();
        let selected_idx = self.selected_smartart_item_index();
        let mut body = v_flex().size_full().gap_2().px(px(14.)).py(px(10.)).p_0();

        if let Some((_, kind, style, _leaves, items, _frame)) = data {
            let header = v_flex().w_full().min_w_0().gap_1()
                .child(div().text_sm().font_weight(gpui_kit::FontWeight::BOLD).child(format!("{}: {}", t("pptx.smartart.change-layout"), kind.display_label())))
                .child(div().text_sm().text_color(rgb(0x5f6368)).child(format!("({})", style.label())));
            body = body.child(header);

            let reorder = self.pptx()?.smartart_reorder.clone();
            let table_top = h_flex().w_full().justify_end().child(crate::shell::chrome::action_button("smart-add").small().label(t("pptx.smartart.add-item-short")).on_click(cx.listener(|e, _, _, cx| e.smartart_add_shape(cx))));
            body = body.child(table_top);
            {
                use crate::shell::chrome::data_table::DataTable;
                use crate::shell::chrome::virtual_table::{TableColumn, TableRow};
                let editor = cx.entity().downgrade();
                let items = std::rc::Rc::new(items);
                let count = items.len();
                let build = {
                    let (items, editor) = (items.clone(), editor.clone());
                    move |range: std::ops::Range<usize>, _: &mut Window, _: &mut App| -> Vec<TableRow> {
                        range.filter_map(|i| items.get(i).map(|text| (i, text.clone()))).map(|(i, text)| {
                            let number = div().w_full().text_sm().text_color(rgb(0x5f6368)).child(format!("{}.", i + 1)).into_any_element();
                            let name = div().w_full().min_w_0().overflow_hidden().text_sm().child(text.clone()).into_any_element();
                            let (select, delete) = (editor.clone(), editor.clone());
                            let mut row = TableRow::new(vec![number, name])
                                .selected(selected_idx == Some(i))
                                .on_click(move |_, _, cx| { let _ = select.update(cx, |e, cx| e.select_smartart_item(i, cx)); })
                                .draggable(i as u64, SharedString::from(text.clone()))
                                .drop_between(i as u64);
                            if count > 1 {
                                row = row.action("smart-del", "delete", crate::shell::i18n::text("_shell.delete"), move |_, _, cx| {
                                    let _ = delete.update(cx, |e, cx| {
                                        if let Some((_, kind, style, leaves, mut it, frame)) = e.selected_smartart_data() {
                                            if it.len() > 1 && i < it.len() {it.remove(i); let r: Vec<&str> = it.iter().map(String::as_str).collect();e.replace_smartart_shapes(&leaves, kind, style, &r, frame, cx);}
                                        }
                                    });
                                });
                            }
                            row
                        }).collect()
                    }
                };
                let table = DataTable::reorder_list("pptx-smartart-text-table", vec![TableColumn::fixed("number", 24.), TableColumn::flex("text")], vec![div().into_any_element(), div().into_any_element()], &reorder, count, build, |e, from, to, _, cx| {
                    if let Some((_, kind, style, leaves, mut it, frame)) = e.selected_smartart_data() {
                        if from < it.len() && to < it.len() {let item = it.remove(from); it.insert(to, item);let r: Vec<&str> = it.iter().map(String::as_str).collect();e.replace_smartart_shapes(&leaves, kind, style, &r, frame, cx);}
                    }
                }, cx).headerless().row_actions(1);
                let (right, dnd) = (editor.clone(), reorder.clone());
                body = body.child(div().id("smart-table-host").w_full().flex_1().min_h_0().child(table)
                    .on_mouse_down(MouseButton::Right, move |ev: &MouseDownEvent, window, cx| {
                        if let Some(i) = (0..count).find(|i| dnd.row_bounds(*i).is_some_and(|b| b.contains(&ev.position))) {
                            cx.stop_propagation();
                            let _ = right.update(cx, |e, cx| e.open_smartart_text_pane_context_menu(i, ev.position, window, cx));
                        }
                    }));
            }
            if kind == SmartArtKind::OrganizationChart {
                body = body.child(crate::shell::chrome::action_button("smart-add-asst").small().label(t("pptx.smartart.add-assistant")).on_click(cx.listener(|e, _, _, cx| e.smartart_add_assistant(cx))));
            }
            let styles = [(SmartArtColorStyle::Colorful, "pptx.smartart.color.colorful"), (SmartArtColorStyle::Accent1, "pptx.smartart.color.accent1"), (SmartArtColorStyle::Accent2, "pptx.smartart.color.accent2"), (SmartArtColorStyle::Accent3, "pptx.smartart.color.accent3"), (SmartArtColorStyle::Outline, "pptx.smartart.color.outline")];
            let selected = styles.iter().position(|(value, _)| *value == style).unwrap_or(0);
            let choices = styles.into_iter().map(|(value, key)| crate::shell::chrome::preview_dropdown::PreviewChoice {
                label: t(key),
                content: std::rc::Rc::new(move || h_flex().gap(px(SWATCH_GAP)).children((0..5).map(|i| {let (fill, stroke, _) = value.palette(i);div().size(px(SWATCH_SIDE)).flex_none().rounded(px(3.)).bg(rgb(fill.0)).border_1().border_color(rgb(stroke.unwrap_or(fill).0))})).into_any_element()),
                choose: std::rc::Rc::new(move |e, _, cx| e.set_smartart_color_style(value, cx)),
            }).collect();
            body = body.child(crate::shell::chrome::dropdown_row(t("pptx.smartart.color"), crate::shell::chrome::preview_dropdown::preview_dropdown_sized("pptx-smartart-color", choices, selected, crate::shell::chrome::preview_dropdown::PreviewSize::fitting(SWATCH_ROW_WIDTH), cx)));
        } else {
            body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(t("pptx.smartart.select-prompt")));
        }

        Some(SidebarPanel { title: title.into(), body: body.into_any_element() }.fill())
    }
}
