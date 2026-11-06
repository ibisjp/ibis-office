// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Documents made for the tests and the scenarios: a file's own content
//! with things added that the test files lack.

/// A document (`data`) with a paragraph of the things Word puts in text that the
/// editor keeps as they are: a comment's range and mark, a tracked
/// insertion and deletion, a footnote's mark and a simple field.
pub fn docx_with_complex_paragraph(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let main = "word/document.xml";
    let xml = String::from_utf8(package.read(main).unwrap()).unwrap();
    let paragraph = concat!(
        r#"<w:p><w:r><w:t xml:space="preserve">前の文 </w:t></w:r><w:commentRangeStart w:id="90"/><w:r><w:t>注釈の範囲</w:t></w:r><w:commentRangeEnd w:id="90"/>"#,
        r#"<w:r><w:commentReference w:id="90"/></w:r>"#,
        r#"<w:ins w:id="91" w:author="Test" w:date="2026-01-01T00:00:00Z"><w:r><w:t>挿入された文字</w:t></w:r></w:ins>"#,
        r#"<w:del w:id="92" w:author="Test" w:date="2026-01-01T00:00:00Z"><w:r><w:delText>削除された文字</w:delText></w:r></w:del>"#,
        r#"<w:r><w:t xml:space="preserve"> 脚注</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="1"/></w:r>"#,
        r#"<w:fldSimple w:instr=" DATE \@ &quot;yyyy/MM/dd&quot; "><w:r><w:t>2026/01/01</w:t></w:r></w:fldSimple>"#,
        r#"<w:r><w:t xml:space="preserve"> 後の文の終わり</w:t></w:r></w:p>"#
    );
    let at = xml.rfind("<w:sectPr").unwrap();
    let xml = format!("{}{}{}", &xml[..at], paragraph, &xml[at..]);
    package.set_bytes(main, xml.into_bytes());
    let w = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;
    let comments = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:comments {w}><w:comment w:id="90" w:author="Test" w:date="2026-01-01T00:00:00Z" w:initials="T"><w:p><w:r><w:t>コメント</w:t></w:r></w:p></w:comment></w:comments>"#);
    let footnotes = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes {w}><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> 脚注の文</w:t></w:r></w:p></w:footnote></w:footnotes>"#
    );
    package.set_bytes("word/comments.xml", comments.into_bytes());
    package.set_bytes("word/footnotes.xml", footnotes.into_bytes());
    package.set_override("word/comments.xml", "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml").unwrap();
    package.set_override("word/footnotes.xml", "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml").unwrap();
    package.add_relationship(main, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments", "comments.xml", false).unwrap();
    if !package.relationships(main).unwrap().iter().any(|r| !r.external && r.kind.ends_with("/footnotes") && r.target == "footnotes.xml") {
        package.add_relationship(main, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes", "footnotes.xml", false).unwrap();
    }
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A document (`data`) with a paragraph with a formula in its text (`m:oMath`: a fraction) and one with a displayed
/// formula (`m:oMathPara`: a square root), each between texts.
pub fn docx_with_math(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let main = "word/document.xml";
    let xml = String::from_utf8(package.read(main).unwrap()).unwrap();
    let m = r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math""#;
    let paragraphs = format!(
        concat!(
            r#"<w:p><w:r><w:t xml:space="preserve">式 </w:t></w:r><m:oMath {m}><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:r><m:t>b</m:t></m:r></m:den></m:f></m:oMath><w:r><w:t xml:space="preserve"> の次</w:t></w:r></w:p>"#,
            r#"<w:p><m:oMathPara {m}><m:oMath><m:rad><m:radPr><m:degHide m:val="1"/></m:radPr><m:deg/><m:e><m:r><m:t>x</m:t></m:r></m:e></m:rad></m:oMath></m:oMathPara></w:p>"#
        ),
        m = m
    );
    let at = xml.rfind("<w:sectPr").unwrap();
    let xml = format!("{}{}{}", &xml[..at], paragraphs, &xml[at..]);
    package.set_bytes(main, xml.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A document (`data`) whose first picture has an outer shadow and a reflection (`a:effectLst` of its `pic:spPr`).
pub fn docx_with_picture_effects(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let main = "word/document.xml";
    let xml = String::from_utf8(package.read(main).unwrap()).unwrap();
    let effects = r#"<a:effectLst><a:outerShdw blurRad="50800" dist="38100" dir="2700000" algn="tl" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw><a:reflection blurRad="6350" stA="50000" endA="300" endPos="35000" dir="5400000" sy="-100000" algn="bl" rotWithShape="0"/></a:effectLst>"#;
    let at = xml.find("</pic:spPr>").unwrap();
    let xml = format!("{}{}{}", &xml[..at], effects, &xml[at..]);
    package.set_bytes(main, xml.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// The text of part `name` of a package (`word/document.xml`).
pub fn part_text(data: &[u8], name: &str) -> Option<String> {
    let package = ibis_ooxml::Package::open(data.to_vec()).ok()?;
    String::from_utf8(package.read(name).ok()?).ok()
}

/// A content-controlled paragraph in the second cell: a bounded merge rejects it.
pub fn docx_with_wrapped_cell_paragraph(data:&[u8])->Vec<u8> {
    let mut package=ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let mut xml=ibis_ooxml::XmlDocument::parse(&package.read("word/document.xml").unwrap()).unwrap();
    let body=xml.child(xml.root_element().unwrap(),"body").unwrap();let tbl=xml.child(body,"tbl").unwrap();let tr=xml.child(tbl,"tr").unwrap();
    let cells:Vec<_>=xml.children_named(tr,"tc").collect();let tc=cells[1];let p=xml.child(tc,"p").unwrap();
    let index=xml.children(tc).iter().position(|&n|n==p).unwrap();xml.detach(p);
    let sdt=xml.insert_element(tc,index,"w:sdt");let content=xml.append_element(sdt,"w:sdtContent");xml.insert(content,0,p);
    package.set_xml("word/document.xml",xml);package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A horizontally spanned content-controlled cell, preserving its wrapped paragraph.
pub fn docx_with_wrapped_spanned_cell(data:&[u8])->Vec<u8> {
    let wrapped=docx_with_wrapped_cell_paragraph(data);
    let mut package=ibis_ooxml::Package::open(wrapped).unwrap();
    let mut xml=ibis_ooxml::XmlDocument::parse(&package.read("word/document.xml").unwrap()).unwrap();
    let body=xml.child(xml.root_element().unwrap(),"body").unwrap();let tbl=xml.child(body,"tbl").unwrap();
    let grid=xml.child(tbl,"tblGrid").unwrap();let cols:Vec<_>=xml.children_named(grid,"gridCol").collect();
    let width: i64=cols[1..3].iter().map(|&c|xml.attr_local(c,"w").unwrap().parse::<i64>().unwrap()).sum();
    let row=xml.child(tbl,"tr").unwrap();let cells:Vec<_>=xml.children_named(row,"tc").collect();let pr=xml.child(cells[1],"tcPr").unwrap();
    let tcw=xml.child(pr,"tcW").unwrap();xml.set_attr(tcw,"w:w",&width.to_string());
    let index=xml.children(pr).iter().position(|&n|n==tcw).unwrap()+1;
    let span=xml.insert_element(pr,index,"w:gridSpan");xml.set_attr(span,"w:val","2");xml.detach(cells[2]);
    package.set_xml("word/document.xml",xml);package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A nested table within the first cell, retaining its final paragraph.
pub fn docx_with_nested_table(data:&[u8])->Vec<u8> {
    let mut package=ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let mut xml=ibis_ooxml::XmlDocument::parse(&package.read("word/document.xml").unwrap()).unwrap();
    let body=xml.child(xml.root_element().unwrap(),"body").unwrap();let tbl=xml.child(body,"tbl").unwrap();
    let row=xml.child(tbl,"tr").unwrap();let tc=xml.child(row,"tc").unwrap();let p=xml.child(tc,"p").unwrap();
    let pristine=xml.clone();let nested=xml.import(&pristine,tbl);
    // The structural fixture copies text/table structure, not drawings with unique IDs.
    let drawings:Vec<_>=xml.descendants(nested).into_iter().filter(|&n|matches!(xml.local_name(n),"drawing"|"pict"|"AlternateContent")).collect();
    for n in drawings {xml.detach(n);}
    xml.insert_before(p,nested);
    package.set_xml("word/document.xml",xml);package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// Raw old history for comparing ZIP XML before and after replacement.
pub const ORIGINAL_DELETION_XML: &str = r#"<w:del w:id="841" w:author="Test" w:date="2026-10-01T00:00:00Z" xmlns:ibis="urn:ibis:test:history" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" mc:Ignorable="ibis" ibis:marker="old"><!--old history container--><w:r w:rsidR="12345678"><w:delText xml:space="preserve">X </w:delText></w:r><w:r><w:delText>Y</w:delText></w:r></w:del>"#;

/// An actual source ZIP containing unmodeled deletion attributes/children.
pub fn docx_with_original_deletion_history(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let paragraph = format!("<w:p><w:r><w:t>ab</w:t></w:r>{ORIGINAL_DELETION_XML}<w:r><w:t>cd</w:t></w:r></w:p>");
    let at = xml.rfind("<w:sectPr").unwrap();
    package.set_bytes("word/document.xml", format!("{}{}{}", &xml[..at], paragraph, &xml[at..]).into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// Fixed prefix/suffix/all-deleted scopes and two visible crossing matches.
pub fn docx_with_search_history_edges(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let history = |id: &str| ORIGINAL_DELETION_XML.replace("841", id);
    let paragraphs = format!("<w:p>{}<w:r><w:t>pq</w:t></w:r></w:p><w:p><w:r><w:t>rs</w:t></w:r>{}</w:p><w:p>{}</w:p><w:p><w:r><w:t>ab</w:t></w:r>{}<w:r><w:t xml:space=\"preserve\">cd ab</w:t></w:r>{}<w:r><w:t>cd</w:t></w:r></w:p>", history("843"), history("844"), history("845"), history("841"), history("842"));
    let at = xml.rfind("<w:sectPr").unwrap();
    package.set_bytes("word/document.xml", format!("{}{}{}", &xml[..at], paragraphs, &xml[at..]).into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A field immediately after a two-run original deletion container.
pub const HISTORY_ADJACENT_FIELD_XML: &str = r#"<w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple>"#;
pub fn docx_with_history_adjacent_field(data: &[u8]) -> Vec<u8> {
    let mut package=ibis_ooxml::Package::open(docx_with_original_deletion_history(data)).unwrap();
    let xml=String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let tail="<w:r><w:t>cd</w:t></w:r></w:p>";
    assert_eq!(xml.matches(tail).count(),1);
    package.set_bytes("word/document.xml",xml.replace(tail,&format!("<w:r><w:t>cd</w:t></w:r>{HISTORY_ADJACENT_FIELD_XML}</w:p>")).into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A real footnote source containing the same raw two-run deletion scope.
pub fn docx_with_note_deletion_history(data: &[u8]) -> Vec<u8> {
    let mut package=ibis_ooxml::Package::open(docx_with_complex_paragraph(data)).unwrap();
    let xml=String::from_utf8(package.read("word/footnotes.xml").unwrap()).unwrap();
    let old=r#"<w:r><w:t xml:space="preserve"> 脚注の文</w:t></w:r>"#;
    assert_eq!(xml.matches(old).count(),1);
    let text=format!(r#"<w:r><w:t xml:space="preserve"> ab</w:t></w:r>{ORIGINAL_DELETION_XML}<w:r><w:t>cd</w:t></w:r>"#);
    package.set_bytes("word/footnotes.xml",xml.replace(old,&text).into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// Qualified footnote id alongside an ignored foreign id, with original history.
pub fn docx_with_note_id_namespace(data: &[u8], variant: usize) -> Vec<u8> {
    let mut package=ibis_ooxml::Package::open(docx_with_note_deletion_history(data)).unwrap();
    let xml=String::from_utf8(package.read("word/footnotes.xml").unwrap()).unwrap();
    let doc=roxmltree::Document::parse(&xml).unwrap();
    let ns="http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let note=doc.descendants().find(|n|n.is_element()&&n.tag_name().name()=="footnote"&&n.attribute((ns,"id")).and_then(|v|v.parse::<i32>().ok()).is_some_and(|id|id>0)).unwrap();
    let begin=note.range().start;let end=xml[begin..].find('>').unwrap()+begin;
    let id=note.attribute((ns,"id")).unwrap();let proper=format!("w:id=\"{id}\"");let foreign="f:id=\"424242\"";
    let attrs=match variant {0=>format!("{proper} {foreign}"),1=>format!("{foreign} {proper}"),2=>format!("{foreign} x:id=\"{id}\""),_=>panic!("unsupported note namespace variant")};
    let changed=xml[begin..end].replace(&proper,&attrs)+" xmlns:f=\"urn:ibis:test:foreign\" xmlns:x=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"f\"";
    let updated=format!("{}{}{}",&xml[..begin],changed,&xml[end..]);package.set_bytes("word/footnotes.xml",updated.into_bytes());package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

pub fn positive_footnote_ids(xml: &str) -> Vec<u32> {
    let doc=roxmltree::Document::parse(xml).unwrap();let ns="http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    doc.descendants().filter(|n|n.is_element()&&n.tag_name().namespace()==Some(ns)&&n.tag_name().name()=="footnote").filter_map(|n|n.attribute((ns,"id")).and_then(|v|v.parse::<u32>().ok()).filter(|&id|id>0)).collect()
}

/// Fixed source deletion followed by a hyperlink, for visible typing across both.
pub fn docx_with_linked_history_tail(data: &[u8]) -> Vec<u8> {
    let original = docx_with_original_deletion_history(data);
    let mut package = ibis_ooxml::Package::open(original).unwrap();
    let rid = package.add_relationship("word/document.xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink", "https://example.com/history-tail", true).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let tail = "<w:r><w:t>cd</w:t></w:r></w:p>";
    assert_eq!(xml.matches(tail).count(), 1);
    package.set_bytes("word/document.xml", xml.replace(tail, &format!("<w:hyperlink r:id=\"{rid}\"><w:r><w:t>cd</w:t></w:r></w:hyperlink></w:p>")).into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// The original direct insertion scope carries the same extra XML metadata.
pub fn docx_with_original_insertion_history(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(docx_with_original_deletion_history(data)).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let insertion = ORIGINAL_DELETION_XML
        .replace("w:delText", "w:t")
        .replace("w:del", "w:ins")
        .replace("841", "941");
    assert_eq!(xml.matches(ORIGINAL_DELETION_XML).count(), 1);
    package.set_bytes(
        "word/document.xml",
        xml.replace(ORIGINAL_DELETION_XML, &insertion).into_bytes(),
    );
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// Legal ignored foreign attributes alongside qualified source-history attributes.
pub fn docx_with_history_namespace_attributes(data: &[u8], variant: usize) -> Vec<u8> {
    let mut package =
        ibis_ooxml::Package::open(docx_with_original_insertion_history(data)).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let old = ORIGINAL_DELETION_XML
        .replace("w:delText", "w:t")
        .replace("w:del", "w:ins")
        .replace("841", "941");
    let standard = r#"w:id="941" w:author="Test" w:date="2026-10-01T00:00:00Z""#;
    let foreign = r#"f:id="1" f:author="Foreign" f:date="1900-01-01T00:00:00Z""#;
    let attrs = match variant {
        0 => format!("{standard} {foreign}"),
        1 => format!("{foreign} {standard}"),
        2 => format!("{foreign} {}", standard.replace("w:", "x:")),
        _ => panic!("unsupported namespace variant"),
    };
    let changed = old.replace(standard, &attrs).replace(
        "mc:Ignorable=\"ibis\"",
        "xmlns:f=\"urn:ibis:test:foreign\" xmlns:x=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" mc:Ignorable=\"ibis f\"",
    );
    assert_eq!(xml.matches(&old).count(), 1);
    package.set_bytes(
        "word/document.xml",
        xml.replace(&old, &changed).into_bytes(),
    );
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// Expanded-name IDs, also rejecting malformed duplicate expanded attributes.
pub fn word_revision_ids(xml: &str) -> Vec<u64> {
    let doc = roxmltree::Document::parse(xml).unwrap();
    let ns = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    doc.descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().namespace() == Some(ns)
                && matches!(n.tag_name().name(), "ins" | "del")
        })
        .map(|n| n.attribute((ns, "id")).unwrap().parse().unwrap())
        .collect()
}

/// A non-modeled comment inside a source insertion run, for save refusal.
pub fn docx_with_insertion_run_comment(data: &[u8]) -> Vec<u8> {
    let mut package =
        ibis_ooxml::Package::open(docx_with_original_insertion_history(data)).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let run = "<w:r w:rsidR=\"12345678\">";
    assert_eq!(xml.matches(run).count(), 1);
    package.set_bytes(
        "word/document.xml",
        xml.replace(run, &format!("{run}<!--original run payload-->"))
            .into_bytes(),
    );
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A complex PAGE can legally belong wholly to an insertion scope.
pub const INSERTION_COMPLEX_PAGE_XML: &str = r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#;

/// PAGE inside an original insertion, for reader/rejection history ownership.
pub fn docx_with_insertion_page_field(data: &[u8]) -> Vec<u8> {
    docx_with_insertion_page_field_at(data, 2)
}

/// Position the wholly owned field before, between, or after the text runs.
pub fn docx_with_insertion_page_field_at(data: &[u8], at: usize) -> Vec<u8> {
    let mut package =
        ibis_ooxml::Package::open(docx_with_original_insertion_history(data)).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let insertion = ORIGINAL_DELETION_XML
        .replace("w:delText", "w:t")
        .replace("w:del", "w:ins")
        .replace("841", "941");
    assert_eq!(xml.matches(&insertion).count(), 1);
    let with_field = match at {
        0 => insertion.replace(
            "<w:r w:rsidR=\"12345678\">",
            &format!("{INSERTION_COMPLEX_PAGE_XML}<w:r w:rsidR=\"12345678\">"),
        ),
        1 => insertion.replace(
            "<w:r><w:t>Y</w:t></w:r>",
            &format!("{INSERTION_COMPLEX_PAGE_XML}<w:r><w:t>Y</w:t></w:r>"),
        ),
        2 => insertion.replace("</w:ins>", &format!("{INSERTION_COMPLEX_PAGE_XML}</w:ins>")),
        _ => panic!("unsupported field placement"),
    };
    package.set_bytes(
        "word/document.xml",
        xml.replace(&insertion, &with_field).into_bytes(),
    );
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A source insertion under one hyperlink, with parent attributes to retain.
pub fn docx_with_linked_insertion_history(data: &[u8]) -> Vec<u8> {
    let mut package =
        ibis_ooxml::Package::open(docx_with_original_insertion_history(data)).unwrap();
    let rid = package
        .add_relationship(
            "word/document.xml",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink",
            "https://example.com/original-insertion",
            true,
        )
        .unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let insertion = ORIGINAL_DELETION_XML
        .replace("w:delText", "w:t")
        .replace("w:del", "w:ins")
        .replace("841", "941");
    assert_eq!(xml.matches(&insertion).count(), 1);
    let link = format!(
        "<w:hyperlink r:id=\"{rid}\" w:history=\"0\" w:tgtFrame=\"frame\">{insertion}</w:hyperlink>"
    );
    package.set_bytes(
        "word/document.xml",
        xml.replace(&insertion, &link).into_bytes(),
    );
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}



/// Original reference scopes including unknown attributes, in document order.
pub fn story_reference_xmls(xml: &str) -> Vec<String> {
    let doc = roxmltree::Document::parse(xml).unwrap();
    doc.descendants()
        .filter(|n| {
            n.is_element()
                && n.tag_name().namespace()
                    == Some("http://schemas.openxmlformats.org/wordprocessingml/2006/main")
                && matches!(n.tag_name().name(), "headerReference" | "footerReference")
        })
        .map(|n| xml[n.range()].to_string())
        .collect()
}

/// Default header/footer references opposed by ignored ids for empty even parts.
pub fn docx_with_story_relationship_namespace(data: &[u8], variant: usize) -> Vec<u8> {
    story_relationship_namespace_fixture(data, variant, false)
}

pub fn docx_with_all_story_relationship_namespaces(data: &[u8], variant: usize) -> Vec<u8> {
    story_relationship_namespace_fixture(data, variant, true)
}

fn story_relationship_namespace_fixture(data: &[u8], variant: usize, all_types: bool) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let ns = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let rns = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    let types: &[&str] = if all_types {
        &["default", "first", "even"]
    } else {
        &["default"]
    };
    for element in ["headerReference", "footerReference"] {
        for &wanted in types {
            let parsed = roxmltree::Document::parse(&xml).unwrap();
            let nodes: Vec<_> = parsed
                .descendants()
                .filter(|n| {
                    n.is_element()
                        && n.tag_name().namespace() == Some(ns)
                        && n.tag_name().name() == element
                })
                .collect();
            let real = nodes
                .iter()
                .find(|n| n.attribute((ns, "type")) == Some(wanted))
                .unwrap();
            let wrong = nodes
                .iter()
                .find(|n| {
                    n.attribute((ns, "type"))
                        == Some(if wanted == "default" {
                            "even"
                        } else {
                            "default"
                        })
                })
                .unwrap()
                .attribute((rns, "id"))
                .unwrap();
            let range = real.range();
            let old = &xml[range.clone()];
            let id = real.attribute((rns, "id")).unwrap();
            let proper = format!("r:id=\"{id}\"");
            let foreign = format!("f:id=\"{wrong}\"");
            let attrs = match variant {
                0 => format!("{proper} {foreign}"),
                1 => format!("{foreign} {proper}"),
                2 => format!("{foreign} q:id=\"{id}\""),
                _ => panic!("unsupported story namespace variant"),
            };
            let changed = old.replace(&proper,&attrs).replace("/>", " xmlns:f=\"urn:ibis:test:foreign\" xmlns:q=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"f\"/>");
            assert_ne!(old, changed);
            xml.replace_range(range, &changed);
        }
    }
    package.set_bytes("word/document.xml", xml.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// An image embed with an opposing ignored id pointing to distinct PNG bytes.
pub fn docx_with_picture_relationship_namespace(
    data: &[u8],
    foreign_png: &[u8],
    variant: usize,
) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    assert_ne!(package.read("word/media/image1.png").unwrap(), foreign_png);
    package.set_bytes("word/media/namespace-foreign.png", foreign_png.to_vec());
    let wrong = package
        .add_relationship(
            "word/document.xml",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image",
            "media/namespace-foreign.png",
            false,
        )
        .unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let begin = xml.find("<a:blip r:embed=\"").unwrap() + "<a:blip ".len();
    let end = xml[begin..].find("/>").unwrap() + begin;
    let real = &xml[begin..end];
    let foreign = format!("f:embed=\"{wrong}\"");
    let attrs = match variant {
        0 => format!("{real} {foreign}"),
        1 => format!("{foreign} {real}"),
        2 => format!("{foreign} {}", real.replace("r:embed", "q:embed")),
        _ => panic!("unsupported image namespace variant"),
    };
    let attrs = format!(
        "{attrs} xmlns:f=\"urn:ibis:test:foreign\" xmlns:q=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"f\""
    );
    package.set_bytes(
        "word/document.xml",
        format!("{}{}{}", &xml[..begin], attrs, &xml[end..]).into_bytes(),
    );
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// Opposing ignored relationship id alongside a legal source hyperlink id.
pub fn docx_with_link_relationship_namespace(data: &[u8], variant: usize) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(docx_with_linked_insertion_history(data)).unwrap();
    let wrong = package.add_relationship("word/document.xml", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink", "https://example.com/foreign-link", true).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let begin = xml.find("<w:hyperlink r:id=\"").unwrap() + "<w:hyperlink ".len();
    let end = xml[begin..].find(" w:history").unwrap() + begin;
    let real = &xml[begin..end];
    let foreign = format!("f:id=\"{wrong}\"");
    let attrs = match variant {
        0 => format!("{real} {foreign}"),
        1 => format!("{foreign} {real}"),
        2 => format!("{foreign} {}", real.replace("r:id", "q:id")),
        _ => panic!("unsupported relationship namespace variant"),
    };
    let attrs = format!("{attrs} xmlns:f=\"urn:ibis:test:foreign\" xmlns:q=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" mc:Ignorable=\"f\"");
    let changed = format!("{}{}{}", &xml[..begin], attrs, &xml[end..]);
    package.set_bytes("word/document.xml", changed.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}


pub fn docx_with_linked_deletion_history(data: &[u8]) -> Vec<u8> {
    let linked = docx_with_linked_insertion_history(data);
    let mut package = ibis_ooxml::Package::open(linked).unwrap();
    let original = ORIGINAL_DELETION_XML
        .replace("w:delText", "w:t")
        .replace("w:del", "w:ins")
        .replace("841", "941");
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    assert!(xml.contains(&original));
    package.set_bytes(
        "word/document.xml",
        xml.replace(&original, ORIGINAL_DELETION_XML).into_bytes(),
    );
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// Adjacent equal URLs have separate parent metadata and source revision IDs.
pub fn docx_with_distinct_link_frames(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(docx_with_linked_insertion_history(data)).unwrap();
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let scope = ORIGINAL_DELETION_XML
        .replace("w:delText", "w:t")
        .replace("w:del", "w:ins")
        .replace("841", "941");
    assert_eq!(xml.matches(&scope).count(), 1);
    let at = xml.find(&scope).unwrap();
    let start = xml[..at].rfind("<w:hyperlink ").unwrap();
    let end = at + scope.len() + "</w:hyperlink>".len();
    assert_eq!(&xml[end - "</w:hyperlink>".len()..end], "</w:hyperlink>");
    let link = &xml[start..end];
    let first = link.replace("w:tgtFrame=\"frame\"", "w:tgtFrame=\"first\"");
    let second_scope = scope
        .replace("941", "951")
        .replace("ibis:marker=\"old\"", "ibis:marker=\"second\"")
        .replace("old history container", "second history container")
        .replace(">X </w:t>", ">M </w:t>")
        .replace(">Y</w:t>", ">N</w:t>");
    let second = link
        .replace(&scope, &second_scope)
        .replace("w:tgtFrame=\"frame\"", "w:tgtFrame=\"second\"");
    package.set_bytes(
        "word/document.xml",
        xml.replace(link, &format!("{first}{second}")).into_bytes(),
    );
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// An effect `p:par`: class, preset, subtype, node type, delay, the shape,
/// and its behaviours.
#[allow(clippy::too_many_arguments)]
fn anim_effect(id: u32, class: &str, preset: u32, subtype: u32, node: &str, delay: u32, spid: u32, behaviours: &str) -> String {
    format!(
        r#"<p:par><p:cTn id="{id}" presetID="{preset}" presetClass="{class}" presetSubtype="{subtype}" fill="hold" nodeType="{node}"><p:stCondLst><p:cond delay="{delay}"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:cTn id="{}" dur="1" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="{}"/></p:to></p:set>{behaviours}</p:childTnLst></p:cTn></p:par>"#,
        id + 1,
        if class == "exit" { "hidden" } else { "visible" }
    )
}

/// A group of effects `delay` after its click group starts.
fn anim_timed(id: u32, delay: u32, effects: &str) -> String {
    format!(r#"<p:par><p:cTn id="{id}" fill="hold"><p:stCondLst><p:cond delay="{delay}"/></p:stCondLst><p:childTnLst>{effects}</p:childTnLst></p:cTn></p:par>"#)
}

fn anim_click(id: u32, groups: &str) -> String {
    format!(r#"<p:par><p:cTn id="{id}" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst>{groups}</p:childTnLst></p:cTn></p:par>"#)
}

fn anim_timing() -> String {
    let fade = |id: u32, spid: u32, dur: u32, dir: &str| format!(r#"<p:animEffect transition="{dir}" filter="fade"><p:cBhvr><p:cTn id="{id}" dur="{dur}"/><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl></p:cBhvr></p:animEffect>"#);
    let fly = |id: u32, spid: u32| format!(r##"<p:anim calcmode="lin" valueType="num"><p:cBhvr additive="base"><p:cTn id="{id}" dur="500" fill="hold"/><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl><p:attrNameLst><p:attrName>ppt_y</p:attrName></p:attrNameLst></p:cBhvr><p:tavLst><p:tav tm="0"><p:val><p:strVal val="1+#ppt_h/2"/></p:val></p:tav><p:tav tm="100000"><p:val><p:strVal val="#ppt_y"/></p:val></p:tav></p:tavLst></p:anim>"##);
    // Click 1: the title appears; after it the subtitle fades in (0.5 s),
    // and with that the title flies in from the bottom. Click 2: the
    // subtitle fades out.
    let first = anim_click(
        3,
        &(anim_timed(4, 0, &anim_effect(5, "entr", 1, 0, "clickEffect", 0, 4, ""))
            + &anim_timed(7, 0, &(anim_effect(8, "entr", 10, 0, "afterEffect", 0, 5, &fade(10, 5, 500, "in")) + &anim_effect(11, "entr", 2, 4, "withEffect", 0, 4, &fly(13, 4))))),
    );
    let second = anim_click(14, &anim_timed(15, 0, &anim_effect(16, "exit", 10, 0, "clickEffect", 0, 5, &fade(18, 5, 500, "out"))));
    format!(
        r#"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst>{first}{second}</p:childTnLst></p:cTn><p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"#
    )
}

/// The plan test file (`data`, test/editing/pptx-plan.pptx) with
/// animations on its first slide, as PowerPoint writes them: click 1, the
/// title (shape 4) appears; after it the subtitle (5) fades in (0.5 s) and
/// with that the title flies in from the bottom; click 2, the subtitle fades
/// out.
pub fn pptx_with_animations(data: &[u8]) -> Vec<u8> {
    let data = data.to_vec();
    let mut package = ibis_ooxml::Package::open(data).unwrap();
    let slide = String::from_utf8(package.read("ppt/slides/slide1.xml").unwrap()).unwrap();
    let slide = slide.replacen("<p:clrMapOvr>", &format!("{}<p:clrMapOvr>", anim_timing()), 1);
    package.set_bytes("ppt/slides/slide1.xml", slide.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}


/// The bytes of part `name` of a package.
pub fn part_bytes(data: &[u8], name: &str) -> Option<Vec<u8>> {
    ibis_ooxml::Package::open(data.to_vec()).ok()?.read(name).ok()
}

/// A Word document (`data`) made a .docm: its main part's content type
/// the macro-enabled one and `vba` as word/vbaProject.bin related to it.
pub fn docx_with_macros(data: &[u8], vba: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    package.set_override("word/document.xml", "application/vnd.ms-word.document.macroEnabled.main+xml").unwrap();
    package.set_bytes("word/vbaProject.bin", vba.to_vec());
    package.set_override("word/vbaProject.bin", "application/vnd.ms-office.vbaProject").unwrap();
    package.add_relationship("word/document.xml", "http://schemas.microsoft.com/office/2006/relationships/vbaProject", "vbaProject.bin", false).unwrap();
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A Word document (`data`) whose footers end with a paragraph
/// 「PAGE / NUMPAGES (SECTIONPAGES \* ROMAN)」, the fields' saved results
/// 「1 / 9 (IX)」.
pub fn docx_with_page_count_footer(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let field = |code: &str, shown: &str| {
        format!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> {code} </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>{shown}</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
        )
    };
    let paragraph = format!(
        r#"<w:p>{}<w:r><w:t xml:space="preserve"> / </w:t></w:r>{}<w:r><w:t xml:space="preserve"> (</w:t></w:r>{}<w:r><w:t>)</w:t></w:r></w:p>"#,
        field("PAGE", "1"),
        field("NUMPAGES", "9"),
        field("SECTIONPAGES \\* ROMAN", "IX")
    );
    for name in ["word/footer1.xml", "word/footer2.xml", "word/footer3.xml"] {
        let Ok(bytes) = package.read(name) else { continue };
        let xml = String::from_utf8(bytes).unwrap();
        let at = xml.rfind("</w:ftr>").unwrap();
        package.set_bytes(name, format!("{}{}{}", &xml[..at], paragraph, &xml[at..]).into_bytes());
    }
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// `data` (a .docx) whose headers hold five paragraphs of exactly 18 pt
/// lines (90 pt) under a top margin of 45 pt and a header distance of
/// 42.5 pt (GitHub issue #10): the header ends below the margin.
pub fn docx_with_tall_header(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    while let Some(at) = xml.find("<w:pgMar") {
        let end = at + xml[at..].find("/>").unwrap() + 2;
        xml.replace_range(at..end, r#"<w:PGMAR w:top="900" w:right="1440" w:bottom="1440" w:left="1440" w:header="850" w:footer="720" w:gutter="0"/>"#);
    }
    let xml = xml.replace("<w:PGMAR", "<w:pgMar");
    package.set_bytes("word/document.xml", xml.into_bytes());
    let paragraph = r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="360" w:lineRule="exact"/></w:pPr><w:r><w:t>ヘッダーの行</w:t></w:r></w:p>"#.repeat(5);
    let names: Vec<String> = package.names().filter(|n| n.starts_with("word/header") && n.ends_with(".xml")).map(str::to_string).collect();
    for name in names {
        let xml = String::from_utf8(package.read(&name).unwrap()).unwrap();
        let at = xml.rfind("</w:hdr>").unwrap();
        package.set_bytes(&name, format!("{}{}{}", &xml[..at], paragraph, &xml[at..]).into_bytes());
    }
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// `data` (a .docx) with its last section in two columns with a line
/// between (`w:cols w:num="2" w:sep="1"`), and 80 numbered paragraphs at
/// its end (more than a column holds).
pub fn docx_with_two_columns(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.rfind("<w:sectPr").unwrap();
    let lines: String = (1..=80).map(|i| format!("<w:p><w:r><w:t>段組みの行 {i}</w:t></w:r></w:p>")).collect();
    xml.insert_str(at, &lines);
    let sect = xml.rfind("<w:sectPr").unwrap();
    let mut tail = xml[sect..].to_string();
    if let Some(c) = tail.find("<w:cols") {
        let close = c + tail[c..].find('>').unwrap();
        let end = if tail[..close].ends_with('/') { close + 1 } else { tail.find("</w:cols>").unwrap() + "</w:cols>".len() };
        tail.replace_range(c..end, r#"<w:cols w:num="2" w:space="425" w:sep="1"/>"#);
    } else {
        let e = tail.find("</w:sectPr>").unwrap();
        tail.insert_str(e, r#"<w:cols w:num="2" w:space="425" w:sep="1"/>"#);
    }
    xml.replace_range(sect.., &tail);
    package.set_bytes("word/document.xml", xml.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A presentation (`data`) whose charts each get an embedded workbook (an
/// empty Sheet1, as their data's references name), related from the chart
/// part as PowerPoint relates it (`package`), for the chart data's cells.
pub fn pptx_with_embedded_chart_data(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let charts: Vec<String> = package.names().filter(|n| n.starts_with("ppt/charts/chart") && n.ends_with(".xml")).map(str::to_string).collect();
    let book = crate::csv::workbook_from("", "Sheet1").unwrap();
    for (i, chart) in charts.iter().enumerate() {
        let part = format!("ppt/embeddings/Microsoft_Excel_Worksheet{i}.xlsx");
        package.set_bytes(&part, book.clone());
        package.set_override(&part, "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet").unwrap();
        package
            .add_relationship(chart, "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package", &format!("../embeddings/Microsoft_Excel_Worksheet{i}.xlsx"), false)
            .unwrap();
    }
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// `data` (a .docx) with a table style formatting the heading row (blue,
/// bold) and the rows' bands (green), and a 3 × 2 table of it at the end
/// (the heading row and the bands on).
pub fn docx_with_styled_table(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let style = r#"<w:style w:type="table" w:styleId="Banded"><w:name w:val="Banded"/><w:tblPr><w:tblStyleRowBandSize w:val="1"/><w:tblBorders><w:top w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:bottom w:val="single" w:sz="4" w:space="0" w:color="808080"/><w:insideH w:val="single" w:sz="4" w:space="0" w:color="808080"/></w:tblBorders></w:tblPr><w:tblStylePr w:type="firstRow"><w:rPr><w:b/></w:rPr><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="4472C4"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type="band1Horz"><w:tcPr><w:shd w:val="clear" w:color="auto" w:fill="E2EFDA"/></w:tcPr></w:tblStylePr></w:style>"#;
    let styles = String::from_utf8(package.read("word/styles.xml").unwrap()).unwrap();
    let at = styles.rfind("</w:styles>").unwrap();
    package.set_bytes("word/styles.xml", format!("{}{style}{}", &styles[..at], &styles[at..]).into_bytes());
    let cell = |t: &str| format!(r#"<w:tc><w:tcPr><w:tcW w:w="2000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>{t}</w:t></w:r></w:p></w:tc>"#);
    let rows: String = ["見出し", "一行目", "二行目"].iter().map(|t| format!("<w:tr>{}{}</w:tr>", cell(t), cell("値"))).collect();
    let table = format!(r#"<w:tbl><w:tblPr><w:tblStyle w:val="Banded"/><w:tblW w:w="4000" w:type="dxa"/><w:tblLook w:val="0420" w:firstRow="1" w:lastRow="0" w:firstColumn="0" w:lastColumn="0" w:noHBand="0" w:noVBand="1"/></w:tblPr><w:tblGrid><w:gridCol w:w="2000"/><w:gridCol w:w="2000"/></w:tblGrid>{rows}</w:tbl><w:p/>"#);
    let xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let at = xml.rfind("<w:sectPr").unwrap();
    package.set_bytes("word/document.xml", format!("{}{table}{}", &xml[..at], &xml[at..]).into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// A workbook (`data`) with part `chart_part` replaced with `chart_xml`.
pub fn xlsx_with_chart_xml(data: &[u8], chart_part: &str, chart_xml: &str) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    package.set_bytes(chart_part, chart_xml.as_bytes().to_vec());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}


/// `data` (a .docx) with a list of picture bullets (`w:numPicBullet`, the report's first
/// picture at 9 pt) as its last two paragraphs of the body: "画像の行頭文字 1" and "2".
pub fn docx_with_picture_bullet(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let mut numbering = String::from_utf8(package.read("word/numbering.xml").unwrap()).unwrap();
    let bullet = r##"<w:numPicBullet w:numPicBulletId="0"><w:pict><v:shape id="_x0000_i1025" type="#_x0000_t75" style="width:9pt;height:9pt" o:bullet="t" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><v:imagedata r:id="rIdPicBullet" o:title=""/></v:shape></w:pict></w:numPicBullet>"##;
    let at = numbering.find("<w:abstractNum ").unwrap();
    numbering.insert_str(at, bullet);
    let definition = r#"<w:abstractNum w:abstractNumId="90"><w:multiLevelType w:val="hybridMultilevel"/><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="bullet"/><w:lvlText w:val="&#xF0B7;"/><w:lvlPicBulletId w:val="0"/><w:lvlJc w:val="left"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum>"#;
    let at = numbering.find("<w:num ").unwrap();
    numbering.insert_str(at, definition);
    let at = numbering.rfind("</w:numbering>").unwrap();
    numbering.insert_str(at, r#"<w:num w:numId="90"><w:abstractNumId w:val="90"/></w:num>"#);
    package.set_bytes("word/numbering.xml", numbering.into_bytes());
    let image = package.names().find(|n| n.ends_with(".png")).unwrap().to_string();
    let target = image.strip_prefix("word/").unwrap().to_string();
    package.set_bytes(
        "word/_rels/numbering.xml.rels",
        format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rIdPicBullet" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="{target}"/></Relationships>"#).into_bytes(),
    );
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let item = |t: &str| format!(r#"<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="90"/></w:numPr></w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#);
    let at = xml.rfind("<w:sectPr").unwrap();
    xml.insert_str(at, &format!("{}{}", item("画像の行頭文字 1"), item("画像の行頭文字 2")));
    package.set_bytes("word/document.xml", xml.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// `data` (the editing report .docx, whose heading styles have the ids 1 to 9) with an outline
/// at the end of its body: 章A (heading 1), A本文, 節A-1 (heading 2), A-1本文, 章B (heading 1), B本文.
pub fn docx_with_outline(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let paragraph = |style: Option<&str>, text: &str| match style {
        Some(s) => format!(r#"<w:p><w:pPr><w:pStyle w:val="{s}"/></w:pPr><w:r><w:t>{text}</w:t></w:r></w:p>"#),
        None => format!(r#"<w:p><w:r><w:t>{text}</w:t></w:r></w:p>"#),
    };
    let outline: String = [(Some("1"), "章A"), (None, "A本文"), (Some("2"), "節A-1"), (None, "A-1本文"), (Some("1"), "章B"), (None, "B本文")].iter().map(|(s, t)| paragraph(*s, t)).collect();
    let at = xml.rfind("<w:sectPr").unwrap();
    xml.insert_str(at, &outline);
    package.set_bytes("word/document.xml", xml.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// `data` (the editing report .docx) with the result of an `INCLUDEPICTURE` field for `file`: the first picture of its body
/// (the run with a `w:drawing`) in the field's result.
pub fn docx_with_include_picture(data: &[u8], file: &str) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let drawing = xml.find("<w:drawing").expect("a picture");
    let start = xml[..drawing].rfind("<w:r>").or_else(|| xml[..drawing].rfind("<w:r ")).unwrap();
    let end = drawing + xml[drawing..].find("</w:r>").unwrap() + "</w:r>".len();
    let run = xml[start..end].to_string();
    let wrapped = format!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> INCLUDEPICTURE "{file}" \d </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{run}<w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    );
    xml.replace_range(start..end, &wrapped);
    package.set_bytes("word/document.xml", xml.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// `data` (the editing report .docx) with a tracked move at the end of its body: a paragraph whose text "移動前" is a `w:moveFrom` and
/// one whose text "移動後" is a `w:moveTo` (the same words as when Word moves a text), then a plain one.
pub fn docx_with_tracked_move(data: &[u8]) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let attrs = r#"w:author="Tester" w:date="2026-10-14T00:00:00Z""#;
    let moved = format!(
        r#"<w:p><w:r><w:t xml:space="preserve">前文 </w:t></w:r><w:moveFrom w:id="9001" {attrs}><w:r><w:t>移動前</w:t></w:r></w:moveFrom></w:p><w:p><w:r><w:t xml:space="preserve">後文 </w:t></w:r><w:moveTo w:id="9002" {attrs}><w:r><w:t>移動後</w:t></w:r></w:moveTo></w:p>"#
    );
    let at = xml.rfind("<w:sectPr").unwrap();
    xml.insert_str(at, &moved);
    package.set_bytes("word/document.xml", xml.into_bytes());
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}

/// `data` (a .docx) with a table of contents (`TOC \o "1-3" \h \z \u`, in a content control of the "Table of Contents"
/// gallery) at the start of the body that was never generated, as a program that writes the field and leaves its result to
/// Word writes it (GitHub issue #112): `result` is the text shown in it (empty for none), and `dirty` marks the field
/// `w:dirty="true"`; `update_fields` adds `w:updateFields` to the settings.
pub fn docx_with_unupdated_toc(data: &[u8], result: &str, dirty: bool, update_fields: bool) -> Vec<u8> {
    let mut package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    let mut xml = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let dirty = if dirty { r#" w:dirty="true""# } else { "" };
    let shown = if result.is_empty() { String::new() } else { format!("<w:r><w:t>{result}</w:t></w:r>") };
    let toc = format!(
        r#"<w:sdt><w:sdtPr><w:docPartObj><w:docPartGallery w:val="Table of Contents"/><w:docPartUnique/></w:docPartObj></w:sdtPr><w:sdtContent><w:p><w:r><w:t>目次</w:t></w:r></w:p><w:p><w:r><w:fldChar w:fldCharType="begin"{dirty}/></w:r><w:r><w:instrText xml:space="preserve"> TOC \o "1-3" \h \z \u </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>{shown}<w:r><w:fldChar w:fldCharType="end"/></w:r></w:p></w:sdtContent></w:sdt>"#
    );
    let at = xml.find("<w:body>").unwrap() + "<w:body>".len();
    xml.insert_str(at, &toc);
    package.set_bytes("word/document.xml", xml.into_bytes());
    if update_fields {
        let settings = String::from_utf8(package.read("word/settings.xml").unwrap()).unwrap();
        let start = settings.find("<w:settings").unwrap();
        let end = start + settings[start..].find('>').unwrap();
        let settings = format!("{}><w:updateFields w:val=\"true\"/>{}", &settings[..end], &settings[end + 1..]);
        package.set_bytes("word/settings.xml", settings.into_bytes());
    }
    package.save(ibis_ooxml::SaveOptions::default()).unwrap()
}
