// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ハイパーリンク (挿入 > リンク…, ⌘K): the text selected, or the shape
//! selected, made a link to a web page or file, or to a slide of the
//! presentation (or the next, previous, first or last), or the link taken
//! away. A link of text takes the look the file gives it (underlined, in
//! the theme's link colour: read back through the file). In the slide show
//! and editor a normal click follows it.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{CharacterFormat, Command, EmuPoint, ShapeId, ShapeKind, SlideId, SlideLink};

use crate::pptx::interaction;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

/// What the link goes on.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LinkTarget {
    /// Characters `start..end` of a shape's text.
    Text { shape: ShapeId, start: usize, end: usize },
    /// Characters `start..end` of a table cell's text.
    CellText { shape: ShapeId, row: usize, col: usize, start: usize, end: usize },
    Shape(ShapeId),
}

/// The panel's choices.
pub(crate) struct LinkPanel {
    pub(crate) target: LinkTarget,
    pub(crate) slide: SlideId,
    /// A web page or file (`true`), else a place in the presentation.
    pub(crate) web: bool,
    pub(crate) url: Entity<InputState>,
    /// The place chosen in the presentation.
    pub(crate) place: Option<SlideLink>,
    /// ScreenTip (ヒント).
    pub(crate) tooltip: Entity<InputState>,
}

/// The link of `shape` on `slide`: its own, else its text's when the text
/// has one link only.
pub(crate) fn link_of_shape(slide: &ibis_doc_model::Slide, shape: ShapeId) -> Option<SlideLink> {
    if let Some(l) = slide.shape_links.get(&shape) {
        return Some(l.clone());
    }
    let sh = slide.shape(shape)?;
    if let ShapeKind::Table(t) = &sh.kind {
        let mut links = t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.text.paragraphs.iter()).flat_map(|p| p.runs.iter()).filter_map(|r| r.style.link.clone());
        let first = links.next()?;
        return links.all(|l| l == first).then_some(first);
    }
    let body = sh.text_body()?;
    let mut links = body.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| r.style.link.clone());
    let first = links.next()?;
    links.all(|l| l == first).then_some(first)
}

/// The link tooltip of `shape` on `slide`: its own, else its text's when the text has a link tooltip.
pub(crate) fn link_tooltip_of_shape(slide: &ibis_doc_model::Slide, shape: ShapeId) -> Option<String> {
    if let Some(t) = slide.shape_link_tooltips.get(&shape) {
        return Some(t.clone());
    }
    let sh = slide.shape(shape)?;
    if let ShapeKind::Table(t) = &sh.kind {
        return t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.text.paragraphs.iter()).flat_map(|p| p.runs.iter()).find_map(|r| r.style.link_tooltip.clone());
    }
    let body = sh.text_body()?;
    body.paragraphs.iter().flat_map(|p| p.runs.iter()).find_map(|r| r.style.link_tooltip.clone())
}

/// The link of table cell `(row, col)` in `shape` on `slide`.
pub(crate) fn link_of_cell(slide: &ibis_doc_model::Slide, shape: ShapeId, row: usize, col: usize) -> Option<SlideLink> {
    let sh = slide.shape(shape)?;
    let ShapeKind::Table(t) = &sh.kind else { return None };
    let cell = t.rows.get(row)?.cells.get(col)?;
    let mut links = cell.text.paragraphs.iter().flat_map(|p| p.runs.iter()).filter_map(|r| r.style.link.clone());
    let first = links.next()?;
    links.all(|l| l == first).then_some(first)
}

/// The link tooltip of table cell `(row, col)` in `shape` on `slide`.
pub(crate) fn link_tooltip_of_cell(slide: &ibis_doc_model::Slide, shape: ShapeId, row: usize, col: usize) -> Option<String> {
    let sh = slide.shape(shape)?;
    let ShapeKind::Table(t) = &sh.kind else { return None };
    let cell = t.rows.get(row)?.cells.get(col)?;
    cell.text.paragraphs.iter().flat_map(|p| p.runs.iter()).find_map(|r| r.style.link_tooltip.clone())
}

impl SlideEditor {
    /// ⌘K: the panel for the text selected (while editing) or the shape selected.
    pub(crate) fn slide_open_link_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = match self.text_editing() {
            Some(e) => match e.cell {
                None if !e.range().is_empty() => LinkTarget::Text { shape: e.shape, start: e.range().start, end: e.range().end },
                None => LinkTarget::Shape(e.shape),
                Some((row, col)) if !e.range().is_empty() => {
                    LinkTarget::CellText { shape: e.shape, row, col, start: e.range().start, end: e.range().end }
                }
                Some((row, col)) => {
                    let len = self.edited_body().map(|b| b.text().chars().count()).unwrap_or(0);
                    LinkTarget::CellText { shape: e.shape, row, col, start: 0, end: len }
                }
            },
            None => match self.selected_ids().as_slice() {
                [one] => LinkTarget::Shape(*one),
                _ => return,
            },
        };
        let target=match target{
            LinkTarget::Shape(shape) if !self.slide().shape_links.contains_key(&shape)=>{
                self.slide().shape(shape).and_then(|s|s.text_body()).and_then(|b|{
                    let mut offset=0;
                    for p in &b.paragraphs{for r in &p.runs{
                        if r.style.link.is_some(){return text_link_span(b,offset).map(|(start,end,_)|LinkTarget::Text{shape,start,end});}
                        offset+=r.text.chars().count();
                    }offset+=1;}None
                }).unwrap_or(LinkTarget::Shape(shape))
            },other=>other,
        };
        self.slide_open_link_target(target,window,cx);
    }
    pub(crate) fn slide_open_link_target(&mut self,target:LinkTarget,window:&mut Window,cx:&mut Context<Self>){
        let slide=self.current_slide;
        let (existing, existing_tooltip) = match &target {
            LinkTarget::Shape(shape) => (link_of_shape(self.slide(), *shape), link_tooltip_of_shape(self.slide(), *shape)),
            LinkTarget::Text { shape,start,.. } => (self.slide().shape(*shape).and_then(|s|s.text_body()).and_then(|b|text_link_span(b,*start)).map(|(_,_,l)|l), link_tooltip_of_shape(self.slide(), *shape)),
            LinkTarget::CellText { shape, row, col,start,.. } => (self.slide().shape(*shape).and_then(|s|if let ShapeKind::Table(t)=&s.kind{t.rows.get(*row)?.cells.get(*col).and_then(|c|text_link_span(&c.text,*start)).map(|(_,_,l)|l)}else{None}), link_tooltip_of_cell(self.slide(), *shape, *row, *col)),
        };
        if self.text_editing().is_some() {
            self.end_text_edit(window, cx);
        }
        let url_text = match &existing {
            Some(SlideLink::Url(u)) => u.clone(),
            _ => String::new(),
        };
        let web = !matches!(existing, Some(ref l) if !matches!(l, SlideLink::Url(_)));
        let url = cx.new(|cx| InputState::new(window, cx).placeholder("https://").default_value(url_text));
        url.update(cx, |f, cx| f.focus(window, cx));
        let tooltip_text = existing_tooltip.unwrap_or_default();
        let tooltip = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text("pptx.links.screentip-text")).default_value(tooltip_text));
        let place = existing.filter(|l| !matches!(l, SlideLink::Url(_)));
        if let Some(s) = self.pptx_mut() {
            s.link_panel = Some(LinkPanel { target, slide, web, url, place, tooltip });
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn slide_close_link_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.pptx_mut() {
            s.link_panel = None;
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    /// The link chosen in the panel, if one is.
    fn slide_chosen_link(&self, cx: &App) -> Option<SlideLink> {
        let p = self.pptx()?.link_panel.as_ref()?;
        if p.web {
            let url = p.url.read(cx).value().trim().to_string();
            (!url.is_empty()).then(|| SlideLink::Url(if url.contains(':') { url } else { format!("https://{url}") }))
        } else {
            p.place.clone()
        }
    }

    /// 適用 (`link`: the panel's choice) or リンクの削除 (`None`).
    pub(crate) fn slide_apply_link(&mut self, link: Option<SlideLink>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(p) = self.pptx().and_then(|s| s.link_panel.as_ref()) else { return };
        let (target, slide) = (p.target.clone(), p.slide);
        let tooltip_str = p.tooltip.read(cx).value().trim().to_string();
        let tooltip = if link.is_some() && !tooltip_str.is_empty() {
            Some(tooltip_str)
        } else {
            None
        };
        match target {
            LinkTarget::Shape(shape) => {
                let mut cmds = Vec::new();
                if self.presentation().slide(slide).and_then(|s| s.shape_links.get(&shape)) != link.as_ref() {
                    cmds.push(Command::SetShapeLink { slide, shape, link });
                }
                if self.presentation().slide(slide).and_then(|s| s.shape_link_tooltips.get(&shape)).cloned() != tooltip {
                    cmds.push(Command::SetShapeLinkTooltip { slide, shape, tooltip });
                }
                if !cmds.is_empty() {
                    self.run(cmds, cx);
                }
            }
            LinkTarget::Text { shape, start, end } => {
                let Some(mut changed) = self.presentation().slide(slide).and_then(|s| s.shape(shape)).cloned() else { return };
                let Some(body) = changed.text_body_mut() else { return };
                // Taken away: the text as the rest of its body (no underline, its colour).
                let format = match &link {
                    Some(l) => CharacterFormat {
                        link: Some(Some(l.clone())),
                        link_tooltip: Some(tooltip),
                        ..Default::default()
                    },
                    None => CharacterFormat {
                        link: Some(None),
                        link_tooltip: Some(None),
                        underline: Some(false),
                        color: Some(body.style.color),
                        ..Default::default()
                    },
                };
                body.format_runs(start, end, &format);
                let kind = match &self.source {
                    Some(source) => {
                        let mut p = self.presentation().clone();
                        if let Some(t) = p.slides.iter_mut().find(|s| s.id == slide).and_then(|s| s.shapes.iter_mut().find(|x| x.id == shape)) {
                            *t = changed.clone();
                        }
                        match source.as_read(&p, slide, shape) {
                            Ok(read) => read.kind,
                            Err(err) => {
                                log::warn!("Could not add the link: {err}");
                                changed.kind
                            }
                        }
                    }
                    None => changed.kind,
                };
                if self.presentation().slide(slide).and_then(|s| s.shape(shape)).is_some_and(|s| s.kind != kind) {
                    self.run(vec![Command::SetKind { slide, shape, kind: Box::new(kind) }], cx);
                }
            }
            LinkTarget::CellText { shape, row, col, start, end } => {
                let Some(mut changed) = self.presentation().slide(slide).and_then(|s| s.shape(shape)).cloned() else { return };
                let ShapeKind::Table(ref mut table) = changed.kind else { return };
                let Some(cell) = table.rows.get_mut(row).and_then(|r| r.cells.get_mut(col)) else { return };
                let format = match &link {
                    Some(l) => CharacterFormat {
                        link: Some(Some(l.clone())),
                        link_tooltip: Some(tooltip),
                        underline: Some(true),
                        ..Default::default()
                    },
                    None => CharacterFormat {
                        link: Some(None),
                        link_tooltip: Some(None),
                        underline: Some(false),
                        color: Some(cell.text.style.color),
                        ..Default::default()
                    },
                };
                cell.text.format_runs(start, end, &format);
                let kind = match &self.source {
                    Some(source) => {
                        let mut p = self.presentation().clone();
                        if let Some(t) = p.slides.iter_mut().find(|s| s.id == slide).and_then(|s| s.shapes.iter_mut().find(|x| x.id == shape)) {
                            *t = changed.clone();
                        }
                        match source.as_read(&p, slide, shape) {
                            Ok(read) => read.kind,
                            Err(err) => {
                                log::warn!("Could not add the link: {err}");
                                changed.kind
                            }
                        }
                    }
                    None => changed.kind,
                };
                if self.presentation().slide(slide).and_then(|s| s.shape(shape)).is_some_and(|s| s.kind != kind) {
                    self.run(vec![Command::SetKind { slide, shape, kind: Box::new(kind) }], cx);
                }
            }
        }
        self.slide_close_link_panel(window, cx);
    }

    /// Follow `link`: a page opened in the browser, or the slide gone to
    /// (in the show, or in the editor).
    pub(crate) fn slide_follow_link(&mut self, link: &SlideLink, window: &mut Window, cx: &mut Context<Self>) {
        let slides = &self.presentation().slides;
        let index = self.presentation().slide_index(self.current_slide).unwrap_or(0);
        let to = match link {
            SlideLink::Url(url) => {
                crate::shell::links::open_external_url(url,cx);
                return;
            }
            SlideLink::Slide(id) => slides.iter().position(|s| s.id == *id),
            SlideLink::NextSlide => (index + 1 < slides.len()).then_some(index + 1),
            SlideLink::PreviousSlide => index.checked_sub(1),
            SlideLink::FirstSlide => Some(0),
            SlideLink::LastSlide => Some(slides.len() - 1),
            SlideLink::LastSlideViewed => index.checked_sub(1),
            SlideLink::EndShow => None,
        };
        let Some(to) = to else { return };
        self.show_slide_number(to + 1, window, cx);
        cx.notify();
    }

    /// The link under `p` (slide coordinates) of the slide shown, if any.
    pub(crate) fn slide_link_at(&self, p: EmuPoint) -> Option<SlideLink> {
        self.slide_shape_and_link_at(p).map(|(_, l)| l)
    }

    /// The shape and its link under `p` (slide coordinates) of the slide shown, if any.
    pub(crate) fn slide_shape_and_link_at(&self, p: EmuPoint) -> Option<(ibis_doc_model::ShapeId, SlideLink)> {
        self.slide_link_target_at(p).map(|(target,link)|{
            let shape=match target{LinkTarget::Shape(id)|LinkTarget::Text{shape:id,..}|LinkTarget::CellText{shape:id,..}=>id};(shape,link)
        })
    }
    /// Use the same character positions that painted the text, including rotation.
    pub(crate) fn slide_link_target_at(&self,p:EmuPoint)->Option<(LinkTarget,SlideLink)>{
        let slide=self.slide();let hit=slide.hit_test(p);let layout=self.slide_layout()?;
        let x=f32::from(layout.origin.x)+p.x as f32*layout.scale;let y=f32::from(layout.origin.y)+p.y as f32*layout.scale;
        let cache=self.link_text_positions.borrow();
        // Text may extend past the authored shape/row frame. Its painted
        // characters remain clickable, while a shape painted above blocks it.
        for sh in slide.shown_shapes().collect::<Vec<_>>().into_iter().rev(){
            let shape=sh.id;
            if hit==Some(shape){if let Some(link)=slide.shape_links.get(&shape){return Some((LinkTarget::Shape(shape),link.clone()));}}
            let mut slots=cache.iter().filter(|((id,_),_)|*id==shape).collect::<Vec<_>>();
            slots.sort_by_key(|((_,cell),_)|std::cmp::Reverse(*cell));
            for ((_,cell),positions)in slots{
                let body=match (&sh.kind,cell){(ShapeKind::Table(t),Some((row,col)))=>&t.rows.get(*row)?.cells.get(*col)?.text,_=>sh.text_body()?};
                let mut offset=0;
                for(paragraph,slot)in body.paragraphs.iter().zip(positions.paragraphs.borrow().iter()){
                    let q=slot.borrow();let(tx,ty)=q.text_point(x,y);
                    for(i,c)in q.chars.iter().enumerate(){let line=q.lines.get(c.line)?;
                        if tx>=c.x.min(c.x+c.advance)&&tx<c.x.max(c.x+c.advance)&&ty>=line.top&&ty<line.top+line.height{
                            let(start,end,link)=text_link_span(body,offset+i)?;
                            let target=match cell{Some((row,col))=>LinkTarget::CellText{shape,row:*row,col:*col,start,end},None=>LinkTarget::Text{shape,start,end}};return Some((target,link));
                        }
                    }
                    offset+=paragraph.runs.iter().map(|r|r.text.chars().count()).sum::<usize>()+1;
                }
            }
            if hit==Some(shape){return None;}
        }None
    }

    pub(crate) fn link_panel_set_mode(&mut self, web: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.link_panel.as_mut()) {
            p.web = web;
        }
        cx.notify();
    }

    pub(crate) fn link_panel_select_place(&mut self, place: SlideLink, cx: &mut Context<Self>) {
        if let Some(p) = self.pptx_mut().and_then(|s| s.link_panel.as_mut()) {
            p.web = false;
            p.place = Some(place);
        }
        cx.notify();
    }

    pub(crate) fn link_panel_copy_address(&mut self, cx: &mut Context<Self>) {
        let Some(link) = self.slide_chosen_link(cx) else { return };
        let text_to_copy = match link {
            SlideLink::Url(u) => u,
            SlideLink::Slide(id) => {
                let idx = self.presentation().slides.iter().position(|s| s.id == id).unwrap_or(0);
                format!("{} {}", crate::shell::i18n::text("_shell.slide"), idx + 1)
            }
            SlideLink::FirstSlide => crate::shell::i18n::text("pptx.links.first-slide").to_string(),
            SlideLink::LastSlide => crate::shell::i18n::text("pptx.links.last-slide").to_string(),
            SlideLink::NextSlide => crate::shell::i18n::text("pptx.links.next-slide").to_string(),
            SlideLink::PreviousSlide => crate::shell::i18n::text("pptx.links.previous-slide").to_string(),
            SlideLink::LastSlideViewed => crate::shell::i18n::text("pptx.links.last-slide-viewed").to_string(),
            SlideLink::EndShow => crate::shell::i18n::text("pptx.links.end-show").to_string(),
        };
        if !text_to_copy.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(text_to_copy));
        }
    }

    pub(crate) fn link_panel_follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(link) = self.slide_chosen_link(cx) {
            self.slide_follow_link(&link, window, cx);
        }
    }

    pub(crate) fn link_panel_context_menu_entries(&self, cx: &App) -> Vec<crate::shell::chrome::MenuEntry> {
        use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
        let Some(p) = self.pptx().and_then(|s| s.link_panel.as_ref()) else {
            return Vec::new();
        };
        let t = crate::shell::i18n::text;
        let web = p.web;
        let place = p.place.clone();
        let has_chosen = self.slide_chosen_link(cx).is_some();

        let mut place_items: Vec<MenuEntry> = vec![
            MenuItem::tr("pptx.links.first-slide")
                .checked(place == Some(SlideLink::FirstSlide))
                .on(|this, _, cx| this.link_panel_select_place(SlideLink::FirstSlide, cx))
                .into(),
            MenuItem::tr("pptx.links.last-slide")
                .checked(place == Some(SlideLink::LastSlide))
                .on(|this, _, cx| this.link_panel_select_place(SlideLink::LastSlide, cx))
                .into(),
            MenuItem::tr("pptx.links.next-slide")
                .checked(place == Some(SlideLink::NextSlide))
                .on(|this, _, cx| this.link_panel_select_place(SlideLink::NextSlide, cx))
                .into(),
            MenuItem::tr("pptx.links.previous-slide")
                .checked(place == Some(SlideLink::PreviousSlide))
                .on(|this, _, cx| this.link_panel_select_place(SlideLink::PreviousSlide, cx))
                .into(),
            MenuItem::tr("pptx.links.last-slide-viewed")
                .checked(place == Some(SlideLink::LastSlideViewed))
                .on(|this, _, cx| this.link_panel_select_place(SlideLink::LastSlideViewed, cx))
                .into(),
            MenuItem::tr("pptx.links.end-show")
                .checked(place == Some(SlideLink::EndShow))
                .on(|this, _, cx| this.link_panel_select_place(SlideLink::EndShow, cx))
                .into(),
            MenuEntry::Separator,
        ];

        for (i, s) in self.presentation().slides.iter().enumerate() {
            let sid = s.id;
            let title = s
                .shapes
                .iter()
                .find(|sh| s.placeholders.get(&sh.id).is_some_and(|ph| matches!(ph.placeholder.normalized(), "title" | "ctrTitle")))
                .and_then(|sh| sh.text_body())
                .map(|b| b.text().replace('\n', " "))
                .unwrap_or_default();
            let label = format!("{} {}. {title}", t("_shell.slide"), i + 1);
            let item_id = match i {
                0 => "pptx.links.slide.1",
                1 => "pptx.links.slide.2",
                2 => "pptx.links.slide.3",
                3 => "pptx.links.slide.4",
                4 => "pptx.links.slide.5",
                5 => "pptx.links.slide.6",
                6 => "pptx.links.slide.7",
                7 => "pptx.links.slide.8",
                8 => "pptx.links.slide.9",
                9 => "pptx.links.slide.10",
                _ => "pptx.links.slide",
            };
            place_items.push(
                MenuItem::new(item_id, label)
                    .checked(place == Some(SlideLink::Slide(sid)))
                    .on(move |this, _, cx| this.link_panel_select_place(SlideLink::Slide(sid), cx))
                    .into(),
            );
        }

        vec![
            MenuItem::tr("_shell.apply")
                .icon("check")
                .on(|this, w, cx| {
                    if let Some(link) = this.slide_chosen_link(cx) {
                        this.slide_apply_link(Some(link), w, cx);
                    }
                })
                .into(),
            MenuItem::tr("_shell.view")
                .icon("open_in_new")
                .disabled(!has_chosen)
                .on(|this, w, cx| this.link_panel_follow(w, cx))
                .into(),
            MenuItem::tr("edit.copy")
                .icon("content_copy")
                .disabled(!has_chosen)
                .on(|this, _, cx| this.link_panel_copy_address(cx))
                .into(),
            MenuItem::tr("pptx.links.remove-link")
                .icon("link_off")
                .on(|this, w, cx| this.slide_apply_link(None, w, cx))
                .into(),
            MenuEntry::Separator,
            MenuItem::tr("pptx.links.web-page-file")
                .checked(web)
                .on(|this, _, cx| this.link_panel_set_mode(true, cx))
                .into(),
            MenuItem::tr("pptx.links.place-document")
                .checked(!web)
                .on(|this, _, cx| this.link_panel_set_mode(false, cx))
                .into(),
            Submenu::new("pptx.links.place-submenu", "slideshow", t("pptx.links.place-document"), place_items).into(),
            MenuEntry::Separator,
            MenuItem::tr("_shell.close")
                .icon("close")
                .on(|this, w, cx| this.slide_close_link_panel(w, cx))
                .into(),
        ]
    }

    pub(crate) fn open_link_panel_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_text_edit(window, cx);
        let items = self.link_panel_context_menu_entries(cx);
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

    pub(crate) fn slide_link_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.pptx()?.link_panel.as_ref()?;
        let t = crate::shell::i18n::text;
        let web = p.web;
        let place = p.place.clone();
        let tab = |id: &'static str, label: &'static str, on: bool, web: bool| {
            crate::shell::chrome::segment_control::SegmentItem::label(id, t(label), on).on_click(cx.listener(move |e, _, _, cx| {
                e.link_panel_set_mode(web, cx);
            }))
        };
        let mut body = v_flex()
            .id("pptx-link-panel")
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .on_mouse_down(MouseButton::Right, cx.listener(|this, event: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.open_link_panel_context_menu(event.position, window, cx);
            }))
            .child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic).child(tab("link-web", "pptx.links.web-page-file", web, true)).child(tab("link-place", "pptx.links.place-document", !web, false)));
        if web {
            body = body.child(div().text_sm().child(t("pptx.links.address"))).child(crate::shell::chrome::text_field(&p.url).full_width());
        } else {
            let mut places: Vec<(SlideLink, String)> = vec![
                (SlideLink::FirstSlide, t("pptx.links.first-slide").to_string()),
                (SlideLink::LastSlide, t("pptx.links.last-slide").to_string()),
                (SlideLink::NextSlide, t("pptx.links.next-slide").to_string()),
                (SlideLink::PreviousSlide, t("pptx.links.previous-slide").to_string()),
                (SlideLink::LastSlideViewed, t("pptx.links.last-slide-viewed").to_string()),
                (SlideLink::EndShow, t("pptx.links.end-show").to_string()),
            ];
            for (i, s) in self.presentation().slides.iter().enumerate() {
                let title = s.shapes.iter().find(|sh| s.placeholders.get(&sh.id).is_some_and(|ph| matches!(ph.placeholder.normalized(), "title" | "ctrTitle"))).and_then(|sh| sh.text_body()).map(|b| b.text().replace('\n', " ")).unwrap_or_default();
                places.push((SlideLink::Slide(s.id), format!("{} {}. {title}", t("_shell.slide"), i + 1)));
            }
            let list = v_flex().gap_0p5().children(places.into_iter().enumerate().map(|(i, (link, label))| {
                let on = place.as_ref() == Some(&link);
                let link_click = link.clone();
                let link_right = link.clone();
                div()
                    .id(("link-place-item", i))
                    .px_1()
                    .rounded(px(3.))
                    .text_sm()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgb(0xd3e3fd)))
                    .on_click(cx.listener(move |e, _, _, cx| {
                        e.link_panel_select_place(link_click.clone(), cx);
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |e, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        e.link_panel_select_place(link_right.clone(), cx);
                        e.open_link_panel_context_menu(event.position, window, cx);
                    }))
                    .child(label)
            }));
            body = body.child(list);
        }
        body = body.child(div().text_sm().child(t("pptx.links.screentip"))).child(crate::shell::chrome::text_field(&p.tooltip).full_width());
        let panel_actions = crate::shell::chrome::action_bar()
                .action(crate::shell::chrome::ActionDefinition::new("link-apply", t("_shell.apply"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|e, _, w, cx| {
                    if let Some(link) = e.slide_chosen_link(cx) {
                        e.slide_apply_link(Some(link), w, cx);
                    }
                })))
                .action(crate::shell::chrome::ActionDefinition::new("link-cancel", t("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|e, _, w, cx| e.slide_close_link_panel(w, cx))))
                .when(match &p.target {LinkTarget::Shape(id)=>link_of_shape(self.slide(),*id).is_some(),LinkTarget::Text{shape,start,..}=>self.slide().shape(*shape).and_then(|s|s.text_body()).and_then(|b|text_link_span(b,*start)).is_some(),LinkTarget::CellText{shape,row,col,start,..}=>self.slide().shape(*shape).and_then(|s|if let ShapeKind::Table(t)=&s.kind{t.rows.get(*row)?.cells.get(*col).and_then(|c|text_link_span(&c.text,*start))}else{None}).is_some()},|bar|bar.action(crate::shell::chrome::ActionDefinition::new("link-remove", t("pptx.links.remove-link"), crate::shell::chrome::ActionButtonKind::Destructive).on_click(cx.listener(|e, _, w, cx| e.slide_apply_link(None, w, cx)))));
        Some(SidebarPanel::with_footer(t("pptx.links.hyperlink"), body.into_any_element(), panel_actions))
    }
}


impl SlideEditor {
    pub(crate) fn slide_hyperlink_context_entry(&self)->crate::shell::chrome::MenuEntry{
        use crate::shell::chrome::MenuItem;
        let link=self.selected_ids().first().and_then(|id|link_of_shape(self.slide(),*id));
        let Some(link)=link else{return MenuItem::tr("pptx.link").icon("link").shortcut("⌘K").on(|e,w,cx|e.slide_open_link_panel(w,cx)).into();};
        self.slide_hyperlink_menu_for(LinkTarget::Shape(self.selected_ids()[0]),link)
    }
    pub(crate) fn slide_hyperlink_menu_for(&self,target:LinkTarget,link:SlideLink)->crate::shell::chrome::MenuEntry{
        use crate::shell::chrome::{MenuItem,Submenu};
        let edit=target.clone();
        Submenu::tr("hyperlink.menu","link",vec![
            MenuItem::tr("hyperlink.open").on(move|e,w,cx|e.slide_follow_link(&link,w,cx)).into(),
            MenuItem::tr("hyperlink.edit").on(move|e,w,cx|e.slide_open_link_target(edit.clone(),w,cx)).into(),
            MenuItem::tr("hyperlink.remove").on(move|e,w,cx|{e.slide_open_link_target(target.clone(),w,cx);e.slide_apply_link(None,w,cx);}).into(),
        ]).into()
    }
}

/// Character span of the link at a text position; adjacent styled runs retain one target.
fn text_link_span(body:&ibis_doc_model::TextBody,at:usize)->Option<(usize,usize,SlideLink)>{
    let mut offset=0;let mut spans=Vec::new();
    for p in &body.paragraphs{for r in &p.runs{let end=offset+r.text.chars().count();spans.push((offset,end,r.style.link.clone()));offset=end;}offset+=1;}
    let i=spans.iter().position(|(a,b,_)|*a<=at&&at<*b)?;let link=spans[i].2.clone()?;let(mut a,mut b)=(i,i);
    while a>0&&spans[a-1].1==spans[a].0&&spans[a-1].2.as_ref()==Some(&link){a-=1;}
    while b+1<spans.len()&&spans[b].1==spans[b+1].0&&spans[b+1].2.as_ref()==Some(&link){b+=1;}
    Some((spans[a].0,spans[b].1,link))
}
