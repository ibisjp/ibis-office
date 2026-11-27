// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/249)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use ibis_doc_model::{Block, Presentation};
use ibis_ooxml::zip::{EntryMeta, STORED, Writer, crc32};

const MIME: &[u8] = b"application/vnd.oasis.opendocument.text";

fn odt(body: &str) -> Vec<u8> {
    let content = format!(
        r#"<o:document-content xmlns:o="urn:oasis:names:tc:opendocument:xmlns:office:1.0"
            xmlns:t="urn:oasis:names:tc:opendocument:xmlns:text:1.0"
            xmlns:tbl="urn:oasis:names:tc:opendocument:xmlns:table:1.0"
            xmlns:other="urn:example:foreign" o:version="1.3">
            <o:body>{body}</o:body>
        </o:document-content>"#
    );
    let mut zip = Writer::new();
    zip.add_raw(
        "mimetype",
        EntryMeta::new(),
        STORED,
        crc32(MIME),
        MIME.len(),
        MIME,
    )
    .unwrap();
    zip.add("content.xml", EntryMeta::new(), content.as_bytes())
        .unwrap();
    zip.finish().unwrap()
}

fn check_body(presentation: &Presentation) {
    let flow = presentation.flow.as_deref().unwrap();
    let [
        Block::Paragraph(before),
        Block::Paragraph(header),
        Block::Paragraph(first),
        Block::Paragraph(nested),
        Block::Paragraph(continuation),
        Block::Paragraph(numbered),
        Block::Table(table),
        Block::Paragraph(section_end),
        Block::Paragraph(after),
    ] = flow.body.as_slice()
    else {
        panic!("expected paragraphs and a table in source order");
    };
    assert_eq!(before.text(), "前");
    assert_eq!(header.text(), "見出し");
    assert_eq!(header.style.as_deref(), Some("Heading2"));
    assert_eq!(first.text(), "項目 & one");
    assert_eq!(nested.text(), "子項目");
    assert_eq!(continuation.text(), "");
    assert_eq!(numbered.text(), "旧形式の番号付き段落");
    let [Block::Paragraph(cell)] = table.rows[1].cells[0].blocks.as_slice() else {
        panic!("expected the list item's paragraph in the table cell");
    };
    assert_eq!(table.rows.len(), 2);
    let [Block::Paragraph(header)] = table.rows[0].cells[0].blocks.as_slice() else {
        panic!("expected the header paragraph in the first row");
    };
    assert_eq!(header.text(), "表見出し");
    assert_eq!(cell.text(), "セル内の項目");
    // The end of a `text:section` is a paragraph of its own (a continuous section break, as Word writes it).
    assert_eq!(section_end.text(), "");
    assert_eq!(after.text(), "後");
}

#[test]
fn preserves_list_and_section_paragraphs_in_order() {
    let data = odt(r#"<o:text>
            <t:p>前</t:p>
            <t:section t:name="visible">
                <t:list>
                    <t:list-header><t:h t:outline-level="2">見出し</t:h></t:list-header>
                    <t:list-item>
                        <t:p>項目 &amp; <t:span>one</t:span><o:annotation><t:p>注釈</t:p></o:annotation></t:p>
                        <t:list><t:list-item><t:p>子項目</t:p></t:list-item></t:list>
                        <t:p/>
                    </t:list-item>
                </t:list>
                <t:numbered-paragraph><t:p>旧形式の番号付き段落</t:p></t:numbered-paragraph>
                <tbl:table tbl:name="table">
                    <tbl:table-header-rows>
                        <tbl:table-row><tbl:table-cell><t:p>表見出し</t:p></tbl:table-cell></tbl:table-row>
                    </tbl:table-header-rows>
                    <tbl:table-row-group><tbl:table-rows>
                        <tbl:table-row><tbl:table-cell>
                            <t:list><t:list-item><t:p>セル内の項目</t:p></t:list-item></t:list>
                        </tbl:table-cell></tbl:table-row>
                    </tbl:table-rows></tbl:table-row-group>
                    <tbl:table-row-group tbl:display="false"><tbl:table-row><tbl:table-cell>
                        <t:p>非表示の表行</t:p>
                    </tbl:table-cell></tbl:table-row></tbl:table-row-group>
                </tbl:table>
            </t:section>
            <t:section t:name="hidden" t:display="none"><t:p>非表示</t:p></t:section>
            <t:section t:name="conditional" t:display="condition"><t:p>条件付き</t:p></t:section>
            <other:list><t:p>別名前空間の要素</t:p></other:list>
            <tbl:table tbl:name="hidden-table"><tbl:table-row tbl:visibility="collapse">
                <tbl:table-cell><t:p>全行非表示</t:p></tbl:table-cell>
            </tbl:table-row></tbl:table>
            <t:p>後</t:p>
        </o:text>"#);
    check_body(&ibis_odf::read(&data).unwrap());
    let opened = ibis_odf::open(data).unwrap();
    check_body(&opened.presentation);
    let saved = opened
        .source
        .save(&opened.presentation, Default::default())
        .unwrap();
    check_body(&ibis_odf::read(&saved).unwrap());
}

#[test]
fn rejects_non_text_body_and_accepts_empty_text_body() {
    let error = ibis_odf::open(odt("<o:spreadsheet/>")).err().unwrap();
    assert!(error.to_string().contains("office:body/office:text"));
    assert!(ibis_odf::open(odt("<o:text/>")).is_ok());
}

#[test]
fn preserves_merged_and_repeated_table_cells_in_their_grid_positions() {
    use ibis_doc_model::VerticalMerge;

    let data = odt(r#"<o:text><tbl:table tbl:name="Grid">
      <tbl:table-row>
        <tbl:table-cell tbl:number-columns-spanned="2" tbl:number-rows-spanned="2"><t:p>結合</t:p></tbl:table-cell>
        <tbl:covered-table-cell/>
        <tbl:table-cell><t:p>右上</t:p></tbl:table-cell>
      </tbl:table-row>
      <tbl:table-row>
        <tbl:covered-table-cell tbl:number-columns-repeated="2"/>
        <tbl:table-cell><t:p>右下</t:p></tbl:table-cell>
      </tbl:table-row>
      <tbl:table-row tbl:number-rows-repeated="2">
        <tbl:table-cell tbl:number-columns-repeated="2"><t:p>反復</t:p></tbl:table-cell>
        <tbl:table-cell><t:p>末尾</t:p></tbl:table-cell>
      </tbl:table-row>
    </tbl:table></o:text>"#);
    let check = |presentation: &Presentation| {
        let [Block::Table(table)] = presentation.flow.as_ref().unwrap().body.as_slice() else {
            panic!("expected the table");
        };
        assert_eq!(table.rows.len(), 4);
        let text = |row: usize, cell: usize| match &table.rows[row].cells[cell].blocks[0] {
            Block::Paragraph(paragraph) => paragraph.text(),
            _ => panic!("expected paragraph"),
        };
        assert_eq!(table.rows[0].cells[0].span, 2);
        assert_eq!(table.rows[1].cells[0].span, 2);
        assert_eq!(table.rows[0].cells[0].merge, VerticalMerge::Start);
        assert_eq!(table.rows[1].cells[0].merge, VerticalMerge::Continue);
        assert_eq!(text(0, 0), "結合");
        assert_eq!(text(0, 1), "右上");
        assert_eq!(text(1, 1), "右下");
        for row in 2..4 {
            assert_eq!(
                [text(row, 0), text(row, 1), text(row, 2)],
                ["反復", "反復", "末尾"]
            );
        }
    };
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
fn rejects_table_expansion_that_exceeds_the_import_limit() {
    for table in [
        r#"<tbl:table-row tbl:number-rows-repeated="100001"><tbl:table-cell/></tbl:table-row>"#,
        r#"<tbl:table-row><tbl:table-cell tbl:number-columns-repeated="257"/></tbl:table-row>"#,
        r#"<tbl:table-row><tbl:table-cell><t:p><t:s t:c="999999999"/></t:p></tbl:table-cell></tbl:table-row>"#,
    ] {
        let data = odt(&format!("<o:text><tbl:table>{table}</tbl:table></o:text>"));
        let error = ibis_odf::open(data).err().unwrap();
        assert!(error.to_string().contains("表を展開すると大きすぎます"));
    }
}
