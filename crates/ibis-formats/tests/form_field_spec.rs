// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #270: the field definition with `type`, `maxChars` and `format` (tasks/doc/test-design/issue-270-screen.md, T10-T15).
use ibis_formats::form_template::{self, FieldSpec, Violation};
use ibis_formats::read_docx;
use serde_json::{Value, json};

const TEMPLATE: &[u8] = include_bytes!("../../../test/fixtures/docx-form/elementary-school-template.docx");

fn inputs() -> (Value, Value) {
    (
        serde_json::from_str(include_str!("../../../test/fixtures/docx-form/form.json")).unwrap(),
        serde_json::from_str(include_str!("../../../test/fixtures/docx-form/record.json")).unwrap(),
    )
}

fn text_of(bytes: &[u8]) -> String {
    fn walk(blocks: &[ibis_doc_model::Block], out: &mut Vec<String>) {
        for b in blocks {
            match b {
                ibis_doc_model::Block::Paragraph(p) => out.push(p.text()),
                ibis_doc_model::Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut texts = Vec::new();
    walk(&read_docx(bytes).unwrap().flow.unwrap().body, &mut texts);
    texts.join("\n")
}

fn spec(v: Value) -> FieldSpec {
    FieldSpec::from_json(&v).unwrap()
}

/// T10: the object form gives the same document as the string form.
#[test]
fn object_form_is_the_same_as_the_string_form() {
    let (mut definition, data) = inputs();
    let plain = form_template::fill(TEMPLATE, &definition, &data).unwrap();
    definition["fields"]["pupil.name"] = json!({"type": "text"});
    definition["fields"]["nextSchool"] = json!({"type": "text", "maxChars": 100});
    assert_eq!(form_template::fill(TEMPLATE, &definition, &data).unwrap(), plain);
}

/// T11: a number is shown with the Excel number format code (General when there is none).
#[test]
fn a_number_is_shown_with_its_format() {
    let n = |format: Option<&str>| match format {
        Some(f) => spec(json!({"type": "number", "format": f})),
        None => spec(json!({"type": "number"})),
    };
    assert_eq!(n(Some("#,##0")).show("1234567").unwrap(), "1,234,567");
    assert_eq!(n(Some("#,##0.00")).show(" -1234.5 ").unwrap(), "-1,234.50");
    assert_eq!(n(Some("0.0%")).show("0.256").unwrap(), "25.6%");
    assert_eq!(n(None).show("1234.5").unwrap(), "1234.5");
    // In the document too.
    let (mut definition, mut data) = inputs();
    definition["fields"]["nextSchool"] = json!({"type": "number", "format": "#,##0"});
    data["data"]["nextSchool"] = json!("1234567");
    let out = form_template::fill(TEMPLATE, &definition, &data).unwrap();
    assert!(text_of(&out).contains("1,234,567"));
}

/// T12: what is not a number is refused (also a thousands comma and a unit), and the document is not made.
#[test]
fn a_value_that_is_not_a_number_is_refused() {
    let s = spec(json!({"type": "number", "format": "#,##0"}));
    for v in ["", "abc", "1,234", "12円", "NaN", "inf", "--1"] {
        assert_eq!(s.show(v), Err(Violation::Invalid), "{v:?}");
    }
    let (mut definition, mut data) = inputs();
    definition["fields"]["nextSchool"] = json!({"type": "number"});
    data["data"]["nextSchool"] = json!("abc");
    assert!(form_template::fill(TEMPLATE, &definition, &data).is_err());
}

/// T13: maxChars counts the characters shown (line breaks not counted); the maximum is allowed, one more is refused, nothing is cut.
#[test]
fn max_chars_refuses_instead_of_cutting() {
    let s = spec(json!({"type": "text", "maxChars": 5}));
    assert_eq!(s.show("あいうえお").unwrap(), "あいうえお");
    assert_eq!(s.show("あい\nうえお").unwrap(), "あい\nうえお");
    assert_eq!(s.show("あいうえおか"), Err(Violation::TooLong { chars: 6, max: 5 }));
    // A number counts the shown text: 1,234 is 5 characters.
    let n = spec(json!({"type": "number", "format": "#,##0", "maxChars": 5}));
    assert_eq!(n.show("1234").unwrap(), "1,234");
    assert!(n.show("12345").is_err());
    let (mut definition, mut data) = inputs();
    definition["fields"]["pupil.name"] = json!({"type": "text", "maxChars": 3});
    assert!(form_template::fill(TEMPLATE, &definition, &data).is_err(), "紅葉 葵 is 4 characters");
    data["data"]["pupil.name"] = json!("紅葉葵");
    assert!(form_template::fill(TEMPLATE, &definition, &data).is_ok());
    // fill-csv refuses too.
    let csv = "form.id,form.version,pupil.name,pupil.birthDate,nextSchool\nelementary-school-record,1,紅葉 葵,2019-04-02,x\n";
    assert!(form_template::fill_csv(TEMPLATE, &definition.to_string(), csv).is_err());
}

/// T14: a definition that cannot be read is refused: unknown type, bad maxChars, format on a text, a stray key, a non-string non-object.
#[test]
fn bad_definitions_are_refused() {
    for v in [
        json!("integer"),
        json!({"type": "number", "maxChars": 0}),
        json!({"type": "number", "maxChars": 2.5}),
        json!({"type": "number", "maxChars": "9"}),
        json!({"type": "text", "format": "0"}),
        json!({"type": "number", "format": 3}),
        json!({"type": "text", "max": 3}),
        json!({"maxChars": 3}),
        json!(5),
    ] {
        assert!(FieldSpec::from_json(&v).is_err(), "{v}");
    }
}

/// T15: the date type still works in the object form, with maxChars on the shown text.
#[test]
fn date_in_the_object_form() {
    let s = spec(json!({"type": "date-ja", "maxChars": 9}));
    assert_eq!(s.show("2019-04-02").unwrap(), "平成31年4月2日");
    assert!(spec(json!({"type": "date-ja", "maxChars": 5})).show("2019-04-02").is_err());
    assert_eq!(s.show("2019-02-29"), Err(Violation::Invalid));
}
