// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Apply-button regression audit of the real dialog catalog.
//! Fixture inputs are changed; only a pointer click may commit them.
use super::{Scenario, button_probe as buttons};
use gpui_kit::*;
use std::cell::{Cell, RefCell};
thread_local! {
    static ACTIVE:Cell<bool>=const{Cell::new(false)};
    static RESULTS:RefCell<Vec<(String,bool)>>=const{RefCell::new(Vec::new())};
}
pub(crate) fn active() -> bool {
    ACTIVE.with(|v| v.get())
}
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![("shell::all_dialog_apply_buttons", run),("shell::dialog_apply_additional_modes",extra_test)]
}
fn run() {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ACTIVE.with(|v| v.set(false));
            crate::shell::chrome::FORCED_SIDEBAR.with(|v| v.set(None));
        }
    }
    let _reset = Reset;
    let _probe = buttons::Probe::new();
    RESULTS.with(|r| r.borrow_mut().clear());
    ACTIVE.with(|v| v.set(true));
    super::panel_catalog::dump_all_right_panels();
    if std::env::var_os("IBIS_GALLERY_ONLY").is_none() {
        extra_dialogs();
    }
    let failures = RESULTS.with(|r| {
        r.borrow()
            .iter()
            .filter(|(_, ok)| !*ok)
            .cloned()
            .collect::<Vec<_>>()
    });
    let count = RESULTS.with(|r| r.borrow().len());
    println!("Apply audit: {count} buttons, failures={failures:?}");
    if std::env::var_os("IBIS_GALLERY_ONLY").is_none() {
        let actual = RESULTS.with(|r| {
            r.borrow()
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<std::collections::BTreeSet<_>>()
        });
        let expected = EXPECTED_APPLY_CASES
            .iter()
            .map(|id| id.to_string())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            actual, expected,
            "every Apply dialog/mode in the inventory must remain covered"
        );
    } else {
        assert!(
            count > 0,
            "the selected dialog must render an Apply control"
        );
    }
    assert!(
        failures.is_empty(),
        "inert Apply buttons or invalid fixture: {failures:?}"
    );
}
fn snapshot(s: &mut Scenario) -> String {
    let model = s.with(|e, _| {
        format!(
            "{:?}\n{:?}\n{:?}\n{:?}",
            e.presentation(),
            e.doc_state().and_then(|s| s.pending.as_ref()),
            e.custom_user_name,
            e.custom_user_initials
        )
    });
    model

}
const APPLY_IDS: &[&str] = &[
    "template-apply-btn",
    "pptx-nm-all",
    "link-apply",
    "link-insert",
    "docx-font-apply",
    "docx-color-apply",
    "para-apply",
    "style-apply",
    "ruby-apply",
    "table-size-apply",
    "table-formula-apply",
    "notes-apply",
    "watermark-apply",
    "apply_properties",
    "user-info-apply",
    "kinsoku-apply",
    "control-apply",
    "theme-custom-apply",
    "docx-compress-apply",
    "alt-text-apply",
    "theme-custom-apply",
    "chart-apply",
    "goal-apply",
    "row-col-size-apply",
    "chart-template-apply-btn",
    "phonetic-apply",
    "xlsx.sparkline.apply",
    "custom-format-apply",
    "cell-format-rotation-set",
    "pptx-section-apply",
    "theme-custom-apply",
    "pptx-anim-opt-apply",
    "pptx-size-apply",
    "pptx-layout-apply",
    "pptx-paragraph-apply",
    "pptx-insets-apply",
    "pptx-timing-apply",
    "pptx-crop-apply",
    "pptx-chart-apply",
    "pptx-glow-apply",
    "pptx-hf-apply",
    "pptx-compress-apply",
    "pptx-show-apply",
    "pptx-frame-apply",
    "pptx-cell-size-apply",
    "pptx-shadow-apply",
    "pptx-gradient-apply",
];
pub(crate) fn audit(s: &mut Scenario, fmt: &str, key: &str) {
    if matches!(key,"docx_theme_panel"|"xlsx_theme_panel"|"theme_panel") {buttons::click(s,"theme-add");}
    let ids = buttons::all()
        .into_iter()
        .filter(|id| APPLY_IDS.contains(&id.as_str()) || id.ends_with("-apply-name"))
        .collect::<Vec<_>>();
    for id in ids {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            prepare(s, fmt, key, &id);
            let before = snapshot(s);
            buttons::click(s, &id);
            let after = snapshot(s);
            let compression = id == "docx-compress-apply" || id == "pptx-compress-apply";
            let changed = before != after
                || compression
                    && s.with(|e, _| {
                        e.status.as_ref().is_some_and(|m| {
                            m.contains(
                                &crate::shell::i18n::text(if fmt == "docx" {
                                    "docx.compress.no-smaller"
                                } else {
                                    "pptx.compress.no-pictures-can-made-smaller"
                                })
                                .to_string(),
                            )
                        })
                    });
            if !changed {
                s.screenshot(&format!("apply-failure-{fmt}-{key}-{id}"));
            }
            println!(
                "APPLY {fmt}/{key}/{id}: {}",
                if changed { "changed" } else { "NO EFFECT" }
            );
            changed
        }))
        .unwrap_or(false);
        RESULTS.with(|r| r.borrow_mut().push((format!("{fmt}/{key}/{id}"), result)));
    }
}
fn prepare(s: &mut Scenario, fmt: &str, key: &str, id: &str) {
    if id == "docx-compress-apply" || id == "pptx-compress-apply" {
        s.update(|e, _, _| e.status = None);
        return;
    }
    if id == "template-apply-btn" {
        let path = crate::shell::scenario::repo_root()
            .join("test/editing/docx-report.docx")
            .display()
            .to_string();
        s.update(|e, w, cx| {
            let input = e
                .doc_state()
                .unwrap()
                .template_panel
                .as_ref()
                .unwrap()
                .path_input
                .clone();
            input.update(cx, |i, cx| i.set_value(path, w, cx));
        });
        return;
    }
    if id == "pptx-nm-all" {
        s.update(|e, w, cx| {
            e.toggle_notes_marks("header", true, cx);
            let input = e
                .pptx()
                .unwrap()
                .notes_marks_panel
                .as_ref()
                .unwrap()
                .header_text
                .clone();
            input.update(cx, |i, cx| i.set_value("Apply audit notes", w, cx));
        });
        return;
    }
    if id=="pptx-paragraph-apply"{
        // The first field is inactive while line spacing is Default.
        // Change space-before, which is effective in every line-spacing mode.
        s.update(|e,w,cx|{let input=e.pptx().unwrap().paragraph_panel.as_ref().unwrap().fields[1].clone();input.update(cx,|i,cx|i.set_value("2",w,cx));});return;
    }
    if id == "docx-font-apply" {
        s.update(|e, w, cx| {
            let input = e.doc_state().unwrap().font_edit.as_ref().unwrap().fields[0].clone();
            input.update(cx, |i, cx| i.set_value("Arial", w, cx));
        });
        return;
    }
    if id == "xlsx.sparkline.apply" {
        s.update(|e, w, cx| {
            let p = e.wb().panels.sparkline.as_ref().unwrap();
            let (a, b) = (p.data_range_input.clone(), p.location_range_input.clone());
            a.update(cx, |i, cx| i.set_value("A2:C4", w, cx));
            b.update(cx, |i, cx| i.set_value("Z2:Z4", w, cx));
        });
        return;
    }
    if id == "pptx-hf-apply" {
        s.update(|e, w, cx| {
            e.toggle_header_footer("footer", true, cx);
            let input = e
                .pptx()
                .unwrap()
                .header_footer
                .as_ref()
                .unwrap()
                .footer_text
                .clone();
            input.update(cx, |i, cx| i.set_value("Apply audit footer", w, cx));
        });
        return;
    }
    if id == "pptx-show-apply" {
        s.update(|e, _, cx| e.toggle_show_panel_option("loop", cx));
        return;
    }
    if id == "pptx-anim-opt-apply" {
        s.update(|e, _, cx| {
            if let Some(p) = e
                .pptx_mut()
                .and_then(|s| s.animation_options_panel.as_mut())
            {
                p.delay_ms += 123;
            }
            cx.notify();
        });
        return;
    }
    if id == "pptx-shadow-apply" || id == "pptx-glow-apply" {
        // Imported placeholders inherit effects; use a direct shape for the success path.
        // Rejection of inherited effects has dedicated serializer/snapshot tests.
        s.update(|e, w, cx| {
            let mut auto = ibis_doc_model::AutoShape::new(
                ibis_doc_model::Geometry::Rect,
                ibis_doc_model::Fill::Solid(ibis_doc_model::Color::BLACK),
                None,
            );
            auto.text = Some(ibis_doc_model::TextBody::plain(
                "Apply audit",
                ibis_doc_model::TextStyle::default(),
            ));
            let shape = ibis_doc_model::Shape::new(
                e.document.ids().new_shape_id(),
                ibis_doc_model::EmuRect::new(500_000, 500_000, 2_000_000, 1_000_000),
                ibis_doc_model::ShapeKind::Auto(auto),
            );
            e.insert_shape(shape, cx);
            if id == "pptx-shadow-apply" {
                e.open_shadow_panel(w, cx);
            } else {
                e.open_glow_panel(w, cx);
            }
        });
    }
    let inputs = s.with(|_, cx| buttons::inputs(cx));
    if inputs.is_empty() {
        if id == "pptx-show-apply" {
            s.update(|e, _, cx| e.toggle_show_panel_option("loop", cx));
        }
        return;
    }
    let values = s.with(|_, cx| {
        inputs
            .iter()
            .map(|i| i.read(cx).value().to_string())
            .collect::<Vec<_>>()
    });
    let current = &values[0];
    let value = if id == "link-apply" || id == "link-insert" {
        "https://example.com/apply-audit".to_string()
    } else if key.contains("theme") || key == "color_panel" || key == "pptx_gradient_panel_pattern"
    {
        if current.trim_start_matches('#').eq_ignore_ascii_case("A13B7C") {"D24C8D".to_string()} else {"A13B7C".to_string()}
    } else if key == "font_panel" {
        "Arial".to_string()
    } else if id == "custom-format-apply" {
        "0.0000".to_string()
    } else if id == "cell-format-rotation-set" {
        "23".to_string()
    } else if let Ok(n) = current.parse::<f64>() {
        format!("{}", n + 1.)
    } else if current.starts_with('#') && current.len() == 7 {
        "#A13B7C".to_string()
    } else {
        "Apply audit value".to_string()
    };
    let index = if id == "docx-color-apply" {
        values.len() - 1
    } else {
        0
    };
    let input = inputs[index].clone();
    s.update(|_, w, cx| {
        input.update(cx, |i, cx| i.set_value(value, w, cx));
        cx.notify();
    });
    println!("FIXTURE {fmt}/{key}/{id}: {values:?}");
}

fn extra_dialogs() {
    use ibis_doc_model::{CellEdit, CellInput, CellValue, Command, SheetCommand};
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.update(|e, w, cx| {
        e.document
            .execute(vec![Command::Sheet(SheetCommand::SetCells {
                slide: e.current_slide,
                edits: vec![
                    CellEdit {
                        row: 0,
                        column: 0,
                        input: CellInput::Value(CellValue::Number(1.)),
                    },
                    CellEdit {
                        row: 0,
                        column: 1,
                        input: CellInput::Formula("A1*2".into()),
                    },
                ],
            })])
            .unwrap();
        e.open_goal_seek(w, cx);
        let p = e.wb().panels.goal_seek.as_ref().unwrap();
        for (i, v) in [
            (p.formula.clone(), "B1"),
            (p.target.clone(), "10"),
            (p.changing.clone(), "A1"),
        ] {
            i.update(cx, |i, cx| i.set_value(v, w, cx));
        }
        e.run_goal_seek(cx);
    });
    assert!(s.with(|e, _| e.wb().panels.goal_seek.as_ref().unwrap().found.is_some()));
    let before = snapshot(&mut s);
    buttons::click(&mut s, "goal-apply");
    assert!(before != snapshot(&mut s));
    assert!(s.with(|e, _| e.wb().panels.goal_seek.is_none()));
    RESULTS.with(|r| {
        r.borrow_mut()
            .push(("xlsx/goal_seek_result/goal-apply".into(), true))
    });
    // Exercise the Apply mode, not just the catalog's Save-template mode.
    let chart = s
        .with(|e, _| {
            e.slide()
                .shapes
                .iter()
                .find(|sh| matches!(sh.kind, ibis_doc_model::ShapeKind::Chart(_)))
                .map(|s| s.id)
        })
        .unwrap();
    s.update(|e, w, cx| {
        e.selection = Some(chart);
        e.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Save, w, cx);
        let input = e
            .wb()
            .panels
            .chart_template
            .as_ref()
            .unwrap()
            .template_name
            .clone();
        input.update(cx, |i, cx| i.set_value("ApplyAudit", w, cx));
        e.save_selected_chart_template(cx);
        e.change_sheet_chart(|c| c.style = Some(42), cx);
        e.open_chart_template_panel(crate::xlsx::chart_template::ChartTemplateMode::Apply, w, cx);
        let i = e
            .wb()
            .panels
            .chart_template
            .as_ref()
            .unwrap()
            .templates
            .iter()
            .position(|t| t.name == "ApplyAudit")
            .unwrap();
        e.set_selected_chart_template(i, cx);
    });
    let before = snapshot(&mut s);
    buttons::click(&mut s, "chart-template-apply-btn");
    assert!(before != snapshot(&mut s));
    assert!(s.with(|e, _| e.wb().panels.chart_template.is_none()));
    RESULTS.with(|r| {
        r.borrow_mut().push((
            "xlsx/chart_template_apply/chart-template-apply-btn".into(),
            true,
        ))
    });
    drop(s);
    let mut p = Scenario::open("test/editing/pptx-plan.pptx");
    p.update(|e, w, cx| {
        e.open_header_footer(w, cx);
        e.toggle_header_footer("footer", true, cx);
        let input = e
            .pptx()
            .unwrap()
            .header_footer
            .as_ref()
            .unwrap()
            .footer_text
            .clone();
        input.update(cx, |i, cx| i.set_value("Apply all slides", w, cx));
    });
    let before = snapshot(&mut p);
    buttons::click(&mut p, "pptx-hf-all");
    assert!(before != snapshot(&mut p));
    RESULTS.with(|r| {
        r.borrow_mut()
            .push(("pptx/header_footer_all/pptx-hf-all".into(), true))
    });
    drop(p);
    let mut s = Scenario::open("test/editing/docx-report.docx");
    use crate::docx::merge_panel::MergeDialog;
    s.update(|e, w, cx| {
        e.open_merge(w, cx);
        e.set_merge_data(
            vec!["Name".into()],
            vec![vec!["Alice".into()], vec!["Bob".into()]],
            cx,
        );
        e.open_merge_task(MergeDialog::Filter, cx);
        let i = e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .filter_field
            .clone();
        i.update(cx, |i, cx| i.set_value("Alice", w, cx));
    });
    buttons::click(&mut s, "merge-filter-ok");
    assert!(s.with(|e, _| {
        e.doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .filter
            .is_some()
    }));
    RESULTS.with(|r| {
        r.borrow_mut()
            .push(("docx/merge_filter/merge-filter-ok".into(), true))
    });
    s.update(|e, _, cx| {
        e.open_merge_task(MergeDialog::Mapping, cx);
        e.doc_state_mut()
            .unwrap()
            .merge
            .as_mut()
            .unwrap()
            .mapping_draft = vec![ibis_doc_model::merge::FieldMapData {
            name: "Name".into(),
            mapped_name: "First Name".into(),
            column: 0,
        }];
        cx.notify();
    });
    let before = snapshot(&mut s);
    buttons::click(&mut s, "merge-mapping-ok");
    assert!(before != snapshot(&mut s));
    RESULTS.with(|r| {
        r.borrow_mut()
            .push(("docx/merge_mapping/merge-mapping-ok".into(), true))
    });
    s.update(|e, _, cx| e.open_merge_task(MergeDialog::Columns, cx));
    s.update(|e, w, cx| {
        let i = e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .new_field
            .clone();
        i.update(cx, |i, cx| i.set_value("Address", w, cx));
    });
    buttons::click(&mut s, "merge-add-field");
    assert!(s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().fields.len() == 2));
    buttons::click(&mut s, "merge-task-ok");
    assert!(s.with(|e, _| {
        let m = e.doc_state().unwrap().merge.as_ref().unwrap();
        m.dialog.is_none() && m.column_backup.is_none()
    }));
    RESULTS.with(|r| {
        r.borrow_mut()
            .push(("docx/merge_columns/merge-task-ok".into(), true))
    });
}

// Explicit inventory: a missing control or a lost display mode must fail the audit.
const EXPECTED_APPLY_CASES: &[&str] = &[
    "xlsx/sheet_alt_text_panel/alt-text-apply",
    "docx/theme_palette_editor/theme-custom-apply",
    "xlsx/theme_palette_editor/theme-custom-apply",
    "pptx/theme_palette_editor/theme-custom-apply",
    "docx/color_panel/docx-color-apply",
    "docx/control_panel/control-apply",
    "docx/doc_properties_panel/apply_properties",
    "docx/doc_template_panel/template-apply-btn",
    "docx/docx_compress_panel/docx-compress-apply",
    "docx/docx_theme_panel/theme-custom-apply",
    "docx/font_panel/docx-font-apply",
    "docx/kinsoku_panel/kinsoku-apply",
    "docx/link_panel/link-apply",
    "docx/merge_columns/merge-task-ok",
    "docx/merge_filter/merge-filter-ok",
    "docx/merge_mapping/merge-mapping-ok",
    "docx/notes_panel/notes-apply",
    "docx/paragraph_panel/para-apply",
    "docx/ruby_panel/ruby-apply",
    "docx/style_panel/style-apply",
    "docx/table_formula_panel/table-formula-apply",
    "docx/table_size_panel/table-size-apply",
    "docx/user_info_panel/user-info-apply",
    "docx/watermark_panel/watermark-apply",
    "pptx/alt_text_panel/alt-text-apply",
    "pptx/animation_options_panel/pptx-anim-opt-apply",
    "pptx/cell_size_panel/pptx-cell-size-apply",
    // Chart data now applies live; there is no terminal Apply button.
    "pptx/compress_panel/pptx-compress-apply",
    "pptx/crop_panel/pptx-crop-apply",
    "pptx/frame_panel/pptx-frame-apply",
    "pptx/glow_panel/pptx-glow-apply",
    "pptx/header_footer_all/pptx-hf-all",
    "pptx/header_footer_panel/pptx-hf-apply",
    "pptx/layout_name_panel/pptx-layout-apply",
    "pptx/notes_marks_panel/pptx-nm-all",
    "pptx/pptx_gradient_panel_linear/pptx-gradient-apply",
    "pptx/pptx_gradient_panel_path/pptx-gradient-apply",
    "pptx/pptx_gradient_panel_pattern/pptx-gradient-apply",
    "pptx/pptx_paragraph_panel/pptx-paragraph-apply",
    "pptx/pptx_section_panel/pptx-section-apply",
    "pptx/presentation_properties_panel/apply_properties",
    "pptx/shadow_panel/pptx-shadow-apply",
    "pptx/show_panel/pptx-show-apply",
    "pptx/slide_link_panel/link-apply",
    "pptx/slide_size_panel/pptx-size-apply",
    "pptx/text_insets_panel/pptx-insets-apply",
    "pptx/theme_panel/theme-custom-apply",
    "pptx/timing_panel/pptx-timing-apply",
    "xlsx/cell_link_panel/link-insert",
    "xlsx/chart_template_apply/chart-template-apply-btn",
    "xlsx/goal_seek_result/goal-apply",
    "xlsx/phonetic_panel/phonetic-apply",
    "xlsx/pivot_panel_value_settings/pivot-_xlsx.values-0-apply-name",
    "xlsx/row_col_size_panel_col/row-col-size-apply",
    "xlsx/row_col_size_panel_row/row-col-size-apply",
    "xlsx/sheet_doc_properties_panel/apply_properties",
    "xlsx/sparkline_panel/xlsx.sparkline.apply",
    "xlsx/xlsx_theme_panel/theme-custom-apply",
];

fn extra_test(){let _probe=buttons::Probe::new();extra_dialogs();}
