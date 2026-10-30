// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! スタイルの一覧 > 新しいスタイル… (Word's 書式から新しいスタイルを
//! 作成): a paragraph or character style named by the user, based on a
//! style, with the next paragraph's style, made from the formatting at the
//! caret (the font, size, bold, italic and colour shown, changeable), and
//! applied to the selection. And 基準にするスタイル of the caret's
//! paragraph style changed.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{FlowCommand, FlowStyle, FlowStyles, ParaChange, StyleKind, flow_command};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// The panel's fields.
pub(crate) struct NewStylePanel {
    pub(crate) name: Entity<InputState>,
    pub(crate) character: bool,
    /// 基準にするスタイル and 次の段落のスタイル, by the names shown.
    pub(crate) base: Entity<InputState>,
    pub(crate) next: Entity<InputState>,
    pub(crate) font: Entity<InputState>,
    pub(crate) size: Entity<InputState>,
    /// `RRGGBB`.
    pub(crate) color: Entity<InputState>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// The fields' first values: only what was changed is set.
    initial: ([String; 3], bool, bool),
    /// The new base of the caret's paragraph style.
    pub(crate) rebase: Entity<InputState>,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

/// The style of `kind` named `name` (its name in the file, or as the menus
/// show it).
pub(crate) fn style_named(styles: &FlowStyles, name: &str, kind: StyleKind) -> Option<String> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    styles
        .styles
        .iter()
        .filter(|(_, s)| s.kind == kind)
        .find(|(_, s)| s.name.eq_ignore_ascii_case(name) || super::toolbar::style_display_name(&s.name) == name)
        .map(|(id, _)| id.clone())
}

/// A new style id for `name`: its letters and digits (Word's way), or `a`
/// and a number for a name of other characters (as Word's Japanese
/// template's ids are), not taken.
fn new_style_id(styles: &FlowStyles, name: &str) -> String {
    let ascii: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
    if !ascii.is_empty() && !styles.styles.contains_key(&ascii) {
        return ascii;
    }
    let stem = if ascii.is_empty() { "a".to_string() } else { ascii };
    (1..).map(|n| format!("{stem}{n}")).find(|id| !styles.styles.contains_key(id)).unwrap_or_default()
}

impl SlideEditor {
    fn input_with(&self, value: String, window: &mut Window, cx: &mut Context<Self>) -> Entity<InputState> {
        cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(value, window, cx);
            s
        })
    }

    pub(crate) fn open_new_style_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let Some((start, _)) = self.ordered_selection() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(p) = flow_command::paragraph(doc, start.para) else { return };
        let (_, _, drawn) = p.format_at(start.offset);
        let base = self.caret_style().and_then(|id| doc.styles.styles.get(&id).map(|s| super::toolbar::style_display_name(&s.name))).unwrap_or_default();
        let font = drawn.east_asian_font.clone().or_else(|| drawn.font.clone()).unwrap_or_default();
        let size = {
            let s = format!("{:.1}", drawn.size as f32 / 100.0);
            s.strip_suffix(".0").map_or(s.clone(), str::to_string)
        };
        let color = format!("{:06X}", drawn.color.0 & 0xFF_FF_FF);
        let fields = [font.clone(), size.clone(), color.clone()];
        let name = self.input_with(String::new(), window, cx);
        name.update(cx, |i, cx| i.focus(window, cx));
        let panel = NewStylePanel {
            name,
            character: false,
            base: self.input_with(base.clone(), window, cx),
            next: self.input_with(String::new(), window, cx),
            font: self.input_with(font, window, cx),
            size: self.input_with(size, window, cx),
            color: self.input_with(color, window, cx),
            bold: drawn.bold,
            italic: drawn.italic,
            initial: (fields, drawn.bold, drawn.italic),
            rebase: self.input_with(String::new(), window, cx),
            message: String::new(),
            ui: Default::default(),
        };
        if let Some(s) = self.doc_state_mut() {
            s.find = None;
            s.link = None;
            s.style_edit = None;
            s.font_edit = None;
            s.color_edit = None;
            s.paragraph_edit = None;
            s.picture_edit = None;
            s.table_size = None;
            s.comments = None;
            s.ruby = None;
            s.text_box = None;
            s.merge = None;
            s.helper = super::help_panels::HelpPanels::default();
            s.helper.new_style = Some(panel);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    fn new_style_message(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.new_style.as_mut()) {
            p.message = message.into();
        }
        cx.notify();
    }

    /// 作成: the style made and applied to the caret's paragraphs (a
    /// paragraph style) or the selected text (a character style).
    pub(crate) fn create_style(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.new_style.as_ref()) else { return };
        let value = |e: &Entity<InputState>| e.read(cx).value().trim().to_string();
        let (name, base, next) = (value(&p.name), value(&p.base), value(&p.next));
        let fields = [value(&p.font), value(&p.size), value(&p.color)];
        let (character, bold, italic, initial) = (p.character, p.bold, p.italic, p.initial.clone());
        let (Some((start, end)), Some(doc)) = (self.ordered_selection(), self.flow()) else { return };
        if name.is_empty() {
            return self.new_style_message(text("fix.style-new.enter-name-style"), cx);
        }
        if doc.styles.styles.values().any(|s| s.name.eq_ignore_ascii_case(&name) || super::toolbar::style_display_name(&s.name) == name) {
            return self.new_style_message(text("fix.style-new.style-name-already-exists"), cx);
        }
        let kind = if character { StyleKind::Character } else { StyleKind::Paragraph };
        let based_on = if base.is_empty() {
            None
        } else {
            match style_named(&doc.styles, &base, kind) {
                Some(id) => Some(id),
                // The caret's paragraph style shown for a character style:
                // based on none (the paragraph's font).
                None if character && style_named(&doc.styles, &base, StyleKind::Paragraph).is_some() => None,
                None => return self.new_style_message(crate::shell::i18n::format("fix.style-new.there-no-style-named-name", &[("name", &base)]), cx),
            }
        };
        let id = new_style_id(&doc.styles, &name);
        let next = if character || next.is_empty() {
            (!character).then(|| id.clone())
        } else {
            match style_named(&doc.styles, &next, StyleKind::Paragraph) {
                Some(n) => Some(n),
                None if next == name => Some(id.clone()),
                None => return self.new_style_message(crate::shell::i18n::format("fix.style-new.there-no-style-named-name", &[("name", &next)]), cx),
            }
        };
        // The formatting at the caret (direct), and what was changed.
        let Some(para) = flow_command::paragraph(doc, start.para) else { return };
        let (mut run, _, _) = para.format_at(start.offset);
        let [font, size, color] = &fields;
        if *font != initial.0[0] && !font.is_empty() {
            run.font = Some(font.clone());
            run.east_asian_font = (!font.is_ascii()).then(|| font.clone()).or(run.east_asian_font);
        }
        if *size != initial.0[1]
            && let Ok(pt) = size.parse::<f32>()
            && (1.0..=1638.0).contains(&pt)
        {
            run.size = Some((pt * 2.0).round() as u32 * 50);
        }
        if *color != initial.0[2]
            && color.trim_start_matches('#').len() == 6
            && let Ok(c) = u32::from_str_radix(color.trim_start_matches('#'), 16)
        {
            run.color = Some(ibis_doc_model::Rgb(c));
        }
        if bold != initial.1 {
            run.bold = Some(bold);
        }
        if italic != initial.2 {
            run.italic = Some(italic);
        }
        let mut style = FlowStyle { kind, name: name.clone(), based_on, next, ..FlowStyle::default() };
        style.run = run;
        let mut commands = Vec::new();
        if character {
            commands.push(FlowCommand::SetStyle { id: id.clone(), style: Some(style) });
            if start != end {
                // The direct formatting taken into the style (as Word does).
                commands.push(FlowCommand::SetRunFormat { from: start, to: end, change: ibis_doc_model::RunChange::Clear });
                commands.push(FlowCommand::SetCharStyle { from: start, to: end, style: Some(id.clone()) });
            }
        } else {
            style.para = ibis_doc_model::ParaFormat { num_id: None, level: None, ..para.direct.clone() };
            commands.push(FlowCommand::SetStyle { id: id.clone(), style: Some(style) });
            let Some((first, last)) = self.selected_paragraphs() else { return };
            commands.push(FlowCommand::SetParagraphFormat { first, last, change: ParaChange::Style(Some(id.clone())) });
        }
        // Made: the panel closes (as Word's dialog does), its fields being
        // the caret's of when it opened.
        if self.run_flow(commands, cx)
            && let Some(state) = self.doc_state_mut()
        {
            state.helper.new_style = None;
            cx.notify();
        }
    }

    /// 基準にするスタイル of the caret's paragraph style.
    pub(crate) fn rebase_caret_style(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.doc_state().and_then(|s| s.helper.new_style.as_ref()) else { return };
        let base = p.rebase.read(cx).value().trim().to_string();
        let (Some(id), Some(doc)) = (self.caret_style(), self.flow()) else { return };
        let Some(mut style) = doc.styles.styles.get(&id).cloned() else { return };
        let based_on = if base.is_empty() {
            None
        } else {
            match style_named(&doc.styles, &base, StyleKind::Paragraph) {
                Some(b) => Some(b),
                None => return self.new_style_message(crate::shell::i18n::format("fix.style-new.there-no-style-named-name", &[("name", &base)]), cx),
            }
        };
        // Not on itself, nor on one based on it.
        if let Some(b) = &based_on
            && doc.styles.chain(b).iter().any(|s| std::ptr::eq(*s, &doc.styles.styles[&id]))
        {
            return self.new_style_message(text("fix.style-new.style-can-t-base-based"), cx);
        }
        if style.based_on == based_on {
            return;
        }
        style.based_on = based_on;
        if self.set_style(id, style, cx) {
            self.new_style_message(text("fix.style-new.base-style-changed"), cx);
        }
    }

    pub(crate) fn new_style_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.new_style.as_ref()?;
        let caret = self.caret_style_name().map(|n| super::toolbar::style_display_name(&n)).unwrap_or_default();
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let row = |id: &str, label: &str, input: &Entity<InputState>| h_flex().gap_2().child(div().w(px(120.)).text_sm().child(text(label))).child(div().flex_1().child(mark(id, crate::shell::chrome::text_field(input).full_width().into_any_element())));
        let toggle = |id: &'static str, label: &str, on: bool, f: fn(&mut NewStylePanel)| {
            mark(
                id,
                crate::shell::chrome::checkbox_button(SharedString::from(format!("new-style-{id}")),text(label),on,false,cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.doc_state_mut().and_then(|s| s.helper.new_style.as_mut()) {
                            f(p);
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        let style_names:Vec<String>=self.flow()?.styles.styles.values().map(|s|super::toolbar::style_display_name(&s.name)).collect();
        let mut body = crate::shell::chrome::panel_form()
            .p_0().children(crate::shell::chrome::panel_message(&p.message))
            .child(row("name", "_shell.name", &p.name))
            .child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
                .child(crate::shell::chrome::segment_control::SegmentItem::label("new-style-paragraph",text("fix.style-new.paragraph"),!p.character).on_click(cx.listener(|e,_,_,cx|{if let Some(p)=e.doc_state_mut().and_then(|s|s.helper.new_style.as_mut()){p.character=false;}cx.notify();})).on_bounds({let ui=p.ui.clone();move|bounds|{ui.borrow_mut().insert("paragraph".to_string(),bounds);}}))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("new-style-character",text("fix.goto-special.text"),p.character).on_click(cx.listener(|e,_,_,cx|{if let Some(p)=e.doc_state_mut().and_then(|s|s.helper.new_style.as_mut()){p.character=true;}cx.notify();})).on_bounds({let ui=p.ui.clone();move|bounds|{ui.borrow_mut().insert("character".to_string(),bounds);}})))
            .child(h_flex().gap_2().items_center().child(div().w(px(120.)).text_sm().child(text("fix.style-new.style-based"))).child(crate::shell::chrome::input_dropdown("new-style-base",&p.base,style_names.clone(),cx)));
        if !p.character {
            body = body.child(h_flex().gap_2().items_center().child(div().w(px(120.)).text_sm().child(text("fix.style-new.style-following-paragraph"))).child(crate::shell::chrome::input_dropdown("new-style-next",&p.next,style_names.clone(),cx)));
        }
        body = body
            .child(h_flex().gap_2().items_center().child(div().w(px(120.)).text_sm().child(text("_shell.font"))).child(mark("font",crate::shell::chrome::panel_font_dropdown("panel-font-new-style",&p.font,cx))))
            // The size (pt): applied by 作成 only, so ▲▼ only change the number.
            .child(h_flex().gap_2().child(div().w(px(120.)).text_sm().child(text("_shell.size"))).child(div().w(crate::shell::chrome::FIELD_WIDTH).flex_none().child(mark("size", crate::shell::chrome::text_field(&p.size).full_width().spinner("new-style-size", crate::shell::chrome::SpinnerSpec::new(1.0).steps(FONT_STEPS).range(1.0, 1638.0), cx, |_, _, _, _| {}).into_any_element()))))
            .child(h_flex().gap_2().child(div().w(px(120.)).text_sm().child(text("docx.common.color"))).child(div().w(crate::shell::chrome::FIELD_WIDTH).flex_none().child(mark("color", crate::shell::chrome::panel_color_field("new-style-color",&p.color,cx)))))
            .child(h_flex().gap_1().child(toggle("bold", "_shell.bold", p.bold, |p| p.bold = !p.bold)).child(toggle("italic", "docx.common.italic", p.italic, |p| p.italic = !p.italic)))
            .child(div().h(px(8.)))
            .child(div().text_sm().child(crate::shell::i18n::format("fix.style-new.style-name-based", &[("name", &caret)])))
            .child(mark("rebase", crate::shell::chrome::input_dropdown("new-style-rebase",&p.rebase,style_names.clone(),cx)))
            ;
        let panel_terminal_actions = crate::shell::chrome::action_bar().action(mark_action("create", crate::shell::chrome::ActionDefinition::new("new-style-create", text("_xlsx.create"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e,_,_,cx|e.create_style(cx))))).action(mark_action("rebase-apply", crate::shell::chrome::ActionDefinition::new("new-style-rebase", text("fix.style-new.change-base"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|this, _, _, cx| this.rebase_caret_style(cx)))))
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("fix.style-new.new-style"), body.into_any_element(), panel_terminal_actions))
    }
}

/// Word's font sizes, the steps of the size's ▲▼.
const FONT_STEPS: &[f32] = &[8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0, 72.0];

/// The menu ids of the document's character styles offered.
const CHAR_STYLE_IDS: [&str; 30] = [
    "docx.cstyle.1", "docx.cstyle.2", "docx.cstyle.3", "docx.cstyle.4", "docx.cstyle.5", "docx.cstyle.6", "docx.cstyle.7", "docx.cstyle.8", "docx.cstyle.9", "docx.cstyle.10",
    "docx.cstyle.11", "docx.cstyle.12", "docx.cstyle.13", "docx.cstyle.14", "docx.cstyle.15", "docx.cstyle.16", "docx.cstyle.17", "docx.cstyle.18", "docx.cstyle.19", "docx.cstyle.20",
    "docx.cstyle.21", "docx.cstyle.22", "docx.cstyle.23", "docx.cstyle.24", "docx.cstyle.25", "docx.cstyle.26", "docx.cstyle.27", "docx.cstyle.28", "docx.cstyle.29", "docx.cstyle.30",
];

/// The document's character styles a user applies (強調, 引用文字 2, …):
/// not the default one, a paragraph style's linked one (`… (文字)`,
/// `… Char`) nor the ones Word sets itself (note and comment marks, page
/// and line numbers), by the names shown.
pub(crate) fn document_character_styles(doc: &ibis_doc_model::FlowDocument) -> Vec<(String, String)> {
    const WORD_OWN: [&str; 10] = ["default paragraph font", "footnote reference", "endnote reference", "annotation reference", "page number", "line number", "placeholder text", "comment reference", "hyperlink", "followedhyperlink"];
    let mut out: Vec<(String, String)> = doc
        .styles
        .styles
        .iter()
        .filter(|(_, s)| s.kind == StyleKind::Character)
        .filter(|(_, s)| {
            let lower = s.name.to_ascii_lowercase();
            !WORD_OWN.contains(&lower.as_str()) && !s.name.ends_with(" (文字)") && !s.name.ends_with("(文字)") && !lower.ends_with(" char") // i18n-ok（Word の文字スタイル名の照合（「(文字)」の付いた名前は除く）。ファイルの中）
        })
        .map(|(id, s)| (id.clone(), super::toolbar::style_display_name(&s.name)))
        .collect();
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out.truncate(CHAR_STYLE_IDS.len());
    out
}

/// The style list's 文字スタイル submenu: the document's character styles
/// (the caret's checked) and 文字スタイルの解除.
pub(crate) fn char_style_entries(editor: &SlideEditor) -> Vec<crate::shell::chrome::MenuEntry> {
    use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
    let Some(doc) = editor.flow() else { return Vec::new() };
    let styles = document_character_styles(doc);
    // The style of the first selected character (else the one before the caret).
    let current = editor.ordered_selection().and_then(|(start, end)| {
        let at = if start != end && start.para == end.para { start.offset + 1 } else { start.offset };
        flow_command::paragraph(doc, start.para).and_then(|p| p.format_at(at).1)
    });
    let mut items: Vec<MenuEntry> = styles
        .into_iter()
        .zip(CHAR_STYLE_IDS)
        .map(|((id, label), item)| {
            let on = current.as_deref() == Some(id.as_str());
            MenuItem::new(item, label).checked(on).on(move |this, _, cx| this.apply_char_style(Some(id.clone()), cx)).into()
        })
        .collect();
    if !items.is_empty() {
        items.push(MenuEntry::Separator);
    }
    items.push(MenuItem::tr("docx.cstyle.clear").on(|this, _, cx| this.apply_char_style(None, cx)).into());
    vec![MenuEntry::Separator, Submenu::tr("docx.cstyle", "text_format", items).into()]
}

impl SlideEditor {
    /// The character style `style` (or none) on the selection, or on the
    /// word at the caret (as Word applies one).
    pub(crate) fn apply_char_style(&mut self, style: Option<String>, cx: &mut Context<Self>) {
        let Some((start, end)) = self.ordered_selection() else { return };
        let (start, end) = if start == end { self.word_at(start) } else { (start, end) };
        if start == end {
            return;
        }
        let keep = self.text_selection();
        self.run_flow(vec![FlowCommand::SetCharStyle { from: start, to: end, style }], cx);
        if let (Some((a, c)), Some(state)) = (keep, self.doc_state_mut()) {
            state.anchor = Some(a);
            state.caret = Some(c);
        }
    }
}
