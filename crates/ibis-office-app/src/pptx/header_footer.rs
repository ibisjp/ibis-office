// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ヘッダーとフッター (挿入 > ヘッダーとフッター…): the date, the slide
//! number and the footer put on the slide (適用) or on every slide (すべてに
//! 適用) as their layouts' placeholders, or taken away; タイトル スライドに
//! 表示しない leaves them off slides of a title layout. Their place and look
//! are the layout's (the file's reading of them, when there is a file).
//! すべてに適用 also marks the presentation (see `HeaderFooterMarks`).

use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{AutoShape, Command, Fill, Geometry, Run, RunStyle, Shape, ShapeKind, Slide, SlideId, SlidePlaceholder, TextBody, TextField};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// The panel's choices.
pub(crate) struct HeaderFooterPanel {
    pub(crate) date: bool,
    /// 自動的に更新: the date a field (today's when shown), else the text typed.
    pub(crate) auto_date: bool,
    pub(crate) date_text: Entity<InputState>,
    pub(crate) number: bool,
    pub(crate) footer: bool,
    pub(crate) footer_text: Entity<InputState>,
    pub(crate) not_on_title: bool,
}

/// The kinds of placeholder the panel puts on: date, footer, slide number.
const KINDS: [&str; 3] = ["dt", "ftr", "sldNum"];

/// A slide's placeholder of `kind`, if it has one.
fn placeholder_of<'a>(slide: &'a Slide, kind: &str) -> Option<&'a Shape> {
    slide.shapes.iter().find(|s| slide.placeholders.get(&s.id).is_some_and(|p| p.placeholder.kind.as_deref() == Some(kind)))
}

/// Today as PowerPoint's fixed date shows it (2026/10/7).
pub(crate) fn today() -> String {
    // The computer's calendar (not UTC's: in Japan before 9 o'clock that is yesterday).
    let now = crate::clock::local_now();
    format!("{}/{}/{}", now.year, now.month, now.day)
}

/// A text body of one run: `text`, a field if `field`.
fn body(text: &str, field: Option<TextField>) -> TextBody {
    let mut b = TextBody::plain("", Default::default());
    b.paragraphs[0].runs = vec![Run { text: text.into(), style: RunStyle { field, ..Default::default() } }];
    b
}

/// The date field 自動的に更新 puts on: PowerPoint's first form, in the
/// UI's language.
fn date_field() -> TextField {
    TextField::DateTime { format: 1, east_asian: crate::shell::i18n::language() == crate::shell::i18n::Language::Japanese }
}

impl SlideEditor {
    /// 挿入 > ヘッダーとフッター…: the panel, as the current slide has them.
    pub(crate) fn open_header_footer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.end_text_edit(window, cx);
        let slide = self.slide().clone();
        let marks = self.presentation().header_footer;
        let text_of = |kind: &str| placeholder_of(&slide, kind).and_then(|s| s.text_body()).map(|b| b.text());
        let date_text = text_of("dt").filter(|t| !t.is_empty()).unwrap_or_else(today);
        let footer_text = text_of("ftr").unwrap_or_default();
        let (date, footer, number) = (placeholder_of(&slide, "dt").is_some(), placeholder_of(&slide, "ftr").is_some(), placeholder_of(&slide, "sldNum").is_some());
        let auto_date = placeholder_of(&slide, "dt").and_then(|s| s.text_body()).is_none_or(|b| b.paragraphs.iter().flat_map(|p| &p.runs).any(|r| matches!(r.style.field, Some(TextField::DateTime { .. }))));
        let date_text = cx.new(|cx| InputState::new(window, cx).default_value(date_text));
        let footer_text = cx.new(|cx| InputState::new(window, cx).default_value(footer_text));
        if let Some(s) = self.pptx_mut() {
            s.timing_panel = None; s.section_panel = None; s.gradient_panel = None; s.shadow_panel = None; s.glow_panel = None; s.paragraph_panel = None; s.find = None; s.size_panel = None; s.insets_panel = None; s.crop_panel = None; s.frame_panel = None; s.comments = None; s.chart_panel = None; s.notes_marks_panel = None;
            s.header_footer = Some(HeaderFooterPanel { date, auto_date, date_text, number, footer, footer_text, not_on_title: !marks.on_title_slides });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_header_footer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.header_footer = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// A check box of the panel changed.
    pub(crate) fn toggle_header_footer(&mut self, which: &str, on: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.header_footer.as_mut()) {
            match which {
                "date" => p.date = on,
                "auto" => p.auto_date = on,
                "number" => p.number = on,
                "footer" => p.footer = on,
                _ => p.not_on_title = on,
            }
        }
        cx.notify();
    }

    pub(crate) fn clear_header_footer(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.header_footer.as_mut()) {
            p.date = false;
            p.number = false;
            p.footer = false;
            p.not_on_title = false;
        }
        cx.notify();
    }

    /// 適用 (`all`: すべてに適用): the panel's choices on the slides.
    pub(crate) fn apply_header_footer(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.header_footer.as_ref()) else { return };
        let auto_date = p.auto_date;
        let today = ibis_doc_model::date_field::slide_date_text(1, date_field() == TextField::DateTime { format: 1, east_asian: true }, &crate::clock::local_now()).unwrap_or_default();
        let wanted = [
            p.date.then(|| if auto_date { today.clone() } else { p.date_text.read(cx).value().to_string() }),
            p.footer.then(|| p.footer_text.read(cx).value().to_string()),
            p.number.then(|| "‹#›".to_string()),
        ];
        let not_on_title = p.not_on_title;
        // すべてに適用 marks the presentation as PowerPoint does (the header
        // is the notes' and handouts', not offered here: not shown).
        let marks = all.then(|| ibis_doc_model::model::HeaderFooterMarks { on_title_slides: !not_on_title, header: false, date: p.date, footer: p.footer, number: p.number });
        let targets: Vec<SlideId> = if all { self.presentation().slides.iter().map(|s| s.id).collect() } else { vec![self.current_slide] };
        let mut current = self.presentation().clone();
        let mut changed: Vec<(SlideId, Vec<ibis_doc_model::ShapeId>)> = Vec::new();
        for id in targets {
            let Some(slide) = current.slide(id).cloned() else { continue };
            let layout = slide.layout.and_then(|l| current.layout(l)).cloned();
            let title = layout.as_ref().is_some_and(|l| l.placeholders.iter().any(|p| p.placeholder.kind.as_deref() == Some("ctrTitle")));
            let mut s = slide.clone();
            let mut new_ids = Vec::new();
            for (kind, want) in KINDS.iter().zip(&wanted) {
                let want = if not_on_title && title { None } else { want.clone() };
                let existing = placeholder_of(&s, kind).map(|x| x.id);
                match (existing, want) {
                    (Some(e), None) => {
                        s.shapes.retain(|x| x.id != e);
                        s.placeholders.remove(&e);
                    }
                    (Some(e), Some(text)) if *kind == "dt" => {
                        // A field (自動的に更新) or the text typed.
                        let field = auto_date.then(date_field);
                        if let Some(b) = s.shapes.iter_mut().find(|x| x.id == e).and_then(|x| x.text_body_mut()) {
                            let runs: Vec<&Run> = b.paragraphs.iter().flat_map(|p| &p.runs).collect();
                            let same = runs.len() == 1 && runs[0].text == text && runs[0].style.field == field;
                            if !same {
                                let style = b.paragraphs.first().and_then(|p| p.runs.first()).map(|r| r.style.clone()).unwrap_or_default();
                                b.paragraphs.truncate(1);
                                b.paragraphs[0].runs = vec![Run { text: text.clone(), style: RunStyle { field, ..style } }];
                            }
                        }
                    }
                    (Some(e), Some(text)) if *kind != "sldNum" => {
                        if let Some(b) = s.shapes.iter_mut().find(|x| x.id == e).and_then(|x| x.text_body_mut())
                            && b.text() != text
                        {
                            b.set_text(&text);
                        }
                    }
                    (None, Some(text)) => {
                        // The layout's placeholder of that kind, its place.
                        let Some(lp) = layout.as_ref().and_then(|l| l.placeholders.iter().find(|p| p.placeholder.kind.as_deref() == Some(*kind))) else { continue };
                        let frame = layout.as_ref().and_then(|l| l.placement_for(&lp.placeholder)).map(|p| p.0).unwrap_or_default();
                        let new_id = self.document.ids().new_shape_id();
                        let field = match *kind {
                            "sldNum" => Some(TextField::SlideNumber),
                            "dt" if auto_date => Some(date_field()),
                            _ => None,
                        };
                        let auto = AutoShape { text: Some(body(&text, field)), ..AutoShape::new(Geometry::Rect, Fill::None, None) };
                        s.shapes.push(Shape::new(new_id, frame, ShapeKind::Auto(auto)));
                        s.placeholders.insert(new_id, SlidePlaceholder { placeholder: lp.placeholder.clone(), own_frame: false });
                        new_ids.push(new_id);
                    }
                    _ => {}
                }
            }
            if s != slide {
                *current.slides.iter_mut().find(|x| x.id == id).expect("the slide") = s;
                changed.push((id, new_ids));
            }
        }
        // The new ones as the file reads them (their layout's look).
        if let Some(source) = self.source.clone() {
            let read_from = current.clone();
            for (slide, ids) in &changed {
                for id in ids {
                    match source.as_read(&read_from, *slide, *id) {
                        Ok(read) => {
                            if let Some(target) = current.slides.iter_mut().find(|x| x.id == *slide).and_then(|x| x.shapes.iter_mut().find(|sh| sh.id == *id)) {
                                *target = read;
                            }
                        }
                        Err(err) => log::warn!("Could not add the header and footer: {err}"),
                    }
                }
            }
        }
        let mut commands: Vec<Command> = changed.iter().filter_map(|(id, _)| current.slide(*id).cloned()).map(|slide| Command::SetSlide { slide }).collect();
        if let Some(marks) = marks.filter(|m| *m != self.presentation().header_footer) {
            commands.push(Command::SetHeaderFooterMarks { marks });
        }
        if !commands.is_empty() {
            self.run(commands, cx);
        }
        self.close_header_footer(window, cx);
    }

    pub(crate) fn header_footer_context_menu_entries(&self) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem};
        let Some(p) = self.pptx().and_then(|s| s.header_footer.as_ref()) else {
            return Vec::new();
        };
        let t = crate::shell::i18n::text;
        let date = p.date;
        let auto_date = p.auto_date;
        let number = p.number;
        let footer = p.footer;
        let not_on_title = p.not_on_title;

        vec![
            MenuItem::tr("_shell.apply")
                .icon("check")
                .on(|this, w, cx| this.apply_header_footer(false, w, cx))
                .into(),
            MenuItem::tr("pptx.notes-marks-panel.apply-all")
                .icon("done_all")
                .on(|this, w, cx| this.apply_header_footer(true, w, cx))
                .into(),
            MenuItem::tr("pptx.find.clear")
                .icon("clear")
                .on(|this, _, cx| this.clear_header_footer(cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.date-time")
                .checked(date)
                .on(move |this, _, cx| this.toggle_header_footer("date", !date, cx))
                .into(),
            MenuItem::tr("pptx.notes-marks-panel.update-automatically")
                .checked(auto_date)
                .on(move |this, _, cx| this.toggle_header_footer("auto", !auto_date, cx))
                .into(),
            MenuItem::tr("pptx.header-footer.slide-number")
                .checked(number)
                .on(move |this, _, cx| this.toggle_header_footer("number", !number, cx))
                .into(),
            MenuItem::tr("_shell.footer")
                .checked(footer)
                .on(move |this, _, cx| this.toggle_header_footer("footer", !footer, cx))
                .into(),
            MenuItem::tr("pptx.header-footer.don-t-show-title-slide")
                .checked(not_on_title)
                .on(move |this, _, cx| this.toggle_header_footer("title", !not_on_title, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::new("pptx.notes-header-footer", t("pptx.notes-marks-panel.notes-header-footer"))
                .on(|this, w, cx| this.open_notes_marks(w, cx))
                .into(),
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|this, w, cx| this.close_header_footer(w, cx))
                .into(),
        ]
    }

    pub(crate) fn open_header_footer_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_text_edit(window, cx);
        let items = self.header_footer_context_menu_entries();
        if items.is_empty() {
            return;
        }
        self.focus_handle.focus(window, cx);
        let entries = std::rc::Rc::new(items);
        let editor = cx.entity().downgrade();
        let menu = gpui_kit::component::menu::PopupMenu::build(window, cx, move |menu, window, cx| {
            crate::shell::chrome::fill_menu(menu.min_w(px(220.)), &entries, &editor, window, cx)
        });
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(s) = this.pptx_mut() {
                s.slide_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        if let Some(s) = self.pptx_mut() {
            s.slide_popup = Some(super::SlidePopup { menu, position, _dismiss: dismiss });
        }
        cx.notify();
    }

    pub(crate) fn header_footer_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.header_footer.as_ref()?;
        let t = crate::shell::i18n::text;
        let check = |id: &'static str, label: &'static str, on: bool, which: &'static str| {
            Checkbox::new(id).label(t(label)).checked(on).on_click(cx.listener(move |this, on: &bool, _, cx| this.toggle_header_footer(which, *on, cx)))
        };
        let body = v_flex()
            .id("pptx-header-footer-panel")
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_header_footer_context_menu(event.position, window, cx);
            }))
            .child(check("pptx-hf-date", "_shell.date-time", p.date, "date"))
            .child(check("pptx-hf-auto", "pptx.notes-marks-panel.update-automatically", p.auto_date, "auto"))
            .child(crate::shell::chrome::text_field(&p.date_text).full_width().disabled(p.auto_date))
            .child(check("pptx-hf-number", "pptx.header-footer.slide-number", p.number, "number"))
            .child(check("pptx-hf-footer", "_shell.footer", p.footer, "footer"))
            .child(crate::shell::chrome::text_field(&p.footer_text).full_width())
            .child(check("pptx-hf-title", "pptx.header-footer.don-t-show-title-slide", p.not_on_title, "title"))
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("pptx-hf-apply", t("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_header_footer(false, w, cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("pptx-hf-all", t("pptx.notes-marks-panel.apply-all"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| e.apply_header_footer(true, w, cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("pptx-hf-cancel", t("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.close_header_footer(w, cx))));
        Some(SidebarPanel::with_footer(t("_shell.header-footer"), body.into_any_element(), panel_actions))
    }
}
