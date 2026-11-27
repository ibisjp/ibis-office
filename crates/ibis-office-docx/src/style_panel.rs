// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スタイルの変更 (P13): the sidebar panel that changes a paragraph style
//! (its font, size, bold, italic, alignment and spacing), and スタイルを選択
//! 箇所に合わせて更新, which gives the caret's paragraph style the
//! paragraph's own formatting. Every paragraph of the style changes with it.

use gpui_kit::component::input::InputState;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::flow_command;
use ibis_doc_model::{Block, FlowAlign, FlowCommand, FlowStyle};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The panel's fields, for the style `id` as it was when the panel opened.
pub(crate) struct StyleEditState {
    pub(crate) id: String,
    pub(crate) name: String,
    /// 英数字用 (`w:ascii`, `w:hAnsi`) and 日本語用 (`w:eastAsia`) typefaces: the typeface's name, or the theme font's label (+本文のフォント, ...) when `*_theme` is set.
    pub(crate) font_latin: Entity<InputState>,
    pub(crate) font_east_asian: Entity<InputState>,
    /// The theme font each of the two refers to (`minorHAnsi`, `majorEastAsia`, ...).
    pub(crate) latin_theme: Option<String>,
    pub(crate) east_asian_theme: Option<String>,
    pub(crate) size: Entity<InputState>,
    /// The text's colour, `RRGGBB`.
    pub(crate) color: Entity<InputState>,
    pub(crate) before: Entity<InputState>,
    pub(crate) after: Entity<InputState>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) align: FlowAlign,
    /// スタイルを自動的に更新する (`w:autoRedefine`).
    pub(crate) auto_update: bool,
    /// The fields' first values: only what was changed is set in the style.
    initial: ([String; 4], bool, bool, FlowAlign),
    /// The two typeface fields' first values (text and theme).
    initial_fonts: [(String, Option<String>); 2],
}

/// Office's default theme typefaces for a document that has no theme part (Japanese Word's: Calibri Light / Calibri, Yu Gothic Light / Yu Mincho, by the typefaces' English family names: no Japanese in the source, as lint-i18n asks).
fn default_theme_fonts() -> ibis_doc_model::ThemeFonts {
    let set = |latin: &str, east: &str| ibis_doc_model::FontSet { latin: Some(latin.into()), east_asian: Some(east.into()), complex: None };
    ibis_doc_model::ThemeFonts { major: set("Calibri Light", "Yu Gothic Light"), minor: set("Calibri", "Yu Mincho") }
}

/// The i18n key of a theme font reference's label (Word's +本文のフォント, +見出しの日本語のフォント, ...).
fn theme_label_key(theme: &str) -> &'static str {
    match (theme.starts_with("major"), theme.ends_with("EastAsia")) {
        (false, false) => "docx.style-panel.theme-body-latin",
        (true, false) => "docx.style-panel.theme-heading-latin",
        (false, true) => "docx.style-panel.theme-body-east-asian",
        (true, true) => "docx.style-panel.theme-heading-east-asian",
    }
}

/// What a typeface field shows: the theme font's label, else the typeface.
fn font_text(font: Option<&str>, theme: Option<&str>) -> String {
    match theme {
        Some(t) => crate::shell::i18n::text(theme_label_key(t)).to_string(),
        None => font.unwrap_or_default().to_string(),
    }
}

/// The typeface a theme font reference resolves to in `fonts`.
fn theme_typeface(fonts: &ibis_doc_model::ThemeFonts, theme: &str) -> Option<String> {
    let set = if theme.starts_with("major") { &fonts.major } else { &fonts.minor };
    if theme.ends_with("EastAsia") { set.east_asian.clone() } else { set.latin.clone() }
}

/// Points as the panel shows them ("10.5", "12").
fn points(v: f32) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map_or(s.clone(), str::to_string)
}

/// One range's typeface list: the theme fonts (+本文・+見出し) first, then the recent and installed typefaces. `input` holds what the field shows; choosing
/// an entry sets it and the field's theme reference (`theme` is the reference now).
pub(crate) fn style_font_entries(id: &'static str, input: &Entity<InputState>, theme: Option<String>, east: bool, cx: &Context<SlideEditor>) -> (String, Vec<crate::shell::chrome::MenuEntry>) {
    let current = input.read(cx).value().to_string();
    let themes: Vec<_> = ["minor", "major"]
        .into_iter()
        .map(|kind| {
            let name = format!("{kind}{}", if east { "EastAsia" } else { "HAnsi" });
            let label = crate::shell::i18n::text(theme_label_key(&name));
            let (field, chosen) = (input.clone(), name.clone());
            crate::shell::chrome::MenuItem::new("docx-style-font-theme", label.clone())
                .checked(theme.as_deref() == Some(name.as_str()))
                .on(move |editor: &mut SlideEditor, window, cx| {
                    field.update(cx, |s, cx| s.set_value(label.to_string(), window, cx));
                    if let Some(edit) = editor.doc_state_mut().and_then(|s| s.style_edit.as_mut()) {
                        if east { edit.east_asian_theme = Some(chosen.clone()) } else { edit.latin_theme = Some(chosen.clone()) }
                    }
                    cx.notify();
                })
                .into()
        })
        .collect();
    let field = input.clone();
    let entries = crate::shell::font_list::build_font_menu_entries(id, themes, theme.is_none().then_some(current.as_str()), move |name, editor, window, cx| {
        field.update(cx, |s, cx| s.set_value(name.to_string(), window, cx));
        if let Some(edit) = editor.doc_state_mut().and_then(|s| s.style_edit.as_mut()) {
            if east { edit.east_asian_theme = None } else { edit.latin_theme = None }
        }
        cx.notify();
    });
    (current, entries)
}

#[ibis_office_macros::editor_ext(StylePanel)]
impl SlideEditor {
    /// The paragraph style of the caret's paragraph (its id).
    pub(crate) fn caret_style(&self) -> Option<String> {
        let (start, _) = self.ordered_selection()?;
        let doc = self.flow()?;
        let p = flow_command::paragraph(doc, start.para)?;
        p.style.clone().or_else(|| doc.styles.default_paragraph.clone()).filter(|id| doc.styles.styles.contains_key(id))
    }

    /// The name of the caret's paragraph style, as the menus show it.
    pub(crate) fn caret_style_name(&self) -> Option<String> {
        let id = self.caret_style()?;
        Some(self.flow()?.styles.styles.get(&id)?.name.clone())
    }

    /// スタイルを選択箇所に合わせて更新: the caret's paragraph style takes
    /// the paragraph's formatting and that of the text at the caret.
    pub(crate) fn update_style_to_match(&mut self, cx: &mut Context<Self>) {
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };
        let Some((id, style, paragraph)) = flow_command::style_matching(doc, start.para, start.offset) else { return };
        let Some((container, index)) = flow_command::locate(doc, start.para) else { return };
        let keep = self.text_selection();
        self.run_flow(
            vec![FlowCommand::SetStyle { id, style: Some(style) }, FlowCommand::ReplaceBlockRange { container, index, remove: 1, blocks: vec![Block::Paragraph(paragraph)] }],
            cx,
        );
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
    }

    /// Define style `id` as `style` (one step of undo).
    pub(crate) fn set_style(&mut self, id: String, style: FlowStyle, cx: &mut Context<Self>) -> bool {
        self.run_flow(vec![FlowCommand::SetStyle { id, style: Some(style) }], cx)
    }

    /// スタイルの変更…: the panel, for the caret's paragraph style.
    pub fn open_style_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some(id) = self.caret_style() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(style) = doc.styles.styles.get(&id).cloned() else { return };
        // What the style's paragraphs look like: the chain and the defaults.
        let (para, run) = doc.styles.paragraph_base(Some(&id), None);
        let drawn = run.style();
        // What is drawn for each of the two ranges: the typeface of its own, else the other's (as `font_for` falls back).
        let latin = (run.font.clone().or_else(|| run.east_asian_font.clone()), if run.font.is_some() { run.font_theme.clone() } else { run.east_asian_font_theme.clone() });
        let east = (run.east_asian_font.clone().or_else(|| run.font.clone()), if run.east_asian_font.is_some() { run.east_asian_font_theme.clone() } else { run.font_theme.clone() });
        let latin_text = font_text(latin.0.as_deref(), latin.1.as_deref());
        let east_text = font_text(east.0.as_deref(), east.1.as_deref());
        let size = points(drawn.size as f32 / 100.0);
        let color = format!("{:06X}", drawn.color.0);
        let before = points(para.before.unwrap_or(0) as f32 / 12_700.0);
        let after = points(para.after.unwrap_or(0) as f32 / 12_700.0);
        let align = para.align.unwrap_or(FlowAlign::Justify);
        let input = |value: &str, window: &mut Window, cx: &mut Context<Self>| {
            let value = value.to_string();
            cx.new(|cx| {
                let mut s = InputState::new(window, cx);
                s.set_value(value, window, cx);
                s
            })
        };
        let state = StyleEditState {
            id,
            name: super::toolbar::style_display_name(&style.name),
            font_latin: input(&latin_text, window, cx),
            font_east_asian: input(&east_text, window, cx),
            latin_theme: latin.1.clone(),
            east_asian_theme: east.1.clone(),
            size: input(&size, window, cx),
            color: input(&color, window, cx),
            before: input(&before, window, cx),
            after: input(&after, window, cx),
            bold: drawn.bold,
            italic: drawn.italic,
            align,
            auto_update: style.auto_update,
            initial: ([size, color, before, after], drawn.bold, drawn.italic, align),
            initial_fonts: [(latin_text, latin.1), (east_text, east.1)],
        };
        if let Some(s) = self.doc_state_mut() {
            s.style_edit = Some(state);
            s.font_edit = None; s.color_edit = None; s.paragraph_edit = None; s.picture_edit = None; s.table_size = None;
            s.find = None;
            s.link = None;
            s.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// The panel's changes made to its style.
    pub(crate) fn apply_style_edit(&mut self, cx: &mut Context<Self>) {
        if self.alert_edit_blocked("docx.style.modify", cx) {
            return;
        }
        let Some(edit) = self.doc_state().and_then(|s| s.style_edit.as_ref()) else { return };
        let value = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let fields = [value(&edit.size), value(&edit.color), value(&edit.before), value(&edit.after)];
        let [size, color, before, after] = fields.clone();
        let (latin, east) = (value(&edit.font_latin), value(&edit.font_east_asian));
        let (latin_theme, east_theme) = (edit.latin_theme.clone(), edit.east_asian_theme.clone());
        let initial_fonts = edit.initial_fonts.clone();
        let (bold, italic, align) = (edit.bold, edit.italic, edit.align);
        let auto_update = edit.auto_update;
        let initial = edit.initial.clone();
        let id = edit.id.clone();
        let Some(mut style) = self.flow().and_then(|d| d.styles.styles.get(&id).cloned()) else { return };
        // Only the field that was changed is written, the other's typeface, theme reference and hint stay (K0086).
        let fonts_now = [(latin.clone(), latin_theme.clone()), (east.clone(), east_theme.clone())];
        let theme_fonts = self.flow().and_then(|d| d.theme_fonts.clone()).unwrap_or_else(default_theme_fonts);
        if (latin.clone(), latin_theme.clone()) != initial_fonts[0] && !latin.is_empty() {
            let name = latin_theme.as_deref().and_then(|t| theme_typeface(&theme_fonts, t)).unwrap_or(latin);
            style.run.font = Some(name.clone());
            // Word's Font.Name (the 書体 box of the style dialog) sets the complex script typeface to the same name; a theme reference leaves it (Word wrote no csTheme).
            if latin_theme.is_none() {
                style.run.cs_font = Some(name.clone());
                style.run.cs_font_theme = None;
            }
            style.run.h_ansi_font = Some(name);
            style.run.font_theme = latin_theme.clone();
            style.run.h_ansi_font_theme = latin_theme;
        }
        if (east.clone(), east_theme.clone()) != initial_fonts[1] && !east.is_empty() {
            let name = east_theme.as_deref().and_then(|t| theme_typeface(&theme_fonts, t)).unwrap_or(east);
            style.run.east_asian_font = Some(name);
            style.run.east_asian_font_theme = east_theme;
        }
        if size != initial.0[0]
            && let Ok(pt) = size.parse::<f32>()
            && (1.0..=1638.0).contains(&pt)
        {
            style.run.size = Some((pt * 2.0).round() as u32 * 50);
        }
        let twips = |s: &str| s.parse::<f32>().ok().filter(|v| (0.0..=1584.0).contains(v)).map(|pt| (pt * 20.0).round() as i64 * 635);
        if before != initial.0[2]
            && let Some(v) = twips(&before)
        {
            style.para.before = Some(v);
        }
        if after != initial.0[3]
            && let Some(v) = twips(&after)
        {
            style.para.after = Some(v);
        }
        if color != initial.0[1]
            && let Ok(c) = u32::from_str_radix(color.trim_start_matches('#'), 16)
            && color.trim_start_matches('#').len() == 6
        {
            style.run.color = Some(ibis_doc_model::Rgb(c));
            style.run.theme_color = None;
        }
        if bold != initial.1 {
            style.run.bold = Some(bold);
        }
        if italic != initial.2 {
            style.run.italic = Some(italic);
        }
        if align != initial.3 {
            style.para.align = Some(align);
        }
        style.auto_update = auto_update;
        if self.set_style(id, style, cx)
            && let Some(edit) = self.doc_state_mut().and_then(|s| s.style_edit.as_mut())
        {
            edit.initial = (fields, bold, italic, align);
            edit.initial_fonts = fonts_now;
        }
    }

    pub fn style_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let edit = self.doc_state()?.style_edit.as_ref()?;
        let (font_latin, font_east, size, color, before, after) = (edit.font_latin.clone(), edit.font_east_asian.clone(), edit.size.clone(), edit.color.clone(), edit.before.clone(), edit.after.clone());
        let (latin_theme, east_theme) = (edit.latin_theme.clone(), edit.east_asian_theme.clone());
        let (bold, italic, align, auto_update) = (edit.bold, edit.italic, edit.align, edit.auto_update);
        let title = crate::shell::i18n::format("docx.style-panel.modify-style-name", &[("name", &edit.name)]);
        let (current_latin, latin_entries) = style_font_entries("docx-style-font-list-ascii", &font_latin, latin_theme, false, cx);
        let (current_east, east_entries) = style_font_entries("docx-style-font-list-east-asian", &font_east, east_theme, true, cx);
        let current_color = u32::from_str_radix(color.read(cx).value().trim_start_matches('#'), 16).ok().map(ibis_doc_model::Rgb);
        let color_input = color.clone();
        let pick: crate::shell::color_palette::PickColor = std::rc::Rc::new(move |this, chosen, window, cx| {
            if let Some(c) = chosen { this.note_recent_color(c); color_input.update(cx, |s, cx| s.set_value(format!("{:06X}", c.0), window, cx)); }
            cx.notify();
        });
        let standard_colors: Vec<_> = super::toolbar::COLORS.iter().map(|&(id, label, rgb)| (crate::shell::i18n::text(label), ibis_doc_model::Rgb(rgb), id)).collect();
        let color_entries = crate::shell::color_palette::simple_palette(None, Some(&super::toolbar::THEME_10), &standard_colors, current_color, pick);
        let weak = cx.weak_entity();
        // Size and spacing (pt): applied by 適用 only (Enter does not apply them), so ▲▼ only change the number.
        const FONT_STEPS: &[f32] = &[8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0];
        let field_cx = &*cx;
        let spin_row = |label: &'static str, input: &Entity<InputState>, id: &'static str, spec: crate::shell::chrome::SpinnerSpec| {
            h_flex().gap_2().child(div().w(px(64.)).flex_none().text_sm().child(crate::shell::i18n::text(label))).child(crate::shell::chrome::text_field(input).full_width().spinner(id, spec, field_cx, |_, _, _, _| {}).unit("pt",px(24.)))
        };
        let toggle = |id: &'static str, label: &'static str, on: bool, f: fn(&mut StyleEditState)| {
            crate::shell::chrome::checkbox_button(id, crate::shell::i18n::text(label), on, false, cx.listener(move |this, _, _, cx| {
                if let Some(edit) = this.doc_state_mut().and_then(|s| s.style_edit.as_mut()) {
                    f(edit);
                }
                cx.notify();
            }))
        };
        let align_button = |id: &'static str, label: &'static str, a: FlowAlign| {
            crate::shell::chrome::radio_button(id, align == a, false, crate::shell::i18n::text(label), cx.listener(move |this, _, _, cx| {
                if let Some(edit) = this.doc_state_mut().and_then(|s| s.style_edit.as_mut()) {
                    edit.align = a;
                }
                cx.notify();
            }))
        };
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(h_flex().items_center().gap_2().child(div().w(px(64.)).flex_none().text_sm().child(crate::shell::i18n::text("docx.style-panel.east-asian-font"))).child(div().w(crate::shell::chrome::FONT_DROPDOWN_WIDTH).flex_none().child(crate::shell::chrome::font_dropdown("docx-style-font-list-east-asian", &current_east, east_entries, 0, &weak))))
            .child(h_flex().items_center().gap_2().child(div().w(px(64.)).flex_none().text_sm().child(crate::shell::i18n::text("docx.style-panel.latin-font"))).child(div().w(crate::shell::chrome::FONT_DROPDOWN_WIDTH).flex_none().child(crate::shell::chrome::font_dropdown("docx-style-font-list-ascii", &current_latin, latin_entries, 1, &weak))))
            .child(spin_row("_shell.size", &size, "style-size", crate::shell::chrome::SpinnerSpec::new(1.0).steps(FONT_STEPS).range(1.0, 1638.0)))
            .child(h_flex().gap_2().child(div().w(px(64.)).flex_none().text_sm().child(crate::shell::i18n::text("docx.common.color"))).child(div().w(crate::shell::chrome::FIELD_WIDTH).flex_none().child(crate::shell::chrome::color_text_field("style-color", &color, current_color, color_entries, &weak))))
            .child(h_flex().gap_1().child(toggle("style-bold", "_shell.bold", bold, |e| e.bold = !e.bold)).child(toggle("style-italic", "docx.common.italic", italic, |e| e.italic = !e.italic)))
            .child(
                crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                    .child(align_button("style-left", "_shell.left", FlowAlign::Left))
                    .child(align_button("style-center", "_shell.center", FlowAlign::Center))
                    .child(align_button("style-right", "_shell.right", FlowAlign::Right))
                    .child(align_button("style-justify", "docx.style-panel.justify", FlowAlign::Justify)),
            )
            .child(toggle("style-auto-update", "docx.style-panel.automatically-update-style", auto_update, |e| e.auto_update = !e.auto_update))
            .child(spin_row("docx.paragraph-panel.before", &before, "style-before", crate::shell::chrome::SpinnerSpec::new(6.0).range(0.0, 1584.0)))
            .child(spin_row("docx.paragraph-panel.after", &after, "style-after", crate::shell::chrome::SpinnerSpec::new(6.0).range(0.0, 1584.0)))
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("style-apply", crate::shell::i18n::text("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|this, _, _, cx| this.apply_style_edit(cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("style-match", crate::shell::i18n::text("docx.style-panel.update-match-selection"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|this, _, _, cx| this.update_style_to_match(cx))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(title, body.into_any_element(), panel_actions))
    }
}
