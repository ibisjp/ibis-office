// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The settings of a chartEx chart (a waterfall's totals, a histogram's bins) are changed, the chart made again from its
//! cells, and written back into its part.
use ibis_doc_model::{BinBy, Chart, Number, Presentation, ShapeKind};

fn chart_of(p: &Presentation) -> Chart {
    p.slides.iter().flat_map(|s| &s.shapes).find_map(|sh| match &sh.kind { ShapeKind::Chart(c) => Some((**c).clone()), _ => None }).expect("a chart")
}

fn with_chart(p: &mut Presentation, chart: Chart) {
    for sh in p.slides.iter_mut().flat_map(|s| s.shapes.iter_mut()) {
        if matches!(sh.kind, ShapeKind::Chart(_)) {
            sh.kind = ShapeKind::Chart(Box::new(chart.clone()));
        }
    }
}

fn open(file: &str) -> ibis_formats::Opened {
    ibis_formats::open(std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../test/editing/").to_string() + file).unwrap()).unwrap()
}

#[test]
fn a_waterfall_takes_another_point_as_a_total_and_saves_it() {
    let opened = open("xlsx-chartex-waterfall.xlsx");
    let mut p = opened.presentation.clone();
    let chart = chart_of(&p);
    let source = chart.ex_source.clone().expect("the chart has its source");
    assert_eq!(source.0.options().subtotals, vec![5]);
    let totals = |c: &Chart| c.groups[0].series.iter().find(|s| s.name == "合計").map(|s| s.values.iter().map(|v| v.map(|n| n.0)).collect::<Vec<_>>()).unwrap();
    assert_eq!(totals(&chart), vec![None, None, None, None, None, Some(-12.0)]);
    // The first point (the opening balance) is a total too.
    let mut options = source.0.options();
    options.subtotals = vec![0, 5];
    let changed = Chart { ex_source: source.0.with_options(&options), ..chart.clone() };
    with_chart(&mut p, changed);
    ibis_doc_model::chart_refresh::refresh_charts(&mut p);
    let made = chart_of(&p);
    assert_eq!(totals(&made), vec![Some(100.0), None, None, None, None, Some(-12.0)]);
    // Saved, and read again: the part holds both.
    let saved = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();
    let again = ibis_formats::open(saved).unwrap();
    let back = chart_of(&again.presentation);
    assert_eq!(back.ex_source.unwrap().0.options().subtotals, vec![0, 5]);
    assert_eq!(totals(&chart_of(&again.presentation)), vec![Some(100.0), None, None, None, None, Some(-12.0)]);
}

#[test]
fn a_histogram_takes_a_bin_width_and_saves_it() {
    let opened = open("xlsx-chartex-histogram.xlsx");
    let mut p = opened.presentation.clone();
    let chart = chart_of(&p);
    let source = chart.ex_source.clone().expect("the chart has its source");
    let bins = |c: &Chart| c.groups[0].series[0].categories.clone();
    let before = bins(&chart);
    assert_eq!(before.len(), 4);
    let mut options = source.0.options();
    assert!(options.closed_right);
    options.bin_by = BinBy::Width(Number(10.0));
    let changed = Chart { ex_source: source.0.with_options(&options), ..chart.clone() };
    with_chart(&mut p, changed);
    ibis_doc_model::chart_refresh::refresh_charts(&mut p);
    let made = chart_of(&p);
    assert_ne!(bins(&made), before);
    assert_eq!(bins(&made).len(), 5, "52 to 100 in tens");
    let saved = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();
    let again = ibis_formats::open(saved).unwrap();
    let back = chart_of(&again.presentation);
    assert_eq!(back.ex_source.as_ref().unwrap().0.options().bin_by, BinBy::Width(Number(10.0)));
    assert_eq!(bins(&back), bins(&made));
}

/// A histogram with a bin width, an underflow and an overflow bin set, for opening in Excel (`OFFICE_OUT`: a folder).
#[test]
fn a_histogram_with_every_bin_setting_is_written() {
    let opened = open("xlsx-chartex-histogram.xlsx");
    let mut p = opened.presentation.clone();
    let chart = chart_of(&p);
    let source = chart.ex_source.clone().unwrap();
    let mut options = source.0.options();
    options.bin_by = BinBy::Width(Number(10.0));
    options.underflow = Some(Number(60.0));
    options.overflow = Some(Number(90.0));
    let changed = Chart { ex_source: source.0.with_options(&options), ..chart.clone() };
    with_chart(&mut p, changed);
    ibis_doc_model::chart_refresh::refresh_charts(&mut p);
    let made = chart_of(&p);
    // Under 60, the tens from 60 to 90, over 90.
    assert!(made.groups[0].series[0].categories.len() >= 4, "{:?}", made.groups[0].series[0].categories);
    let saved = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();
    if let Ok(dir) = std::env::var("OFFICE_OUT") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("ibis-chartex-histogram.xlsx"), &saved).unwrap();
    }
    let again = ibis_formats::open(saved).unwrap();
    let back = chart_of(&again.presentation).ex_source.unwrap().0.options();
    assert_eq!((back.bin_by, back.underflow, back.overflow), (BinBy::Width(Number(10.0)), Some(Number(60.0)), Some(Number(90.0))));
}

#[test]
fn a_box_whisker_changes_display_and_quartile_settings_and_saves() {
    let opened = open("xlsx-chartex-boxwhisker.xlsx");
    let mut p = opened.presentation.clone();
    let chart = chart_of(&p);
    let source = chart.ex_source.clone().expect("box whisker has ex_source");
    let initial_options = source.0.options();
    assert!(initial_options.box_mean_marker);
    assert!(!initial_options.box_mean_line);
    assert!(initial_options.box_outliers);
    assert!(!initial_options.box_inclusive);

    let mut options = initial_options;
    options.box_mean_marker = false;
    options.box_mean_line = true;
    options.box_outliers = false;
    options.box_inclusive = true;

    let changed = Chart { ex_source: source.0.with_options(&options), ..chart.clone() };
    with_chart(&mut p, changed);
    ibis_doc_model::chart_refresh::refresh_charts(&mut p);

    let made = chart_of(&p);
    if let Some(ibis_doc_model::ChartEx::Box { mean_marker, mean_line, outliers, .. }) = &made.ex {
        assert!(!*mean_marker);
        assert!(*mean_line);
        assert!(!*outliers);
    } else {
        panic!("expected ChartEx::Box");
    }

    let saved = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();
    let again = ibis_formats::open(saved).unwrap();
    let back = chart_of(&again.presentation);
    let back_options = back.ex_source.unwrap().0.options();
    assert!(!back_options.box_mean_marker);
    assert!(back_options.box_mean_line);
    assert!(!back_options.box_outliers);
    assert!(back_options.box_inclusive);
}

#[test]
fn a_treemap_toggles_overlapping_parent_labels_and_saves() {
    let opened = open("xlsx-chartex-treemap.xlsx");
    let mut p = opened.presentation.clone();
    let chart = chart_of(&p);
    let source = chart.ex_source.clone().expect("treemap has ex_source");
    let initial_options = source.0.options();
    assert!(initial_options.treemap_overlapping);

    let mut options = initial_options;
    options.treemap_overlapping = false;

    let changed = Chart { ex_source: source.0.with_options(&options), ..chart.clone() };
    with_chart(&mut p, changed);
    ibis_doc_model::chart_refresh::refresh_charts(&mut p);

    let made = chart_of(&p);
    if let Some(ibis_doc_model::ChartEx::Treemap { overlapping, .. }) = &made.ex {
        assert!(!*overlapping);
    } else {
        panic!("expected ChartEx::Treemap with overlapping=false");
    }

    let saved = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();
    let again = ibis_formats::open(saved).unwrap();
    let back = chart_of(&again.presentation);
    let back_options = back.ex_source.unwrap().0.options();
    assert!(!back_options.treemap_overlapping);
}

#[test]
fn a_funnel_changes_gap_width_and_saves() {
    let opened = open("xlsx-chartex-funnel.xlsx");
    let mut p = opened.presentation.clone();
    let chart = chart_of(&p);
    let source = chart.ex_source.clone().expect("funnel has ex_source");
    let initial_options = source.0.options();
    assert_eq!(initial_options.gap_width, Some(6));

    let mut options = initial_options;
    options.gap_width = Some(25);

    let changed = Chart { ex_source: source.0.with_options(&options), ..chart.clone() };
    with_chart(&mut p, changed);
    ibis_doc_model::chart_refresh::refresh_charts(&mut p);

    let made = chart_of(&p);
    if let ibis_doc_model::ChartType::Bar { gap_width, .. } = &made.groups[0].kind {
        assert_eq!(*gap_width, 25);
    } else {
        panic!("expected Bar chart for Funnel with gap_width=25");
    }

    let saved = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();
    let again = ibis_formats::open(saved).unwrap();
    let back = chart_of(&again.presentation);
    let back_options = back.ex_source.unwrap().0.options();
    assert_eq!(back_options.gap_width, Some(25));
    if let ibis_doc_model::ChartType::Bar { gap_width, .. } = &back.groups[0].kind {
        assert_eq!(*gap_width, 25);
    } else {
        panic!("expected Bar chart for reloaded Funnel with gap_width=25");
    }
}

#[test]
fn a_pareto_changes_gap_width_and_saves() {
    let opened = open("xlsx-chartex-pareto.xlsx");
    let mut p = opened.presentation.clone();
    let chart = chart_of(&p);
    let source = chart.ex_source.clone().expect("pareto has ex_source");
    let initial_options = source.0.options();
    assert_eq!(initial_options.gap_width, Some(0));

    let mut options = initial_options;
    options.gap_width = Some(40);

    let changed = Chart { ex_source: source.0.with_options(&options), ..chart.clone() };
    with_chart(&mut p, changed);
    ibis_doc_model::chart_refresh::refresh_charts(&mut p);

    let made = chart_of(&p);
    if let ibis_doc_model::ChartType::Bar { gap_width, .. } = &made.groups[0].kind {
        assert_eq!(*gap_width, 40);
    } else {
        panic!("expected Bar chart for Pareto with gap_width=40");
    }

    let saved = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();
    let again = ibis_formats::open(saved).unwrap();
    let back = chart_of(&again.presentation);
    let back_options = back.ex_source.unwrap().0.options();
    assert_eq!(back_options.gap_width, Some(40));
    if let ibis_doc_model::ChartType::Bar { gap_width, .. } = &back.groups[0].kind {
        assert_eq!(*gap_width, 40);
    } else {
        panic!("expected Bar chart for reloaded Pareto with gap_width=40");
    }
}

#[test]
fn a_chart_ex_changes_title_and_legend_and_saves() {
    use ibis_doc_model::chart::{ChartText, ChartTitle, Legend, LegendPosition, Side};

    let opened = open("xlsx-chartex-waterfall.xlsx");
    let mut p = opened.presentation.clone();
    let chart = chart_of(&p);
    let source = chart.ex_source.clone().expect("the chart has its source");

    // 1. Change title text, legend position to Bottom, and also toggle opening balance as subtotal.
    let mut options = source.0.options();
    options.subtotals = vec![0, 5];
    let mut changed = Chart { ex_source: source.0.with_options(&options), ..chart.clone() };
    changed.title = Some(ChartTitle {
        text: "損益ウォーターフォール".into(),
        style: ChartText { size: 1400, ..ChartText::default() },
        at: None,
    });
    changed.legend = Some(Legend {
        source_position: Some(LegendPosition::Bottom),
        side: Side::Bottom,
        overlay: Some(false),
        layout: None,
        text: ChartText { size: 900, ..ChartText::default() },
        hidden: Vec::new(),
    });
    with_chart(&mut p, changed);
    ibis_doc_model::chart_refresh::refresh_charts(&mut p);

    let saved = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();
    let again = ibis_formats::open(saved).unwrap();
    let back = chart_of(&again.presentation);
    assert_eq!(back.title.as_ref().map(|t| t.text.as_str()), Some("損益ウォーターフォール"));
    assert_eq!(back.legend.as_ref().and_then(|l| l.source_position), Some(LegendPosition::Bottom));
    assert_eq!(back.ex_source.unwrap().0.options().subtotals, vec![0, 5]);

    // 2. Hide both title and legend, and save again from the original source.
    let mut hidden = chart_of(&p);
    hidden.title = None;
    hidden.legend = None;
    with_chart(&mut p, hidden);

    let saved_hidden = opened.source.save(&p, ibis_formats::SaveOptions::default()).unwrap();
    let again_hidden = ibis_formats::open(saved_hidden).unwrap();
    let back_hidden = chart_of(&again_hidden.presentation);
    assert!(back_hidden.title.is_none());
    assert!(back_hidden.legend.is_none());
}


