// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Compare many small tests laid out on pages (the coverage tests,
//! test/coverage/): for each test, cut its rectangle out of the reference
//! picture and out of the candidate picture and compare the two.
//!
//!   cargo run --release -p ibis-imgdiff --example crop_compare -- <jobs.tsv> <results.tsv> [<crops-dir>]
//!
//! Each line of jobs.tsv: `id  reference.png  candidate.png  fx0 fy0 fx1 fy1  rx0 ry0 rx1 ry1 [page]`.
//! With `page` the frame is the whole page (fx0 fy0 = 0 0, fx1 fy1 = its size): for tests
//! of a whole page (page borders, headers, floating pictures) that have no marks.
//! The frame (f…) is the rectangle the page's magenta registration marks span,
//! the test's rectangle (r…) is in the same units. In each picture the marks
//! are found (the bounding box of its magenta pixels, to a fraction of a
//! pixel from the coverage of their anti-aliased edges) and the rectangle is
//! mapped linearly from the frame to them, so the two pictures may differ in
//! scale and margins. The candidate's cut is resized to the reference's.
//!
//! Pictures of the same page drawn by two programs land up to a pixel apart
//! (rounding of cell edges, the marks' own edges): a pixel counts as
//! differing only when no pixel within `--shift` pixels (default 1) of it in
//! the other picture is close in colour, checked both ways, before
//! ibis_imgdiff::compare groups the differences. `strict_ratio` is the
//! ratio without that allowance.
//!
//! `--mask <masks.tsv>` (an auxiliary comparison, not the pass criterion):
//! each line `reference.png  x0 y0 x1 y1` is a rectangle in that reference
//! picture's pixels (the text, from the PDFs' word boxes) painted white in
//! both cuts before comparing, so only shapes, borders and fills are compared.
//!
//! `crop_compare --marks <png>...` prints each picture's size and marks:
//! `path  width  height  x0  y0  x1  y1`.
//!
//! results.tsv: `id  status  differing_ratio  regions  max_delta_e  strict_ratio` with
//! status `match`, `differ`, `no-mark-reference`, `no-mark-candidate`,
//! `missing-reference`, `missing-candidate`. With a crops dir, each test that
//! differs gets `<id>.png`: reference | candidate | differences side by side.

use std::collections::HashMap;
use std::io::Write;

use ibis_imgdiff::{Options, compare, outline, resize_to};
use image::{Rgba, RgbaImage};

struct Page {
    image: RgbaImage,
    /// The marks' bounding box (x0, y0, x1, y1), exclusive ends, to a
    /// fraction of a pixel (`find_marks`).
    marks: Option<(f64, f64, f64, f64)>,
    /// The same in whole pixels: the clearly magenta ones.
    coarse: Option<(f64, f64, f64, f64)>,
    /// Whether the marks reach the picture's edge (a sheet printed from its
    /// marks' cells, with no margin), where their edges' coverage cannot
    /// be seen.
    at_border: bool,
}

fn is_mark(p: &Rgba<u8>) -> bool {
    p[3] > 128 && p[0] > 200 && p[1] < 70 && p[2] > 200
}

/// How much of a pixel a magenta mark covers, from its colour (magenta
/// over the page's white: the green that is missing), 0 to 1.
fn mark_coverage(p: &Rgba<u8>) -> f64 {
    if p[3] <= 128 || p[0] < 160 || p[2] < 160 {
        return 0.0;
    }
    let lack = p[0].min(p[2]) as f64 - p[1] as f64;
    (lack / 255.0).clamp(0.0, 1.0)
}

/// The marks' bounding box, to a fraction of a pixel: the pixels that are
/// clearly magenta (`is_mark`) give its whole pixels, and the partly
/// covered pixels just outside them (a mark's anti-aliased edge, drawn
/// differently by each program) add the fraction they cover, so a mark
/// whose edge falls inside a pixel is found where it is in both pictures.
/// The marks' bounding box in whole pixels: the clearly magenta ones.
fn coarse_marks(image: &RgbaImage) -> Option<(f64, f64, f64, f64)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in image.enumerate_pixels() {
        if is_mark(p) {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    (x1 > x0.saturating_add(4) && y1 > y0.saturating_add(4)).then_some((x0 as f64, y0 as f64, x1 as f64, y1 as f64))
}

fn find_marks(image: &RgbaImage) -> Option<(f64, f64, f64, f64)> {
    let (w, h) = image.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in image.enumerate_pixels() {
        if is_mark(p) {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    if !(x1 > x0.saturating_add(4) && y1 > y0.saturating_add(4)) {
        return None;
    }
    // The most a pixel of column (or row) `i` is covered, among the rows
    // (or columns) the marks span.
    let column = |x: u32| (y0..y1).map(|y| mark_coverage(image.get_pixel(x, y))).fold(0.0, f64::max);
    let row = |y: u32| (x0..x1).map(|x| mark_coverage(image.get_pixel(x, y))).fold(0.0, f64::max);
    // A side's edge: in the first clearly magenta pixel by the share it is
    // not covered, and out into the pixel beyond by the share that is.
    let within = |i: u32, along: &dyn Fn(u32) -> f64| 1.0 - along(i);
    let beyond = |o: Option<u32>, along: &dyn Fn(u32) -> f64| o.map_or(0.0, along);
    let left = x0 as f64 + within(x0, &column) - beyond(x0.checked_sub(1), &column);
    let right = x1 as f64 - within(x1 - 1, &column) + beyond((x1 < w).then_some(x1), &column);
    let top = y0 as f64 + within(y0, &row) - beyond(y0.checked_sub(1), &row);
    let bottom = y1 as f64 - within(y1 - 1, &row) + beyond((y1 < h).then_some(y1), &row);
    Some((left, top, right, bottom))
}

fn load(path: &str) -> Option<Page> {
    let image = image::open(path).ok()?.to_rgba8();
    let marks = find_marks(&image);
    let coarse = coarse_marks(&image);
    let (w, h) = (image.width() as f64, image.height() as f64);
    let at_border = coarse.is_some_and(|(x0, y0, x1, y1)| x0 <= 0.0 || y0 <= 0.0 || x1 >= w || y1 >= h);
    Some(Page { image, marks, coarse, at_border })
}

fn cut(page: &Page, frame: [f64; 4], rect: [f64; 4], whole: bool, fine: bool) -> Option<RgbaImage> {
    cut_at(page, frame, rect, whole, fine).map(|(img, _, _)| img)
}

/// The cut and its top-left corner in the page.
/// `fine`: from the marks to a fraction of a pixel, from their middle (both
/// pictures show their marks' edges); else from the whole magenta pixels'
/// corner (one picture's marks reach its edge, so the other's are taken
/// as coarsely: a sheet printed with no margin against Excel's PDF).
fn cut_at(page: &Page, frame: [f64; 4], rect: [f64; 4], whole: bool, fine: bool) -> Option<(RgbaImage, u32, u32)> {
    // A whole-page test: the frame is the page itself (no marks needed).
    let (mx0, my0, mx1, my1) = if whole {
        (0.0, 0.0, page.image.width() as f64, page.image.height() as f64)
    } else if fine {
        page.marks?
    } else {
        page.coarse?
    };
    let sx = (mx1 - mx0) / (frame[2] - frame[0]);
    let sy = (my1 - my0) / (frame[3] - frame[1]);
    // From the marks' middle, which does not move with how a program draws
    // their edges (a PDF rasterizer puts a rectangle's edges on whole
    // pixels, outward; GPUI smooths them), out by the scale.
    let (cx, cy) = if fine { ((mx0 + mx1) / 2.0, (my0 + my1) / 2.0) } else { (mx0, my0) };
    let (fcx, fcy) = if fine { ((frame[0] + frame[2]) / 2.0, (frame[1] + frame[3]) / 2.0) } else { (frame[0], frame[1]) };
    let map_x = |v: f64| cx + (v - fcx) * sx;
    let map_y = |v: f64| cy + (v - fcy) * sy;
    let x0 = map_x(rect[0]).round().max(0.0) as u32;
    let y0 = map_y(rect[1]).round().max(0.0) as u32;
    let x1 = (map_x(rect[2]).round() as u32).min(page.image.width());
    let y1 = (map_y(rect[3]).round() as u32).min(page.image.height());
    if x1 <= x0 + 1 || y1 <= y0 + 1 {
        return None;
    }
    Some((image::imageops::crop_imm(&page.image, x0, y0, x1 - x0, y1 - y0).to_image(), x0, y0))
}

/// Paints the mask rectangles (page pixels) white in a cut whose corner is at (ox, oy).
fn paint_mask(img: &mut RgbaImage, rects: &[[u32; 4]], ox: u32, oy: u32) {
    let (w, h) = img.dimensions();
    for r in rects {
        let x0 = r[0].saturating_sub(ox).min(w);
        let y0 = r[1].saturating_sub(oy).min(h);
        let x1 = r[2].saturating_sub(ox).min(w);
        let y1 = r[3].saturating_sub(oy).min(h);
        for y in y0..y1 {
            for x in x0..x1 {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }
    }
}

/// sRGB over white to CIE L*a*b* (the same formula as ibis_imgdiff).
fn lab(p: &Rgba<u8>) -> [f32; 3] {
    let a = p[3] as f32 / 255.0;
    let lin = |c: u8| {
        let c = (c as f32 / 255.0) * a + (1.0 - a);
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (lin(p[0]), lin(p[1]), lin(p[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f32| if t > 0.008_856 { t.cbrt() } else { 7.787 * t + 16.0 / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

fn delta_e(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Edge (anti-aliasing) allowance, decided pixel by pixel: `b` with every
/// pixel that has, within `r` pixels in `b`, a pixel of a colour close to
/// `a`'s pixel there (ΔE ≤ `limit`), or two pixels between whose colours it
/// lies (an edge whose coverage the two programs split differently), set to
/// `a`'s pixel, so only pixels with no such colour nearby are left to differ.
fn allow_edges(a: &RgbaImage, b: &RgbaImage, r: i64, limit: f32) -> RgbaImage {
    let (w, h) = a.dimensions();
    let la: Vec<[f32; 3]> = a.pixels().map(lab).collect();
    let lb: Vec<[f32; 3]> = b.pixels().map(lab).collect();
    let mut out = b.clone();
    for y in 0..h as i64 {
        for x in 0..w as i64 {
            let want = la[(y * w as i64 + x) as usize];
            let mut near: Vec<[f32; 3]> = Vec::with_capacity(9);
            for dy in -r..=r {
                for dx in -r..=r {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64 {
                        near.push(lb[(ny * w as i64 + nx) as usize]);
                    }
                }
            }
            // A close colour nearby, or (an edge drawn with its coverage
            // split differently) a close colour on the way between two
            // nearby colours.
            let close = near.iter().any(|q| delta_e(want, *q) <= limit)
                || near.iter().enumerate().any(|(i, p)| {
                    near[i + 1..].iter().any(|q| {
                        (1..10).any(|k| {
                            let t = k as f32 / 10.0;
                            let m = [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t, p[2] + (q[2] - p[2]) * t];
                            delta_e(want, m) <= limit
                        })
                    })
                });
            if close {
                out.put_pixel(x as u32, y as u32, *a.get_pixel(x as u32, y as u32));
            }
        }
    }
    out
}

/// The tolerant comparison: differences left after the edge allowance,
/// both ways; the report with more differing pixels.
fn tolerant(a: &RgbaImage, b: &RgbaImage, r: i64, options: &Options) -> Option<ibis_imgdiff::Report> {
    let one = compare(a, &allow_edges(a, b, r, options.threshold), options).ok()?;
    let two = compare(b, &allow_edges(b, a, r, options.threshold), options).ok()?;
    Some(if one.differing_pixels >= two.differing_pixels { one } else { two })
}

/// Checks that the tolerant comparison still fails real differences: a line
/// moved 2 px, a thicker line, a line of another colour; and passes a line
/// drawn with different anti-aliasing.
fn self_test() {
    let options = Options::default();
    let line = |y0: f32, thick: f32, color: [u8; 3]| {
        let mut img = RgbaImage::from_pixel(80, 40, Rgba([255, 255, 255, 255]));
        for x in 5..75u32 {
            for y in 0..40u32 {
                // Coverage of the pixel row by the band [y0, y0 + thick).
                let cov = ((y as f32 + 1.0).min(y0 + thick) - (y as f32).max(y0)).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let mix = |c: u8| (255.0 - (255.0 - c as f32) * cov).round() as u8;
                    img.put_pixel(x, y, Rgba([mix(color[0]), mix(color[1]), mix(color[2]), 255]));
                }
            }
        }
        img
    };
    let base = line(18.0, 2.0, [0, 0, 0]);
    let cases = [
        ("same", line(18.0, 2.0, [0, 0, 0]), true),
        ("anti-aliased 0.5 px lower", line(18.5, 2.0, [0, 0, 0]), true),
        ("moved 2 px", line(20.0, 2.0, [0, 0, 0]), false),
        ("thicker (4 px)", line(18.0, 4.0, [0, 0, 0]), false),
        ("other colour", line(18.0, 2.0, [200, 0, 0]), false),
    ];
    let mut ok = true;
    for (name, img, want) in cases {
        let got = tolerant(&base, &img, 1, &options).map(|r| r.regions.is_empty()).unwrap_or(false);
        println!("{name}: {} (expected {})", if got { "match" } else { "differ" }, if want { "match" } else { "differ" });
        ok &= got == want;
    }
    // Marks found to a fraction of a pixel: a magenta square from (x, y)
    // to (x + 10, y + 10), its edges anti-aliased.
    let marked = |x: f32, y: f32| {
        let mut img = RgbaImage::from_pixel(60, 60, Rgba([255, 255, 255, 255]));
        for py in 0..60u32 {
            for px in 0..60u32 {
                let cover = |p: u32, a: f32| ((p as f32 + 1.0).min(a + 10.0) - (p as f32).max(a)).clamp(0.0, 1.0);
                let c = cover(px, x) * cover(py, y);
                if c > 0.0 {
                    img.put_pixel(px, py, Rgba([255, (255.0 * (1.0 - c)).round() as u8, 255, 255]));
                }
            }
        }
        img
    };
    for (x, y) in [(18.0, 18.0), (18.45, 18.45), (18.8, 19.3)] {
        let found = find_marks(&marked(x, y));
        let near = found.is_some_and(|(x0, y0, x1, y1)| {
            (x0 - x as f64).abs() < 0.05 && (y0 - y as f64).abs() < 0.05 && (x1 - x as f64 - 10.0).abs() < 0.05 && (y1 - y as f64 - 10.0).abs() < 0.05
        });
        println!("marks from ({x}, {y}): {found:?} ({})", if near { "right" } else { "wrong" });
        ok &= near;
    }
    if !ok {
        eprintln!("self test FAILED");
        std::process::exit(1);
    }
    println!("self test passed");
}

fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--self-test") {
        self_test();
        return;
    }
    if args.get(1).map(String::as_str) == Some("--marks") {
        for p in &args[2..] {
            match load(p) {
                Some(Page { image, marks: Some((x0, y0, x1, y1)), .. }) => println!("{p}\t{}\t{}\t{x0}\t{y0}\t{x1}\t{y1}", image.width(), image.height()),
                Some(Page { image, marks: None, .. }) => println!("{p}\t{}\t{}\t\t\t\t", image.width(), image.height()),
                None => println!("{p}\t\t\t\t\t\t"),
            }
        }
        return;
    }
    let mut shift = 1i64;
    if let Some(i) = args.iter().position(|a| a == "--shift") {
        shift = args[i + 1].parse().expect("--shift pixels");
        args.drain(i..i + 2);
    }
    let mut masks: HashMap<String, Vec<[u32; 4]>> = HashMap::new();
    if let Some(i) = args.iter().position(|a| a == "--mask") {
        for l in std::fs::read_to_string(&args[i + 1]).expect("readable masks").lines() {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() == 5 {
                let n = |k: usize| f[k].parse::<f64>().unwrap_or(0.0).max(0.0).round() as u32;
                masks.entry(f[0].to_string()).or_default().push([n(1), n(2), n(3), n(4)]);
            }
        }
        args.drain(i..i + 2);
    }
    if args.len() < 3 {
        eprintln!("usage: crop_compare <jobs.tsv> <results.tsv> [<crops-dir>]");
        std::process::exit(2);
    }
    let jobs = std::fs::read_to_string(&args[1]).expect("readable jobs");
    let crops = args.get(3).cloned();
    if let Some(dir) = &crops {
        std::fs::create_dir_all(dir).expect("crops dir");
    }
    let mut out = std::fs::File::create(&args[2]).expect("results file");
    writeln!(out, "id\tstatus\tdiffering_ratio\tregions\tmax_delta_e\tstrict_ratio\tstrict_status").unwrap();
    let mut pages: HashMap<String, Option<Page>> = HashMap::new();
    let options = Options::default();
    let (mut matched, mut total) = (0, 0);
    for line in jobs.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#')) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 11 {
            eprintln!("bad line: {line}");
            continue;
        }
        let num = |i: usize| f[i].parse::<f64>().unwrap_or(0.0);
        let (id, refp, candp) = (f[0], f[1], f[2]);
        let frame = [num(3), num(4), num(5), num(6)];
        let rect = [num(7), num(8), num(9), num(10)];
        let whole = f.get(11).is_some_and(|x| *x == "page");
        total += 1;
        for p in [refp, candp] {
            if !pages.contains_key(p) {
                let page = load(p);
                if std::env::var_os("CROP_DEBUG").is_some() {
                    eprintln!("{p}: size {:?} marks {:?}", page.as_ref().map(|x| x.image.dimensions()), page.as_ref().and_then(|x| x.marks));
                }
                pages.insert(p.to_string(), page);
            }
        }
        let row = |status: &str, ratio: f64, regions: usize, max: f32| format!("{id}\t{status}\t{ratio:.5}\t{regions}\t{max:.1}");
        let row2 = |status: &str, ratio: f64, regions: usize, max: f32, strict: f64, strict_ok: bool| format!("{id}\t{status}\t{ratio:.5}\t{regions}\t{max:.1}\t{strict:.5}\t{}", if strict_ok { "match" } else { "differ" });
        let (Some(rp), Some(cp)) = (&pages[refp], &pages[candp]) else {
            let status = if pages[refp].is_none() { "missing-reference" } else { "missing-candidate" };
            writeln!(out, "{}", row(status, 1.0, 0, 0.0)).unwrap();
            continue;
        };
        let fine = !rp.at_border && !cp.at_border;
        let Some((mut a, ox, oy)) = cut_at(rp, frame, rect, whole, fine) else {
            writeln!(out, "{}", row("no-mark-reference", 1.0, 0, 0.0)).unwrap();
            continue;
        };
        let Some(b) = cut(cp, frame, rect, whole, fine) else {
            writeln!(out, "{}", row("no-mark-candidate", 1.0, 0, 0.0)).unwrap();
            continue;
        };
        let mut b = if b.dimensions() == a.dimensions() { b } else { resize_to(&b, a.width(), a.height()) };
        if let Some(rects) = masks.get(refp) {
            // A line the mask's edge cuts shows a sliver of different width
            // when the two programs put it a pixel apart: the 2 px band
            // around each rectangle is taken from the reference.
            let (w, h) = a.dimensions();
            for r in rects {
                let x0 = r[0].saturating_sub(ox + 2).min(w);
                let y0 = r[1].saturating_sub(oy + 2).min(h);
                let x1 = (r[2] + 2).saturating_sub(ox).min(w);
                let y1 = (r[3] + 2).saturating_sub(oy).min(h);
                for y in y0..y1 {
                    for x in x0..x1 {
                        b.put_pixel(x, y, *a.get_pixel(x, y));
                    }
                }
            }
            paint_mask(&mut a, rects, ox, oy);
            paint_mask(&mut b, rects, ox, oy);
        }
        let strict_report = compare(&a, &b, &options).ok();
        let strict = strict_report.as_ref().map(|r| r.differing_ratio()).unwrap_or(1.0);
        let strict_ok = strict_report.as_ref().is_some_and(|r| r.regions.is_empty());
        let Some(report) = tolerant(&a, &b, shift, &options) else {
            eprintln!("{id}: cannot compare");
            continue;
        };
        let max = report.regions.iter().map(|r| r.max_delta_e).fold(0.0f32, f32::max);
        let ok = report.regions.is_empty();
        if ok {
            matched += 1;
        } else if let Some(dir) = &crops {
            let (w, h) = a.dimensions();
            let mut side = RgbaImage::from_pixel(w * 3 + 8, h, Rgba([255, 255, 255, 255]));
            image::imageops::overlay(&mut side, &a, 0, 0);
            image::imageops::overlay(&mut side, &b, (w + 4) as i64, 0);
            image::imageops::overlay(&mut side, &outline(&a, &report.regions), (2 * w + 8) as i64, 0);
            side.save(format!("{dir}/{id}.png")).ok();
        }
        writeln!(out, "{}", row2(if ok { "match" } else { "differ" }, report.differing_ratio(), report.regions.len(), max, strict, strict_ok)).unwrap();
    }
    eprintln!("{matched}/{total} match");
}
