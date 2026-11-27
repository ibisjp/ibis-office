// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #227: the PDF of a document or a presentation keeps its hyperlinks (`/Subtype /Link` annotations: an address, a mailto, a bookmark of the document, a slide), as the
//! PDFs of Word and PowerPoint do.
use std::path::Path;
use image::ImageEncoder;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::issue227_a_link_on_a_group_is_the_union_of_its_shapes_boxes", a_link_on_a_group_is_the_union_of_its_shapes_boxes),
        ("pptx::issue227_a_turned_shapes_text_link_is_its_glyph_box_turned", a_turned_shapes_text_link_is_its_glyph_box_turned),
        ("pptx::issue227_a_picture_alone_in_a_hyperlink_has_no_link_and_one_with_text_is_one_rectangle", a_picture_alone_in_a_hyperlink_has_no_link_and_one_with_text_is_one_rectangle),
        ("pptx::issue227_the_shapes_of_a_docx_group_share_the_groups_link", the_shapes_of_a_docx_group_share_the_groups_link),
        ("pptx::issue227_a_shape_with_text_has_no_link_unless_turned_or_flipped", a_shape_with_text_has_no_link_unless_turned_or_flipped),
        ("pptx::issue227_a_link_in_text_of_a_turned_shape_stays_in_its_box", a_link_in_text_of_a_turned_shape_stays_in_its_box),
        ("pptx::issue227_a_document_pdf_has_its_hyperlinks", a_document_pdf_has_its_hyperlinks),
        ("pptx::issue227_a_presentation_pdf_has_its_shape_links", a_presentation_pdf_has_its_shape_links),
        ("pptx::issue227_a_text_run_link_is_over_its_characters", a_text_run_link_is_over_its_characters),
        ("pptx::issue227_a_link_in_a_table_cell_and_on_a_turned_shape", a_link_in_a_table_cell_and_on_a_turned_shape),
        ("pptx::issue227_a_link_on_a_picture_of_a_document", a_link_on_a_picture_of_a_document),
        ("pptx::issue227_a_document_link_is_the_line_box_with_its_spacing", a_document_link_is_the_line_box_with_its_spacing),
        ("pptx::issue227_a_bookmark_link_has_a_whole_number_destination", a_bookmark_link_has_a_whole_number_destination),
        ("pptx::issue227_a_footnote_reference_links_to_its_note", a_footnote_reference_links_to_its_note),
        ("pptx::issue227_a_link_in_vertical_text_runs_down_the_column", a_link_in_vertical_text_runs_down_the_column),
        ("pptx::issue227_a_floating_picture_link_is_its_frame", a_floating_picture_link_is_its_frame),
        ("pptx::issue227_next_and_previous_slide_links_have_no_annotation", next_and_previous_slide_links_have_no_annotation),
        ("pptx::issue227_a_shape_link_reaches_out_to_its_outline_and_shadow", a_shape_link_reaches_out_to_its_outline_and_shadow),
        ("pptx::issue227_a_link_that_goes_on_in_the_next_line_runs_to_the_margin", a_link_that_goes_on_in_the_next_line_runs_to_the_margin),
        ("pptx::issue227_a_hyperlink_field_shows_its_result_and_links_it", a_hyperlink_field_shows_its_result_and_links_it),
        ("pptx::issue227_the_cell_margins_of_a_table_by_its_style_and_the_default", the_cell_margins_of_a_table_by_its_style_and_the_default),
        ("pptx::issue227_a_table_of_a_file_with_no_table_style_has_no_cell_margin", a_table_of_a_file_with_no_table_style_has_no_cell_margin),
        ("pptx::issue227_an_inline_shape_stands_on_the_baseline_like_a_picture", an_inline_shape_stands_on_the_baseline_like_a_picture),
        ("pptx::issue227_the_boxes_of_paragraphs_with_space_after_and_before_follow_each_other", the_boxes_of_paragraphs_with_space_after_and_before_follow_each_other),
        ("pptx::issue227_a_link_in_a_text_box_of_a_document_is_over_its_line", a_link_in_a_text_box_of_a_document_is_over_its_line),
        ("pptx::issue227_a_bookmark_in_the_middle_of_a_paragraph_is_a_destination_at_its_character", a_bookmark_in_the_middle_of_a_paragraph_is_a_destination_at_its_character),
    ]
}

fn options(dir: &Path, name: &str) -> crate::export::Options {
    crate::export::Options {
        format: crate::export::Format::Pdf, out_dir: dir.into(), name: name.into(), width: 960, slides: Vec::new(), layout: false,
        file_name: None, print_backgrounds: true, print_hidden_text: false, print_markup: false, color: Default::default(), handout: None,
        frame: false, handout_landscape: false, paper: None, fit_paper: true, notes_pages: false, outline_pages: false, notes_source: None, handout_marks: Default::default(),
    }
}

/// (the `/Link` annotations, the ones with a `/URI`, the ones that go to a place of the file).
fn counts(pdf: &[u8]) -> (usize, usize, usize) {
    let text = String::from_utf8_lossy(pdf);
    let links = text.matches("/Subtype/Link").count();
    let uris = text.matches("/S/URI").count();
    let places = text.matches("/Dest ").count() + text.matches("/S/GoTo").count();
    (links, uris, places)
}

fn docx_with_links() -> Vec<u8> {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId10" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/a" TargetMode="External"/><Relationship Id="rId11" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="mailto:a@example.com" TargetMode="External"/></Relationships>"#;
    let link = |text: &str, attrs: &str| format!(r#"<w:p><w:hyperlink {attrs}><w:r><w:t>{text}</w:t></w:r></w:hyperlink></w:p>"#);
    let body = format!(
        "{}{}{}<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p><w:p><w:bookmarkStart w:id=\"1\" w:name=\"second\"/><w:r><w:t>Second page</w:t></w:r><w:bookmarkEnd w:id=\"1\"/></w:p>",
        link("The site", r#"r:id="rId10""#), link("The mail", r#"r:id="rId11""#), link("The bookmark", r#"w:anchor="second""#)
    );
    let document = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#);
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    z.finish().unwrap()
}

fn a_document_pdf_has_its_hyperlinks() {
    let presentation = ibis_formats::read(&docx_with_links()).expect("the document");
    let dir = Path::new("target/scenario/issue227");
    std::fs::create_dir_all(dir).unwrap();
    crate::export::export(presentation, &options(dir, "links-docx")).expect("the PDF");
    let pdf = std::fs::read(dir.join("links-docx.pdf")).unwrap();
    assert_eq!(counts(&pdf), (3, 2, 1), "an address, a mailto and a bookmark");
}

fn a_presentation_pdf_has_its_shape_links() {
    let root = crate::shell::scenario::repo_root();
    let data = std::fs::read(root.join("test/editing/pptx-plan.pptx")).expect("the plan");
    let mut presentation = ibis_formats::read(&data).expect("the presentation");
    let second = presentation.slides[1].id;
    let (a, b) = (presentation.slides[0].shapes[0].id, presentation.slides[0].shapes[1].id);
    presentation.slides[0].shape_links.insert(a, ibis_doc_model::SlideLink::Url("https://example.com/p".into()));
    presentation.slides[0].shape_links.insert(b, ibis_doc_model::SlideLink::Slide(second));
    let dir = root.join("target/scenario/issue227");
    std::fs::create_dir_all(&dir).unwrap();
    crate::export::export(presentation, &options(&dir, "links-pptx")).expect("the PDF");
    let pdf = std::fs::read(dir.join("links-pptx.pdf")).unwrap();
    assert_eq!(counts(&pdf), (2, 1, 1), "an address and a slide");
}

/// A run that is a link is a `/Link` over its characters (the box of its line), not over the shape's frame.
fn a_text_run_link_is_over_its_characters() {
    let root = crate::shell::scenario::repo_root();
    let data = std::fs::read(root.join("test/editing/pptx-plan.pptx")).expect("the plan");
    let mut presentation = ibis_formats::read(&data).expect("the presentation");
    let (shape_index, frame) = presentation.slides[0].shapes.iter().enumerate().find_map(|(i, s)| s.text_body().is_some_and(|b| !b.is_empty()).then_some((i, s.frame))).expect("a shape with text");
    let body = presentation.slides[0].shapes[shape_index].text_body_mut().unwrap();
    let run = body.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()).find(|r| !r.text.is_empty()).expect("a run");
    run.style.link = Some(ibis_doc_model::SlideLink::Url("https://example.com/t".into()));
    let dir = root.join("target/scenario/issue227");
    std::fs::create_dir_all(&dir).unwrap();
    crate::export::export(presentation, &options(&dir, "links-text")).expect("the PDF");
    let pdf = std::fs::read(dir.join("links-text.pdf")).unwrap();
    assert_eq!(counts(&pdf), (1, 1, 0), "one address");
    // Its rectangle is inside the shape's frame (points) and smaller than it.
    let text = String::from_utf8_lossy(&pdf);
    let at = text.find("/Subtype/Link/Rect[").expect("a link") + "/Subtype/Link/Rect[".len();
    let rect: Vec<f32> = text[at..].split(']').next().unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
    let (fx, fw) = (frame.x as f32 / 12_700.0, frame.width as f32 / 12_700.0);
    assert!(rect[0] >= fx - 1.0 && rect[2] <= fx + fw + 1.0, "inside the frame: {rect:?} in x {fx}..{}", fx + fw);
    assert!(rect[2] - rect[0] < fw, "narrower than the frame: {rect:?} against width {fw}");
}

/// A link on the text of a table's cell is over its characters (in the cell), and a link on a turned shape is over the box that holds it.
fn a_link_in_a_table_cell_and_on_a_turned_shape() {
    use ibis_doc_model::{EmuRect, Shape, ShapeKind, SlideLink, Table};
    let root = crate::shell::scenario::repo_root();
    let data = std::fs::read(root.join("test/editing/pptx-plan.pptx")).expect("the plan");
    let mut presentation = ibis_formats::read(&data).expect("the presentation");
    let mut table = Table::new_grid(2, 2, 3_000_000, 1_200_000);
    table.rows[1].cells[1].text.set_text("cell link");
    table.rows[1].cells[1].text.paragraphs[0].runs[0].style.link = Some(SlideLink::Url("https://example.com/cell".into()));
    let (table_id, turned_id) = (presentation.new_shape_id(), presentation.new_shape_id());
    let slide = &mut presentation.slides[0];
    slide.shapes.push(Shape::new(table_id, EmuRect::new(4_000_000, 2_000_000, 3_000_000, 1_200_000), ShapeKind::Table(Box::new(table))));
    let mut turned = Shape::new(turned_id, EmuRect::new(1_000_000, 1_000_000, 2_000_000, 400_000), ShapeKind::Auto(ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, ibis_doc_model::Fill::None, None)));
    turned.rotation = 90 * 60_000;
    slide.shapes.push(turned);
    slide.shape_links.insert(turned_id, SlideLink::Url("https://example.com/turned".into()));
    let dir = root.join("target/scenario/issue227");
    std::fs::create_dir_all(&dir).unwrap();
    crate::export::export(presentation, &options(&dir, "links-cell")).expect("the PDF");
    let pdf = std::fs::read(dir.join("links-cell.pdf")).unwrap();
    assert_eq!(counts(&pdf), (2, 2, 0), "the cell's and the turned shape's");
    let text = String::from_utf8_lossy(&pdf);
    let rect_of = |uri: &str| -> Vec<f32> {
        let at = text.find(uri).unwrap_or_else(|| panic!("{uri}"));
        let start = text[..at].rfind("/Rect[").expect("a rect") + "/Rect[".len();
        text[start..].split(']').next().unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect()
    };
    // The cell's: inside the table's frame (4 000 000 to 7 000 000 EMU: 314.96 to 551.18 pt) and narrower than a column.
    let cell = rect_of("https://example.com/cell");
    assert!(cell[0] >= 314.0 && cell[2] <= 552.0 && cell[2] - cell[0] < 118.0, "the cell's rectangle {cell:?}");
    // The turned one: 2 000 000 x 400 000 EMU turned a quarter is 400 000 wide (31.5 pt) and 2 000 000 high (157.5 pt) about the same centre.
    let turned = rect_of("https://example.com/turned");
    assert!((turned[2] - turned[0] - 31.5).abs() < 0.5 && (turned[3] - turned[1] - 157.5).abs() < 0.5, "the turned shape's rectangle {turned:?}");
}

/// A picture with a link (`wp:docPr/a:hlinkClick`, as Word writes the link of a picture) is a `/Link` over the picture.
fn a_link_on_a_picture_of_a_document() {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(&[255u8, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8).unwrap();
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let ct = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/><Relationship Id="rId8" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/picture" TargetMode="External"/></Relationships>"#;
    let drawing = r#"<w:p><w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><wp:extent cx="1800000" cy="900000"/><wp:docPr id="1" name="Picture 1"><a:hlinkClick xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" r:id="rId8"/></wp:docPr><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="0" name="p"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId7"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1800000" cy="900000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#;
    let document = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{drawing}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#);
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    z.add("word/media/image1.png", EntryMeta::new(), &png).unwrap();
    let presentation = ibis_formats::read(&z.finish().unwrap()).expect("the document");
    let dir = Path::new("target/scenario/issue227");
    std::fs::create_dir_all(dir).unwrap();
    crate::export::export(presentation, &options(dir, "links-picture")).expect("the PDF");
    let pdf = std::fs::read(dir.join("links-picture.pdf")).unwrap();
    assert_eq!(counts(&pdf), (1, 1, 0), "the picture's link");
    let text = String::from_utf8_lossy(&pdf);
    let at = text.find("/Subtype/Link/Rect[").unwrap() + "/Subtype/Link/Rect[".len();
    let rect: Vec<f32> = text[at..].split(']').next().unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
    // 1 800 000 x 900 000 EMU: 141.7 x 70.9 pt, at the page's left margin (72 pt): as wide and as high as the picture's frame (Word's PDF: the frame, nothing added).
    assert!((rect[2] - rect[0] - 141.7).abs() < 1.0 && (rect[3] - rect[1] - 70.87).abs() < 0.1 && (rect[0] - 72.0).abs() < 1.0, "the picture's rectangle {rect:?}");
}

/// A docx (`document.xml`'s `body`, with the extra parts `(name, content type or None, bytes)` and the extra `w:sectPr` content `section`).
fn docx_of(body: &str, section: &str, parts: &[(&str, &str, &[u8])], rels_extra: &str) -> Vec<u8> {
    use ibis_formats::ibis_ooxml::zip::{EntryMeta, Writer};
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let overrides: String = parts.iter().filter(|p| !p.1.is_empty()).map(|p| format!(r#"<Override PartName="/{}" ContentType="{}"/>"#, p.0, p.1)).collect();
    let ct = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>{overrides}</Types>"#);
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let doc_rels = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId10" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com/a" TargetMode="External"/>{rels_extra}</Relationships>"#);
    let document = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{w}" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body>{body}<w:sectPr>{section}<w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#);
    let mut z = Writer::new();
    z.add("[Content_Types].xml", EntryMeta::new(), ct.as_bytes()).unwrap();
    z.add("_rels/.rels", EntryMeta::new(), rels.as_bytes()).unwrap();
    z.add("word/_rels/document.xml.rels", EntryMeta::new(), doc_rels.as_bytes()).unwrap();
    z.add("word/document.xml", EntryMeta::new(), document.as_bytes()).unwrap();
    for (name, _, bytes) in parts {
        z.add(name, EntryMeta::new(), bytes).unwrap();
    }
    z.finish().unwrap()
}

/// The PDF of a docx.
fn docx_pdf(docx: &[u8], name: &str) -> Vec<u8> {
    let presentation = ibis_formats::read(docx).expect("the document");
    let dir = Path::new("target/scenario/issue227");
    std::fs::create_dir_all(dir).unwrap();
    crate::export::export(presentation, &options(dir, name)).expect("the PDF");
    std::fs::read(dir.join(format!("{name}.pdf"))).unwrap()
}

/// The `/Rect`s of the links of a PDF page `page_height` high, as (x, y from the top, width, height) in points.
fn link_rects(pdf: &[u8], page_height: f32) -> Vec<[f32; 4]> {
    let text = String::from_utf8_lossy(pdf);
    text.match_indices("/Subtype/Link/Rect[")
        .map(|(at, m)| {
            let v: Vec<f32> = text[at + m.len()..].split(']').next().unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
            [v[0], page_height - v[3], v[2] - v[0], v[3] - v[1]]
        })
        .collect()
}

/// The whole-number destinations (`/XYZ x y 0`) of a PDF: (x, y from the bottom).
fn destinations(pdf: &[u8]) -> Vec<(f32, f32)> {
    let text = String::from_utf8_lossy(pdf);
    text.match_indices("/XYZ ")
        .map(|(at, m)| {
            let v: Vec<f32> = text[at + m.len()..].split(']').next().unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
            assert_eq!(v[2], 0.0, "the zoom is 0");
            (v[0], v[1])
        })
        .collect()
}

const A4_HEIGHT: f32 = 841.9;

/// Word's PDF has a link over the box of its line, the space before a paragraph's first line and the one after its last line in it, from 3 pixels (2.25 pt) left of the first
/// linked character on the pixel grid: a paragraph of 12 pt before and after is 24 pt higher than the one without.
fn a_document_link_is_the_line_box_with_its_spacing() {
    let link = |before: u32, after: u32| format!(r#"<w:p><w:pPr><w:spacing w:before="{before}" w:after="{after}"/></w:pPr><w:hyperlink r:id="rId10"><w:r><w:t>The site</w:t></w:r></w:hyperlink></w:p>"#);
    let body = |before: u32, after: u32| format!(r#"<w:p><w:r><w:t>Top line</w:t></w:r></w:p>{}"#, link(before, after));
    let plain = link_rects(&docx_pdf(&docx_of(&body(0, 0), "", &[], ""), "links-box-plain"), A4_HEIGHT);
    let spaced = link_rects(&docx_pdf(&docx_of(&body(240, 240), "", &[], ""), "links-box-spaced"), A4_HEIGHT);
    assert_eq!((plain.len(), spaced.len()), (1, 1));
    let (p, s) = (plain[0], spaced[0]);
    assert!((s[3] - p[3] - 24.0).abs() < 0.05, "24 pt of spacing in the box: {p:?} {s:?}");
    // The paragraph's top is its spacing above its text's line: the box starts 12 pt above where the plain box starts after the same line before it.
    assert!(s[1] - p[1] < 0.05, "the box starts at the paragraph's top, the line before it ends there: {p:?} {s:?}");
    // 3 pixels left of the text (72 pt), on the 0.75 pt grid; the width a multiple of it.
    for r in [p, s] {
        assert!((r[0] - 69.75).abs() < 0.01, "left of the link {r:?}");
        let pixels = r[2] / 0.75;
        assert!((pixels - pixels.round()).abs() < 0.02, "width on the pixel grid {r:?}");
    }
}

/// A link to a bookmark is a destination `/XYZ x y 0` of whole numbers: x 3 pixels left of the paragraph's text rounded down (69), y the paragraph's top above the page's bottom
/// rounded down.
fn a_bookmark_link_has_a_whole_number_destination() {
    let body = r#"<w:p><w:hyperlink w:anchor="second"><w:r><w:t>The bookmark</w:t></w:r></w:hyperlink></w:p><w:p><w:r><w:br w:type="page"/></w:r></w:p><w:p><w:bookmarkStart w:id="1" w:name="second"/><w:r><w:t>Second page</w:t></w:r><w:bookmarkEnd w:id="1"/></w:p>"#;
    let pdf = docx_pdf(&docx_of(body, "", &[], ""), "links-dest");
    let text = String::from_utf8_lossy(&pdf);
    assert_eq!(counts(&pdf), (1, 0, 1));
    assert!(!text.contains("/Dests") && !text.contains("/Names"), "no named destinations");
    let d = destinations(&pdf);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].0, 69.0, "x");
    // The page's top margin is 72 pt: 841.92 - 72 = 769.92 rounded down.
    assert_eq!(d[0].1, 769.0, "y");
}

/// The reference to a footnote in the text is a link to the note (without a `w:hyperlink`), over the line.
fn a_footnote_reference_links_to_its_note() {
    let w = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
    let notes = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:footnotes xmlns:w="{w}"><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:type="continuationSeparator" w:id="0"><w:p><w:r><w:continuationSeparator/></w:r></w:p></w:footnote><w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r><w:r><w:t xml:space="preserve"> The note.</w:t></w:r></w:p></w:footnote></w:footnotes>"#);
    let body = r#"<w:p><w:r><w:t xml:space="preserve">Text</w:t></w:r><w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="1"/></w:r></w:p>"#;
    let parts = [("word/footnotes.xml", "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml", notes.as_bytes())];
    let rels = r#"<Relationship Id="rId20" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/footnotes" Target="footnotes.xml"/>"#;
    let pdf = docx_pdf(&docx_of(body, "", &parts, rels), "links-footnote");
    assert_eq!(counts(&pdf), (1, 0, 1), "the reference's link to the note");
    let d = destinations(&pdf);
    // The note is near the page's bottom.
    assert!(d[0].1 < 150.0, "the note's destination {d:?}");
}

/// A link in text that runs down (縦書き) is a rectangle along the column: as wide as the column, longer than it is wide, 3 pixels beyond the linked characters.
fn a_link_in_vertical_text_runs_down_the_column() {
    let body = r#"<w:p><w:hyperlink r:id="rId10"><w:r><w:t>ABCDEFGHI</w:t></w:r></w:hyperlink></w:p>"#;
    let pdf = docx_pdf(&docx_of(body, r#"<w:textDirection w:val="tbRl"/>"#, &[], ""), "links-vertical");
    let rects = link_rects(&pdf, A4_HEIGHT);
    assert_eq!(rects.len(), 1);
    let r = rects[0];
    assert!(r[3] > r[2] * 2.0, "along the column {r:?}");
    assert!((r[1] - (72.0 - 2.25)).abs() < 1.0 || r[1] >= 69.0, "starts 3 pixels before the first character {r:?}");
}

/// A floating picture with a link is a rectangle over its frame (no 3 pixels beyond it).
fn a_floating_picture_link_is_its_frame() {
    use image::ImageEncoder;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(&[255u8, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8).unwrap();
    let drawing = r#"<w:p><w:r><w:t>Text</w:t></w:r><w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>2540000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>3810000</wp:posOffset></wp:positionV><wp:extent cx="1200000" cy="900000"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="1" name="Picture 1"><a:hlinkClick xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" r:id="rId10"/></wp:docPr><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="0" name="p"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId7"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1200000" cy="900000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#;
    let rels = r#"<Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>"#;
    let parts = [("word/media/image1.png", "", png.as_slice())];
    let pdf = docx_pdf(&docx_of(drawing, "", &parts, rels), "links-floating");
    let rects = link_rects(&pdf, A4_HEIGHT);
    assert_eq!(rects.len(), 1, "the picture's link");
    let r = rects[0];
    // 1 200 000 x 900 000 EMU = 94.49 x 70.87 pt at 200 pt and 300 pt from the page's left and top.
    assert!((r[2] - 94.49).abs() < 0.5 && (r[3] - 70.87).abs() < 0.5, "the frame's size {r:?}");
    assert!((r[0] - 200.0).abs() < 0.5 && (r[1] - 300.0).abs() < 0.5, "the frame's place {r:?}");
}

/// PowerPoint's PDF has no link for the next, previous, first and last slide, nor for ending the show; a link to a slide goes to the top left of its page.
fn next_and_previous_slide_links_have_no_annotation() {
    let root = crate::shell::scenario::repo_root();
    let data = std::fs::read(root.join("test/editing/pptx-plan.pptx")).expect("the plan");
    let mut presentation = ibis_formats::read(&data).expect("the presentation");
    let second = presentation.slides[1].id;
    let mut ids = Vec::new();
    for k in 0..5 {
        let id = presentation.new_shape_id();
        let auto = ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, ibis_doc_model::Fill::None, None);
        presentation.slides[0].shapes.push(ibis_doc_model::Shape::new(id, ibis_doc_model::EmuRect::new(500_000 + k * 700_000, 5_000_000, 600_000, 400_000), ibis_doc_model::ShapeKind::Auto(auto)));
        ids.push(id);
    }
    use ibis_doc_model::SlideLink::*;
    for (id, link) in ids.iter().zip([NextSlide, PreviousSlide, FirstSlide, LastSlide, Slide(second)]) {
        presentation.slides[0].shape_links.insert(*id, link);
    }
    let dir = root.join("target/scenario/issue227");
    std::fs::create_dir_all(&dir).unwrap();
    crate::export::export(presentation, &options(&dir, "links-nav")).expect("the PDF");
    let pdf = std::fs::read(dir.join("links-nav.pdf")).unwrap();
    assert_eq!(counts(&pdf), (1, 0, 1), "only the link to a slide");
    let d = destinations(&pdf);
    assert_eq!(d, vec![(0.0, 540.0)], "the top left of the slide's page");
}

/// A shape's link reaches out half the outline's width on every side, and over its drop shadow (to the right and below).
fn a_shape_link_reaches_out_to_its_outline_and_shadow() {
    use ibis_doc_model::{AutoShape, Color, EmuRect, Fill, Geometry, Shadow, Shape, ShapeKind, SlideLink, Stroke};
    let root = crate::shell::scenario::repo_root();
    let data = std::fs::read(root.join("test/editing/pptx-plan.pptx")).expect("the plan");
    let mut presentation = ibis_formats::read(&data).expect("the presentation");
    let (a, b) = (presentation.new_shape_id(), presentation.new_shape_id());
    let mut outlined = AutoShape::new(Geometry::Rect, Fill::None, Some(Stroke::solid(Color::BLACK, 19_050)));
    outlined.shadow = None;
    let mut shadowed = AutoShape::new(Geometry::Rect, Fill::None, None);
    shadowed.shadow = Some(Shadow::simple(Color::BLACK, 50_800, 127_000, 2_700_000));
    let slide = &mut presentation.slides[0];
    slide.shapes.push(Shape::new(a, EmuRect::new(1_270_000, 1_270_000, 2_540_000, 1_270_000), ShapeKind::Auto(outlined)));
    slide.shapes.push(Shape::new(b, EmuRect::new(5_080_000, 1_270_000, 2_540_000, 1_270_000), ShapeKind::Auto(shadowed)));
    slide.shape_links.insert(a, SlideLink::Url("https://example.com/outline".into()));
    slide.shape_links.insert(b, SlideLink::Url("https://example.com/shadow".into()));
    let dir = root.join("target/scenario/issue227");
    std::fs::create_dir_all(&dir).unwrap();
    crate::export::export(presentation, &options(&dir, "links-outline")).expect("the PDF");
    let pdf = std::fs::read(dir.join("links-outline.pdf")).unwrap();
    let rects = link_rects(&pdf, 540.0);
    assert_eq!(rects.len(), 2);
    // 100 x 100 pt (1 270 000 EMU) at (100, 100) with a 1.5 pt outline: 0.75 pt out on every side.
    let r = rects[0];
    assert!((r[0] - 99.25).abs() < 0.05 && (r[1] - 99.25).abs() < 0.05 && (r[2] - 200.0 - 1.5).abs() < 0.05 && (r[3] - 100.0 - 1.5).abs() < 0.05, "outline {r:?}");
    // The shadow 10 pt (127 000 EMU) down-right (45 degrees: 2 700 000) with 4 pt of blur: 10 cos 45 + 4 = 11.07 pt right and below, none left and above.
    let s = rects[1];
    assert!((s[0] - 400.0).abs() < 0.05 && (s[1] - 100.0).abs() < 0.05, "no reach left and above {s:?}");
    assert!((s[2] - 200.0 - 11.07).abs() < 0.1 && (s[3] - 100.0 - 11.07).abs() < 0.1, "reach right and below {s:?}");
}

/// A link that goes on in the next line: Word's rectangle of the line it leaves runs to the right margin (523.3 pt) and 3 pixels (2.25 pt) beyond it, not only to the end of what is
/// written on it (Word's PDFs: every line of a wrapped link but the last has its right at the margin + 2.25 pt, in lines that end 10 to 26 pt short of it).
fn a_link_that_goes_on_in_the_next_line_runs_to_the_margin() {
    let long = "The quick brown fox jumps over the lazy dog while the rest of this sentence keeps going so that it wraps across several lines of the page body for the test. ".repeat(2);
    let body = format!(r#"<w:p><w:r><w:t xml:space="preserve">Start: </w:t></w:r><w:hyperlink r:id="rId10"><w:r><w:t xml:space="preserve">{long}</w:t></w:r></w:hyperlink><w:r><w:t xml:space="preserve"> tail</w:t></w:r></w:p>"#);
    let rects = link_rects(&docx_pdf(&docx_of(&body, "", &[], ""), "links-wrapped"), A4_HEIGHT);
    assert!(rects.len() >= 3, "a rectangle for each line of the link: {rects:?}");
    let right = 595.3 - 72.0 + 2.25;
    // Every line the link leaves reaches the margin and 3 pixels beyond it (a line that ends in a space standing past the margin goes on by that space).
    for r in &rects[..rects.len() - 1] {
        assert!(r[0] + r[2] > right - 0.8, "a line the link leaves runs to the margin and 3 pixels beyond: {r:?}");
    }
    assert!(rects[..rects.len() - 1].iter().any(|r| (r[0] + r[2] - right).abs() < 0.8), "a line that ends short of the margin has its right at the margin and 3 pixels: {rects:?}");
    let last = rects[rects.len() - 1];
    assert!(last[0] + last[2] < right - 5.0, "the last line ends with the link: {last:?}");
}

/// A `HYPERLINK` field shows the text of its result (not its address) and links it: the rectangle is the one of the same text in a `w:hyperlink`.
fn a_hyperlink_field_shows_its_result_and_links_it() {
    let field = r#"<w:p><w:r><w:t xml:space="preserve">J. field: </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> HYPERLINK "https://example.com/a" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>field link text</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:p>"#;
    let link = r#"<w:p><w:r><w:t xml:space="preserve">J. field: </w:t></w:r><w:hyperlink r:id="rId10"><w:r><w:t>field link text</w:t></w:r></w:hyperlink></w:p>"#;
    let a = link_rects(&docx_pdf(&docx_of(field, "", &[], ""), "links-field"), A4_HEIGHT);
    let b = link_rects(&docx_pdf(&docx_of(link, "", &[], ""), "links-field-same"), A4_HEIGHT);
    assert_eq!((a.len(), b.len()), (1, 1));
    for k in 0..4 {
        assert!((a[0][k] - b[0][k]).abs() < 0.01, "the field's link is the hyperlink's: {a:?} {b:?}");
    }
}

fn styles_part(table_style: &str) -> Vec<u8> {
    format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>{table_style}</w:styles>"#).into_bytes()
}

/// A file whose styles have no default table style gives a table no margin at the cell's sides (Word's PDF: the text of a cell starts at the cell's edge, and a line runs to its
/// other edge); with the "Normal Table" style Word makes (its 108 twips) the text is 5.4 pt in.
fn a_table_of_a_file_with_no_table_style_has_no_cell_margin() {
    let table = r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:hyperlink r:id="rId10"><w:r><w:t>cell link</w:t></w:r></w:hyperlink></w:p></w:tc></w:tr></w:tbl><w:p/>"#;
    let rel = r#"<Relationship Id="rId21" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#;
    let ct = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
    let none = styles_part("");
    let normal = styles_part(r#"<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:tblPr><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>"#);
    // (Compatibility mode 15: the table's edge is at the margin.)
    let settings = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#;
    let rel = format!(r#"{rel}<Relationship Id="rId22" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>"#);
    let sct = "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml";
    let a = link_rects(&docx_pdf(&docx_of(table, "", &[("word/styles.xml", ct, &none), ("word/settings.xml", sct, settings)], &rel), "links-table-none"), A4_HEIGHT);
    let b = link_rects(&docx_pdf(&docx_of(table, "", &[("word/styles.xml", ct, &normal), ("word/settings.xml", sct, settings)], &rel), "links-table-normal"), A4_HEIGHT);
    assert_eq!((a.len(), b.len()), (1, 1));
    assert!((b[0][0] - a[0][0] - 5.4).abs() < 0.8, "5.4 pt of margin only with the Normal Table: {a:?} {b:?}");
}

/// Word's PDFs (`tasks/doc/requests/2026-11-16-227-table-margin-result.md`, compatibility mode 15): a table that names no style has 108 twips (5.4 pt) at the cell's sides when the
/// file has a default table style and none when it has not (no styles.xml included); a table that names a style has that style's margins and not the default style's.
fn the_cell_margins_of_a_table_by_its_style_and_the_default() {
    let plain = r#"<w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr><w:p><w:hyperlink r:id="rId10"><w:r><w:t>cell link</w:t></w:r></w:hyperlink></w:p></w:tc></w:tr></w:tbl>"#;
    let named = plain.replace("<w:tblPr>", r#"<w:tblPr><w:tblStyle w:val="TableGrid"/>"#);
    let rel = r#"<Relationship Id="rId21" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId22" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/>"#;
    let (ct, sct) = ("application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml", "application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml");
    let settings = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>"#;
    let normal = r#"<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:tblPr><w:tblCellMar><w:left w:w="108" w:type="dxa"/><w:right w:w="108" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>"#;
    let grid_bare = r#"<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:tblPr/></w:style>"#;
    let grid_200 = r#"<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:tblPr><w:tblCellMar><w:left w:w="200" w:type="dxa"/><w:right w:w="200" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>"#;
    let grid_based = r#"<w:style w:type="table" w:styleId="TableGrid"><w:name w:val="Table Grid"/><w:basedOn w:val="TableNormal"/><w:tblPr/></w:style>"#;
    let normal_zero = r#"<w:style w:type="table" w:default="1" w:styleId="TableNormal"><w:name w:val="Normal Table"/><w:tblPr><w:tblCellMar><w:left w:w="0" w:type="dxa"/><w:right w:w="0" w:type="dxa"/></w:tblCellMar></w:tblPr></w:style>"#;
    let left = |table: &str, styles: Option<String>, name: &str| -> f32 {
        let styles = styles.map(|x| styles_part(&x));
        let mut parts: Vec<(&str, &str, &[u8])> = vec![("word/settings.xml", sct, settings)];
        if let Some(bytes) = &styles {
            parts.push(("word/styles.xml", ct, bytes));
        }
        let rects = link_rects(&docx_pdf(&docx_of(table, "", &parts, rel), name), A4_HEIGHT);
        assert_eq!(rects.len(), 1, "{name}");
        rects[0][0]
    };
    // (A file with no styles.xml has no relationship to it either: the rel is there but the part is not.)
    let none = left(plain, None, "links-margin-no-styles");
    let no_default = left(plain, Some(String::new()), "links-margin-no-default");
    let default = left(plain, Some(normal.to_string()), "links-margin-default");
    // A file with no styles.xml, or with styles but no default table style, has no margin: Windows Word's XPS of one-factor tests (`2026-11-18-227-margin-s1-result.md`) puts 108
    // only where a `w:default="1"` table style is in the file (the earlier 108 for a file with no styles followed a misreading of the probes' PDFs).
    assert!((none - no_default).abs() < 0.3, "no styles.xml has no margin: {none} {no_default}");
    assert!((default - no_default - 5.4).abs() < 0.8, "108 twips with the default table style, none without: {no_default} {default}");
    // A named style with no margins: none, the default style's 108 is not inherited; with 200 twips: 10 pt.
    let bare = left(&named, Some(format!("{normal}{grid_bare}")), "links-margin-named-bare");
    let wide = left(&named, Some(format!("{normal}{grid_200}")), "links-margin-named-200");
    assert!((bare - no_default).abs() < 0.3, "the default style is not inherited by a named one: {no_default} {bare}");
    assert!((wide - no_default - 10.0).abs() < 0.8, "the named style's 200 twips: {no_default} {wide}");
    // A named style that is based on the Normal Table has its 108 twips (Word-made files: Table Grid is based on it); the Normal Table with 0 in the file is read as 108 all the same.
    let based = left(&named, Some(format!("{normal}{grid_based}")), "links-margin-named-based");
    assert!((based - no_default - 5.4).abs() < 0.8, "based on the Normal Table: {no_default} {based}");
    let zero = left(&named, Some(format!("{normal_zero}{grid_based}")), "links-margin-named-based-zero");
    assert!((zero - no_default - 5.4).abs() < 0.8, "the Normal Table's own 0 is not read: {no_default} {zero}");
    let zero_plain = left(plain, Some(normal_zero.to_string()), "links-margin-default-zero");
    assert!((zero_plain - no_default - 5.4).abs() < 0.8, "the default table style with 0: {no_default} {zero_plain}");
}

/// An inline shape stands on the baseline as an inline picture does, with the text's room below the baseline under it: the line of a shape 50 pt high is taller than the shape by
/// that room (Word's PDF of a 55 pt inline shape: the next line 3 pt lower than the shape's foot).
fn an_inline_shape_stands_on_the_baseline_like_a_picture() {
    let shape = r#"<w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0"><wp:extent cx="1500000" cy="635000"/><wp:docPr id="2" name="s"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1500000" cy="635000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FFE0A0"/></a:solidFill></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#;
    let body = format!(r#"<w:p><w:hyperlink r:id="rId10"><w:r><w:t>Shape: </w:t></w:r></w:hyperlink>{shape}</w:p>"#);
    let rects = link_rects(&docx_pdf(&docx_of(&body, "", &[], ""), "links-inline-shape"), A4_HEIGHT);
    assert_eq!(rects.len(), 1);
    // 635000 EMU = 50 pt.
    let room = rects[0][3] - 50.0;
    assert!(room > 2.0 && room < 6.0, "the text's room below the baseline under the shape: {rects:?}");
}

/// The boxes of the links of paragraphs follow each other: a paragraph with 6 pt after and the next with 12 pt before are 12 pt apart (the larger of the two, not both), the
/// first's box takes its 6 pt after and the second's takes only the 6 pt of its space before the first's does not cover (Word's PDF: the destination of a heading under a heading
/// with 6 pt after is 6 pt above its line, not 12).
fn the_boxes_of_paragraphs_with_space_after_and_before_follow_each_other() {
    let para = |before: u32, after: u32| format!(r#"<w:p><w:pPr><w:spacing w:before="{before}" w:after="{after}"/></w:pPr><w:hyperlink r:id="rId10"><w:r><w:t>The site</w:t></w:r></w:hyperlink></w:p>"#);
    let body = format!("{}{}", para(0, 120), para(240, 0));
    let rects = link_rects(&docx_pdf(&docx_of(&body, "", &[], ""), "links-tile"), A4_HEIGHT);
    assert_eq!(rects.len(), 2);
    let (a, b) = (rects[0], rects[1]);
    assert!((a[1] + a[3] - b[1]).abs() < 0.05, "the second box starts where the first ends: {a:?} {b:?}");
    assert!((b[3] - (a[3] - 6.0 + 6.0)).abs() < 0.05, "the second box: its line and the 6 pt of its space before not covered: {a:?} {b:?}");
}

/// A bookmark in the middle of a paragraph is a destination 3 pixels before the character it starts at (Word's PDF: "Before " takes 36 pt and the destination is 106, not the
/// paragraph's 69); one at the start of the paragraph is where the paragraph starts.
fn a_bookmark_in_the_middle_of_a_paragraph_is_a_destination_at_its_character() {
    let body = r#"<w:p><w:hyperlink w:anchor="mid"><w:r><w:t>go</w:t></w:r></w:hyperlink></w:p><w:p><w:r><w:t xml:space="preserve">Before </w:t></w:r><w:bookmarkStart w:id="1" w:name="mid"/><w:r><w:t>mid-bookmarked words</w:t></w:r><w:bookmarkEnd w:id="1"/><w:r><w:t xml:space="preserve"> after.</w:t></w:r></w:p>"#;
    let d = destinations(&docx_pdf(&docx_of(body, "", &[], ""), "links-mid-bookmark"));
    assert_eq!(d.len(), 1);
    // "Before " is some 30 to 40 pt of the 10.5 pt text: past 72 + 25 - 2.25 and short of 72 + 45.
    assert!(d[0].0 > 94.0 && d[0].0 < 117.0, "x at the character the bookmark starts at: {d:?}");
}

/// A link in the text of a text box (`wps:txbx`) of a document: the text is blue and underlined as the Hyperlink style says, and the PDF has the link over the box's line (the line's
/// height, from 3 pixels before the linked text to 3 pixels after it), inside the text box.
fn a_link_in_a_text_box_of_a_document_is_over_its_line() {
    let styles = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style><w:style w:type="character" w:styleId="Hyperlink"><w:name w:val="Hyperlink"/><w:rPr><w:color w:val="0563C1"/><w:u w:val="single"/></w:rPr></w:style></w:styles>"#;
    let rel = r#"<Relationship Id="rId21" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#;
    let ct = "application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml";
    // A 150 x 55 pt box at 236 pt from the column's left, 8 pt from the paragraph's top.
    let shape = r#"<w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="column"><wp:posOffset>3000000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="paragraph"><wp:posOffset>100000</wp:posOffset></wp:positionV><wp:extent cx="1905000" cy="698500"/><wp:wrapSquare wrapText="bothSides"/><wp:docPr id="2" name="box"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1905000" cy="698500"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent><w:p><w:hyperlink r:id="rId10"><w:r><w:rPr><w:rStyle w:val="Hyperlink"/></w:rPr><w:t>textbox link</w:t></w:r></w:hyperlink><w:r><w:t xml:space="preserve"> plain</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#;
    let body = format!(r#"<w:p><w:r><w:t>Text box:</w:t></w:r>{shape}</w:p>"#);
    let pdf = docx_pdf(&docx_of(&body, "", &[("word/styles.xml", ct, styles)], rel), "links-text-box");
    let rects = link_rects(&pdf, A4_HEIGHT);
    assert_eq!(rects.len(), 1, "the link in the box: {rects:?}");
    let r = rects[0];
    // The box is at 72 + 236.2 = 308.2 pt, 150 pt wide; its text starts 7.2 pt in; the line is 10 to 20 pt high.
    assert!(r[0] > 308.2 && r[0] + r[2] < 308.2 + 150.0, "inside the box across: {r:?}");
    assert!(r[3] > 8.0 && r[3] < 25.0, "the height of a line of text: {r:?}");
    assert!(r[2] > 30.0, "the width of the linked text with 3 pixels each side: {r:?}");
}

/// Word's PDF (Windows Word's ExportAsFixedFormat, #227): a `w:hyperlink` that holds only a picture has no link; one with text and a picture ("text <picture> more") has one
/// rectangle over the line, as high as the picture.
fn a_picture_alone_in_a_hyperlink_has_no_link_and_one_with_text_is_one_rectangle() {
    use image::ImageEncoder;
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png).write_image(&[255u8, 0, 0, 255], 1, 1, image::ExtendedColorType::Rgba8).unwrap();
    let picture = r#"<w:r><w:drawing><wp:inline xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing"><wp:extent cx="1800000" cy="900000"/><wp:docPr id="1" name="Picture 1"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="0" name="a.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId7"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1800000" cy="900000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#;
    let rels = r#"<Relationship Id="rId7" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/>"#;
    let parts = [("word/media/image1.png", "", png.as_slice())];
    let alone = format!(r#"<w:p><w:hyperlink r:id="rId10">{picture}</w:hyperlink></w:p>"#);
    let pdf = docx_pdf(&docx_of(&alone, "", &parts, rels), "links-picture-alone-in-link");
    assert_eq!(link_rects(&pdf, A4_HEIGHT).len(), 0, "a picture alone in a hyperlink has no link");
    let with_text = format!(r#"<w:p><w:hyperlink r:id="rId10"><w:r><w:t xml:space="preserve">text </w:t></w:r>{picture}<w:r><w:t xml:space="preserve"> more</w:t></w:r></w:hyperlink></w:p>"#);
    let pdf = docx_pdf(&docx_of(&with_text, "", &parts, rels), "links-picture-and-text-in-link");
    let rects = link_rects(&pdf, A4_HEIGHT);
    assert_eq!(rects.len(), 1, "one rectangle for the text and the picture: {rects:?}");
    assert!(rects[0][3] >= 70.8, "as high as the picture: {:?}", rects[0]);
}

/// Word's PDF (Windows Word, #227, `2026-11-17-227-word-shape-links-result.md` 1): a shape of a document with a link (`wp:docPr/a:hlinkClick`) that holds text (a `wps:txbx`, even empty or
/// one letter) and is neither turned nor flipped has no link; without text, or turned or flipped, it has the box that holds it.
fn a_shape_with_text_has_no_link_unless_turned_or_flipped() {
    let shape = |text: &str, xfrm_attr: &str| {
        let txbx = if text.is_empty() { String::new() } else { format!(r#"<wps:txbx><w:txbxContent><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:txbxContent></wps:txbx>"#) };
        format!(
            r#"<w:p><w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>2540000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>2540000</wp:posOffset></wp:positionV><wp:extent cx="2400000" cy="900000"/><wp:wrapNone/><wp:docPr id="1" name="Shape 1"><a:hlinkClick xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" r:id="rId10"/></wp:docPr><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:cNvSpPr/><wps:spPr><a:xfrm{xfrm_attr}><a:off x="0" y="0"/><a:ext cx="2400000" cy="900000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></wps:spPr>{txbx}<wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#
        )
    };
    let rects = |text: &str, xfrm_attr: &str, name: &str| link_rects(&docx_pdf(&docx_of(&shape(text, xfrm_attr), "", &[], ""), name), A4_HEIGHT);
    assert_eq!(rects("", "", "links-shape-plain").len(), 1, "no text: the shape's box");
    assert_eq!(rects("Hi", "", "links-shape-text").len(), 0, "text, not turned: no link");
    assert_eq!(rects("Hi", r#" rot="0""#, "links-shape-text-rot0").len(), 0, "rot 0 is not turned");
    assert_eq!(rects("Hi", r#" rot="60000""#, "links-shape-text-1deg").len(), 1, "turned 1 degree: the box that holds it");
    assert_eq!(rects("Hi", r#" flipH="1""#, "links-shape-text-flip").len(), 1, "flipped: the box");
}

/// The shapes of a group in a document (`wpg:wgp`) are read one by one with the link of the group's `wp:docPr`: Word's PDF has one rectangle over their union.
fn the_shapes_of_a_docx_group_share_the_groups_link() {
    let wsp = |x: i64, y: i64, w: i64, h: i64, id: u32| format!(r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></wps:spPr><wps:bodyPr/></wps:wsp><!--{id}-->"#);
    let drawing = format!(
        r#"<w:p><w:r><w:t>Text</w:t></w:r><w:r><w:drawing><wp:anchor xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>2540000</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>3810000</wp:posOffset></wp:positionV><wp:extent cx="3000000" cy="1000000"/><wp:wrapNone/><wp:docPr id="1" name="Group 1"><a:hlinkClick xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" r:id="rId10"/></wp:docPr><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="3000000" cy="1000000"/><a:chOff x="0" y="0"/><a:chExt cx="3000000" cy="1000000"/></a:xfrm></wpg:grpSpPr>{}{}</wpg:wgp></a:graphicData></a:graphic></wp:anchor></w:drawing></w:r></w:p>"#,
        wsp(0, 0, 1_200_000, 600_000, 2),
        wsp(1_800_000, 400_000, 1_200_000, 600_000, 3)
    );
    let pdf = docx_pdf(&docx_of(&drawing, "", &[], ""), "links-docx-group");
    let rects = link_rects(&pdf, A4_HEIGHT);
    assert_eq!(rects.len(), 1, "one rectangle for the group: {rects:?}");
    // 3 000 000 x 1 000 000 EMU is 236.2 x 78.7 pt (the two shapes' union, with their lines).
    assert!((rects[0][2] - 237.0).abs() < 2.0 && (rects[0][3] - 79.5).abs() < 2.0, "the union of the shapes: {:?}", rects[0]);
}

/// The link of a run in the text of a shape turned 30 degrees is a rectangle that stays inside the box that holds the turned shape.
fn a_link_in_text_of_a_turned_shape_stays_in_its_box() {
    let root = crate::shell::scenario::repo_root();
    let data = std::fs::read(root.join("test/editing/pptx-plan.pptx")).expect("the plan");
    let mut presentation = ibis_formats::read(&data).expect("the presentation");
    let (shape_index, frame) = presentation.slides[0].shapes.iter().enumerate().find_map(|(i, s)| s.text_body().is_some_and(|b| !b.is_empty()).then_some((i, s.frame))).expect("a shape with text");
    presentation.slides[0].shapes[shape_index].rotation = 30 * 60_000;
    let shape = presentation.slides[0].shapes[shape_index].clone();
    let body = presentation.slides[0].shapes[shape_index].text_body_mut().unwrap();
    let run = body.paragraphs.iter_mut().flat_map(|p| p.runs.iter_mut()).find(|r| !r.text.is_empty()).expect("a run");
    run.style.link = Some(ibis_doc_model::SlideLink::Url("https://example.com/turned-text".into()));
    let dir = root.join("target/scenario/issue227");
    std::fs::create_dir_all(&dir).unwrap();
    crate::export::export(presentation, &options(&dir, "links-turned-text")).expect("the PDF");
    let pdf = std::fs::read(dir.join("links-turned-text.pdf")).unwrap();
    assert_eq!(counts(&pdf), (1, 1, 0), "one address");
    let _ = frame;
    let [l, t, r, b] = shape.link_box(shape.frame);
    let text = String::from_utf8_lossy(&pdf);
    let at = text.find("/Subtype/Link/Rect[").unwrap() + "/Subtype/Link/Rect[".len();
    let rect: Vec<f32> = text[at..].split(']').next().unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
    let pt = |v: f64| v as f32 / 12_700.0;
    assert!(rect[0] >= pt(l) - 1.0 && rect[2] <= pt(r) + 1.0, "x inside the turned shape's box: {rect:?} in {}..{}", pt(l), pt(r));
}

/// PowerPoint's PDF of a link on a group (`a:hlinkClick` of the group's `cNvPr`): one annotation over the union of the shapes' boxes, each turned with the group
/// and with its line (Windows PowerPoint, a group turned 20 degrees: 309.6 x 261.9 pt, not the turned group's 355.4 x 259.8).
fn a_link_on_a_group_is_the_union_of_its_shapes_boxes() {
    use ibis_doc_model::{EmuRect, Shape, ShapeGroup, ShapeKind, SlideLink};
    let root = crate::shell::scenario::repo_root();
    let data = std::fs::read(root.join("test/editing/pptx-plan.pptx")).expect("the plan");
    let mut presentation = ibis_formats::read(&data).expect("the presentation");
    let (a, b) = (presentation.new_shape_id(), presentation.new_shape_id());
    let slide = &mut presentation.slides[0];
    let rect = |id, frame, turn: i32| {
        let mut shape = Shape::new(id, frame, ShapeKind::Auto(ibis_doc_model::AutoShape::new(ibis_doc_model::Geometry::Rect, ibis_doc_model::Fill::None, None)));
        shape.rotation = turn;
        shape
    };
    let (first, second) = (rect(a, EmuRect::new(1_000_000, 1_000_000, 2_000_000, 400_000), 20 * 60_000), rect(b, EmuRect::new(4_000_000, 2_000_000, 1_500_000, 900_000), 20 * 60_000));
    let expected: Vec<[f64; 4]> = [&first, &second].iter().map(|s| s.link_box(s.frame)).collect();
    slide.shapes.push(first);
    slide.shapes.push(second);
    slide.groups = vec![ShapeGroup::from_shape_ids("4242", "group", &[a, b])];
    slide.group_links.insert("4242".into(), SlideLink::Url("https://example.com/group".into()));
    let dir = root.join("target/scenario/issue227");
    std::fs::create_dir_all(&dir).unwrap();
    crate::export::export(presentation, &options(&dir, "links-group")).expect("the PDF");
    let pdf = std::fs::read(dir.join("links-group.pdf")).unwrap();
    assert_eq!(counts(&pdf), (1, 1, 0), "one annotation for the group");
    let text = String::from_utf8_lossy(&pdf);
    let at = text.find("/Subtype/Link/Rect[").unwrap() + "/Subtype/Link/Rect[".len();
    let rect: Vec<f32> = text[at..].split(']').next().unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
    let pt = |v: f64| v as f32 / 12_700.0;
    let (l, t) = (expected[0][0].min(expected[1][0]), expected[0][1].min(expected[1][1]));
    let (r, bt) = (expected[0][2].max(expected[1][2]), expected[0][3].max(expected[1][3]));
    assert!((rect[0] - pt(l)).abs() < 0.2 && (rect[2] - pt(r)).abs() < 0.2, "x {rect:?}: {}..{}", pt(l), pt(r));
    // (PDF's y is up from the page's foot; the page is 405 pt high here.)
    let height = (pt(bt) - pt(t)).abs();
    assert!(((rect[3] - rect[1]) - height).abs() < 0.2, "height {rect:?}: {height}");
}

/// PowerPoint's PDF of a link in the text of a turned shape: the box of the line's glyphs, turned about the shape's centre, its bounding rectangle (the box of
/// the unturned line turned a quarter is as wide as it was high and as high as it was wide).
fn a_turned_shapes_text_link_is_its_glyph_box_turned() {
    let root = crate::shell::scenario::repo_root();
    let sized = |turn: i32, name: &str| -> (f32, f32) {
        let data = std::fs::read(root.join("test/editing/pptx-plan.pptx")).expect("the plan");
        let mut presentation = ibis_formats::read(&data).expect("the presentation");
        let index = presentation.slides[0].shapes.iter().position(|s| s.text_body().is_some_and(|b| !b.is_empty())).expect("a shape with text");
        presentation.slides[0].shapes[index].rotation = turn;
        let body = presentation.slides[0].shapes[index].text_body_mut().unwrap();
        // One short line of one run, wholly the link.
        body.set_text("Link");
        body.paragraphs.truncate(1);
        body.paragraphs[0].runs[0].style.link = Some(ibis_doc_model::SlideLink::Url("https://example.com/glyphs".into()));
        let dir = root.join("target/scenario/issue227");
        std::fs::create_dir_all(&dir).unwrap();
        crate::export::export(presentation, &options(&dir, name)).expect("the PDF");
        let pdf = std::fs::read(dir.join(format!("{name}.pdf"))).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        let at = text.find("/Subtype/Link/Rect[").unwrap() + "/Subtype/Link/Rect[".len();
        let rect: Vec<f32> = text[at..].split(']').next().unwrap().split_whitespace().map(|v| v.parse().unwrap()).collect();
        (rect[2] - rect[0], rect[3] - rect[1])
    };
    let (w0, h0) = sized(0, "links-glyph-0");
    let (w90, h90) = sized(90 * 60_000, "links-glyph-90");
    assert!((w90 - h0).abs() < 0.2 && (h90 - w0).abs() < 0.2, "0 degrees {w0} x {h0}, 90 degrees {w90} x {h90}");
}
