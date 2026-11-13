// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The workbook's Draw panel : the pen, its colour, width and effect, and the strokes drawn on a sheet. The pens and the
//! recognition of strokes are `shell/ink.rs` (the three formats use them); this is the workbook's use of them. It was in `shell/ink.rs`.

#![allow(unused_imports)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use gpui_kit::*;
use ibis_doc_model::{Color, Command, EmuRect, Geometry, LineCap, LineJoin, PathOp, Rgb, Shape, ShapeId, ShapeKind, Stroke};
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};
use crate::shell::ink::*;
use crate::ext::*;

#[ibis_office_macros::editor_ext(InkPanelExt)]
impl SlideEditor {
    pub fn open_ink_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let panel = InkPanel { pen: PenKind::Pen, lasso: false, color: Rgb(COLORS[0]), effect: None, width: WIDTHS[1], mouse: true, stroke: None, message: String::new(), ui: Default::default() };
        let state = self.wb_mut();
        state.close_settings();
        state.panels.ink = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn ink_set_effect(&mut self, effect: Option<InkEffect>, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.ink.as_mut() {
            p.effect = effect;
            if let Some(effect) = effect {
                if let Some(first) = effect.stops().first() {
                    p.color = first.color.rgb;
                }
            }
        }
        cx.notify();
    }

    pub(crate) fn ink_set_color(&mut self, color: Rgb, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.ink.as_mut() {
            p.color = color;
            p.effect = None;
        }
        cx.notify();
    }

    pub(crate) fn ink_set_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.ink.as_mut() {
            p.width = width;
        }
        cx.notify();
    }

    #[allow(dead_code)]
    pub(crate) fn ink_set_pen(&mut self, pen: PenKind, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.ink.as_mut() {
            p.pen = pen;
            p.lasso = false;
            if pen == PenKind::Highlighter || pen == PenKind::Pencil {
                p.effect = None;
            }
        }
        cx.notify();
    }

    /// Whether a press on the sheet is for drawing (or erasing, or lasso selecting).
    /// Only while the Draw panel is on screen: with the pane hidden (or a dialog having closed it) the pen is over, so that no stroke is
    /// drawn that its user cannot see the mode of (the clicks of a worksheet whose cells would not be selected).
    pub(crate) fn ink_pen(&self) -> Option<PenKind> {
        if !self.sidebar_open { return None; }
        let p = self.sheet_state()?.panels.ink.as_ref()?;
        (p.mouse && (p.pen != PenKind::Select || p.lasso)).then_some(p.pen)
    }

    fn ink_lasso_active(&self) -> bool {
        self.sheet_state()
            .and_then(|s| s.panels.ink.as_ref())
            .is_some_and(|p| p.mouse && p.lasso)
    }

    /// A press on the sheet while the Draw panel is open: a stroke begins, or one is erased.
    /// True if the press was taken.
    pub(crate) fn ink_press(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(pen) = self.ink_pen() else { return false };
        let Some(layout) = self.slide_layout() else { return false };
        let at = layout.to_emu(position);
        let at = (at.x, at.y);
        if self.ink_lasso_active() {
            // Pressing directly on an already-selected shape in Lasso mode lets the user drag-move the selection.
            if self.selected_ids().iter().any(|&id| self.slide().shape(id).is_some_and(|s| s.frame.contains(ibis_doc_model::EmuPoint::new(at.0, at.1)))) {
                return false;
            }
            if let Some(p) = self.wb_mut().panels.ink.as_mut() {
                p.stroke = Some(vec![at]);
            }
            cx.notify();
            return true;
        }
        match pen {
            PenKind::Eraser => {
                self.ink_erase_at(at, cx);
            }
            _ => {
                if let Some(p) = self.wb_mut().panels.ink.as_mut() {
                    p.stroke = Some(vec![at]);
                }
                cx.notify();
            }
        }
        true
    }

    /// The pointer moved with the button down while drawing (or erasing).
    pub(crate) fn ink_move(&mut self, position: Point<Pixels>, pressed: bool, cx: &mut Context<Self>) -> bool {
        let Some(pen) = self.ink_pen() else { return false };
        let drawing = self.sheet_state().and_then(|s| s.panels.ink.as_ref()).is_some_and(|p| p.stroke.is_some());
        if pen != PenKind::Eraser && !drawing {
            return false;
        }
        let Some(layout) = self.slide_layout() else { return true };
        let e = layout.to_emu(position);
        if !pressed {
            // The button was let go away from the window: the stroke ends.
            return self.ink_release(cx);
        }
        if pen == PenKind::Eraser && !self.ink_lasso_active() {
            self.ink_erase_at((e.x, e.y), cx);
        } else if let Some(stroke) = self.wb_mut().panels.ink.as_mut().and_then(|p| p.stroke.as_mut()) {
            let far_enough = stroke.last().is_none_or(|l| (l.0 - e.x).abs() + (l.1 - e.y).abs() > emu_per_pt() / 2);
            if far_enough {
                stroke.push((e.x, e.y));
            }
            cx.notify();
        }
        true
    }

    /// The button went up: the stroke is made a shape, or a lasso selects enclosed strokes.
    pub(crate) fn ink_release(&mut self, cx: &mut Context<Self>) -> bool {
        // Called on every button release in a workbook's window: mostly no stroke is being drawn.
        let Some(points) = self.sheet_state_mut().and_then(|s| s.panels.ink.as_mut()).and_then(|p| p.stroke.take()) else { return false };
        let Some(p) = self.sheet_state().and_then(|s| s.panels.ink.as_ref()) else { return true };
        let (pen, lasso, color, effect, width) = (p.pen, p.lasso, p.color, p.effect, p.width);
        if lasso {
            let hit: Vec<ShapeId> = self
                .slide()
                .shapes
                .iter()
                .filter(|s| stroke_points(s).is_some_and(|pts| lasso_selects_stroke(&points, &pts)))
                .map(|s| s.id)
                .collect();
            self.set_selection(hit);
            cx.notify();
            return true;
        }
        let points = if points.len() == 1 { vec![points[0], (points[0].0 + 1, points[0].1)] } else { points };
        let stroke = ink_stroke_with_effect(pen, color, effect, width);
        self.insert_ink_shape(&points, stroke, cx);
        true
    }

    fn insert_ink_shape(&mut self, points: &[(i64, i64)], stroke: Stroke, cx: &mut Context<Self>) -> ShapeId {
        let (min_x, max_x) = points.iter().fold((i64::MAX, i64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (min_y, max_y) = points.iter().fold((i64::MAX, i64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let frame = EmuRect::new(min_x, min_y, (max_x - min_x).max(1), (max_y - min_y).max(1));
        let local: Vec<(i64, i64)> = points.iter().map(|p| (p.0 - min_x, p.1 - min_y)).collect();
        let id = self.document.ids().new_shape_id();
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        let shape = crate::shell::shape_items::new_freehand_shape(id, frame, &local, Some(stroke));
        let slide_ref = self.slide();
        let used: Vec<usize> = slide_ref
            .shapes
            .iter()
            .filter_map(|s| slide_ref.shape_names.get(&s.id))
            .filter(|name| is_ink_name(name))
            .filter_map(|name| {
                ["インク ", "Ink "].iter().find_map(|p| { // i18n-ok（ファイルの中のインクの図形名の接頭辞の照合。訳さない）
                    let rest = name.strip_prefix(p)?;
                    let num_str = rest.split_whitespace().next()?.trim_end_matches(|c: char| !c.is_ascii_digit());
                    num_str.parse::<usize>().ok()
                })
            })
            .collect();
        let n = used.into_iter().max().unwrap_or(0) + 1;
        let name = format("xlsx.ink.name", &[("n", &n)]);
        self.run_sheet(vec![Command::InsertShape { slide, index, shape }, Command::SetShapeName { slide, shape: id, name }], cx);
        id
    }

    /// The strokes whose line the point touches are taken away (one step).
    fn ink_erase_at(&mut self, at: (i64, i64), cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let reach = 5.0 * emu_per_pt() as f64;
        let hit: Vec<ShapeId> = self
            .slide()
            .shapes
            .iter()
            .filter(|s| self.is_ink_shape(s))
            .filter(|s| stroke_points(s).is_some_and(|pts| distance_to_stroke(&pts, at) <= reach))
            .map(|s| s.id)
            .collect();
        if !hit.is_empty() {
            self.run_sheet(vec![Command::RemoveShapes { slide, shapes: hit }], cx);
        }
    }

    /// The ink strokes of the sheet.
    fn ink_ids(&self) -> Vec<ShapeId> {
        self.slide().shapes.iter().filter(|s| self.is_ink_shape(s)).map(|s| s.id).collect()
    }

    /// Whether a shape is an ink stroke: a polyline with no fill and a round-ended line, named `インク N` (this app's own name) or with no name
    /// of its own (the `Shape N` a file that lost the name has; this app wrote such names before it kept the ink's). Another named
    /// freeform is the user's drawing, not ink.
    fn is_ink_shape(&self, shape: &ibis_doc_model::Shape) -> bool {
        if stroke_points(shape).is_none() { return false; }
        match self.slide().shape_names.get(&shape.id) {
            Some(name) if is_ink_name(name) => true,
            name => {
                let generic = name.is_none_or(|n| n.is_empty() || n.strip_prefix("Shape ").is_some_and(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_digit())));
                let ibis_doc_model::ShapeKind::Auto(auto) = &shape.kind else { return false };
                generic && matches!(auto.fill, ibis_doc_model::Fill::None) && auto.stroke.as_ref().is_some_and(|st| st.cap == ibis_doc_model::LineCap::Round)
            }
        }
    }

    fn ink_selected_ids(&self) -> Vec<ShapeId> {
        self.selected_ids().into_iter().filter(|id| self.slide().shape(*id).is_some_and(|s| stroke_points(s).is_some())).collect()
    }

    /// すべてのインクを選択.
    pub(crate) fn ink_select_all(&mut self, cx: &mut Context<Self>) {
        let ids = self.ink_ids();
        self.set_selection(ids);
        cx.notify();
    }

    /// 選択インクを消去 (one step).
    pub(crate) fn ink_erase_selected(&mut self, cx: &mut Context<Self>) {
        let (slide, shapes) = (self.current_slide, self.ink_selected_ids());
        if !shapes.is_empty() {
            self.run_sheet(vec![Command::RemoveShapes { slide, shapes }], cx);
        }
    }

    /// すべてのインクを消去 (one step).
    pub(crate) fn ink_erase_all(&mut self, cx: &mut Context<Self>) {
        let (slide, shapes) = (self.current_slide, self.ink_ids());
        if !shapes.is_empty() {
            self.run_sheet(vec![Command::RemoveShapes { slide, shapes }], cx);
        }
    }

    /// インクを図形に変換: the selected stroke(s) become the shapes they look like.
    pub(crate) fn ink_to_shape(&mut self, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let selected_ids = self.selected_ids();
        let candidates: Vec<(Shape, Vec<(i64, i64)>)> = selected_ids
            .into_iter()
            .filter_map(|id| self.slide().shape(id).cloned())
            .filter_map(|s| stroke_points(&s).map(|p| (s, p)))
            .collect();
        if candidates.is_empty() {
            return self.ink_message(text("xlsx.ink.message.no-ink").to_string(), cx);
        }
        let mut commands = Vec::new();
        let mut new_ids = Vec::new();
        for (shape, points) in candidates {
            let Some(kind) = recognize_xlsx(&points) else { continue };
            let stroke = match &shape.kind {
                ShapeKind::Auto(a) => a.stroke.clone(),
                _ => None,
            }
            .map(|s| Stroke { color: Color::opaque(s.color.rgb), ..s });
            let frame = shape.frame;
            let id = self.document.ids().new_shape_id();
            let index = self.slide().shapes.iter().position(|s| s.id == shape.id).unwrap_or(0);
            let replacement = if let Some(preset) = kind.preset() {
                let mut s = crate::shell::shape_items::new_shape(id, preset, frame);
                if let (ShapeKind::Auto(auto), Some(stroke)) = (&mut s.kind, stroke) {
                    auto.stroke = Some(stroke);
                    auto.fill = ibis_doc_model::Fill::None;
                }
                s
            } else {
                let (first, last) = (points[0], points[points.len() - 1]);
                let line_frame = EmuRect::new(first.0.min(last.0), first.1.min(last.1), (first.0 - last.0).abs(), (first.1 - last.1).abs());
                let mut s = crate::shell::shape_items::new_line(id, line_frame, first.0 > last.0, first.1 > last.1, false);
                if let (ShapeKind::Line(line), Some(stroke)) = (&mut s.kind, stroke) {
                    *line = stroke;
                }
                s
            };
            commands.push(Command::RemoveShape { slide, shape: shape.id });
            commands.push(Command::InsertShape { slide, index, shape: replacement });
            new_ids.push(id);
        }
        if commands.is_empty() {
            return self.ink_message(text("xlsx.ink.message.not-recognized").to_string(), cx);
        }
        self.run_sheet(commands, cx);
        self.set_selection(new_ids);
        self.ink_message(text("xlsx.ink.message.converted").to_string(), cx);
    }

    fn ink_message(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.ink.as_mut() {
            p.message = message;
        }
        cx.notify();
    }

    /// The stroke being drawn, as a line over the sheet (the preview).
    pub(crate) fn render_ink_preview(&self, scale: f32) -> Option<Div> {
        let p = self.sheet_state()?.panels.ink.as_ref()?;
        let points = p.stroke.as_ref().filter(|s| s.len() >= 2)?;
        let pen = p.pen;
        let lasso = p.lasso;
        let effect = if lasso { None } else { p.effect };
        let stroke = ink_stroke_with_effect(pen, p.color, effect, p.width);
        let color = if lasso {
            rgba(0x2563EB_B0)
        } else {
            rgba(((stroke.color.rgb.0) << 8) | u32::from(stroke.color.alpha))
        };
        let width = if lasso { 1.5 } else { (stroke.width as f32 * scale).max(1.0) };
        let closed = lasso;
        let pts: Vec<Point<Pixels>> = points.iter().map(|&(x, y)| point(px(x as f32 * scale), px(y as f32 * scale))).collect();
        Some(
            div().absolute().top_0().left_0().size_full().child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let o = bounds.origin;
                        let shifted: Vec<Point<Pixels>> = pts.iter().map(|p| point(o.x + p.x, o.y + p.y)).collect();
                        if let Some(effect) = effect {
                            let stops = effect.stops();
                            let total_len: f32 = shifted.windows(2).map(|w| (f32::from(w[1].x - w[0].x)).hypot(f32::from(w[1].y - w[0].y))).sum();
                            if total_len > 0.0 {
                                let mut cum_len = 0.0f32;
                                for w in shifted.windows(2) {
                                    let seg_len = (f32::from(w[1].x - w[0].x)).hypot(f32::from(w[1].y - w[0].y));
                                    let t = (cum_len + seg_len * 0.5) / total_len;
                                    let seg_c = crate::render::gradient::color_at_mode(&stops, t, false);
                                    if let Some(path) = crate::render::shape_view::polygon_styled(&[w[0], w[1]], Some(px(width)), false, LineCap::Round, LineJoin::Round) {
                                        window.paint_path(path, seg_c);
                                    }
                                    cum_len += seg_len;
                                }
                                return;
                            }
                        }
                        if let Some(path) = crate::render::shape_view::polygon_styled(&shifted, Some(px(width)), closed, LineCap::Round, LineJoin::Round) {
                            window.paint_path(path, color);
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            ),
        )
    }

    pub fn ink_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel>{
        use crate::shell::ink_panel::{InkChoice,InkToggle,InkRow,InkSection,InkPanelSpec};
        let p=self.sheet_state()?.panels.ink.as_ref()?;
        let bounds=|id:&str|{let ui=p.ui.clone();let id=id.to_string();move|b|{ui.borrow_mut().insert(id.clone(),b);}};
        let tools=[("ink-select","xlsx.ink.off",PenKind::Select,false),("ink-pen","xlsx.ink.pen",PenKind::Pen,false),("ink-pencil","xlsx.ink.pencil",PenKind::Pencil,false),("ink-highlighter","xlsx.ink.highlighter",PenKind::Highlighter,false),("ink-eraser","xlsx.ink.eraser",PenKind::Eraser,false),("ink-lasso","xlsx.ink.lasso",PenKind::Select,true)].into_iter().map(|(id,label,kind,lasso)|{
            InkChoice::new(id,text(label),if lasso{p.lasso}else{p.pen==kind&&!p.lasso},move|e,_,cx|{
                if let Some(p)=e.wb_mut().panels.ink.as_mut(){
                    p.pen=kind;p.lasso=lasso;
                    p.color=match kind{PenKind::Highlighter=>Rgb(COLORS[3]),PenKind::Pencil=>Rgb(COLORS[1]),PenKind::Pen if p.color==Rgb(COLORS[3])||p.color==Rgb(COLORS[1])=>Rgb(COLORS[0]),_=>p.color};
                    if matches!(kind,PenKind::Highlighter|PenKind::Pencil){p.effect=None;}
                    p.width=match kind{PenKind::Highlighter=>WIDTHS[2],PenKind::Pencil=>WIDTHS[0],_=>WIDTHS[1]};
                }
                cx.notify();
            }).on_bounds(bounds(id))
        }).collect();
        let widths=WIDTHS.into_iter().enumerate().map(|(i,width)|{
            let id=format!("ink-width-{i}");InkChoice::new(id.clone(),format("xlsx.ink.width-pt",&[("pt",&width)]),(p.width-width).abs()<0.01,move|e,_,cx|{if let Some(p)=e.wb_mut().panels.ink.as_mut(){p.width=width;}cx.notify();}).points(width).on_bounds(bounds(&id))
        }).collect();
        let mut effects=vec![InkChoice::new("ink-effect-none",text("docx.ink.effect.none"),p.effect.is_none(),|e,_,cx|{if let Some(p)=e.wb_mut().panels.ink.as_mut(){p.effect=None;}cx.notify();})];
        effects.extend(InkEffect::ALL.into_iter().map(|effect|InkChoice::new(effect.id(),text(effect.label_key()),p.effect==Some(effect),move|e,_,cx|{if let Some(p)=e.wb_mut().panels.ink.as_mut(){p.effect=Some(effect);if let Some(stop)=effect.stops().first(){p.color=stop.color.rgb;}}cx.notify();}).on_bounds(bounds(effect.id())).texture(crate::shell::ink_panel::InkTexture::Gradient(effect.stops().iter().map(|s|(s.position,s.color.rgb.0)).collect()))));
        let hide=self.current_sheet().map(|(_,s)|s.hide_ink).unwrap_or(false);
        let options=InkSection{title:None,rows:vec![
            InkRow::Toggle(InkToggle::new("ink-hide",text("xlsx.hide-ink"),hide,|e,_,cx|e.toggle_hide_ink(cx)).on_bounds(bounds("ink-hide"))),
        ]};
        let actions=crate::shell::ink_panel::InkActions{selected:self.ink_selected_ids().len(),has_ink:!self.ink_ids().is_empty(),
            to_shape:Rc::new(|e,_,cx|e.ink_to_shape(cx)),select_all:Rc::new(|e,_,cx|e.ink_select_all(cx)),
            erase_selected:Rc::new(|e,_,cx|e.ink_erase_selected(cx)),erase_all:Rc::new(|e,_,cx|e.ink_erase_all(cx)),to_text:None,to_math:None,replay:None}.section();
        Some(crate::shell::ink_panel::panel(InkPanelSpec{
            color_id:"xlsx-ink-color-field",tools,widths,drawing:matches!(p.pen,PenKind::Pen|PenKind::Pencil|PenKind::Highlighter)&&!p.lasso,color:p.color,
            change_color:Rc::new(|e,color,_,cx|{if let Some(p)=e.wb_mut().panels.ink.as_mut(){p.color=color;p.effect=None;}cx.notify();}),effects,
            mouse:InkToggle::new("ink-mouse",text("ink.panel.mouse"),p.mouse,|e,_,cx|{if let Some(p)=e.wb_mut().panels.ink.as_mut(){p.mouse=!p.mouse;}cx.notify();}).on_bounds(bounds("ink-mouse")),
            sections:vec![options,actions],message:p.message.clone(),context_menu:None,
        },cx))
    }

}
