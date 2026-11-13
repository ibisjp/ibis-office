// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! セルの書式設定 (⌘1), in the sidebar: 表示形式・配置・フォント・罫線・塗りつぶし・保護 as sections
//! (a row with a triangle: pressing it opens or closes the section; which are open is kept while the
//! window lives). The choices are drop-downs (the colours open the palette of the toolbar, with a
//! rounded square of the colour now in use), the font size has − and +, bold/italic/underline/strike
//! are the toolbar's buttons, and the border lines are drawn on a sample (as Excel's 罫線 tab): press
//! a line of it to draw or take it off.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Disableable as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Border, BorderLine, CellAlign, CellVerticalAlign, StyleChange};

use crate::shell::chrome::gradient_slider::{GradientSliderState, gradient_slider};
use crate::shell::chrome::preview_dropdown::{PreviewChoice, preview_dropdown};
use crate::shell::chrome::synced_field::{Commit, FieldSpec, SyncedFields};
use crate::shell::chrome::{ActionButtonKind, ActionDefinition, ButtonGroupItem, MenuEntry, MenuItem, SidebarPanel, ToolbarItem, button_group, color_row, control_row, dropdown_row, font_dropdown_row, text_field_row, toolbar_row};
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;
use crate::ext::*;

/// The sections of セルの書式設定 (Excel's tabs of the dialog).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FormatTab {
    #[default]
    Number,
    Alignment,
    Font,
    Border,
    Fill,
    Protection,
}

impl FormatTab {
    pub(crate) const ALL: [FormatTab; 6] = [Self::Number, Self::Alignment, Self::Font, Self::Border, Self::Fill, Self::Protection];

    /// The tab of the panel header that holds this section: 書式 (表示形式・配置・フォント), 罫線 (罫線・塗りつぶし), 保護.
    pub(crate) fn header_tab(self) -> usize {
        match self {
            Self::Number | Self::Alignment | Self::Font => 0,
            Self::Border | Self::Fill => 1,
            Self::Protection => 2,
        }
    }

    /// The sections shown under header tab `tab`.
    pub(crate) fn of_header_tab(tab: usize) -> &'static [FormatTab] {
        match tab {
            0 => &[Self::Number, Self::Alignment, Self::Font],
            1 => &[Self::Border, Self::Fill],
            _ => &[Self::Protection],
        }
    }

    /// The labels of the panel header's tabs.
    pub(crate) const HEADER_LABELS: [&'static str; 3] = ["xlsx.format-pane.tab-format", "xlsx.format-pane.tab-border", "xlsx.format-pane.tab-protection"];
}

/// The listed angles of 文字の方向 (the label's id and the OOXML rotation); any other angle is カスタム.
const ROTATIONS: [(&str, u8); 6] = [("0°", 0), ("45°", 45), ("90°", 90), ("135°", 135), ("180°", 180), ("_xlsx.vertical-text", 255)];

/// Where the parts are (set when painted), for the scenarios.
pub(crate) type PaneBounds = Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>;
/// The items of the drop-downs by the id of the drop-down, for the scenarios.
pub(crate) type PaneMenus = Rc<RefCell<BTreeMap<String, Vec<MenuEntry>>>>;

pub struct FormatPanel {
    pub(crate) code: Entity<InputState>,
    pub(crate) custom_selected:bool,
    /// The font's name and size (点) typed in フォント.
    pub(crate) font: Entity<InputState>,
    pub(crate) size: Entity<InputState>,
    /// The text rotation angle (度) typed in 配置.
    pub(crate) rotation: Entity<InputState>,
    pub(crate) indent: Entity<InputState>,
    pub(crate) font_color:Entity<InputState>,
    pub(crate) fill_color:Entity<InputState>,
    pub(crate) pattern_color:Entity<InputState>,
    /// The line colour of 罫線 (a hex field, as the other colours): it is the workbook's current border colour.
    pub(crate) border_color:Entity<InputState>,
    /// The colour and the position (%) of the gradient's selected stop.
    pub(crate) grad_color: Entity<InputState>,
    pub(crate) grad_pos: Entity<InputState>,
    /// 文字の方向 is カスタム (not one of the listed angles): the angle field is on.
    pub(crate) rotation_custom: bool,
    /// The number format and rotation of the document at the last sync (to see that they changed under the two modes).
    seen: RefCell<(String, u8)>,
    /// リアルタイム更新: a change of the pane is written to the cells at once (ON, the default; kept in the settings file). OFF: the
    /// changes wait in `draft` and 適用 writes them together (one Undo step).
    pub(crate) live: bool,
    /// The changes made while `live` is off, in the order they were made (a change of the same kind replaces the earlier one).
    pub(crate) draft: Vec<StyleChange>,
    /// What the draft was made for (the cells selected, the sheet, the history position): another one drops the draft.
    draft_key: String,
    /// The stop of the gradient the colour and position fields show, the slider's drag state, and the history position and
    /// drag count of the last change by the slider (a drag is one Undo step).
    pub(crate) grad_selected: usize,
    grad_slider: GradientSliderState,
    grad_mark: Option<(usize, u64)>,
    /// The declared input fields (all of the inputs above).
    pub(crate) fields: SyncedFields,
    pub(crate) ui: PaneBounds,
    pub(crate) menus: PaneMenus,
    _subscriptions: Vec<Subscription>,
}

/// What the parts of the pane share while it is built.
#[derive(Clone)]
struct Pane {
    ui: PaneBounds,
    menus: PaneMenus,
    editor: WeakEntity<SlideEditor>,
}

impl Pane {
    /// `element` with its place noted when painted, under `id`.
    fn marked(&self, id: &str, element: impl IntoElement) -> AnyElement {
        let (ui, id) = (self.ui.clone(), id.to_string());
        div().relative().child(element).child(canvas(|_, _, _| {}, move |bounds, _, _, _| { ui.borrow_mut().insert(id.clone(), bounds); }).absolute().top_0().left_0().size_full()).into_any_element()
    }

    /// A drop-down filling its row's control cell: the choice now made and, when pressed, `menu`.
    fn select(&self, id: &'static str, label: SharedString, menu: Vec<MenuEntry>) -> AnyElement {
        self.select_with_icon(id, None, label, menu)
    }

    /// [`Pane::select`] whose items have icons (the items' `icon`): the icon of the choice made is on the closed button
    /// (`icon`) and every row of the list keeps its icon, the choice made shows by its row's background.
    fn select_with_icon(&self, id: &'static str, icon: Option<&'static str>, label: SharedString, menu: Vec<MenuEntry>) -> AnyElement {
        self.menus.borrow_mut().insert(id.to_string(), menu.clone());
        let button=crate::shell::chrome::dropdown_button(id,
            h_flex().w_full().gap(px(6.)).items_center()
                .when_some(icon, |d, name| d.child(crate::shell::chrome::icon(name, 16., crate::shell::chrome::text_primary())))
                .child(div().flex_1().truncate().text_size(px(13.)).child(label)).into_any_element());
        let dropdown = crate::shell::chrome::Dropdown::with_button(id,button,menu,true,0,&self.editor).fill_width();
        self.marked(id, if icon.is_some() { dropdown.checkmarks(false) } else { dropdown })
    }

    /// A drop-down whose items are pictures (the shared `preview_dropdown`: the same picture on the closed button
    /// and in the list, the name in a tooltip at the left of the item). `menu` is kept for the scenarios.
    fn preview(&self, id: &'static str, choices: Vec<PreviewChoice>, selected: usize, cx: &Context<SlideEditor>) -> AnyElement {
        self.menus.borrow_mut().insert(id.to_string(), choices.iter().map(|c| {
            let mut item = MenuItem::new("xlsx.preview-choice", c.label.clone());
            item.handler = Some(c.choose.clone());
            item.into()
        }).collect());
        self.marked(id, preview_dropdown(id, choices, selected, cx))
    }
}

fn rgb_hsla(c: ibis_doc_model::Rgb) -> Hsla {
    gpui_kit::rgb(c.0).into()
}


/// One line of the sample: its pieces, absolutely placed in the sample (`from` to `to`), as `border` draws them.
fn sample_line(from: (f32, f32), to: (f32, f32), border: Border) -> Vec<AnyElement> {
    let (thickness, pattern): (f32, &[f32]) = match border.line {
        BorderLine::Thin => (1.0, &[]),
        BorderLine::Hair => (1.0, &[1.0, 1.0]),
        BorderLine::Medium => (2.0, &[]),
        BorderLine::Thick => (3.0, &[]),
        BorderLine::Double => (1.0, &[]),
        BorderLine::Dotted => (1.0, &[2.0, 2.0]),
        BorderLine::Dashed => (1.0, &[6.0, 3.0]),
        BorderLine::DashDot => (1.0, &[6.0, 3.0, 2.0, 3.0]),
        BorderLine::DashDotDot => (1.0, &[6.0, 3.0, 2.0, 3.0, 2.0, 3.0]),
        BorderLine::MediumDashed => (2.0, &[6.0, 3.0]),
        BorderLine::MediumDashDot => (2.0, &[6.0, 3.0, 2.0, 3.0]),
        BorderLine::MediumDashDotDot => (2.0, &[6.0, 3.0, 2.0, 3.0, 2.0, 3.0]),
        BorderLine::SlantDashDot => (2.0, &[8.0, 3.0, 3.0, 3.0]),
    };
    let color = rgb_hsla(border.color);
    let length = ((to.0 - from.0).powi(2) + (to.1 - from.1).powi(2)).sqrt();
    // The stretches along the line that are drawn.
    let mut stretches = Vec::new();
    if pattern.is_empty() {
        stretches.push((0.0, length));
    } else {
        let (mut at, mut i) = (0.0, 0);
        while at < length {
            let len = pattern[i % pattern.len()].min(length - at);
            if i % 2 == 0 {
                stretches.push((at, len));
            }
            at += pattern[i % pattern.len()];
            i += 1;
        }
    }
    // A double line is two thin ones 2 px apart.
    let rows: &[f32] = if border.line == BorderLine::Double { &[-1.5, 1.5] } else { &[0.0] };
    let mut out = Vec::new();
    let piece = |x: f32, y: f32, w: f32, h: f32| div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h)).bg(color).into_any_element();
    if (from.1 - to.1).abs() < 0.01 {
        for &row in rows {
            for &(start, len) in &stretches {
                out.push(piece(from.0.min(to.0) + start, from.1 + row - thickness / 2.0, len, thickness));
            }
        }
    } else if (from.0 - to.0).abs() < 0.01 {
        for &row in rows {
            for &(start, len) in &stretches {
                out.push(piece(from.0 + row - thickness / 2.0, from.1.min(to.1) + start, thickness, len));
            }
        }
    } else {
        let (dx, dy) = ((to.0 - from.0) / length, (to.1 - from.1) / length);
        let t = thickness.max(1.5);
        for &(start, len) in &stretches {
            let mut s = start;
            while s < start + len {
                out.push(piece(from.0 + dx * s - t / 2.0, from.1 + dy * s - t / 2.0, t, t));
                s += 1.0;
            }
        }
    }
    out
}

/// A gradient of 塗りつぶし: how it goes. 0° is left to right, 90° top to bottom (Excel's linear degrees).
#[derive(Clone, Copy)]
enum GradientShape { Linear(u16), Center }
impl GradientShape {
    fn matches(self, g: &ibis_doc_model::CellGradient) -> bool {
        match (self, g) {
            (Self::Linear(d), ibis_doc_model::CellGradient::Linear { degree, .. }) => d == *degree,
            (Self::Center, ibis_doc_model::CellGradient::Path { .. }) => true,
            _ => false,
        }
    }
}
#[derive(Clone, Copy)]
struct GradientKind { label: &'static str, shape: GradientShape }
const GRADIENTS: [GradientKind; 5] = [
    GradientKind { label: "_xlsx.horizontal-left-to-right", shape: GradientShape::Linear(0) },
    GradientKind { label: "_xlsx.vertical-top-to-bottom", shape: GradientShape::Linear(90) },
    GradientKind { label: "_xlsx.diagonal-top-left-to-bottom-right", shape: GradientShape::Linear(45) },
    GradientKind { label: "_xlsx.diagonal-top-right-to-bottom-left", shape: GradientShape::Linear(135) },
    GradientKind { label: "_xlsx.from-center", shape: GradientShape::Center },
];
/// The colour a gradient starts from: the cell's fill, or a blue when it has none (or white, which would make a gradient to white
/// that cannot be seen). The list's pictures and what choosing an item applies use this one colour.
fn gradient_base(fill: Option<ibis_doc_model::Rgb>) -> ibis_doc_model::Rgb {
    fill.filter(|c| c.0 != 0xFF_FFFF).unwrap_or(ibis_doc_model::Rgb(0x4472C4))
}
/// The stops of a new gradient: from `color` to white.
fn default_gradient_stops(color: ibis_doc_model::Rgb) -> Vec<(u16, ibis_doc_model::Rgb)> {
    vec![(0, color), (1000, ibis_doc_model::Rgb(0xFF_FFFF))]
}
/// The gradient of `shape` with `stops` (what choosing an item of the list applies, and what its picture shows).
fn gradient_with(shape: GradientShape, stops: Vec<(u16, ibis_doc_model::Rgb)>) -> ibis_doc_model::CellGradient {
    match shape {
        GradientShape::Linear(degree) => ibis_doc_model::CellGradient::Linear { degree, stops },
        GradientShape::Center => ibis_doc_model::CellGradient::Path { path: ibis_doc_model::GradientPath::Rect, focus: [50_000, 50_000, 50_000, 50_000], stops },
    }
}

/// The frame of a picture in a list item: it fills the item's width.
fn preview_frame() -> Div {
    crate::shell::chrome::preview_dropdown::preview_frame()
}

/// The picture of a cell pattern: its 8 x 8 tile of cells (Excel's, as the sheet draws it), a pixel to a cell, in `fg` on `bg`
/// (the shared `pattern_preview`). `label` stands in for "none" (a pattern without a tile).
fn pattern_preview(preset: &'static str, fg: ibis_doc_model::Rgb, bg: ibis_doc_model::Rgb, label: SharedString) -> AnyElement {
    let tile = ibis_doc_model::spreadsheet_pattern_of_preset(preset).and_then(ibis_doc_model::spreadsheet_pattern_tile);
    crate::shell::chrome::preview_dropdown::pattern_preview(tile, fg, bg, label)
}

/// The picture of a cell gradient (the colours as the sheet draws it); `None` is the item without one (its name stands for it).
fn gradient_preview(gradient: Option<ibis_doc_model::CellGradient>, label: SharedString) -> AnyElement {
    let frame = preview_frame();
    match gradient {
        None => frame.bg(gpui_kit::rgb(0xffffff)).flex().items_center().px(px(4.)).text_size(px(12.)).child(div().min_w_0().truncate().child(label)).into_any_element(),
        Some(ibis_doc_model::CellGradient::Linear { degree, stops }) => {
            let at = stops_colors(&stops);
            // Painted as 2 px squares of the colour at their middle (a gradient quad is not drawn inside the list).
            frame.child(canvas(|_, _, _| {}, move |bounds, _, window, _| {
                const CELL: f32 = 2.0;
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                // 0° runs left to right, 90° top to bottom (Excel's linear angle, y down).
                let (dx, dy) = ((degree as f32).to_radians().cos(), (degree as f32).to_radians().sin());
                let span = (w * dx).abs() + (h * dy).abs();
                let mut y = 0.0;
                while y < h {
                    let mut x = 0.0;
                    while x < w {
                        let t = ((((x + CELL / 2.0) - w / 2.0) * dx + ((y + CELL / 2.0) - h / 2.0) * dy) / span + 0.5).clamp(0.0, 1.0);
                        window.paint_quad(gpui_kit::fill(Bounds::new(bounds.origin + point(px(x), px(y)), size(px(CELL.min(w - x)), px(CELL.min(h - y)))), rgb_hsla(at(t))));
                        x += CELL;
                    }
                    y += CELL;
                }
            }).absolute().inset_0()).into_any_element()
        }
        Some(ibis_doc_model::CellGradient::Path { stops, .. }) => {
            let at = stops_colors(&stops);
            frame.child(canvas(|_, _, _| {}, move |bounds, _, window, _| {
                // Rectangles shrinking to the middle, the colour going from the last stop's (the edge) to the first's (the centre).
                const STEPS: usize = 12;
                let (hx, hy) = (f32::from(bounds.size.width) / 2.0, f32::from(bounds.size.height) / 2.0);
                for i in 0..STEPS {
                    let (t, inset) = (i as f32 / (STEPS - 1) as f32, i as f32 / STEPS as f32);
                    window.paint_quad(gpui_kit::fill(Bounds::new(bounds.origin + point(px(hx * inset), px(hy * inset)), size(px(hx * 2.0 * (1.0 - inset)), px(hy * 2.0 * (1.0 - inset)))), rgb_hsla(at(1.0 - t))));
                }
            }).absolute().inset_0()).into_any_element()
        }
    }
}

/// The colour of a gradient with `stops` at `t` (0 to 1 along it), as the shared gradient slider interpolates the stops.
fn stops_colors(stops: &[(u16, ibis_doc_model::Rgb)]) -> impl Fn(f32) -> ibis_doc_model::Rgb + 'static {
    let points: Vec<ibis_doc_model::GradientStop> = stops.iter().map(|&(pos, c)| ibis_doc_model::GradientStop { position: pos as u32 * 100, color: ibis_doc_model::Color::opaque(c) }).collect();
    move |t| crate::shell::chrome::gradient_slider::interpolated(&points, (t.clamp(0.0, 1.0) * 100_000.0).round() as u32).rgb
}

/// Whether `code` is a complete number format code, so that applying it as it is typed never leaves the cells with half a code:
/// not empty, at most the 4 sections and 255 characters Excel takes, quotes and brackets closed, no escape (`\`, `_`, `*`) left without its character.
fn is_complete_format_code(code: &str) -> bool {
    if code.trim().is_empty() || code.chars().count() > 255 { return false; }
    let (mut sections, mut quoted, mut bracket, mut escaped) = (1, false, false, false);
    for c in code.chars() {
        if escaped { escaped = false; continue; }
        match c {
            '\\' | '_' | '*' if !quoted && !bracket => escaped = true,
            '"' if !bracket => quoted = !quoted,
            '[' if !quoted && !bracket => bracket = true,
            ']' if !quoted && bracket => bracket = false,
            ';' if !quoted && !bracket => sections += 1,
            _ => {}
        }
    }
    sections <= 4 && !quoted && !bracket && !escaped
}

/// The rotation field's text for an OOXML angle (`0..=90`, `91..=180` as -1..-90; vertical text has none).
fn rotation_text(rotation: u8) -> String {
    match rotation {
        0..=90 => format!("{rotation}"),
        91..=180 => format!("-{}", rotation as i16 - 90),
        _ => String::new(),
    }
}

/// The OOXML angle of the text typed in the rotation field (度, `-90..=90` or `0..=180`).
fn parse_rotation(text: &str) -> Option<u8> {
    let deg = text.trim().trim_end_matches('°').trim_end_matches('度').trim().parse::<i16>().ok()?;
    if (0..=180).contains(&deg) { Some(deg as u8) } else if (-90..=-1).contains(&deg) { Some((90 - deg) as u8) } else { None }
}

/// The colour typed in a colour field: `RRGGBB` or `#RRGGBB`; nothing typed is `None` (no colour).
fn parse_color(text: &str) -> Option<Option<ibis_doc_model::Rgb>> {
    let t = text.trim().trim_start_matches('#');
    if t.is_empty() { return Some(None); }
    (t.len() == 6).then(|| u32::from_str_radix(t, 16).ok().map(|c| Some(ibis_doc_model::Rgb(c)))).flatten()
}
fn show_color(c: &Option<ibis_doc_model::Rgb>) -> String {
    c.map(|c| format!("{:06X}", c.0)).unwrap_or_default()
}

/// The text of a gradient stop's position (per mille) in percent: `50`, `12.5`.
fn position_text(per_mille: u16) -> String {
    if per_mille % 10 == 0 { format!("{}", per_mille / 10) } else { format!("{:.1}", per_mille as f32 / 10.0) }
}

/// The stops of a cell gradient.
fn gradient_stops(g: &ibis_doc_model::CellGradient) -> &Vec<(u16, ibis_doc_model::Rgb)> {
    match g {
        ibis_doc_model::CellGradient::Linear { stops, .. } | ibis_doc_model::CellGradient::Path { stops, .. } => stops,
    }
}
fn gradient_stops_mut(g: &mut ibis_doc_model::CellGradient) -> &mut Vec<(u16, ibis_doc_model::Rgb)> {
    match g {
        ibis_doc_model::CellGradient::Linear { stops, .. } | ibis_doc_model::CellGradient::Path { stops, .. } => stops,
    }
}

/// Declare every input field of セルの書式設定 (the only place where they are made). Order: code, font, size, rotation,
/// indent, font colour, fill colour, pattern colour, border colour, gradient stop colour, gradient stop position.
/// What the fields read is the pane's style (the document's with the draft of 適用 applied) and what they write goes through
/// `panel_change` (at once, or into the draft when リアルタイム更新 is off).
fn format_fields(fields: &SyncedFields, editor: &SlideEditor, subs: &mut Vec<Subscription>, window: &mut Window, cx: &mut Context<SlideEditor>) -> [Entity<InputState>; 11] {
    use ibis_doc_model::Rgb;
    let code = fields.add(FieldSpec::<String> {
        id: "code", placeholder: "#,##0", commit: Commit::Typing,
        read: |e| e.panel_style().number_format,
        show: |v| v.clone(),
        // A code that is not complete is a draft: applying it as it is typed never leaves the cells with half a code.
        parse: |t| is_complete_format_code(t).then(|| t.to_string()),
        write: |e, v, cx| e.panel_change(StyleChange::NumberFormat(v), cx),
    }, editor, subs, window, cx);
    let font = fields.add(FieldSpec::<Option<String>> {
        id: "font", placeholder: "游ゴシック", commit: Commit::Enter, // i18n-ok（フォント名の見本（入力欄の placeholder）。どの言語でも同じ）
        read: |e| e.panel_style().font,
        show: |v| v.clone().unwrap_or_default(),
        parse: |t| Some((!t.trim().is_empty()).then(|| t.trim().to_string())),
        write: |e, v, cx| e.panel_change(StyleChange::Font(v), cx),
    }, editor, subs, window, cx);
    let size = fields.add(FieldSpec::<u32> {
        id: "size", placeholder: "11", commit: Commit::Typing,
        read: |e| e.panel_style().font_size,
        show: |v| format!("{}", *v as f32 / 100.0),
        parse: |t| t.trim().parse::<f32>().ok().filter(|p| (1.0..=409.0).contains(p)).map(|p| (p * 100.0).round() as u32),
        write: |e, v, cx| e.panel_change(StyleChange::FontSize(v), cx),
    }, editor, subs, window, cx);
    let rotation = fields.add(FieldSpec::<u8> {
        id: "rotation", placeholder: "0", commit: Commit::Typing,
        read: |e| e.panel_style().rotation,
        show: |v| rotation_text(*v),
        parse: parse_rotation,
        write: |e, v, cx| e.panel_change(StyleChange::Rotation(v), cx),
    }, editor, subs, window, cx);
    let indent = fields.add(FieldSpec::<u32> {
        id: "indent", placeholder: "", commit: Commit::Typing,
        read: |e| e.panel_style().indent as u32,
        show: |v| v.to_string(),
        parse: |t| t.trim().parse::<u32>().ok().map(|n| n.min(250)),
        write: |e, v, cx| e.panel_change(StyleChange::Indent(v), cx),
    }, editor, subs, window, cx);
    let font_color = fields.add(FieldSpec::<Rgb> {
        id: "font-color", placeholder: "", commit: Commit::Typing,
        read: |e| e.panel_style().color,
        show: |v| format!("{:06X}", v.0),
        parse: |t| parse_color(t).map(|c| c.unwrap_or(Rgb::BLACK)),
        write: |e, v, cx| e.panel_change(StyleChange::Color(v), cx),
    }, editor, subs, window, cx);
    let fill_color = fields.add(FieldSpec::<Option<Rgb>> {
        id: "fill-color", placeholder: "", commit: Commit::Typing,
        read: |e| e.panel_style().fill,
        show: show_color,
        parse: parse_color,
        write: |e, v, cx| e.panel_change(StyleChange::Fill(v), cx),
    }, editor, subs, window, cx);
    let pattern_color = fields.add(FieldSpec::<Option<Rgb>> {
        id: "pattern-color", placeholder: "", commit: Commit::Typing,
        read: |e| e.panel_style().pattern.map(|p| p.1),
        show: show_color,
        parse: parse_color,
        write: |e, v, cx| {
            // Nothing typed is the default colour (black): not a change when the pattern already has it.
            let c = v.unwrap_or(Rgb::BLACK);
            if e.panel_style().pattern.is_none_or(|p| p.1 != c) { e.panel_change(StyleChange::PatternColor(c), cx); }
        },
    }, editor, subs, window, cx);
    // The line colour of 罫線: the workbook's current border colour (what the toolbar's 枠線の色 sets too).
    let border_color = fields.add(FieldSpec::<Rgb> {
        id: "border-color", placeholder: "", commit: Commit::Typing,
        read: |e| e.wb().border_color,
        show: |v| format!("{:06X}", v.0),
        parse: |t| parse_color(t).and_then(|c| c),
        write: |e, v, cx| { e.wb_mut().border_color = v; cx.notify(); },
    }, editor, subs, window, cx);
    // The colour and the position (%) of the gradient's selected stop.
    let grad_color = fields.add(FieldSpec::<Rgb> {
        id: "grad-color", placeholder: "", commit: Commit::Typing,
        read: |e| e.panel_gradient_stop().map(|s| s.1).unwrap_or(Rgb::BLACK),
        show: |v| format!("{:06X}", v.0),
        parse: |t| parse_color(t).and_then(|c| c),
        write: |e, v, cx| e.panel_edit_gradient_stop(cx, |s| s.1 = v),
    }, editor, subs, window, cx);
    let grad_pos = fields.add(FieldSpec::<u16> {
        id: "grad-pos", placeholder: "", commit: Commit::Typing,
        read: |e| e.panel_gradient_stop().map(|s| s.0).unwrap_or(0),
        show: |v| position_text(*v),
        parse: |t| t.trim().trim_end_matches('%').trim().parse::<f32>().ok().filter(|p| (0.0..=100.0).contains(p)).map(|p| (p * 10.0).round() as u16),
        write: |e, v, cx| e.panel_edit_gradient_stop(cx, |s| s.0 = v),
    }, editor, subs, window, cx);
    [code, font, size, rotation, indent, font_color, fill_color, pattern_color, border_color, grad_color, grad_pos]
}

/// The sample's size and the margin around the cell in it.
const SAMPLE_W: f32 = 168.0;
const SAMPLE_H: f32 = 96.0;
const MARGIN: f32 = 10.0;

/// The settings key of リアルタイム更新 (`1` / `0`; ON when it is not set).
const LIVE_UPDATE_KEY: &str = "xlsx.format-pane.live-update";

/// Whether リアルタイム更新 is on in the settings.
fn live_update_setting() -> bool {
    crate::shell::settings::get(LIVE_UPDATE_KEY).is_none_or(|v| v != "0")
}

#[ibis_office_macros::editor_ext(CustomFormat)]
impl SlideEditor {
    /// セルの書式設定 (⌘1): the sections open as they were left.
    pub fn open_custom_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_cell_format_with(FormatTab::Number, false, window, cx);
    }

    /// セルの書式設定, with the section `tab` opened (for the scenarios).
    #[allow(dead_code)]
    pub(crate) fn open_cell_format(&mut self, tab: FormatTab, window: &mut Window, cx: &mut Context<Self>) {
        self.open_cell_format_with(tab, true, window, cx);
    }

    /// The pane following the selection (right_pane.rs): the cell format, without taking the keyboard from the sheet.
    pub(crate) fn open_custom_format_unfocused(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_cell_format_focus(FormatTab::Number, false, false, window, cx);
    }

    fn open_cell_format_with(&mut self, tab: FormatTab, show: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.open_cell_format_focus(tab, show, true, window, cx);
    }

    fn open_cell_format_focus(&mut self, tab: FormatTab, show: bool, focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        if focus { self.document.begin_repeat_sheet_group(); }
        let style = self.current_style();
        // Every input of the pane is declared in `format_fields` (shell/chrome/synced_field.rs): it shows the document's
        // value at every render (Undo, Redo, another cell, an external update) and writes what is typed.
        let fields = SyncedFields::new();
        let mut subscriptions = Vec::new();
        let [code, font, size, rotation, indent, font_color, fill_color, pattern_color, border_color, grad_color, grad_pos] = format_fields(&fields, self, &mut subscriptions, window, cx);
        if focus && tab == FormatTab::Number {
            code.update(cx, |i, cx| i.focus(window, cx));
        }
        if let Some(s) = self.sheet_state_mut() {
            s.close_settings();
            if show {
                s.format_tab = tab.header_tab();
            }
            s.custom_format = Some(FormatPanel {
                custom_selected: !super::toolbar::NUMBER_FORMATS.iter().any(|(_, f)| *f == style.number_format),
                code, font, size, rotation, indent, font_color, fill_color, pattern_color, border_color, grad_color, grad_pos,
                rotation_custom: !ROTATIONS.iter().any(|(_, d)| *d == style.rotation),
                seen: RefCell::new((style.number_format.clone(), style.rotation)),
                live: live_update_setting(),
                draft: Vec::new(),
                draft_key: String::new(),
                grad_selected: 0,
                grad_slider: GradientSliderState::default(),
                grad_mark: None,
                fields,
                ui: Default::default(),
                menus: Default::default(),
                _subscriptions: subscriptions,
            });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The style the pane shows and edits: the selected cell's, with the draft of 適用 (what was changed while リアルタイム更新 is off) on it.
    pub(crate) fn panel_style(&self) -> ibis_doc_model::CellStyle {
        let mut style = self.current_style();
        if let Some(panel) = self.sheet_state().and_then(|s| s.custom_format.as_ref()) {
            for change in &panel.draft {
                change.apply(&mut style);
            }
        }
        style
    }

    /// What a draft belongs to: the sheet, the cells selected and the history position. A draft for other cells is dropped.
    fn panel_draft_key(&self) -> String {
        format!("{:?}|{:?}|{}|{}", self.current_slide, self.selected_ranges(), self.document.undo_len(), self.document.can_redo())
    }

    /// A change made in the pane: written to the cells at once (リアルタイム更新 on), or kept in the draft until 適用.
    /// The draft keeps the changes in the order they were made; a change of the same kind replaces the one before it.
    pub(crate) fn panel_change(&mut self, change: StyleChange, cx: &mut Context<Self>) {
        let live = self.sheet_state().and_then(|s| s.custom_format.as_ref()).is_none_or(|p| p.live);
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.custom_format.as_mut()) {
            p.grad_mark = None;
        }
        if live {
            self.format_cells(change, cx);
            return;
        }
        let key = self.panel_draft_key();
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.custom_format.as_mut()) {
            if p.draft.is_empty() {
                p.draft_key = key;
            }
            p.draft.retain(|c| std::mem::discriminant(c) != std::mem::discriminant(&change));
            p.draft.push(change);
        }
        cx.notify();
    }

    /// 適用: the draft goes to the selected cells together, as one Undo step.
    pub(crate) fn panel_apply(&mut self, cx: &mut Context<Self>) {
        let changes = self.sheet_state_mut().and_then(|s| s.custom_format.as_mut()).map(|p| std::mem::take(&mut p.draft)).unwrap_or_default();
        self.format_cells_batch(changes, cx);
        cx.notify();
    }

    /// リアルタイム更新 turned on or off (kept in the settings file). Turning it on writes what waits in the draft.
    pub(crate) fn panel_set_live(&mut self, on: bool, cx: &mut Context<Self>) {
        crate::shell::settings::set(LIVE_UPDATE_KEY, if on { "1" } else { "0" });
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.custom_format.as_mut()) {
            p.live = on;
            p.grad_mark = None;
        }
        if on {
            self.panel_apply(cx);
        }
        cx.notify();
    }

    /// `changes` written to the selected cells as ONE Undo step (what `format_cells` does for one change).
    fn format_cells_batch(&mut self, changes: Vec<StyleChange>, cx: &mut Context<Self>) {
        if changes.is_empty() {
            return;
        }
        // Text being edited and the shapes of the sheet take a change their own way: one by one.
        if changes.len() == 1 || self.editing.is_some() || self.cell_edit_mut().is_some() || !self.selected_ids().is_empty() || self.selected_sheet_shape().is_some() {
            for change in changes {
                self.format_cells(change, cx);
            }
            return;
        }
        self.commit_cell_edit(cx);
        if self.protection_refuses(super::protection::Guarded::FormatCells, cx) {
            return;
        }
        let Some(range) = self.selected_range() else { return };
        let (slide, ranges) = (self.current_slide, self.selected_ranges());
        let wrap_on = changes.iter().any(|c| matches!(c, StyleChange::Wrap(true) | StyleChange::Rotation(1..=255)));
        let mut commands: Vec<ibis_doc_model::Command> = changes
            .into_iter()
            .map(|change| ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetFormat { slide, ranges: ranges.clone(), change }))
            .collect();
        self.run_sheet(commands.clone(), cx);
        // Wrapping or turning on: the rows grow to the lines, as Excel's do (in the same step).
        if wrap_on {
            let ts = cx.text_system().clone();
            let fit = self.fit_rows_commands(range.first_row, range.last_row, &ts);
            if !fit.is_empty() {
                self.document.undo();
                commands.extend(fit);
                self.run_sheet(commands, cx);
            }
        }
    }

    /// The selected stop of the gradient the pane shows (position per mille, colour).
    pub(crate) fn panel_gradient_stop(&self) -> Option<(u16, ibis_doc_model::Rgb)> {
        let gradient = self.panel_style().gradient?;
        let stops = gradient_stops(&gradient);
        let at = self.sheet_state()?.custom_format.as_ref()?.grad_selected.min(stops.len().saturating_sub(1));
        stops.get(at).copied()
    }

    /// The selected stop of the gradient changed by `edit` (the colour or position field, the spinner).
    fn panel_edit_gradient_stop(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut (u16, ibis_doc_model::Rgb))) {
        let Some(mut gradient) = self.panel_style().gradient else { return };
        let selected = self.sheet_state().and_then(|s| s.custom_format.as_ref()).map_or(0, |p| p.grad_selected);
        let stops = gradient_stops_mut(&mut gradient);
        let at = selected.min(stops.len().saturating_sub(1));
        if let Some(stop) = stops.get_mut(at) {
            edit(stop);
        }
        self.panel_change(StyleChange::Gradient(Some(gradient)), cx);
    }

    /// The selected stop taken out of the gradient (a gradient keeps at least two).
    fn panel_remove_gradient_stop(&mut self, cx: &mut Context<Self>) {
        let Some(mut gradient) = self.panel_style().gradient else { return };
        let selected = self.sheet_state().and_then(|s| s.custom_format.as_ref()).map_or(0, |p| p.grad_selected);
        let stops = gradient_stops_mut(&mut gradient);
        if stops.len() <= 2 {
            return;
        }
        stops.remove(selected.min(stops.len() - 1));
        let left = stops.len();
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.custom_format.as_mut()) {
            p.grad_selected = selected.min(left - 1);
        }
        self.panel_change(StyleChange::Gradient(Some(gradient)), cx);
    }

    /// The gradient slider moved, added or selected a stop. A drag of a stop is ONE Undo step: while the mouse is down, the
    /// step the drag made last is replaced (the history position and the number of drags ended tell it is still the same drag).
    fn panel_slider_changed(&mut self, points: Vec<ibis_doc_model::GradientStop>, selected: usize, cx: &mut Context<Self>) {
        let Some(old) = self.panel_style().gradient else { return };
        let stops: Vec<(u16, ibis_doc_model::Rgb)> = points.iter().map(|s| ((s.position / 100).min(1000) as u16, s.color.rgb)).collect();
        let gradient = match old {
            ibis_doc_model::CellGradient::Linear { degree, .. } => ibis_doc_model::CellGradient::Linear { degree, stops },
            ibis_doc_model::CellGradient::Path { path, focus, .. } => ibis_doc_model::CellGradient::Path { path, focus, stops },
        };
        let (live, ended, mark) = self.sheet_state().and_then(|s| s.custom_format.as_ref()).map_or((true, 0, None), |p| (p.live, p.grad_slider.drags_ended(), p.grad_mark));
        if live && mark == Some((self.document.undo_len(), ended)) && !self.document.can_redo() {
            self.document.undo();
        }
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.custom_format.as_mut()) {
            p.grad_selected = selected;
        }
        self.panel_change(StyleChange::Gradient(Some(gradient)), cx);
        let len = self.document.undo_len();
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.custom_format.as_mut()) {
            p.grad_mark = live.then_some((len, ended));
        }
    }

    /// Render step of the pane: a draft made for other cells is dropped, the fields show the document's values (see
    /// `SyncedFields`), and the two modes the pane keeps (カスタム of the category and of the text direction) go back to what
    /// the document says when the document changed under them (Undo, another cell) and the user is not typing in the field
    /// the mode belongs to.
    pub(crate) fn sync_format_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.sheet_state().and_then(|s| s.custom_format.as_ref()) else { return };
        let (fields, code, rotation) = (panel.fields.clone(), panel.code.clone(), panel.rotation.clone());
        if !panel.draft.is_empty() && panel.draft_key != self.panel_draft_key() {
            if let Some(p) = self.sheet_state_mut().and_then(|s| s.custom_format.as_mut()) { p.draft.clear(); }
        }
        let history = self.document.undo_len();
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.custom_format.as_mut()) {
            if p.grad_mark.is_some_and(|(len, _)| len != history) { p.grad_mark = None; }
        }
        let style = self.panel_style();
        let (code_focused, rotation_focused) = (code.read(cx).focus_handle(cx).is_focused(window), rotation.read(cx).focus_handle(cx).is_focused(window));
        fields.sync(self, window, cx);
        if let Some(p) = self.sheet_state_mut().and_then(|s| s.custom_format.as_mut()) {
            let mut seen = p.seen.borrow_mut();
            if seen.0 != style.number_format {
                seen.0 = style.number_format.clone();
                if !code_focused { p.custom_selected = false; }
            }
            if seen.1 != style.rotation {
                seen.1 = style.rotation;
                if !rotation_focused { p.rotation_custom = false; }
            }
        }
    }

    /// The size (点) typed in the pane, applied to the selected cells.
    fn apply_pane_size(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Ok(points) = text.parse::<f32>()
            && (1.0..=409.0).contains(&points)
        {
            self.panel_change(StyleChange::FontSize((points * 100.0).round() as u32), cx);
        }
    }

    /// The rotation angle (度, `-90..=90` or OOXML `0..=180`) given as text, applied to the selected cells.
    pub(crate) fn apply_pane_rotation(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Some(r) = parse_rotation(text) {
            self.panel_change(StyleChange::Rotation(r), cx);
        }
    }

    pub fn custom_format_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.custom_format.as_ref()?;
        let (code, font, size, rotation) = (panel.code.clone(), panel.font.clone(), panel.size.clone(), panel.rotation.clone());
        let indent=panel.indent.clone();
        let _strict = panel.fields.strict_scope();
        let style = self.panel_style();
        let (live, draft_empty) = (panel.live, panel.draft.is_empty());
        // カスタム is what the user chose, or what the document's code is (a code that is not in the list).
        let custom_selected = panel.custom_selected || !super::toolbar::NUMBER_FORMATS.iter().any(|(_, f)| *f == style.number_format);
        let pane = Pane { ui: panel.ui.clone(), menus: panel.menus.clone(), editor: cx.entity().downgrade() };
        let current_tab = self.wb().format_tab.min(2);
        pane.menus.borrow_mut().clear();
        let text = |s: &str| SharedString::from(crate::shell::i18n::text(s).to_string());
        let caption = |s: &str| div().text_sm().text_color(crate::shell::chrome::text_secondary()).child(text(s));
        let change_check = |id: String, label: SharedString, checked: bool, change: StyleChange, cx: &mut Context<Self>| {
            pane.marked(&id.clone(), crate::shell::chrome::checkbox_button(SharedString::from(id), label, checked, false,
                cx.listener(move |this, _, _, cx| this.panel_change(change.clone(), cx))))
        };
        // A menu item that changes the selected cells' format.
        let choice = |id: &'static str, label: SharedString, on: bool, change: StyleChange| -> MenuEntry {
            MenuItem::new(id, label).checked(on).on(move |this, _, cx| this.panel_change(change.clone(), cx)).into()
        };
        let (rotation_custom, border_color) = (panel.rotation_custom, panel.border_color.clone());

        let mut body = v_flex().gap_1().px(px(10.)).py(px(8.)).p_0();
        // リアルタイム更新: on the top of the first tab. Off, the changes wait for 適用 (the action bar area).
        if current_tab == 0 {
            body = body.child(div().px(px(10.)).pt(px(8.)).child(pane.marked("cell-format-live",
                crate::shell::chrome::checkbox_button("cell-format-live", text("xlsx.format-pane.live-update"), live, false,
                    cx.listener(move |this, _, _, cx| this.panel_set_live(!live, cx))))));
        }
        // The sections of the tab, one under the other (the tabs of the panel header do the switching; they have no headings).
        for &tab in FormatTab::of_header_tab(current_tab) {
            let mut section = v_flex().gap_2().px(px(6.)).pt(px(4.)).pb(px(10.));
            match tab {
                FormatTab::Number => {
                    let typed = code.read(cx).value().to_string();
                    // What the selected cell shows in it.
                    let sample = self.current_cell().and_then(|(c, r)| self.current_sheet().and_then(|(_, s)| s.cell(r, c).map(|x| x.value.clone()))).map(|v| ibis_calc::numfmt::format(&v, &typed, false).text).unwrap_or_default();
                    let current = super::toolbar::NUMBER_FORMATS.iter().find(|(_, f)| *f == style.number_format).map(|(name, _)| text(name)).unwrap_or_else(|| text("xlsx.format-pane.custom"));
                    let current_icon = if custom_selected { super::toolbar::number_format_icon("") } else { super::toolbar::number_format_icon(&style.number_format) };
                    let mut items:Vec<MenuEntry>=super::toolbar::NUMBER_FORMATS.iter().map(|&(name,format)|{
                        MenuItem::new("xlsx.number-format",text(name)).icon(super::toolbar::number_format_icon(format)).checked(!custom_selected&&style.number_format==format).on(move |e,_,cx|{
                            if let Some(p)=e.wb_mut().custom_format.as_mut(){p.custom_selected=false;}
                            // The format's own code shows in the user-defined field by itself (the field follows the document).
                            e.panel_change(StyleChange::NumberFormat(format.into()),cx);
                        }).into()}).collect();
                    items.push(MenuItem::new("xlsx.number-format.custom",text("xlsx.format-pane.custom")).icon(super::toolbar::number_format_icon("")).checked(custom_selected).on(|e,_,cx|{if let Some(p)=e.wb_mut().custom_format.as_mut(){p.custom_selected=true;}cx.notify();}).into());
                    section = section
                        .child(dropdown_row(text("xlsx.format-pane.category"), pane.select_with_icon("cell-format-number", Some(current_icon), if custom_selected{text("xlsx.format-pane.custom")}else{current}, items)))
                        // The code applies to the selected cells as it is typed (a code that is not complete is not applied).
                        .child(text_field_row(text("xlsx.format-pane.custom"), crate::shell::chrome::text_field(&code).full_width().disabled(!custom_selected)))
                        // One line, not wrapped: what does not fit is clipped at the right edge of the section (the panel's own right margin).
                        .child(div().w_full().min_w_0().overflow_hidden().whitespace_nowrap().text_sm().child(SharedString::from(format!("{}{sample}", crate::shell::i18n::text("_xlsx.sample")))));
                }
                FormatTab::Alignment => {
                    let horizontal = [("_xlsx.general", CellAlign::General), ("_xlsx.left-indent", CellAlign::Left), ("_shell.center-2", CellAlign::Center), ("_xlsx.right-indent", CellAlign::Right), ("_xlsx.fill", CellAlign::Fill), ("align.justify", CellAlign::Justify), ("_xlsx.center-across-selection", CellAlign::CenterAcross), ("_xlsx.distributed-indent", CellAlign::Distributed)];
                    let vertical = [("_xlsx.top-2", CellVerticalAlign::Top), ("_shell.center-2", CellVerticalAlign::Center), ("_xlsx.bottom-2", CellVerticalAlign::Bottom), ("align.justify", CellVerticalAlign::Justify), ("_shell.distributed", CellVerticalAlign::Distributed)];
                    let listed = ROTATIONS.iter().any(|(_, d)| *d == style.rotation);
                    let is_custom = rotation_custom || !listed;
                    let h_label = horizontal.iter().find(|(_, a)| *a == style.align).map(|(n, _)| text(n)).unwrap_or_default();
                    let v_label = vertical.iter().find(|(_, a)| *a == style.vertical).map(|(n, _)| text(n)).unwrap_or_default();
                    let t_label = if is_custom { text("xlsx.format-pane.rotation-numeric") } else {
                        ROTATIONS.iter().find(|(_, d)| *d == style.rotation).map(|(n, _)| text(n)).unwrap_or_default()
                    };
                    let h_items: Vec<MenuEntry> = horizontal.into_iter().map(|(n, a)| choice("xlsx.halign", text(n), style.align == a, StyleChange::Align(a))).collect();
                    let v_items: Vec<MenuEntry> = vertical.into_iter().map(|(n, a)| choice("xlsx.valign", text(n), style.vertical == a, StyleChange::Vertical(a))).collect();
                    let mut t_items: Vec<MenuEntry> = Vec::new();
                    for &(n, d) in ROTATIONS.iter() {
                        t_items.push(MenuItem::new("xlsx.rotation", text(n)).checked(!is_custom && style.rotation == d).on(move |this, _, cx| {
                            if let Some(p) = this.wb_mut().custom_format.as_mut() { p.rotation_custom = false; }
                            this.panel_change(StyleChange::Rotation(d), cx);
                        }).into());
                        // 数値指定 comes after 180°: the angle field is on and the angle typed in it applies.
                        if d == 180 {
                            t_items.push(MenuItem::new("xlsx.rotation.custom", text("xlsx.format-pane.rotation-numeric")).checked(is_custom).on(|this, _, cx| {
                                if let Some(p) = this.wb_mut().custom_format.as_mut() { p.rotation_custom = true; }
                                cx.notify();
                            }).into());
                        }
                    }

                    section = section
                        .child(dropdown_row(text("_xlsx.horizontal"), pane.select("cell-format-halign", h_label, h_items)))
                        .child(dropdown_row(text("_xlsx.vertical"), pane.select("cell-format-valign", v_label, v_items)))
                        .child(dropdown_row(text("_xlsx.text-direction"), pane.select("cell-format-rotation", t_label, t_items)))
                        .child(text_field_row(text("xlsx.format-pane.angle"), crate::shell::chrome::text_field(&rotation).unit("°",px(20.)).disabled(!is_custom).spinner(
                            "cell-format-rotation-spin",
                            crate::shell::chrome::SpinnerSpec::new(1.0).range(-90.0, 90.0).integer_only(),
                            cx,
                            |this, deg, _, cx| this.apply_pane_rotation(&format!("{}", deg as i32), cx),
                        )))
                        .child(h_flex().gap_1().flex_wrap()
                            .child(change_check("cell-format-wrap".into(), text("_xlsx.wrap-text"), style.wrap, StyleChange::Wrap(!style.wrap), cx))
                            .child(change_check("cell-format-shrink".into(), text("_xlsx.shrink-to-fit"), style.shrink, StyleChange::Shrink(!style.shrink), cx)))
                        .child(pane.marked("cell-format-indent", text_field_row(text("_xlsx.indent"),crate::shell::chrome::text_field(&indent).spinner("cell-indent",crate::shell::chrome::SpinnerSpec::new(1.).range(0.,250.).integer_only(),cx,|e,n,_,cx|e.panel_change(StyleChange::Indent(n as u32),cx)))));
                }
                FormatTab::Font => {
                    // Bold, italic, underline and strike: the toolbar's buttons, in a row.
                    let toggle = |id: &'static str, icon: &'static str, tip: &'static str, on: bool, change: StyleChange, cx: &mut Context<Self>| -> ToolbarItem {
                        ToolbarItem::icon(id, icon).on(on).tooltip(text(tip)).on_click(cx.listener(move |this, _, _, cx| this.panel_change(change.clone(), cx)))
                    };
                    let styles = [
                        toggle("cell-format-bold", "format_bold", "_shell.bold-b", style.bold, StyleChange::Bold(!style.bold), cx),
                        toggle("cell-format-italic", "format_italic", "_shell.italic-i", style.italic, StyleChange::Italic(!style.italic), cx),
                        toggle("cell-format-underline", "format_underlined", "_shell.underline-u", style.underline.enabled(), StyleChange::Underline(!style.underline.enabled()), cx),
                        toggle("cell-format-strike", "format_strikethrough", "pptx.chrome.strikethrough", style.strike, StyleChange::Strike(!style.strike), cx),
                    ];
                    let u_label = |u: ibis_doc_model::CellUnderline| match u {
                        ibis_doc_model::CellUnderline::None => text("_xlsx.no-underline"),
                        ibis_doc_model::CellUnderline::Single => text("_xlsx.single-underline"),
                        ibis_doc_model::CellUnderline::Double => text("_xlsx.double-underline"),
                        ibis_doc_model::CellUnderline::SingleAccounting => text("_xlsx.single-accounting"),
                        ibis_doc_model::CellUnderline::DoubleAccounting => text("_xlsx.double-accounting"),
                    };
                    let u_items: Vec<MenuEntry> = ibis_doc_model::CellUnderline::ALL
                        .into_iter()
                        .map(|u| choice("xlsx.underline-kind", u_label(u), style.underline == u, StyleChange::UnderlineKind(u)))
                        .collect();
                    let effects = [
                        ("_xlsx.normal-no-superscript-or-subscript", ibis_doc_model::VerticalPosition::Baseline),
                        ("docx.chrome.superscript", ibis_doc_model::VerticalPosition::Superscript),
                        ("docx.chrome.subscript", ibis_doc_model::VerticalPosition::Subscript),
                    ];
                    let e_label = effects.iter().find(|(_, v)| *v == style.vert_align).map(|(n, _)| text(n)).unwrap_or_default();
                    let e_items: Vec<MenuEntry> = effects
                        .into_iter()
                        .map(|(n, v)| choice("xlsx.font-effect", text(n), style.vert_align == v, StyleChange::VertAlign(v)))
                        .collect();
                    // Choosing a font applies it at once (there is no 設定 button): on one line with its label when it fits.
                    section = section
                        .child(font_dropdown_row(text("_xlsx.font-name"), "panel-font-xlsx-custom-format-1", &font, cx,
                            |this, name, _, cx| this.panel_change(StyleChange::Font((!name.is_empty()).then(|| name.to_string())), cx)))
                        .child(text_field_row(text("xlsx.format-pane.size"),crate::shell::chrome::text_field(&size).unit("pt",px(20.)).spinner("cell-format-size-spin",crate::shell::chrome::SpinnerSpec::new(1.).range(1.,409.),cx,|this,n,_,cx|this.apply_pane_size(&n.to_string(),cx))))
                        .child(color_row(text("_shell.font-color"), pane.marked("cell-format-color",crate::shell::chrome::panel_color_field("cell-format-color-input",&panel.font_color,cx))))
                        .child(toolbar_row(text("xlsx.format-pane.style"), styles))
                        .child(dropdown_row(text("xlsx.format-pane.underline"), pane.select("cell-format-underline-kind", u_label(style.underline), u_items)))
                        .child(dropdown_row(text("xlsx.font-effects"), pane.select("cell-format-effect", e_label, e_items)));
                }
                FormatTab::Border => {
                    let range = self.selected_range();
                    let multi_rows = range.is_some_and(|r| r.last_row > r.first_row);
                    let multi_columns = range.is_some_and(|r| r.last_column > r.first_column);
                    section = section
                        .child(dropdown_row(text("_xlsx.line-style"), pane.select("cell-format-line", border_line_label(self.wb().border_line), super::toolbar::border_style_items(self))))
                        .child(color_row(text("docx.common.color"), pane.marked("cell-format-border-color", crate::shell::chrome::panel_color_field("cell-format-border-color-input", &border_color, cx))))
                        .child(caption("_xlsx.borders"))
                        .child(self.border_sample(&pane, multi_rows, multi_columns, cx));
                }
                FormatTab::Fill => {
                    let cur_pat = style.pattern.as_ref().map(|p| p.0.as_str()).unwrap_or("");
                    let pat_fg = style.pattern.as_ref().map(|p| p.1).unwrap_or(ibis_doc_model::Rgb(0));
                    let pat_bg = style.pattern.as_ref().map(|p| p.2).or(style.fill).unwrap_or(ibis_doc_model::Rgb(0xFF_FFFF));
                    // なし is the first item.
                    let pat_choices: Vec<PreviewChoice> = super::format_more::PATTERNS.iter().map(|&(n, preset)| {
                        let (label, shown) = (text(n), text(n));
                        PreviewChoice {
                            label,
                            content: Rc::new(move || pattern_preview(preset, pat_fg, pat_bg, shown.clone())),
                            choose: Rc::new(move |this, _, cx| {
                                let st = this.panel_style();
                                let fg = st.pattern.as_ref().map(|p| p.1).unwrap_or(ibis_doc_model::Rgb(0));
                                let bg = st.pattern.as_ref().map(|p| p.2).or(st.fill).unwrap_or(ibis_doc_model::Rgb(0xFF_FFFF));
                                let change = if preset.is_empty() { StyleChange::Pattern(None) } else { StyleChange::Pattern(Some((preset.to_string(), fg, bg))) };
                                this.panel_change(change, cx);
                            }),
                        }
                    }).collect();
                    let pat_selected = super::format_more::PATTERNS.iter().position(|(_, p)| *p == cur_pat).unwrap_or(0);
                    // The stops a gradient chosen from the list gets: the ones the cell's gradient has (so the direction can change
                    // without losing the stops adjusted), else from the fill colour to white.
                    let grad_color = gradient_base(style.fill);
                    let grad_stops = style.gradient.as_ref().map(|g| gradient_stops(g).clone()).unwrap_or_else(|| default_gradient_stops(grad_color));
                    let mut grad_choices: Vec<PreviewChoice> = vec![PreviewChoice {
                        label: text("_xlsx.no-gradient"),
                        content: { let l = text("_xlsx.no-gradient"); Rc::new(move || gradient_preview(None, l.clone())) },
                        choose: Rc::new(|this, _, cx| this.panel_change(StyleChange::Gradient(None), cx)),
                    }];
                    for kind in GRADIENTS {
                        let label = text(kind.label);
                        let stops = grad_stops.clone();
                        grad_choices.push(PreviewChoice {
                            label: label.clone(),
                            content: Rc::new(move || gradient_preview(Some(gradient_with(kind.shape, stops.clone())), label.clone())),
                            choose: Rc::new(move |this, _, cx| {
                                let st = this.panel_style();
                                let stops = st.gradient.as_ref().map(|g| gradient_stops(g).clone()).unwrap_or_else(|| default_gradient_stops(gradient_base(st.fill)));
                                this.panel_change(StyleChange::Gradient(Some(gradient_with(kind.shape, stops))), cx);
                            }),
                        });
                    }
                    let grad_selected = match &style.gradient {
                        None => 0,
                        Some(g) => GRADIENTS.iter().position(|k| k.shape.matches(g)).map_or(0, |i| i + 1),
                    };
                    section = section
                        .child(color_row(text("_xlsx.background-color"), pane.marked("cell-format-fill",crate::shell::chrome::panel_color_field("cell-format-fill-input",&panel.fill_color,cx))))
                        .child(control_row(text("xlsx.fill-pattern"), pane.preview("cell-format-pattern", pat_choices, pat_selected, cx)))
                        .child(color_row(text("xlsx.pattern-color"), pane.marked("cell-format-pattern-color",crate::shell::chrome::panel_color_field("cell-format-pattern-color-input",&panel.pattern_color,cx))))
                        .child(control_row(text("xlsx.fill-gradient"), pane.preview("cell-format-gradient", grad_choices, grad_selected, cx)));
                    // A gradient chosen: its stops are adjusted under the list, on the shared gradient slider (press the bar to add a
                    // stop, drag a knob to move it), with the colour and position of the selected stop and a button to remove it.
                    if let Some(gradient) = &style.gradient {
                        let stops = gradient_stops(gradient);
                        let selected = panel.grad_selected.min(stops.len().saturating_sub(1));
                        let points: Vec<ibis_doc_model::GradientStop> = stops.iter().map(|&(pos, c)| ibis_doc_model::GradientStop { position: pos as u32 * 100, color: ibis_doc_model::Color::opaque(c) }).collect();
                        section = section
                            .child(pane.marked("cell-format-gradient-slider", gradient_slider(points, selected, true, panel.grad_slider.clone(), cx,
                                |this: &mut SlideEditor, points, selected, _, cx| this.panel_slider_changed(points, selected, cx))))
                            .child(color_row(text("xlsx.format-pane.stop-color"), pane.marked("cell-format-gradient-stop-color", crate::shell::chrome::panel_color_field("cell-format-gradient-stop-color-input", &panel.grad_color, cx))))
                            .child(text_field_row(text("xlsx.format-pane.stop-position"), crate::shell::chrome::text_field(&panel.grad_pos).unit("%", px(20.)).spinner(
                                "cell-format-gradient-stop-spin",
                                crate::shell::chrome::SpinnerSpec::new(1.0).range(0.0, 100.0),
                                cx,
                                |this, percent, _, cx| this.panel_edit_gradient_stop(cx, |s| s.0 = (percent * 10.0).round() as u16),
                            )))
                            .child(button_group([ButtonGroupItem::new("cell-format-gradient-remove-stop", text("xlsx.format-pane.remove-stop"))
                                .disabled(stops.len() <= 2)
                                .on_click(cx.listener(|this, _, _, cx| this.panel_remove_gradient_stop(cx)))]));
                    }
                }
                FormatTab::Protection => {
                    section = section
                        .child(change_check("cell-format-locked".into(), text("_xlsx.locked"), !style.unlocked, StyleChange::Locked(style.unlocked), cx))
                        .child(change_check("cell-format-hidden".into(), text("_xlsx.hidden-hide-the-formula"), style.formula_hidden, StyleChange::FormulaHidden(!style.formula_hidden), cx))
                        .child(div().text_sm().child(text("_xlsx.locked-and-hidden-take-effect-when-the-sheet-is")));
                }
            }
            body = body.child(section);
        }
        let labels = FormatTab::HEADER_LABELS.iter().map(|k| text(k)).collect::<Vec<_>>();
        let (title, body) = ("fix.protection.format-cells", body.into_any_element());
        // リアルタイム更新 off: 適用 in the action bar area (the same one the dialogs have), primary, off while nothing waits.
        let panel = if live {
            SidebarPanel { title: title.into(), body }
        } else {
            let apply = ActionDefinition::new("cell-format-apply", text("_shell.apply"), ActionButtonKind::Primary)
                .disabled(draft_empty)
                .on_click(cx.listener(|this, _, _, cx| this.panel_apply(cx)));
            SidebarPanel::with_footer(title, body, crate::shell::chrome::action_bar().action(apply))
        };
        Some(panel.header_tabs("cell-format-tabs", labels, current_tab, |e, i, cx| { e.wb_mut().format_tab = i; cx.notify(); }, cx))
    }

    /// The border lines' sample (Excel's 罫線 tab): the presets, and the cell with its lines, which are drawn in
    /// the style and the colour they have; pressing a line (or its button) draws it or takes it off.
    fn border_sample(&self, pane: &Pane, multi_rows: bool, multi_columns: bool, cx: &mut Context<Self>) -> AnyElement {
        let text = |s: &str| SharedString::from(crate::shell::i18n::text(s).to_string());
        let (left, top, right, bottom) = (MARGIN, MARGIN, MARGIN + SAMPLE_W, MARGIN + SAMPLE_H);
        let (mid_x, mid_y) = ((left + right) / 2.0, (top + bottom) / 2.0);
        let preset = |id: &'static str, icon: &'static str, tip: &'static str, kind: &'static str, enabled: bool| -> AnyElement {
            pane.marked(id, Button::new(SharedString::from(id)).ghost().small().icon(Icon::new(Material(icon))).label(text(tip)).disabled(!enabled).on_click(cx.listener(move |this, _, _, cx| this.set_borders(kind, cx))))
        };
        // The line buttons of the sides, as Excel's.
        let side = |kind: &'static str, icon: Option<&'static str>, glyph: &'static str, tip: &'static str, enabled: bool| -> AnyElement {
            let id = format!("cell-format-border-{kind}");
            let on = self.range_edge(kind).is_some();
            let button = Button::new(SharedString::from(id.clone())).ghost().small().selected(on).disabled(!enabled).tooltip(text(tip));
            let button = match icon {
                Some(i) => button.icon(Icon::new(Material(i))),
                None => button.label(glyph),
            };
            pane.marked(&id, button.on_click(cx.listener(move |this, _, _, cx| this.toggle_border(kind, cx))))
        };
        let mut sample = div().relative().w(px(SAMPLE_W + 2.0 * MARGIN)).h(px(SAMPLE_H + 2.0 * MARGIN)).flex_none().overflow_hidden().rounded(px(4.)).border_1().border_color(gpui_kit::rgb(0xc7c7c7)).bg(gpui_kit::rgb(0xffffff));
        // The cell, with the guides of where the inside lines go.
        sample = sample.child(div().absolute().left(px(left)).top(px(top)).w(px(SAMPLE_W)).h(px(SAMPLE_H)).bg(gpui_kit::rgb(0xf6f6f6)));
        let guide = Border { line: BorderLine::Hair, color: ibis_doc_model::Rgb(0xbfbfbf) };
        for (from, to, shown) in [((left, mid_y), (right, mid_y), multi_rows), ((mid_x, top), (mid_x, bottom), multi_columns)] {
            if shown {
                for piece in sample_line(from, to, guide) {
                    sample = sample.child(piece);
                }
            }
        }
        let lines = [
            ("top", (left, top), (right, top)),
            ("bottom", (left, bottom), (right, bottom)),
            ("left", (left, top), (left, bottom)),
            ("right", (right, top), (right, bottom)),
            ("inside-h", (left, mid_y), (right, mid_y)),
            ("inside-v", (mid_x, top), (mid_x, bottom)),
            ("diagonal-up", (left, bottom), (right, top)),
            ("diagonal-down", (left, top), (right, bottom)),
        ];
        for (kind, from, to) in lines {
            if let Some(border) = self.range_edge(kind) {
                for piece in sample_line(from, to, border) {
                    sample = sample.child(piece);
                }
            }
        }
        // The places to press: strips over the sides and the middle.
        let zone = |kind: &'static str, x: f32, y: f32, w: f32, h: f32| -> AnyElement {
            let id = format!("cell-format-sample-{kind}");
            let ui = pane.ui.clone();
            let key = id.clone();
            div()
                .id(SharedString::from(id))
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(w))
                .h(px(h))
                .cursor_pointer()
                .hover(|s| s.bg(hsla(0.6, 0.8, 0.5, 0.18)))
                .on_click(cx.listener(move |this, _, _, cx| this.toggle_border(kind, cx)))
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| { ui.borrow_mut().insert(key.clone(), bounds); }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let strip = 8.0;
        sample = sample
            .child(zone("top", left, top - strip / 2.0, SAMPLE_W, strip))
            .child(zone("bottom", left, bottom - strip / 2.0, SAMPLE_W, strip))
            .child(zone("left", left - strip / 2.0, top, strip, SAMPLE_H))
            .child(zone("right", right - strip / 2.0, top, strip, SAMPLE_H));
        if multi_rows {
            sample = sample.child(zone("inside-h", left, mid_y - strip / 2.0, SAMPLE_W, strip));
        }
        if multi_columns {
            sample = sample.child(zone("inside-v", mid_x - strip / 2.0, top, strip, SAMPLE_H));
        }
        v_flex()
            .gap_2()
            .child(h_flex().gap_1().flex_wrap()
                .child(preset("cell-format-border-none", "border_clear", "_shell.none", "none", true))
                .child(preset("cell-format-border-outside", "border_outer", "_xlsx.outside-borders", "outside", true))
                .child(preset("cell-format-border-inside", "border_inner", "xlsx.format-pane.border-inside", "inside", multi_rows || multi_columns)))
            .child(h_flex().gap_1()
                .child(v_flex().gap_1().justify_between()
                    .child(side("top", Some("border_top"), "", "_xlsx.top-border-2", true))
                    .child(side("inside-h", Some("border_horizontal"), "", "_xlsx.horizontal-border", multi_rows))
                    .child(side("bottom", Some("border_bottom"), "", "_xlsx.bottom-border-2", true)))
                .child(sample))
            .child(h_flex().gap_1()
                .child(side("left", Some("border_left"), "", "_xlsx.left-border-2", true))
                .child(side("inside-v", Some("border_vertical"), "", "_xlsx.vertical-border", multi_columns))
                .child(side("right", Some("border_right"), "", "_xlsx.right-border-2", true))
                .child(side("diagonal-up", None, "╱", "_xlsx.diagonal-up-border-2", true))
                .child(side("diagonal-down", None, "╲", "_xlsx.diagonal-down-border-2", true)))
            .child(div().text_size(px(12.)).text_color(crate::shell::chrome::text_secondary()).child(text("xlsx.format-pane.border-hint")))
            .into_any_element()
    }
}

/// The name of a border line style in the UI's language.
pub(crate) fn border_line_label(line: BorderLine) -> SharedString {
    let id = match line {
        BorderLine::Thin => "_xlsx.thin",
        BorderLine::Hair => "_xlsx.hair",
        BorderLine::Medium => "_xlsx.medium",
        BorderLine::Thick => "_shell.thick",
        BorderLine::Double => "_shell.double",
        BorderLine::Dotted => "_shell.dotted",
        BorderLine::Dashed => "_shell.dashed",
        BorderLine::DashDot => "docx.picture.border.dash.dashdot",
        BorderLine::DashDotDot => "_xlsx.dash-dot-dot",
        BorderLine::MediumDashed => "_xlsx.medium-dashed",
        BorderLine::MediumDashDot => "_xlsx.medium-dash-dot-2",
        BorderLine::MediumDashDotDot => "_xlsx.medium-dash-dot-dot-2",
        BorderLine::SlantDashDot => "_xlsx.slant-dash-dot-2",
    };
    crate::shell::i18n::text(id)
}
