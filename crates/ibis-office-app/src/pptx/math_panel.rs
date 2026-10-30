// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 挿入 > 数式… (PPTX-MATH-001, 011, 013): a formula typed in UnicodeMath's linear form (`a/b`, `x^2`, `√(x+1)`,
//! `∑_(i=1)^n x`, `■(a&b@c&d)`) is read into a tree, shown typeset, and inserted as a text box of its own.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::math::{Math, parse_latex, parse_linear};
use ibis_doc_model::{EmuRect, Paragraph, Run, RunStyle, Shape, ShapeKind, TextAlign, TextBody, TextStyle};

use crate::pptx::SlidePopup;
use crate::shell::chrome::{MenuEntry, MenuItem, SidebarPanel, text_secondary};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) use crate::shell::math_editor::{MathFormat, MathEditorState as MathPanel};
use crate::shell::math_editor::{MathDialogSpec, math_dialog};

impl SlideEditor {
    pub(crate) fn open_pptx_math_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let editor = MathPanel::new("", window, cx, |e, _, cx| e.apply_pptx_math_panel(false, cx));
        if let Some(s) = self.pptx_mut() {
            s.ink = None;
            s.autocorrect = None;
            s.math_panel = Some(editor);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Opens the Math panel pre-populated with the currently selected shape's formula, if any.
    pub(crate) fn open_pptx_math_panel_for_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let initial = self.selected_math().map(|m| m.linear());
        self.open_pptx_math_panel(window, cx);
        if let Some(initial) = initial
            && let Some(input) = self.pptx().and_then(|s| s.math_panel.as_ref()).map(|p| p.input.clone())
        {
            input.update(cx, |st, cx| st.set_value(initial, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn set_pptx_math_format(&mut self, format: MathFormat, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.math_panel.as_mut()) {
            p.format = format;
        }
        cx.notify();
    }

    pub(crate) fn copy_pptx_math_panel_latex(&mut self, cx: &mut Context<Self>) {
        let latex = self
            .pptx()
            .and_then(|s| s.math_panel.as_ref())
            .and_then(|p| {
                let val = p.input.read(cx).value().to_string();
                let trimmed = val.trim();
                if trimmed.is_empty() {
                    return None;
                }
                let m = if p.format == MathFormat::Latex || trimmed.starts_with('\\') {
                    parse_latex(trimmed)
                } else {
                    parse_linear(trimmed)
                };
                Some(m.to_latex())
            })
            .or_else(|| self.selected_math().map(|m| m.to_latex()));
        if let Some(latex) = latex {
            cx.write_to_clipboard(ClipboardItem::new_string(latex));
            self.set_status(crate::shell::i18n::text("pptx.math.latex-copied").to_string(), cx);
        }
    }

    pub(crate) fn math_panel_context_menu_entries(&self, cx: &App) -> Vec<MenuEntry> {
        let (format, has_typed) = self
            .pptx()
            .and_then(|s| s.math_panel.as_ref())
            .map_or((MathFormat::UnicodeMath, false), |p| (p.format, !p.input.read(cx).value().trim().is_empty()));
        let has_text_shape = self.selected_shapes().iter().any(|s| s.text_body().is_some());
        vec![
            MenuItem::tr("pptx.math.insert")
                .icon("functions")
                .disabled(!has_typed)
                .on(|this, _, cx| {
                    let typed = this.pptx().and_then(|s| s.math_panel.as_ref()).map(|p| p.input.read(cx).value().to_string());
                    if let Some(t) = typed
                        && this.insert_pptx_math(&t, cx).is_some()
                        && let Some(s) = this.pptx_mut()
                    {
                        s.math_panel = None;
                    }
                    cx.notify();
                })
                .into(),
            MenuItem::tr("pptx.math.insert-inline")
                .icon("short_text")
                .disabled(!has_typed || !has_text_shape)
                .on(|this, _, cx| {
                    let typed = this.pptx().and_then(|s| s.math_panel.as_ref()).map(|p| p.input.read(cx).value().to_string());
                    if let Some(t) = typed
                        && this.insert_pptx_math_inline(&t, cx).is_some()
                        && let Some(s) = this.pptx_mut()
                    {
                        s.math_panel = None;
                    }
                    cx.notify();
                })
                .into(),
            MenuItem::tr("pptx.math.copy-latex")
                .icon("content_copy")
                .disabled(!has_typed && self.selected_math().is_none())
                .on(|this, _, cx| this.copy_pptx_math_panel_latex(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.math.format-unicodemath")
                .checked(format == MathFormat::UnicodeMath)
                .on(|this, _, cx| this.set_pptx_math_format(MathFormat::UnicodeMath, cx))
                .into(),
            MenuItem::tr("pptx.math.format-latex")
                .checked(format == MathFormat::Latex)
                .on(|this, _, cx| this.set_pptx_math_format(MathFormat::Latex, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|this, _, cx| {
                    if let Some(s) = this.pptx_mut() {
                        s.math_panel = None;
                    }
                    cx.notify();
                })
                .into(),
        ]
    }

    pub(crate) fn open_math_panel_context_menu(&mut self, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(self.math_panel_context_menu_entries(cx));
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

    /// The formula (typed in the panel) in a text box in the middle of the slide.
    pub(crate) fn insert_pptx_math(&mut self, text: &str, cx: &mut Context<Self>) -> Option<ibis_doc_model::ShapeId> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let format = self.pptx().and_then(|s| s.math_panel.as_ref()).map(|p| p.format).unwrap_or_default();
        let math = if format == MathFormat::Latex || text.starts_with('\\') {
            parse_latex(text)
        } else {
            parse_linear(text)
        };
        Some(self.insert_math(math, cx))
    }

    /// The formula in the line of the selected shape's text, at its end (行内の数式): `None` when no text shape is selected.
    pub(crate) fn insert_pptx_math_inline(&mut self, text: &str, cx: &mut Context<Self>) -> Option<ibis_doc_model::ShapeId> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        let shape = self.selected_shapes().into_iter().find(|s| s.text_body().is_some())?;
        let (id, mut body) = (shape.id, shape.text_body()?.clone());
        let format = self.pptx().and_then(|s| s.math_panel.as_ref()).map(|p| p.format).unwrap_or_default();
        let math = if format == MathFormat::Latex || text.starts_with('\\') {
            parse_latex(text)
        } else {
            parse_linear(text)
        };
        let paragraph = body.paragraphs.last_mut()?;
        let size = paragraph.runs.last().map_or(body.style.font_size, |r| r.style.font_size.unwrap_or(body.style.font_size));
        paragraph.runs.push(Run { text: math.linear(), style: RunStyle { font_size: Some(size), font: Some("Cambria Math".to_string()), math: Some(Box::new(math)), math_inline: true, ..Default::default() } });
        self.run(vec![ibis_doc_model::Command::SetTextBody { slide: self.current_slide, shape: id, body }], cx);
        Some(id)
    }

    /// Returns the first math formula found in the currently selected shape, if any.
    pub(crate) fn selected_math(&self) -> Option<Math> {
        for shape in self.selected_shapes() {
            if let Some(body) = shape.text_body() {
                for p in &body.paragraphs {
                    for r in &p.runs {
                        if let Some(m) = &r.style.math {
                            return Some((**m).clone());
                        }
                    }
                }
            }
        }
        None
    }

    /// Copies the currently selected formula as LaTeX to the clipboard.
    pub(crate) fn copy_selected_math_as_latex(&mut self, cx: &mut Context<Self>) {
        if let Some(math) = self.selected_math() {
            let latex = math.to_latex();
            cx.write_to_clipboard(ClipboardItem::new_string(latex));
            self.set_status(crate::shell::i18n::text("pptx.math.latex-copied").to_string(), cx);
        }
    }

    pub(crate) fn insert_math(&mut self, math: Math, cx: &mut Context<Self>) -> ibis_doc_model::ShapeId {
        let id = self.document.ids().new_shape_id();
        let size = self.presentation().size_of(self.slide());
        let width = (6_096_000i64).min(size.width * 8 / 10);
        let height = 1_422_400i64;
        let frame = EmuRect::new((size.width - width) / 2, (size.height - height) / 2, width, height);
        let mut style = TextStyle::default().with_font_size(3200);
        style.align = TextAlign::Center;
        let mut body = TextBody::default();
        body.style = style;
        body.paragraphs = vec![Paragraph {
            align: Some(TextAlign::Center),
            runs: vec![Run {
                text: math.linear(),
                style: RunStyle { font_size: Some(3200), font: Some("Cambria Math".to_string()), math: Some(Box::new(math)), ..Default::default() },
            }],
            ..Default::default()
        }];
        self.insert_shape(Shape::new(id, frame, ShapeKind::TextBox(body)), cx);
        id
    }

    pub(crate) fn pptx_math_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let panel = self.pptx()?.math_panel.as_ref()?;
        let format = panel.format;
        let input = panel.input.clone();
        let empty = input.read(cx).value().trim().is_empty();
        let has_text_shape = self.selected_shapes().iter().any(|s| s.text_body().is_some());
        let insert = crate::shell::chrome::ActionDefinition::new("math-insert", text("pptx.math.insert"), crate::shell::chrome::ActionButtonKind::Primary)
            .disabled(empty)
            .on_click(cx.listener(|e, _, _, cx| e.apply_pptx_math_panel(false, cx)));
        let commands = vec![
            crate::shell::chrome::ActionDefinition::new("math-copy-latex", text("pptx.math.copy-latex"), crate::shell::chrome::ActionButtonKind::Normal)
                .disabled(empty).on_click(cx.listener(|e, _, _, cx| e.copy_pptx_math_panel_latex(cx))),
            crate::shell::chrome::ActionDefinition::new("math-insert-inline", text("pptx.math.insert-inline"), crate::shell::chrome::ActionButtonKind::Normal)
                .disabled(empty || !has_text_shape)
                .on_click(cx.listener(|e, _, _, cx| e.apply_pptx_math_panel(true, cx))),
        ];
        Some(math_dialog(MathDialogSpec {
            input, format,
            set_format: |e, f, cx| e.set_pptx_math_format(f, cx),
            options: Vec::new(), error: None,
            context_menu: Some(|e, p, w, cx| e.open_math_panel_context_menu(p, w, cx)),
            insert, commands,
        }, cx))
    }

    fn apply_pptx_math_panel(&mut self, inline: bool, cx: &mut Context<Self>) {
        let Some(value) = self.pptx().and_then(|s| s.math_panel.as_ref()).map(|p| p.input.read(cx).value().to_string()) else { return };
        let inserted = if inline { self.insert_pptx_math_inline(&value, cx) } else { self.insert_pptx_math(&value, cx) };
        if inserted.is_some() && let Some(s) = self.pptx_mut() { s.math_panel = None; }
        cx.notify();
    }
}
