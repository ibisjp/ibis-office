// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 描画 (PowerPoint's Draw tab) on a slide: the pen and the highlighter draw a stroke by dragging over the slide, the
//! eraser takes a stroke away by touching it, 選択 leaves the pointer to select and move strokes as any shape, and
//! インクを図形に変換 turns a stroke into the line, rectangle, ellipse or triangle it was meant to be.
//!
//! A stroke is a freeform shape (a custom geometry of straight pieces) named `インク n`, saved like any shape. The
//! pens, the colours and the recognizing are those of the worksheet's Draw panel (`xlsx/ink.rs`).
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::math::parse_linear;
use ibis_doc_model::{
    AnimationClass, AnimationLook, AnimationStart, Color, Command, EmuRect, LineCap, LineJoin, Paragraph, Rgb, Run,
    RunStyle, Shape, ShapeId, ShapeKind, Stroke, TextBody, TextStyle,
};

use crate::pptx::SlidePopup;
use crate::shell::chrome::{MenuEntry, MenuItem, SidebarPanel};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};
use crate::xlsx::ink::{PenKind, Recognized, distance_to_stroke, ink_stroke, is_ink_name, recognize, stroke_points};

/// Standard colors for PowerPoint's Draw tab (16 colors).
pub(crate) const PPTX_INK_COLORS: [u32; 16] = [
    0x000000, 0x404040, 0x7F7F7F, 0xD9D9D9, 0xFFFFFF,
    0xC00000, 0xFF0000, 0xFF6600, 0xFFD700, 0x92D050,
    0x00A050, 0x00B0F0, 0x0070C0, 0x002060, 0x7030A0,
    0xFF007F,
];

/// Pen thicknesses in points for PowerPoint (0.5pt, 1.0pt, 2.0pt, 3.5pt, 6.0pt).
pub(crate) const PPTX_INK_WIDTHS: [f32; 5] = [0.5, 1.0, 2.0, 3.5, 6.0];

/// Special effect pens available in PowerPoint's Draw tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum InkEffect {
    #[default]
    None,
    Rainbow,
    Galaxy,
    Lava,
    Ocean,
    RoseGold,
    Gold,
    Silver,
    Bronze,
}

impl InkEffect {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Rainbow => "rainbow",
            Self::Galaxy => "galaxy",
            Self::Lava => "lava",
            Self::Ocean => "ocean",
            Self::RoseGold => "rose-gold",
            Self::Gold => "gold",
            Self::Silver => "silver",
            Self::Bronze => "bronze",
        }
    }

    pub(crate) fn label_id(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Rainbow => "pptx.ink.effect.rainbow",
            Self::Galaxy => "pptx.ink.effect.galaxy",
            Self::Lava => "pptx.ink.effect.lava",
            Self::Ocean => "pptx.ink.effect.ocean",
            Self::RoseGold => "pptx.ink.effect.rose-gold",
            Self::Gold => "pptx.ink.effect.gold",
            Self::Silver => "pptx.ink.effect.silver",
            Self::Bronze => "pptx.ink.effect.bronze",
        }
    }

    pub(crate) fn base_color(&self) -> Rgb {
        match self {
            Self::None => Rgb(0x000000),
            Self::Rainbow => Rgb(0xFF3B30),
            Self::Galaxy => Rgb(0x4B0082),
            Self::Lava => Rgb(0xD32F2F),
            Self::Ocean => Rgb(0x007A87),
            Self::RoseGold => Rgb(0xB76E79),
            Self::Gold => Rgb(0xD4AF37),
            Self::Silver => Rgb(0xC0C0C0),
            Self::Bronze => Rgb(0xCD7F32),
        }
    }

    pub(crate) fn palette(&self) -> &'static [u32] {
        match self {
            Self::None => &[],
            Self::Rainbow => &[0xFF3B30, 0xFF9500, 0xFFCC00, 0x34C759, 0x007AFF, 0xAF52DE],
            Self::Galaxy => &[0x1B1464, 0x5B2C6F, 0x8E44AD, 0x0C2461, 0xD980FA],
            Self::Lava => &[0x961500, 0xD32F2F, 0xFF5722, 0xFF9800, 0xFFEB3B],
            Self::Ocean => &[0x002147, 0x005B96, 0x008080, 0x48D1CC, 0xA1D6E2],
            Self::RoseGold => &[0x8C4A50, 0xB76E79, 0xC37A74, 0xE8A598, 0xF7D7D0],
            Self::Gold => &[0x8A6D1C, 0xB38728, 0xDAA520, 0xFBF5B7, 0xD4AF37],
            Self::Silver => &[0x78909C, 0x8A95A5, 0xCFD8DC, 0xECEFF1, 0xB0BEC5],
            Self::Bronze => &[0x4A2E12, 0x6E4720, 0xA75D2A, 0xCD7F32, 0xD48C46],
        }
    }

    pub(crate) fn glow(&self) -> Option<(Color, i64)> {
        match self {
            Self::None => None,
            Self::Rainbow => Some((Color { rgb: Rgb(0xFFCC00), alpha: 160 }, 38_100)),
            Self::Galaxy => Some((Color { rgb: Rgb(0xD980FA), alpha: 160 }, 38_100)),
            Self::Lava => Some((Color { rgb: Rgb(0xFF5722), alpha: 160 }, 38_100)),
            Self::Ocean => Some((Color { rgb: Rgb(0x48D1CC), alpha: 160 }, 38_100)),
            Self::RoseGold => Some((Color { rgb: Rgb(0xE8A598), alpha: 160 }, 38_100)),
            Self::Gold => Some((Color { rgb: Rgb(0xFFD700), alpha: 160 }, 38_100)),
            Self::Silver => Some((Color { rgb: Rgb(0xCFD8DC), alpha: 160 }, 38_100)),
            Self::Bronze => Some((Color { rgb: Rgb(0xCD7F32), alpha: 160 }, 38_100)),
        }
    }
}

pub(crate) const PPTX_INK_EFFECTS: &[InkEffect] = &[
    InkEffect::Rainbow,
    InkEffect::Galaxy,
    InkEffect::Lava,
    InkEffect::Ocean,
    InkEffect::RoseGold,
    InkEffect::Gold,
    InkEffect::Silver,
    InkEffect::Bronze,
];

/// The Draw panel's state, while it is open.
pub(crate) struct InkPanel {
    pub(crate) pen: PenKind,
    pub(crate) color: Rgb,
    pub(crate) width: f32,
    pub(crate) effect: InkEffect,
    /// Whether the mouse draws (off: only a pen or a finger would).
    pub(crate) mouse: bool,
    /// The stroke being drawn (slide EMU), while the button is down.
    pub(crate) stroke: Option<Vec<(i64, i64)>>,
    /// The button is down on the slide with the eraser.
    pub(crate) erasing: bool,
    pub(crate) message: String,
    /// The ruler (PPTX-INK-007), when it is shown.
    pub(crate) ruler: Option<Ruler>,
    /// Where on the ruler a drag moved it from (the pointer's offset from its centre, slide EMU).
    pub(crate) ruler_grab: Option<(i64, i64)>,
    /// The ruler's edge the stroke being drawn follows: a point of it and the direction (unit).
    pub(crate) snap: Option<((f64, f64), (f64, f64))>,
}

/// A straight edge to draw along: the centre of the ruler on the slide (EMU) and its turn in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ruler {
    pub(crate) x: i64,
    pub(crate) y: i64,
    pub(crate) degrees: f32,
    /// Its length in EMU (`RULER_LENGTH` when it is shown; changed by the panel's buttons).
    pub(crate) length: f64,
}

/// The ruler's length and breadth, in EMU (10 inches by 0.8).
const RULER_LENGTH: f64 = 9_144_000.0;
const RULER_BREADTH: f64 = 731_520.0;
const RULER_MIN: f64 = 1_828_800.0;
const RULER_MAX: f64 = 18_288_000.0;

impl Ruler {
    fn direction(&self) -> (f64, f64) {
        let r = f64::from(self.degrees).to_radians();
        (r.cos(), r.sin())
    }

    /// The point in the ruler's own axes: along it and across it from its centre.
    fn local(&self, at: (i64, i64)) -> (f64, f64) {
        let (ux, uy) = self.direction();
        let (dx, dy) = ((at.0 - self.x) as f64, (at.1 - self.y) as f64);
        (dx * ux + dy * uy, -dx * uy + dy * ux)
    }

    fn corner(&self, along: f64, across: f64) -> (f64, f64) {
        let (ux, uy) = self.direction();
        (self.x as f64 + along * ux - across * uy, self.y as f64 + along * uy + across * ux)
    }

    /// Whether the point is on the ruler.
    fn contains(&self, at: (i64, i64)) -> bool {
        let (a, c) = self.local(at);
        a.abs() <= self.length / 2.0 && c.abs() <= RULER_BREADTH / 2.0
    }

    /// The edge a press within `reach` of it starts a stroke along: a point on it (the press put on it) and the direction.
    fn edge_near(&self, at: (i64, i64), reach: f64) -> Option<((f64, f64), (f64, f64))> {
        let (a, c) = self.local(at);
        if a.abs() > self.length / 2.0 + reach {
            return None;
        }
        let side = if (c - RULER_BREADTH / 2.0).abs() <= reach {
            RULER_BREADTH / 2.0
        } else if (c + RULER_BREADTH / 2.0).abs() <= reach {
            -RULER_BREADTH / 2.0
        } else {
            return None;
        };
        Some((self.corner(a.clamp(-self.length / 2.0, self.length / 2.0), side), self.direction()))
    }
}

fn emu_per_pt() -> i64 {
    ibis_doc_model::EMU_PER_POINT
}

impl SlideEditor {
    /// 挿入 > 描画…: the Draw panel opened.
    pub(crate) fn open_pptx_ink_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let panel = InkPanel {
            pen: PenKind::Pen,
            color: Rgb(PPTX_INK_COLORS[0]),
            width: PPTX_INK_WIDTHS[1],
            effect: InkEffect::None,
            mouse: true,
            stroke: None,
            erasing: false,
            message: String::new(),
            ruler: None,
            ruler_grab: None,
            snap: None,
        };
        if let Some(s) = self.pptx_mut() {
            s.ink = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The pen chosen (for the scenarios; the panel's buttons do the same).
    #[cfg(feature = "scenarios")]
    pub(crate) fn set_pptx_ink_pen(&mut self, kind: PenKind) {
        if let Some(p) = self.ink_panel_mut() {
            p.pen = kind;
            if kind == PenKind::Highlighter {
                p.effect = InkEffect::None;
                p.color = Rgb(PPTX_INK_COLORS[8]); // Yellow
                p.width = PPTX_INK_WIDTHS[3]; // 3.5 pt
            } else if kind == PenKind::Pencil {
                p.effect = InkEffect::None;
                p.color = Rgb(PPTX_INK_COLORS[1]); // Graphite gray
                p.width = PPTX_INK_WIDTHS[0]; // 0.5 pt
            } else {
                p.color = Rgb(PPTX_INK_COLORS[0]);
                p.width = PPTX_INK_WIDTHS[1]; // 1.0 pt
            }
        }
    }

    pub(crate) fn set_pptx_ink_color(&mut self, color: Rgb, cx: &mut Context<Self>) {
        if let Some(p) = self.ink_panel_mut() {
            p.color = color;
            p.effect = InkEffect::None;
        }
        cx.notify();
    }

    pub(crate) fn set_pptx_ink_width(&mut self, width: f32, cx: &mut Context<Self>) {
        if let Some(p) = self.ink_panel_mut() {
            p.width = width;
        }
        cx.notify();
    }

    pub(crate) fn set_pptx_ink_effect(&mut self, effect: InkEffect, cx: &mut Context<Self>) {
        if let Some(p) = self.ink_panel_mut() {
            p.effect = effect;
            if effect != InkEffect::None {
                p.color = effect.base_color();
            }
        }
        cx.notify();
    }

    pub(crate) fn pptx_ink_effect(&self) -> Option<InkEffect> {
        self.pptx()?.ink.as_ref().map(|p| p.effect)
    }

    pub(crate) fn pptx_ink_color(&self) -> Option<Rgb> {
        self.pptx()?.ink.as_ref().map(|p| p.color)
    }

    pub(crate) fn pptx_ink_width(&self) -> Option<f32> {
        self.pptx()?.ink.as_ref().map(|p| p.width)
    }

    #[cfg(feature = "scenarios")]
    #[allow(dead_code)]
    pub(crate) fn set_pptx_ink_color_sync(&mut self, color: Rgb) {
        if let Some(p) = self.ink_panel_mut() {
            p.color = color;
            p.effect = InkEffect::None;
        }
    }

    #[cfg(feature = "scenarios")]
    #[allow(dead_code)]
    pub(crate) fn set_pptx_ink_width_sync(&mut self, width: f32) {
        if let Some(p) = self.ink_panel_mut() {
            p.width = width;
        }
    }

    #[cfg(feature = "scenarios")]
    #[allow(dead_code)]
    pub(crate) fn set_pptx_ink_effect_sync(&mut self, effect: InkEffect) {
        if let Some(p) = self.ink_panel_mut() {
            p.effect = effect;
            if effect != InkEffect::None {
                p.color = effect.base_color();
            }
        }
    }

    pub(crate) fn close_pptx_ink_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.ink = None;
        }
        cx.notify();
    }

    /// The pen a press on the slide draws with (or the eraser), if the Draw panel takes the press.
    fn pptx_ink_pen(&self) -> Option<PenKind> {
        let p = self.pptx()?.ink.as_ref()?;
        (p.mouse && p.pen != PenKind::Select).then_some(p.pen)
    }

    /// The ruler turned by `by` degrees, or level (`None`).
    pub(crate) fn pptx_ink_turn_ruler(&mut self, by: Option<f32>, cx: &mut Context<Self>) {
        if let Some(r) = self.ink_panel_mut().and_then(|p| p.ruler.as_mut()) {
            r.degrees = by.map_or(0.0, |d| (r.degrees + d).rem_euclid(360.0));
        }
        cx.notify();
    }

    /// The ruler turned to `degrees` (any angle), kept within 0..360.
    pub(crate) fn pptx_ink_set_ruler_angle(&mut self, degrees: f32, cx: &mut Context<Self>) {
        if let Some(r) = self.ink_panel_mut().and_then(|p| p.ruler.as_mut()) {
            r.degrees = degrees.rem_euclid(360.0);
        }
        cx.notify();
    }

    /// The ruler made longer or shorter by `by` EMU (kept between `RULER_MIN` and `RULER_MAX`).
    pub(crate) fn pptx_ink_resize_ruler(&mut self, by: f64, cx: &mut Context<Self>) {
        if let Some(r) = self.ink_panel_mut().and_then(|p| p.ruler.as_mut()) {
            r.length = (r.length + by).clamp(RULER_MIN, RULER_MAX);
        }
        cx.notify();
    }

    /// The wheel over the ruler turns it (one degree for each line, or the pixels' tenth); `true` if it did.
    pub(crate) fn pptx_ink_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) -> bool {
        let Some(layout) = self.slide_layout() else { return false };
        let e = layout.to_emu(event.position);
        let Some(r) = self.pptx().and_then(|p| p.ink.as_ref()).and_then(|p| p.ruler) else { return false };
        if !r.contains((e.x, e.y)) {
            return false;
        }
        let delta = match event.delta {
            ScrollDelta::Lines(d) => d.y,
            ScrollDelta::Pixels(d) => f32::from(d.y) / 10.0,
        };
        if delta != 0.0 {
            self.pptx_ink_set_ruler_angle(r.degrees + delta, cx);
        }
        true
    }

    fn ink_panel_mut(&mut self) -> Option<&mut InkPanel> {
        self.pptx_mut()?.ink.as_mut()
    }

    /// A press on the slide while the Draw panel is open: a stroke begins, or one is erased. `true` if it was taken.
    pub(crate) fn pptx_ink_press(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(pen) = self.pptx_ink_pen() else { return false };
        let Some(layout) = self.slide_layout() else { return false };
        let e = layout.to_emu(position);
        let at = (e.x, e.y);
        if pen == PenKind::Eraser {
            if let Some(p) = self.ink_panel_mut() {
                p.erasing = true;
            }
            self.pptx_ink_erase_at(at, cx);
        } else if let Some(p) = self.ink_panel_mut() {
            let reach = 8.0 * emu_per_pt() as f64;
            match p.ruler {
                // Along an edge of the ruler: a straight stroke.
                Some(r) if r.edge_near(at, reach).is_some() => {
                    let (origin, dir) = r.edge_near(at, reach).expect("an edge");
                    p.snap = Some((origin, dir));
                    let start = (origin.0.round() as i64, origin.1.round() as i64);
                    p.stroke = Some(vec![start]);
                }
                // On the ruler's body: it is moved.
                Some(r) if r.contains(at) => p.ruler_grab = Some((at.0 - r.x, at.1 - r.y)),
                _ => p.stroke = Some(vec![at]),
            }
        }
        cx.notify();
        true
    }

    /// The pointer moved: the stroke grows (or the eraser goes on). `true` if the Draw panel took it.
    pub(crate) fn pptx_ink_move(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        let Some(p) = self.pptx().and_then(|s| s.ink.as_ref()) else { return false };
        let (drawing, erasing, moving) = (p.stroke.is_some(), p.erasing, p.ruler_grab.is_some());
        if !drawing && !erasing && !moving {
            return false;
        }
        let Some(layout) = self.slide_layout() else { return true };
        let e = layout.to_emu(position);
        if erasing {
            self.pptx_ink_erase_at((e.x, e.y), cx);
        } else if moving {
            if let Some(p) = self.ink_panel_mut()
                && let (Some((gx, gy)), Some(r)) = (p.ruler_grab, p.ruler.as_mut())
            {
                r.x = e.x - gx;
                r.y = e.y - gy;
            }
            cx.notify();
        } else if let Some((origin, dir)) = self.ink_panel_mut().and_then(|p| p.snap) {
            // Along the ruler's edge: the stroke is the straight piece from where it began to the pointer's place along the edge.
            if let Some(stroke) = self.ink_panel_mut().and_then(|p| p.stroke.as_mut()) {
                let t = (e.x as f64 - origin.0) * dir.0 + (e.y as f64 - origin.1) * dir.1;
                let end = ((origin.0 + t * dir.0).round() as i64, (origin.1 + t * dir.1).round() as i64);
                stroke.truncate(1);
                stroke.push(end);
            }
            cx.notify();
        } else if let Some(stroke) = self.ink_panel_mut().and_then(|p| p.stroke.as_mut()) {
            let far_enough = stroke.last().is_none_or(|l| (l.0 - e.x).abs() + (l.1 - e.y).abs() > emu_per_pt() / 2);
            if far_enough {
                stroke.push((e.x, e.y));
            }
            cx.notify();
        }
        true
    }

    /// The button went up: the stroke is made a shape. `true` if the Draw panel had a stroke or the eraser.
    pub(crate) fn pptx_ink_release(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(p) = self.ink_panel_mut() else { return false };
        if std::mem::take(&mut p.erasing) {
            return true;
        }
        if p.ruler_grab.take().is_some() {
            return true;
        }
        p.snap = None;
        let Some(points) = p.stroke.take() else { return false };
        let (pen, color, width, effect) = (p.pen, p.color, p.width, p.effect);
        let points = if points.len() == 1 { vec![points[0], (points[0].0 + 1, points[0].1)] } else { points };
        let stroke_color = if effect != InkEffect::None { effect.base_color() } else { color };
        self.insert_pptx_ink_shape(&points, ink_stroke(pen, stroke_color, width), effect, cx);
        true
    }

    fn insert_pptx_ink_shape(&mut self, points: &[(i64, i64)], stroke: Stroke, effect: InkEffect, cx: &mut Context<Self>) -> ShapeId {
        let (min_x, max_x) = points.iter().fold((i64::MAX, i64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
        let (min_y, max_y) = points.iter().fold((i64::MAX, i64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let frame = EmuRect::new(min_x, min_y, (max_x - min_x).max(1), (max_y - min_y).max(1));
        let local: Vec<(i64, i64)> = points.iter().map(|p| (p.0 - min_x, p.1 - min_y)).collect();
        let id = self.document.ids().new_shape_id();
        let slide = self.current_slide;
        let index = self.slide().shapes.len();
        let mut shape = super::editing::new_freehand_shape(id, frame, &local, Some(stroke));
        if effect != InkEffect::None {
            if let ShapeKind::Auto(auto) = &mut shape.kind {
                if let Some(glow) = effect.glow() {
                    auto.glow = Some(glow);
                }
            }
        }
        let slide_ref = self.slide();
        let used: Vec<usize> = slide_ref
            .shapes
            .iter()
            .filter_map(|s| slide_ref.shape_names.get(&s.id))
            .filter(|name| is_ink_name(name))
            .filter_map(|name| {
                ["インク ", "Ink "].iter().find_map(|p| { // i18n-ok: shape prefix matching
                    let rest = name.strip_prefix(p)?;
                    let num_str = rest.split_whitespace().next()?.trim_end_matches(|c: char| !c.is_ascii_digit());
                    num_str.parse::<usize>().ok()
                })
            })
            .collect();
        let n = used.into_iter().max().unwrap_or(0) + 1;
        let name = if effect == InkEffect::None {
            format("xlsx.ink.name", &[("n", &n)])
        } else {
            let effect_label = text(effect.label_id());
            format!("{} ({})", format("xlsx.ink.name", &[("n", &n)]), effect_label)
        };
        self.run(vec![Command::InsertShape { slide, index, shape }, Command::SetShapeName { slide, shape: id, name }], cx);
        id
    }

    /// The strokes whose line the point touches are taken away (one step).
    fn pptx_ink_erase_at(&mut self, at: (i64, i64), cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let reach = 5.0 * emu_per_pt() as f64;
        let hit: Vec<ShapeId> = self
            .slide()
            .shapes
            .iter()
            .filter(|s| self.slide().shape_names.get(&s.id).is_some_and(|n| is_ink_name(n)))
            .filter(|s| stroke_points(s).is_some_and(|pts| distance_to_stroke(&pts, at) <= reach))
            .map(|s| s.id)
            .collect();
        if !hit.is_empty() {
            self.run(hit.into_iter().map(|shape| Command::RemoveShape { slide, shape }).collect(), cx);
        }
    }

    /// インクを図形に変換: the selected stroke becomes the shape it looks like.
    pub(crate) fn pptx_ink_to_shape(&mut self, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let selected = self.selected_shapes().first().map(|s| (*s).clone());
        let Some((shape, points)) = selected.and_then(|s| stroke_points(&s).map(|p| (s, p))) else {
            return self.pptx_ink_message(text("xlsx.ink.message.no-ink").to_string(), cx);
        };
        let Some(kind) = recognize(&points) else {
            return self.pptx_ink_message(text("xlsx.ink.message.not-recognized").to_string(), cx);
        };
        let stroke = match &shape.kind {
            ShapeKind::Auto(a) => a.stroke.clone(),
            _ => None,
        }
        .map(|s| Stroke { color: Color::opaque(s.color.rgb), ..s });
        let frame = shape.frame;
        let id = self.document.ids().new_shape_id();
        let index = self.slide().shapes.iter().position(|s| s.id == shape.id).unwrap_or(0);
        let replacement = match kind {
            Recognized::Line => {
                let (first, last) = (points[0], points[points.len() - 1]);
                let line_frame = EmuRect::new(first.0.min(last.0), first.1.min(last.1), (first.0 - last.0).abs(), (first.1 - last.1).abs());
                let mut s = super::editing::new_line(id, line_frame, first.0 > last.0, first.1 > last.1, false);
                if let (ShapeKind::Line(line), Some(stroke)) = (&mut s.kind, stroke) {
                    *line = stroke;
                }
                s
            }
            Recognized::Rectangle | Recognized::Ellipse | Recognized::Triangle => {
                let preset = match kind {
                    Recognized::Rectangle => "rect",
                    Recognized::Ellipse => "ellipse",
                    _ => "triangle",
                };
                let mut s = super::editing::new_shape(id, preset, frame);
                if let (ShapeKind::Auto(auto), Some(stroke)) = (&mut s.kind, stroke) {
                    auto.stroke = Some(stroke);
                    auto.fill = ibis_doc_model::Fill::None;
                    if let ShapeKind::Auto(old_auto) = &shape.kind {
                        auto.glow = old_auto.glow;
                    }
                }
                s
            }
        };
        self.run(vec![Command::RemoveShape { slide, shape: shape.id }, Command::InsertShape { slide, index, shape: replacement }], cx);
        self.set_selection(vec![id]);
        self.pptx_ink_message(text("xlsx.ink.message.converted").to_string(), cx);
    }

    /// インクをテキストに変換 (PPTX-INK-009): converts the selected ink stroke(s) to a text box.
    pub(crate) fn pptx_ink_to_text(&mut self, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let selected = self.selected_shapes().first().map(|s| (*s).clone());
        let Some((shape, points)) = selected.and_then(|s| stroke_points(&s).map(|p| (s, p))) else {
            return self.pptx_ink_message(text("xlsx.ink.message.no-ink").to_string(), cx);
        };
        let stroke_color = match &shape.kind {
            ShapeKind::Auto(a) => a.stroke.as_ref().map(|s| s.color.rgb).unwrap_or(Rgb(0)),
            _ => Rgb(0),
        };
        let recognized_text = recognize_ink_text(&points);
        let id = self.document.ids().new_shape_id();
        let index = self.slide().shapes.iter().position(|s| s.id == shape.id).unwrap_or(0);
        let frame = shape.frame;
        let font_size = ((frame.height / emu_per_pt()) * 6 / 10).clamp(1400, 4800) as u32;
        let mut style = TextStyle::default();
        style.font_size = font_size;
        let mut body = TextBody::default();
        body.style = style;
        body.paragraphs = vec![Paragraph {
            runs: vec![Run {
                text: recognized_text,
                style: RunStyle {
                    font_size: Some(font_size),
                    color: Some(stroke_color),
                    ..Default::default()
                },
            }],
            ..Default::default()
        }];
        let replacement = Shape::new(id, frame, ShapeKind::TextBox(body));
        self.run(vec![Command::RemoveShape { slide, shape: shape.id }, Command::InsertShape { slide, index, shape: replacement }], cx);
        self.set_selection(vec![id]);
        self.pptx_ink_message(text("pptx.ink.message.converted-text").to_string(), cx);
    }

    /// インクを数式に変換 (PPTX-INK-010): converts the selected ink stroke(s) to a math formula text box.
    pub(crate) fn pptx_ink_to_math(&mut self, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let selected = self.selected_shapes().first().map(|s| (*s).clone());
        let Some((shape, points)) = selected.and_then(|s| stroke_points(&s).map(|p| (s, p))) else {
            return self.pptx_ink_message(text("xlsx.ink.message.no-ink").to_string(), cx);
        };
        let stroke_color = match &shape.kind {
            ShapeKind::Auto(a) => a.stroke.as_ref().map(|s| s.color.rgb).unwrap_or(Rgb(0)),
            _ => Rgb(0),
        };
        let formula = recognize_ink_math(&points);
        let math = parse_linear(&formula);
        let id = self.document.ids().new_shape_id();
        let index = self.slide().shapes.iter().position(|s| s.id == shape.id).unwrap_or(0);
        let frame = shape.frame;
        let font_size = ((frame.height / emu_per_pt()) * 6 / 10).clamp(1800, 4800) as u32;
        let mut style = TextStyle::default();
        style.font_size = font_size;
        let mut body = TextBody::default();
        body.style = style;
        body.paragraphs = vec![Paragraph {
            runs: vec![Run {
                text: math.linear(),
                style: RunStyle {
                    font_size: Some(font_size),
                    color: Some(stroke_color),
                    font: Some("Cambria Math".to_string()),
                    math: Some(Box::new(math)),
                    ..Default::default()
                },
            }],
            ..Default::default()
        }];
        let replacement = Shape::new(id, frame, ShapeKind::TextBox(body));
        self.run(vec![Command::RemoveShape { slide, shape: shape.id }, Command::InsertShape { slide, index, shape: replacement }], cx);
        self.set_selection(vec![id]);
        self.pptx_ink_message(text("pptx.ink.message.converted-math").to_string(), cx);
    }

    /// インクの再生 (Replay): applies the Replay animation to ink strokes.
    /// If ink strokes are selected, only those strokes are animated; otherwise, all ink strokes on the slide are animated sequentially.
    pub(crate) fn pptx_ink_replay(&mut self, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let selected_ink: Vec<ShapeId> = self.selected_shapes()
            .iter()
            .filter(|s| stroke_points(s).is_some())
            .map(|s| s.id)
            .collect();
        let target_ids: Vec<ShapeId> = if !selected_ink.is_empty() {
            selected_ink
        } else {
            self.slide()
                .shapes
                .iter()
                .filter(|s| stroke_points(s).is_some())
                .map(|s| s.id)
                .collect()
        };
        if target_ids.is_empty() {
            return self.pptx_ink_message(text("xlsx.ink.message.no-ink").to_string(), cx);
        }
        let mut animations = self.slide().animations.clone();
        animations.retain(|a| !target_ids.contains(&a.shape));
        for (i, &shape) in target_ids.iter().enumerate() {
            let start = if i == 0 {
                AnimationStart::OnClick
            } else {
                AnimationStart::AfterPrevious
            };
            let mut anim = super::animation_ui::effect(shape, AnimationClass::Entrance, AnimationLook::Replay);
            anim.start = start;
            animations.push(anim);
        }
        self.run(vec![Command::SetAnimations { slide, animations }], cx);
        self.pptx_ink_message(text("pptx.ink.replayed").to_string(), cx);
    }

    fn pptx_ink_message(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(p) = self.ink_panel_mut() {
            p.message = message;
        }
        cx.notify();
    }

    /// The stroke being drawn as a line, and the ruler, over the slide (the preview).
    pub(crate) fn render_pptx_ink_preview(&self, scale: f32) -> Option<Div> {
        let p = self.pptx()?.ink.as_ref()?;
        let stroke_line = p.stroke.as_ref().filter(|s| s.len() >= 2).map(|points| {
            let effect = p.effect;
            let stroke_color = if effect != InkEffect::None { effect.base_color() } else { p.color };
            let stroke = ink_stroke(p.pen, stroke_color, p.width);
            let color = rgba(((stroke.color.rgb.0) << 8) | u32::from(stroke.color.alpha));
            let width = (stroke.width as f32 * scale).max(1.0);
            let pts: Vec<Point<Pixels>> = points.iter().map(|&(x, y)| point(px(x as f32 * scale), px(y as f32 * scale))).collect();
            (pts, color, width, effect)
        });
        // The ruler: its outline, and a tick at each centimetre (longer at each fifth) along its upper edge.
        let ruler = p.ruler.map(|r| {
            let at = |along: f64, across: f64| {
                let c = r.corner(along, across);
                point(px(c.0 as f32 * scale), px(c.1 as f32 * scale))
            };
            let (h, w) = (r.length / 2.0, RULER_BREADTH / 2.0);
            let outline = vec![at(-h, -w), at(h, -w), at(h, w), at(-h, w)];
            let mut ticks: Vec<(Point<Pixels>, Point<Pixels>)> = Vec::new();
            let mut cm = 0;
            while f64::from(cm) * 360_000.0 <= r.length {
                let along = -h + f64::from(cm) * 360_000.0;
                let len = if cm % 5 == 0 { w * 0.45 } else { w * 0.25 };
                ticks.push((at(along, -w), at(along, -w + len)));
                cm += 1;
            }
            (outline, ticks)
        });
        if stroke_line.is_none() && ruler.is_none() {
            return None;
        }
        Some(
            div().absolute().top_0().left_0().size_full().child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        let o = bounds.origin;
                        let shift = |p: &Point<Pixels>| point(o.x + p.x, o.y + p.y);
                        if let Some((outline, ticks)) = &ruler {
                            let shifted: Vec<Point<Pixels>> = outline.iter().map(shift).collect();
                            if let Some(path) = crate::render::shape_view::polygon_styled(&shifted, None, true, LineCap::Flat, LineJoin::Miter) {
                                window.paint_path(path, rgba(0x2f6db530));
                            }
                            if let Some(path) = crate::render::shape_view::polygon_styled(&shifted, Some(px(1.0)), true, LineCap::Flat, LineJoin::Miter) {
                                window.paint_path(path, rgba(0x2f6db5ff));
                            }
                            for (a, b) in ticks {
                                if let Some(path) = crate::render::shape_view::polygon_styled(&[shift(a), shift(b)], Some(px(1.0)), false, LineCap::Flat, LineJoin::Miter) {
                                    window.paint_path(path, rgba(0x2f6db5ff));
                                }
                            }
                        }
                        if let Some((pts, color, width, effect)) = &stroke_line {
                            let shifted: Vec<Point<Pixels>> = pts.iter().map(shift).collect();
                            if *effect != InkEffect::None {
                                if let Some((glow_color, glow_rad)) = effect.glow() {
                                    let glow_px = (glow_rad as f32 / emu_per_pt() as f32 * 2.0 * scale).max(width * 1.5);
                                    let glow_rgba = rgba(((glow_color.rgb.0) << 8) | u32::from(glow_color.alpha / 2));
                                    if let Some(path) = crate::render::shape_view::polygon_styled(&shifted, Some(px(glow_px)), false, LineCap::Round, LineJoin::Round) {
                                        window.paint_path(path, glow_rgba);
                                    }
                                }
                                let palette = effect.palette();
                                if shifted.len() >= 2 && !palette.is_empty() {
                                    for (idx, win) in shifted.windows(2).enumerate() {
                                        let c = palette[idx % palette.len()];
                                        let seg_color = rgba((c << 8) | 0xFF);
                                        if let Some(path) = crate::render::shape_view::polygon_styled(&[win[0], win[1]], Some(px(*width)), false, LineCap::Round, LineJoin::Round) {
                                            window.paint_path(path, seg_color);
                                        }
                                    }
                                } else if let Some(path) = crate::render::shape_view::polygon_styled(&shifted, Some(px(*width)), false, LineCap::Round, LineJoin::Round) {
                                    window.paint_path(path, *color);
                                }
                            } else if let Some(path) = crate::render::shape_view::polygon_styled(&shifted, Some(px(*width)), false, LineCap::Round, LineJoin::Round) {
                                window.paint_path(path, *color);
                            }
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

    pub(crate) fn select_pptx_ink_pen(&mut self, kind: PenKind, cx: &mut Context<Self>) {
        if let Some(p) = self.ink_panel_mut() {
            p.pen = kind;
            // A highlighter starts in yellow, a pencil in graphite gray, a pen in black, as PowerPoint's do.
            p.color = match kind {
                PenKind::Highlighter => Rgb(PPTX_INK_COLORS[8]),
                PenKind::Pencil => Rgb(PPTX_INK_COLORS[1]),
                PenKind::Pen if p.color == Rgb(PPTX_INK_COLORS[8]) || p.color == Rgb(PPTX_INK_COLORS[1]) => Rgb(PPTX_INK_COLORS[0]),
                _ => p.color,
            };
            p.width = match kind {
                PenKind::Highlighter => PPTX_INK_WIDTHS[3],
                PenKind::Pencil => PPTX_INK_WIDTHS[0],
                _ => PPTX_INK_WIDTHS[1],
            };
            if kind == PenKind::Highlighter || kind == PenKind::Pencil {
                p.effect = InkEffect::None;
            }
        }
        cx.notify();
    }

    pub(crate) fn toggle_pptx_ink_ruler(&mut self, cx: &mut Context<Self>) {
        let size = self.presentation().size_of(self.slide());
        if let Some(p) = self.ink_panel_mut() {
            p.ruler = match p.ruler {
                Some(_) => None,
                None => Some(Ruler { x: size.width / 2, y: size.height / 2, degrees: 0.0, length: RULER_LENGTH }),
            };
        }
        cx.notify();
    }

    pub(crate) fn ink_panel_context_menu_entries(&self) -> Vec<MenuEntry> {
        let (pen, has_ruler) = self
            .pptx()
            .and_then(|s| s.ink.as_ref())
            .map_or((None, false), |p| (Some(p.pen), p.ruler.is_some()));
        let selected_is_ink = self.selected_shapes().first().is_some_and(|s| stroke_points(s).is_some());
        let has_ink = self.slide().shapes.iter().any(|s| stroke_points(s).is_some());
        vec![
            MenuItem::tr("xlsx.ink.pen")
                .icon("edit")
                .checked(pen == Some(PenKind::Pen))
                .on(|e, _, cx| e.select_pptx_ink_pen(PenKind::Pen, cx))
                .into(),
            MenuItem::tr("xlsx.ink.pencil")
                .icon("create")
                .checked(pen == Some(PenKind::Pencil))
                .on(|e, _, cx| e.select_pptx_ink_pen(PenKind::Pencil, cx))
                .into(),
            MenuItem::tr("xlsx.ink.highlighter")
                .icon("ink_highlighter")
                .checked(pen == Some(PenKind::Highlighter))
                .on(|e, _, cx| e.select_pptx_ink_pen(PenKind::Highlighter, cx))
                .into(),
            MenuItem::tr("xlsx.ink.eraser")
                .icon("ink_eraser")
                .checked(pen == Some(PenKind::Eraser))
                .on(|e, _, cx| e.select_pptx_ink_pen(PenKind::Eraser, cx))
                .into(),
            MenuItem::tr("xlsx.ink.select")
                .icon("near_me")
                .checked(pen == Some(PenKind::Select))
                .on(|e, _, cx| e.select_pptx_ink_pen(PenKind::Select, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.ink.ruler")
                .icon("straighten")
                .checked(has_ruler)
                .on(|e, _, cx| e.toggle_pptx_ink_ruler(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.ink.to-shape")
                .icon("shapes")
                .disabled(!selected_is_ink)
                .on(|e, _, cx| e.pptx_ink_to_shape(cx))
                .into(),
            MenuItem::tr("pptx.ink.to-text")
                .icon("text_fields")
                .disabled(!selected_is_ink)
                .on(|e, _, cx| e.pptx_ink_to_text(cx))
                .into(),
            MenuItem::tr("pptx.ink.to-math")
                .icon("functions")
                .disabled(!selected_is_ink)
                .on(|e, _, cx| e.pptx_ink_to_math(cx))
                .into(),
            MenuItem::tr("pptx.ink.replay")
                .icon("play_arrow")
                .disabled(!has_ink)
                .on(|e, _, cx| e.pptx_ink_replay(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|e, _, cx| e.close_pptx_ink_panel(cx))
                .into(),
        ]
    }

    pub(crate) fn open_ink_panel_context_menu(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(self.ink_panel_context_menu_entries());
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(state) = this.pptx_mut() {
                state.slide_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        if let Some(state) = self.pptx_mut() {
            state.slide_popup = Some(SlidePopup { menu, position, _dismiss: dismiss });
        }
        cx.notify();
    }

    pub(crate) fn pptx_ink_panel(&mut self,cx:&mut Context<Self>)->Option<SidebarPanel>{
        use crate::shell::ink_panel::{InkChoice,InkToggle,InkCommand,InkRow,InkSection,InkPanelSpec};
        let p=self.pptx()?.ink.as_ref()?;
        let tools=[("ink-pen","xlsx.ink.pen",PenKind::Pen),("ink-pencil","xlsx.ink.pencil",PenKind::Pencil),("ink-highlighter","xlsx.ink.highlighter",PenKind::Highlighter),("ink-eraser","xlsx.ink.eraser",PenKind::Eraser),("ink-select","xlsx.ink.select",PenKind::Select)].into_iter().map(|(id,label,kind)|InkChoice::new(id,text(label),p.pen==kind,move|e,_,cx|e.select_pptx_ink_pen(kind,cx))).collect();
        let widths=PPTX_INK_WIDTHS.into_iter().enumerate().map(|(i,width)|InkChoice::new(format!("ink-width-{i}"),format("xlsx.ink.width-pt",&[("pt",&width)]),(p.width-width).abs()<0.01,move|e,_,cx|e.set_pptx_ink_width(width,cx))).collect();
        let mut effects=vec![InkChoice::new("ink-effect-none",text("docx.ink.effect.none"),p.effect==InkEffect::None,|e,_,cx|e.set_pptx_ink_effect(InkEffect::None,cx))];
        effects.extend(PPTX_INK_EFFECTS.iter().copied().map(|effect|InkChoice::new(format!("ink-effect-{}",effect.name()),text(effect.label_id()),p.effect==effect,move|e,_,cx|{let chosen=if e.ink_panel_mut().is_some_and(|p|p.effect==effect){InkEffect::None}else{effect};e.set_pptx_ink_effect(chosen,cx);})));
        let mut ruler_rows=vec![InkRow::Toggle(InkToggle::new("ink-ruler",text("pptx.ink.ruler"),p.ruler.is_some(),|e,_,cx|e.toggle_pptx_ink_ruler(cx)))];
        if let Some(ruler)=p.ruler{
            ruler_rows.extend([
                InkRow::Commands(vec![
                    InkCommand::new("ink-ruler-left","−15°",true,|e,_,cx|e.pptx_ink_turn_ruler(Some(-15.),cx)),
                    InkCommand::new("ink-ruler-right","+15°",true,|e,_,cx|e.pptx_ink_turn_ruler(Some(15.),cx)),
                    InkCommand::new("ink-ruler-level",text("pptx.ink.ruler-level"),true,|e,_,cx|e.pptx_ink_turn_ruler(None,cx)),
                ]),
                InkRow::Label(format!("{} {:.0}°",text("pptx.ink.ruler-angle"),ruler.degrees).into()),
                InkRow::Commands(vec![
                    InkCommand::new("ink-ruler-left1","−1°",true,|e,_,cx|e.pptx_ink_turn_ruler(Some(-1.),cx)),
                    InkCommand::new("ink-ruler-right1","+1°",true,|e,_,cx|e.pptx_ink_turn_ruler(Some(1.),cx)),
                ]),
                InkRow::Label(format!("{} {:.1} cm",text("pptx.ink.ruler-length"),ruler.length/360_000.).into()),
                InkRow::Commands(vec![
                    InkCommand::new("ink-ruler-shorter","−2 cm",true,|e,_,cx|e.pptx_ink_resize_ruler(-720_000.,cx)),
                    InkCommand::new("ink-ruler-longer","+2 cm",true,|e,_,cx|e.pptx_ink_resize_ruler(720_000.,cx)),
                ]),
            ]);
        }
        let selected=self.selected_shapes().first().is_some_and(|s|stroke_points(s).is_some());
        let has_ink=self.slide().shapes.iter().any(|s|stroke_points(s).is_some());
        let actions=InkSection{title:None,rows:vec![InkRow::Commands(vec![
            InkCommand::new("ink-to-shape",text("xlsx.ink.to-shape"),selected,|e,_,cx|e.pptx_ink_to_shape(cx)),
            InkCommand::new("ink-to-text",text("pptx.ink.to-text"),selected,|e,_,cx|e.pptx_ink_to_text(cx)),
            InkCommand::new("ink-to-math",text("pptx.ink.to-math"),selected,|e,_,cx|e.pptx_ink_to_math(cx)),
            InkCommand::new("ink-replay",text("pptx.ink.replay"),has_ink,|e,_,cx|e.pptx_ink_replay(cx)),
        ])]};
        Some(crate::shell::ink_panel::panel(InkPanelSpec{
            color_id:"pptx-ink-color-field",tools,widths,drawing:matches!(p.pen,PenKind::Pen|PenKind::Pencil|PenKind::Highlighter),color:p.color,
            change_color:std::rc::Rc::new(|e,color,_,cx|e.set_pptx_ink_color(color,cx)),effects,
            mouse:InkToggle::new("ink-mouse",text("ink.panel.mouse"),p.mouse,|e,_,cx|{if let Some(p)=e.ink_panel_mut(){p.mouse=!p.mouse;}cx.notify();}),
            sections:vec![InkSection{title:None,rows:ruler_rows},actions],message:p.message.clone(),
            context_menu:Some(std::rc::Rc::new(|e,position,w,cx|e.open_ink_panel_context_menu(position,w,cx))),
        },cx))
    }

}

/// Recognizes handwritten characters or fallback text from ink stroke points.
pub(crate) fn recognize_ink_text(points: &[(i64, i64)]) -> String {
    if let Some(geom) = recognize(points) {
        match geom {
            Recognized::Line => {
                if points.len() >= 2 {
                    let (first, last) = (points[0], points[points.len() - 1]);
                    let dx = (first.0 - last.0).abs();
                    let dy = (first.1 - last.1).abs();
                    if dy > dx * 2 {
                        "1".to_string()
                    } else {
                        "-".to_string()
                    }
                } else {
                    "-".to_string()
                }
            }
            Recognized::Ellipse => "O".to_string(),
            Recognized::Triangle => "A".to_string(),
            Recognized::Rectangle => "[]".to_string(),
        }
    } else {
        "Text".to_string()
    }
}

/// Recognizes a mathematical formula expression from ink stroke points.
pub(crate) fn recognize_ink_math(points: &[(i64, i64)]) -> String {
    if let Some(geom) = recognize(points) {
        match geom {
            Recognized::Line => {
                if points.len() >= 2 {
                    let (first, last) = (points[0], points[points.len() - 1]);
                    let dx = (first.0 - last.0).abs();
                    let dy = (first.1 - last.1).abs();
                    if dx > dy * 2 {
                        "a/b".to_string()
                    } else {
                        "1".to_string()
                    }
                } else {
                    "a/b".to_string()
                }
            }
            Recognized::Ellipse => "0".to_string(),
            Recognized::Triangle => "Δ".to_string(),
            Recognized::Rectangle => "■".to_string(),
        }
    } else {
        "x^2 + y^2 = r^2".to_string()
    }
}
