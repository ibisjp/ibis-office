// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Synthetic namespace-boundary probes; none is a native Excel truth input.
use ibis_doc_model::{CellRange, Command, Document, SheetCommand, StyleChange};
use ibis_calc::{CellValue, eval::{Stored, evaluate_cell}};
use ibis_doc_model::sheet_calc::BookView;
use ibis_formats::SaveOptions;

const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const VERSION: &str = "http://schemas.microsoft.com/office/spreadsheetml/2024/workbookCompatibilityVersion";
const URI: &str = "{D14903EA-33C4-47F7-8F05-3474C54BE107}";

#[test]
fn compatibility_requires_correct_ancestors_uri_and_attributes() {
    let stock=std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    for (kind, expected) in [("absent",None),("valid1",Some(1)),("valid2",Some(2)),("future7",Some(7)),("foreign_list",None),("foreign_ext",None),("wrong_uri",None),("foreign_version",None),("warn3_set2",None),("warn0_set2",None),("duplicate",None),("foreign_root",None),("wrong_ext_name",None),("shadow_foreign_list",Some(2))] {
        let mut package=ibis_ooxml::Package::open(stock.clone()).unwrap();
        let doc=package.xml_mut("xl/workbook.xml").unwrap();let root=doc.root_element().unwrap();
        let old:Vec<_>=doc.elements(root).filter(|&n|doc.local_name(n)=="extLst" && doc.namespace(n)==Some(MAIN)).collect();
        for node in old {doc.detach(node);}
        if kind=="foreign_root" {doc.set_attr(root,"xmlns","urn:ibis:foreign-root-probe");}
        if kind=="shadow_foreign_list" {
            let list=doc.append_element(root,"foreign:extLst");doc.set_attr(list,"xmlns:foreign","urn:ibis:foreign-list-probe");
            let ext=doc.append_element(list,"ext");doc.set_attr(ext,"uri",URI);doc.set_attr(ext,"xmlns:wcv",VERSION);
            let node=doc.append_element(ext,"wcv:version");doc.set_attr(node,"setVersion","1");
        }
        if kind!="absent" {
            let list_name=if kind=="foreign_list" {"foreign:extLst"} else {"extLst"};
            let list=doc.append_element(root,list_name);doc.set_attr(list,"xmlns:foreign","urn:ibis:foreign-compatibility-probe");
            if kind=="foreign_root" {doc.set_attr(list,"xmlns",MAIN);}
            let ext_name=if kind=="foreign_ext" {"foreign:ext"} else if kind=="wrong_ext_name" {"carrier"} else {"ext"};
            let ext=doc.append_element(list,ext_name);doc.set_attr(ext,"uri",if kind=="wrong_uri" {"urn:ibis:wrong-compatibility-probe"} else {URI});
            doc.set_attr(ext,"xmlns:wcv",if kind=="foreign_version" {"urn:ibis:foreign-version-probe"} else {VERSION});
            let node=doc.append_element(ext,"wcv:version");
            doc.set_attr(node,"setVersion",match kind {"valid1"=>"1","future7"=>"7",_=>"2"});
            if kind=="warn3_set2" {doc.set_attr(node,"warnBelowVersion","3");}
            if kind=="warn0_set2" {doc.set_attr(node,"warnBelowVersion","0");}
            if kind=="duplicate" {let node=doc.append_element(ext,"wcv:version");doc.set_attr(node,"setVersion","1");}
        }
        let original_xml=package.read("xl/workbook.xml").unwrap();let original=package.save(Default::default()).unwrap();
        let opened=ibis_formats::open(original.clone()).unwrap();
        assert_eq!(opened.presentation.workbook.as_deref().unwrap().compatibility_version,expected,"{kind}");
        let view=BookView::new(&opened.presentation);let modern=expected.is_some_and(|v|v>=2);
        for (formula,legacy,modern_value) in [("LEN(\"😀x\")",CellValue::Number(3.),CellValue::Number(2.)),("MID(\"😀x\",1,1)",CellValue::Text("�".into()),CellValue::Text("😀".into())),("REPLACE(\"😀x\",1,1,\"Q\")",CellValue::Text("Q�x".into()),CellValue::Text("Qx".into())),("FIND(\"x\",\"😀x\")",CellValue::Number(3.),CellValue::Number(2.)),("SEARCH(\"?x\",\"😀x\")",CellValue::Number(2.),CellValue::Number(1.))] {
            assert_eq!(evaluate_cell(&view,&Stored,0,0,0,&ibis_calc::formula::parse(formula).unwrap()),if modern {modern_value} else {legacy},"{kind}: {formula}");
        }
        let mut d=Document::new(opened.presentation.clone());let slide=d.presentation().slides[0].id;
        let sh=ibis_doc_model::sheet_calc::sheet_of_slide(&d.presentation().slides[0]).unwrap();
        let was_bold=sh.styles[sh.cell(0,0).unwrap().style as usize].bold;
        d.execute(vec![Command::Sheet(SheetCommand::SetFormat {sheet: slide,ranges:vec![CellRange {first_row:0,last_row:0,first_column:0,last_column:0}],change:StyleChange::Bold(!was_bold)})]).unwrap();
        let bytes=opened.source.save(d.presentation(),SaveOptions::default()).unwrap();
        assert_eq!(ibis_ooxml::Package::open(bytes.clone()).unwrap().read("xl/workbook.xml").unwrap(),original_xml,"{kind} extension untouched");
        assert_eq!(ibis_formats::read(&bytes).unwrap().workbook.as_deref().unwrap().compatibility_version,expected);
        assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
        assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
    }
}

#[test]
fn compatibility_selection_preserves_foreign_payload_and_validates_minimum() {
    let stock=std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    for kind in ["absent","foreign_list","foreign_ext","wrong_uri","future7","warn3_set2","warn0_set2","duplicate"] {
        let mut package=ibis_ooxml::Package::open(stock.clone()).unwrap();let doc=package.xml_mut("xl/workbook.xml").unwrap();let root=doc.root_element().unwrap();
        let old:Vec<_>=doc.elements(root).filter(|&n|doc.local_name(n)=="extLst" && doc.namespace(n)==Some(MAIN)).collect();for n in old{doc.detach(n);}
        if kind!="absent" {
            let list=doc.append_element(root,if kind=="foreign_list" {"foreign:extLst"} else {"extLst"});doc.set_attr(list,"xmlns:foreign","urn:ibis:foreign-selection-probe");
            let ext=doc.append_element(list,if kind=="foreign_ext" {"foreign:ext"} else {"ext"});doc.set_attr(ext,"uri",if kind=="wrong_uri" {"urn:ibis:foreign-selection-probe"} else {URI});doc.set_attr(ext,"xmlns:wcv",VERSION);
            let node=doc.append_element(ext,"wcv:version");doc.set_attr(node,"setVersion",if kind=="future7" {"7"} else {"2"});
            doc.set_attr(node,"warnBelowVersion",match kind {"future7"=>"6","warn3_set2"=>"3","warn0_set2"=>"0",_=>"1"});
            doc.set_attr(node,"xmlns:probe","urn:ibis:selection-payload");doc.set_attr(node,"probe:token","keep-original");
            if kind=="duplicate" {let other=doc.append_element(ext,"wcv:version");doc.set_attr(other,"setVersion","1");}
        }
        let original=package.save(Default::default()).unwrap();let opened=ibis_formats::open(original.clone()).unwrap();let mut d=Document::new(opened.presentation.clone());
        let version=if matches!(kind,"future7"|"warn3_set2") {ibis_doc_model::CalculationCompatibilityVersion::V1} else {ibis_doc_model::CalculationCompatibilityVersion::V2};
        d.execute(vec![Command::Sheet(SheetCommand::SetCalculationCompatibilityVersion {version})]).unwrap();
        let result=opened.source.save(d.presentation(),SaveOptions::default());
        if matches!(kind,"warn0_set2"|"duplicate") {
            assert!(result.is_err(),"{kind} must not silently rewrite malformed metadata");assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);continue;
        }
        let bytes=result.unwrap();assert_eq!(ibis_formats::read(&bytes).unwrap().workbook.as_deref().unwrap().compatibility_version,Some(version.number()),"{kind}");
        let saved=ibis_ooxml::Package::open(bytes.clone()).unwrap();let xml=saved.xml("xl/workbook.xml").unwrap();let root=xml.root_element().unwrap();
        let sentinel=xml.descendants(root).into_iter().find(|&n|xml.attr(n,"probe:token")==Some("keep-original"));
        if kind!="absent" {
            let node=sentinel.unwrap();assert_eq!(xml.attr(node,"probe:token"),Some("keep-original"));
            let foreign=matches!(kind,"foreign_list"|"foreign_ext"|"wrong_uri");
            assert_eq!(xml.attr(node,"setVersion"),Some(if foreign {"2"} else {"1"}));assert_eq!(xml.attr(node,"warnBelowVersion"),Some("1"));
        }
        let calc=xml.child(root,"calcPr").unwrap();assert_eq!(xml.attr(calc,"fullCalcOnLoad"),Some("1"));
        assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);
        assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
    }
}


#[test]
fn compatibility_attributes_ignore_foreign_shadows_in_both_orders() {
    let stock = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    for field in ["uri", "setVersion", "warnBelowVersion"] {
        for order in ["foreign-only", "foreign-first", "proper-first"] {
            let label = format!("{field}-{order}");
            let mut package = ibis_ooxml::Package::open(stock.clone()).unwrap();
            let doc = package.xml_mut("xl/workbook.xml").unwrap(); let root = doc.root_element().unwrap();
            let old: Vec<_> = doc.elements(root).filter(|&n| doc.local_name(n) == "extLst" && doc.namespace(n) == Some(MAIN)).collect(); for n in old { doc.detach(n); }
            doc.set_attr(root, "xmlns:probe", "urn:ibis:compatibility-attribute");
            let list = doc.append_element(root, "extLst"); let ext = doc.append_element(list, "ext"); doc.set_attr(ext, "uri", URI); doc.set_attr(ext, "xmlns:wcv", VERSION);
            let node = doc.append_element(ext, "wcv:version"); doc.set_attr(node, "setVersion", "2"); doc.set_attr(node, "warnBelowVersion", "1");
            let target = if field == "uri" { ext } else { node }; let proper = match field { "uri" => URI, "setVersion" => "2", _ => "1" }; let foreign = match field { "uri" => "urn:ibis:wrong", "setVersion" => "1", _ => "3" };
            doc.remove_attr(target, field);
            if order == "proper-first" { doc.set_attr(target, field, proper); }
            doc.set_attr(target, &format!("probe:{field}"), if order == "foreign-only" { proper } else { foreign });
            if order == "foreign-first" { doc.set_attr(target, field, proper); }
            let original = package.save(Default::default()).unwrap(); let original_xml = package.read("xl/workbook.xml").unwrap(); let opened = ibis_formats::open(original.clone()).unwrap();
            let expected = if order == "foreign-only" && field != "warnBelowVersion" { None } else { Some(2) };
            assert_eq!(opened.presentation.workbook.as_deref().unwrap().compatibility_version, expected, "{label}");
            let view = BookView::new(&opened.presentation);
            assert_eq!(evaluate_cell(&view, &Stored, 0, 0, 0, &ibis_calc::formula::parse("LEN(\"😀x\")").unwrap()), CellValue::Number(if expected == Some(2) { 2. } else { 3. }), "{label} calculation");
            assert_eq!(opened.source.save(&opened.presentation, SaveOptions::default()).unwrap(), original);
            let mut d = Document::new(opened.presentation.clone());
            d.execute(vec![Command::Sheet(SheetCommand::SetCalculationCompatibilityVersion { version: ibis_doc_model::CalculationCompatibilityVersion::V1 })]).unwrap();
            let bytes = opened.source.save(d.presentation(), SaveOptions::default()).unwrap();
            assert_eq!(ibis_formats::read(&bytes).unwrap().workbook.as_deref().unwrap().compatibility_version, Some(1), "{label} selected version survives save");
            let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join(format!("compatibility-attribute-{label}-source.xlsx")), &original).unwrap();
            std::fs::write(dir.join(format!("compatibility-attribute-{label}-saved.xlsx")), &bytes).unwrap();
            let saved_xml = ibis_ooxml::Package::open(bytes.clone()).unwrap().read("xl/workbook.xml").unwrap();
            assert!(String::from_utf8(saved_xml).unwrap().contains(&format!("probe:{field}=\"{}\"", if order == "foreign-only" { proper } else { foreign })), "{label} opaque attribute kept");
            assert!(d.undo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), original); assert!(d.redo()); assert_eq!(opened.source.save(d.presentation(), SaveOptions::default()).unwrap(), bytes);
            assert!(String::from_utf8(original_xml).unwrap().contains(&format!("probe:{field}")));
        }
    }
}

#[test]
fn compatibility_integer_attributes_use_xml_whitespace_without_rewriting_opaque_values() {
    let stock=std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/xlsx-sales.xlsx")).unwrap();
    for field in ["setVersion","warnBelowVersion"] {
        for (label,text,valid) in [("plain","2",true),("leading","002",true),("space"," 2 ",true),("tab","\t2\n",true),("nbsp","\u{a0}2\u{a0}",false),("internal","2 0",false),("overflow","4294967296",false),("zero","0",false),("negative","-1",false)] {
            let mut package=ibis_ooxml::Package::open(stock.clone()).unwrap();let doc=package.xml_mut("xl/workbook.xml").unwrap();let root=doc.root_element().unwrap();let old:Vec<_>=doc.children_named(root,"extLst").collect();for n in old {doc.detach(n);}
            let list=doc.append_element(root,"extLst");let ext=doc.append_element(list,"ext");doc.set_attr(ext,"uri",URI);doc.set_attr(ext,"xmlns:wcv",VERSION);let node=doc.append_element(ext,"wcv:version");doc.set_attr(node,"setVersion","2");doc.set_attr(node,"warnBelowVersion","1");doc.set_attr(node,field,text);doc.set_attr(node,"xmlns:probe","urn:ibis:compatibility-whitespace");doc.set_attr(node,"probe:token","keep");
            let original=package.save(Default::default()).unwrap();let original_xml=package.read("xl/workbook.xml").unwrap();let opened=ibis_formats::open(original.clone()).unwrap();let expected=valid.then_some(2);
            assert_eq!(opened.presentation.workbook.as_deref().unwrap().compatibility_version,expected,"{field}-{label}");
            let view=BookView::new(&opened.presentation);assert_eq!(evaluate_cell(&view,&Stored,0,0,0,&ibis_calc::formula::parse("LEN(\"😀x\")").unwrap()),CellValue::Number(if valid {2.} else {3.}));
            assert_eq!(opened.source.save(&opened.presentation,SaveOptions::default()).unwrap(),original);
            let mut d=Document::new(opened.presentation.clone());let slide=d.presentation().slides[0].id;
            let sh=ibis_doc_model::sheet_calc::sheet_of_slide(&d.presentation().slides[0]).unwrap();let bold=sh.styles[sh.cell(0,0).unwrap().style as usize].bold;
            d.execute(vec![Command::Sheet(SheetCommand::SetFormat {sheet: slide,ranges:vec![CellRange {first_row:0,last_row:0,first_column:0,last_column:0}],change:StyleChange::Bold(!bold)})]).unwrap();
            assert_eq!(ibis_ooxml::Package::open(opened.source.save(d.presentation(),SaveOptions::default()).unwrap()).unwrap().read("xl/workbook.xml").unwrap(),original_xml);assert!(d.undo());
            d.execute(vec![Command::Sheet(SheetCommand::SetCalculationCompatibilityVersion {version:ibis_doc_model::CalculationCompatibilityVersion::V1})]).unwrap();let result=opened.source.save(d.presentation(),SaveOptions::default());
            if field=="warnBelowVersion"&&!valid {assert!(result.is_err(),"{field}-{label}");assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert!(opened.source.save(d.presentation(),SaveOptions::default()).is_err());continue;}
            let bytes=result.unwrap();let reread=ibis_formats::read(&bytes).unwrap();assert_eq!(reread.workbook.as_deref().unwrap().compatibility_version,Some(1));let view=BookView::new(&reread);assert_eq!(evaluate_cell(&view,&Stored,0,0,0,&ibis_calc::formula::parse("LEN(\"😀x\")").unwrap()),CellValue::Number(3.));
            let pkg=ibis_ooxml::Package::open(bytes.clone()).unwrap();let xml=pkg.xml("xl/workbook.xml").unwrap();let root=xml.root_element().unwrap();let node=xml.descendants(root).into_iter().find(|&n|xml.attr(n,"probe:token")==Some("keep")).unwrap();assert_eq!(xml.attr(node,"setVersion"),Some("1"));assert_eq!(xml.attr(node,"warnBelowVersion"),Some("1"));
            assert!(d.undo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),original);assert!(d.redo());assert_eq!(opened.source.save(d.presentation(),SaveOptions::default()).unwrap(),bytes);
            let dir=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-edit");std::fs::create_dir_all(&dir).unwrap();std::fs::write(dir.join(format!("compatibility-integer-{field}-{label}-source.xlsx")),&original).unwrap();std::fs::write(dir.join(format!("compatibility-integer-{field}-{label}-saved.xlsx")),&bytes).unwrap();
        }
    }
}
