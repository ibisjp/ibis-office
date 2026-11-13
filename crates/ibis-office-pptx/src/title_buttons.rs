// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The title row's buttons of the presentation screen (`FormatEditor::title_mode_button`): the one that closes the mode being in
//! (slide master, notes master, handout master, notes formatting) and the slide show's with its start menu. They were in
//! `shell/chrome.rs`.

#![allow(unused_imports)]

use std::rc::Rc;

use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Selectable as _, Sizable as _, h_flex, v_flex};

use crate::shell::chrome::*;
use crate::shell::chrome::{MenuEntry, MenuItem};
use crate::shell::icons::Material;
use crate::shell::editor::SlideEditor;
use crate::shell::palette::rgb;
use crate::ext::*;

#[ibis_office_macros::editor_ext(TitleButtons)]
impl SlideEditor {
    /// スライドショー, with ▼ for how to start; `icon_only` leaves out its
    /// label (in the toolbar's title row).
    /// The title row's button of the presentation screen: the one that closes the mode being in (slide master, notes
    /// master, handout master, notes formatting), else the slide show's.
    pub(crate) fn render_slideshow_or_close_button(&self, editor: &WeakEntity<SlideEditor>, icon_only: bool) -> AnyElement {
        let close_mode: Option<(&'static str, Handler)> = if self.is_master_view() {
            Some(("pptx.master-view.close-master-view", Rc::new(|this: &mut SlideEditor, window, cx| this.close_master_view(window, cx))))
        } else if self.is_page_view() {
            // The notes master closes as the slide master does; the notes page and the handout master are "format editing".
            let label = if self.is_notes_master() { "pptx.master-view.close-master-view" } else { "pptx.notes-format.close-format-editing" };
            Some((label, Rc::new(|this: &mut SlideEditor, window, cx| this.close_notes_format(window, cx))))
        } else {
            None
        };
        match close_mode {
            Some((label, close)) => self.render_close_mode_button(editor, label, close),
            None => self.render_slideshow_button(editor, icon_only),
        }
    }

    /// 「マスター表示を閉じる」「書式編集を閉じる」: the shell's pill of the title row that closes the mode (`close_mode_button`).
    pub(crate) fn render_close_mode_button(&self, editor: &WeakEntity<SlideEditor>, label: &'static str, close: Handler) -> AnyElement {
        crate::shell::chrome::close_mode_button("pptx-master-title-close", label, editor, &close)
    }

    pub(crate) fn render_slideshow_button(&self, editor: &WeakEntity<SlideEditor>, icon_only: bool) -> AnyElement {
        let disabled = self.loading;
        let start: Handler = Rc::new(|this: &mut SlideEditor, window, cx| this.start_slideshow(window, cx));
        let from_start: Handler = Rc::new(|this: &mut SlideEditor, window, cx| {
            this.show_slide_number(1, window, cx);
            this.start_slideshow(window, cx);
        });
        let mut items: Vec<MenuEntry> = vec![
            MenuItem::tr("slideshow.from-start").icon("play_arrow").shortcut("⇧⌘↩").on(move |e, w, c| from_start(e, w, c)).into(),
            MenuItem::tr("slideshow.from-current").icon("slideshow").shortcut("⌘↩").on({
                let start = start.clone();
                move |e, w, c| start(e, w, c)
            })
            .into(),
            MenuItem::tr("slideshow.presenter").icon("co_present").shortcut("⌥⌘↩").on(|e, w, c| e.start_presenter_view(w, c)).into(),
        ];
        let custom_shows = self.presentation().custom_shows.clone();
        if !custom_shows.is_empty() {
            let mut sub_items: Vec<MenuEntry> = Vec::new();
            for cs in custom_shows {
                let id = cs.id;
                sub_items.push(MenuItem::new("slideshow.custom-show-item", cs.name).icon("filter_none").on(move |e, w, c| e.start_custom_slideshow(id, w, c)).into());
            }
            items.push(Submenu::tr("slideshow.custom-shows", "filter_none", sub_items).into());
        }
        items.push(MenuEntry::Separator);
        items.push(MenuItem::tr("slideshow.rehearse-timings").icon("schedule").on(|e, w, c| e.start_rehearsal(w, c)).into());
        items.push(MenuItem::tr("slideshow.settings").icon("settings").on(|e, w, c| e.open_show_panel(w, c)).into());
        let theme_border = rgb(0x747775);
        let caret_editor = editor.clone();
        h_flex()
            .h(px(30.))
            .rounded(px(15.))
            .border_1()
            .border_color(theme_border)
            .bg(crate::shell::palette::white())
            .when(disabled, |d| d.opacity(0.5))
            .child(
                div()
                    .id("slideshow")
                    .h_full()
                    .pl(px(if icon_only { 8. } else { 10. }))
                    .pr(px(if icon_only { 6. } else { 8. }))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .rounded_l(px(15.))
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .cursor_pointer()
                    .hover(|s| s.bg(crate::shell::palette::shade(0.0, 0.04)))
                    .child(icon("slideshow", 18.0, rgb(ICON).into()))
                    .when(!icon_only, |d| d.child(crate::shell::i18n::text("_shell.slide-show")))
                    .when(!disabled, |d| d.on_click(listener(editor, &start)))
                    .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new(crate::shell::i18n::text("_shell.play-from-current-slide")).build(window, cx)),
            )
            .child(div().w(px(1.)).h(px(18.)).bg(crate::shell::palette::shade(0.0, 0.15)))
            .child(
                Button::new("slideshow-caret")
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(Material("arrow_drop_down")))
                    .tooltip(crate::shell::i18n::text("_shell.choose-how-to-start"))
                    .disabled(disabled)
                    .rounded_r(px(15.))
                    .dropdown_menu(move |menu, window, cx| fill_menu(menu.min_w(px(300.)), &items, &caret_editor, window, cx)),
            )
            .into_any_element()
    }
}
