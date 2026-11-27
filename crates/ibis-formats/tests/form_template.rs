// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/270)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
use ibis_formats::{form_template, read_docx};
use serde_json::{Value, json};

const TEMPLATE: &[u8] =
    include_bytes!("../../../test/fixtures/docx-form/elementary-school-template.docx");

fn inputs() -> (Value, Value) {
    (
        serde_json::from_str(include_str!("../../../test/fixtures/docx-form/form.json")).unwrap(),
        serde_json::from_str(include_str!("../../../test/fixtures/docx-form/record.json")).unwrap(),
    )
}

/// Texts of every paragraph, including those in table cells.
fn collect(blocks: &[ibis_doc_model::Block], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            ibis_doc_model::Block::Paragraph(p) => out.push(p.text()),
            ibis_doc_model::Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        collect(&cell.blocks, out);
                    }
                }
            }
        }
    }
}

#[test]
fn fills_repeated_names_and_preserves_multiline_value() {
    let counts = form_template::inspect(TEMPLATE).unwrap();
    assert_eq!(counts["pupil.name"], 4);
    let (definition, mut data) = inputs();
    data["data"]["pupil.name"] = json!("青葉 <遥> & 葵");
    let bytes = form_template::fill(TEMPLATE, &definition, &data).unwrap();
    let flow = read_docx(&bytes).unwrap().flow.unwrap();
    let mut texts = Vec::new();
    collect(&flow.body, &mut texts);
    let all = texts.join("\n");
    assert!(all.contains("平成31年4月2日"), "{all}");
    assert!(
        all.contains("架空市立そよかぜ中学校\u{b}架空市ひなた町１丁目"),
        "{all}"
    );
    let repetitions = texts
        .iter()
        .filter(|t| t.contains("青葉 <遥> & 葵"))
        .count();
    assert_eq!(repetitions, 4);
}

#[test]
fn rejects_wrong_version_invalid_date_and_missing_field() {
    let (definition, mut data) = inputs();
    data["form"]["version"] = json!(2);
    assert!(form_template::fill(TEMPLATE, &definition, &data).is_err());
    data["form"]["version"] = json!(1);
    data["data"]["pupil.birthDate"] = json!("2019-02-29");
    assert!(form_template::fill(TEMPLATE, &definition, &data).is_err());
    data["data"]
        .as_object_mut()
        .unwrap()
        .remove("pupil.birthDate");
    assert!(form_template::fill(TEMPLATE, &definition, &data).is_err());
}

#[test]
fn shows_era_dates_and_refuses_others() {
    let (definition, mut data) = inputs();
    let birth = |value: &str, data: &mut Value| {
        data["data"]["pupil.birthDate"] = json!(value);
        form_template::fill(TEMPLATE, &definition, data)
    };
    let text_of = |bytes: &[u8]| {
        let flow = read_docx(bytes).unwrap().flow.unwrap();
        let mut texts = Vec::new();
        collect(&flow.body, &mut texts);
        texts.join("\n")
    };
    for (value, shown) in [
        ("2019-04-30", "平成31年4月30日"),
        ("2019-05-01", "令和元年5月1日"),
        ("1989-01-08", "平成元年1月8日"),
        ("2020-02-29", "令和2年2月29日"),
    ] {
        let bytes = birth(value, &mut data).unwrap();
        assert!(text_of(&bytes).contains(shown), "{value}");
    }
    for value in [
        "1989-01-07",
        "2019-4-2",
        "2019-04-31",
        "2100-02-29",
        "20190402",
        "２０１９-04-02",
    ] {
        assert!(birth(value, &mut data).is_err(), "{value}");
    }
}
