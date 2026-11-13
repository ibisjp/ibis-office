// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The 図形の書式 pane, ONE component for the document, spreadsheet and presentation screens (K0029, 2026-11-13).
//!
//! The document screen's pane (tabs 基本・装飾・文字) is the reference. A screen gives this module an adapter: the fields it keeps
//! (`ShapeFormatFields`), what it shows now (`ShapeFormatSpec`), and a few function pointers (`ShapeFormatHooks`) that say where
//! the dash menu's state is, and what to run after a field or the dash changed. The module owns the tabs, the rows, the colour
//! palettes, the line dash list and the spinners; reading the shape and writing the change (each format's own commands, and its
//! saving: WordprocessingML for the document, DrawingML for the sheet and the slide) stays with the screen.
use crate::chrome::{self, SidebarPanel, SpinnerSpec};
use crate::editor::SlideEditor;
use crate::effect_editor::{self, EffectEditor, EffectHooks, SECTIONS};
use crate::i18n::text;
use gpui_kit::component::{ActiveTheme as _, h_flex, input::InputState, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{LineDash, Rgb};

/// The outline dash patterns the pane offers.
pub const DASHES: [LineDash; 4] = [LineDash::Solid, LineDash::Dash, LineDash::Dot, LineDash::DashDot];

/// The line-style dropdown's state: the pattern chosen, whether its list is open, and the row the pointer is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DashUi {
    pub dash: LineDash,
    pub open: bool,
    pub hovered: Option<LineDash>,
}
impl DashUi {
    pub fn new(dash: LineDash) -> Self {
        Self { dash, open: false, hovered: None }
    }
}

/// The unit the size and place fields show (the document screen's are points, the sheet's and the slide's centimetres).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeUnit {
    Point,
    Centimetre,
}
impl SizeUnit {
    fn label(self) -> &'static str {
        match self {
            SizeUnit::Point => "pt",
            SizeUnit::Centimetre => "cm",
        }
    }
    fn size_spec(self) -> SpinnerSpec {
        match self {
            SizeUnit::Point => SpinnerSpec::new(1.0).range(1.0, 9999.0).integer_only(),
            SizeUnit::Centimetre => SpinnerSpec::new(0.1).min(0.01),
        }
    }
    fn place_spec(self) -> SpinnerSpec {
        match self {
            SizeUnit::Point => SpinnerSpec::new(1.0).range(-9999.0, 9999.0).integer_only(),
            SizeUnit::Centimetre => SpinnerSpec::new(0.1),
        }
    }
}

/// What the screen runs for the pane.
#[derive(Clone, Copy)]
pub struct ShapeFormatHooks {
    /// A tab was chosen (0 基本, 1 装飾, 2 文字).
    pub set_tab: fn(&mut SlideEditor, usize),
    /// Where the line-style dropdown's state is kept.
    pub dash_ui: fn(&mut SlideEditor) -> Option<&mut DashUi>,
    /// A field, a spinner or the dash changed: write what differs from what the shape has (one step of undo).
    pub apply: fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>),
}

/// The fields of the basic tab. Every one is an input the screen keeps (and subscribes to for the change).
#[derive(Clone)]
pub struct ShapeFormatFields {
    pub unit: SizeUnit,
    pub width: Entity<InputState>,
    pub height: Entity<InputState>,
    pub rotation: Option<Entity<InputState>>,
    pub left: Option<Entity<InputState>>,
    pub top: Option<Entity<InputState>>,
    /// `None` for a shape with no fill (a line, a picture).
    pub fill: Option<Entity<InputState>>,
    /// `None` for a shape with no outline to set (a picture).
    pub line: Option<Entity<InputState>>,
    pub line_width: Option<Entity<InputState>>,
    /// `None` for a shape that holds no text.
    pub font_size: Option<Entity<InputState>>,
    pub text_color: Option<Entity<InputState>>,
}

/// The decoration and text tabs (the shared effect editor) of the shape.
pub struct ShapeFormatEffects<'a> {
    pub editor: &'a EffectEditor,
    pub enabled: [bool; SECTIONS],
    pub hooks: EffectHooks,
    pub scroll: ScrollHandle,
}

/// What the pane shows now.
pub struct ShapeFormatSpec<'a> {
    /// A prefix of the element ids (`docx`, `pptx`, `xlsx`): the screens' ids stay apart.
    pub id: &'static str,
    /// The title when there are no tabs (a shape the decoration does not edit).
    pub plain_title_key: &'static str,
    pub tab: usize,
    pub fields: ShapeFormatFields,
    pub hooks: ShapeFormatHooks,
    /// The line-style dropdown; `None` hides it.
    pub dash: Option<DashUi>,
    pub effects: Option<ShapeFormatEffects<'a>>,
    /// Before the rows (the document's shape styles).
    pub lead: Vec<AnyElement>,
    /// After the size and place rows (the document's wrapping).
    pub after_place: Vec<AnyElement>,
    /// After the line style (a line's arrows).
    pub after_line: Vec<AnyElement>,
    /// At the end of the basic tab (the sheet's lock, placement and print rows).
    pub tail: Vec<AnyElement>,
}

const STD_COLORS: &[(&str, u32, &str)] = &[
    ("_shell.black", 0x000000, "tb.color.black"),
    ("_shell.dark-red", 0xc00000, "tb.color.dark-red"),
    ("_shell.red", 0xff0000, "tb.color.red"),
    ("_shell.orange", 0xffc000, "tb.color.orange"),
    ("_shell.yellow", 0xffff00, "tb.color.yellow"),
    ("_shell.light-green", 0x92d050, "tb.color.light-green"),
    ("_shell.green", 0x00b050, "tb.color.green"),
    ("_shell.light-blue", 0x00b0f0, "tb.color.light-blue"),
    ("_shell.blue", 0x0070c0, "tb.color.blue"),
    ("_shell.dark-blue", 0x002060, "tb.color.dark-blue"),
    ("_shell.purple", 0x7030a0, "tb.color.purple"),
];
const THEME_10: [Rgb; 10] = [
    Rgb(0xffffff),
    Rgb(0x000000),
    Rgb(0xe7e6e6),
    Rgb(0x44546a),
    Rgb(0x4472c4),
    Rgb(0xed7d31),
    Rgb(0xa5a5a5),
    Rgb(0xffc000),
    Rgb(0x5b9bd5),
    Rgb(0x70ad47),
];
const LINE_WIDTH_STEPS: &[f32] = &[0.25, 0.5, 0.75, 1.0, 1.5, 2.25, 3.0, 4.5, 6.0];
const FONT_STEPS: &[f32] = &[6.0, 7.0, 8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0];

fn dash_label(dash: LineDash) -> &'static str {
    match dash {
        LineDash::Solid => "docx.text-box-panel.line-solid",
        LineDash::Dash | LineDash::LgDash | LineDash::SysDash => "docx.text-box-panel.line-dash",
        LineDash::Dot | LineDash::SysDot => "docx.text-box-panel.line-dot",
        _ => "docx.text-box-panel.line-dash-dot",
    }
}

/// A picture of a dash pattern, `width_px` wide.
fn dash_graphic(dash: LineDash, width_px: f32, color: Hsla) -> AnyElement {
    let mut row = h_flex().w(px(width_px)).h(px(12.)).items_center().gap(px(3.));
    match dash {
        LineDash::Solid => {
            row = row.child(div().w(px(width_px)).h(px(2.)).bg(color));
        }
        LineDash::Dash | LineDash::LgDash | LineDash::SysDash => {
            let seg = 10.0;
            let count = ((width_px + 3.0) / (seg + 3.0)).floor().max(1.0) as usize;
            for _ in 0..count {
                row = row.child(div().w(px(seg)).h(px(2.)).bg(color));
            }
        }
        LineDash::Dot | LineDash::SysDot => {
            let seg = 3.0;
            let count = ((width_px + 3.0) / (seg + 3.0)).floor().max(1.0) as usize;
            for _ in 0..count {
                row = row.child(div().w(px(seg)).h(px(2.)).bg(color));
            }
        }
        _ => {
            let unit = 10.0 + 3.0 + 3.0 + 3.0;
            let count = ((width_px + 3.0) / unit).floor().max(1.0) as usize;
            for _ in 0..count {
                row = row.child(div().w(px(10.)).h(px(2.)).bg(color)).child(div().w(px(3.)).h(px(2.)).bg(color));
            }
        }
    }
    row.into_any_element()
}

/// A colour field of the pane: the code, a swatch and the shared palette (theme row, standard colours; `allow_none` adds 塗りつぶしなし).
/// A pick writes the field and sends its change, so the screen's subscription applies it.
pub fn shape_color_field(id: &'static str, input: &Entity<InputState>, allow_none: bool, cx: &Context<SlideEditor>) -> AnyElement {
    let parse = |input: &Entity<InputState>| {
        let v = input.read(cx).value();
        let s = v.trim().trim_start_matches('#');
        (s.len() == 6).then(|| u32::from_str_radix(s, 16).ok()).flatten().map(Rgb)
    };
    let current = parse(input);
    let field = input.clone();
    let pick: crate::color_palette::PickColor = std::rc::Rc::new(move |this, chosen, window, cx| {
        if let Some(c) = chosen {
            this.note_recent_color(c);
        }
        let value = chosen.map(|c| format!("{:06X}", c.0 & 0xffffff)).unwrap_or_default();
        field.update(cx, |s, cx| {
            s.set_value(value, window, cx);
            cx.emit(gpui_kit::component::input::InputEvent::Change);
        });
        cx.notify();
    });
    let none_item = allow_none.then(|| {
        let field = input.clone();
        chrome::MenuItem::new("tb.color.none", "_shell.no-fill").checked(current.is_none()).on(move |_, window, cx| {
            field.update(cx, |s, cx| {
                s.set_value(String::new(), window, cx);
                cx.emit(gpui_kit::component::input::InputEvent::Change);
            });
            cx.notify();
        })
    });
    let std_list: Vec<(SharedString, Rgb, &'static str)> = STD_COLORS.iter().map(|&(k, c, id)| (text(k), Rgb(c), id)).collect();
    let entries = crate::color_palette::simple_palette(none_item, Some(&THEME_10), &std_list, current, pick);
    div().w(chrome::FIELD_WIDTH).child(chrome::color_text_field(id, input, current, entries, &cx.weak_entity())).into_any_element()
}

fn number_row(label: &'static str, input: &Entity<InputState>, unit: &'static str, id: SharedString, spec: SpinnerSpec, hooks: ShapeFormatHooks, cx: &Context<SlideEditor>) -> crate::chrome::control_row::ControlRow {
    chrome::text_field_row(text(label), chrome::text_field(input).unit(unit, px(22.)).spinner(id, spec, cx, move |this, _, window, cx| (hooks.apply)(this, window, cx)))
}

/// The line-style dropdown (a picture of the pattern; the list opens under it, each row a picture, the name at the hovered one).
fn dash_dropdown(prefix: &'static str, ui: DashUi, hooks: ShapeFormatHooks, cx: &Context<SlideEditor>) -> AnyElement {
    let theme = cx.theme();
    let (fg, border_col, popover_bg, accent_bg) = (theme.foreground, theme.border, theme.popover, theme.accent);
    let width = chrome::FIELD_WIDTH + px(30.);
    let mut wrap = div().relative().w(width);
    let button = h_flex()
        .id(SharedString::from(format!("{prefix}-line-dash")))
        .w(width)
        .h(px(28.))
        .px_2()
        .items_center()
        .justify_between()
        .rounded(px(4.))
        .border_1()
        .border_color(border_col)
        .bg(popover_bg)
        .cursor_pointer()
        .child(dash_graphic(ui.dash, 84.0, fg))
        .child(gpui_kit::component::Icon::new(crate::icons::Material("arrow_drop_down")).size(px(16.)).text_color(fg))
        .when(!ui.open, |el| {
            el.on_click(cx.listener(move |this, _, _, cx| {
                if let Some(ui) = (hooks.dash_ui)(this) {
                    ui.open = true;
                    ui.hovered = None;
                }
                cx.notify();
            }))
        });
    wrap = wrap.child(button);
    if ui.open {
        let mut list = v_flex()
            .id(SharedString::from(format!("{prefix}-line-dash-list")))
            .w(width)
            .py_1()
            .rounded(px(4.))
            .border_1()
            .border_color(border_col)
            .bg(popover_bg)
            .shadow_md()
            .occlude()
            .on_mouse_down_out(cx.listener(move |this, _, _, cx| {
                gpui_kit::component::popup_guard::note_outside_dismiss();
                if let Some(ui) = (hooks.dash_ui)(this) {
                    ui.open = false;
                    ui.hovered = None;
                }
                cx.notify();
            }));
        for (index, d) in DASHES.into_iter().enumerate() {
            let (selected, hovered) = (ui.dash == d, ui.hovered == Some(d));
            let mut row = h_flex()
                .id((SharedString::from(format!("{prefix}-line-dash-opt")), index))
                .relative()
                .w_full()
                .h(px(26.))
                .px_2()
                .items_center()
                .cursor_pointer()
                .when(selected || hovered, |el| el.bg(accent_bg))
                .child(dash_graphic(d, 96.0, fg))
                .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                    if let Some(ui) = (hooks.dash_ui)(this) {
                        if *over {
                            ui.hovered = Some(d);
                        } else if ui.hovered == Some(d) {
                            ui.hovered = None;
                        }
                    }
                    cx.notify();
                }))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(ui) = (hooks.dash_ui)(this) {
                        ui.dash = d;
                        ui.open = false;
                        ui.hovered = None;
                    }
                    (hooks.apply)(this, window, cx);
                }));
            if hovered {
                row = row.child(
                    div()
                        .absolute()
                        .right(px(128.))
                        .top(px(2.))
                        .px_2()
                        .py(px(2.))
                        .rounded(px(4.))
                        .border_1()
                        .border_color(border_col)
                        .bg(popover_bg)
                        .shadow_sm()
                        .text_sm()
                        .whitespace_nowrap()
                        .child(text(dash_label(d))),
                );
            }
            list = list.child(row);
        }
        wrap = wrap.child(deferred(div().absolute().top(px(30.)).left_0().child(list)).with_priority(10));
    }
    wrap.into_any_element()
}

#[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
thread_local! {
    /// The id prefix (`docx`, `pptx`, `xlsx`) and the tab of the pane built last: the scenarios check that every screen builds this pane.
    pub static LAST_BUILT: std::cell::RefCell<Option<(&'static str, usize, bool)>> = const { std::cell::RefCell::new(None) };
}

/// The pane: the title, the three tabs (基本・装飾・文字) in the header, and the tab's body. Without effects (a shape the decoration
/// does not edit) the basic tab shows alone under `plain_title_key`.
pub fn shape_format_panel(spec: ShapeFormatSpec<'_>, cx: &Context<SlideEditor>) -> SidebarPanel {
    let ShapeFormatSpec { id, plain_title_key, tab, fields, hooks, dash, effects, lead, after_place, after_line, tail } = spec;
    let tab = if effects.is_some() { tab.min(2) } else { 0 };
    #[cfg(all(feature = "scenarios", any(target_os = "macos", target_os = "linux")))]
    LAST_BUILT.with(|l| *l.borrow_mut() = Some((id, tab, effects.is_some())));
    let body = match (&effects, tab) {
        (Some(e), 1) => v_flex().gap_2().child(effect_editor::panel(e.editor, &e.enabled, &[0, 1, 2, 3, 4], e.hooks, e.scroll.clone(), cx)),
        (Some(e), 2) => {
            let mut body = v_flex().gap_2();
            if let Some(color) = &fields.text_color {
                body = body.child(chrome::color_field_row(text("pptx.sidebar.text-fill"), shape_color_field(leak_id(id, "text-fill"), color, false, cx)));
            }
            body.child(effect_editor::panel(e.editor, &e.enabled, &[5, 6, 7], e.hooks, e.scroll.clone(), cx))
        }
        _ => basic_tab(id, &fields, hooks, dash, lead, after_place, after_line, tail, cx),
    };
    let panel = SidebarPanel { title: text(if effects.is_some() { "shape.effects.panel-title" } else { plain_title_key }), body: body.into_any_element() };
    if effects.is_none() {
        return panel;
    }
    let set_tab = hooks.set_tab;
    panel.header_tabs(leak_id(id, "tabs"), [text("shape.effects.basic"), text("shape.effects.decoration"), text("shape.effects.text")], tab, move |e, index, cx| {
        set_tab(e, index);
        cx.notify();
    }, cx)
}

/// An element id of the pane for `prefix` (`docx`, `pptx`, `xlsx`): the few prefixes are interned once.
fn leak_id(prefix: &'static str, name: &'static str) -> &'static str {
    use std::{collections::HashMap, sync::Mutex};
    static IDS: Mutex<Option<HashMap<(&'static str, &'static str), &'static str>>> = Mutex::new(None);
    let mut guard = IDS.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    *map.entry((prefix, name)).or_insert_with(|| Box::leak(format!("{prefix}-shape-format-{name}").into_boxed_str()))
}

#[allow(clippy::too_many_arguments)]
fn basic_tab(id: &'static str, f: &ShapeFormatFields, hooks: ShapeFormatHooks, dash: Option<DashUi>, lead: Vec<AnyElement>, after_place: Vec<AnyElement>, after_line: Vec<AnyElement>, tail: Vec<AnyElement>, cx: &Context<SlideEditor>) -> gpui_kit::Div {
    let unit = f.unit.label();
    let sid = |name: &'static str| SharedString::from(leak_id(id, name));
    let mut body = v_flex().gap_2();
    for el in lead {
        body = body.child(el);
    }
    body = body
        .child(number_row("docx.picture-panel.width-pt", &f.width, unit, sid("width"), f.unit.size_spec(), hooks, cx))
        .child(number_row("docx.picture-panel.height-pt", &f.height, unit, sid("height"), f.unit.size_spec(), hooks, cx));
    if let Some(rotation) = &f.rotation {
        body = body.child(number_row("_shell.rotate", rotation, "°", sid("rotation"), SpinnerSpec::new(1.0).range(-360.0, 360.0), hooks, cx));
    }
    if let Some(left) = &f.left {
        body = body.child(number_row("docx.text-box-panel.left-pt", left, unit, sid("left"), f.unit.place_spec(), hooks, cx));
    }
    if let Some(top) = &f.top {
        body = body.child(number_row("docx.text-box-panel.top-pt", top, unit, sid("top"), f.unit.place_spec(), hooks, cx));
    }
    for el in after_place {
        body = body.child(el);
    }
    if let Some(fill) = &f.fill {
        body = body.child(chrome::color_field_row(text("docx.text-box-panel.fill-color"), shape_color_field(leak_id(id, "fill"), fill, true, cx)));
    }
    if let Some(line) = &f.line {
        body = body.child(chrome::color_field_row(text("docx.text-box-panel.outline-color"), shape_color_field(leak_id(id, "line"), line, true, cx)));
    }
    if let Some(width) = &f.line_width {
        body = body.child(number_row("docx.text-box-panel.line-width-pt", width, "pt", sid("line-width"), SpinnerSpec::new(0.5).steps(LINE_WIDTH_STEPS).range(0.0, 100.0), hooks, cx));
    }
    if let Some(ui) = dash {
        body = body.child(chrome::control_row(text("docx.text-box-panel.line-style"), dash_dropdown(leak_id(id, "dash"), ui, hooks, cx)));
    }
    for el in after_line {
        body = body.child(el);
    }
    if let Some(size) = &f.font_size {
        body = body.child(number_row("docx.text-box-panel.font-size", size, "pt", sid("font-size"), SpinnerSpec::new(1.0).steps(FONT_STEPS).range(1.0, 1638.0), hooks, cx));
    }
    if let Some(color) = &f.text_color {
        body = body.child(chrome::color_field_row(text("docx.text-box-panel.text-color"), shape_color_field(leak_id(id, "text-color"), color, false, cx)));
    }
    for el in tail {
        body = body.child(el);
    }
    body
}

/// The id (and the words' key) of 図形の書式設定…, the first item of the context menu of a shape, a picture or a text box on the three
/// screens (K0027). It opens this pane.
pub const SHAPE_FORMAT_ITEM: &str = "pptx.format-shape";

/// The context menu's first item, 図形の書式設定…: `open` shows the pane (each screen's own way: the document's pane for the shape
/// at the caret, the sheet's for the selected shape, the slide's sidebar).
pub fn shape_format_menu_item(open: fn(&mut SlideEditor, &mut Window, &mut Context<SlideEditor>)) -> chrome::MenuEntry {
    chrome::MenuItem::tr(SHAPE_FORMAT_ITEM).icon("format_shapes").on(move |e, w, cx| open(e, w, cx)).into()
}
