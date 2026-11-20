// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0032: 挿入 > グラフ is one part on the three screens: the same kinds in the same order, the same pictures where no cells are chosen (the kinds
//! of chartEx too: Pareto, sunburst…), drawn with one function.
use crate::shell::chrome::MenuEntry;
use crate::shell::chart_insert::{blank_presentation_chart, chart_ex_items_with_thumbs, chart_items_with_thumbs, chart_with_preview_data};
use ibis_formats::NewChartKind;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::k0032_the_sheet_menu_has_the_kinds_of_the_other_screens", the_sheet_menu_has_the_kinds_of_the_other_screens),
        ("xlsx::k0032_no_cells_chosen_the_pictures_are_the_fixed_ones_of_every_kind", fixed_pictures_of_every_kind),
    ]
}

fn ids(entries: &[MenuEntry]) -> Vec<&'static str> {
    entries.iter().filter_map(|e| if let MenuEntry::Item(i) = e { Some(i.id) } else { None }).collect()
}

fn the_sheet_menu_has_the_kinds_of_the_other_screens() {
    let sheet = super::super::chart_insert::chart_items();
    let sheet_ids = ids(&sheet);
    // The presentation's and the document's menus are these same lists (`shell::chart_insert`); the sheet's has 「おすすめグラフ」 before them.
    let nop_insert: fn(&mut crate::shell::editor::SlideEditor, NewChartKind, &mut gpui_kit::Window, &mut gpui_kit::Context<crate::shell::editor::SlideEditor>) = |_, _, _, _| {};
    let nop_thumb: fn(&crate::shell::editor::SlideEditor, NewChartKind, f32) -> gpui_kit::AnyElement = |_, _, _| gpui_kit::IntoElement::into_any_element(gpui_kit::div());
    let base = chart_items_with_thumbs(nop_insert, Some(nop_thumb));
    let ex = chart_ex_items_with_thumbs(nop_insert, nop_thumb);
    let mut expected = vec!["xlsx.recommended-charts"];
    expected.extend(ids(&base));
    expected.extend(ids(&ex));
    assert_eq!(sheet_ids, expected);
    assert!(sheet_ids.contains(&"pptx.insert-chart.pareto") && sheet_ids.contains(&"pptx.insert-chart.sunburst"));
}

/// The picture of every kind, with no cells chosen, is a chart that has something to draw (the Pareto chart and the sunburst chart were
/// empty pictures on the sheet).
fn fixed_pictures_of_every_kind() {
    use NewChartKind as K;
    for kind in [K::ClusteredColumn, K::Pie, K::Scatter, K::Line, K::ComboColumnLine, K::Waterfall, K::Histogram, K::Pareto, K::Funnel, K::BoxWhisker, K::Treemap, K::Sunburst] {
        let chart = blank_presentation_chart(kind).unwrap_or_else(|| panic!("{kind:?} is made"));
        let chart = chart_with_preview_data(chart, kind);
        assert!(chart.groups.iter().any(|g| g.series.iter().any(|s| !s.values.is_empty())) || chart.ex.is_some(), "{kind:?} has data to draw");
        assert!(chart.title.is_none() && chart.legend.is_none(), "{kind:?}: no title or legend in a picture");
    }
}
