// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Compact chart styles and palette samples, sharing the standard dropdown trigger.
use super::chart_edit::{CHART_STYLES, COLOR_PALETTES};
use crate::shell::{chrome, editor::SlideEditor, palette::rgb};
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
impl SlideEditor {
    pub(crate) fn chart_style_dropdown(&self, cx: &Context<Self>) -> AnyElement {
        let chart = self.sheet_chart().unwrap().1;
        let selected = CHART_STYLES
            .iter()
            .position(|(_, s)| s.matches(chart));
        let entries = CHART_STYLES
            .iter()
            .map(|(key, style)| {
                let style = *style;
                chrome::MenuItem::tr(key)
                    .checked(style.matches(chart))
                    .on(move |e, _, cx| e.set_sheet_chart_visual_style(style, cx))
                    .into()
            })
            .collect();
        chrome::Dropdown::from_entries("chart-style-dropdown",crate::shell::i18n::text(selected.map_or("chart.ui.style-custom",|i|CHART_STYLES[i].0)),entries,true,0,&cx.weak_entity()).button_width(chrome::FIELD_WIDTH+px(chrome::BUTTON_HEIGHT)).into_any_element()
    }
    pub(crate) fn chart_palette_dropdown(&self, cx: &Context<Self>) -> AnyElement {
        let p = self.wb().chart_edit.as_ref().unwrap();
        let chart = self.sheet_chart().unwrap().1;
        let colors = chart
            .groups
            .iter()
            .flat_map(|g| &g.series)
            .map(|s| match &s.fill {
                ibis_doc_model::Fill::Solid(c) => c.rgb.0,
                _ => s.stroke.as_ref().map_or(0xdddddd, |s| s.color.rgb.0),
            })
            .take(5)
            .collect::<Vec<_>>();
        let matches = |p: &[u32]| !colors.is_empty() && colors.iter().enumerate().all(|(i,c)|*c==p[i%p.len()]);
        let selected = p.palette_selected.filter(|i|COLOR_PALETTES.get(*i).is_some_and(|(_,p)|matches(p))).or_else(|| COLOR_PALETTES.iter().position(|(_, p)| {
            !colors.is_empty() && colors.iter().enumerate().all(|(i, c)| *c == p[i % p.len()])
        }));
        let choices=COLOR_PALETTES.iter().enumerate().map(|(i,(key,colors))|chrome::dropdown::ColorPaletteItem {
            label:crate::shell::i18n::text(key),colors:colors.to_vec(),
            on_select:std::rc::Rc::new(move|e,_,cx|e.set_sheet_chart_color_palette(i,cx)),
        }).collect();
        chrome::ColorPaletteDropdown::new("chart-palette-dropdown",choices,selected.unwrap_or(0),&cx.weak_entity()).fill_width().into_any_element()
    }
}
