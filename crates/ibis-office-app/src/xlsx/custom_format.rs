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
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{Disableable as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::{Border, BorderLine, CellAlign, CellVerticalAlign, StyleChange};

use crate::shell::chrome::{Handler, MenuEntry, MenuItem, SidebarPanel, ToolButton, ToolItem, fill_menu, tool_item};
use crate::shell::editor::SlideEditor;
use crate::shell::icons::Material;

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

    fn label(self) -> &'static str {
        match self {
            Self::Number => "_xlsx.number-2",
            Self::Alignment => "_xlsx.alignment",
            Self::Font => "_shell.font",
            Self::Border => "pptx.custom-format.borders",
            Self::Fill => "pptx.common.fill",
            Self::Protection => "_xlsx.protection",
        }
    }
}

/// Where the parts are (set when painted), for the scenarios.
pub(crate) type PaneBounds = Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>;
/// The items of the drop-downs by the id of the drop-down, for the scenarios.
pub(crate) type PaneMenus = Rc<RefCell<BTreeMap<String, Vec<MenuEntry>>>>;

pub(crate) struct FormatPanel {
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

    /// A drop-down: the choice now made (with a rounded square of its colour when `chip` has one) and, when
    /// pressed, `menu`.
    fn select(&self, id: &'static str, chip: Option<Hsla>, label: SharedString, menu: Vec<MenuEntry>) -> AnyElement {
        self.menus.borrow_mut().insert(id.to_string(), menu.clone());
        let button=crate::shell::chrome::dropdown_button(id,
            h_flex().w_full().gap(px(6.)).items_center().when_some(chip,|d,color|d.child(color_chip(color)))
                .child(div().flex_1().truncate().text_size(px(13.)).child(label)).into_any_element());
        let button=if menu.iter().any(|e|matches!(e,MenuEntry::Palette(_))) {
            crate::shell::chrome::ColorPaletteDropdown::from_palette(id,button,menu,&self.editor).into_any_element()
        }else{crate::shell::chrome::Dropdown::with_button(id,button,menu,true,0,&self.editor).into_any_element()};
        self.marked(id, button)
    }
}

/// A rounded square of `color`, such as a drop-down's left end shows.
fn color_chip(color: Hsla) -> Div {
    div().w(px(16.)).h(px(12.)).flex_none().rounded(px(3.)).bg(color).border_1().border_color(gpui_kit::rgb(0xc7c7c7))
}

fn rgb_hsla(c: ibis_doc_model::Rgb) -> Hsla {
    gpui_kit::rgb(c.0).into()
}

/// The name of colour `c` among `names` (the toolbar's), else `#RRGGBB`.
fn color_name(c: ibis_doc_model::Rgb, names: &[(&str, u32)]) -> SharedString {
    match names.iter().find(|(_, v)| *v == c.0) {
        Some((name, _)) => crate::shell::i18n::text(name),
        None => format!("#{:06X}", c.0).into(),
    }
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

/// The sample's size and the margin around the cell in it.
const SAMPLE_W: f32 = 168.0;
const SAMPLE_H: f32 = 96.0;
const MARGIN: f32 = 10.0;

impl SlideEditor {
    /// セルの書式設定 (⌘1): the sections open as they were left.
    pub(crate) fn open_custom_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("#,##0"));
        code.update(cx, |i, cx| {
            i.set_value(style.number_format.clone(), window, cx);
            if focus && tab == FormatTab::Number {
                i.focus(window, cx);
            }
        });
        let font = cx.new(|cx| InputState::new(window, cx).placeholder("游ゴシック")); // i18n-ok（フォント名の見本（入力欄の placeholder）。どの言語でも同じ）
        font.update(cx, |i, cx| i.set_value(style.font.clone().unwrap_or_default(), window, cx));
        let size = cx.new(|cx| InputState::new(window, cx).placeholder("11"));
        size.update(cx, |i, cx| i.set_value(format!("{}", style.font_size as f32 / 100.0), window, cx));
        let rotation = cx.new(|cx| InputState::new(window, cx).placeholder("0"));
        let rot_text = match style.rotation {
            0..=90 => format!("{}", style.rotation),
            91..=180 => format!("-{}", style.rotation as i16 - 90),
            _ => String::new(),
        };
        rotation.update(cx, |i, cx| i.set_value(rot_text, window, cx));
        let indent=cx.new(|cx|InputState::new(window,cx));
        indent.update(cx,|i,cx|i.set_value(style.indent.to_string(),window,cx));
        let on_indent=cx.subscribe_in(&indent,window,|e,i,event:&InputEvent,_,cx|{if matches!(event,InputEvent::Change|InputEvent::PressEnter{..}){if let Ok(n)=i.read(cx).value().parse::<u32>(){e.format_cells(StyleChange::Indent(n.min(250)),cx);}}});
        let mut color_inputs=Vec::new();
        let mut color_subscriptions=Vec::new();
        for (kind,value) in [(0,Some(style.color)),(1,style.fill),(2,style.pattern.as_ref().map(|p|p.1))] {
            let input=cx.new(|cx|InputState::new(window,cx));
            input.update(cx,|i,cx|i.set_value(value.map(|c|format!("{:06X}",c.0)).unwrap_or_default(),window,cx));
            color_subscriptions.push(cx.subscribe_in(&input,window,move|this,input,event:&InputEvent,_,cx|{
                if !matches!(event,InputEvent::Change|InputEvent::PressEnter{..}) {return;}
                let value=input.read(cx).value().trim().trim_start_matches('#').to_string();
                let color=if value.is_empty(){None}else{u32::from_str_radix(&value,16).ok().filter(|_|value.len()==6).map(ibis_doc_model::Rgb)};
                if !value.is_empty()&&color.is_none(){return;}
                let change=match kind {0=>StyleChange::Color(color.unwrap_or(ibis_doc_model::Rgb::BLACK)),1=>StyleChange::Fill(color),_=>StyleChange::PatternColor(color.unwrap_or(ibis_doc_model::Rgb::BLACK))};
                this.format_cells(change,cx);
            }));
            color_inputs.push(input);
        }
        let font_color=color_inputs.remove(0);let fill_color=color_inputs.remove(0);let pattern_color=color_inputs.remove(0);
        // Enter in the size, the name, or the rotation angle applies it.
        let on_size = cx.subscribe_in(&size, window, |this: &mut Self, input, event: &InputEvent, _, cx| {
            if matches!(event,InputEvent::Change|InputEvent::PressEnter { .. }) {
                let text = input.read(cx).value().trim().to_string();
                this.apply_pane_size(&text, cx);
            }
        });
        let on_font = cx.subscribe_in(&font, window, |this: &mut Self, input, event: &InputEvent, _, cx| {
            if let InputEvent::PressEnter { .. } = event {
                let name = input.read(cx).value().trim().to_string();
                this.format_cells(StyleChange::Font((!name.is_empty()).then_some(name)), cx);
            }
        });
        let on_rotation = cx.subscribe_in(&rotation, window, |this: &mut Self, input, event: &InputEvent, _, cx| {
            if matches!(event,InputEvent::Change|InputEvent::PressEnter { .. }) {
                let text = input.read(cx).value().trim().to_string();
                this.apply_pane_rotation(&text, cx);
            }
        });
        if let Some(s) = self.sheet_state_mut() {
            s.close_settings();
            if show {
                s.format_open[tab as usize] = true;
            }
            s.custom_format = Some(FormatPanel { custom_selected:!super::toolbar::NUMBER_FORMATS.iter().any(|(_,f)|*f==style.number_format),code, font, size, rotation, indent, font_color,fill_color,pattern_color,ui: Default::default(), menus: Default::default(), _subscriptions: {color_subscriptions.extend([on_size,on_font,on_rotation,on_indent]);color_subscriptions} });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The size (点) typed in the pane, applied to the selected cells.
    fn apply_pane_size(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Ok(points) = text.parse::<f32>()
            && (1.0..=409.0).contains(&points)
        {
            self.format_cells(StyleChange::FontSize((points * 100.0).round() as u32), cx);
        }
    }

    /// The rotation angle (度, `-90..=90` or OOXML `0..=180`) typed in the pane, applied to the selected cells.
    pub(crate) fn apply_pane_rotation(&mut self, text: &str, cx: &mut Context<Self>) {
        let trimmed = text.trim().trim_end_matches('°').trim_end_matches('度').trim();
        if let Ok(deg) = trimmed.parse::<i16>() {
            let rot = if (0..=180).contains(&deg) {
                Some(deg as u8)
            } else if (-90..=-1).contains(&deg) {
                Some((90 - deg) as u8)
            } else {
                None
            };
            if let Some(r) = rot {
                self.format_cells(StyleChange::Rotation(r), cx);
            }
        }
    }

    pub(crate) fn custom_format_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.sheet_state()?.custom_format.as_ref()?;
        let (code, font, size, rotation) = (panel.code.clone(), panel.font.clone(), panel.size.clone(), panel.rotation.clone());
        let indent=panel.indent.clone();
        let custom_selected=panel.custom_selected;
        let pane = Pane { ui: panel.ui.clone(), menus: panel.menus.clone(), editor: cx.entity().downgrade() };
        let open = self.wb().format_open;
        pane.menus.borrow_mut().clear();
        let style = self.current_style();
        let text = |s: &str| SharedString::from(crate::shell::i18n::text(s).to_string());
        let caption = |s: &str| div().text_sm().text_color(crate::shell::chrome::text_secondary()).child(text(s));
        // A button that changes the selected cells' format.
        let change = |id: String, label: SharedString, change: StyleChange, cx: &mut Context<Self>| {
            pane.marked(&id.clone(), crate::shell::chrome::action_button(SharedString::from(id)).small().label(label).on_click(cx.listener(move |this, _, _, cx| this.format_cells(change.clone(), cx))))
        };
        let change_check = |id: String, label: SharedString, checked: bool, change: StyleChange, cx: &mut Context<Self>| {
            pane.marked(&id.clone(), crate::shell::chrome::checkbox_button(SharedString::from(id), label, checked, false,
                cx.listener(move |this, _, _, cx| this.format_cells(change.clone(), cx))))
        };
        // A menu item that changes the selected cells' format.
        let choice = |id: &'static str, label: SharedString, on: bool, change: StyleChange| -> MenuEntry {
            MenuItem::new(id, label).checked(on).on(move |this, _, cx| this.format_cells(change.clone(), cx)).into()
        };

        let mut body = v_flex().gap_1().px(px(10.)).py(px(8.)).p_0();
        for (n, tab) in FormatTab::ALL.into_iter().enumerate() {
            let is_open = open[n];
            body = body.child(self.pane_section_header(&pane, n, tab, is_open, cx));
            if !is_open {
                continue;
            }
            let mut section = v_flex().gap_2().px(px(6.)).pt(px(4.)).pb(px(10.));
            match tab {
                FormatTab::Number => {
                    let typed = code.read(cx).value().to_string();
                    // What the selected cell shows in it.
                    let sample = self.current_cell().and_then(|(c, r)| self.current_sheet().and_then(|(_, s)| s.cell(r, c).map(|x| x.value.clone()))).map(|v| ibis_calc::numfmt::format(&v, &typed, false).text).unwrap_or_default();
                    let c2 = code.clone();
                    let current = super::toolbar::NUMBER_FORMATS.iter().find(|(_, f)| *f == style.number_format).map(|(name, _)| text(name)).unwrap_or_else(|| text("xlsx.format-pane.custom"));
                    let mut items:Vec<MenuEntry>=super::toolbar::NUMBER_FORMATS.iter().map(|&(name,format)|MenuItem::new("xlsx.number-format",text(name)).checked(!custom_selected&&style.number_format==format).on(move |e,_,cx|{if let Some(p)=e.wb_mut().custom_format.as_mut(){p.custom_selected=false;}e.format_cells(StyleChange::NumberFormat(format.into()),cx);}).into()).collect();
                    items.push(MenuItem::new("xlsx.number-format.custom",text("xlsx.format-pane.custom")).checked(custom_selected).on(|e,_,cx|{if let Some(p)=e.wb_mut().custom_format.as_mut(){p.custom_selected=true;}cx.notify();}).into());
                    section = section
                        .child(pane.select("cell-format-number", None, if custom_selected{text("xlsx.format-pane.custom")}else{current}, items))
                        .child(caption("_xlsx.number-format-custom"))
                        .child(crate::shell::chrome::text_field(&code).full_width().disabled(!custom_selected))
                        .child(div().text_sm().child(SharedString::from(format!("{}{sample}", crate::shell::i18n::text("_xlsx.sample")))))
                        .child(crate::shell::chrome::action_button("custom-format-apply").small().label(text("_xlsx.apply-to-selection")).on_click(cx.listener(move |this, _, _, cx| {
                            let code = c2.read(cx).value().trim().to_string();
                            if !code.is_empty() {
                                this.format_cells(StyleChange::NumberFormat(code), cx);
                            }
                        })));
                }
                FormatTab::Alignment => {
                    let horizontal = [("_xlsx.general", CellAlign::General), ("_xlsx.left-indent", CellAlign::Left), ("_shell.center-2", CellAlign::Center), ("_xlsx.right-indent", CellAlign::Right), ("_xlsx.fill", CellAlign::Fill), ("align.justify", CellAlign::Justify), ("_xlsx.center-across-selection", CellAlign::CenterAcross), ("_xlsx.distributed-indent", CellAlign::Distributed)];
                    let vertical = [("_xlsx.top-2", CellVerticalAlign::Top), ("_shell.center-2", CellVerticalAlign::Center), ("_xlsx.bottom-2", CellVerticalAlign::Bottom), ("align.justify", CellVerticalAlign::Justify), ("_shell.distributed", CellVerticalAlign::Distributed)];
                    let turns = [("0°", 0u8), ("45°", 45), ("90°", 90), ("135°", 135), ("180°", 180), ("_xlsx.vertical-text", 255)];
                    let h_label = horizontal.iter().find(|(_, a)| *a == style.align).map(|(n, _)| text(n)).unwrap_or_default();
                    let v_label = vertical.iter().find(|(_, a)| *a == style.vertical).map(|(n, _)| text(n)).unwrap_or_default();
                    let t_label = turns.iter().find(|(_, d)| *d == style.rotation).map(|(n, _)| text(n)).unwrap_or_else(|| {
                        if (91..=180).contains(&style.rotation) {
                            SharedString::from(format!("-{}°", style.rotation as i16 - 90))
                        } else {
                            SharedString::from(format!("{}°", style.rotation))
                        }
                    });
                    let h_items: Vec<MenuEntry> = horizontal.into_iter().map(|(n, a)| choice("xlsx.halign", text(n), style.align == a, StyleChange::Align(a))).collect();
                    let v_items: Vec<MenuEntry> = vertical.into_iter().map(|(n, a)| choice("xlsx.valign", text(n), style.vertical == a, StyleChange::Vertical(a))).collect();
                    let t_items: Vec<MenuEntry> = turns.into_iter().map(|(n, d)| {
                        let rot_input = rotation.clone();
                        MenuItem::new("xlsx.rotation", text(n)).checked(style.rotation == d).on(move |this, window, cx| {
                            this.format_cells(StyleChange::Rotation(d), cx);
                            let txt = match d {
                                0..=90 => format!("{d}"),
                                91..=180 => format!("-{}", d as i16 - 90),
                                _ => String::new(),
                            };
                            rot_input.update(cx, |i, cx| i.set_value(txt, window, cx));
                        }).into()
                    }).collect();

                    section = section
                        .child(caption("_xlsx.horizontal"))
                        .child(pane.select("cell-format-halign", None, h_label, h_items))
                        .child(caption("_xlsx.vertical"))
                        .child(pane.select("cell-format-valign", None, v_label, v_items))
                        .child(caption("_xlsx.text-direction"))
                        .child(h_flex().gap_1().items_center()
                            .child(div().flex_1().child(pane.select("cell-format-rotation", None, t_label, t_items)))
                            .child(div().flex_none().child(pane.marked("cell-format-rotation-input", crate::shell::chrome::text_field(&rotation).unit("°",px(20.)).spinner(
                                "cell-format-rotation-spin",
                                crate::shell::chrome::SpinnerSpec::new(1.0).range(-90.0, 90.0).integer_only(),
                                cx,
                                |this, deg, _, cx| this.apply_pane_rotation(&format!("{}", deg as i32), cx),
                            ))))
)
                        .child(h_flex().gap_1().flex_wrap()
                            .child(change_check("cell-format-wrap".into(), text("_xlsx.wrap-text"), style.wrap, StyleChange::Wrap(!style.wrap), cx))
                            .child(change_check("cell-format-shrink".into(), text("_xlsx.shrink-to-fit"), style.shrink, StyleChange::Shrink(!style.shrink), cx)))
                        .child(crate::shell::chrome::text_field_row(text("_xlsx.indent"),crate::shell::chrome::text_field(&indent).spinner("cell-indent",crate::shell::chrome::SpinnerSpec::new(1.).range(0.,250.).integer_only(),cx,|e,n,_,cx|e.format_cells(StyleChange::Indent(n as u32),cx))));
                }
                FormatTab::Font => {
                    let editor = pane.editor.clone();
                    let f2 = font.clone();
                    // Bold, italic, underline and strike: the toolbar's buttons.
                    let toggle = |id: &'static str, icon: &'static str, tip: &'static str, on: bool, change: StyleChange| -> AnyElement {
                        pane.marked(id, tool_item(&ToolItem::from(ToolButton::new(id, icon, tip).on(on).click(move |this, _, cx| this.format_cells(change.clone(), cx))), 3001, &editor))
                    };
                    let color = style.color;
                    let color_menu = crate::shell::color_palette::mark_current(super::toolbar::cell_color_menu(self, false), color);
                    section = section
                        .child(caption("_xlsx.font-name"))
                        .child(h_flex().gap_1().flex_wrap()
                            .child(div().w(crate::shell::chrome::FONT_DROPDOWN_WIDTH).flex_none().child(crate::shell::chrome::panel_font_dropdown("panel-font-xlsx-custom-format-1",&font,cx)))
                            .child(crate::shell::chrome::action_button("cell-format-font-set").small().label(text("_xlsx.set-font")).on_click(cx.listener(move |this, _, _, cx| {
                                let name = f2.read(cx).value().trim().to_string();
                                this.format_cells(StyleChange::Font((!name.is_empty()).then_some(name)), cx);
                            }))))
                        .child(crate::shell::chrome::text_field_row(text("_xlsx.size-points"),crate::shell::chrome::text_field(&size).unit("pt",px(20.)).spinner("cell-format-size-spin",crate::shell::chrome::SpinnerSpec::new(1.).range(1.,409.),cx,|this,n,_,cx|this.apply_pane_size(&n.to_string(),cx))))
                        .child(h_flex().gap_1()
                            .child(toggle("cell-format-bold", "format_bold", "_shell.bold-b", style.bold, StyleChange::Bold(!style.bold)))
                            .child(toggle("cell-format-italic", "format_italic", "_shell.italic-i", style.italic, StyleChange::Italic(!style.italic)))
                            .child(toggle("cell-format-underline", "format_underlined", "_shell.underline-u", style.underline.enabled(), StyleChange::Underline(!style.underline.enabled())))
                            .child(toggle("cell-format-strike", "format_strikethrough", "pptx.chrome.strikethrough", style.strike, StyleChange::Strike(!style.strike))))
                        .child(caption("docx.common.color"))
                        .child(pane.marked("cell-format-color",crate::shell::chrome::panel_color_field("cell-format-color-input",&panel.font_color,cx)));
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
                    section = section
                        .child(caption("_shell.underline-u"))
                        .child(pane.select("cell-format-underline-kind", None, u_label(style.underline), u_items))
                        .child(caption("xlsx.font-effects"))
                        .child(pane.select("cell-format-effect", None, e_label, e_items));
                }
                FormatTab::Border => {
                    let wb = self.wb();
                    let (line, line_color) = (wb.border_line, wb.border_color);
                    let colors = crate::shell::color_palette::mark_current(super::toolbar::border_color_menu(self), line_color);
                    let range = self.selected_range();
                    let multi_rows = range.is_some_and(|r| r.last_row > r.first_row);
                    let multi_columns = range.is_some_and(|r| r.last_column > r.first_column);
                    section = section
                        .child(caption("_xlsx.line-style"))
                        .child(pane.select("cell-format-line", None, border_line_label(line), super::toolbar::border_style_items(self)))
                        .child(caption("docx.common.color"))
                        .child(pane.select("cell-format-border-color", Some(rgb_hsla(line_color)), color_name(line_color, super::toolbar::COLORS), colors))
                        .child(caption("_xlsx.borders"))
                        .child(self.border_sample(&pane, multi_rows, multi_columns, cx));
                }
                FormatTab::Fill => {
                    let fill = style.fill;
                    let mut entries = super::toolbar::cell_color_menu(self, true);
                    if let Some(f) = fill {
                        entries = crate::shell::color_palette::mark_current(entries, f);
                    }
                    let label = fill.map(|f| color_name(f, super::toolbar::FILLS)).unwrap_or_else(|| text("xlsx.fill-none"));
                    let cur_pat = style.pattern.as_ref().map(|p| p.0.as_str()).unwrap_or("");
                    let pat_label = super::format_more::PATTERNS
                        .iter()
                        .find(|(_, p)| *p == cur_pat)
                        .map(|(n, _)| text(n))
                        .unwrap_or_else(|| text("_shell.none"));
                    let pat_items: Vec<MenuEntry> = super::format_more::PATTERNS
                        .iter()
                        .map(|&(n, preset)| {
                            MenuItem::new("xlsx.pattern", text(n))
                                .checked(cur_pat == preset)
                                .on(move |this, _, cx| {
                                    let st = this.current_style();
                                    let fg = st.pattern.as_ref().map(|p| p.1).unwrap_or(ibis_doc_model::Rgb(0));
                                    let bg = st.pattern.as_ref().map(|p| p.2).or(st.fill).unwrap_or(ibis_doc_model::Rgb(0xFF_FFFF));
                                    let change = if preset.is_empty() {
                                        StyleChange::Pattern(None)
                                    } else {
                                        StyleChange::Pattern(Some((preset.to_string(), fg, bg)))
                                    };
                                    this.format_cells(change, cx);
                                })
                                .into()
                        })
                        .collect();
                    let pat_col = style.pattern.as_ref().map(|p| p.1).unwrap_or(ibis_doc_model::Rgb(0));
                    let pat_col_label = super::format_more::PATTERN_COLORS
                        .iter()
                        .find(|(_, c)| *c == pat_col)
                        .map(|(n, _)| text(n))
                        .unwrap_or_else(|| format!("#{:06X}", pat_col.0).into());
                    let pat_col_items: Vec<MenuEntry> = super::format_more::PATTERN_COLORS
                        .iter()
                        .map(|&(n, col)| choice("xlsx.pattern-color", text(n), pat_col == col, StyleChange::PatternColor(col)))
                        .collect();
                    let grad_label = match &style.gradient {
                        None => text("_xlsx.no-gradient"),
                        Some(ibis_doc_model::CellGradient::Linear { degree: 0, .. }) => text("_xlsx.horizontal-left-to-right"),
                        Some(ibis_doc_model::CellGradient::Linear { degree: 90, .. }) => text("_xlsx.vertical-top-to-bottom"),
                        Some(ibis_doc_model::CellGradient::Linear { degree: 45, .. }) => text("_xlsx.diagonal-top-left-to-bottom-right"),
                        Some(ibis_doc_model::CellGradient::Linear { degree: 135, .. }) => text("_xlsx.diagonal-top-right-to-bottom-left"),
                        Some(ibis_doc_model::CellGradient::Linear { degree, .. }) => SharedString::from(format!("{degree}°")),
                        Some(ibis_doc_model::CellGradient::Path { .. }) => text("_xlsx.from-center"),
                    };
                    let mut grad_items: Vec<MenuEntry> = vec![
                        choice("xlsx.gradient-none", text("_xlsx.no-gradient"), style.gradient.is_none(), StyleChange::Gradient(None)),
                    ];
                    for (id, n, deg) in [
                        ("xlsx.gradient-horz", "_xlsx.horizontal-left-to-right", 0),
                        ("xlsx.gradient-vert", "_xlsx.vertical-top-to-bottom", 90),
                        ("xlsx.gradient-diag-down", "_xlsx.diagonal-top-left-to-bottom-right", 45),
                        ("xlsx.gradient-diag-up", "_xlsx.diagonal-top-right-to-bottom-left", 135),
                    ] {
                        let is_cur = matches!(style.gradient, Some(ibis_doc_model::CellGradient::Linear { degree, .. }) if degree == deg);
                        grad_items.push(
                            MenuItem::new(id, text(n))
                                .checked(is_cur)
                                .on(move |this, _, cx| {
                                    let col = this.current_style().fill.unwrap_or(ibis_doc_model::Rgb(0x4472C4));
                                    let grad = ibis_doc_model::CellGradient::Linear {
                                        degree: deg,
                                        stops: vec![(0, col), (1000, ibis_doc_model::Rgb(0xFF_FFFF))],
                                    };
                                    this.format_cells(StyleChange::Gradient(Some(grad)), cx);
                                })
                                .into(),
                        );
                    }
                    let is_path = matches!(style.gradient, Some(ibis_doc_model::CellGradient::Path { .. }));
                    grad_items.push(
                        MenuItem::new("xlsx.gradient-center", text("_xlsx.from-center"))
                            .checked(is_path)
                            .on(|this, _, cx| {
                                let col = this.current_style().fill.unwrap_or(ibis_doc_model::Rgb(0x4472C4));
                                let grad = ibis_doc_model::CellGradient::Path {
                                    path: ibis_doc_model::GradientPath::Rect,
                                    focus: [50_000, 50_000, 50_000, 50_000],
                                    stops: vec![(0, col), (1000, ibis_doc_model::Rgb(0xFF_FFFF))],
                                };
                                this.format_cells(StyleChange::Gradient(Some(grad)), cx);
                            })
                            .into(),
                    );
                    section = section
                        .child(caption("_xlsx.background-color"))
                        .child(pane.marked("cell-format-fill",crate::shell::chrome::panel_color_field("cell-format-fill-input",&panel.fill_color,cx)))
                        .child(caption("xlsx.fill-pattern"))
                        .child(pane.select("cell-format-pattern", None, pat_label, pat_items))
                        .child(caption("xlsx.pattern-color"))
                        .child(pane.marked("cell-format-pattern-color",crate::shell::chrome::panel_color_field("cell-format-pattern-color-input",&panel.pattern_color,cx)))
                        .child(caption("xlsx.fill-gradient"))
                        .child(pane.select("cell-format-gradient", None, grad_label, grad_items));
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
        Some(SidebarPanel { title: "fix.protection.format-cells".into(), body: body.into_any_element() })
    }

    /// A section's row: the triangle and the name; pressing it opens or closes the section.
    fn pane_section_header(&self, pane: &Pane, n: usize, tab: FormatTab, open: bool, cx: &mut Context<Self>) -> AnyElement {
        let id = format!("cell-format-section-{tab:?}");
        let row = h_flex()
            .id(SharedString::from(id.clone()))
            .h(px(30.))
            .px(px(4.))
            .gap(px(2.))
            .items_center()
            .rounded(px(4.))
            .cursor_pointer()
            .bg(hsla(0.0, 0.0, 0.5, 0.08))
            .hover(|s| s.bg(hsla(0.0, 0.0, 0.5, 0.16)))
            .child(Icon::new(Material(if open { "arrow_drop_down" } else { "arrow_right" })).size(px(20.)))
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(crate::shell::i18n::text(tab.label())))
            .on_click(cx.listener(move |this, _, _, cx| {
                let open = &mut this.wb_mut().format_open[n];
                *open = !*open;
                cx.notify();
            }));
        pane.marked(&id, row)
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
