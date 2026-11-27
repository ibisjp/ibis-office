// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/249)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use ibis_doc_model::{CellValue, Presentation, sheet_calc::sheet_of_slide};
use ibis_ooxml::zip::{EntryMeta, STORED, Writer, crc32};

fn package(mime: &str, body: &str, styles: &str) -> Vec<u8> {
    let content = format!(
        r#"<office:document-content office:version="1.3"
          xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
          xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
          xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
          xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0"
          xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0"
          xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"
          xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0"
          xmlns:of="urn:oasis:names:tc:opendocument:xmlns:of:1.2">
          <office:automatic-styles>{styles}</office:automatic-styles>
          <office:body>{body}</office:body>
        </office:document-content>"#
    );
    let mut zip = Writer::new();
    let mime = mime.as_bytes();
    zip.add_raw(
        "mimetype",
        EntryMeta::new(),
        STORED,
        crc32(mime),
        mime.len(),
        mime,
    )
    .unwrap();
    zip.add("content.xml", EntryMeta::new(), content.as_bytes())
        .unwrap();
    zip.finish().unwrap()
}

fn check_import_and_ooxml_save(data: Vec<u8>, check: impl Fn(&Presentation)) {
    check(&ibis_odf::read(&data).unwrap());
    let opened = ibis_odf::open(data).unwrap();
    check(&opened.presentation);
    let saved = opened
        .source
        .save(&opened.presentation, Default::default())
        .unwrap();
    check(&ibis_odf::read(&saved).unwrap());
}

#[test]
fn ods_nested_groups_keep_coordinates_values_and_visibility() {
    let data = package(
        "application/vnd.oasis.opendocument.spreadsheet",
        r#"<office:spreadsheet><table:table table:name="Data">
          <table:table-column/>
          <table:table-column-group table:display="false">
            <table:table-column-group>
              <table:table-columns><table:table-column table:style-name="wide"/></table:table-columns>
            </table:table-column-group>
          </table:table-column-group>
          <table:table-column/>
          <table:table-row-group table:display="false">
            <table:table-header-rows>
              <table:table-row><table:table-cell office:value-type="string"><text:p>見出し</text:p>
                <text:list><text:list-item><text:p>追記<office:annotation><text:p>注釈</text:p></office:annotation></text:p></text:list-item></text:list>
              </table:table-cell></table:table-row>
            </table:table-header-rows>
            <table:table-row-group>
              <table:table-rows>
                <table:table-row table:number-rows-repeated="2">
                  <table:table-cell table:number-columns-repeated="2"/>
                  <table:table-cell office:value-type="float" office:value="12.5"/>
                </table:table-row>
              </table:table-rows>
            </table:table-row-group>
          </table:table-row-group>
          <table:table-row>
            <table:table-cell office:value-type="float" office:value="25" table:formula="of:=SUM([.C2:.C3])"/>
          </table:table-row>
        </table:table></office:spreadsheet>"#,
        r#"<style:style style:name="wide" style:family="table-column"><style:table-column-properties style:column-width="4cm"/></style:style>"#,
    );
    check_import_and_ooxml_save(data, |presentation| {
        let sheet = sheet_of_slide(&presentation.slides[0]).unwrap();
        assert_eq!(
            sheet.cell(0, 0).unwrap().value,
            CellValue::Text("見出し\n追記".into())
        );
        for row in [1, 2] {
            assert_eq!(sheet.cell(row, 2).unwrap().value, CellValue::Number(12.5));
        }
        let total = sheet.cell(3, 0).unwrap();
        assert_eq!(total.value, CellValue::Number(25.0));
        assert_eq!(total.formula.as_deref(), Some("SUM(C2:C3)"));
        assert!((0..3).all(|row| sheet.is_row_hidden(row)));
        assert!(!sheet.is_row_hidden(3));
        assert_eq!(
            sheet.hidden_columns.keys().copied().collect::<Vec<_>>(),
            [1]
        );
        assert!((1_400_000..1_480_000).contains(&sheet.hidden_columns[&1]));
    });
}

#[test]
fn odp_text_boxes_and_notes_keep_wrapped_paragraphs() {
    let data = package(
        "application/vnd.oasis.opendocument.presentation",
        r#"<office:presentation><draw:page draw:name="Slide">
          <draw:frame svg:x="2cm" svg:y="2cm" svg:width="12cm" svg:height="2cm">
            <draw:text-box>
              <text:p>開始</text:p>
              <text:section text:name="visible">
                <text:numbered-paragraph text:level="1"><text:p>旧形式</text:p></text:numbered-paragraph>
                <text:list><text:list-item><text:p>リスト</text:p></text:list-item></text:list>
              </text:section>
              <text:section text:name="hidden" text:display="none"><text:p>非表示</text:p></text:section>
              <text:p>終了</text:p>
            </draw:text-box>
          </draw:frame>
          <presentation:notes>
            <draw:frame presentation:class="notes" svg:x="1cm" svg:y="10cm" svg:width="12cm" svg:height="4cm">
              <draw:text-box><text:section text:name="notes"><text:p>発表者ノート</text:p></text:section></draw:text-box>
            </draw:frame>
          </presentation:notes>
        </draw:page></office:presentation>"#,
        "",
    );
    check_import_and_ooxml_save(data, |presentation| {
        let slide = &presentation.slides[0];
        let text: Vec<_> = slide
            .shapes
            .iter()
            .filter_map(|shape| shape.text_body())
            .map(|body| body.text())
            .collect();
        assert_eq!(text, ["開始\n旧形式\nリスト\n終了"]);
        assert_eq!(slide.notes.trim(), "発表者ノート");
    });
}

#[test]
fn odp_table_with_grouped_columns_keeps_the_table() {
    let data = package(
        "application/vnd.oasis.opendocument.presentation",
        r#"<office:presentation><draw:page draw:name="Table">
          <draw:frame svg:x="2cm" svg:y="2cm" svg:width="8cm" svg:height="2cm">
            <table:table table:name="Grouped">
              <table:table-column-group><table:table-column-group>
                <table:table-column table:style-name="column" table:number-columns-repeated="2"/>
              </table:table-column-group></table:table-column-group>
              <table:table-header-rows><table:table-row>
                <table:table-cell><text:p>左</text:p></table:table-cell>
                <table:table-cell><text:p>右</text:p></table:table-cell>
              </table:table-row></table:table-header-rows>
              <table:table-row-group><table:table-rows><table:table-row>
                <table:table-cell><text:p>内容</text:p></table:table-cell>
                <table:table-cell><text:p>詳細</text:p></table:table-cell>
              </table:table-row></table:table-rows></table:table-row-group>
            </table:table>
          </draw:frame>
        </draw:page></office:presentation>"#,
        r#"<style:style style:name="column" style:family="table-column"><style:table-column-properties style:column-width="4cm"/></style:style>"#,
    );
    check_import_and_ooxml_save(data, |presentation| {
        let shapes = &presentation.slides[0].shapes;
        let table = shapes
            .iter()
            .find_map(|shape| match &shape.kind {
                ibis_doc_model::ShapeKind::Table(table) => Some(table),
                _ => None,
            })
            .expect("expected an editable table");
        assert_eq!(table.columns, [1_440_000, 1_440_000]);
        let cells: Vec<Vec<_>> = table
            .rows
            .iter()
            .map(|row| row.cells.iter().map(|cell| cell.text.text()).collect())
            .collect();
        assert_eq!(cells, [vec!["左", "右"], vec!["内容", "詳細"]]);
    });
}

#[test]
fn ods_local_named_ranges_recalculate_in_their_own_sheets() {
    use ibis_doc_model::{
        Command, Document, SheetCommand,
        sheet_command::{CellEdit, CellInput},
    };

    let data = package(
        "application/vnd.oasis.opendocument.spreadsheet",
        r#"<office:spreadsheet>
          <table:table table:name="Base's Data">
            <table:table-row>
              <table:table-cell office:value-type="float" office:value="5"/>
              <table:table-cell office:value-type="float" office:value="10" table:formula="of:=Rate*2"/>
            </table:table-row>
            <table:named-expressions>
              <table:named-range table:name="Rate" table:base-cell-address="$'Base''s Data'.$A$1" table:cell-range-address="$'Base''s Data'.$A$1"/>
            </table:named-expressions>
          </table:table>
          <table:table table:name="Summary">
            <table:table-row>
              <table:table-cell office:value-type="float" office:value="11"/>
              <table:table-cell office:value-type="float" office:value="16" table:formula="of:=Rate+['Base''s Data'.A1]"/>
            </table:table-row>
            <table:named-expressions>
              <table:named-range table:name="Rate" table:base-cell-address="$Summary.$A$1" table:cell-range-address="$Summary.$A$1"/>
            </table:named-expressions>
          </table:table>
        </office:spreadsheet>"#,
        "",
    );
    check_import_and_ooxml_save(data, |presentation| {
        let mut document = Document::new(presentation.clone());
        let value = |doc: &Document, sheet: usize| {
            sheet_of_slide(&doc.presentation().slides[sheet])
                .unwrap()
                .cell(0, 1)
                .unwrap()
                .value
                .clone()
        };
        assert_eq!(value(&document, 0), CellValue::Number(10.0));
        assert_eq!(value(&document, 1), CellValue::Number(16.0));
        for (sheet, n, expected) in [(0, 7.0, [14.0, 18.0]), (1, 13.0, [14.0, 20.0])] {
            let slide = presentation.slides[sheet].id;
            document
                .execute(vec![Command::Sheet(SheetCommand::SetCells {
                    sheet: slide,
                    edits: vec![CellEdit {
                        row: 0,
                        column: 0,
                        input: CellInput::Value(CellValue::Number(n)),
                    }],
                })])
                .unwrap();
            assert_eq!(value(&document, 0), CellValue::Number(expected[0]));
            assert_eq!(value(&document, 1), CellValue::Number(expected[1]));
        }
    });
}
