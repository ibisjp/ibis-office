// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/229)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
// SPDX-License-Identifier: AGPL-3.0-only
use ibis_formats::lawtext::{input_to_markdown, parse_law_xml, parse_lawtext, to_lawtext};

#[test]
fn egov_constitution_opens_as_markdown_and_lawtext() {
    let xml = include_str!("lawtext/321CONSTITUTION.xml");
    let law = parse_law_xml(xml).unwrap();
    assert_eq!(law.law_title, "日本国憲法");
    let markdown = input_to_markdown(xml.as_bytes()).unwrap().unwrap();
    assert!(markdown.contains("われらとわれらの子孫のために"));
    let lawtext = to_lawtext(&law);
    let reparsed = parse_lawtext(&lawtext).unwrap();
    assert_eq!(reparsed.law_title, law.law_title);
    assert_eq!(reparsed.main_provision, law.main_provision);
}

#[test]
fn upstream_lawtext_sample_keeps_the_split_title_and_both_paragraphs() {
    let source = include_str!("lawtext/administrative-procedure-act.lawtext");
    let markdown = input_to_markdown(source.as_bytes()).unwrap().unwrap();
    assert!(markdown.contains("# 行政手続法"));
    assert!(markdown.contains("法令番号: 平成五年法律第八十八号"));
    assert!(markdown.contains("第一条 （目的等）"));
    assert!(markdown.contains("他の法律に特別の定めがある場合"));
}
