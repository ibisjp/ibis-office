// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use crate::shell::scenario::Scenario;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use ibis_doc_model::{CellRange, CellValue, sheet_calc::sheet_of_slide};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue_new_save_recent", new_save_recent),
        ("xlsx::issue34_time_shortcut", time_shortcut),
        ("xlsx::issue47_far_grid_alignment", far_grid_alignment),
        ("pptx::issue48_rotated_lines", rotated_lines),
        ("docx::issue54_vertical_composition", vertical_composition),
        (
            "docx::issue54_large_font_multiple_spacing",
            large_font_multiple_spacing,
        ),
        ("shell::issue55_crop_resolution", crop_resolution),
        ("xlsx::issue52_formula_references", formula_references),
        ("xlsx::formula_edit_mode_arrows", formula_edit_mode_arrows),
        ("xlsx::issue43_series_headers", series_headers),
        (
            "shell::issue41_native_shape_clipboard",
            native_shape_clipboard,
        ),
        ("xlsx::issue35_repeat_edit", repeat_edit),
        ("docx::issue53_fit_text_dialog", fit_text_dialog),
        ("xlsx::issue40_phonetic_reopen", phonetic_reopen),
        ("xlsx::issue37_49_large_filter", large_filter),
        ("xlsx::issue42_trace_sheet_owner", trace_sheet_owner),
        ("xlsx::issue_chart_settings_switch", chart_settings_switch),
    ]
}
fn new_save_recent() {
    for (kind, ext) in [
        (ibis_pptx::Kind::Document, "docx"),
        (ibis_pptx::Kind::Workbook, "xlsx"),
        (ibis_pptx::Kind::Presentation, "pptx"),
    ] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(kind, w, cx));
        if kind == ibis_pptx::Kind::Workbook {
            s.click_cell("A1");
            s.enter("Saved history", "enter");
        }
        let path = s.save_as(&format!("new-recent.{ext}"));
        let canonical = crate::shell::plain_path::canonical(&path).unwrap();
        assert_eq!(crate::shell::recent_files::load().first(), Some(&canonical));
        assert_eq!(
            s.with(|e, _| e.recent_files.first().cloned()),
            Some(canonical.clone())
        );
        s.save_as(&path.to_string_lossy());
        assert_eq!(
            crate::shell::recent_files::load()
                .iter()
                .filter(|p| **p == canonical)
                .count(),
            1
        );
        let bad = path.join("missing/output.docx");
        assert!(s.update(|e, w, _| e.save_to(bad.clone(), w)).is_err());
        assert!(!crate::shell::recent_files::load().contains(&bad));
    }
}
fn phonetic_reopen() {
    // Keep the independently runnable regression when the private fixture is absent.
    let fixture = crate::shell::scenario::repo_root().join("test/github-issue/40/ふりがな1.xlsx"); // i18n-ok: User-provided fixture path; preserve its original Unicode filename.
    let mut s = if fixture.exists() {
        Scenario::open(&fixture.to_string_lossy())
    } else {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
        s.click_cell("A2");
        s.enter("アイビス", "enter"); // i18n-ok: Japanese document fixture content and expected values, not UI labels.
        s
    };
    s.click_cell("A2");
    s.key("cmd-1");
    s.menu("xlsx.phonetic-edit");
    assert_eq!(s.with(|e, _| e.panel_dialog.key), Some("phonetic_panel"));
    if !fixture.exists() {
        assert_eq!(
            s.with(|e, cx| e
                .wb()
                .panels
                .phonetic
                .as_ref()
                .unwrap()
                .input
                .read(cx)
                .value()
                .to_string()),
            ""
        );
    }
    s.update(|e, w, cx| {
        let input = e.wb().panels.phonetic.as_ref().unwrap().input.clone();
        input.update(cx, |i, cx| i.set_value("あいびす", w, cx)); // i18n-ok: Japanese document fixture content and expected values, not UI labels.
        e.apply_phonetic(cx);
    });
    s.menu("xlsx.phonetic-edit");
    assert_eq!(s.with(|e, _| e.panel_dialog.key), Some("phonetic_panel"));
    assert_eq!(
        s.with(|e, cx| e
            .wb()
            .panels
            .phonetic
            .as_ref()
            .unwrap()
            .input
            .read(cx)
            .value()
            .to_string()),
        "あいびす" // i18n-ok: Japanese document fixture content and expected values, not UI labels.
    );
    s.update(|e, w, cx| e.close_panel_dialog(w, cx));
    if !s.with(|e, _| sheet_of_slide(e.slide()).unwrap().show_phonetic) {
        s.menu("xlsx.phonetic-toggle");
    }
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().show_phonetic));
    s.screenshot("phonetic-layout");
}
fn large_filter() {
    use std::fmt::Write as _;
    let path = crate::shell::scenario::repo_root().join("target/scenario-input/issue49-large.csv");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut csv = (0..21)
        .map(|c| format!("field{c}"))
        .collect::<Vec<_>>()
        .join(",");
    csv.push('\n');
    for r in 0..40_000 {
        for c in 0..21 {
            if c > 0 {
                csv.push(',');
            }
            write!(csv, "{r}:{c}").unwrap();
        }
        csv.push('\n');
    }
    std::fs::write(&path, csv).unwrap();
    let mut s = Scenario::open(&path.to_string_lossy());
    let start = crate::clock::Instant::now();
    s.menu("xlsx.filter");
    let elapsed = start.elapsed();
    eprintln!("issue49 filter create: {elapsed:?}");
    assert!(
        elapsed.as_secs_f64() < 5.,
        "filter creation must remain interactive: {elapsed:?}"
    );
    assert_eq!(
        s.with(|e, _| sheet_of_slide(e.slide())
            .unwrap()
            .auto_filter
            .as_ref()
            .unwrap()
            .range),
        CellRange {
            first_row: 0,
            last_row: 40_000,
            first_column: 0,
            last_column: 20
        }
    );
    let start = crate::clock::Instant::now();
    s.update(|e, w, cx| e.open_filter_menu(0, point(px(100.), px(200.)), w, cx));
    s.update(|e, _, cx| e.filter_panel_tick(None, false, cx));
    for query in ["19", "199", "399", "3999"] {
        s.update(|e, w, cx| {
            let input = e.wb().filter_panel.as_ref().unwrap().search.clone();
            input.update(cx, |i, cx| i.set_value(query, w, cx));
            e.filter_panel_tick(None, true, cx);
        });
    }
    s.update(|e, _, cx| e.filter_panel_ok(cx));
    let elapsed = start.elapsed();
    eprintln!("issue49 filter search/apply: {elapsed:?}");
    assert!(
        elapsed.as_secs_f64() < 5.,
        "search and checked-value apply must remain interactive: {elapsed:?}"
    );
    assert!(s.with(|e, _| !sheet_of_slide(e.slide()).unwrap().filtered_rows.is_empty()));
}
fn trace_sheet_owner() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.click_cell("A10");
    s.enter("=A1", "enter");
    s.click_cell("A10");
    s.menu("xlsx.trace-precedents");
    assert!(s.with(|e, _| e.render_trace_arrows(1.).is_some()));
    let original = s.with(|e, _| e.current_slide);
    s.menu("xlsx.insert-sheet");
    assert_ne!(s.with(|e, _| e.current_slide), original);
    assert!(s.with(|e, _| e.render_trace_arrows(1.).is_none()));
    s.update(|e, w, cx| e.select_slide(original, w, cx));
    assert!(s.with(|e, _| e.render_trace_arrows(1.).is_some()));
}
fn chart_settings_switch() {
    let _probe = crate::shell::scenario::button_probe::Probe::new();
    let fixture=crate::shell::scenario::repo_root().join("test/private/グラフ確認.xlsx"); // i18n-ok: Private user fixture path, preserving the original Unicode filename.
    let mut s=if fixture.exists(){Scenario::open(&fixture.to_string_lossy())}else{
        let mut s=Scenario::start_view();
        s.update(|e,w,cx|e.new_file(ibis_pptx::Kind::Workbook,w,cx));
        for (cell,value) in [("A1","x"),("B1","y"),("A2","1"),("B2","2"),("A3","2"),("B3","4")]{s.click_cell(cell);s.enter(value,"enter");}
        s
    };
    s.update(|e,w,cx|e.go_to_reference(if fixture.exists(){"A1:B18"}else{"A1:B3"},w,cx));
    s.menu("pptx.insert-chart.line");
    let chart = s.with(|e, _| {
        e.slide()
            .shapes
            .iter()
            .find(|s| matches!(s.kind, ibis_doc_model::ShapeKind::Chart(_)))
            .unwrap()
            .id
    });
    s.key("cmd-1");
    let index = s.with(|e, _| e.slide().shapes.iter().position(|s| s.id == chart).unwrap());
    let at = s.shape_position(index);
    s.right_click(at);
    let bounds = crate::shell::scenario::button_probe::bounds("menu-item-xlsx.chart-format")
        .expect("chart context menu format row");
    s.click((f32::from(bounds.center().x), f32::from(bounds.center().y)));
    assert!(s.with(|e, _| e.wb().custom_format.is_none()));
    assert!(s.with(|e, _| e.wb().chart_edit.is_some()));
    assert!(s.with(|e, _| e.sidebar_open && !e.panel_dialog.active));
}

fn time_shortcut() {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s.click_cell("A1");
    s.key("ctrl-:");
    assert!(
        matches!(s.value("A1"),CellValue::Number(n)if (0.0..1.0).contains(&n)),
        "JIS colon inserts a time serial"
    );
}
fn repeat_edit() {
    use ibis_doc_model::sheet_command::StyleChange;
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s.click_cell("A1");
    s.toolbar("bold");
    s.click_cell("B1");
    s.key("f4");
    assert!(s.with(|e, _| e.current_style().bold));
    s.click_cell("A2");
    s.key("cmd-1");
    s.update(|e, _, cx| {
        e.format_cells(StyleChange::Bold(true), cx);
        e.format_cells(StyleChange::Italic(true), cx);
        e.format_cells(StyleChange::Fill(Some(ibis_doc_model::Rgb(0x123456))), cx);
    });
    s.click_cell("B2");
    s.key("f4");
    assert!(s.with(|e, _| {
        let st = e.current_style();
        st.bold && st.italic && st.fill == Some(ibis_doc_model::Rgb(0x123456))
    }));
    let rows = s.with(|e, _| sheet_of_slide(e.slide()).unwrap().rows.len());
    s.menu("xlsx.insert-row-above");
    s.key("f4");
    assert!(s.with(|e, _| sheet_of_slide(e.slide()).unwrap().rows.len()) >= rows + 2);
}
fn fit_text_dialog() {
    use crate::docx::scenarios::DocSteps;
    use ibis_doc_model::{FlowCommand, FlowPos, RunChange};
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Document, w, cx));
    s.type_text("あいうえおかき"); // i18n-ok: Japanese document fixture content and expected values, not UI labels.
    let (para, _) = s.paragraph(0);
    s.update(|e, w, cx| {
        e.run_flow(
            vec![FlowCommand::SetRunFormat {
                from: FlowPos::new(para, 0),
                to: FlowPos::new(para, 7),
                change: RunChange::Size(1200),
            }],
            cx,
        );
        let st = e.doc_state_mut().unwrap();
        st.anchor = Some(FlowPos::new(para, 0));
        st.caret = Some(FlowPos::new(para, 7));
        e.open_fit_text(w, cx);
    });
    assert_eq!(s.with(|e, _| e.panel_dialog.key), Some("fit_text_panel"));
    crate::shell::i18n::show(crate::shell::i18n::Language::English);
    s.update(|_,_,cx|cx.notify());
    s.screenshot("fit-text-dialog-en");
    crate::shell::i18n::show(crate::shell::i18n::Language::Japanese);
    s.update(|_,_,cx|cx.notify());
    s.screenshot("fit-text-dialog-ja");
    s.update(|e, w, cx| {
        let field = e
            .doc_state()
            .unwrap()
            .fit_text_edit
            .as_ref()
            .unwrap()
            .input
            .clone();
        field.update(cx, |i, cx| i.set_value("9", w, cx));
        e.apply_fit_text(false, w, cx);
    });
    s.update(|e, _, cx| {
        let st = e.doc_state_mut().unwrap();
        st.anchor = None;
        st.caret = Some(FlowPos::new(para, 3));
        cx.notify();
    });
    assert_eq!(
        s.with(|e, _| e.fit_text_at_caret().map(|(_, _, w, _)| w)),
        Some(2160)
    );
    s.screenshot("fit-text-active-underline");
    s.save_as("fit-text-nine.docx");
    assert!(s.reopen().flow.as_ref().unwrap().body.iter().any(|b|matches!(b,ibis_doc_model::Block::Paragraph(p)if p.runs.iter().any(|r|r.style.fit_text.is_some_and(|(w,_)|w==2160)))));
    s.update(|e, w, cx| e.open_fit_text(w, cx));
    s.update(|e, w, cx| {
        let field = e
            .doc_state()
            .unwrap()
            .fit_text_edit
            .as_ref()
            .unwrap()
            .input
            .clone();
        field.update(cx, |i, cx| i.set_value("12", w, cx));
        e.close_panel_dialog(w, cx);
    });
    assert_eq!(
        s.with(|e, _| e.fit_text_at_caret().map(|(_, _, w, _)| w)),
        Some(2160)
    );
    s.update(|e, w, cx| {
        e.open_fit_text(w, cx);
        e.apply_fit_text(true, w, cx);
    });
    assert!(s.with(|e, _| e.fit_text_at_caret().is_none()));
    s.key("cmd-z");
    assert!(s.with(|e, _| e.fit_text_at_caret().is_some()));
}

fn native_shape_clipboard() {
    use ibis_doc_model::{EmuRect, FlowRunKind, Shape};
    for kind in [
        ibis_pptx::Kind::Presentation,
        ibis_pptx::Kind::Workbook,
        ibis_pptx::Kind::Document,
    ] {
        let mut s = Scenario::start_view();
        s.update(|e, w, cx| e.new_file(kind, w, cx));
        let native = s.with(|e, _| {
            let mut p = e.presentation().clone();
            let id = p.new_shape_id();
            let shape = Shape::text_box(
                id,
                EmuRect::new(0, 0, 3000000, 1000000),
                "Office vector text",
            );
            ibis_pptx::shape_clipboard::write_gvml(&p, &[shape]).unwrap()
        });
        crate::native_shape_clipboard::simulate(Some(native));
        assert!(s.update(|e, _, cx| e.paste_office_shapes(cx)));
        let shapes = s.with(|e, _| {
            if kind == ibis_pptx::Kind::Document {
                let ibis_doc_model::Block::Paragraph(p) = &e.flow().unwrap().body[0] else {
                    panic!()
                };
                p.runs
                    .iter()
                    .filter_map(|r| {
                        if let FlowRunKind::Shape(s) = &r.kind {
                            Some(*s.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
            } else {
                e.slide()
                    .shapes
                    .iter()
                    .filter(|s| {
                        s.text_body()
                            .is_some_and(|b| b.text() == "Office vector text")
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            }
        });
        assert_eq!(shapes.len(), 1);
        assert_eq!(shapes[0].text_body().unwrap().text(), "Office vector text");
        assert!(s.update(|e, _, cx| e.copy_office_shapes(&shapes, cx)));
        assert!(s.with(|_, cx| {
            cx.read_from_clipboard()
                .unwrap()
                .entries()
                .iter()
                .any(|e| matches!(e, ClipboardEntry::Image(_)))
        }));
        #[cfg(target_os = "macos")]
        if std::env::var_os("IBIS_VERIFY_SYSTEM_CLIPBOARD").is_some() {
            let image = s.with(|e, _| crate::native_shape_clipboard::image_fallback(e.presentation(), &shapes).unwrap());
            crate::native_shape_clipboard::verify_system_formats(&image, &crate::native_shape_clipboard::read().unwrap());
        }
        s.screenshot("native-shape");
        s.key("cmd-z");
    }
}

fn formula_references() {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    for (cell, value) in [("A1", "1"), ("B1", "2"), ("C1", "3")] {
        s.click_cell(cell);
        s.enter(value, "enter");
    }
    s.click_cell("D4");
    s.type_text("=");
    s.key("left");
    assert_eq!(s.with(|e, _| e.cell_edit().unwrap().text.clone()), "=C4");
    s.key("shift-left");
    assert_eq!(s.with(|e, _| e.cell_edit().unwrap().text.clone()), "=B4:C4");
    s.key("escape");
    assert!(s.with(|e, _| e.cell_edit().is_none()));
    s.click_cell("D4");
    // "=S" now lists SCAN first (alphabetical, as Excel 365 does), so type
    // enough of the word that SUM is the first candidate.
    s.type_text("=SUM");
    s.key("tab");
    assert_eq!(s.with(|e, _| e.cell_edit().unwrap().text.clone()), "=SUM(");
    s.update(|e, _, cx| {
        assert!(e.point_reference(0, 0, cx));
    });
    s.key("cmd-shift-right");
    assert_eq!(
        s.with(|e, _| e.cell_edit().unwrap().text.clone()),
        "=SUM(A1:C1"
    );
    s.screenshot("formula-reference-handles");
    let (from, to) = s.with(|e, _| (e.cell_corner(2, 0).unwrap(), e.cell_center(2, 1).unwrap()));
    s.drag(
        (f32::from(from.x), f32::from(from.y)),
        (f32::from(to.x), f32::from(to.y)),
    );
    assert_eq!(
        s.with(|e, _| e.cell_edit().unwrap().text.clone()),
        "=SUM(A1:C2"
    );
    s.key("enter");
    assert_eq!(s.value("D4"), CellValue::Number(6.));
    assert_eq!(s.selected(), "D5");
    s.click_cell("D4");
    s.key("f2");
    s.update(|e, _, cx| {
        e.replace_cell_text(
            0,
            e.cell_edit().unwrap().text.len(),
            "=SUM(A1,B1,C1,A2,B2,C2,A3)",
            cx,
        )
    });
    let colors = s.with(|e, _| {
        crate::xlsx::formula_reference::references(&e.cell_edit().unwrap().text)
            .into_iter()
            .map(|r| r.color)
            .collect::<std::collections::HashSet<_>>()
    });
    assert_eq!(colors.len(), 7);
    s.screenshot("seven-reference-colors");
}
// Excel (real, measured 2026-10-28): the arrows point at cells only in Enter mode with a value expected at the caret;
// F2 / a double-click (Edit mode) moves the caret instead; F2 again goes back to Enter mode.
fn formula_edit_mode_arrows() {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    let text = |s: &mut Scenario| s.with(|e, _| e.cell_edit().unwrap().text.clone());
    s.click_cell("A1");
    s.enter("=1+1", "enter");
    s.click_cell("A1");
    s.key("f2");
    s.key("left");
    s.key("left");
    s.type_text("9");
    assert_eq!(text(&mut s), "=19+1");
    s.key("escape");
    s.click_cell("D4");
    s.type_text("=1+");
    s.key("f2");
    s.key("left");
    assert_eq!(text(&mut s), "=1+");
    assert_eq!(s.with(|e, _| e.cell_edit().unwrap().caret_byte()), 2);
    s.key("right");
    s.key("f2");
    s.key("left");
    assert_eq!(text(&mut s), "=1+C4");
    s.key("escape");
    s.click_cell("D5");
    s.type_text("=1+1");
    s.key("left");
    assert!(s.with(|e, _| e.cell_edit().is_none()));
    assert_eq!(s.selected(), "C5");
}
fn series_headers() {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s.update(|e, _, cx| {
        let slide=e.current_slide;
        e.run_sheet(vec![ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::RenameSheet { slide, name:"O'Brien 売上".into() })],cx); // i18n-ok: Japanese document fixture content and expected values, not UI labels.
    });
    for (cell, value) in [
        ("A1", "項目"), // i18n-ok: Japanese document fixture content and expected values, not UI labels.
        ("B1", "売上"), // i18n-ok: Japanese document fixture content and expected values, not UI labels.
        ("A2", "東京"), // i18n-ok: Japanese document fixture content and expected values, not UI labels.
        ("B2", "10"),
        ("A3", "大阪"), // i18n-ok: Japanese document fixture content and expected values, not UI labels.
        ("B3", "20"),
    ] {
        s.click_cell(cell);
        s.enter(value, "enter");
    }
    s.update(|e, w, cx| e.go_to_reference("A1:B3", w, cx));
    s.menu("pptx.insert-chart.line");
    s.update(|e, w, cx| e.open_sheet_chart_panel(w, cx));
    s.update(|e, w, cx| e.open_chart_series_editor(Some(0), w, cx));
    for on in [false, true, false, true] {
        s.update(|e, w, cx| e.toggle_chart_series_header(on, w, cx));
    }
    let range = s.with(|e, cx| {
        e.wb()
            .chart_edit
            .as_ref()
            .unwrap()
            .series_editor
            .as_ref()
            .unwrap()
            .values
            .read(cx)
            .value()
            .to_string()
    });
    assert!(range.contains("$B$2:$B$3"), "{range}");
    assert!(range.starts_with("'O''Brien 売上'!"), "{range}"); // i18n-ok: Japanese document fixture content and expected values, not UI labels.
    let name=s.with(|e,cx|e.wb().chart_edit.as_ref().unwrap().series_editor.as_ref().unwrap().name.read(cx).value().to_string());
    assert_eq!(name,"売上"); // i18n-ok: Japanese document fixture content and expected values, not UI labels.
    s.screenshot("series-first-cell-name");
}

fn far_grid_alignment() {
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Workbook, w, cx));
    s.update(|e, w, cx| {
        e.set_zoom(Some(2.83), w, cx);
        e.go_to_reference("AOU18823", w, cx);
    });
    let (columns, rows) = s.with(|e, _| {
        let g = e.wb().header_geometry.borrow();
        (g.columns.clone(), g.rows.clone())
    });
    assert!(!columns.is_empty() && !rows.is_empty());
    for (c, _, right) in columns {
        let at = s.with(|e, _| e.cell_corner(c, 18822).unwrap());
        assert!(
            (f32::from(at.x) - right).abs() < 1.,
            "column {c}: {} vs {right}",
            f32::from(at.x)
        );
    }
    for (r, _, bottom) in rows {
        let at = s.with(|e, _| e.cell_corner(1060, r).unwrap());
        assert!(
            (f32::from(at.y) - bottom).abs() < 1.,
            "row {r}: {} vs {bottom}",
            f32::from(at.y)
        );
    }
    s.screenshot("far-grid-283-percent");
}
fn rotated_lines() {
    use ibis_doc_model::{EmuRect, Shape};
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Presentation, w, cx));
    let mut widths = Vec::new();
    for angle in [0, 90, 180, 270] {
        let image = s.with(|e, _| {
            let mut p = e.presentation().clone();
            let id = p.new_shape_id();
            let mut shape = Shape::text_box(
                id,
                EmuRect::new(0, 0, 4000000, 4000000),
                "One line\nSecond line\nThird line",
            );
            shape.rotation = angle * 60000;
            shape.text_body_mut().unwrap().style.font_size = 2400;
            crate::native_shape_clipboard::image_fallback(&p, &[shape]).unwrap()
        });
        let image = image::load_from_memory(&image.bytes).unwrap().to_rgba8();
        let image = match angle {
            90 => image::imageops::rotate270(&image),
            180 => image::imageops::rotate180(&image),
            270 => image::imageops::rotate90(&image),
            _ => image,
        };
        let occupied: Vec<_> = (0..image.height())
            .filter(|y| {
                (0..image.width()).any(|x| {
                    let p = image.get_pixel(x, *y).0;
                    p[3] > 128 && p[0] < 100 && p[1] < 100 && p[2] < 100
                })
            })
            .collect();
        assert!(!occupied.is_empty());
        let mut bands = Vec::new();
        for y in occupied {
            if bands.last().is_none_or(|(_, last)| y > *last + 2) {
                bands.push((y, y));
            } else {
                bands.last_mut().unwrap().1 = y;
            }
        }
        assert_eq!(bands.len(), 3, "rotation {angle}: {bands:?}");
        widths.push(bands[2].0 - bands[0].0);
    }
    assert!(
        widths.iter().all(|w| w.abs_diff(widths[0]) <= 3),
        "line spacing under rotation: {widths:?}"
    );
}
fn vertical_composition() {
    use crate::docx::scenarios::DocSteps;
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Document, w, cx));
    s.type_text("日本語。文章（縦書き）"); // i18n-ok: Japanese document fixture content and expected values, not UI labels.
    s.menu("docx.page.direction.vertical");
    s.update(|e, w, cx| {
        e.doc_replace_and_mark_text_in_range(None, "へんかんちゅう", Some(7..7), w, cx) // i18n-ok: Japanese document fixture content and expected values, not UI labels.
    });
    assert!(s.with(|e, _| e.doc_state().unwrap().marked.is_some()));
    s.screenshot("vertical-ime-marked");
    s.update(|e, w, cx| e.doc_replace_text_in_range(None, "変換中", w, cx)); // i18n-ok: Japanese document fixture content and expected values, not UI labels.
    assert!(s.with(|e, _| e.doc_state().unwrap().marked.is_none()));
    let (para, _) = s.paragraph(0);
    let lines = s.with(|e, _| e.flow_lines());
    let before = s.with(|e, _| {
        e.caret_box(&lines, ibis_doc_model::FlowPos::new(para, 3))
            .unwrap()
    });
    let after = s.with(|e, _| {
        e.caret_box(&lines, ibis_doc_model::FlowPos::new(para, 4))
            .unwrap()
    });
    assert!(
        after.2 > before.2,
        "a full vertical cell follows punctuation"
    );
}
fn crop_resolution() {
    use std::io::Cursor;
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Presentation, w, cx));
    let pixels = image::RgbaImage::from_fn(2880, 1920, |_, y| {
        if y % 4 < 2 {
            image::Rgba([0, 0, 0, 255])
        } else {
            image::Rgba([255, 255, 255, 255])
        }
    });
    let mut data = Cursor::new(Vec::new());
    pixels.write_to(&mut data, image::ImageFormat::Png).unwrap();
    let mut p = s.with(|e, _| e.presentation().clone());
    let media = p.add_media(ibis_doc_model::MediaFormat::Png, data.into_inner());
    let resources = s.with(|_, cx| crate::render::shape_view::Resources::new(&p, cx));
    resources.preload.wait_all();
    let crop = ibis_doc_model::Crop {
        left: 9576,
        top: 29480,
        right: 31340,
        bottom: 60199,
    };
    if let Some(ImageSource::Render(image)) = resources.picture_at_crop_size(media, 620., 72., crop)
    {
        let size = image.size(0);
        let visible_h = size.height.0 as f32 * (1. - (crop.top + crop.bottom) as f32 / 100000.);
        assert!(
            visible_h >= 144.,
            "visible crop has enough vertical samples: {visible_h}"
        );
    } else {
        panic!("the decoded crop image");
    }
}

fn large_font_multiple_spacing() {
    use crate::docx::scenarios::DocSteps;
    use ibis_doc_model::{FlowCommand, FlowPos, LineHeight, ParaChange, RunChange};
    let mut s = Scenario::start_view();
    s.update(|e, w, cx| e.new_file(ibis_pptx::Kind::Document, w, cx));
    s.type_text("大きい文字の行間"); // i18n-ok: Japanese document fixture content and expected values, not UI labels.
    let (id, _) = s.paragraph(0);
    s.update(|e, _, cx| {
        let mut page = e.flow().unwrap().page;
        page.line_pitch = Some(20 * 12700);
        e.run_flow(
            vec![
                FlowCommand::SetPage { page },
                FlowCommand::SetRunFormat {
                    from: FlowPos::new(id, 0),
                    to: FlowPos::new(id, 8),
                    change: RunChange::Size(3600),
                },
                FlowCommand::SetParagraphFormat {
                    first: id,
                    last: id,
                    change: ParaChange::SnapToGrid(true),
                },
                FlowCommand::SetParagraphFormat {
                    first: id,
                    last: id,
                    change: ParaChange::Line(LineHeight::Auto(240)),
                },
            ],
            cx,
        );
    });
    let single = s.with(|e, _| {
        e.flow_lines()
            .iter()
            .find(|line| line.para == id)
            .unwrap()
            .h
    });
    s.update(|e, _, cx| {
        e.run_flow(
            vec![FlowCommand::SetParagraphFormat {
                first: id,
                last: id,
                change: ParaChange::Line(LineHeight::Auto(360)),
            }],
            cx,
        );
    });
    let multiple = s.with(|e, _| {
        e.flow_lines()
            .iter()
            .find(|line| line.para == id)
            .unwrap()
            .h
    });
    assert!(
        (multiple - single * 1.5).abs() < 0.01,
        "36pt in a 20pt grid: single={single}, 1.5 lines={multiple}"
    );
    s.screenshot("large-font-multiple-spacing");
}
