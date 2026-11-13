// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! EMF+ records ([MS-EMFPLUS]) played into SVG. The metafiles are built here from fictitious shapes.

use ibis_emf::{ImageFormat, Picture, emf_picture, emf_to_svg};

fn f(v: f32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn u(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

/// One EMF+ record: type, flags, data (padded to four bytes).
fn rec(kind: u16, flags: u16, data: &[u8]) -> Vec<u8> {
    let mut data = data.to_vec();
    while data.len() % 4 != 0 {
        data.push(0);
    }
    let mut v = kind.to_le_bytes().to_vec();
    v.extend(flags.to_le_bytes());
    v.extend(((data.len() + 12) as u32).to_le_bytes());
    v.extend((data.len() as u32).to_le_bytes());
    v.extend(data);
    v
}

fn header(dual: bool) -> Vec<u8> {
    let mut d = u(0xDBC0_1002);
    d.extend(u(0));
    d.extend(u(96));
    d.extend(u(96));
    rec(0x4001, u16::from(dual), &d)
}

fn gdi_rect_record(l: i32, t: i32, r: i32, b: i32) -> Vec<u8> {
    let mut v = 0x2Bu32.to_le_bytes().to_vec();
    v.extend(24u32.to_le_bytes());
    for x in [l, t, r, b] {
        v.extend(x.to_le_bytes());
    }
    v
}

/// An Enhanced Metafile of 100 by 100 pixels holding the EMF+ records in one comment, then `gdi` records.
fn emf(plus: &[Vec<u8>], gdi: &[Vec<u8>]) -> Vec<u8> {
    let mut h = vec![0u8; 88];
    h[0..4].copy_from_slice(&1u32.to_le_bytes());
    h[4..8].copy_from_slice(&88u32.to_le_bytes());
    for (i, v) in [0i32, 0, 99, 99].iter().enumerate() {
        h[8 + i * 4..12 + i * 4].copy_from_slice(&v.to_le_bytes());
    }
    h[40..44].copy_from_slice(b" EMF");
    for (at, v) in [(72, 100i32), (76, 100), (80, 26), (84, 26)] {
        h[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    let body: Vec<u8> = plus.concat();
    let mut out = h;
    out.extend(70u32.to_le_bytes());
    out.extend(((16 + body.len()) as u32).to_le_bytes());
    out.extend(((4 + body.len()) as u32).to_le_bytes());
    out.extend(b"EMF+");
    out.extend(body);
    for g in gdi {
        out.extend(g);
    }
    out.extend(14u32.to_le_bytes());
    out.extend(20u32.to_le_bytes());
    out.extend([0u8; 12]);
    out
}

/// An object record: type 1 brush, 2 pen, 3 path, 5 image, 6 font, 7 string format.
fn object(kind: u16, id: u16, data: &[u8]) -> Vec<u8> {
    rec(0x4008, (kind << 8) | id, data)
}

fn solid_brush(argb: u32) -> Vec<u8> {
    let mut d = u(0xDBC0_1002);
    d.extend(u(0));
    d.extend(u(argb));
    d
}

fn pen(width: f32, argb: u32) -> Vec<u8> {
    let mut d = u(0xDBC0_1002);
    d.extend(u(0));
    d.extend(u(0)); // flags
    d.extend(u(2)); // pixels
    d.extend(f(width));
    d.extend(solid_brush(argb));
    d
}

fn rectf(x: f32, y: f32, w: f32, h: f32) -> Vec<u8> {
    [f(x), f(y), f(w), f(h)].concat()
}

const RED: u32 = 0xFFFF_0000;
const BLUE: u32 = 0xFF00_00FF;

#[test]
fn fills_and_strokes_rectangles() {
    let mut fill = u(RED);
    fill.extend(u(1));
    fill.extend(rectf(10.0, 20.0, 30.0, 40.0));
    let mut draw = u(1);
    draw.extend(rectf(10.0, 20.0, 30.0, 40.0));
    let data = emf(
        &[header(false), object(1, 0, &solid_brush(RED)), object(2, 1, &pen(3.0, BLUE)), rec(0x400A, 0x8000, &fill), rec(0x400B, 1, &draw)],
        &[],
    );
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("fill=\"#FF0000\""), "{svg}");
    assert!(svg.contains("stroke=\"#0000FF\"") && svg.contains("stroke-width=\"3\""), "{svg}");
    assert!(svg.contains("M10 20"), "{svg}");
}

#[test]
fn a_brush_object_with_alpha_gives_opacity() {
    let mut fill = u(0);
    fill.extend(u(1));
    fill.extend(rectf(0.0, 0.0, 10.0, 10.0));
    let data = emf(&[header(false), object(1, 0, &solid_brush(0x80FF_0000)), rec(0x400A, 0, &fill)], &[]);
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("fill-opacity=\"0.5"), "{svg}");
}

#[test]
fn linear_gradients_become_gradient_fills() {
    let mut b = u(0xDBC0_1002);
    b.extend(u(4));
    b.extend(u(0)); // flags
    b.extend(u(4)); // wrap: clamp
    b.extend(rectf(0.0, 0.0, 100.0, 10.0));
    b.extend(u(RED));
    b.extend(u(BLUE));
    b.extend(u(0));
    b.extend(u(0));
    let mut fill = u(0);
    fill.extend(u(1));
    fill.extend(rectf(0.0, 0.0, 100.0, 50.0));
    let data = emf(&[header(false), object(1, 0, &b), rec(0x400A, 0, &fill)], &[]);
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("<linearGradient") && svg.contains("stop-color=\"#FF0000\"") && svg.contains("stop-color=\"#0000FF\""), "{svg}");
    assert!(svg.contains("fill=\"url(#p1)\""), "{svg}");
}

#[test]
fn paths_are_filled_and_the_world_transform_moves_them() {
    // A triangle: start, line, line closed.
    let mut p = u(0xDBC0_1002);
    p.extend(u(3));
    p.extend(u(0));
    for (x, y) in [(0.0f32, 0.0f32), (20.0, 0.0), (10.0, 20.0)] {
        p.extend(f(x));
        p.extend(f(y));
    }
    p.extend([0u8, 1, 0x81]);
    let fill = u(RED);
    let mut translate = f(30.0);
    translate.extend(f(40.0));
    let data = emf(&[header(false), object(3, 0, &p), rec(0x402D, 0, &translate), rec(0x4014, 0x8000, &fill)], &[]);
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("M30 40") && svg.contains("L50 40") && svg.contains("L40 60"), "{svg}");
    assert!(svg.contains('Z'), "{svg}");
}

#[test]
fn page_scale_and_units_apply() {
    // Millimetres at 96 dpi: 1 mm = 3.78 px.
    let mut fill = u(RED);
    fill.extend(u(1));
    fill.extend(rectf(0.0, 0.0, 10.0, 10.0));
    let page = f(1.0);
    let data = emf(&[header(false), rec(0x4030, 6, &page), rec(0x400A, 0x8000, &fill)], &[]);
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("L37.8 0"), "{svg}");
}

#[test]
fn save_and_restore_bring_back_the_transform() {
    let mut fill = u(RED);
    fill.extend(u(1));
    fill.extend(rectf(0.0, 0.0, 5.0, 5.0));
    let mut translate = f(50.0);
    translate.extend(f(50.0));
    let data = emf(
        &[header(false), rec(0x4025, 0, &u(7)), rec(0x402D, 0, &translate), rec(0x4026, 0, &u(7)), rec(0x400A, 0x8000, &fill)],
        &[],
    );
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("M0 0") && !svg.contains("M50 50"), "{svg}");
}

#[test]
fn clip_rectangles_clip_the_drawing() {
    let mut fill = u(RED);
    fill.extend(u(1));
    fill.extend(rectf(0.0, 0.0, 100.0, 100.0));
    let data = emf(&[header(false), rec(0x4032, 0, &rectf(10.0, 10.0, 20.0, 20.0)), rec(0x400A, 0x8000, &fill)], &[]);
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("<clipPath") && svg.contains("clip-path=\"url(#c1)\""), "{svg}");
    assert!(svg.contains("M10 10 L30 10 L30 30 L10 30"), "{svg}");
}

#[test]
fn strings_use_the_font_the_colour_and_the_alignment() {
    let mut font = u(0xDBC0_1002);
    font.extend(f(12.0));
    font.extend(u(2));
    font.extend(u(1)); // bold
    font.extend(u(0));
    font.extend(u(5));
    for c in "Arial".encode_utf16() {
        font.extend(c.to_le_bytes());
    }
    let mut format = u(0xDBC0_1002);
    format.extend(u(0)); // flags
    format.extend(u(0)); // language
    format.extend(u(1)); // centred
    format.extend(u(0));
    format.extend([0u8; 40]);
    let mut text = u(0xFF00_8800);
    text.extend(u(2));
    text.extend(u(2));
    text.extend(rectf(0.0, 10.0, 100.0, 20.0));
    for c in "Hi".encode_utf16() {
        text.extend(c.to_le_bytes());
    }
    let data = emf(&[header(false), object(6, 0, &font), object(7, 2, &format), rec(0x401C, 0x8000, &text)], &[]);
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains(">Hi<") && svg.contains("font-family=\"'Arial") && svg.contains("font-weight=\"bold\""), "{svg}");
    assert!(svg.contains("fill=\"#008800\"") && svg.contains("text-anchor=\"middle\"") && svg.contains("x=\"50\""), "{svg}");
}

fn bitmap_object(w: u32, h: u32) -> Vec<u8> {
    let mut o = u(0xDBC0_1002);
    o.extend(u(1)); // bitmap
    o.extend(u(w));
    o.extend(u(h));
    o.extend(u(w * 4));
    o.extend(u(0x0026_200A)); // 32bpp ARGB
    o.extend(u(0));
    for i in 0..w * h {
        o.extend([0u8, 0, 255, 255 - (i as u8)]);
    }
    o
}

fn draw_image(src: [f32; 4], dst: [f32; 4]) -> Vec<u8> {
    let mut d = u(0);
    d.extend(u(2));
    d.extend(rectf(src[0], src[1], src[2], src[3]));
    d.extend(rectf(dst[0], dst[1], dst[2], dst[3]));
    d
}

#[test]
fn a_bitmap_drawn_into_a_rectangle() {
    let data = emf(&[header(false), object(5, 0, &bitmap_object(4, 2)), rec(0x401A, 0, &draw_image([0.0, 0.0, 4.0, 2.0], [10.0, 20.0, 40.0, 20.0]))], &[]);
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("<image") && svg.contains("data:image/png;base64,"), "{svg}");
    assert!(svg.contains("matrix(10 0 0 10 10 20)"), "{svg}");
}

#[test]
fn a_part_of_a_bitmap_is_cut_out() {
    let data = emf(&[header(false), object(5, 0, &bitmap_object(4, 2)), rec(0x401A, 0, &draw_image([1.0, 0.0, 2.0, 2.0], [0.0, 0.0, 20.0, 20.0]))], &[]);
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("<clipPath") && svg.contains("<g>"), "{svg}");
}

#[test]
fn a_metafile_of_one_whole_bitmap_is_the_bitmap() {
    let data = emf(&[header(false), rec(0x4009, 0, &u(0x00FF_FFFF)), object(5, 0, &bitmap_object(4, 2)), rec(0x401A, 0, &draw_image([0.0, 0.0, 4.0, 2.0], [0.0, 0.0, 4.0, 2.0]))], &[]);
    match emf_picture(&data) {
        Some(Picture::Image(ImageFormat::Png, bytes)) => assert_eq!(&bytes[..4], b"\x89PNG"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_dual_metafile_falls_back_to_its_gdi_records_when_an_object_is_missing() {
    // The image object is never defined: the EMF+ drawing cannot be completed.
    let data = emf(&[header(true), rec(0x401A, 0, &draw_image([0.0, 0.0, 4.0, 2.0], [0.0, 0.0, 4.0, 2.0]))], &[gdi_rect_record(5, 5, 50, 50)]);
    let svg = emf_to_svg(&data).expect("the GDI fallback");
    assert!(svg.contains("M5 5"), "{svg}");
}

#[test]
fn the_gdi_records_of_a_dual_metafile_are_not_drawn_twice() {
    let mut fill = u(RED);
    fill.extend(u(1));
    fill.extend(rectf(0.0, 0.0, 10.0, 10.0));
    let data = emf(&[header(true), rec(0x400A, 0x8000, &fill)], &[gdi_rect_record(5, 5, 50, 50)]);
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("M0 0") && !svg.contains("M5 5"), "{svg}");
}

#[test]
fn ellipses_and_pies_make_curves() {
    let mut fill = u(RED);
    fill.extend(rectf(0.0, 0.0, 40.0, 20.0));
    let mut pie = u(RED);
    pie.extend(f(0.0));
    pie.extend(f(90.0));
    pie.extend(rectf(0.0, 0.0, 40.0, 40.0));
    let data = emf(&[header(false), rec(0x400E, 0x8000, &fill), rec(0x4010, 0x8000, &pie)], &[]);
    let svg = emf_to_svg(&data).expect("drawn");
    assert!(svg.contains("C"), "{svg}");
    assert!(svg.contains("M40 20") || svg.contains("M20 20"), "{svg}");
}
