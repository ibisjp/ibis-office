// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Page Layout view (表示 > ページ レイアウト): the sheet cut into the pages it prints on, each on its own
//! sheet of paper with the margins and the header and footer, the papers laid out as they print (down then
//! over, or over then down), a gap between them. Each page's cells are the sheet drawn again, clipped to the
//! page's part and moved to its place (as the frozen panes are), at the scale the sheet prints at, so
//! everything the sheet shows (the selected cell, charts, pictures) shows here too and a press on a cell
//! of a page selects it.

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Command, EMU_PER_INCH, EMU_PER_POINT, SheetCommand};

use crate::xlsx::header_footer_edit::HfActiveSection;
use crate::shell::editor::{RulerUnit, SlideEditor, SlideLayout, WorkspaceContent};

/// The gap between two papers and around all of them, in inches.
const GAP_INCHES: f32 = 0.25;
/// The room for the rulers above and beside the papers, in pixels.
const RULER: f32 = 20.0;

/// An edge of the page margin being adjusted on the ruler.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RulerMargin {
    Left,
    Right,
    Top,
    Bottom,
}

/// A margin handle being dragged on the ruler.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RulerMarginDrag {
    pub(crate) margin: RulerMargin,
    pub(crate) start_point: Point<Pixels>,
    pub(crate) current_point: Point<Pixels>,
    pub(crate) initial_margins: [ibis_doc_model::Emu; 4],
    pub(crate) scale: f32,
    pub(crate) paper_size: (ibis_doc_model::Emu, ibis_doc_model::Emu),
}

/// One page of the view: where its paper and its cells are in the scrolled content (pixels), and where the
/// slide's own origin would be for its cells.
#[derive(Clone, Debug)]
pub(crate) struct PageBox {
    /// Its number from 0, in printing order.
    pub(crate) index: usize,
    /// The paper: left, top, width, height.
    pub(crate) paper: (f32, f32, f32, f32),
    /// The page's cells' window: left, top, width, height.
    pub(crate) window: (f32, f32, f32, f32),
    /// Where the slide's (0, 0) is, so that the page's cells are in the window.
    pub(crate) slide_origin: (f32, f32),
    /// The part of the slide the page prints: left, top, width, height in EMU.
    pub(crate) part: (i64, i64, i64, i64),
}

/// All the pages and the size of what holds them.
#[derive(Clone, Debug)]
pub(crate) struct PageGeometry {
    pub(crate) boxes: Vec<PageBox>,
    pub(crate) size: (f32, f32),
    /// Pixels per EMU of the cells (the sheet's scale on screen times the print scale).
    pub(crate) cell_scale: f32,
}

impl SlideEditor {
    /// Whether the current sheet is laid out in pages on screen (the page layout view, in a build that can
    /// work the pages out).
    pub(crate) fn page_layout_shown(&self) -> bool {
        cfg!(all(any(target_os = "macos", target_os = "windows"), feature = "export")) && self.current_sheet().is_some_and(|(_, s)| s.page_layout)
    }

    /// The pages of the current worksheet at `scale` (pixels per EMU of paper): `None` when the sheet is not
    /// in the page layout view, or when its pages cannot be worked out here (no exporter in this build).
    pub(crate) fn page_layout_geometry(&self, scale: f32) -> Option<PageGeometry> {
        let (_, sheet) = self.current_sheet()?;
        if !sheet.page_layout {
            return None;
        }
        #[cfg(all(any(target_os = "macos", target_os = "windows"), feature = "export"))]
        let (print_scale, pages) = crate::export::preview_pages(self.presentation(), self.slide(), &self.resources);
        #[cfg(not(all(any(target_os = "macos", target_os = "windows"), feature = "export")))]
        let (print_scale, pages): (f64, Vec<ibis_doc_model::EmuRect>) = (1.0, Vec::new());
        if pages.is_empty() {
            return None;
        }
        let print = &sheet.print;
        let (paper_w, paper_h) = print.page_size();
        let (pw, ph) = (paper_w as f32 * scale, paper_h as f32 * scale);
        let gap = GAP_INCHES * EMU_PER_INCH as f32 * scale;
        let cell_scale = scale * print_scale as f32;
        // The pages sit in a grid by where they start: the columns of papers by their left edge, the rows by their top.
        let mut xs: Vec<i64> = pages.iter().map(|p| p.x).collect();
        let mut ys: Vec<i64> = pages.iter().map(|p| p.y).collect();
        xs.sort_unstable();
        xs.dedup();
        ys.sort_unstable();
        ys.dedup();
        let [ml, mr, mt, mb] = print.margins.map(|m| m as f32 * scale);
        let ruler = if sheet.show_ruler { RULER } else { 0.0 };
        let mut boxes = Vec::with_capacity(pages.len());
        for (index, page) in pages.iter().enumerate() {
            let column = xs.iter().position(|x| *x == page.x).unwrap_or(0);
            let row = ys.iter().position(|y| *y == page.y).unwrap_or(0);
            let (left, top) = (ruler + gap + column as f32 * (pw + gap), ruler + gap + row as f32 * (ph + gap));
            let (cw, ch) = (page.width as f32 * cell_scale, page.height as f32 * cell_scale);
            // The cells' room is the paper less its margins; centred across or down if the setup says so.
            let (room_w, room_h) = ((pw - ml - mr).max(cw), (ph - mt - mb).max(ch));
            let wx = left + ml + if print.centered.0 { (room_w - cw) / 2.0 } else { 0.0 };
            let wy = top + mt + if print.centered.1 { (room_h - ch) / 2.0 } else { 0.0 };
            boxes.push(PageBox {
                index,
                paper: (left, top, pw, ph),
                window: (wx, wy, cw, ch),
                slide_origin: (wx - page.x as f32 * cell_scale, wy - page.y as f32 * cell_scale),
                part: (page.x, page.y, page.width, page.height),
            });
        }
        let width = ruler + gap + xs.len() as f32 * (pw + gap);
        let height = ruler + gap + ys.len() as f32 * (ph + gap);
        Some(PageGeometry { boxes, size: (width, height), cell_scale })
    }

    /// The scale of the sheet on screen (pixels per EMU) when not laid out in pages.
    fn page_layout_base_scale(&self, view: Size<Pixels>) -> Option<f32> {
        let (_, sheet) = self.current_sheet()?;
        let mut page = self.presentation().size_of(self.slide());
        if let Some((shape, _)) = self.current_sheet() {
            page.width = page.width.max(shape.frame.right());
            page.height = page.height.max(shape.frame.bottom());
        }
        let _ = sheet;
        Some(self.page_content(view, page, point(0.0, 0.0), point(0.0, 0.0), true).scale)
    }

    /// The content of the workspace in the page layout view: the papers, scrolled over.
    pub(crate) fn page_layout_content(&self, view: Size<Pixels>) -> Option<WorkspaceContent> {
        let scale = self.page_layout_base_scale(view)?;
        let geometry = self.page_layout_geometry(scale)?;
        let (width, height) = (f32::from(view.width).max(geometry.size.0), f32::from(view.height).max(geometry.size.1));
        Some(WorkspaceContent { size: size(px(width), px(height)), slide_origin: point(px(0.0), px(0.0)), scale })
    }

    /// Where the slide would be, and at what scale, for the page of the view under the window position
    /// `position` (nothing when it is over no page's cells).
    pub(crate) fn page_layout_at(&self, position: Point<Pixels>) -> Option<SlideLayout> {
        let ws = self.workspace_bounds?;
        let content = self.workspace_content()?;
        let geometry = self.page_layout_geometry(content.scale)?;
        let offset = self.workspace_scroll.offset();
        let local = position - ws.origin - offset;
        let (x, y) = (f32::from(local.x), f32::from(local.y));
        let hit = geometry.boxes.iter().find(|b| x >= b.window.0 && y >= b.window.1 && x < b.window.0 + b.window.2 && y < b.window.1 + b.window.3)?;
        Some(SlideLayout { origin: ws.origin + offset + point(px(hit.slide_origin.0), px(hit.slide_origin.1)), scale: geometry.cell_scale })
    }

    /// The window position of the middle of a cell in the page layout view (the first page that shows it).
    #[allow(dead_code)] // For the scenario tests.
    pub(crate) fn page_layout_cell_center(&self, column: u32, row: u32) -> Option<Point<Pixels>> {
        let ws = self.workspace_bounds?;
        let content = self.workspace_content()?;
        let geometry = self.page_layout_geometry(content.scale)?;
        let (shape, sheet) = self.current_sheet()?;
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let x = shape.frame.x + (xs.get(column as usize)? + xs.get(column as usize + 1)?) / 2;
        let y = shape.frame.y + (ys.get(row as usize)? + ys.get(row as usize + 1)?) / 2;
        let offset = self.workspace_scroll.offset();
        for b in &geometry.boxes {
            let (cx_, cy_) = (b.slide_origin.0 + x as f32 * geometry.cell_scale, b.slide_origin.1 + y as f32 * geometry.cell_scale);
            if cx_ >= b.window.0 && cy_ >= b.window.1 && cx_ < b.window.0 + b.window.2 && cy_ < b.window.1 + b.window.3 {
                return Some(ws.origin + offset + point(px(cx_), px(cy_)));
            }
        }
        None
    }

    /// Where a cell is in the page layout view's content (left, top, right, bottom in pixels, on the first
    /// page that shows it).
    pub(crate) fn page_layout_cell_span(&self, column: u32, row: u32, scale: f32) -> Option<(f32, f32, f32, f32)> {
        let geometry = self.page_layout_geometry(scale)?;
        let (shape, sheet) = self.current_sheet()?;
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let at = |o: &[i64], i: usize| *o.get(i.min(o.len().saturating_sub(1))).unwrap_or(&0);
        let (x0, x1) = (shape.frame.x + at(&xs, column as usize), shape.frame.x + at(&xs, column as usize + 1));
        let (y0, y1) = (shape.frame.y + at(&ys, row as usize), shape.frame.y + at(&ys, row as usize + 1));
        let cs = geometry.cell_scale;
        for b in &geometry.boxes {
            let (cx_, cy_) = (b.slide_origin.0 + (x0 + x1) as f32 / 2.0 * cs, b.slide_origin.1 + (y0 + y1) as f32 / 2.0 * cs);
            if cx_ >= b.window.0 && cy_ >= b.window.1 && cx_ < b.window.0 + b.window.2 && cy_ < b.window.1 + b.window.3 {
                return Some((b.slide_origin.0 + x0 as f32 * cs, b.slide_origin.1 + y0 as f32 * cs, b.slide_origin.0 + x1 as f32 * cs, b.slide_origin.1 + y1 as f32 * cs));
            }
        }
        None
    }

    /// Scroll the page layout view so the selected cell is in view (as little as it takes).
    pub(crate) fn page_layout_scroll_into_view(&self) {
        let (Some(content), Some(ws), Some(range)) = (self.workspace_content(), self.workspace_bounds, self.selected_range()) else { return };
        let Some((l, t, r, b)) = self.page_layout_cell_span(range.first_column, range.first_row, content.scale) else { return };
        let offset = self.workspace_scroll.offset();
        let axis = |off: f32, start: f32, end: f32, view: f32, total: f32| {
            let visible = -off;
            let visible = if start < visible { start } else if end > visible + view { (end - view).min(start) } else { visible };
            -visible.clamp(0.0, (total - view).max(0.0))
        };
        let x = axis(f32::from(offset.x), l, r, f32::from(ws.size.width), f32::from(content.size.width));
        let y = axis(f32::from(offset.y), t, b, f32::from(ws.size.height), f32::from(content.size.height));
        self.workspace_scroll.set_offset(point(px(x), px(y)));
    }

    /// The papers with their margins, header and footer and the sheet's cells in them.
    pub(crate) fn render_page_layout(&self, content: WorkspaceContent, cx: &mut Context<Self>) -> Option<Div> {
        let geometry = self.page_layout_geometry(content.scale)?;
        let (_, sheet) = self.current_sheet()?;
        let pages = geometry.boxes.len() as u32;
        let default_style = self.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default();
        let file = self.file_name.as_ref().map(|n| n.to_string()).unwrap_or_default();
        let sheet_name = self.slide().name.clone().unwrap_or_default();
        let (slide_w, slide_h) = {
            let mut page = self.presentation().size_of(self.slide());
            if let Some((shape, _)) = self.current_sheet() {
                page.width = page.width.max(shape.frame.right());
                page.height = page.height.max(shape.frame.bottom());
            }
            (page.width as f32 * geometry.cell_scale, page.height as f32 * geometry.cell_scale)
        };
        let point_px = content.scale * EMU_PER_POINT as f32;
        let [_, _, margin_top, margin_bottom] = sheet.print.margins.map(|m| m as f32 * content.scale);
        let mut papers: Vec<AnyElement> = Vec::new();
        for b in &geometry.boxes {
            let (px_, py_, pw, ph) = b.paper;
            // The header and footer, where the page prints them (points from the paper's corner).
            let marks = crate::xlsx::header_footer::page_marks(&sheet.print, &default_style, 1.0, b.index as u32 + 1, pages, &sheet_name, &file, &self.resources);
            let texts: Vec<AnyElement> = marks
                .iter()
                .map(|m| {
                    let size = m.run.size * point_px;
                    div()
                        .absolute()
                        .left(px(m.x * point_px))
                        .top(px(m.baseline * point_px - size * 0.85))
                        .text_size(px(size))
                        .text_color(rgb(m.run.color.map_or(0x000000, |c| ((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32)))
                        .font(m.font.clone())
                        .when(m.run.bold, |d| d.font_weight(FontWeight::BOLD))
                        .when(m.run.italic, |d| d.italic())
                        .whitespace_nowrap()
                        .child(SharedString::from(m.run.text.clone()))
                        .into_any_element()
                })
                .collect();
            let (wx, wy, ww, wh) = b.window;
            let headings = if sheet.headings { self.page_headings(b, geometry.cell_scale, px_, py_) } else { Vec::new() };
            let inner = SlideLayout { origin: point(px(0.0), px(0.0)), scale: geometry.cell_scale };
            let inner_content = WorkspaceContent { size: size(px(slide_w), px(slide_h)), slide_origin: point(px(0.0), px(0.0)), scale: geometry.cell_scale };
            papers.push(
                div()
                    .absolute()
                    .left(px(px_))
                    .top(px(py_))
                    .w(px(pw))
                    .h(px(ph))
                    .bg(rgb(0xffffff))
                    .shadow_md()
                    .children(texts)
                    .children(headings)
                    .child(Self::header_footer_zone(b.index, true, pw, margin_top, cx))
                    .child(Self::header_footer_zone(b.index, false, pw, margin_bottom, cx))
                    .children(self.header_footer_inputs(b.index, pw, margin_top, margin_bottom))
                    .child(
                        div()
                            .absolute()
                            .left(px(wx - px_))
                            .top(px(wy - py_))
                            .w(px(ww))
                            .h(px(wh))
                            .overflow_hidden()
                            .child(
                                div()
                                    .absolute()
                                    .left(px(b.slide_origin.0 - wx))
                                    .top(px(b.slide_origin.1 - wy))
                                    .w(px(slide_w))
                                    .h(px(slide_h))
                                    .child(self.render_slide_for_pages(inner, inner_content, cx)),
                            ),
                    )
                    .into_any_element(),
            );
        }
        if sheet.show_ruler {
            papers.extend(self.render_rulers(&geometry, &sheet.print, content.scale, cx));
        }
        Some(div().relative().flex_none().w(content.size.width).h(content.size.height).children(papers))
    }

    /// The strip of a paper's top (or bottom) margin where the header (footer) is: a press on it opens the
    /// header and footer editor.
    fn header_footer_zone(index: usize, header: bool, width: f32, height: f32, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::xlsx::header_footer_edit::HfTarget;
        let target = if header { HfTarget::Header } else { HfTarget::Footer };
        div()
            .id(SharedString::from(format!("page-layout-{}-{index}", if header { "header" } else { "footer" })))
            .absolute()
            .left(px(0.0))
            .when(header, |d| d.top(px(0.0)))
            .when(!header, |d| d.bottom(px(0.0)))
            .w(px(width))
            .h(px(height))
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, window, cx| {
                    // The third of the paper's width the press is in is the section to type in.
                    let section = this
                        .page_layout_paper_x(index, e.position)
                        .map_or(HfActiveSection::Center, |t| if t < 1.0 / 3.0 { HfActiveSection::Left } else if t > 2.0 / 3.0 { HfActiveSection::Right } else { HfActiveSection::Center });
                    this.open_header_footer_edit_at(target, Some(index), section, window, cx);
                    cx.stop_propagation();
                }),
            )
    }

    /// A page's column letters above its cells and row numbers beside them (in the margins, as Excel's
    /// page layout view puts them), each over the width or height of its column or row on this page.
    fn page_headings(&self, b: &PageBox, cell_scale: f32, paper_x: f32, paper_y: f32) -> Vec<AnyElement> {
        let Some((shape, sheet)) = self.current_sheet() else { return Vec::new() };
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let (wx, wy, ww, wh) = b.window;
        let (strip_h, strip_w) = (16.0f32, 30.0f32);
        let mut out: Vec<AnyElement> = Vec::new();
        let cell = |left: f32, top: f32, w: f32, h: f32, text: String| -> AnyElement {
            div()
                .absolute()
                .left(px(left - paper_x))
                .top(px(top - paper_y))
                .w(px(w))
                .h(px(h))
                .bg(rgb(0xe6e6e6))
                .border_1()
                .border_color(rgb(0xbdbdbd))
                .flex()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .text_size(px(11.0))
                .text_color(rgb(0x444444))
                .child(SharedString::from(text))
                .into_any_element()
        };
        let (px0, py0, pw, ph) = b.part;
        for column in 0..xs.len().saturating_sub(1) {
            let (a, z) = (shape.frame.x + xs[column], shape.frame.x + xs[column + 1]);
            let (a, z) = (a.max(px0), z.min(px0 + pw));
            if z <= a {
                continue;
            }
            let left = b.slide_origin.0 + a as f32 * cell_scale;
            let width = (z - a) as f32 * cell_scale;
            if left + 0.5 < wx - 0.5 || left > wx + ww {
                continue;
            }
            out.push(cell(left, wy - strip_h, width, strip_h, crate::xlsx::sheet_ui::column_letters(column as u32)));
        }
        for row in 0..ys.len().saturating_sub(1) {
            let (a, z) = (shape.frame.y + ys[row], shape.frame.y + ys[row + 1]);
            let (a, z) = (a.max(py0), z.min(py0 + ph));
            if z <= a {
                continue;
            }
            let top = b.slide_origin.1 + a as f32 * cell_scale;
            let height = (z - a) as f32 * cell_scale;
            if top < wy - 0.5 || top > wy + wh {
                continue;
            }
            out.push(cell(wx - strip_w, top, strip_w, height, (row + 1).to_string()));
        }
        out
    }

    /// How far across paper `index` the window position `position` is (0 at its left edge, 1 at its right).
    fn page_layout_paper_x(&self, index: usize, position: Point<Pixels>) -> Option<f32> {
        let ws = self.workspace_bounds?;
        let content = self.workspace_content()?;
        let geometry = self.page_layout_geometry(content.scale)?;
        let b = geometry.boxes.get(index)?;
        let x = f32::from((position - ws.origin - self.workspace_scroll.offset()).x);
        Some(((x - b.paper.0) / b.paper.2).clamp(0.0, 1.0))
    }

    /// The three sections of the header or footer being edited in place on paper `index`, as boxes over
    /// its top or bottom margin.
    fn header_footer_inputs(&self, index: usize, width: f32, margin_top: f32, margin_bottom: f32) -> Vec<AnyElement> {
        use crate::xlsx::header_footer_edit::HfTarget;
        use gpui_kit::component::input::Input;
        let Some(panel) = self.sheet_state().and_then(|s| s.header_footer_edit.as_ref()).filter(|p| p.inline_page == Some(index) && self.sidebar_open) else { return Vec::new() };
        let header = panel.target == HfTarget::Header;
        let height = if header { margin_top } else { margin_bottom }.clamp(24.0, 60.0);
        let third = (width - 24.0) / 3.0;
        [&panel.left, &panel.center, &panel.right]
            .into_iter()
            .enumerate()
            .map(|(i, state)| {
                div()
                    .absolute()
                    .left(px(12.0 + i as f32 * third))
                    .when(header, |d| d.top(px(2.0)))
                    .when(!header, |d| d.bottom(px(2.0)))
                    .w(px(third - 4.0))
                    .h(px(height))
                    .bg(rgb(0xffffff))
                    .child(Input::new(state))
                    .into_any_element()
            })
            .collect()
    }

    /// The rulers: along the top over each column of papers and down the left beside each row of them,
    /// in the selected ruler unit (centimetres/inches/etc.), zero at the paper's edge, the margins shaded,
    /// with interactive drag handles to adjust page margins.
    fn render_rulers(
        &self,
        geometry: &PageGeometry,
        print: &ibis_doc_model::SheetPrint,
        scale: f32,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let unit = self.ruler_unit;
        let unit_emu = unit.emu_per_unit();
        let (step_emu, per_label) = match unit {
            RulerUnit::Centimeters => (unit_emu / 2.0, 2),
            RulerUnit::Millimeters => (unit_emu, 10),
            RulerUnit::Inches => (unit_emu / 8.0, 8),
            RulerUnit::Points => (unit_emu * 10.0, 5),
            RulerUnit::Picas => (unit_emu, 1),
        };
        let step = step_emu * scale;
        let drag = self.sheet_state().and_then(|s| s.ruler_margin_drag);
        let margins_emu = drag.as_ref().map_or(print.margins, |d| Self::calculate_dragged_margins(d));
        let [ml, mr, mt, mb] = margins_emu.map(|m| m as f32 * scale);
        let mut out: Vec<AnyElement> = Vec::new();
        let band = |left: f32, top: f32, w: f32, h: f32| div().absolute().left(px(left)).top(px(top)).w(px(w)).h(px(h)).bg(rgb(0xf3f3f3)).border_1().border_color(rgb(0xd0d0d0));
        let shade = |left: f32, top: f32, w: f32, h: f32| div().absolute().left(px(left)).top(px(top)).w(px(w)).h(px(h)).bg(rgb(0xc8c8c8));
        let tick = |left: f32, top: f32, w: f32, h: f32| div().absolute().left(px(left)).top(px(top)).w(px(w)).h(px(h)).bg(rgb(0x666666));
        let label = |left: f32, top: f32, text: String| div().absolute().left(px(left)).top(px(top)).text_size(px(9.0)).text_color(rgb(0x444444)).whitespace_nowrap().child(SharedString::from(text));

        // Corner box at (0, 0) cycling ruler units on click
        let corner = div()
            .id("page-layout-ruler-corner")
            .absolute()
            .left(px(0.0))
            .top(px(0.0))
            .w(px(RULER - 2.0))
            .h(px(RULER - 2.0))
            .bg(rgb(0xf3f3f3))
            .border_1()
            .border_color(rgb(0xd0d0d0))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(8.0))
            .text_color(rgb(0x666666))
            .cursor_pointer()
            .child(unit.abbreviation())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.cycle_ruler_unit(cx);
                }),
            );
        out.push(corner.into_any_element());

        let mut done_columns: Vec<i32> = Vec::new();
        let mut done_rows: Vec<i32> = Vec::new();
        for b in &geometry.boxes {
            let (px_, py_, pw, ph) = b.paper;
            if !done_columns.contains(&(px_ as i32)) {
                done_columns.push(px_ as i32);
                out.push(band(px_, 0.0, pw, RULER - 2.0).into_any_element());
                out.push(shade(px_, 1.0, ml.min(pw), RULER - 4.0).into_any_element());
                out.push(shade(px_ + pw - mr.min(pw), 1.0, mr.min(pw), RULER - 4.0).into_any_element());
                let mut i = 0;
                while (i as f32) * step <= pw {
                    let x = px_ + i as f32 * step;
                    let long = i % per_label == 0;
                    out.push(tick(x, if long { 10.0 } else { 14.0 }, 1.0, if long { 6.0 } else { 3.0 }).into_any_element());
                    if long && i > 0 {
                        let val = match unit {
                            RulerUnit::Points => (i / per_label) * 50,
                            _ => i / per_label,
                        };
                        out.push(label(x + 2.0, 1.0, val.to_string()).into_any_element());
                    }
                    i += 1;
                }

                // Left margin drag handle on top ruler
                let h_left = div()
                    .id("page-layout-margin-left")
                    .absolute()
                    .left(px(px_ + ml - 3.0))
                    .top(px(0.0))
                    .w(px(6.0))
                    .h(px(RULER - 2.0))
                    .cursor_col_resize()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().w(px(1.5)).h(px(RULER - 4.0)).bg(rgb(0x777777)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                            this.begin_ruler_margin_drag(RulerMargin::Left, e.position, scale, cx);
                            cx.stop_propagation();
                        }),
                    );
                out.push(h_left.into_any_element());

                // Right margin drag handle on top ruler
                let h_right = div()
                    .id("page-layout-margin-right")
                    .absolute()
                    .left(px(px_ + pw - mr - 3.0))
                    .top(px(0.0))
                    .w(px(6.0))
                    .h(px(RULER - 2.0))
                    .cursor_col_resize()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().w(px(1.5)).h(px(RULER - 4.0)).bg(rgb(0x777777)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                            this.begin_ruler_margin_drag(RulerMargin::Right, e.position, scale, cx);
                            cx.stop_propagation();
                        }),
                    );
                out.push(h_right.into_any_element());
            }
            if !done_rows.contains(&(py_ as i32)) {
                done_rows.push(py_ as i32);
                out.push(band(0.0, py_, RULER - 2.0, ph).into_any_element());
                out.push(shade(1.0, py_, RULER - 4.0, mt.min(ph)).into_any_element());
                out.push(shade(1.0, py_ + ph - mb.min(ph), RULER - 4.0, mb.min(ph)).into_any_element());
                let mut i = 0;
                while (i as f32) * step <= ph {
                    let y = py_ + i as f32 * step;
                    let long = i % per_label == 0;
                    out.push(tick(if long { 10.0 } else { 14.0 }, y, if long { 6.0 } else { 3.0 }, 1.0).into_any_element());
                    if long && i > 0 {
                        let val = match unit {
                            RulerUnit::Points => (i / per_label) * 50,
                            _ => i / per_label,
                        };
                        out.push(label(1.0, y + 1.0, val.to_string()).into_any_element());
                    }
                    i += 1;
                }

                // Top margin drag handle on left ruler
                let h_top = div()
                    .id("page-layout-margin-top")
                    .absolute()
                    .left(px(0.0))
                    .top(px(py_ + mt - 3.0))
                    .w(px(RULER - 2.0))
                    .h(px(6.0))
                    .cursor_row_resize()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().w(px(RULER - 4.0)).h(px(1.5)).bg(rgb(0x777777)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                            this.begin_ruler_margin_drag(RulerMargin::Top, e.position, scale, cx);
                            cx.stop_propagation();
                        }),
                    );
                out.push(h_top.into_any_element());

                // Bottom margin drag handle on left ruler
                let h_bottom = div()
                    .id("page-layout-margin-bottom")
                    .absolute()
                    .left(px(0.0))
                    .top(px(py_ + ph - mb - 3.0))
                    .w(px(RULER - 2.0))
                    .h(px(6.0))
                    .cursor_row_resize()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().w(px(RULER - 4.0)).h(px(1.5)).bg(rgb(0x777777)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                            this.begin_ruler_margin_drag(RulerMargin::Bottom, e.position, scale, cx);
                            cx.stop_propagation();
                        }),
                    );
                out.push(h_bottom.into_any_element());
            }
        }

        // Dotted guideline across the page while dragging margin
        if let Some(drag) = drag {
            for b in &geometry.boxes {
                match drag.margin {
                    RulerMargin::Left => {
                        out.push(
                            div()
                                .absolute()
                                .left(px(b.paper.0 + ml))
                                .top(px(b.paper.1))
                                .w(px(1.0))
                                .h(px(b.paper.3))
                                .border_l_1()
                                .border_dashed()
                                .border_color(rgb(0x4a90e2))
                                .into_any_element(),
                        );
                    }
                    RulerMargin::Right => {
                        out.push(
                            div()
                                .absolute()
                                .left(px(b.paper.0 + b.paper.2 - mr))
                                .top(px(b.paper.1))
                                .w(px(1.0))
                                .h(px(b.paper.3))
                                .border_l_1()
                                .border_dashed()
                                .border_color(rgb(0x4a90e2))
                                .into_any_element(),
                        );
                    }
                    RulerMargin::Top => {
                        out.push(
                            div()
                                .absolute()
                                .left(px(b.paper.0))
                                .top(px(b.paper.1 + mt))
                                .w(px(b.paper.2))
                                .h(px(1.0))
                                .border_t_1()
                                .border_dashed()
                                .border_color(rgb(0x4a90e2))
                                .into_any_element(),
                        );
                    }
                    RulerMargin::Bottom => {
                        out.push(
                            div()
                                .absolute()
                                .left(px(b.paper.0))
                                .top(px(b.paper.1 + b.paper.3 - mb))
                                .w(px(b.paper.2))
                                .h(px(1.0))
                                .border_t_1()
                                .border_dashed()
                                .border_color(rgb(0x4a90e2))
                                .into_any_element(),
                        );
                    }
                }
            }
        }

        out
    }

    /// Begin dragging a margin handle on the ruler.
    pub(crate) fn begin_ruler_margin_drag(
        &mut self,
        margin: RulerMargin,
        start_point: Point<Pixels>,
        scale: f32,
        cx: &mut Context<Self>,
    ) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let paper_size = sheet.print.page_size();
        let drag = RulerMarginDrag {
            margin,
            start_point,
            current_point: start_point,
            initial_margins: sheet.print.margins,
            scale,
            paper_size,
        };
        if let Some(state) = self.sheet_state_mut() {
            state.ruler_margin_drag = Some(drag);
        }
        cx.notify();
    }

    /// Update pointer position during a ruler margin drag.
    pub(crate) fn drag_ruler_margin(
        &mut self,
        pos: Point<Pixels>,
        is_left_down: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.sheet_state_mut() else { return false };
        if state.ruler_margin_drag.is_none() {
            return false;
        }
        if !is_left_down {
            state.ruler_margin_drag = None;
            cx.notify();
            return false;
        }
        if let Some(drag) = state.ruler_margin_drag.as_mut() {
            drag.current_point = pos;
            cx.notify();
        }
        true
    }

    /// Finish a ruler margin drag and apply margin changes via SheetCommand::SetPrint.
    pub(crate) fn end_ruler_margin_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(state) = self.sheet_state_mut() else { return false };
        let Some(drag) = state.ruler_margin_drag.take() else { return false };
        let new_margins = Self::calculate_dragged_margins(&drag);
        if new_margins != drag.initial_margins {
            let slide = self.current_slide;
            if let Some((_, sheet)) = self.current_sheet() {
                let mut print = sheet.print.clone();
                print.margins = new_margins;
                self.execute(vec![Command::Sheet(SheetCommand::SetPrint { slide, print, area: None })], cx);
            }
        }
        cx.notify();
        true
    }

    /// Calculate the resulting page margins in EMU from the current drag state.
    pub(crate) fn calculate_dragged_margins(drag: &RulerMarginDrag) -> [ibis_doc_model::Emu; 4] {
        let mut margins = drag.initial_margins;
        let dx_pixels = f32::from(drag.current_point.x - drag.start_point.x);
        let dy_pixels = f32::from(drag.current_point.y - drag.start_point.y);
        let dx_emu = (dx_pixels / drag.scale).round() as i64;
        let dy_emu = (dy_pixels / drag.scale).round() as i64;
        let (pw_emu, ph_emu) = drag.paper_size;
        let min_body = 360_000; // minimum body width/height: 1 cm (360,000 EMU)
        match drag.margin {
            RulerMargin::Left => {
                let max_left = (pw_emu - margins[1] - min_body).max(0);
                margins[0] = (drag.initial_margins[0] + dx_emu).clamp(0, max_left);
            }
            RulerMargin::Right => {
                let max_right = (pw_emu - margins[0] - min_body).max(0);
                margins[1] = (drag.initial_margins[1] - dx_emu).clamp(0, max_right);
            }
            RulerMargin::Top => {
                let max_top = (ph_emu - margins[3] - min_body).max(0);
                margins[2] = (drag.initial_margins[2] + dy_emu).clamp(0, max_top);
            }
            RulerMargin::Bottom => {
                let max_bottom = (ph_emu - margins[2] - min_body).max(0);
                margins[3] = (drag.initial_margins[3] - dy_emu).clamp(0, max_bottom);
            }
        }
        margins
    }

    /// The window position of a margin drag handle on the ruler (for scenarios).
    #[allow(dead_code)]
    pub(crate) fn page_layout_ruler_margin_handle_pos(&self, margin: RulerMargin) -> Option<Point<Pixels>> {
        let ws = self.workspace_bounds?;
        let c = self.workspace_content()?;
        let g = self.page_layout_geometry(c.scale)?;
        let b = g.boxes.first()?;
        let off = self.workspace_scroll.offset();
        let margins = self.current_sheet()?.1.print.margins;
        let [ml, mr, mt, mb] = margins.map(|m| m as f32 * c.scale);
        match margin {
            RulerMargin::Left => Some(ws.origin + off + point(px(b.paper.0 + ml), px(RULER / 2.0))),
            RulerMargin::Right => Some(ws.origin + off + point(px(b.paper.0 + b.paper.2 - mr), px(RULER / 2.0))),
            RulerMargin::Top => Some(ws.origin + off + point(px(RULER / 2.0), px(b.paper.1 + mt))),
            RulerMargin::Bottom => Some(ws.origin + off + point(px(RULER / 2.0), px(b.paper.1 + b.paper.3 - mb))),
        }
    }

    /// The window position of the top-left ruler corner box (for scenarios).
    #[allow(dead_code)]
    pub(crate) fn page_layout_ruler_corner_pos(&self) -> Option<Point<Pixels>> {
        let ws = self.workspace_bounds?;
        let off = self.workspace_scroll.offset();
        Some(ws.origin + off + point(px(RULER / 2.0), px(RULER / 2.0)))
    }
}
