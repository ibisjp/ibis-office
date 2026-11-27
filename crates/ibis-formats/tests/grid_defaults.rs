// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0060: the grid and guide settings of `viewProps.xml` (`p:gridSpacing`, `snapToGrid`, `snapToObjects`, `showGuides`) and the default shape,
//! line and text box of the theme (`a:objectDefaults`: `spDef`, `lnDef`, `txDef`) are read, written and read back.

use ibis_doc_model::{Color, Command, Document, EmuSize, Fill, Insets, ObjectDefaults, Rgb, ShapeDefault, Stroke, TextStyle, VerticalAnchor, ViewGrid};
use ibis_formats::SaveOptions;

fn file(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test").join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn entry(data: &[u8], part: &str) -> String {
    let package = ibis_ooxml::Package::open(data.to_vec()).unwrap();
    String::from_utf8(package.read(part).unwrap()).unwrap()
}

#[test]
fn the_grid_and_guide_settings_are_read_and_written() {
    let data = file("editing/pptx-footer-by-powerpoint.pptx");
    let opened = ibis_formats::open(data.clone()).unwrap();
    let read = opened.presentation.view_grid;
    // What PowerPoint wrote for this file (not the 1/12 inch default).
    assert_eq!(read.spacing, EmuSize::new(72_008, 72_008), "{read:?}");
    // Unchanged: the file as it was.
    let same = opened.source.save(&opened.presentation, SaveOptions::default()).unwrap();
    assert_eq!(entry(&same, "ppt/viewProps.xml"), entry(&data, "ppt/viewProps.xml"));

    let mut presentation = opened.presentation.clone();
    presentation.view_grid = ViewGrid { spacing: EmuSize::new(144_000, 108_000), snap_to_grid: false, snap_to_objects: true, show_guides: true };
    let saved = opened.source.save(&presentation, SaveOptions::default()).unwrap();
    let view = entry(&saved, "ppt/viewProps.xml");
    assert!(view.contains("gridSpacing") && view.contains("cx=\"144000\"") && view.contains("cy=\"108000\""), "{view}");
    assert!(view.contains("snapToGrid=\"0\"") && view.contains("snapToObjects=\"1\"") && view.contains("showGuides=\"1\""), "{view}");
    let again = ibis_formats::open(saved.clone()).unwrap();
    assert_eq!(again.presentation.view_grid, presentation.view_grid);

    // Put back to the start: snapToGrid is on by default, so the attribute goes; the others go too.
    let mut back = again.presentation.clone();
    back.view_grid = read;
    let restored = again.source.save(&back, SaveOptions::default()).unwrap();
    let view = entry(&restored, "ppt/viewProps.xml");
    assert!(!view.contains("snapToObjects") && !view.contains("showGuides"), "{view}");
    assert_eq!(ibis_formats::open(restored).unwrap().presentation.view_grid.spacing, read.spacing);
}

#[test]
fn a_file_with_no_view_properties_gets_the_grid_written() {
    let data = file("editing/pptx-plan.pptx");
    let opened = ibis_formats::open(data).unwrap();
    let mut presentation = opened.presentation.clone();
    presentation.view_grid = ViewGrid { spacing: EmuSize::new(180_000, 180_000), snap_to_grid: true, snap_to_objects: false, show_guides: false };
    let saved = opened.source.save(&presentation, SaveOptions::default()).unwrap();
    let again = ibis_formats::open(saved).unwrap();
    assert_eq!(again.presentation.view_grid, presentation.view_grid);
}

fn text_style(font: Option<&str>) -> TextStyle {
    TextStyle { color: Rgb(0x112233), font_size: 2400, bold: true, italic: true, font: font.map(str::to_owned), align: ibis_doc_model::TextAlign::Left, ..TextStyle::default() }
}

#[test]
fn the_default_shape_line_and_text_box_are_written_and_read_back() {
    let data = file("editing/pptx-plan.pptx");
    let opened = ibis_formats::open(data).unwrap();
    let shape = ShapeDefault {
        fill: Fill::Solid(Color::opaque(Rgb(0xff8800))),
        stroke: Some(Stroke::solid(Color::opaque(Rgb(0x00aa00)), 25_400)),
        text: text_style(Some("Meiryo")),
        shadow: None,
        glow: None,
        reflection: None,
        soft_edge: None,
        anchor: VerticalAnchor::Top,
        insets: Insets { left: 10_000, top: 20_000, right: 30_000, bottom: 40_000 },
        wrap: false,
    };
    let text_box = ShapeDefault { fill: Fill::None, stroke: None, text: text_style(None), anchor: VerticalAnchor::Bottom, insets: Insets::default(), wrap: true, ..shape.clone() };
    let line = Stroke::solid(Color::opaque(Rgb(0xaa0000)), 38_100);
    let defaults = ObjectDefaults { shape: Some(shape.clone()), line: Some(line.clone()), text_box: Some(text_box.clone()) };
    let mut document = Document::new(opened.presentation.clone());
    document.execute(vec![Command::SetObjectDefaults { defaults: defaults.clone() }]).unwrap();
    let saved = opened.source.save(document.presentation(), SaveOptions::default()).unwrap();

    let theme = entry(&saved, "ppt/theme/theme1.xml");
    // The three definitions, in the schema's order, and no empty `a:style` (it must hold its four references).
    let at = |tag: &str| theme.find(tag).unwrap_or_else(|| panic!("{tag} in {}", &theme[theme.find("objectDefaults").unwrap_or(0)..]));
    let (sp, ln, tx) = (at("<a:spDef"), at("<a:lnDef"), at("<a:txDef"));
    assert!(sp < ln && ln < tx, "{theme}");
    assert!(!theme.contains("<a:style/>") && !theme.contains("<a:style></a:style>"), "{theme}");
    assert!(theme.contains("<a:fontRef") && theme.contains("typeface=\"Meiryo\"") && theme.contains("anchor=\"t\"") && theme.contains("wrap=\"none\"") && theme.contains("lIns=\"10000\""), "{theme}");

    let again = ibis_formats::open(saved).unwrap();
    let got = &again.presentation.object_defaults;
    let s = got.shape.as_ref().expect("spDef");
    assert_eq!(s.fill, shape.fill);
    assert_eq!(s.stroke, shape.stroke);
    assert_eq!((s.anchor, s.insets, s.wrap), (shape.anchor, shape.insets, shape.wrap));
    assert_eq!((s.text.font_size, s.text.bold, s.text.italic, s.text.color, s.text.align, s.text.font.clone()), (2400, true, true, Rgb(0x112233), ibis_doc_model::TextAlign::Left, Some("Meiryo".to_owned())));
    let t = got.text_box.as_ref().expect("txDef");
    assert_eq!(t.fill, Fill::None);
    assert_eq!((t.anchor, t.wrap), (VerticalAnchor::Bottom, true));
    assert_eq!(t.text.font, None);
    assert_eq!(got.line.as_ref(), Some(&line));
}

#[test]
fn a_text_box_made_after_the_default_has_its_look() {
    let defaults = ObjectDefaults {
        shape: None,
        line: None,
        text_box: Some(ShapeDefault {
            fill: Fill::Solid(Color::opaque(Rgb(0xffff00))),
            stroke: None,
            text: text_style(Some("Meiryo")),
            shadow: None,
            glow: None,
            reflection: None,
            soft_edge: None,
            anchor: VerticalAnchor::Middle,
            insets: Insets::default(),
            wrap: true,
        }),
    };
    let made = defaults.apply_to_text_box(ibis_doc_model::Shape::text_box(ibis_doc_model::ShapeId(1), ibis_doc_model::EmuRect::new(0, 0, 100, 100), ""));
    // A fill makes it a rectangle with text, with the text's style as the default has it.
    let ibis_doc_model::ShapeKind::Auto(auto) = &made.kind else { panic!("{:?}", made.kind) };
    assert_eq!(auto.fill, Fill::Solid(Color::opaque(Rgb(0xffff00))));
    let body = auto.text.as_ref().unwrap();
    assert_eq!((body.style.font.clone(), body.style.font_size, body.anchor), (Some("Meiryo".to_owned()), 2400, VerticalAnchor::Middle));
    // With no fill and no line it stays a text box.
    let plain = ObjectDefaults { text_box: Some(ShapeDefault { fill: Fill::None, ..defaults.text_box.clone().unwrap() }), ..ObjectDefaults::default() };
    let made = plain.apply_to_text_box(ibis_doc_model::Shape::text_box(ibis_doc_model::ShapeId(1), ibis_doc_model::EmuRect::new(0, 0, 100, 100), ""));
    assert!(matches!(made.kind, ibis_doc_model::ShapeKind::TextBox(_)));
}
