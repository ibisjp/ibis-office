// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! One palette-selection dialog and one custom-color editor for all three formats.
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::{h_flex,v_flex};
use crate::shell::{chrome, editor::SlideEditor, i18n::text};
use ibis_doc_model::Rgb;

#[derive(Default)]
pub(crate) struct ThemeColorsUi {
    pub editing: bool,
    pub scroll: ScrollHandle,
}
impl SlideEditor {
    fn reopen_theme_colors(&mut self, w:&mut Window,cx:&mut Context<Self>) {
        if self.is_document() { self.open_docx_theme_panel(w,cx); }
        else if self.presentation().workbook.is_some() { self.open_xlsx_theme_panel(w,cx); }
        else { self.open_theme_panel(w,cx); }
    }
    pub(crate) fn set_theme_colors_editor(&mut self, editing:bool,w:&mut Window,cx:&mut Context<Self>) {
        // Rebuild the draft on both entry and cancellation; discarded edits never leak.
        self.reopen_theme_colors(w,cx);
        self.theme_colors_ui.editing=editing;
        self.panel_dialog=Default::default();
        cx.notify();
    }
    pub(crate) fn shared_theme_colors_panel(&mut self,cx:&mut Context<Self>)->Option<chrome::SidebarPanel> {
        let document=self.is_document();let workbook=self.presentation().workbook.is_some();
        let fields=if document {self.doc_state()?.theme_panel.as_ref()?.colors.clone()}
            else if workbook {self.sheet_state()?.panels.theme.as_ref()?.colors.clone()}
            else {self.pptx()?.theme_panel.as_ref()?.colors.clone()};
        if self.theme_colors_ui.editing {
            let mut body=chrome::panel_form();
            for (index,(_,label,input)) in fields.iter().enumerate() {
                let id=["theme-color-0","theme-color-1","theme-color-2","theme-color-3","theme-color-4","theme-color-5","theme-color-6","theme-color-7","theme-color-8","theme-color-9","theme-color-10","theme-color-11"][index];
                body=body.child(chrome::color_field_row(text(label),chrome::panel_color_field(id,input,cx)));
            }
            let error=if document {self.doc_state()?.theme_panel.as_ref()?.error.clone()}
                else if workbook {self.sheet_state()?.panels.theme.as_ref()?.error.clone()}
                else {self.pptx()?.theme_panel.as_ref()?.error.clone()};
            if let Some(error)=error {body=body.child(div().text_color(crate::shell::palette::rgb(0xb3261e)).child(error));}
            let actions=chrome::action_bar()
                .action(chrome::ActionDefinition::new("theme-custom-apply",text("_shell.apply"),chrome::ActionButtonKind::Primary)
                    .on_click(cx.listener(|e,_,w,cx| {
                        if e.is_document(){e.apply_docx_theme_panel(w,cx);}else if e.presentation().workbook.is_some(){e.apply_xlsx_theme_panel(w,cx);}else{e.apply_theme_panel(w,cx);}
                        // Validation failures keep the draft editor open.
                        let failed=if e.is_document(){e.doc_state().and_then(|s|s.theme_panel.as_ref()).is_some()}
                            else if e.presentation().workbook.is_some(){e.sheet_state().and_then(|s|s.panels.theme.as_ref()).is_some()}
                            else {e.pptx().and_then(|s|s.theme_panel.as_ref()).is_some()};
                        if !failed {e.set_theme_colors_editor(false,w,cx);}
                    })))
                .action(chrome::ActionDefinition::new("theme-custom-cancel",text("_shell.cancel"),chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|e,_,w,cx|e.set_theme_colors_editor(false,w,cx))));
            return Some(chrome::SidebarPanel::with_footer(text("theme.custom-editor"),body.into_any_element(),actions));
        }
        let have:Vec<_>=fields.iter().filter_map(|(name,_,input)|u32::from_str_radix(input.read(cx).value().trim_start_matches('#'),16).ok().map(|c|(*name,Rgb(c)))).collect();
        let mut presets:Vec<(&'static str,&'static str,Vec<(&'static str,Rgb)>)>=if document {
            crate::docx::editing::DOCX_THEME_COLORS_PRESETS.iter().map(|p|(p.id,p.label_key,p.colors.to_vec())).collect()
        } else {crate::pptx::theme_panel::THEME_PALETTES.iter().map(|p|(p.id,p.label_key,p.colors.to_vec())).collect()};
        // A custom document palette remains a visible, selected entry after editing.
        if !presets.iter().any(|(_,_,colors)|colors.iter().all(|(n,c)|have.iter().any(|(hn,hc)|hn==n&&hc==c))) {
            presets.push(("document-custom","theme.custom-scheme",have.clone()));
        }
        let mut rows=v_flex().w_full();
        for (id,label,colors) in presets {
            let selected=colors.iter().all(|(n,c)|have.iter().any(|(hn,hc)|hn==n&&hc==c));
            let preview=h_flex().gap_1().children(colors.iter().filter(|(n,_)|!matches!(*n,"hlink"|"folHlink"|"dk1"|"lt1")).take(8).map(|(_,c)|div().size(px(18.)).flex_none().bg(crate::shell::palette::rgb(c.0)).border_1().border_color(crate::shell::palette::rgb(chrome::BORDER))));
            let check=div().w(px(20.)).flex_none().when(selected,|d|d.child(chrome::icon("check",16.,crate::shell::palette::rgb(0x0b57d0).into())));
            let content=h_flex().items_center().gap_2().child(check).child(preview).child(div().flex_1().min_w_0().child(text(label)));
            rows=rows.child(div().id(SharedString::from(format!("theme-palette-{id}"))).cursor_pointer()
                .child(chrome::table_row_with_actions(vec![content.into_any_element()],div().into_any_element(),selected,px(0.)))
                .on_click(cx.listener(move|e,_,w,cx| {
                    if e.is_document(){e.apply_docx_theme_colors(id,cx);}else if e.presentation().workbook.is_some(){e.apply_xlsx_theme_palette(id,cx);}else{e.apply_theme_palette(id,w,cx);}
                    e.reopen_theme_colors(w,cx);cx.notify();
                })));
        }
        let body=chrome::panel_form().child(h_flex().w_full().justify_end().child(chrome::action_button("theme-add").label(text("ui.add")).on_click(cx.listener(|e,_,w,cx|e.set_theme_colors_editor(true,w,cx)))))
            .child(chrome::table("theme-palette-table",h_flex().px_2().h(px(28.)).items_center().child(text("theme.palette-list")).into_any_element(),rows.into_any_element(),self.theme_colors_ui.scroll.clone(),px(300.)));
        let actions=chrome::action_bar().action(chrome::dialog_close_definition(cx));
        Some(chrome::SidebarPanel::with_footer(text("theme.palette-list"),body.into_any_element(),actions))
    }
}
