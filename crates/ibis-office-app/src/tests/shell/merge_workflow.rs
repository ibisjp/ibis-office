// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The mail merge workflow, using real files and the app renderer.
use crate::docx::merge_panel::{MergeDialog, MergeOutput, MergeStep};
use crate::docx::scenarios::DocSteps;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Block, FlowPos};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::merge_workflow_files_and_drafts", files_and_drafts),
        ("shell::merge_workflow_screens", screens),
        (
            "shell::merge_review_scope_and_preflight",
            review_scope_and_preflight,
        ),
        (
            "shell::merge_review_import_save_and_replace",
            review_import_save_and_replace,
        ),
    ]
}
fn fields() -> Vec<String> {
    ["Name", "Company", "Address"].map(str::to_string).to_vec()
}
fn records() -> Vec<Vec<String>> {
    vec![
        vec!["Alice Yamada".into(), "Aoba Trading".into(), "Tokyo".into()],
        vec!["Bob Sato".into(), "Sakura Design".into(), "Osaka".into()],
        vec![
            "Carol Suzuki".into(),
            "Minato Works".into(),
            "Nagoya".into(),
        ],
    ]
}
fn files_and_drafts() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, w, cx| e.open_merge(w, cx));
    s.update(|e, _, cx| e.set_merge_step(MergeStep::Compose, cx));
    assert_eq!(
        s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().step),
        MergeStep::Recipients
    );
    let dir = s.screenshot_path("files");
    std::fs::create_dir_all(&dir).unwrap();
    let csv = "Name,Company,Address\r\nAlice,\"Aoba, Inc.\",00123\r\nBob,Sakura,Osaka\r\n";
    for (name, bytes) in [
        ("recipients.csv", csv.as_bytes().to_vec()),
        (
            "recipients.tsv",
            b"Name\tCompany\tAddress\nAlice\tAoba\t00123\nBob\tSakura\tOsaka\n".to_vec(),
        ),
        ("sjis.csv", ibis_pptx::csv::encode_sjis(csv)),
        (
            "recipients.xlsx",
            ibis_pptx::csv::workbook_from(csv, "Recipients").unwrap(),
        ),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        s.update(|e, w, cx| e.read_merge_file(&path, w, cx));
        assert!(s.with(|e, _| {
            e.doc_state()
                .unwrap()
                .merge
                .as_ref()
                .unwrap()
                .import
                .is_some()
        }));
        let before = s.with(|e, _| {
            e.doc_state()
                .unwrap()
                .merge
                .as_ref()
                .unwrap()
                .records
                .clone()
        });
        s.update(|e, w, cx| e.close_panel_dialog(w, cx));
        assert_eq!(
            s.with(|e, _| e
                .doc_state()
                .unwrap()
                .merge
                .as_ref()
                .unwrap()
                .records
                .clone()),
            before,
            "cancel import retains recipients"
        );
        s.update(|e, w, cx| {
            e.read_merge_file(&path, w, cx);
            e.confirm_merge_import(w, cx);
        });
        assert_eq!(
            s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().records.len()),
            2
        );
        if name.ends_with("csv") {
            assert_eq!(
                s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().records[0][2].clone()),
                "00123"
            );
        }
    }
    let book = ibis_pptx::read(&ibis_pptx::csv::workbook_from(csv, "Recipients").unwrap()).unwrap();
    let xls = dir.join("legacy.xls");
    std::fs::write(&xls, ibis_pptx::write_xls(&book).unwrap()).unwrap();
    s.update(|e, w, cx| {
        e.read_merge_file(&xls, w, cx);
        e.confirm_merge_import(w, cx);
    });
    assert_eq!(
        s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().records.len()),
        2
    );
    s.update(|e, _, cx| {
        e.set_merge_data(fields(), records(), cx);
        e.open_merge_task(MergeDialog::Recipients, cx);
        e.toggle_merge_record(1, cx);
        e.sort_merge_table(0, cx);
    });
    assert_eq!(
        s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().records.len()),
        3,
        "sort does not discard excluded rows"
    );
    assert_eq!(
        s.with(|e, _| e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .included
            .iter()
            .filter(|&&v| !v)
            .count()),
        1
    );
    s.update(|e, w, cx| {
        e.open_merge_row(Some(0), w, cx);
        let input = e.doc_state().unwrap().merge.as_ref().unwrap().row_inputs[0].clone();
        input.update(cx, |i, cx| i.set_value("Changed", w, cx));
        e.save_merge_row(w, cx);
    });
    s.update(|e, w, cx| e.close_panel_dialog(w, cx));
    assert_eq!(
        s.with(|e, _| e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .records
            .clone()),
        records(),
        "cancel manager discards a saved nested edit"
    );
    s.update(|e, _, cx| e.open_merge_task(MergeDialog::Recipients, cx));
    s.update(|e, _, cx| e.open_merge_task(MergeDialog::Columns, cx));
    s.update(|e, w, cx| {
        e.delete_merge_field(1, w, cx);
        e.close_panel_dialog(w, cx);
    });
    assert_eq!(
        s.with(|e, _| e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .fields
            .clone()),
        fields(),
        "cancel columns restores values and columns"
    );
    s.update(|e, _, cx| e.finish_merge_task(cx));
    let para = s
        .document()
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) => Some(p.id),
            _ => None,
        })
        .unwrap();
    s.click_text(FlowPos::new(para, 0));
    s.update(|e, _, cx| e.insert_merge_field("Name", cx));
    s.update(|e, _, cx| {
        e.set_merge_step(MergeStep::Preview, cx);
        e.goto_merge_record(2, cx);
    });
    let file = dir.join("one.docx");
    assert_eq!(
        s.update(|e, _, _| e
            .write_merged_documents_range(&file, ibis_doc_model::merge::MergePrintRange::Current)
            .unwrap()),
        1
    );
    let doc = ibis_pptx::read(&std::fs::read(&file).unwrap()).unwrap();
    let text = format!("{:?}", doc.flow.unwrap().body);
    assert!(text.contains("Bob Sato") && !text.contains("Alice Yamada"));
    assert_eq!(
        s.with(|e, _| e
            .write_merged_separately_range(
                &dir,
                "range",
                ibis_doc_model::merge::MergePrintRange::FromTo(2, 3)
            )
            .unwrap()),
        2
    );
    s.update(|e, _, cx| e.open_merge_task(MergeDialog::Output, cx));
    s.update(|e, w, cx| {
        e.set_merge_print_range(ibis_doc_model::merge::MergePrintRange::FromTo(2, 2), cx);
        e.close_panel_dialog(w, cx);
    });
    assert!(
        s.with(|e, _| matches!(
            e.doc_state().unwrap().merge.as_ref().unwrap().print_range,
            ibis_doc_model::merge::MergePrintRange::All
        )),
        "cancel output restores range"
    );
}
fn shot(s: &mut Scenario, name: &str) {
    s.settle();
    let full = s.screenshot(name);
    let image = image::open(full).unwrap().to_rgba8();
    let (w, h) = image.dimensions();
    let (x, y, width, height) = s.update(|e, window, _| {
        let scale = w as f32 / f32::from(window.viewport_size().width);
        let b = e.panel_dialog.bounds.filter(|_| e.panel_dialog.active);
        if let Some(b) = b {
            (
                (f32::from(b.origin.x) * scale).round() as u32,
                (f32::from(b.origin.y) * scale).round() as u32,
                (f32::from(b.size.width) * scale).round() as u32,
                (f32::from(b.size.height) * scale).round() as u32,
            )
        } else {
            (
                (w as f32 - 300. * scale).round() as u32,
                (108. * scale) as u32,
                (300. * scale) as u32,
                h - (136. * scale) as u32,
            )
        }
    });
    image::imageops::crop_imm(&image, x, y, width.min(w - x), height.min(h - y))
        .to_image()
        .save(s.screenshot_path(&format!("{name}-body")))
        .unwrap();
}
fn screens() {
    let _probe=super::button_probe::Probe::new();
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.size(1200., 800.);
    s.key("cmd-a");
    s.type_text("New product briefing\n\nDear customer,\n\nWe are pleased to invite you to our product briefing.\n\nPlease contact us to reserve your seat.\n\nAoba Trading");
    s.update(|e, w, cx| e.open_merge(w, cx));
    assert!(!s.with(|e, _| e.panel_dialog_is_modal()));
    shot(&mut s, "01-start");
    let path = s.screenshot_path("recipients").with_extension("xlsx");
    std::fs::write(&path,ibis_pptx::csv::workbook_from("Name,Company,Address\nAlice Yamada,Aoba Trading,Tokyo\nBob Sato,Sakura Design,Osaka\nCarol Suzuki,Minato Works,Nagoya","Recipients").unwrap()).unwrap();
    s.update(|e, w, cx| e.read_merge_file(&path, w, cx));
    assert!(s.with(|e, _| e.panel_dialog_is_modal()));
    shot(&mut s, "02-import");
    s.update(|e, _, cx| {
        e.doc_state_mut()
            .unwrap()
            .merge
            .as_mut()
            .unwrap()
            .import
            .as_mut()
            .unwrap()
            .advanced = true;
        cx.notify();
    });
    shot(&mut s, "15-import-range");
    s.update(|e, w, cx| e.confirm_merge_import(w, cx));
    shot(&mut s, "03-source");
    s.update(|e, _, cx| {
        e.open_merge_task(MergeDialog::Recipients, cx);
        e.doc_state_mut()
            .unwrap()
            .merge
            .as_mut()
            .unwrap()
            .selected_row = Some(1);
    });
    shot(&mut s, "04-recipients");
    s.update(|e, _, cx| {
        let m = e.doc_state_mut().unwrap().merge.as_mut().unwrap();
        m.filter = Some(ibis_doc_model::merge::RecordFilter {
            any_of: vec![vec![ibis_doc_model::merge::FilterCondition {
                field: 1,
                op: ibis_doc_model::merge::FilterOp::Contains,
                value: "Aoba".into(),
            }]],
        });
        cx.notify();
    });
    shot(&mut s, "16-filtered-recipients");
    s.update(|e, _, cx| e.clear_merge_filter(cx));
    s.update(|e, _, cx| {
        let mut b = e.panel_dialog.bounds.unwrap();
        b.size.width = gpui_kit::px(320.);
        b.origin.x = gpui_kit::px(440.);
        e.panel_dialog.bounds = Some(b);
        e.panel_dialog.user_resized = true;
        cx.notify();
    });
    shot(&mut s, "17-compact-recipients");
    s.update(|e, _, cx| {
        e.panel_dialog.bounds = None;
        cx.notify();
    });
    s.update(|e, w, cx| e.open_merge_row(Some(1), w, cx));
    shot(&mut s, "05-row");
    s.update(|e, w, cx| e.close_panel_dialog(w, cx));
    s.update(|e, _, cx| e.open_merge_task(MergeDialog::Columns, cx));
    shot(&mut s, "06-columns");
    s.update(|e, w, cx| e.close_panel_dialog(w, cx));
    s.update(|e, _, cx| e.open_merge_task(MergeDialog::Filter, cx));
    shot(&mut s, "07-filter");
    s.update(|e, w, cx| e.close_panel_dialog(w, cx));
    s.update(|e, _, cx| e.finish_merge_task(cx));
    s.update(|e, _, cx| e.set_merge_step(MergeStep::Compose, cx));
    let para = s
        .document()
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) => Some(p.id),
            _ => None,
        })
        .unwrap();
    s.click_text(FlowPos::new(para, 0));
    s.update(|e, _, cx| e.select_merge_field(0, false, cx));
    assert!(
        ibis_doc_model::merge::merge_fields_used(&s.document()).is_empty(),
        "selecting a list row does not insert it"
    );
    s.update(|e, _, cx| e.select_merge_field(0, true, cx));
    s.update(|e, w, cx| {
        e.doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .field_focus
            .focus(w, cx);
    });
    s.key("down");
    s.key("enter");
    assert_eq!(
        s.with(|e, _| e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .selected_field),
        Some(1)
    );
    assert!(
        ibis_doc_model::merge::merge_fields_used(&s.document()).contains(&"Company".to_string())
    );
    shot(&mut s, "08-compose");
    s.update(|e, _, cx| e.open_merge_task(MergeDialog::Condition, cx));
    shot(&mut s, "09-condition");
    s.update(|e, w, cx| e.close_panel_dialog(w, cx));
    s.update(|e, _, cx| e.set_merge_step(MergeStep::Preview, cx));
    shot(&mut s, "10-preview");
    s.update(|e, _, cx| {
        e.doc_state_mut().unwrap().merge.as_mut().unwrap().records[0][0].clear();
        cx.notify();
    });
    shot(&mut s, "18-empty-field-review");
    s.update(|e, _, cx| {
        e.doc_state_mut().unwrap().merge.as_mut().unwrap().records[0][0] = "Alice Yamada".into();
        cx.notify();
    });
    assert!(!s.with(|e, _| e.panel_dialog_is_modal()));
    s.update(|e, _, cx| e.set_merge_step(MergeStep::Output, cx));
    shot(&mut s, "11-output");
    s.update(|e, _, cx| e.open_merge_task(MergeDialog::Output, cx));
    shot(&mut s, "12-output-settings");
    s.update(|e, _, cx| {
        e.goto_merge_record(2, cx);
        e.set_merge_print_range(ibis_doc_model::merge::MergePrintRange::Current, cx);
    });
    shot(&mut s, "19-current-output");
    s.update(|e, w, cx| e.close_panel_dialog(w, cx));
    s.update(|e, _, cx| {
        e.doc_state_mut().unwrap().merge.as_mut().unwrap().output = MergeOutput::Pdf;
        e.open_merge_task(MergeDialog::Output, cx);
    });
    shot(&mut s, "13-pdf-settings");
    s.update(|e, w, cx| e.close_panel_dialog(w, cx));
    s.update(|e, _, cx| e.set_merge_step(MergeStep::Recipients, cx));
    super::button_probe::click(&mut s,"merge-kind");
    shot(&mut s,"14-kind");
    s.key("escape");
    s.update(|e, _, cx| {
        e.doc_state_mut()
            .unwrap()
            .merge
            .as_mut()
            .unwrap()
            .kind_draft = Some("envelopes");
        cx.notify();
    });
    shot(&mut s, "20-replace-body");
}

fn review_scope_and_preflight() {
    use ibis_doc_model::merge::{FilterCondition, FilterOp, MergePrintRange, RecordFilter};
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, w, cx| {
        e.open_merge(w, cx);
        e.set_merge_data(fields(), records(), cx);
    });
    let para = s
        .document()
        .body
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) => Some(p.id),
            _ => None,
        })
        .unwrap();
    s.click_text(FlowPos::new(para, 0));
    s.update(|e, _, cx| {
        e.insert_merge_field("Name", cx);
        e.doc_state_mut().unwrap().merge.as_mut().unwrap().records[0][0].clear();
    });
    assert_eq!(
        s.with(|e, _| e
            .merge_review_issues()
            .iter()
            .filter_map(|i| i.record)
            .collect::<Vec<_>>()),
        vec![0],
        "a blank used field is detected even when company and address have values"
    );
    s.update(|e, _, cx| e.set_merge_step(MergeStep::Output, cx));
    assert_eq!(
        s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().step),
        MergeStep::Preview
    );
    s.update(|e, _, cx| {
        e.acknowledge_merge_warnings(cx);
        e.set_merge_step(MergeStep::Output, cx);
    });
    assert_eq!(
        s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().step),
        MergeStep::Output
    );
    s.update(|e, _, cx| {
        let m = e.doc_state_mut().unwrap().merge.as_mut().unwrap();
        m.records[1][0].clear();
        cx.notify();
    });
    assert!(
        !s.with(|e, _| e.review_can_output()),
        "a changed warning set invalidates acknowledgement"
    );
    s.update(|e, _, cx| {
        e.open_merge_task(MergeDialog::Recipients, cx);
        let m = e.doc_state_mut().unwrap().merge.as_mut().unwrap();
        m.filter = Some(RecordFilter {
            any_of: vec![vec![FilterCondition {
                field: 1,
                op: FilterOp::Contains,
                value: "Minato".into(),
            }]],
        });
        cx.notify();
    });
    assert!(
        s.with(|e, _| e.merge_review_issues().is_empty()),
        "filtered-out records are not checked as output targets"
    );
    s.update(|e, _, cx| {
        let m = e.doc_state_mut().unwrap().merge.as_mut().unwrap();
        m.list_search = "Minato".into();
        cx.notify();
    });
    s.update(|e, _, cx| e.set_visible_merge_recipients(false, cx));
    assert_eq!(
        s.with(|e, _| e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .included
            .clone()),
        vec![true, true, false],
        "bulk operations only affect visible, filter-matching rows"
    );
    s.update(|e, w, cx| e.close_panel_dialog(w, cx));
    assert_eq!(
        s.with(|e, _| e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .included
            .clone()),
        vec![true, true, true]
    );
    s.update(|e, _, cx| {
        e.doc_state_mut().unwrap().merge.as_mut().unwrap().records = records();
        e.goto_merge_record(2, cx);
        e.set_merge_print_range(MergePrintRange::Current, cx);
    });
    assert_eq!(
        s.with(|e, cx| e.merge_output_indices(cx).unwrap()),
        vec![1],
        "summary resolves the actual current recipient"
    );
    s.update(|e, w, cx| {
        e.set_merge_print_range(MergePrintRange::FromTo(1, 1), cx);
        let m = e.doc_state().unwrap().merge.as_ref().unwrap();
        let a = m.print_from_field.clone();
        let b = m.print_to_field.clone();
        a.update(cx, |i, cx| i.set_value("2", w, cx));
        b.update(cx, |i, cx| i.set_value("3", w, cx));
    });
    assert_eq!(
        s.with(|e, cx| e.merge_output_indices(cx).unwrap()),
        vec![1, 2]
    );
    s.update(|e, w, cx| {
        let input = e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .print_to_field
            .clone();
        input.update(cx, |i, cx| i.set_value("4", w, cx));
    });
    assert!(s.with(|e, cx| e.merge_output_indices(cx).is_err()));
    s.update(|e, _, cx| {
        e.insert_merge_field("MissingColumn", cx);
        e.acknowledge_merge_warnings(cx);
    });
    assert!(
        !s.with(|e, _| e.review_can_output()),
        "missing columns cannot be dismissed as intentional blanks"
    );
}
fn review_import_save_and_replace() {
    let mut s = Scenario::open("test/editing/docx-report.docx");
    s.update(|e, w, cx| e.open_merge(w, cx));
    let path = s.screenshot_path("range-import").with_extension("csv");
    std::fs::write(
        &path,
        "Title,,\nName,Company,Company\nAlice,Aoba,00123\nBob,Sakura,00456\nNote,,\n",
    )
    .unwrap();
    s.update(|e, w, cx| e.read_merge_file(&path, w, cx));
    s.update(|e, w, cx| {
        let inputs = e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .import_range
            .clone();
        for (input, value) in inputs.iter().zip(["2", "4", "1", "3"]) {
            input.update(cx, |i, cx| i.set_value(value, w, cx));
        }
        e.apply_merge_import_range(w, cx);
    });
    s.update(|e, w, cx| {
        let input = e.doc_state().unwrap().merge.as_ref().unwrap().import_names[2].clone();
        input.update(cx, |i, cx| i.set_value("Address", w, cx));
        e.confirm_merge_import(w, cx);
    });
    assert_eq!(
        s.with(|e, _| e
            .doc_state()
            .unwrap()
            .merge
            .as_ref()
            .unwrap()
            .fields
            .clone()),
        fields()
    );
    assert_eq!(
        s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().records.len()),
        2
    );
    s.update(|e, _, cx| {
        let m = e.doc_state_mut().unwrap().merge.as_mut().unwrap();
        m.records[0][0] = "'Alice".into();
        m.records[0][1] = "=1+2".into();
        cx.notify();
    });
    let saved = s.screenshot_path("saved-recipients").with_extension("xlsx");
    s.with(|e, _| e.write_merge_recipient_list(&saved).unwrap());
    let bytes = std::fs::read(&saved).unwrap();
    assert!(
        bytes.starts_with(b"PK"),
        "save writes an actual XLSX package"
    );
    let sheet = ibis_pptx::test_files::part_text(&bytes, "xl/worksheets/sheet1.xml").unwrap();
    assert!(
        !sheet.contains("<f>"),
        "recipient text must not become formulas"
    );
    s.update(|e, _, cx| e.set_merge_data(vec![], vec![], cx));
    s.update(|e, w, cx| {
        e.read_merge_file(&saved, w, cx);
        e.confirm_merge_import(w, cx);
    });
    assert_eq!(
        s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().records[0][2].clone()),
        "00123",
        "explicit XLSX save/reload preserves text values"
    );
    assert_eq!(
        s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().records[0][0].clone()),
        "'Alice"
    );
    assert_eq!(
        s.with(|e, _| e.doc_state().unwrap().merge.as_ref().unwrap().records[0][1].clone()),
        "=1+2"
    );
    let before = s.document();
    s.update(|e, _, cx| {
        e.open_merge_task(MergeDialog::Kind, cx);
        e.doc_state_mut()
            .unwrap()
            .merge
            .as_mut()
            .unwrap()
            .kind_draft = Some("envelopes");
        e.apply_merge_kind_review(cx);
    });
    assert!(
        s.document() == before,
        "body replacement requires explicit acknowledgement"
    );
    s.update(|e, _, cx| {
        e.doc_state_mut()
            .unwrap()
            .merge
            .as_mut()
            .unwrap()
            .replace_body_ack = true;
        e.apply_merge_kind_review(cx);
    });
    assert!(s.document() != before);
    s.menu("edit.undo");
    assert!(
        s.document() == before,
        "replacement remains one undo operation"
    );
}
