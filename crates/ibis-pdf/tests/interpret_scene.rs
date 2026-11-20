// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/235)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

#![cfg(feature = "interpret")]

use ibis_pdf::PdfContainer;
use ibis_pdf::interpret::interpret_page_scene;
use ibis_pdf::scene::{DisplayCmd, FillRule};

fn build_vector_and_text_pdf() -> Vec<u8> {
    use std::io::Write as _;
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n");

    let mut offsets = Vec::new();
    offsets.push((1, out.len()));
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    offsets.push((2, out.len()));
    out.extend_from_slice(
        b"2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] /MediaBox [0 0 595 842] >>\nendobj\n",
    );
    offsets.push((3, out.len()));
    out.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n",
    );

    let stream = b"0.2 0.4 0.8 rg 50 700 200 40 re f 0 0 0 rg BT /F1 18 Tf 72 720 Td (Hello ibisOffice) Tj ET";
    offsets.push((4, out.len()));
    let _ = writeln!(out, "4 0 obj\n<< /Length {} >>\nstream", stream.len());
    out.extend_from_slice(stream);
    out.extend_from_slice(b"\nendstream\nendobj\n");

    offsets.push((5, out.len()));
    out.extend_from_slice(
        b"5 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
    );

    let xref_offset = out.len();
    out.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for &(_, off) in &offsets {
        let _ = writeln!(out, "{off:010} 00000 n ");
    }
    let _ = writeln!(
        out,
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF"
    );
    out
}

#[test]
fn interprets_paths_and_extracts_text_coordinates() {
    let pdf = build_vector_and_text_pdf();
    let mut container = PdfContainer::from_bytes(pdf).unwrap();
    let scene = interpret_page_scene(&mut container, 0).expect("interpret page 0");

    assert_eq!(scene.text_index.plain_text(), "Hello ibisOffice");

    let has_rect_path = scene.display_list.iter().any(|cmd| {
        matches!(
            cmd,
            DisplayCmd::DrawPath {
                fill_rgba: Some([51, 102, 204, 255]),
                fill_rule: FillRule::NonZero,
                commands,
                ..
            } if commands.len() >= 4
        )
    });
    assert!(has_rect_path, "expected filled blue rectangle with path commands in display_list");

    // 1 filled rectangle + visible glyph outlines for "Hello ibisOffice" (15 non-space glyphs)
    let path_count = scene
        .display_list
        .iter()
        .filter(|cmd| matches!(cmd, DisplayCmd::DrawPath { .. }))
        .count();
    assert!(
        path_count >= 15,
        "expected rectangle plus glyph outline paths, got {path_count}"
    );
}

#[test]
fn decodes_raster_image_xobject_to_rgba() {
    use std::io::Write as _;
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.7\n");
    let mut offsets = Vec::new();
    offsets.push((1, out.len()));
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    offsets.push((2, out.len()));
    out.extend_from_slice(
        b"2 0 obj\n<< /Type /Pages /Count 1 /Kids [3 0 R] /MediaBox [0 0 200 200] >>\nendobj\n",
    );
    offsets.push((3, out.len()));
    out.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>\nendobj\n",
    );
    let stream = b"q 100 0 0 100 50 50 cm /Im1 Do Q";
    offsets.push((4, out.len()));
    let _ = writeln!(out, "4 0 obj\n<< /Length {} >>\nstream", stream.len());
    out.extend_from_slice(stream);
    out.extend_from_slice(b"\nendstream\nendobj\n");

    // 2x1 RGB image: red pixel (255,0,0), green pixel (0,255,0)
    let img_bytes = [255u8, 0, 0, 0, 255, 0];
    offsets.push((5, out.len()));
    let _ = writeln!(
        out,
        "5 0 obj\n<< /Type /XObject /Subtype /Image /Width 2 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length {} >>\nstream",
        img_bytes.len()
    );
    out.extend_from_slice(&img_bytes);
    out.extend_from_slice(b"\nendstream\nendobj\n");

    let xref_offset = out.len();
    out.extend_from_slice(b"xref\n0 6\n0000000000 65535 f \n");
    for &(_, off) in &offsets {
        let _ = writeln!(out, "{off:010} 00000 n ");
    }
    let _ = writeln!(
        out,
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF"
    );

    let mut container = PdfContainer::from_bytes(out).unwrap();
    let scene = interpret_page_scene(&mut container, 0).unwrap();
    let img_cmd = scene.display_list.iter().find_map(|cmd| match cmd {
        DisplayCmd::DrawImage {
            width,
            height,
            rgba,
            ..
        } => Some((*width, *height, rgba.clone())),
        _ => None,
    });
    assert_eq!(
        img_cmd,
        Some((2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]))
    );
}
