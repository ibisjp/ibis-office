// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #270 screen part (tasks/doc/test-design/issue-270-screen.md T1–T7): the form fields of a document on the screen.
use ibis_doc_model::{FlowCommand, FlowDocument, FlowPos, FlowRunKind, flow_command};
use ibis_formats::{form_data, form_fill, form_template, read_docx};

const TEMPLATE: &[u8] = include_bytes!("../../../test/fixtures/docx-form/elementary-school-template.docx");

fn flow(bytes: &[u8]) -> FlowDocument {
    (*read_docx(bytes).unwrap().flow.unwrap()).clone()
}

fn places_of(doc: &FlowDocument, id: &str) -> Vec<String> {
    let mut out = Vec::new();
    for p in flow_command::all_paragraphs(doc) {
        for r in &p.runs {
            if let FlowRunKind::PlainText { text, .. } = &r.kind
                && r.control.as_ref().is_some_and(|c| c.tag == format!("ibis.form.{id}"))
            {
                out.push(text.clone());
            }
        }
    }
    out
}

/// T1: the tagged fields of the sample template, once for each ID, with how many places each is in.
#[test]
fn t1_items_of_the_template() {
    let doc = flow(TEMPLATE);
    let items = form_fill::items(&doc);
    let ids: Vec<_> = items.iter().map(|i| (i.id.as_str(), i.places)).collect();
    let counts = form_template::inspect(TEMPLATE).unwrap();
    assert_eq!(items.len(), counts.len(), "{ids:?}");
    for (id, n) in &counts {
        assert_eq!(items.iter().find(|i| &i.id == id).map(|i| i.places), Some(*n), "{id}");
    }
    assert!(counts.values().any(|&n| n > 1), "the template has a field in several places");
}

/// T2: a control whose tag is not `ibis.form.` is not a field.
#[test]
fn t2_other_tags_are_not_fields() {
    let mut doc = flow(TEMPLATE);
    let id = form_fill::items(&doc)[0].id.clone();
    let (para, at) = first_place(&doc, &id);
    let cmd = form_fill::tag_command(&doc, FlowPos::new(para, at), "x").unwrap();
    let FlowCommand::SetControlInfo { at: pos, mut info } = cmd else { panic!() };
    info.as_mut().unwrap().tag = "other.tag".into();
    flow_command::apply_to(&mut doc, FlowCommand::SetControlInfo { at: pos, info }).unwrap();
    assert!(form_fill::items(&doc).iter().all(|i| i.id != "x" && i.places > 0));
    assert!(form_fill::items(&doc).iter().all(|i| i.id != id || i.places >= 1));
}

fn first_place(doc: &FlowDocument, id: &str) -> (u32, usize) {
    for p in flow_command::all_paragraphs(doc) {
        let mut at = 0;
        for r in &p.runs {
            if matches!(r.kind, FlowRunKind::PlainText { .. }) && r.control.as_ref().is_some_and(|c| c.tag == format!("ibis.form.{id}")) {
                return (p.id, at);
            }
            at += r.len();
        }
    }
    panic!("no place of {id}")
}

/// T3: the ID check, and tagging a control that is not a plain-text control.
#[test]
fn t3_ids() {
    for ok in ["a", "a.b_c-1", "pupil.name"] {
        assert!(form_fill::valid_id(ok), "{ok}");
    }
    for bad in ["", "a b", "氏名", "a/b", "a:b"] {
        assert!(!form_fill::valid_id(bad), "{bad}");
    }
    let doc = flow(TEMPLATE);
    let p = flow_command::all_paragraphs(&doc)[0];
    assert!(form_fill::tag_command(&doc, FlowPos::new(p.id, 0), "bad id").is_err());
    let (para, at) = first_place(&doc, "pupil.name");
    assert!(form_fill::tag_command(&doc, FlowPos::new(para, at), "ok.id").is_ok());
}

/// T4 and T5: a value goes into every place of the ID; one undo (the inverse commands) takes it back.
#[test]
fn t4_t5_set_and_undo() {
    let mut doc = flow(TEMPLATE);
    let before = places_of(&doc, "pupil.name");
    assert!(before.len() > 1);
    let commands = form_fill::set_commands(&doc, "pupil.name", "紅葉 葵\r\nさん");
    assert_eq!(commands.len(), before.len());
    let mut inverse = Vec::new();
    for c in commands {
        inverse.push(flow_command::apply_to(&mut doc, c).unwrap());
    }
    assert!(places_of(&doc, "pupil.name").iter().all(|t| t == "紅葉 葵\nさん"));
    let item = form_fill::items(&doc).into_iter().find(|i| i.id == "pupil.name").unwrap();
    assert_eq!(item.text, "紅葉 葵\nさん");
    for c in inverse.into_iter().rev() {
        flow_command::apply_to(&mut doc, c).unwrap();
    }
    assert_eq!(places_of(&doc, "pupil.name"), before);
    assert!(form_fill::set_commands(&doc, "no.such", "x").is_empty());
}

/// T6: the record as CSV reads back to the same values (quotes, commas and line breaks included).
#[test]
fn t6_csv_round_trip() {
    let mut items = form_fill::items(&flow(TEMPLATE));
    let tricky = ["a,b", "say \"hi\"", "two\nlines", "plain"];
    for (i, item) in items.iter_mut().enumerate() {
        item.text = tricky[i % tricky.len()].to_owned();
    }
    let csv = form_fill::record_csv(&items, "elementary-school-record", 1).unwrap();
    let records = form_data::records_from_csv(&csv).unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0], form_fill::record(&items, "elementary-school-record", 1));
}

/// T7: the JSON record is what `fill` takes: filling the template with it (and the sample definition) works.
#[test]
fn t7_record_fills_the_template() {
    let definition: serde_json::Value = serde_json::from_str(include_str!("../../../test/fixtures/docx-form/form.json")).unwrap();
    let mut items = form_fill::items(&flow(TEMPLATE));
    for item in &mut items {
        item.text = if item.id == "pupil.birthDate" { "2019-04-02".into() } else { format!("v-{}", item.id) };
    }
    let record = form_fill::record(&items, "elementary-school-record", 1);
    let filled = form_template::fill(TEMPLATE, &definition, &record).unwrap();
    let text: String = flow_command::all_paragraphs(&flow(&filled)).iter().map(|p| p.text()).collect::<Vec<_>>().join("
");
    assert_eq!(text.matches("v-pupil.name").count(), form_template::inspect(TEMPLATE).unwrap()["pupil.name"]);
}
