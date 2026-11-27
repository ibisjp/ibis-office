// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! #270: YAML and CSV input of the DOCX form (fictional data of test/fixtures/docx-form).
use ibis_formats::{form_data, form_template, read_docx};
use serde_json::{Value, json};

const TEMPLATE: &[u8] =
    include_bytes!("../../../test/fixtures/docx-form/elementary-school-template.docx");

fn definition() -> Value {
    serde_json::from_str(include_str!("../../../test/fixtures/docx-form/form.json")).unwrap()
}

fn text_of(bytes: &[u8]) -> String {
    fn walk(blocks: &[ibis_doc_model::Block], out: &mut Vec<String>) {
        for b in blocks {
            match b {
                ibis_doc_model::Block::Paragraph(p) => out.push(p.text()),
                ibis_doc_model::Block::Table(t) => t
                    .rows
                    .iter()
                    .flat_map(|r| &r.cells)
                    .for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut texts = Vec::new();
    walk(&read_docx(bytes).unwrap().flow.unwrap().body, &mut texts);
    texts.join("\n")
}

/// D1: the YAML definition and record give the same document as the JSON ones.
#[test]
fn yaml_and_json_give_the_same_document() {
    let yaml_definition = form_data::parse_structured(
        "form:\n  id: elementary-school-record\n  version: 1\nfields:\n  pupil.name: text\n  pupil.birthDate: date-ja\n  nextSchool: text\ntemplateSha256: 4e47538c817ceb4a3d93534d7dab301b066cf43941f1f615d22a7358641cb53d\n",
    )
    .unwrap();
    assert_eq!(yaml_definition, definition());
    let yaml_record = form_data::parse_structured(
        "form: {id: elementary-school-record, version: 1}\ndata:\n  pupil.name: 紅葉 葵\n  pupil.birthDate: \"2019-04-02\"\n  nextSchool: |-\n    架空市立そよかぜ中学校\n    架空市ひなた町１丁目\n",
    )
    .unwrap();
    let json_record: Value = serde_json::from_str(include_str!(
        "../../../test/fixtures/docx-form/record.json"
    ))
    .unwrap();
    assert_eq!(yaml_record, json_record);
    let a = form_template::fill(TEMPLATE, &yaml_definition, &yaml_record).unwrap();
    assert!(text_of(&a).contains("平成31年4月2日"));
    // Unquoted dates are text in this parser's output only when quoted; an unquoted date is
    // still a string for serde_json::Value (YAML 1.2 core schema keeps it a string).
    let unquoted = form_data::parse_structured("a: 2019-04-02\n").unwrap();
    assert_eq!(unquoted["a"], json!("2019-04-02"));
    assert!(form_data::parse_structured("- 1\n- 2\n").is_err());
    assert!(form_data::parse_structured("a: [\n").is_err());
}

/// D2: each CSV row is one record; quotes, commas, line breaks, CRLF and a BOM are kept.
#[test]
fn csv_rows_fill_one_document_each() {
    let csv = "\u{feff}form.id,form.version,pupil.name,pupil.birthDate,nextSchool\r\n\
elementary-school-record,1,\"紅葉, 葵\",2019-04-02,\"架空市立そよかぜ中学校\n架空市ひなた町１丁目\"\r\n\
elementary-school-record,1,青葉 遥,2019-05-01,\"\"\"桜\"\"小学校\"\r\n\r\n";
    let records = form_data::records_from_csv(csv).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["data"]["pupil.name"], "紅葉, 葵");
    assert_eq!(records[1]["data"]["nextSchool"], "\"桜\"小学校");
    let a = text_of(&form_template::fill(TEMPLATE, &definition(), &records[0]).unwrap());
    let b = text_of(&form_template::fill(TEMPLATE, &definition(), &records[1]).unwrap());
    assert!(a.contains("紅葉, 葵") && a.contains("平成31年4月2日"));
    assert!(b.contains("青葉 遥") && b.contains("令和元年5月1日") && b.contains("\"桜\"小学校"));
}

/// D3: the CSV written from records reads back as the same records; the header alone is the blank sheet.
#[test]
fn csv_round_trip_and_blank_header() {
    let def = definition();
    assert_eq!(
        form_data::records_to_csv(&def, &[]).unwrap(),
        "form.id,form.version,pupil.name,pupil.birthDate,nextSchool\n"
    );
    let record: Value = serde_json::from_str(include_str!(
        "../../../test/fixtures/docx-form/record.json"
    ))
    .unwrap();
    let csv = form_data::records_to_csv(&def, std::slice::from_ref(&record)).unwrap();
    assert_eq!(form_data::records_from_csv(&csv).unwrap(), vec![record.clone()]);
    let mut other = record;
    other["form"]["version"] = json!(2);
    assert!(form_data::records_to_csv(&def, &[other]).is_err());
}

/// D4: malformed CSV is refused, not guessed.
#[test]
fn bad_csv_is_refused() {
    for csv in [
        "",
        "form.id,form.version,a\nf,1\n",
        "form.id,a\nf,x\n",
        "form.id,form.version,a\nf,x,1\n",
        "form.id,form.version,a,a\nf,1,1,2\n",
        "form.id,form.version,a\nf,1,\"open\n",
        "form.id,form.version,a\nf,1,x\"y\n",
        "form.id,form.version,a\nf,1,\"x\"y\n",
    ] {
        assert!(form_data::records_from_csv(csv).is_err(), "{csv:?}");
    }
}

/// D4 (#270 screen): `fill_csv` (the input pane and `docx_form fill-csv`) gives the same bytes as filling each record by hand, in the order of the rows.
#[test]
fn fill_csv_is_the_same_as_filling_each_record() {
    let csv = "\u{feff}form.id,form.version,pupil.name,pupil.birthDate,nextSchool\r\n\
elementary-school-record,1,紅葉 葵,2019-04-02,A\r\n\
elementary-school-record,1,青葉 遥,2019-05-01,B\r\n";
    let definition_text = include_str!("../../../test/fixtures/docx-form/form.json");
    let made = form_template::fill_csv(TEMPLATE, definition_text, csv).unwrap();
    let records = form_data::records_from_csv(csv.trim_start_matches('\u{feff}')).unwrap();
    assert_eq!(made.len(), 2);
    for (bytes, record) in made.iter().zip(&records) {
        assert_eq!(*bytes, form_template::fill(TEMPLATE, &definition(), record).unwrap());
    }
    assert!(text_of(&made[1]).contains("青葉 遥"));
    assert!(form_template::fill_csv(TEMPLATE, "{", csv).is_err());
}
