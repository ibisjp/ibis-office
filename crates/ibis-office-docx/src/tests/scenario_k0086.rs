// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0086: the style's typefaces in スタイルの変更 (tasks/doc/test-design/K0086.md). The panel has a 日本語用 and a 英数字用 typeface list (the theme fonts first); only the
//! field that was changed is written into the style's `w:rFonts`, a theme font is kept as the reference, and one 適用 is one step of undo.

use crate::ext::*;
use crate::shell::scenario::Scenario;
use ibis_doc_model::{Block, FlowDocument, FlowPos};
use ibis_formats::ibis_ooxml::{Package, SaveOptions};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("docx::k0086_style_panel_has_japanese_and_latin_font_dropdowns", two_dropdowns as fn()),
        ("docx::k0086_panel_initial_fonts_per_slot", initial_fonts),
        ("docx::k0086_theme_font_shown_as_theme_name", theme_shown),
        ("docx::k0086_font_list_starts_with_theme_fonts", list_starts_with_theme),
        ("docx::k0086_choose_theme_font_saves_theme_attrs", choose_theme_saves_attrs),
        ("docx::k0086_font_search_filters_and_keeps_missing_value", missing_value_kept),
        ("docx::k0086_latin_only_changes_ascii_hansi", latin_only),
        ("docx::k0086_japanese_only_changes_east_asia", japanese_only),
        ("docx::k0086_slot_is_not_guessed_from_the_name", slot_not_guessed),
        ("docx::k0086_untouched_font_attrs_survive", untouched_survive),
        ("docx::k0086_doc_defaults_unchanged", doc_defaults_unchanged),
        ("docx::k0086_style_font_reaches_paragraphs_direct_format_wins", reaches_paragraphs),
        ("docx::k0086_each_char_uses_its_slot_font", each_char_slot),
        ("docx::k0086_style_fonts_round_trip", round_trip),
        ("docx::k0086_panel_shows_saved_fonts_after_reopen", panel_after_reopen),
        ("docx::k0086_style_font_undo_redo", undo_redo),
        ("docx::k0086_protected_doc_blocks_style_font", protected_blocks),
        ("docx::k0086_style_font_i18n_keys_exist", i18n_keys),
        ("docx::docx_style_font_panel", panel_picture),
        ("docx::k0086_word_made_styles_show_word_values", word_made),
        ("docx::k0086_font_search_narrows_the_list", search_narrows),
        ("docx::docx_style_font_list_open", list_open_pictures),
    ]
}

const REPORT: &str = "test/editing/docx-report.docx";

/// The styles the scenarios change (added to the report's styles.xml): `Fh1` (ascii Arial, hAnsi Verdana, eastAsia MS Gothic, cs Tahoma, hint eastAsia), `Fchild` on it with no
/// fonts, `Fh2` (Times New Roman, and the same for hAnsi, eastAsia ＭＳ 明朝), `Ftheme` (the theme's body fonts), `Fnone` (no fonts).
const STYLES: &str = r#"<w:style w:type="paragraph" w:styleId="Fh1"><w:name w:val="K86 h1"/><w:basedOn w:val="a"/><w:qFormat/><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Verdana" w:eastAsia="MS Gothic" w:cs="Tahoma" w:hint="eastAsia"/><w:sz w:val="28"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Fchild"><w:name w:val="K86 child"/><w:basedOn w:val="Fh1"/></w:style><w:style w:type="paragraph" w:styleId="Fh2"><w:name w:val="K86 h2"/><w:basedOn w:val="a"/><w:rPr><w:rFonts w:ascii="Times New Roman" w:hAnsi="Times New Roman" w:eastAsia="ＭＳ 明朝"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Ftheme"><w:name w:val="K86 theme"/><w:basedOn w:val="a"/><w:rPr><w:rFonts w:asciiTheme="minorHAnsi" w:hAnsiTheme="minorHAnsi" w:eastAsiaTheme="minorEastAsia"/></w:rPr></w:style><w:style w:type="paragraph" w:styleId="Fnone"><w:name w:val="K86 none"/><w:basedOn w:val="a"/></w:style>"#;

/// The fixture's paragraphs, in order: h1 (Fh1), child (Fchild), h2 (Fh2), mixed (Fh1 with one run's own fonts), theme (Ftheme), none (Fnone).
const BODY: &str = r#"<w:p><w:pPr><w:pStyle w:val="Fh1"/></w:pPr><w:r><w:t>Head ハロー Ａ１ ★</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Fchild"/></w:pPr><w:r><w:t>Child こんにちは</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Fh2"/></w:pPr><w:r><w:t>Second あいう</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Fh1"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Courier New" w:hAnsi="Courier New" w:eastAsia="MS Mincho"/></w:rPr><w:t>Own</w:t></w:r><w:r><w:t>Plain</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Ftheme"/></w:pPr><w:r><w:t>Themed テーマ</w:t></w:r></w:p><w:p><w:pPr><w:pStyle w:val="Fnone"/></w:pPr><w:r><w:t>None なし</w:t></w:r></w:p>"#;

fn fixture(name: &str, doc_defaults_fonts: Option<&str>) -> String {
    let data = std::fs::read(crate::shell::scenario::repo_root().join(REPORT)).unwrap();
    let mut package = Package::open(data).unwrap();
    let mut styles = String::from_utf8(package.read("word/styles.xml").unwrap()).unwrap();
    styles = styles.replace("</w:styles>", &format!("{STYLES}</w:styles>"));
    if let Some(fonts) = doc_defaults_fonts {
        let at = styles.find("<w:rFonts").expect("docDefaults fonts");
        let end = at + styles[at..].find("/>").unwrap() + 2;
        styles.replace_range(at..end, fonts);
    }
    package.set_bytes("word/styles.xml", styles.into_bytes());
    let document = String::from_utf8(package.read("word/document.xml").unwrap()).unwrap();
    let from = document.find("<w:body>").unwrap() + "<w:body>".len();
    let to = document.rfind("<w:sectPr").unwrap();
    let document = format!("{}{BODY}{}", &document[..from], &document[to..]);
    package.set_bytes("word/document.xml", document.into_bytes());
    let dir = crate::shell::scenario::repo_root().join("target/scenario-input");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("k0086-{name}.docx")), package.save(SaveOptions::default()).unwrap()).unwrap();
    format!("target/scenario-input/k0086-{name}.docx")
}

fn paragraphs(doc: &FlowDocument) -> Vec<&ibis_doc_model::FlowParagraph> {
    doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).collect()
}

fn doc(s: &mut Scenario) -> FlowDocument {
    s.with(|e, _| (**e.flow().expect("a document")).clone())
}

/// Paragraph `n` of the fixture (0: h1 ... 5: none) opened in スタイルの変更….
fn open_panel(s: &mut Scenario, n: usize) {
    let id = paragraphs(&doc(s))[n].id;
    s.update(|e, window, cx| {
        e.set_caret(FlowPos::new(id, 1), false, cx);
        e.open_style_edit(window, cx);
    });
    assert!(s.with(|e, _| e.doc_state().is_some_and(|st| st.style_edit.is_some())), "the style panel is open");
}

/// (what the 英数字用 field shows, what the 日本語用 field shows).
fn shown(s: &mut Scenario) -> (String, String) {
    s.with(|e, cx| {
        let edit = e.doc_state().unwrap().style_edit.as_ref().unwrap();
        (edit.font_latin.read(cx).value().to_string(), edit.font_east_asian.read(cx).value().to_string())
    })
}

/// Choose in the fields as the lists do: a typeface name (`None`: leave the field) or a theme font (the reference).
fn choose(s: &mut Scenario, latin: Option<(&str, Option<&str>)>, east: Option<(&str, Option<&str>)>) {
    s.update(|e, window, cx| {
        let edit = e.doc_state_mut().unwrap().style_edit.as_mut().unwrap();
        if let Some((text, theme)) = latin {
            edit.font_latin.update(cx, |i, cx| i.set_value(text, window, cx));
            edit.latin_theme = theme.map(str::to_string);
        }
        if let Some((text, theme)) = east {
            edit.font_east_asian.update(cx, |i, cx| i.set_value(text, window, cx));
            edit.east_asian_theme = theme.map(str::to_string);
        }
    });
}

fn apply(s: &mut Scenario) {
    s.update(|e, _, cx| e.apply_style_edit(cx));
}

/// The `w:style` element of `id` and the `w:rFonts` in it, from the file saved as `file`.
fn saved_style(s: &mut Scenario, file: &str, id: &str) -> (String, String) {
    let path = s.save_as(file);
    let package = Package::open(std::fs::read(path).unwrap()).unwrap();
    let styles = String::from_utf8(package.read("word/styles.xml").unwrap()).unwrap();
    let at = styles.find(&format!("w:styleId=\"{id}\"")).expect("the style");
    let end = at + styles[at..].find("</w:style>").unwrap();
    let style = styles[at..end].to_string();
    let fonts = style.find("<w:rFonts").map(|f| style[f..f + style[f..].find("/>").unwrap() + 2].to_string()).unwrap_or_default();
    (style, fonts)
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let at = tag.find(&key)? + key.len();
    Some(tag[at..at + tag[at..].find('"')?].to_string())
}

fn two_dropdowns() {
    let mut s = Scenario::open(&fixture("t1", None));
    open_panel(&mut s, 0);
    s.settle();
    s.screenshot("panel");
    // The labels are i18n texts, in both languages (T19 checks the files).
    for key in ["docx.style-panel.east-asian-font", "docx.style-panel.latin-font"] {
        assert_ne!(crate::shell::i18n::text(key).to_string(), key, "{key} has a text");
    }
    let (latin, east) = shown(&mut s);
    assert_eq!((latin.as_str(), east.as_str()), ("Arial", "MS Gothic"), "two fields with a typeface of their own each");
}

fn initial_fonts() {
    let mut s = Scenario::open(&fixture("t2", None));
    for (n, want) in [(0, ("Arial", "MS Gothic")), (1, ("Arial", "MS Gothic")), (2, ("Times New Roman", "ＭＳ 明朝"))] {
        open_panel(&mut s, n);
        let (a, b) = shown(&mut s);
        assert_eq!((a.as_str(), b.as_str()), want, "paragraph {n}: the typefaces drawn for the style (inherited through basedOn too)");
    }
}

fn theme_shown() {
    let mut s = Scenario::open(&fixture("t3", None));
    open_panel(&mut s, 4);
    let (latin, east) = shown(&mut s);
    assert_eq!(latin, crate::shell::i18n::text("docx.style-panel.theme-body-latin").to_string());
    assert_eq!(east, crate::shell::i18n::text("docx.style-panel.theme-body-east-asian").to_string());
    assert!(!latin.contains("Yu"), "the theme's name for the typeface is not what is shown: {latin}");
    s.screenshot("theme");
}

fn list_starts_with_theme() {
    let mut s = Scenario::open(&fixture("t4", None));
    open_panel(&mut s, 0);
    for (east, id, want) in [
        (false, "docx-style-font-list-ascii", ["docx.style-panel.theme-body-latin", "docx.style-panel.theme-heading-latin"]),
        (true, "docx-style-font-list-east-asian", ["docx.style-panel.theme-body-east-asian", "docx.style-panel.theme-heading-east-asian"]),
    ] {
        let labels: Vec<String> = s.update(|e, _, cx| {
            let edit = e.doc_state().unwrap().style_edit.as_ref().unwrap();
            let input = if east { edit.font_east_asian.clone() } else { edit.font_latin.clone() };
            let (_, entries) = super::style_panel::style_font_entries(id, &input, None, east, cx);
            entries.iter().filter_map(|m| if let crate::shell::chrome::MenuEntry::Item(i) = m { Some(i.label.to_string()) } else { None }).take(2).collect()
        });
        assert_eq!(labels, want.map(|k| crate::shell::i18n::text(k).to_string()), "the theme fonts come first in the {id} list");
    }
}

fn choose_theme_saves_attrs() {
    let mut s = Scenario::open(&fixture("t5", None));
    open_panel(&mut s, 0);
    choose(&mut s, Some(("+h", Some("majorHAnsi"))), Some(("+h", Some("majorEastAsia"))));
    apply(&mut s);
    let (_, fonts) = saved_style(&mut s, "t5.docx", "Fh1");
    assert_eq!(attr(&fonts, "w:asciiTheme").as_deref(), Some("majorHAnsi"), "{fonts}");
    assert_eq!(attr(&fonts, "w:hAnsiTheme").as_deref(), Some("majorHAnsi"), "{fonts}");
    assert_eq!(attr(&fonts, "w:eastAsiaTheme").as_deref(), Some("majorEastAsia"), "{fonts}");
    for plain in ["w:ascii", "w:hAnsi", "w:eastAsia"] {
        assert_eq!(attr(&fonts, plain), None, "{plain} is not written with the theme reference: {fonts}");
    }
    assert_eq!(attr(&fonts, "w:cs").as_deref(), Some("Tahoma"));
    // The drawn typeface is the theme's heading font.
    let d = doc(&mut s);
    assert_eq!(paragraphs(&d)[0].runs[0].style.font.as_deref(), Some("Yu Gothic Light"));
}

fn missing_value_kept() {
    let mut s = Scenario::open(&fixture("t6", None));
    open_panel(&mut s, 0);
    choose(&mut s, Some(("No Such Typeface", None)), None);
    apply(&mut s);
    open_panel(&mut s, 0);
    assert_eq!(shown(&mut s).0, "No Such Typeface", "a typeface that is not installed is still the value");
    let (_, fonts) = saved_style(&mut s, "t6.docx", "Fh1");
    assert_eq!(attr(&fonts, "w:ascii").as_deref(), Some("No Such Typeface"));
}

fn latin_only() {
    let mut s = Scenario::open(&fixture("t7a", None));
    open_panel(&mut s, 0);
    choose(&mut s, Some(("Georgia", None)), None);
    apply(&mut s);
    let (_, fonts) = saved_style(&mut s, "t7a.docx", "Fh1");
    assert_eq!((attr(&fonts, "w:ascii").as_deref(), attr(&fonts, "w:hAnsi").as_deref(), attr(&fonts, "w:eastAsia").as_deref()), (Some("Georgia"), Some("Georgia"), Some("MS Gothic")), "{fonts}");
}

fn japanese_only() {
    let mut s = Scenario::open(&fixture("t7b", None));
    open_panel(&mut s, 0);
    choose(&mut s, None, Some(("游明朝", None)));
    apply(&mut s);
    let (_, fonts) = saved_style(&mut s, "t7b.docx", "Fh1");
    assert_eq!((attr(&fonts, "w:ascii").as_deref(), attr(&fonts, "w:hAnsi").as_deref(), attr(&fonts, "w:eastAsia").as_deref()), (Some("Arial"), Some("Verdana"), Some("游明朝")), "{fonts}");
}

fn slot_not_guessed() {
    let mut s = Scenario::open(&fixture("t8", None));
    open_panel(&mut s, 0);
    choose(&mut s, Some(("ＭＳ 明朝", None)), Some(("Arial", None)));
    apply(&mut s);
    let (_, fonts) = saved_style(&mut s, "t8.docx", "Fh1");
    assert_eq!((attr(&fonts, "w:ascii").as_deref(), attr(&fonts, "w:hAnsi").as_deref(), attr(&fonts, "w:eastAsia").as_deref()), (Some("ＭＳ 明朝"), Some("ＭＳ 明朝"), Some("Arial")), "{fonts}");
}

fn untouched_survive() {
    // The size only: every attribute of rFonts is as it was.
    {
    let mut s = Scenario::open(&fixture("t9a", None));
    open_panel(&mut s, 0);
    s.update(|e, window, cx| {
        let size = e.doc_state().unwrap().style_edit.as_ref().unwrap().size.clone();
        size.update(cx, |i, cx| i.set_value("20", window, cx));
    });
    apply(&mut s);
    let (_, fonts) = saved_style(&mut s, "t9a.docx", "Fh1");
    for (name, want) in [("w:ascii", "Arial"), ("w:hAnsi", "Verdana"), ("w:eastAsia", "MS Gothic"), ("w:cs", "Tahoma"), ("w:hint", "eastAsia")] {
        assert_eq!(attr(&fonts, name).as_deref(), Some(want), "{name} after a size change: {fonts}");
    }
    }
    {
    // The 英数字用 only: the other range's typeface and the hint stay, the complex script typeface follows (Word's Font.Name).
    let mut s = Scenario::open(&fixture("t9b", None));
    open_panel(&mut s, 0);
    choose(&mut s, Some(("Georgia", None)), None);
    apply(&mut s);
    let (_, fonts) = saved_style(&mut s, "t9b.docx", "Fh1");
    for (name, want) in [("w:eastAsia", "MS Gothic"), ("w:cs", "Georgia"), ("w:hint", "eastAsia")] {
        assert_eq!(attr(&fonts, name).as_deref(), Some(want), "{name} after a 英数字用 change: {fonts}");
    }
    }
    {
    // The 日本語用 only, on a style that refers to the theme for the other: that reference stays.
    let mut s = Scenario::open(&fixture("t9c", None));
    open_panel(&mut s, 4);
    choose(&mut s, None, Some(("MS Gothic", None)));
    apply(&mut s);
    let (_, fonts) = saved_style(&mut s, "t9c.docx", "Ftheme");
    assert_eq!(attr(&fonts, "w:asciiTheme").as_deref(), Some("minorHAnsi"), "{fonts}");
    assert_eq!(attr(&fonts, "w:eastAsia").as_deref(), Some("MS Gothic"), "{fonts}");
    assert_eq!(attr(&fonts, "w:eastAsiaTheme"), None, "{fonts}");
    }
}

fn doc_defaults_unchanged() {
    let mut s = Scenario::open(&fixture("t10", Some(r#"<w:rFonts w:ascii="Century" w:hAnsi="Century" w:eastAsia="ＭＳ 明朝"/>"#)));
    open_panel(&mut s, 5);
    choose(&mut s, Some(("Georgia", None)), Some(("メイリオ", None)));
    apply(&mut s);
    let path = s.save_as("t10.docx");
    let package = Package::open(std::fs::read(path).unwrap()).unwrap();
    let styles = String::from_utf8(package.read("word/styles.xml").unwrap()).unwrap();
    let defaults = &styles[styles.find("<w:docDefaults>").unwrap()..styles.find("</w:docDefaults>").unwrap()];
    assert!(defaults.contains(r#"<w:rFonts w:ascii="Century" w:hAnsi="Century" w:eastAsia="ＭＳ 明朝"/>"#), "{defaults}");
    let (_, fonts) = saved_style(&mut s, "t10b.docx", "Fnone");
    assert_eq!(attr(&fonts, "w:ascii").as_deref(), Some("Georgia"), "the style has them: {fonts}");
    // A paragraph of another style is as it was.
    let d = doc(&mut s);
    assert_eq!(paragraphs(&d)[2].runs[0].style.font.as_deref(), Some("Times New Roman"));
}

fn reaches_paragraphs() {
    let mut s = Scenario::open(&fixture("t11", None));
    let before = doc(&mut s);
    open_panel(&mut s, 0);
    choose(&mut s, Some(("Georgia", None)), Some(("游明朝", None)));
    apply(&mut s);
    let d = doc(&mut s);
    let (b, a) = (paragraphs(&before), paragraphs(&d));
    let font = |p: &ibis_doc_model::FlowParagraph, run: usize, c: char| p.runs[run].style.font_for(c).map(str::to_string);
    for n in [0, 1] {
        assert_eq!(font(a[n], 0, 'H').as_deref(), Some("Georgia"), "paragraph {n} (the style's own or inherited) follows");
        assert_eq!(font(a[n], 0, 'ロ').as_deref(), Some("游明朝"), "paragraph {n}");
    }
    assert_eq!(font(a[2], 0, 'S'), font(b[2], 0, 'S'), "another style is as it was");
    assert_eq!(font(a[2], 0, 'あ'), font(b[2], 0, 'あ'));
    // The run with its own fonts keeps them, the plain run of the same paragraph follows.
    assert_eq!(font(a[3], 0, 'O').as_deref(), Some("Courier New"), "the direct format wins");
    assert_eq!(font(a[3], 1, 'P').as_deref(), Some("Georgia"));
    s.screenshot("reaches");
}

fn each_char_slot() {
    let mut s = Scenario::open(&fixture("t12", None));
    open_panel(&mut s, 0);
    choose(&mut s, Some(("Georgia", None)), Some(("游明朝", None)));
    apply(&mut s);
    let d = doc(&mut s);
    let p = paragraphs(&d)[0];
    let style = &p.runs[0].style;
    for c in "Head ハロー Ａ１ ★".chars() {
        let slot = ibis_doc_model::flow::font_slot(c, style.east_asia_hint);
        let want = match slot {
            ibis_doc_model::flow::FontSlot::EastAsia => style.east_asian_font.as_deref(),
            ibis_doc_model::flow::FontSlot::HAnsi => style.h_ansi_font.as_deref().or(style.font.as_deref()),
            _ => style.font.as_deref(),
        };
        assert_eq!(style.font_for(c), want, "{c:?} ({slot:?})");
    }
    assert_eq!(style.font_for('H'), Some("Georgia"));
    assert_eq!(style.font_for('ハ'), Some("游明朝"));
}

fn round_trip() {
    let mut s = Scenario::open(&fixture("t13", None));
    open_panel(&mut s, 0);
    choose(&mut s, Some(("Georgia", None)), Some(("+b", Some("minorEastAsia"))));
    apply(&mut s);
    open_panel(&mut s, 2);
    choose(&mut s, Some(("+m", Some("minorHAnsi"))), Some(("メイリオ", None)));
    apply(&mut s);
    let want = doc(&mut s);
    s.save_as("t13.docx");
    let back = s.reopen();
    let back = back.flow.as_ref().expect("a document");
    for id in ["Fh1", "Fh2", "Ftheme"] {
        assert_eq!(back.styles.styles[id].run, want.styles.styles[id].run, "{id} reads back as it was saved");
    }
    assert_eq!(back.styles.styles["Fh1"].run.east_asian_font_theme.as_deref(), Some("minorEastAsia"));
    assert_eq!(back.styles.styles["Fh2"].run.font_theme.as_deref(), Some("minorHAnsi"));
}

fn panel_after_reopen() {
    let mut s = Scenario::open(&fixture("t14", None));
    open_panel(&mut s, 0);
    choose(&mut s, Some(("Georgia", None)), Some(("+b", Some("majorEastAsia"))));
    apply(&mut s);
    s.save_as("t14.docx");
    s.reload_saved();
    open_panel(&mut s, 0);
    let (latin, east) = shown(&mut s);
    assert_eq!(latin, "Georgia");
    assert_eq!(east, crate::shell::i18n::text("docx.style-panel.theme-heading-east-asian").to_string());
}

fn undo_redo() {
    let mut s = Scenario::open(&fixture("t15", None));
    let original = doc(&mut s);
    open_panel(&mut s, 0);
    choose(&mut s, Some(("Georgia", None)), Some(("游明朝", None)));
    apply(&mut s);
    let changed = doc(&mut s);
    assert!(changed != original);
    // The size only (the fonts not changed): a step of its own.
    s.update(|e, window, cx| {
        let size = e.doc_state().unwrap().style_edit.as_ref().unwrap().size.clone();
        size.update(cx, |i, cx| i.set_value("22", window, cx));
    });
    apply(&mut s);
    assert!(s.update(|e, _, cx| { let u = e.document.undo(); cx.notify(); u }), "undo the size");
    assert!(doc(&mut s) == changed, "one apply is one undo: the fonts stay");
    assert!(s.update(|e, _, cx| { let u = e.document.undo(); cx.notify(); u }));
    assert!(doc(&mut s) == original, "undone: as it was");
    let (_, fonts) = saved_style(&mut s, "t15.docx", "Fh1");
    assert_eq!((attr(&fonts, "w:ascii").as_deref(), attr(&fonts, "w:eastAsia").as_deref(), attr(&fonts, "w:hAnsi").as_deref()), (Some("Arial"), Some("MS Gothic"), Some("Verdana")), "{fonts}");
    assert!(s.update(|e, _, cx| { let r = e.document.redo(); cx.notify(); r }));
    assert!(doc(&mut s) == changed, "redone");
}

fn protected_blocks() {
    let mut s = Scenario::open(&fixture("t18", None));
    s.menu("docx.protect.read-only");
    let before = doc(&mut s);
    open_panel(&mut s, 0);
    choose(&mut s, Some(("Georgia", None)), None);
    apply(&mut s);
    assert!(doc(&mut s) == before, "a protected document's style is not changed");
}

fn i18n_keys() {
    for key in [
        "docx.style-panel.east-asian-font",
        "docx.style-panel.latin-font",
        "docx.style-panel.theme-body-latin",
        "docx.style-panel.theme-heading-latin",
        "docx.style-panel.theme-body-east-asian",
        "docx.style-panel.theme-heading-east-asian",
    ] {
        for lang in ["ja", "en"] {
            let file = crate::shell::scenario::repo_root().join(format!("crates/ibis-office-base/i18n/{lang}/docx.tsv"));
            let text = std::fs::read_to_string(file).unwrap();
            assert!(text.lines().any(|l| l.split('\t').next() == Some(key) && l.split('\t').nth(1).is_some_and(|v| !v.is_empty())), "{lang}: {key}");
        }
    }
}

fn panel_picture() {
    let mut s = Scenario::open(&fixture("pic", None));
    open_panel(&mut s, 4);
    s.settle();
    s.screenshot("theme-fonts");
    open_panel(&mut s, 0);
    s.settle();
    s.screenshot("panel");
}

/// T17: styles made in Word (`Style.Font.Name` / `NameAscii` / `NameFarEast` and the theme names, set through COM and saved by Word's own WordOpenXML) shown in the panel as Word reads
/// them back (the values Word's COM gave are in tasks/doc/reports/2026-11-26-K0086-word-check.md). Fixture: test/fixtures/docx-form/k0086-word-made-styles.docx (Wa..Wg = paragraphs 0..6).
fn word_made() {
    let mut s = Scenario::open("test/fixtures/docx-form/k0086-word-made-styles.docx");
    let theme = |k: &str| crate::shell::i18n::text(k).to_string();
    let want: [(&str, String, String); 7] = [
        ("Wa: Font.Name=Arial", "Arial".into(), "ＭＳ ゴシック".into()),
        ("Wb: NameAscii=Georgia", "Georgia".into(), "ＭＳ ゴシック".into()),
        ("Wc: NameFarEast=メイリオ", "Arial".into(), "メイリオ".into()),
        ("Wd: the body theme fonts", theme("docx.style-panel.theme-body-latin"), theme("docx.style-panel.theme-body-east-asian")),
        ("We: the heading theme fonts", theme("docx.style-panel.theme-heading-latin"), theme("docx.style-panel.theme-heading-east-asian")),
        ("Wf: Verdana / ＭＳ 明朝", "Verdana".into(), "ＭＳ 明朝".into()),
        ("Wg: ascii Arial, east asian heading theme", "Arial".into(), theme("docx.style-panel.theme-heading-east-asian")),
    ];
    let mut report = String::new();
    for (n, (label, latin, east)) in want.iter().enumerate() {
        open_panel(&mut s, n);
        let (a, b) = shown(&mut s);
        report.push_str(&format!("{label}\tshown: {a} | {b}\n"));
        assert_eq!((a.as_str(), b.as_str()), (latin.as_str(), east.as_str()), "{label}");
    }
    eprintln!("{report}");
}

/// T6: the list is opened with the button, the search field narrows it by what is typed (the typeface names; none for a name nothing has), and a typeface that is not installed is the value.
fn search_narrows() {
    let _probe = crate::shell::scenario::button_probe::Probe::new();
    let mut s = Scenario::open(&fixture("t6b", None));
    open_panel(&mut s, 0);
    s.settle();
    let id = "docx-style-font-list-ascii";
    crate::shell::scenario::button_probe::click(&mut s, id);
    s.settle();
    assert_eq!(s.with(|e, _| e.toolbar_search_open), Some(id), "the list is open");
    let rows = |s: &mut Scenario, query: &str| -> Vec<String> {
        s.update(|e, window, cx| {
            let input = e.toolbar_search_inputs.get(id).expect("the search field").clone();
            input.update(cx, |i, cx| i.set_value(query, window, cx));
        });
        s.settle();
        s.update(|e, _, cx| {
            let items = e.toolbar_search_popup_items.as_ref().map(|(_, i)| i.clone()).expect("the list's items");
            let query = e.toolbar_search_inputs.get(id).unwrap().read(cx).value().to_string();
            crate::shell::chrome::filter_menu_items(&items, &query).iter().map(|m| m.label.to_string()).collect()
        })
    };
    let all = rows(&mut s, "");
    let arial = rows(&mut s, "Arial");
    let none = rows(&mut s, "zzzzqq");
    let ja = rows(&mut s, "ゴ");
    eprintln!("all {} arial {:?} none {:?} ja {:?}", all.len(), arial, none, &ja[..ja.len().min(8)]);
    assert!(all.len() > 20, "the list has the installed typefaces and the theme fonts");
    assert!(!arial.is_empty() && arial.len() < all.len() && arial.iter().all(|l| l.to_lowercase().contains("arial")), "{arial:?}");
    assert!(none.is_empty(), "{none:?}");
    assert!(ja.len() < all.len() && ja.iter().all(|l| l.contains('ゴ')), "{ja:?}");
}

/// T20: the list open in the panel, Japanese UI light, English UI light, Japanese UI dark.
fn list_open_pictures() {
    use crate::shell::i18n::{self, Language};
    let _probe = crate::shell::scenario::button_probe::Probe::new();
    let mut s = Scenario::open(&fixture("t20", None));
    let id = "docx-style-font-list-ascii";
    for (lang, label) in [(Language::Japanese, "ja-light"), (Language::English, "en-light")] {
        i18n::show(lang);
        open_panel(&mut s, 0);
        s.settle();
        crate::shell::scenario::button_probe::click(&mut s, id);
        s.settle();
        assert_eq!(s.with(|e, _| e.toolbar_search_open), Some(id));
        s.screenshot(label);
        s.key("escape");
        s.settle();
    }
    i18n::show(Language::Japanese);
    open_panel(&mut s, 0);
    s.update(|_, _, cx| crate::shell::palette::system_dark(cx));
    s.settle();
    crate::shell::scenario::button_probe::click(&mut s, id);
    s.settle();
    s.screenshot("ja-dark");
}
