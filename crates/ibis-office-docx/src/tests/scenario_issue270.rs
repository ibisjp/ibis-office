// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #270 (tasks/doc/test-design/issue-270-screen.md T8): the input pane of a form document. 表示 > 入力フォーム opens it, a box for
//! each field ID of the document shows what the document has, typing in a box fills every place of the ID in one Undo step, and the
//! pane follows the document.
use crate::ext::*;
use crate::shell::scenario::Scenario;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::issue270_the_input_pane_fills_every_place_of_a_field", fill_every_place),
        ("docx::issue270_the_rows_of_a_csv_are_written_as_documents", write_documents),
        ("docx::issue270_a_csv_row_fills_the_fields_in_one_undo_step", csv_row_is_one_step),
        ("docx::issue270_max_chars_and_numbers_in_the_input_pane", max_chars_and_numbers),
    ]
}

fn texts(s: &mut Scenario, id: &str) -> Vec<String> {
    let id = id.to_owned();
    s.with(move |e, _| {
        let doc = e.flow().expect("a document");
        ibis_doc_model::flow_command::all_paragraphs(doc)
            .into_iter()
            .flat_map(|p| p.runs.iter())
            .filter_map(|r| match &r.kind {
                ibis_doc_model::FlowRunKind::PlainText { text, .. } if r.control.as_ref().is_some_and(|c| c.tag == format!("ibis.form.{id}")) => Some(text.clone()),
                _ => None,
            })
            .collect()
    })
}

fn fill_every_place() {
    let mut s = Scenario::open("test/fixtures/docx-form/elementary-school-template.docx");
    s.menu("docx.show-form-fill");
    assert!(s.with(|e, _| e.sidebar_open && e.doc_state().is_some_and(|st| st.form_fill.is_some())), "the input pane opens");
    // The pane is drawn once so that its boxes are made.
    s.update(|e, _, cx| { cx.notify(); let _ = e.form_items(); });
    let ids: Vec<String> = s.with(|e, _| e.form_items().unwrap().into_iter().map(|i| i.id).collect());
    assert!(ids.iter().any(|i| i == "pupil.name"), "{ids:?}");
    let places = texts(&mut s, "pupil.name").len();
    assert!(places > 1, "the name is in several places");
    s.update(|e, _, cx| e.form_fill_write("pupil.name", "紅葉 葵", cx));
    assert_eq!(texts(&mut s, "pupil.name"), vec!["紅葉 葵".to_owned(); places]);
    // One Undo takes it back in every place.
    s.menu("edit.undo");
    assert!(texts(&mut s, "pupil.name").iter().all(|t| t != "紅葉 葵"), "one undo takes back every place");
}

/// A CSV row fills every field of the row in one step; the next row replaces them in one step; one Undo takes back one row.
fn csv_row_is_one_step() {
    let mut s = Scenario::open("test/fixtures/docx-form/elementary-school-template.docx");
    s.menu("docx.show-form-fill");
    s.update(|e, _, cx| { cx.notify(); let _ = e.form_items(); });
    let places = texts(&mut s, "pupil.name").len();
    assert!(places > 1);
    let before_school = texts(&mut s, "nextSchool");
    let csv = "form.id,form.version,pupil.name,nextSchool
f,1,紅葉 葵,\"A
B\"
f,1,青山 空,C
";
    let status = s.update(|e, _, cx| e.form_fill_load_text(csv, cx));
    assert!(status.is_ok_and(|t| !t.is_empty()));
    assert_eq!(texts(&mut s, "pupil.name"), vec!["紅葉 葵".to_owned(); places]);
    assert!(texts(&mut s, "nextSchool").iter().all(|t| t == "A
B"), "{:?}", texts(&mut s, "nextSchool"));
    s.update(|e, _, cx| e.form_fill_step(1, cx));
    assert_eq!(texts(&mut s, "pupil.name"), vec!["青山 空".to_owned(); places]);
    assert!(texts(&mut s, "nextSchool").iter().all(|t| t == "C"));
    // The end stays; one Undo takes back the second row (both fields) and shows the first row.
    s.update(|e, _, cx| e.form_fill_step(1, cx));
    assert_eq!(texts(&mut s, "pupil.name"), vec!["青山 空".to_owned(); places]);
    s.menu("edit.undo");
    assert_eq!(texts(&mut s, "pupil.name"), vec!["紅葉 葵".to_owned(); places], "one undo takes the whole row back");
    assert!(texts(&mut s, "nextSchool").iter().all(|t| t == "A
B"), "the other field went back in the same step");
    s.menu("edit.undo");
    assert!(texts(&mut s, "pupil.name").iter().all(|t| t != "紅葉 葵"));
    assert_eq!(texts(&mut s, "nextSchool"), before_school, "one more undo takes the first row back");
}

/// The input pane's 入力済みの文書を書き出し: one DOCX for each CSV row, the bytes `docx_form fill-csv` makes, and no file is overwritten.
fn write_documents() {
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let template = root.join("test/fixtures/docx-form/elementary-school-template.docx");
    let definition = root.join("test/fixtures/docx-form/form.json");
    let (template, definition) = (template.as_path(), definition.as_path());
    let csv = "form.id,form.version,pupil.name,pupil.birthDate,nextSchool
elementary-school-record,1,紅葉 葵,2019-04-02,A
elementary-school-record,1,青葉 遥,2019-05-01,B
";
    let folder = root.join("target/issue270-write");
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    let count = crate::form_fill_panel::write_documents(template, definition, csv, &folder).unwrap();
    assert_eq!(count, 2);
    let expected = ibis_formats::form_template::fill_csv(&std::fs::read(template).unwrap(), &std::fs::read_to_string(definition).unwrap(), csv).unwrap();
    for (n, bytes) in expected.iter().enumerate() {
        assert_eq!(&std::fs::read(folder.join(format!("record-{}.docx", n + 1))).unwrap(), bytes);
    }
    assert!(crate::form_fill_panel::write_documents(template, definition, csv, &folder).is_err(), "a file that is there is not overwritten");
    let _ = std::fs::remove_dir_all(&folder);
}

const DEFINITION: &str = r##"{
  "form": {"id": "elementary-school-record", "version": 1},
  "fields": {
    "pupil.name": {"type": "text", "maxChars": 3},
    "pupil.birthDate": "date-ja",
    "nextSchool": {"type": "number", "format": "#,##0"}
  }
}"##;

/// T16: with the definition read in, a field whose text is longer than maxChars is told in the pane and the record is not written;
/// shortening it lets the record be written. T17: a `number` field gets a line; typing a number puts the format's text in every place, one
/// Undo takes it back, and what is not a number leaves the document and is told.
fn max_chars_and_numbers() {
    let mut s = Scenario::open("test/fixtures/docx-form/elementary-school-template.docx");
    s.menu("docx.show-form-fill");
    s.update(|e, _, cx| { cx.notify(); let _ = e.form_items(); });
    s.update(|e, _, cx| { e.form_fill_load_definition_text(DEFINITION, cx).expect("the definition is read"); });
    // The boxes are made again from the definition (a number field is a line).
    s.update(|e, _, cx| { cx.notify(); let _ = e.form_items(); });
    let name_places = texts(&mut s, "pupil.name").len();
    assert!(name_places > 1);
    // T16: four characters is over the limit of three.
    s.update(|e, _, cx| e.form_fill_write("pupil.name", "紅葉 葵", cx));
    let problems: Vec<String> = s.with(|e, _| e.form_fill_problems().into_iter().map(|(id, _)| id).collect());
    assert_eq!(problems, vec!["pupil.name".to_owned()], "the field over maxChars is told");
    assert_eq!(texts(&mut s, "pupil.name"), vec!["紅葉 葵".to_owned(); name_places], "the text is not cut");
    s.update(|e, _, cx| e.form_fill_write("pupil.name", "紅葉葵", cx));
    assert!(s.with(|e, _| e.form_fill_problems().is_empty()), "three characters are allowed");
    // T17: a number.
    let school_places = texts(&mut s, "nextSchool").len();
    s.update(|e, _, cx| e.form_fill_write_number("nextSchool", "1234567", cx));
    assert_eq!(texts(&mut s, "nextSchool"), vec!["1,234,567".to_owned(); school_places]);
    s.menu("edit.undo");
    assert!(texts(&mut s, "nextSchool").iter().all(|t| t != "1,234,567"), "one undo takes back every place");
    s.update(|e, _, cx| e.form_fill_write_number("nextSchool", "89", cx));
    let before = texts(&mut s, "nextSchool");
    s.update(|e, _, cx| e.form_fill_write_number("nextSchool", "89x", cx));
    assert_eq!(texts(&mut s, "nextSchool"), before, "what is not a number does not change the document");
    let problems: Vec<String> = s.with(|e, _| e.form_fill_problems().into_iter().map(|(id, _)| id).collect());
    assert_eq!(problems, vec!["nextSchool".to_owned()]);
    s.update(|e, _, cx| e.form_fill_write_number("nextSchool", "90", cx));
    assert!(s.with(|e, _| e.form_fill_problems().is_empty()));
    assert_eq!(texts(&mut s, "nextSchool"), vec!["90".to_owned(); school_places]);
}
