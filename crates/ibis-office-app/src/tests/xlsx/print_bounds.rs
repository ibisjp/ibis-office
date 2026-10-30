// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

#[test]
fn print_shaped_width_is_stable_in_sheet_points_at_different_print_scales() {
    let (_platform, mut app) = crate::export::headless_app();
    app.allow_parking();
    app.update(|cx| {
        let res = crate::render::shape_view::Resources::new(&ibis_doc_model::Presentation::new(), cx);
        let text_system = gpui_kit::WindowTextSystem::new(std::sync::Arc::clone(res.text_system()));
        let mut conditions = 0;
        for name in ["Arial", "Calibri", "Yu Gothic", "Aptos Narrow"] {
            if !res.has_typeface(name) { continue }
            for size in [9.0, 11.0, 14.0, 20.0] {
                for text in ["OVERFLOW wide text 0123456789", "Typography AV fi ellipsis...", "日本語の印刷文字幅"] { // i18n-ok（試験の入力の和文）
                    let font = gpui_kit::font(name);
                    let unit = ibis_pptx::EXCEL_METRICS.pixel_pt() as f32;
                    let widths: Vec<_> = [0.5, 1.0, 1.3].into_iter().map(|scale| {
                        (super::printed_shaped_width(&text_system, text, &font, size, scale) / unit).round() as i64
                    }).collect();
                    assert_eq!(widths[0], widths[1], "{name} {size}pt, 50% and 100%: {text}");
                    assert_eq!(widths[1], widths[2], "{name} {size}pt, 100% and 130%: {text}");
                    conditions += 1;
                }
            }
        }
        assert!(conditions >= 12, "at least one actual installed typeface must be measured");
    });
}

#[test]
fn print_regions_cache_reuses_and_invalidates_for_edits_fonts_scale_and_release() {
    use std::sync::Arc;
    use ibis_doc_model::{CellKind, CellStyle, CellValue, Sheet, SheetCell};
    let (_platform, mut app) = crate::export::headless_app();
    app.allow_parking();
    app.update(|cx| {
        super::release_layouts();
        let presentation = ibis_doc_model::Presentation::new();
        let mut res = crate::render::shape_view::Resources::new(&presentation, cx);
        res.print = true;
        let text = "A long printed string that runs through empty columns".to_string();
        let mut sheet = Arc::new(Sheet {
            columns: vec![20 * 12_700; 2], rows: vec![20 * 12_700], default_column: 20 * 12_700, default_row: 20 * 12_700,
            styles: vec![CellStyle { font: Some("Arial".into()), font_size: 1100, ..CellStyle::default() }],
            cells: vec![SheetCell { row: 0, column: 0, value: CellValue::Text(text.clone()), text, kind: CellKind::Text, formula: None, style: 0, phonetic: None, runs: None }].into(),
            ..Sheet::default()
        });
        let cached = || super::PRINT_REGIONS.with(|cache| cache.borrow().len());
        let before = super::printed_text_regions(&sheet, &res);
        assert!(!before.is_empty());
        assert_eq!(super::printed_text_regions(&sheet, &res), before);
        assert_eq!(cached(), 1);
        assert_eq!(Arc::strong_count(&sheet), 1, "extent cache must not force full-sheet copy-on-write");
        Arc::make_mut(&mut sheet).styles[0].font_size = 2000;
        let larger = super::printed_text_regions(&sheet, &res);
        assert!(larger.iter().map(|r| r.right()).max() > before.iter().map(|r| r.right()).max());
        assert_eq!(cached(), 1, "the old sheet's Weak entry is evicted");
        Arc::make_mut(&mut sheet).columns[0] = 1000 * 12_700;
        assert!(super::printed_text_regions(&sheet, &res).is_empty(), "geometry edits invalidate a cached overflow");
        Arc::make_mut(&mut sheet).columns[0] = 20 * 12_700;
        Arc::make_mut(&mut sheet).cells[0].text.push_str(" and additional edited text");
        let edited = super::printed_text_regions(&sheet, &res);
        assert!(edited.iter().map(|r| r.right()).max() > larger.iter().map(|r| r.right()).max());
        res.print_scale.set(0.5);
        assert_eq!(super::printed_text_regions(&sheet, &res), edited);
        assert_eq!(cached(), 2, "print scale participates in the key");
        let mut new_resources = crate::render::shape_view::Resources::new(&presentation, cx);
        new_resources.print = true;
        new_resources.print_scale.set(0.5);
        super::printed_text_regions(&sheet, &new_resources);
        assert_eq!(cached(), 3, "new font/resources generation cannot reuse old layout entries");
        let sheets: Vec<_> = (0..=super::CACHED_LAYOUTS).map(|_| Arc::new((*sheet).clone())).collect();
        for s in &sheets { super::printed_text_regions(s, &res); }
        assert_eq!(cached(), super::CACHED_LAYOUTS);
        let first_key = super::layout_key(&sheets[0], 1.0 / 12_700.0, &res);
        assert!(!super::PRINT_REGIONS.with(|cache| cache.borrow().iter().any(|(key, _, _)| *key == first_key)), "oldest live sheet is evicted at the LRU boundary");
        super::release_layouts();
        assert_eq!(cached(), 0);
    });
}
