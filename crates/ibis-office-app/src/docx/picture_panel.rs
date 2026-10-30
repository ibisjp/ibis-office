// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Numeric size controls for the inline picture at the caret.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputEvent,InputState};
use gpui_kit::component::{Disableable as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use crate::shell::palette::rgb;
use ibis_doc_model::{FlowPos,FlowRunKind,FlowCommand,EmuSize,MediaId,PictureLook,Crop,Stroke,Color,Rgb,LineDash};
use crate::shell::{chrome::SidebarPanel,editor::SlideEditor,icons::Material};

pub(crate) const PICTURE_BORDER_WIDTHS: [(&str, i64); 7] = [
    ("0.5 pt", 6_350),
    ("1 pt", 12_700),
    ("1.5 pt", 19_050),
    ("2.25 pt", 28_575),
    ("3 pt", 38_100),
    ("4.5 pt", 57_150),
    ("6 pt", 76_200),
];

pub(crate) const PICTURE_BORDER_COLORS: [(&str, u32); 6] = [
    ("_docx.black", 0x000000),
    ("_docx.gray-2", 0x757575),
    ("_docx.red", 0xD32F2F),
    ("_docx.blue", 0x1B65D4),
    ("_docx.green", 0x2E7D32),
    ("_docx.orange", 0xF57C00),
];

pub(crate) const PICTURE_BORDER_DASHES: [(&str, LineDash); 5] = [
    ("pptx.chrome.solid", LineDash::Solid),
    ("_docx.round-dot", LineDash::Dot),
    ("_docx.dash", LineDash::Dash),
    ("pptx.chrome.long-dash", LineDash::LgDash),
    ("_docx.dash-dot", LineDash::DashDot),
];

pub(crate) const PICTURE_CROP_SHAPES: [(&str, Option<&str>); 15] = [
    ("docx.picture.crop-to-shape.rect", None),
    ("docx.picture.crop-to-shape.round-rect", Some("roundRect")),
    ("docx.picture.crop-to-shape.ellipse", Some("ellipse")),
    ("docx.picture.crop-to-shape.triangle", Some("triangle")),
    ("docx.picture.crop-to-shape.diamond", Some("diamond")),
    ("docx.picture.crop-to-shape.parallelogram", Some("parallelogram")),
    ("docx.picture.crop-to-shape.trapezoid", Some("trapezoid")),
    ("docx.picture.crop-to-shape.hexagon", Some("hexagon")),
    ("docx.picture.crop-to-shape.octagon", Some("octagon")),
    ("docx.picture.crop-to-shape.heart", Some("heart")),
    ("docx.picture.crop-to-shape.star5", Some("star5")),
    ("docx.picture.crop-to-shape.cloud", Some("cloud")),
    ("docx.picture.crop-to-shape.arrow", Some("rightArrow")),
    ("docx.picture.crop-to-shape.teardrop", Some("teardrop")),
    ("docx.picture.crop-to-shape.callout", Some("wedgeRoundRectCallout")),
];

/// A recolouring of a picture (Word's 色).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PictureRecolor {
    Grayscale,
    Sepia,
    Washout,
    BlackAndWhite50,
    Blue,
    Green,
    Red,
    Orange,
    Purple,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ZOrderAction {
    BringToFront,
    BringForward,
    SendBackward,
    SendToBack,
}

#[derive(Clone, Debug)]
pub(crate) struct FloatingPictureInfo {
    pub(crate) at: FlowPos,
    pub(crate) size: EmuSize,
    pub(crate) anchor: ibis_doc_model::FloatingAnchor,
}

pub(crate) struct PictureEditState {
    pub(crate) name_field: Entity<InputState>,
    pub(crate) initial_name: String,
    pub(crate) fields:[Entity<InputState>;2],
    /// The cut at the top, bottom, left and right, in percent.
    pub(crate) crop:[Entity<InputState>;4],
    pub(crate) keep_ratio:bool,
    pub(crate) alt_title: Entity<InputState>,
    pub(crate) alt_desc: Entity<InputState>,
    pub(crate) initial_alt: ibis_doc_model::AltText,
    pub(crate) decorative: bool,
    pub(crate) initial_decorative: bool,
    target:(FlowPos,MediaId,EmuSize),
    ratio_size:EmuSize,
    initial:[String;2],
    error:Option<String>,
    _subscriptions:Vec<Subscription>,
}
impl SlideEditor {
    pub(crate) fn picture_at_caret(&self)->Option<(FlowPos,MediaId,EmuSize)> {
        let (start,_) = self.ordered_selection()?;
        let p=self.paragraph_of(start.para)?;
        let mut offset=0;
        let mut previous=None;
        for run in &p.runs {
            match &run.kind {
                FlowRunKind::Picture {media,size,..} | FlowRunKind::PictureControl {media,size,..} => {
                    let found=Some((FlowPos::new(p.id,offset),*media,*size));
                    if offset==start.offset {return found;}
                    if offset+1==start.offset {previous=found;}
                }
                FlowRunKind::Shape(shape) => {
                    if let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind {
                        let size = EmuSize::new(shape.frame.width, shape.frame.height);
                        let found = Some((FlowPos::new(p.id, offset), pic.media, size));
                        if offset == start.offset { return found; }
                        if offset + 1 == start.offset { previous = found; }
                    }
                }
                _ => {}
            }
            offset+=run.len();
        }
        previous
    }
    pub(crate) fn open_picture_edit(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(target)=self.picture_at_caret() else{return};
        let initial=[format!("{:.2}",target.2.width as f64/12700.),format!("{:.2}",target.2.height as f64/12700.)];
        let fields=initial.clone().map(|value|cx.new(|cx|{
            let mut field=InputState::new(window,cx);field.set_value(value,window,cx);field
        }));
        let mut subscriptions: Vec<Subscription> = fields.iter().map(|field|cx.subscribe_in(field,window,|this,input,event:&InputEvent,window,cx|{
            if matches!(event,InputEvent::Change|InputEvent::PressEnter{..}) {
                let raw=input.read(cx).value().to_string();this.apply_picture_edit(window,cx);
                input.update(cx,|i,cx|{i.set_value(raw,window,cx);i.focus(window,cx);});
            }
        })).collect();
        let look_now = self.picture_look_at_caret().unwrap_or_default();
        let crop_now = look_now.crop;
        let initial_alt = look_now.alt_text.clone();
        let decorative = look_now.decorative;
        let initial_name = look_now.name.clone();
        let name_field = cx.new(|cx| {
            let mut field = InputState::new(window, cx);
            field.set_value(initial_name.clone(), window, cx);
            field
        });
        subscriptions.push(cx.subscribe_in(&name_field, window, |this, _, event: &InputEvent, _window, cx| {
            if matches!(event,InputEvent::Change|InputEvent::PressEnter{..}) { this.apply_picture_name(cx); }
        }));
        let percent=|v:i32|{let p=v as f64/1000.;if p.fract()==0.{format!("{}",p as i64)}else{format!("{p:.1}")}};
        let crop=[crop_now.top,crop_now.bottom,crop_now.left,crop_now.right].map(|v|cx.new(|cx|{
            let mut field=InputState::new(window,cx);field.set_value(percent(v),window,cx);field
        }));
        subscriptions.extend(crop.iter().map(|f|cx.subscribe_in(f,window,|e,_,event:&InputEvent,_,cx|{if matches!(event,InputEvent::Change|InputEvent::PressEnter{..}){e.apply_picture_crop(cx);}})));
        let alt_title = cx.new(|cx| {
            let mut field = InputState::new(window, cx);
            field.set_value(initial_alt.title.clone(), window, cx);
            field
        });
        let alt_desc = cx.new(|cx| {
            let mut field = InputState::new(window, cx);
            field.set_value(initial_alt.description.clone(), window, cx);
            field
        });
        subscriptions.push(cx.subscribe_in(&alt_title, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event,InputEvent::Change|InputEvent::PressEnter{..}) { this.apply_picture_alt_text(cx); }
        }));
        subscriptions.push(cx.subscribe_in(&alt_desc, window, |this, _, event: &InputEvent, window, cx| {
            if matches!(event,InputEvent::Change|InputEvent::PressEnter{..}) { this.apply_picture_alt_text(cx); }
        }));
        if let Some(state)=self.doc_state_mut() {
            state.picture_edit=Some(PictureEditState {name_field,initial_name,fields,crop,keep_ratio:true,alt_title,alt_desc,initial_alt,decorative,initial_decorative:decorative,target,ratio_size:target.2,initial,error:None,_subscriptions:subscriptions});
            state.table_size=None;state.font_edit=None; state.color_edit=None; state.paragraph_edit=None;state.style_edit=None;state.link=None;state.find=None;state.comments=None;
        }
        self.sidebar_open=true;cx.notify();
    }
    /// How the picture at the caret is turned, flipped and cut.
    pub(crate) fn picture_look_at_caret(&self)->Option<PictureLook> {
        let (at,..)=self.picture_at_caret()?;
        let p=self.paragraph_of(at.para)?;
        let mut offset=0;
        for run in &p.runs {
            match &run.kind {
                FlowRunKind::Picture {look,..} | FlowRunKind::PictureControl {look,..} => {
                    if offset==at.offset {return Some(look.clone());}
                }
                FlowRunKind::Shape(shape) => {
                    if offset==at.offset && let ibis_doc_model::ShapeKind::Picture(pic) = &shape.kind {
                        return Some(PictureLook {
                            rotation: shape.rotation,
                            flip_h: shape.flip_h,
                            flip_v: shape.flip_v,
                            crop: pic.crop,
                            stroke: pic.stroke.clone(),
                            luminance: pic.luminance,
                            duotone: pic.duotone,
                            alt_text: pic.alt_text.clone(),
                            decorative: pic.decorative,
                            name: pic.name.clone(),
                            linked_file: None,
                            soft_edge: pic.soft_edge,
                            effects: pic.effects,
                            geometry: pic.geometry.clone(),
                        });
                    }
                }
                _ => {}
            }
            offset+=run.len();
        }
        None
    }

    /// The text wrapping style of the picture at the caret.
    pub(crate) fn picture_wrap_at_caret(&self) -> Option<ibis_doc_model::PictureWrapType> {
        let (at, ..) = self.picture_at_caret()?;
        let p = self.paragraph_of(at.para)?;
        let mut offset = 0;
        for run in &p.runs {
            if offset == at.offset {
                return match &run.kind {
                    FlowRunKind::Picture { .. } | FlowRunKind::PictureControl { .. } => Some(ibis_doc_model::PictureWrapType::Inline),
                    FlowRunKind::Shape(shape) => {
                        let anchor = shape.floating.as_ref();
                        match anchor {
                            None => Some(ibis_doc_model::PictureWrapType::Inline),
                            Some(f) => Some(ibis_doc_model::PictureWrapType::from_anchor(f)),
                        }
                    }
                    _ => None,
                };
            }
            offset += run.len();
        }
        None
    }

    /// The floating anchor of the picture at the caret, if floating.
    pub(crate) fn picture_floating_at_caret(&self) -> Option<ibis_doc_model::FloatingAnchor> {
        let (at, ..) = self.picture_at_caret()?;
        let p = self.paragraph_of(at.para)?;
        let mut offset = 0;
        for run in &p.runs {
            if offset == at.offset {
                if let FlowRunKind::Shape(shape) = &run.kind {
                    return shape.floating.clone();
                }
            }
            offset += run.len();
        }
        None
    }

    /// Set the text wrapping style of the picture at the caret.
    pub(crate) fn set_picture_wrap(&mut self, wrap: ibis_doc_model::PictureWrapType, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        self.run_flow(vec![FlowCommand::SetPictureWrap { at, wrap }], cx);
    }

    /// Set the floating distance from text of the picture at the caret.
    pub(crate) fn set_picture_floating_distance(&mut self, distance: [ibis_doc_model::Emu; 4], cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.dist_l = distance[0];
        anchor.dist_r = distance[1];
        anchor.dist_t = distance[2];
        anchor.dist_b = distance[3];
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// 折返し点の編集モードかどうか (DOCX-FLOAT-006).
    pub(crate) fn is_editing_wrap_points(&self) -> bool {
        self.doc_state().is_some_and(|s| s.editing_wrap_points)
    }

    /// 折返し点の編集モードの切り替え (DOCX-FLOAT-006).
    pub(crate) fn toggle_edit_wrap_points(&mut self, cx: &mut Context<Self>) {
        let now = !self.is_editing_wrap_points();
        if let Some(state) = self.doc_state_mut() {
            state.editing_wrap_points = now;
        }
        if now && let Some((at, ..)) = self.picture_at_caret() {
            let wrap = self.picture_wrap_at_caret().unwrap_or(ibis_doc_model::PictureWrapType::Inline);
            if !matches!(wrap, ibis_doc_model::PictureWrapType::Tight | ibis_doc_model::PictureWrapType::Through) {
                self.run_flow(vec![FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::Tight }], cx);
            } else if let Some(mut anchor) = self.picture_floating_at_caret()
                && anchor.wrap_polygon.is_none()
            {
                anchor.wrap_polygon = Some(ibis_doc_model::WrapPolygon::default_rect_21600());
                self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
            }
        }
        cx.notify();
    }

    /// キャレット位置の画像の折返しポリゴンを取得 (DOCX-FLOAT-006).
    pub(crate) fn picture_wrap_polygon_at_caret(&self) -> Option<ibis_doc_model::WrapPolygon> {
        self.picture_floating_at_caret().and_then(|a| a.wrap_polygon)
    }

    /// キャレット位置の画像の折返しポリゴンを設定（1ステップ Undo/Redo 対応、DOCX-FLOAT-006）.
    pub(crate) fn set_picture_wrap_polygon(&mut self, polygon: Option<ibis_doc_model::WrapPolygon>, cx: &mut Context<Self>) -> bool {
        let Some((at, ..)) = self.picture_at_caret() else { return false };
        let mut cmds = Vec::new();
        if self.picture_floating_at_caret().is_none() {
            cmds.push(FlowCommand::SetPictureWrap { at, wrap: ibis_doc_model::PictureWrapType::Tight });
        }
        let mut anchor = self.picture_floating_at_caret().unwrap_or_else(|| {
            let mut a = ibis_doc_model::FloatingAnchor::default();
            a.wrap = "tight".to_string();
            a
        });
        if polygon.is_some() && !matches!(anchor.wrap.as_str(), "tight" | "through") {
            anchor.wrap = "tight".to_string();
        }
        anchor.wrap_polygon = polygon;
        cmds.push(FlowCommand::SetPictureFloating { at, anchor: Some(anchor) });
        let ok = self.run_flow(cmds, cx);
        cx.notify();
        ok
    }

    /// 折返し点を既定の矩形（0..21600）にリセット (DOCX-FLOAT-006).
    pub(crate) fn reset_picture_wrap_points(&mut self, cx: &mut Context<Self>) -> bool {
        self.set_picture_wrap_polygon(Some(ibis_doc_model::WrapPolygon::default_rect_21600()), cx)
    }

    /// 折返しポリゴンのプリセット適用 (DOCX-FLOAT-006: "rect", "diamond", "octagon", "inset").
    pub(crate) fn apply_picture_wrap_polygon_preset(&mut self, preset: &str, cx: &mut Context<Self>) -> bool {
        let poly = match preset {
            "diamond" => ibis_doc_model::WrapPolygon::diamond_21600(),
            "octagon" => ibis_doc_model::WrapPolygon::octagon_21600(),
            "inset" => ibis_doc_model::WrapPolygon::inset_rect_21600(2700),
            _ => ibis_doc_model::WrapPolygon::default_rect_21600(),
        };
        self.set_picture_wrap_polygon(Some(poly), cx)
    }

    /// 折返しポリゴンの頂点 `index`（0 = start、1.. = line_to）を更新 (DOCX-FLOAT-006).
    pub(crate) fn set_picture_wrap_point(&mut self, index: usize, point: ibis_doc_model::WrapPoint, cx: &mut Context<Self>) -> bool {
        let mut poly = self
            .picture_wrap_polygon_at_caret()
            .unwrap_or_else(ibis_doc_model::WrapPolygon::default_rect_21600);
        if index == 0 {
            let old_start = poly.start;
            poly.start = point;
            if let Some(last) = poly.line_to.last_mut()
                && *last == old_start
            {
                *last = point;
            }
        } else if let Some(slot) = poly.line_to.get_mut(index - 1) {
            *slot = point;
        } else {
            return false;
        }
        poly.edited = true;
        self.set_picture_wrap_polygon(Some(poly), cx)
    }

    /// 折返しポリゴンの頂点 `index` を `(dx, dy)` だけ移動 (DOCX-FLOAT-006).
    pub(crate) fn nudge_picture_wrap_point(&mut self, index: usize, dx: i64, dy: i64, cx: &mut Context<Self>) -> bool {
        let poly = self
            .picture_wrap_polygon_at_caret()
            .unwrap_or_else(ibis_doc_model::WrapPolygon::default_rect_21600);
        let cur = if index == 0 {
            poly.start
        } else if let Some(&pt) = poly.line_to.get(index - 1) {
            pt
        } else {
            return false;
        };
        let next = ibis_doc_model::WrapPoint::new(
            (cur.x + dx).clamp(-10_800, 32_400),
            (cur.y + dy).clamp(-10_800, 32_400),
        );
        self.set_picture_wrap_point(index, next, cx)
    }

    /// 折返しポリゴンに新しい頂点を挿入 (DOCX-FLOAT-006).
    pub(crate) fn insert_picture_wrap_point(&mut self, after_index: usize, point: ibis_doc_model::WrapPoint, cx: &mut Context<Self>) -> bool {
        let mut poly = self
            .picture_wrap_polygon_at_caret()
            .unwrap_or_else(ibis_doc_model::WrapPolygon::default_rect_21600);
        let pos = after_index.min(poly.line_to.len());
        poly.line_to.insert(pos, point);
        poly.edited = true;
        self.set_picture_wrap_polygon(Some(poly), cx)
    }

    /// 折返しポリゴンから頂点 `index`（1..）を削除 (DOCX-FLOAT-006).
    pub(crate) fn remove_picture_wrap_point(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let mut poly = self
            .picture_wrap_polygon_at_caret()
            .unwrap_or_else(ibis_doc_model::WrapPolygon::default_rect_21600);
        if poly.line_to.len() <= 3 || index == 0 || index > poly.line_to.len() {
            return false;
        }
        poly.line_to.remove(index - 1);
        poly.edited = true;
        self.set_picture_wrap_polygon(Some(poly), cx)
    }

    /// ページ上の折返し点オーバーレイ（赤い破線枠と黒い頂点ハンドル）を描画 (DOCX-FLOAT-006).
    pub(crate) fn wrap_points_overlay(&self, page_index: usize, k: f32) -> Vec<AnyElement> {
        let mut out = Vec::new();
        if !self.is_editing_wrap_points() {
            return out;
        }
        let Some((at, ..)) = self.picture_at_caret() else { return out };
        let Some(doc) = self.flow() else { return out };
        let Some(paragraph) = self.paragraph_of(at.para) else { return out };
        let mut offset = 0;
        let mut found_shape = None;
        for run in &paragraph.runs {
            if offset == at.offset
                && let FlowRunKind::Shape(shape) = &run.kind
                && let Some(anchor) = shape.floating.as_ref()
            {
                let poly = anchor
                    .wrap_polygon
                    .clone()
                    .unwrap_or_else(ibis_doc_model::WrapPolygon::default_rect_21600);
                found_shape = Some((shape.id, poly));
            }
            offset += run.len();
        }
        let Some((shape_id, poly)) = found_shape else { return out };
        let laid = super::flow_layout::pages(doc, &self.resources);
        let Some(pg) = laid.pages.get(page_index) else { return out };
        for item in &pg.items {
            if let super::flow_layout::Item::Shape { shape, x, y, w, h } = item
                && shape.id == shape_id
            {
                let mut pts = Vec::with_capacity(1 + poly.line_to.len());
                pts.push(poly.start);
                pts.extend(poly.line_to.iter().copied());
                let red = rgb(0xd93025);
                for (&p0, &p1) in pts.iter().zip(pts.iter().skip(1)) {
                    let x0 = *x + (p0.x as f32 / 21_600.0) * *w;
                    let y0 = *y + (p0.y as f32 / 21_600.0) * *h;
                    let x1 = *x + (p1.x as f32 / 21_600.0) * *w;
                    let y1 = *y + (p1.y as f32 / 21_600.0) * *h;
                    let min_x = x0.min(x1);
                    let min_y = y0.min(y1);
                    let seg_w = (x1 - x0).abs().max(1.5 / k);
                    let seg_h = (y1 - y0).abs().max(1.5 / k);
                    out.push(
                        div()
                            .absolute()
                            .left(px(min_x * k))
                            .top(px(min_y * k))
                            .w(px(seg_w * k))
                            .h(px(seg_h * k))
                            .border_1()
                            .border_color(red)
                            .into_any_element(),
                    );
                }
                for (vi, pt) in pts.iter().enumerate() {
                    let vx = *x + (pt.x as f32 / 21_600.0) * *w;
                    let vy = *y + (pt.y as f32 / 21_600.0) * *h;
                    out.push(
                        div()
                            .id(("wrap-point-handle", vi))
                            .absolute()
                            .left(px(vx * k - 3.0))
                            .top(px(vy * k - 3.0))
                            .w(px(6.0))
                            .h(px(6.0))
                            .bg(rgb(0x000000))
                            .border_1()
                            .border_color(red)
                            .into_any_element(),
                    );
                }
                break;
            }
        }
        out
    }

    /// Set the horizontal alignment of the floating picture at the caret.
    pub(crate) fn set_picture_floating_align_h(&mut self, align: Option<&str>, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.align_h = align.map(str::to_string);
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Set the vertical alignment of the floating picture at the caret.
    pub(crate) fn set_picture_floating_align_v(&mut self, align: Option<&str>, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.align_v = align.map(str::to_string);
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Toggle allow overlap for the floating picture at the caret.
    pub(crate) fn toggle_picture_allow_overlap(&mut self, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.allow_overlap = !anchor.allow_overlap;
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Toggle locked anchor for the floating picture at the caret.
    pub(crate) fn toggle_picture_locked(&mut self, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.locked = !anchor.locked;
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Toggle layout in cell for the floating picture at the caret.
    pub(crate) fn toggle_picture_layout_in_cell(&mut self, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.layout_in_cell = !anchor.layout_in_cell;
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }
    /// Set whether the floating picture moves with text (relativeFrom="paragraph") or has a fixed position on page (relativeFrom="page").
    pub(crate) fn set_picture_move_with_text(&mut self, move_with_text: bool, cx: &mut Context<Self>) {
        let rel = if move_with_text { "paragraph" } else { "page" };
        self.set_picture_floating_relative_v(rel, cx);
    }

    /// Set vertical positioning reference for the floating picture at the caret.
    pub(crate) fn set_picture_floating_relative_v(&mut self, relative_v: &str, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.relative_v = relative_v.to_string();
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Collect all relative_height values of floating shapes in the document.
    pub(crate) fn document_floating_relative_heights(&self) -> Vec<u32> {
        let mut heights = Vec::new();
        let Some(flow) = self.flow() else { return heights };
        let mut scan_paragraph = |p: &ibis_doc_model::FlowParagraph| {
            for r in &p.runs {
                if let FlowRunKind::Shape(ref s) = r.kind {
                    if let Some(ref fl) = s.floating {
                        heights.push(fl.relative_height);
                    }
                }
            }
        };
        for b in &flow.body {
            match b {
                ibis_doc_model::Block::Paragraph(p) => scan_paragraph(p),
                ibis_doc_model::Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            for cb in &cell.blocks {
                                if let ibis_doc_model::Block::Paragraph(p) = cb {
                                    scan_paragraph(p);
                                }
                            }
                        }
                    }
                }
            }
        }
        heights
    }

    /// Reorder the floating picture at the caret (front, forward, backward, back).
    pub(crate) fn reorder_picture_zorder(&mut self, action: ZOrderAction, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        let cur = anchor.relative_height;
        let heights = self.document_floating_relative_heights();
        let new_height = match action {
            ZOrderAction::BringToFront => {
                let max = heights.iter().copied().max().unwrap_or(cur);
                max.saturating_add(1).max(cur + 1)
            }
            ZOrderAction::BringForward => {
                let next = heights.iter().copied().filter(|&h| h > cur).min();
                next.map_or(cur.saturating_add(1), |h| h.saturating_add(1))
            }
            ZOrderAction::SendBackward => {
                let prev = heights.iter().copied().filter(|&h| h < cur).max();
                prev.map_or(cur.saturating_sub(1), |h| h.saturating_sub(1))
            }
            ZOrderAction::SendToBack => {
                let min = heights.iter().copied().min().unwrap_or(cur);
                min.saturating_sub(1)
            }
        };
        anchor.relative_height = new_height;
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Collect all floating pictures in the document with their position, size and anchor.
    pub(crate) fn document_floating_pictures(&self) -> Vec<FloatingPictureInfo> {
        let mut pictures = Vec::new();
        let Some(flow) = self.flow() else { return pictures };
        let mut scan_paragraph = |p: &ibis_doc_model::FlowParagraph| {
            let mut offset = 0;
            for r in &p.runs {
                if let FlowRunKind::Shape(ref s) = r.kind {
                    if let ibis_doc_model::ShapeKind::Picture(_) = &s.kind {
                        if let Some(ref fl) = s.floating {
                            let size = EmuSize::new(s.frame.width, s.frame.height);
                            pictures.push(FloatingPictureInfo {
                                at: FlowPos::new(p.id, offset),
                                size,
                                anchor: fl.clone(),
                            });
                        }
                    }
                }
                offset += r.len();
            }
        };
        for b in &flow.body {
            match b {
                ibis_doc_model::Block::Paragraph(p) => scan_paragraph(p),
                ibis_doc_model::Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            for cb in &cell.blocks {
                                if let ibis_doc_model::Block::Paragraph(p) = cb {
                                    scan_paragraph(p);
                                }
                            }
                        }
                    }
                }
            }
        }
        pictures
    }

    /// Distribute floating pictures horizontally.
    /// If 3 or more pictures exist, places intermediate pictures at equal intervals between
    /// the leftmost and rightmost pictures. If 1 or 2 pictures exist, centres or balances them.
    pub(crate) fn distribute_pictures_horizontally(&mut self, cx: &mut Context<Self>) {
        let mut pics = self.document_floating_pictures();
        if pics.is_empty() { return; }
        if pics.len() == 1 {
            let mut anchor = pics[0].anchor.clone();
            anchor.align_h = Some("center".to_string());
            self.run_flow(vec![FlowCommand::SetPictureFloating { at: pics[0].at, anchor: Some(anchor) }], cx);
            return;
        }
        pics.sort_by_key(|p| p.anchor.offset_x);
        let n = pics.len();
        let min_x = pics[0].anchor.offset_x;
        let max_x = pics[n - 1].anchor.offset_x;
        let total_intermediate_w: i64 = pics[1..n - 1].iter().map(|p| p.size.width).sum();
        let available_gap = (max_x - min_x - pics[0].size.width - total_intermediate_w).max(0);
        let gap = if n > 1 { available_gap / ((n - 1) as i64) } else { 0 };
        let mut commands = Vec::new();
        if available_gap > 0 {
            let mut cur_x = min_x;
            for p in &mut pics {
                p.anchor.offset_x = cur_x;
                p.anchor.align_h = None;
                cur_x += p.size.width + gap;
                commands.push(FlowCommand::SetPictureFloating { at: p.at, anchor: Some(p.anchor.clone()) });
            }
        } else {
            let step = if max_x > min_x { (max_x - min_x) / ((n - 1) as i64) } else { 100_000 };
            for (i, p) in pics.iter_mut().enumerate() {
                let new_x = min_x + step * (i as i64);
                p.anchor.offset_x = new_x;
                p.anchor.align_h = None;
                commands.push(FlowCommand::SetPictureFloating { at: p.at, anchor: Some(p.anchor.clone()) });
            }
        }
        self.run_flow(commands, cx);
    }

    /// Distribute floating pictures vertically.
    /// If 3 or more pictures exist, places intermediate pictures at equal intervals between
    /// the topmost and bottommost pictures. If 1 or 2 pictures exist, centres or balances them.
    pub(crate) fn distribute_pictures_vertically(&mut self, cx: &mut Context<Self>) {
        let mut pics = self.document_floating_pictures();
        if pics.is_empty() { return; }
        if pics.len() == 1 {
            let mut anchor = pics[0].anchor.clone();
            anchor.align_v = Some("center".to_string());
            self.run_flow(vec![FlowCommand::SetPictureFloating { at: pics[0].at, anchor: Some(anchor) }], cx);
            return;
        }
        pics.sort_by_key(|p| p.anchor.offset_y);
        let n = pics.len();
        let min_y = pics[0].anchor.offset_y;
        let max_y = pics[n - 1].anchor.offset_y;
        let total_intermediate_h: i64 = pics[1..n - 1].iter().map(|p| p.size.height).sum();
        let available_gap = (max_y - min_y - pics[0].size.height - total_intermediate_h).max(0);
        let gap = if n > 1 { available_gap / ((n - 1) as i64) } else { 0 };
        let mut commands = Vec::new();
        if available_gap > 0 {
            let mut cur_y = min_y;
            for p in &mut pics {
                p.anchor.offset_y = cur_y;
                p.anchor.align_v = None;
                cur_y += p.size.height + gap;
                commands.push(FlowCommand::SetPictureFloating { at: p.at, anchor: Some(p.anchor.clone()) });
            }
        } else {
            let step = if max_y > min_y { (max_y - min_y) / ((n - 1) as i64) } else { 100_000 };
            for (i, p) in pics.iter_mut().enumerate() {
                let new_y = min_y + step * (i as i64);
                p.anchor.offset_y = new_y;
                p.anchor.align_v = None;
                commands.push(FlowCommand::SetPictureFloating { at: p.at, anchor: Some(p.anchor.clone()) });
            }
        }
        self.run_flow(commands, cx);
    }

    /// Set alignment reference ("margin" or "page") for floating picture at caret.
    pub(crate) fn set_picture_align_relative(&mut self, relative: &str, cx: &mut Context<Self>) {
        let Some((at, ..)) = self.picture_at_caret() else { return };
        let mut anchor = self.picture_floating_at_caret().unwrap_or_default();
        anchor.relative_h = relative.to_string();
        anchor.relative_v = relative.to_string();
        self.run_flow(vec![FlowCommand::SetPictureFloating { at, anchor: Some(anchor) }], cx);
    }

    /// Change the look of the picture at the caret by `f` (one step to undo).
    pub(crate) fn change_picture_look(&mut self,f:impl FnOnce(&mut PictureLook),cx:&mut Context<Self>) {
        let (Some((at,..)),Some(mut look))=(self.picture_at_caret(),self.picture_look_at_caret()) else{return};
        f(&mut look);
        self.run_flow(vec![FlowCommand::SetPictureLook {at,look}],cx);
        cx.notify();
    }
    /// Outline geometry of the picture at caret, if any preset was applied.
    pub(crate) fn picture_geometry_at_caret(&self) -> Option<ibis_doc_model::Geometry> {
        self.picture_look_at_caret().and_then(|l| l.geometry)
    }

    /// Preset name of the picture crop shape at caret (e.g. "roundRect", "ellipse", "heart"),
    /// or None if rect/default.
    pub(crate) fn picture_geometry_preset_at_caret(&self) -> Option<&'static str> {
        let geom = self.picture_geometry_at_caret()?;
        match &geom {
            ibis_doc_model::Geometry::Rect => None,
            ibis_doc_model::Geometry::RoundRect { .. } => Some("roundRect"),
            ibis_doc_model::Geometry::Ellipse => Some("ellipse"),
            ibis_doc_model::Geometry::Formula(f) => match f.preset.as_deref() {
                Some("triangle") => Some("triangle"),
                Some("diamond") => Some("diamond"),
                Some("parallelogram") => Some("parallelogram"),
                Some("trapezoid") => Some("trapezoid"),
                Some("hexagon") => Some("hexagon"),
                Some("octagon") => Some("octagon"),
                Some("heart") => Some("heart"),
                Some("star5") => Some("star5"),
                Some("cloud") => Some("cloud"),
                Some("rightArrow") => Some("rightArrow"),
                Some("teardrop") => Some("teardrop"),
                Some("wedgeRoundRectCallout") => Some("wedgeRoundRectCallout"),
                _ => None,
            },
        }
    }

    /// 図形に合わせてトリミング (DOCX-IMAGE-005): crop picture to preset shape (None or "rect" resets to rect).
    pub(crate) fn crop_picture_to_shape(&mut self, preset: Option<&str>, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            l.geometry = match preset {
                None | Some("rect") => None,
                Some(name) => ibis_pptx::geometry::preset(name),
            };
        }, cx);
    }

    /// 図のリセット (DOCX-IMAGE-023): the picture's turn, flips, crop, border, brightness and contrast and recolouring are taken
    /// back; its size, name and alternative text stay. One step of undo.
    pub(crate) fn reset_picture(&mut self, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            *l = PictureLook { alt_text: std::mem::take(&mut l.alt_text), decorative: l.decorative, name: std::mem::take(&mut l.name), linked_file: l.linked_file.take(), ..Default::default() };
        }, cx);
    }
    /// Outline (border) of the picture at the caret, if any.
    pub(crate) fn picture_stroke_at_caret(&self)->Option<Stroke> {
        self.picture_look_at_caret().and_then(|l| l.stroke)
    }
    /// 明るさ・コントラスト: the picture's brightness (`bright`) or contrast changed by `delta` percent (±100).
    pub(crate) fn adjust_picture_luminance(&mut self, bright: bool, delta: i32, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            let (b, c) = l.luminance.unwrap_or((0, 0));
            let step = delta * 1000;
            let (b, c) = if bright { ((b + step).clamp(-100_000, 100_000), c) } else { (b, (c + step).clamp(-100_000, 100_000)) };
            l.luminance = (b != 0 || c != 0).then_some((b, c));
        }, cx);
    }
    /// 色: the picture recoloured or as it was (`None`).
    pub(crate) fn recolor_picture(&mut self, kind: Option<PictureRecolor>, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            match kind {
                None => {
                    l.duotone = None;
                    l.effects.bilevel = None;
                    l.effects.grayscale = false;
                }
                Some(PictureRecolor::Grayscale) => {
                    l.duotone = Some((Rgb(0x000000), Rgb(0xFFFFFF)));
                    l.effects.bilevel = None;
                    l.effects.grayscale = true;
                }
                Some(PictureRecolor::Sepia) => {
                    l.duotone = Some((Rgb(0x3F2300), Rgb(0xFFF3D6)));
                    l.effects.bilevel = None;
                    l.effects.grayscale = false;
                }
                Some(PictureRecolor::Washout) => {
                    l.duotone = None;
                    l.effects.bilevel = None;
                    l.effects.grayscale = false;
                    l.luminance = Some((70_000, -70_000));
                }
                Some(PictureRecolor::BlackAndWhite50) => {
                    l.duotone = None;
                    l.effects.bilevel = Some(50_000);
                    l.effects.grayscale = false;
                }
                Some(PictureRecolor::Blue) => {
                    l.duotone = Some((Rgb(0x002060), Rgb(0xD9E1F2)));
                    l.effects.bilevel = None;
                    l.effects.grayscale = false;
                }
                Some(PictureRecolor::Green) => {
                    l.duotone = Some((Rgb(0x004010), Rgb(0xE2EFDA)));
                    l.effects.bilevel = None;
                    l.effects.grayscale = false;
                }
                Some(PictureRecolor::Red) => {
                    l.duotone = Some((Rgb(0x600000), Rgb(0xFCE4D6)));
                    l.effects.bilevel = None;
                    l.effects.grayscale = false;
                }
                Some(PictureRecolor::Orange) => {
                    l.duotone = Some((Rgb(0x803000), Rgb(0xFFE6CC)));
                    l.effects.bilevel = None;
                    l.effects.grayscale = false;
                }
                Some(PictureRecolor::Purple) => {
                    l.duotone = Some((Rgb(0x400060), Rgb(0xF2E6FF)));
                    l.effects.bilevel = None;
                    l.effects.grayscale = false;
                }
            }
        }, cx);
    }
    /// 透明度: the picture's opacity (0 to 100 000, 100 000 = 100% opaque, None = opaque).
    pub(crate) fn set_picture_opacity(&mut self, opacity: Option<i32>, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            l.effects.opacity = opacity.filter(|&a| a < 100_000);
        }, cx);
    }
    /// 透明度 of the picture at caret.
    pub(crate) fn picture_opacity_at_caret(&self) -> Option<i32> {
        self.picture_look_at_caret().and_then(|l| l.effects.opacity)
    }
    /// 色の彩度: the picture's saturation (0 to 400 000, 100 000 = 100% normal, None = normal).
    pub(crate) fn set_picture_saturation(&mut self, saturation: Option<i32>, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            l.effects.saturation = saturation.filter(|&s| s != 100_000);
        }, cx);
    }
    /// 色の彩度 of the picture at caret.
    pub(crate) fn picture_saturation_at_caret(&self) -> Option<i32> {
        self.picture_look_at_caret().and_then(|l| l.effects.saturation)
    }
    /// ぼかし・ソフトエッジ: the soft edge blur radius in EMU.
    pub(crate) fn set_picture_soft_edge(&mut self, rad: Option<i64>, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            l.soft_edge = rad.filter(|&r| r > 0);
        }, cx);
    }
    /// ぼかし・ソフトエッジ of the picture at caret.
    pub(crate) fn picture_soft_edge_at_caret(&self) -> Option<i64> {
        self.picture_look_at_caret().and_then(|l| l.soft_edge)
    }
    /// Set or change the border width (EMU) of the picture at the caret.
    pub(crate) fn set_picture_border_width(&mut self, width: i64, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            if let Some(s) = l.stroke.as_mut() {
                s.width = width;
            } else {
                l.stroke = Some(Stroke::solid(Color::BLACK, width));
            }
        }, cx);
    }
    /// Set or change the border color of the picture at the caret.
    pub(crate) fn set_picture_border_color(&mut self, color: Color, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            if let Some(s) = l.stroke.as_mut() {
                s.color = color;
            } else {
                l.stroke = Some(Stroke::solid(color, Stroke::DEFAULT_WIDTH));
            }
        }, cx);
    }
    /// Set or change the border dash style of the picture at the caret.
    pub(crate) fn set_picture_border_dash(&mut self, dash: LineDash, cx: &mut Context<Self>) {
        self.change_picture_look(|l| {
            if let Some(s) = l.stroke.as_mut() {
                s.dash = dash;
                s.dashed = dash != LineDash::Solid;
            } else {
                l.stroke = Some(Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH).with_dash(dash));
            }
        }, cx);
    }
    /// Remove the border around the picture at the caret.
    pub(crate) fn clear_picture_border(&mut self, cx: &mut Context<Self>) {
        self.change_picture_look(|l| l.stroke = None, cx);
    }
    /// Turn the picture a quarter (`turns` 1 clockwise, -1 anticlockwise).
    pub(crate) fn turn_picture(&mut self,turns:i32,cx:&mut Context<Self>) {
        self.change_picture_look(|l|l.rotation=(l.rotation+turns*5_400_000).rem_euclid(21_600_000),cx);
    }
    pub(crate) fn flip_picture(&mut self,horizontal:bool,cx:&mut Context<Self>) {
        self.change_picture_look(|l|if horizontal {l.flip_h=!l.flip_h} else {l.flip_v=!l.flip_v},cx);
    }
    /// Cut the picture as the four fields say (percent of its size; each 0 to 99, together under 100 a side).
    pub(crate) fn apply_picture_crop(&mut self,cx:&mut Context<Self>) {
        let Some(fields)=self.doc_state().and_then(|s|s.picture_edit.as_ref()).map(|e|e.crop.clone()) else{return};
        let values=fields.map(|f|f.read(cx).value().trim().parse::<f64>().ok().filter(|v|v.is_finite()&&*v>=0.&&*v<100.).map(|v|(v*1000.).round() as i32));
        let [Some(top),Some(bottom),Some(left),Some(right)]=values else {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {edit.error=Some(crate::shell::i18n::text("fix.picture-panel.enter-crop-number-0-up").into());}
            cx.notify();return;
        };
        if top+bottom>=100_000||left+right>=100_000 {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {edit.error=Some(crate::shell::i18n::text("fix.picture-panel.top-plus-bottom-left-plus").into());}
            cx.notify();return;
        }
        if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {edit.error=None;}
        self.change_picture_look(|l|l.crop=Crop {left,top,right,bottom},cx);
    }
    pub(crate) fn toggle_picture_ratio(&mut self,cx:&mut Context<Self>) {
        if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {edit.keep_ratio=!edit.keep_ratio;if edit.keep_ratio{edit.ratio_size=edit.target.2;}}
        cx.notify();
    }
    pub(crate) fn apply_picture_edit(&mut self,window:&mut Window,cx:&mut Context<Self>) {
        let Some(edit)=self.doc_state().and_then(|s|s.picture_edit.as_ref()) else{return};
        let values=edit.fields.clone().map(|f|f.read(cx).value().trim().to_string());
        let target=edit.target;
        let ratio=edit.ratio_size;
        let changed=[values[0]!=edit.initial[0],values[1]!=edit.initial[1]];
        let parsed=values.clone().map(|v|v.parse::<f64>().ok().filter(|v|v.is_finite() && *v>=0.01 && *v<=1584.));
        let mut size=match parsed { [Some(w),Some(h)]=>Some(EmuSize::new((w*12700.).round() as i64,(h*12700.).round() as i64)),_=>None };
        if edit.keep_ratio && changed.iter().any(|v|*v) {
            size=size.and_then(|mut size|{
                if ratio.width<=0 || ratio.height<=0 {return None;}
                match changed {
                    [true,false]=>size.height=(size.width as f64*ratio.height as f64/ratio.width as f64).round() as i64,
                    [false,true]=>size.width=(size.height as f64*ratio.width as f64/ratio.height as f64).round() as i64,
                    [true,true]=>if (size.width as f64/size.height as f64-ratio.width as f64/ratio.height as f64).abs()>0.0001 {return None;},
                    _=>{}
                }
                Some(size)
            });
        }
        let size=size.filter(|s|s.width>=127 && s.height>=127 && s.width<=20116800 && s.height<=20116800);
        if self.picture_at_caret()!=Some(target) || size.is_none() {
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {edit.error=Some(crate::shell::i18n::text("docx.picture-panel.check-picture-selection-value").into());}
            cx.notify();return;
        }
        let size=size.unwrap();
        if !changed.iter().any(|v|*v) {
            self.apply_picture_alt_text(cx);
            self.focus_handle.focus(window,cx);
            return;
        }
        if self.run_flow(vec![FlowCommand::SetPictureSize {at:target.0,size}],cx) {
            let values=[format!("{:.2}",size.width as f64/12700.),format!("{:.2}",size.height as f64/12700.)];
            if let Some(edit)=self.doc_state_mut().and_then(|s|s.picture_edit.as_mut()) {
                edit.target=(target.0,target.1,size);edit.initial=values.clone();edit.error=None;
                let fields=edit.fields.clone();
                for (field,value) in fields.into_iter().zip(values) {field.update(cx,|i,cx|i.set_value(value,window,cx));}
            }
            self.apply_picture_alt_text(cx);
            self.apply_picture_name(cx);
            self.focus_handle.focus(window,cx);
        }
        cx.notify();
    }
    pub(crate) fn picture_name_at_caret(&self) -> Option<String> {
        self.picture_look_at_caret().map(|l| l.name)
    }
    pub(crate) fn set_picture_name(&mut self, name: String, cx: &mut Context<Self>) {
        self.change_picture_look(|l| l.name = name, cx);
    }
    pub(crate) fn apply_picture_name(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state().and_then(|s| s.picture_edit.as_ref()) else { return };
        let new_name = edit.name_field.read(cx).value().trim().to_string();
        if new_name != edit.initial_name {
            self.set_picture_name(new_name.clone(), cx);
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.picture_edit.as_mut()) {
                edit.initial_name = new_name;
            }
        }
    }
    pub(crate) fn open_picture_alt_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.open_picture_edit(window, cx);
        if let Some(state) = self.doc_state().and_then(|s| s.picture_edit.as_ref()) {
            state.alt_desc.update(cx, |f, cx| f.focus(window, cx));
        }
    }
    pub(crate) fn picture_alt_text_at_caret(&self) -> Option<ibis_doc_model::AltText> {
        self.picture_look_at_caret().map(|l| l.alt_text)
    }
    pub(crate) fn set_picture_alt_text(&mut self, alt_text: ibis_doc_model::AltText, cx: &mut Context<Self>) {
        self.change_picture_look(|l| l.alt_text = alt_text, cx);
    }
    pub(crate) fn picture_decorative_at_caret(&self) -> Option<bool> {
        self.picture_look_at_caret().map(|l| l.decorative)
    }
    pub(crate) fn set_picture_decorative(&mut self, decorative: bool, cx: &mut Context<Self>) {
        self.change_picture_look(|l| l.decorative = decorative, cx);
    }
    pub(crate) fn toggle_picture_decorative(&mut self, cx: &mut Context<Self>) {
        let (current_dec, has_panel) = if let Some(edit) = self.doc_state().and_then(|s| s.picture_edit.as_ref()) {
            (edit.decorative, true)
        } else {
            (self.picture_decorative_at_caret().unwrap_or(false), false)
        };
        let new_dec = !current_dec;
        self.set_picture_decorative(new_dec, cx);
        if has_panel {
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.picture_edit.as_mut()) {
                edit.decorative = new_dec;
                edit.initial_decorative = new_dec;
            }
        }
        cx.notify();
    }
    pub(crate) fn apply_picture_alt_text(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.doc_state().and_then(|s| s.picture_edit.as_ref()) else { return };
        if edit.decorative {
            return;
        }
        let title = edit.alt_title.read(cx).value().trim().to_string();
        let description = edit.alt_desc.read(cx).value().trim().to_string();
        let new_alt = ibis_doc_model::AltText { title, description };
        if new_alt != edit.initial_alt {
            self.set_picture_alt_text(new_alt.clone(), cx);
            if let Some(edit) = self.doc_state_mut().and_then(|s| s.picture_edit.as_mut()) {
                edit.initial_alt = new_alt;
            }
        }
    }
    pub(crate) fn picture_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.doc_state()?.picture_edit.as_ref()?;
        self.render_picture_panel(cx)
    }

    #[inline(never)]
    fn render_picture_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        body = self.picture_panel_size_section(body, cx)?;
        body = self.picture_panel_color_section(body, cx);
        body = self.picture_panel_effects_section(body, cx)?;
        Some(SidebarPanel { title: crate::shell::i18n::text("docx.picture-panel.picture-size"), body: body.into_any_element() })
    }

    #[inline(never)]
    fn picture_panel_size_section(&mut self, body: Div, cx: &mut Context<Self>) -> Option<Div> {
        let edit = self.doc_state()?.picture_edit.as_ref()?;
        let name_field = edit.name_field.clone();
        let fields = edit.fields.clone();
        let crop = edit.crop.clone();
        let ratio = edit.keep_ratio;
        let error = edit.error.clone();
        let geom_preset = self.picture_geometry_preset_at_caret();
        let row = |label: &'static str, field: &Entity<InputState>| h_flex().gap_2().child(div().w(px(80.)).text_sm().child(crate::shell::i18n::text(label))).child(crate::shell::chrome::text_field(field).full_width());
        let field_cx = &*cx;
        type Step = fn(&mut SlideEditor, f32, &mut Window, &mut Context<SlideEditor>);
        let spin_row = |label: &'static str, field: &Entity<InputState>, id: &'static str, spec: crate::shell::chrome::SpinnerSpec, on_step: Step| h_flex().gap_2()
            .child(div().w(px(80.)).text_sm().child(crate::shell::i18n::text(label)))
            .child(crate::shell::chrome::text_field(field).full_width().unit(if id.starts_with("picture-crop"){"%"}else{"pt"},px(22.)).spinner(id, spec, field_cx, on_step));
        // Size (pt): Enter applies it, so does ▲▼. Crop (%): applied by its button only, ▲▼ only change the number.
        let size_spec = crate::shell::chrome::SpinnerSpec::new(1.0).range(1.0, 1584.0);
        let crop_spec = crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 99.0);
        let apply_size: Step = |this, _, window, cx| this.apply_picture_edit(window, cx);
        let no_apply: Step = |e, _, _, cx| e.apply_picture_crop(cx);
        Some(
            body.child(crate::shell::chrome::text_field_row(crate::shell::i18n::text("docx.picture.name"),crate::shell::chrome::text_field(&name_field).full_width()).flex_col().items_start().gap_1())
                .child(spin_row("docx.picture-panel.width-pt", &fields[0], "picture-width", size_spec, apply_size))
                .child(spin_row("docx.picture-panel.height-pt", &fields[1], "picture-height", size_spec, apply_size))
                .child(crate::shell::chrome::checkbox_button("picture-ratio",crate::shell::i18n::text("docx.picture-panel.lock-aspect-ratio"),ratio,false,cx.listener(|this,_,_,cx|this.toggle_picture_ratio(cx))))
                .children(error.map(|e| div().text_sm().child(e)))
                .child(h_flex().gap_2()

                    .child(crate::shell::chrome::action_button("picture-replace").small().label(crate::shell::i18n::text("_docx.change-picture")).on_click(cx.listener(|this, _, window, cx| this.doc_pick_replacement_picture(window, cx))))
                    .child(crate::shell::chrome::action_button("picture-compress").small().label(crate::shell::i18n::text("docx.picture.compress")).on_click(cx.listener(|this, _, window, cx| this.open_docx_compress_panel(window, cx)))))
                .child(div().pt_2().text_sm().child(crate::shell::i18n::text("fix.picture-panel.rotate-flip")))
                .child(h_flex().gap_1().flex_wrap()
                    .child(crate::shell::chrome::action_button("picture-turn-left").small().icon(crate::shell::icons::Material("rotate_90_degrees_ccw")).tooltip(crate::shell::i18n::text("fix.picture-panel.rotate-left-90")).on_click(cx.listener(|this, _, _, cx| this.turn_picture(-1, cx))))
                    .child(crate::shell::chrome::action_button("picture-turn-right").small().icon(crate::shell::icons::Material("rotate_90_degrees_cw")).tooltip(crate::shell::i18n::text("fix.picture-panel.rotate-right-90")).on_click(cx.listener(|this, _, _, cx| this.turn_picture(1, cx))))
                    .child(crate::shell::chrome::action_button("picture-flip-h").small().icon(crate::shell::icons::Material("flip")).tooltip(crate::shell::i18n::text("fix.picture-panel.flip-horizontal")).on_click(cx.listener(|this, _, _, cx| this.flip_picture(true, cx))))
                    .child(crate::shell::chrome::action_button("picture-flip-v").small().icon(Icon::new(crate::shell::icons::Material("flip")).transform(gpui::Transformation::rotate(gpui::radians(std::f32::consts::FRAC_PI_2)))).tooltip(crate::shell::i18n::text("fix.picture-panel.flip-vertical")).on_click(cx.listener(|this, _, _, cx| this.flip_picture(false, cx)))))
                .child(crate::shell::chrome::action_button("picture-reset").small().label(crate::shell::i18n::text("docx.picture.reset")).on_click(cx.listener(|this, _, _, cx| this.reset_picture(cx))))
                .child(div().pt_2().text_sm().child(crate::shell::i18n::text("fix.picture-panel.crop")))
                .child(spin_row("_shell.top", &crop[0], "picture-crop-top", crop_spec, no_apply))
                .child(spin_row("_shell.bottom", &crop[1], "picture-crop-bottom", crop_spec, no_apply))
                .child(spin_row("_shell.left", &crop[2], "picture-crop-left", crop_spec, no_apply))
                .child(spin_row("_shell.right", &crop[3], "picture-crop-right", crop_spec, no_apply))
                .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture.crop-to-shape.label")))
                .child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                    .child(crate::shell::chrome::radio_button("pic-crop-rect",geom_preset.is_none(),false,crate::shell::i18n::text("docx.picture.crop-to-shape.rect"),cx.listener(|this, _, _, cx| this.crop_picture_to_shape(None, cx))))
                    .child(crate::shell::chrome::radio_button("pic-crop-round-rect",geom_preset == Some("roundRect"),false,crate::shell::i18n::text("docx.picture.crop-to-shape.round-rect"),cx.listener(|this, _, _, cx| this.crop_picture_to_shape(Some("roundRect"), cx))))
                    .child(crate::shell::chrome::radio_button("pic-crop-ellipse",geom_preset == Some("ellipse"),false,crate::shell::i18n::text("docx.picture.crop-to-shape.ellipse"),cx.listener(|this, _, _, cx| this.crop_picture_to_shape(Some("ellipse"), cx))))
                    .child(crate::shell::chrome::radio_button("pic-crop-triangle",geom_preset == Some("triangle"),false,crate::shell::i18n::text("docx.picture.crop-to-shape.triangle"),cx.listener(|this, _, _, cx| this.crop_picture_to_shape(Some("triangle"), cx))))
                    .child(crate::shell::chrome::radio_button("pic-crop-diamond",geom_preset == Some("diamond"),false,crate::shell::i18n::text("docx.picture.crop-to-shape.diamond"),cx.listener(|this, _, _, cx| this.crop_picture_to_shape(Some("diamond"), cx))))
                    .child(crate::shell::chrome::radio_button("pic-crop-heart",geom_preset == Some("heart"),false,crate::shell::i18n::text("docx.picture.crop-to-shape.heart"),cx.listener(|this, _, _, cx| this.crop_picture_to_shape(Some("heart"), cx))))
                    .child(crate::shell::chrome::radio_button("pic-crop-star5",geom_preset == Some("star5"),false,crate::shell::i18n::text("docx.picture.crop-to-shape.star5"),cx.listener(|this, _, _, cx| this.crop_picture_to_shape(Some("star5"), cx))))
                    .child(crate::shell::chrome::radio_button("pic-crop-cloud",geom_preset == Some("cloud"),false,crate::shell::i18n::text("docx.picture.crop-to-shape.cloud"),cx.listener(|this, _, _, cx| this.crop_picture_to_shape(Some("cloud"), cx))))),
        )
    }

    #[inline(never)]
    fn picture_panel_color_section(&mut self, body: Div, cx: &mut Context<Self>) -> Div {
        let duotone_now = self.picture_look_at_caret().and_then(|l| l.duotone);
        let saturation_now = self.picture_saturation_at_caret();
        let bilevel_now = self.picture_look_at_caret().and_then(|l| l.effects.bilevel);
        let lum_now = self.picture_look_at_caret().and_then(|l| l.luminance);
        body.child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture-panel.picture-corrections")))
            .child(h_flex().gap_1().flex_wrap()
                .child(crate::shell::chrome::action_button("pic-bright-down").small().label(crate::shell::i18n::text("docx.picture-panel.brightness-10-percent")).on_click(cx.listener(|this, _, _, cx| this.adjust_picture_luminance(true, -10, cx))))
                .child(crate::shell::chrome::action_button("pic-bright-up").small().label(crate::shell::i18n::text("docx.picture-panel.brightness-plus-10-percent")).on_click(cx.listener(|this, _, _, cx| this.adjust_picture_luminance(true, 10, cx))))
                .child(crate::shell::chrome::action_button("pic-contrast-down").small().label(crate::shell::i18n::text("docx.picture-panel.contrast-10-percent")).on_click(cx.listener(|this, _, _, cx| this.adjust_picture_luminance(false, -10, cx))))
                .child(crate::shell::chrome::action_button("pic-contrast-up").small().label(crate::shell::i18n::text("docx.picture-panel.contrast-plus-10-percent")).on_click(cx.listener(|this, _, _, cx| this.adjust_picture_luminance(false, 10, cx)))))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture-panel.color-recolor")))
            .child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                .child(crate::shell::chrome::radio_button("pic-recolor-none",duotone_now.is_none() && bilevel_now.is_none() && lum_now != Some((70_000, -70_000)),false,crate::shell::i18n::text("docx.picture-panel.no-recolor"),cx.listener(|this, _, _, cx| this.recolor_picture(None, cx))))
                .child(crate::shell::chrome::radio_button("pic-recolor-gray",duotone_now == Some((Rgb(0x000000), Rgb(0xFFFFFF))),false,crate::shell::i18n::text("pptx.print-panel.grayscale"),cx.listener(|this, _, _, cx| this.recolor_picture(Some(PictureRecolor::Grayscale), cx))))
                .child(crate::shell::chrome::radio_button("pic-recolor-sepia",duotone_now == Some((Rgb(0x3F2300), Rgb(0xFFF3D6))),false,crate::shell::i18n::text("docx.picture-panel.sepia"),cx.listener(|this, _, _, cx| this.recolor_picture(Some(PictureRecolor::Sepia), cx))))
                .child(crate::shell::chrome::radio_button("pic-recolor-washout",lum_now == Some((70_000, -70_000)),false,crate::shell::i18n::text("docx.picture-panel.washout"),cx.listener(|this, _, _, cx| this.recolor_picture(Some(PictureRecolor::Washout), cx))))
                .child(crate::shell::chrome::radio_button("pic-recolor-bw50",bilevel_now == Some(50_000),false,crate::shell::i18n::text("docx.picture-panel.black-and-white"),cx.listener(|this, _, _, cx| this.recolor_picture(Some(PictureRecolor::BlackAndWhite50), cx))))
                .child(crate::shell::chrome::radio_button("pic-recolor-blue",duotone_now == Some((Rgb(0x002060), Rgb(0xD9E1F2))),false,crate::shell::i18n::text("_docx.blue"),cx.listener(|this, _, _, cx| this.recolor_picture(Some(PictureRecolor::Blue), cx))))
                .child(crate::shell::chrome::radio_button("pic-recolor-green",duotone_now == Some((Rgb(0x004010), Rgb(0xE2EFDA))),false,crate::shell::i18n::text("_docx.green"),cx.listener(|this, _, _, cx| this.recolor_picture(Some(PictureRecolor::Green), cx))))
                .child(crate::shell::chrome::radio_button("pic-recolor-red",duotone_now == Some((Rgb(0x600000), Rgb(0xFCE4D6))),false,crate::shell::i18n::text("_docx.red"),cx.listener(|this, _, _, cx| this.recolor_picture(Some(PictureRecolor::Red), cx))))
                .child(crate::shell::chrome::radio_button("pic-recolor-orange",duotone_now == Some((Rgb(0x803000), Rgb(0xFFE6CC))),false,crate::shell::i18n::text("_docx.orange"),cx.listener(|this, _, _, cx| this.recolor_picture(Some(PictureRecolor::Orange), cx))))
                .child(crate::shell::chrome::radio_button("pic-recolor-purple",duotone_now == Some((Rgb(0x400060), Rgb(0xF2E6FF))),false,crate::shell::i18n::text("docx.picture-panel.purple"),cx.listener(|this, _, _, cx| this.recolor_picture(Some(PictureRecolor::Purple), cx)))))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture-panel.saturation")))
            .child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-sat-0","0%",saturation_now == Some(0)).on_click(cx.listener(|this, _, _, cx| this.set_picture_saturation(Some(0), cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-sat-50","50%",saturation_now == Some(50_000)).on_click(cx.listener(|this, _, _, cx| this.set_picture_saturation(Some(50_000), cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-sat-100","100%",saturation_now.is_none() || saturation_now == Some(100_000)).on_click(cx.listener(|this, _, _, cx| this.set_picture_saturation(None, cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-sat-200","200%",saturation_now == Some(200_000)).on_click(cx.listener(|this, _, _, cx| this.set_picture_saturation(Some(200_000), cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-sat-300","300%",saturation_now == Some(300_000)).on_click(cx.listener(|this, _, _, cx| this.set_picture_saturation(Some(300_000), cx)))))
    }

    #[inline(never)]
    fn picture_panel_effects_section(&mut self, body: Div, cx: &mut Context<Self>) -> Option<Div> {
        let edit = self.doc_state()?.picture_edit.as_ref()?;
        let alt_title = edit.alt_title.clone();
        let alt_desc = edit.alt_desc.clone();
        let decorative = edit.decorative;
        let stroke_now = self.picture_stroke_at_caret();
        let opacity_now = self.picture_opacity_at_caret();
        let soft_edge_now = self.picture_soft_edge_at_caret();
        let row = |label: &'static str, field: &Entity<InputState>| h_flex().gap_2().child(div().w(px(80.)).text_sm().child(crate::shell::i18n::text(label))).child(crate::shell::chrome::text_field(field).full_width());
        let mut body = body
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture-panel.transparency")))
            .child(crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                .child(crate::shell::chrome::radio_button("pic-trans-0",opacity_now.is_none(),false,"0%",cx.listener(|this, _, _, cx| this.set_picture_opacity(None, cx))))
                .child(crate::shell::chrome::radio_button("pic-trans-15",opacity_now == Some(85_000),false,"15%",cx.listener(|this, _, _, cx| this.set_picture_opacity(Some(85_000), cx))))
                .child(crate::shell::chrome::radio_button("pic-trans-30",opacity_now == Some(70_000),false,"30%",cx.listener(|this, _, _, cx| this.set_picture_opacity(Some(70_000), cx))))
                .child(crate::shell::chrome::radio_button("pic-trans-50",opacity_now == Some(50_000),false,"50%",cx.listener(|this, _, _, cx| this.set_picture_opacity(Some(50_000), cx))))
                .child(crate::shell::chrome::radio_button("pic-trans-65",opacity_now == Some(35_000),false,"65%",cx.listener(|this, _, _, cx| this.set_picture_opacity(Some(35_000), cx))))
                .child(crate::shell::chrome::radio_button("pic-trans-80",opacity_now == Some(20_000),false,"80%",cx.listener(|this, _, _, cx| this.set_picture_opacity(Some(20_000), cx)))))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture-panel.soft-edge")))
            .child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-soft-0",crate::shell::i18n::text("pptx.chrome.no-border"),soft_edge_now.is_none()).on_click(cx.listener(|this, _, _, cx| this.set_picture_soft_edge(None, cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-soft-1","1 pt",soft_edge_now == Some(12_700)).on_click(cx.listener(|this, _, _, cx| this.set_picture_soft_edge(Some(12_700), cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-soft-2","2.5 pt",soft_edge_now == Some(31_750)).on_click(cx.listener(|this, _, _, cx| this.set_picture_soft_edge(Some(31_750), cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-soft-5","5 pt",soft_edge_now == Some(63_500)).on_click(cx.listener(|this, _, _, cx| this.set_picture_soft_edge(Some(63_500), cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-soft-10","10 pt",soft_edge_now == Some(127_000)).on_click(cx.listener(|this, _, _, cx| this.set_picture_soft_edge(Some(127_000), cx)))))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture.border.label")))
            .child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-border-none",crate::shell::i18n::text("pptx.chrome.no-border"),stroke_now.is_none()).on_click(cx.listener(|this, _, _, cx| this.clear_picture_border(cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-border-w1","1 pt",stroke_now.as_ref().is_some_and(|s| s.width == 12_700)).on_click(cx.listener(|this, _, _, cx| this.set_picture_border_width(12_700, cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-border-w2","1.5 pt",stroke_now.as_ref().is_some_and(|s| s.width == 19_050)).on_click(cx.listener(|this, _, _, cx| this.set_picture_border_width(19_050, cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-border-w3","3 pt",stroke_now.as_ref().is_some_and(|s| s.width == 38_100)).on_click(cx.listener(|this, _, _, cx| this.set_picture_border_width(38_100, cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("pic-border-w6","6 pt",stroke_now.as_ref().is_some_and(|s| s.width == 76_200)).on_click(cx.listener(|this, _, _, cx| this.set_picture_border_width(76_200, cx)))))
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture.alt-text")))
            .child(crate::shell::chrome::checkbox_button("picture-decorative",crate::shell::i18n::text("docx.picture.decorative"),decorative,false,cx.listener(|this, _, _, cx| this.toggle_picture_decorative(cx))));
        if decorative {
            body = body.child(div().text_xs().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("docx.picture.decorative.hint")));
        } else {
            body = body
                .child(div().text_xs().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("docx.picture.alt-text.hint")))
                .child(row("docx.picture.alt-text.title", &alt_title))
                .child(row("docx.picture.alt-text.desc", &alt_desc))
;
        }
        let editing_wp = self.is_editing_wrap_points();
        let poly_opt = self.picture_wrap_polygon_at_caret();
        body = body
            .child(div().pt_2().text_sm().child(crate::shell::i18n::text("docx.picture.edit-wrap-points")))
            .child(h_flex().gap_1().flex_wrap()
                .child(crate::shell::chrome::checkbox_button("pic-wrap-points-toggle",crate::shell::i18n::text("docx.picture.edit-wrap-points"),editing_wp,false,cx.listener(|this, _, _, cx| this.toggle_edit_wrap_points(cx))))
                .child(crate::shell::chrome::action_button("pic-wrap-points-rect").small().label(crate::shell::i18n::text("docx.picture.wrap-points.rect")).on_click(cx.listener(|this, _, _, cx| { this.apply_picture_wrap_polygon_preset("rect", cx); })))
                .child(crate::shell::chrome::action_button("pic-wrap-points-diamond").small().label(crate::shell::i18n::text("docx.picture.wrap-points.diamond")).on_click(cx.listener(|this, _, _, cx| { this.apply_picture_wrap_polygon_preset("diamond", cx); })))
                .child(crate::shell::chrome::action_button("pic-wrap-points-octagon").small().label(crate::shell::i18n::text("docx.picture.wrap-points.octagon")).on_click(cx.listener(|this, _, _, cx| { this.apply_picture_wrap_polygon_preset("octagon", cx); })))
                .child(crate::shell::chrome::action_button("pic-wrap-points-inset").small().label(crate::shell::i18n::text("docx.picture.wrap-points.inset")).on_click(cx.listener(|this, _, _, cx| { this.apply_picture_wrap_polygon_preset("inset", cx); })))
                .child(crate::shell::chrome::action_button("pic-wrap-points-reset").small().label(crate::shell::i18n::text("docx.picture.wrap-points.reset")).on_click(cx.listener(|this, _, _, cx| { this.reset_picture_wrap_points(cx); }))));
        if editing_wp || poly_opt.is_some() {
            let poly = poly_opt.unwrap_or_else(ibis_doc_model::WrapPolygon::default_rect_21600);
            let mut vertices = vec![poly.start];
            vertices.extend(poly.line_to.iter().copied());
            for (idx, pt) in vertices.iter().enumerate().take(8) {
                let pct_x = (pt.x as f64 / 216.0).round() as i32;
                let pct_y = (pt.y as f64 / 216.0).round() as i32;
                body = body.child(
                    h_flex().gap_1().items_center()
                        .child(div().w(px(88.)).text_xs().child(format!("#{idx} ({pct_x}%, {pct_y}%)")))
                        .child(crate::shell::chrome::action_button(("wp-left", idx)).small().label("←").on_click(cx.listener(move |this, _, _, cx| { this.nudge_picture_wrap_point(idx, -1080, 0, cx); })))
                        .child(crate::shell::chrome::action_button(("wp-right", idx)).small().label("→").on_click(cx.listener(move |this, _, _, cx| { this.nudge_picture_wrap_point(idx, 1080, 0, cx); })))
                        .child(crate::shell::chrome::action_button(("wp-up", idx)).small().label("↑").on_click(cx.listener(move |this, _, _, cx| { this.nudge_picture_wrap_point(idx, 0, -1080, cx); })))
                        .child(crate::shell::chrome::action_button(("wp-down", idx)).small().label("↓").on_click(cx.listener(move |this, _, _, cx| { this.nudge_picture_wrap_point(idx, 0, 1080, cx); })))
                        .child(crate::shell::chrome::action_button(("wp-del", idx)).small().label("-").disabled(idx == 0 || poly.line_to.len() <= 3).on_click(cx.listener(move |this, _, _, cx| { this.remove_picture_wrap_point(idx, cx); })))
                );
            }
            body = body.child(
                crate::shell::chrome::action_button("pic-wrap-points-add")
                    .small()
                    .label(crate::shell::i18n::text("docx.picture.wrap-points.add-vertex"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.insert_picture_wrap_point(1, ibis_doc_model::WrapPoint::new(10_800, 2_700), cx);
                    })),
            );
        }
        Some(body)
    }

    /// Collect all pictures/shapes in the document for the Selection Pane.
    pub(crate) fn document_picture_objects(&self) -> Vec<PictureObjectInfo> {
        let mut list = Vec::new();
        let Some(flow) = self.flow() else { return list };
        for b in &flow.body {
            if let ibis_doc_model::Block::Paragraph(p) = b {
                let mut offset = 0;
                for r in &p.runs {
                    match &r.kind {
                        FlowRunKind::Picture { size, look, .. } | FlowRunKind::PictureControl { size, look, .. } => {
                            let name = if look.name.is_empty() {
                                crate::shell::i18n::format("_docx.picture-n", &[("n", &(list.len() + 1))])
                            } else {
                                look.name.clone()
                            };
                            list.push(PictureObjectInfo {
                                at: FlowPos::new(p.id, offset),
                                name,
                                is_floating: false,
                                size: *size,
                            });
                        }
                        FlowRunKind::Shape(s) => {
                            if let ibis_doc_model::ShapeKind::Picture(pic) = &s.kind {
                                let name = if pic.name.is_empty() {
                                    crate::shell::i18n::format("_docx.picture-n", &[("n", &(list.len() + 1))])
                                } else {
                                    pic.name.clone()
                                };
                                let size = EmuSize::new(s.frame.width, s.frame.height);
                                list.push(PictureObjectInfo {
                                    at: FlowPos::new(p.id, offset),
                                    name,
                                    is_floating: s.floating.is_some(),
                                    size,
                                });
                            }
                        }
                        _ => {}
                    }
                    offset += r.len();
                }
            }
        }
        list
    }

    /// Select and jump caret to picture at `at`.
    pub(crate) fn select_picture_at(&mut self, at: FlowPos, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.anchor = Some(at);
            state.caret = Some(FlowPos::new(at.para, at.offset + 1));
        }
        cx.notify();
    }

    /// Open the Selection Pane (オブジェクトの選択と表示).
    pub(crate) fn open_selection_pane(&mut self, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if let Some(state) = self.doc_state_mut() {
            state.selection_pane = true;
            state.selection_reorder = Default::default();
            state.picture_edit = None;
            state.table_size = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.style_edit = None;
            state.link = None;
            state.find = None;
            state.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// All selectable drawing objects, including shapes/text boxes and inline pictures.
    pub(crate) fn document_selection_objects(&self) -> Vec<SelectionObjectInfo> {
        fn collect(blocks: &[ibis_doc_model::Block], objects: &mut Vec<SelectionObjectInfo>) {
            for block in blocks {
                match block {
                    ibis_doc_model::Block::Paragraph(p) => {
                        let mut offset = 0;
                        for run in &p.runs {
                            let data = match &run.kind {
                                FlowRunKind::Shape(shape) => {
                                    let (name, icon) = match &shape.kind {
                                        ibis_doc_model::ShapeKind::Picture(picture) => (picture.name.clone(), "image"),
                                        ibis_doc_model::ShapeKind::TextBox(body) => (body.text(), "text_fields"),
                                        ibis_doc_model::ShapeKind::Auto(auto) => (auto.text.as_ref().map(|t| t.text()).unwrap_or_default(), "category"),
                                        ibis_doc_model::ShapeKind::Chart(_) => (String::new(), "bar_chart"),
                                        _ => (String::new(), "category"),
                                    };
                                    Some((name, icon, shape.floating.clone()))
                                }
                                FlowRunKind::Picture { look, .. } | FlowRunKind::PictureControl { look, .. } => Some((look.name.clone(), "image", None)),
                                _ => None,
                            };
                            if let Some((name, icon, anchor)) = data {
                                let name = if name.trim().is_empty() {
                                    crate::shell::i18n::format("docx.selection-pane.object-number", &[("number", &(objects.len() + 1))])
                                } else { name.lines().next().unwrap_or_default().chars().take(64).collect() };
                                objects.push(SelectionObjectInfo { at: FlowPos::new(p.id, offset), name, icon, anchor });
                            }
                            offset += run.len();
                        }
                    }
                    ibis_doc_model::Block::Table(table) => for row in &table.rows { for cell in &row.cells { collect(&cell.blocks, objects); } },
                }
            }
        }
        let mut objects = Vec::new();
        if let Some(doc) = self.flow() { collect(&doc.body, &mut objects); }
        // First row represents the foremost floating object; inline items keep document order.
        objects.sort_by(|a, b| match (&a.anchor, &b.anchor) {
            (Some(a), Some(b)) => b.relative_height.cmp(&a.relative_height),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        objects
    }

    /// Commit all changed Z positions in one document transaction (one Undo).
    pub(crate) fn reorder_selection_objects(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let mut objects = self.document_selection_objects().into_iter().filter(|o| o.anchor.is_some()).collect::<Vec<_>>();
        if from == to || from >= objects.len() || to >= objects.len() { return; }
        let moved = objects.remove(from);
        objects.insert(to, moved);
        let count = objects.len();
        let commands = objects.into_iter().enumerate().filter_map(|(index, object)| {
            let mut anchor = object.anchor?;
            // relativeHeight is an ordering key; consecutive ranks give a strict order even if the source used ties.
            let rank = u32::try_from(count - index).unwrap_or(u32::MAX);
            if anchor.relative_height == rank { return None; }
            anchor.relative_height = rank;
            Some(FlowCommand::SetPictureFloating { at: object.at, anchor: Some(anchor) })
        }).collect::<Vec<_>>();
        self.run_flow(commands, cx);
    }

    /// Selection uses the app-wide table and its animated, Escape-cancellable drag behavior.
    pub(crate) fn selection_pane_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let state = self.doc_state()?;
        if !state.selection_pane { return None; }
        let reorder = state.selection_reorder.clone();
        let objects = self.document_selection_objects();
        let current_at = self.ordered_selection().map(|(start, _)| start);
        let mut body = v_flex().gap_2().w_full().min_w_0();
        if objects.is_empty() {
            body = body.child(div().text_sm().text_color(crate::shell::chrome::text_secondary()).child(crate::shell::i18n::text("docx.selection-pane.empty")));
        } else {
            let (floating, inline): (Vec<_>, Vec<_>) = objects.into_iter().partition(|o| o.anchor.is_some());
            let header = || div().h_full().flex().items_center().px_2().text_sm().child(crate::shell::i18n::text("docx.selection-pane.object-name")).into_any_element();
            let row = |index: usize, object: SelectionObjectInfo, draggable: bool| {
                let at = object.at;
                let selected = current_at == Some(at);
                let item = h_flex().id(("selection-item", index)).size_full().items_center().gap_2().px_2().cursor_pointer()
                    .bg(rgb(if selected { crate::shell::chrome::SELECTED_ROW_BG } else { 0xffffff }))
                    .hover(|s| s.bg(rgb(0xf1f3f4)))
                    .child(Icon::new(Material(object.icon)).size(px(16.)))
                    .child(div().flex_1().min_w_0().overflow_hidden().text_sm().child(object.name))
                    .when(draggable, |d| d.child(Icon::new(Material("menu")).size(px(18.))))
                    .on_click(cx.listener(move |e, _, _, cx| {
                        e.select_picture_at(at, cx);
                        if let Some(s) = e.doc_state_mut() { s.shape_selected = draggable; s.selected_shapes.clear(); }
                    }));
                (index, item.into_any_element(), selected)
            };
            if !floating.is_empty() {
                let rows = floating.into_iter().enumerate().map(|(i, o)| row(i, o, true)).collect();
                body = body.child(crate::shell::chrome::data_table::DataTable::new("docx-selection-floating", Some(header()), rows, cx)
                    .reorderable(reorder, |e, from, to, _, cx| e.reorder_selection_objects(from, to, cx)));
            }
            if !inline.is_empty() {
                let rows = inline.into_iter().enumerate().map(|(i, o)| row(i, o, false)).collect();
                body = body.child(div().text_sm().child(crate::shell::i18n::text("_docx.in-line-with-text")))
                    .child(crate::shell::chrome::data_table::DataTable::new("docx-selection-inline", Some(header()), rows, cx));
            }
        }
        Some(SidebarPanel { title: crate::shell::i18n::text("docx.selection-pane.title"), body: body.into_any_element() })
    }

}

#[derive(Clone, Debug)]
pub(crate) struct PictureObjectInfo {
    pub(crate) at: FlowPos,
    pub(crate) name: String,
    pub(crate) is_floating: bool,
    #[allow(dead_code)]
    pub(crate) size: EmuSize,
}


#[derive(Clone, Debug)]
pub(crate) struct SelectionObjectInfo {
    pub(crate) at: FlowPos,
    pub(crate) name: String,
    pub(crate) icon: &'static str,
    pub(crate) anchor: Option<ibis_doc_model::FloatingAnchor>,
}
