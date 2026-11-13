// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Issue #83: drawings (EMF, SVG) pasted from other apps, and copy / paste as a picture as vector graphics.
use super::Scenario;
use crate::xlsx::scenarios::SheetSteps;
use gpui_kit::*;
use ibis_doc_model::{MediaFormat, ShapeKind};
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::issue83_emf_picture_is_drawn", emf_picture_is_drawn),
        ("xlsx::issue83_paste_drawings_and_bitmaps", paste_drawings_and_bitmaps),
        ("xlsx::issue83_copy_range_as_vector_picture", copy_range_as_vector_picture),
        ("xlsx::issue83_paste_vector_from_the_pasteboard", paste_vector_from_the_pasteboard),
    ]
}

const EMF: &[u8] = include_bytes!("../../../../../test/editing/xlsx-emf-picture-image1.emf");

fn pictures(s: &mut Scenario) -> Vec<(MediaFormat, bool)> {
    s.with(|e, _| {
        e.resources.preload.wait_all();
        e.slide()
            .shapes
            .iter()
            .filter_map(|shape| match &shape.kind {
                ShapeKind::Picture(p) => Some((e.presentation().media(p.media).map(|m| m.format).unwrap(), e.resources.has_picture(p.media))),
                _ => None,
            })
            .collect()
    })
}

/// The EMF of the issue (a freeform copied from Office: EMF+ records only) is drawn, not a grey "image (unsupported)" box.
fn emf_picture_is_drawn() {
    let mut s = Scenario::open("test/editing/xlsx-emf-picture.xlsx");
    s.size(1200., 700.);
    assert_eq!(pictures(&mut s), vec![(MediaFormat::Emf, true)]);
    s.screenshot("emf-picture");
}

fn paste_image(s: &mut Scenario, format: ImageFormat, bytes: Vec<u8>) {
    s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(format, bytes))));
    s.click_cell("B8");
    s.key("cmd-v");
}

/// An image on the clipboard is named by what its bytes are: an EMF is kept as an EMF and drawn, a TIFF (what the Mac
/// pasteboard holds for a copied Office drawing) becomes a PNG, an SVG stays vector; none is "image (unsupported)".
fn paste_drawings_and_bitmaps() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 700.);
    // No Office on this pasteboard: the OS-level formats are not read.
    s.with_app(|_| crate::native_shape_clipboard::simulate(None));
    let before = pictures(&mut s).len();

    paste_image(&mut s, ImageFormat::Png, EMF.to_vec());
    assert_eq!(pictures(&mut s)[before..], [(MediaFormat::Emf, true)], "an EMF is pasted and drawn");

    let mut tiff = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(6, 4, image::Rgb([200, 30, 30]))).write_to(&mut tiff, image::ImageFormat::Tiff).unwrap();
    paste_image(&mut s, ImageFormat::Tiff, tiff.into_inner());
    assert_eq!(pictures(&mut s)[before + 1..], [(MediaFormat::Png, true)], "a TIFF is pasted as a PNG");

    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="40" viewBox="0 0 80 40"><path d="M5 35 L40 5 L75 35 Z" fill="#2060c0"/></svg>"##;
    paste_image(&mut s, ImageFormat::Svg, svg.to_vec());
    assert_eq!(pictures(&mut s)[before + 2..], [(MediaFormat::Svg, true)], "an SVG is pasted as a vector picture");
    s.screenshot("pasted-drawings");

    // Saved, and read back as the same pictures.
    s.save_as("issue83_pasted.xlsx");
    let saved = s.reopen();
    let formats: Vec<MediaFormat> = saved.slides[0]
        .shapes
        .iter()
        .filter_map(|shape| match &shape.kind {
            ShapeKind::Picture(p) => saved.media(p.media).map(|m| m.format),
            _ => None,
        })
        .collect();
    assert!(formats.ends_with(&[MediaFormat::Emf, MediaFormat::Png, MediaFormat::Svg]), "{formats:?}");
}

/// Copy as picture: the clipboard gets a PNG, and the range is an SVG (text as text, borders, fills) that
/// paste as picture places as a vector picture.
fn copy_range_as_vector_picture() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 700.);
    s.with_app(|_| crate::native_shape_clipboard::simulate(None));
    let svg = s.with(|e, _| {
        let (_, sheet) = e.current_sheet().unwrap();
        crate::xlsx::copy_picture::sheet_range_svg(sheet, ibis_doc_model::CellRange { first_row: 0, last_row: 3, first_column: 0, last_column: 3 }).unwrap()
    });
    assert!(svg.starts_with("<svg") && svg.contains("<text") && svg.contains("font-family"), "{svg}");
    assert!(svg.contains("<line"), "the grid lines are drawn");
    let _ = std::fs::write(s.screenshot_path("range").with_extension("svg"), &svg);

    s.update(|e, _, _| {
        let slide = e.current_slide;
        e.set_cell(0, 0);
        e.wb_mut().cell_ends.insert(slide, (3, 8));
    });
    s.menu("xlsx.copy-as-picture");
    s.with(|e, cx| {
        let item = cx.read_from_clipboard().expect("clipboard");
        let image = item.entries().iter().find_map(|x| if let ClipboardEntry::Image(i) = x { Some(i.clone()) } else { None }).expect("image");
        assert!(image.bytes.starts_with(b"\x89PNG") || image.bytes.starts_with(b"<svg"));
        // A cell with no fill (A1, the title) is transparent, as Excel's copy as picture has it: not a white box.
        if image.bytes.starts_with(b"\x89PNG") {
            let png = image::load_from_memory(&image.bytes).unwrap().to_rgba8();
            assert_eq!(png.get_pixel(6, 6)[3], 0, "the corner of a cell with no fill is transparent");
        }
        let _ = e;
    });
    // The clipboard holds the bitmap, the SVG, and the PDF (what Office for Mac pastes as a vector picture).
    let (written_svg, pdf) = crate::native_shape_clipboard::written_vector().expect("the SVG went on the clipboard");
    assert!(written_svg.starts_with("<svg"));
    let pdf = pdf.expect("the PDF went on the clipboard");
    assert!(pdf.starts_with(b"%PDF"), "a PDF");
    let _ = std::fs::write(s.screenshot_path("range").with_extension("pdf"), &pdf);
    let drawn = crate::pdf_picture::pdf_to_svg(&pdf).expect("the PDF reads back");
    assert!(drawn.contains("<path") && drawn.contains("fill=\"#"), "the PDF holds the cells' shapes as paths: {drawn}");
    let before = pictures(&mut s).len();
    s.click_cell("G3");
    s.update(|e, _, cx| e.paste_as_picture(false, cx));
    assert_eq!(pictures(&mut s)[before..], [(MediaFormat::Svg, true)], "pasted as a vector picture");
    s.screenshot("range-as-picture");
    s.save_as("issue83_range_picture.xlsx");
    let saved = s.reopen();
    assert!(saved.media.iter().any(|m| m.format == MediaFormat::Svg), "the SVG picture is saved");
}

/// A vector picture on the pasteboard (what Office for Mac puts there for a copied drawing: an SVG and a PDF beside a bitmap) is
/// pasted as a vector picture, not as the bitmap beside it.
fn paste_vector_from_the_pasteboard() {
    let svg = include_bytes!("../../../../../test/editing/mac-office-shape.svg");
    let pdf = include_bytes!("../../../../../test/editing/mac-office-shape.pdf");
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 700.);
    s.with_app(|_| crate::native_shape_clipboard::simulate(None));
    let before = pictures(&mut s).len();
    let png = {
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 6, image::Rgb([20, 90, 130]))).write_to(&mut out, image::ImageFormat::Png).unwrap();
        out.into_inner()
    };
    let mut sources = vec![("svg", svg.to_vec(), "svg"), ("pdf", pdf.to_vec(), "pdf")];
    // The PDF Excel for Mac puts there for copied cells (kept out of the repository: test/github-issue).
    for (file, name) in [("excel-mac-copy.pdf", "excel-cell-copy-pdf"), ("copyAsFigure/0-com.adobe.pdf.bin", "excel-copy-as-figure-pdf")] {
        if let Ok(excel) = std::fs::read(format!("{}/../../test/github-issue/83/{file}", env!("CARGO_MANIFEST_DIR"))) {
            sources.push(("pdf", excel, name));
        }
    }
    let count = sources.len();
    for (kind, bytes, expected) in sources {
        s.with_app(|_| crate::native_shape_clipboard::simulate_vector(Some((kind, bytes.clone()))));
        s.update(|_, _, cx| cx.write_to_clipboard(ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, png.clone()))));
        s.click_cell("B8");
        s.key("cmd-v");
        let all = pictures(&mut s);
        assert_eq!(all.last().copied(), Some((MediaFormat::Svg, true)), "{expected}: pasted as a vector picture, not the PNG beside it: {all:?}");
        s.screenshot(&format!("pasted-pasteboard-{expected}"));
        // Zoomed in, the picture stays sharp (it is drawn again at the size it is shown, not stretched from a bitmap).
        if expected.starts_with("excel") {
            s.update(|e, window, cx| {
                e.set_zoom(Some(4.0), window, cx);
                e.set_cell(1, 7);
                e.scroll_range_into_view(true);
            });
            s.screenshot(&format!("pasted-pasteboard-{expected}-zoom400"));
            s.update(|e, window, cx| e.set_zoom(Some(1.0), window, cx));
        }
    }
    assert_eq!(pictures(&mut s).len(), before + count);
    s.with_app(|_| crate::native_shape_clipboard::simulate_vector(None));
}
