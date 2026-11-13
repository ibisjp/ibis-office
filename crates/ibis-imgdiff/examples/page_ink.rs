// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! How much of a page's ink lands in the same place in two pictures of it:
//! the reference (Office's PDF drawn to PNG) and the candidate (ibisOffice's,
//! drawn the same way at the same width). Measures the layout and the drawing
//! together, without reading the text: column widths, row heights, page
//! breaks, scale, alignment, borders, fills.
//!
//!   cargo run --release -p ibis-imgdiff --example page_ink -- <jobs.tsv> [--shift 1]
//!
//! Each line of jobs.tsv: `id  reference.png  candidate.png` (a missing
//! picture is an empty page). A pixel is ink when it is not near white
//! (its darkest channel below 200, after compositing over white). A
//! reference ink pixel is matched when the candidate has ink within
//! `--shift` pixels of it (a square), and the other way round. Prints per
//! line: `id  ref_ink  ref_matched  cand_ink  cand_matched  score`, where
//! score is the F-measure of the two matched fractions (1 = the same ink in
//! the same places). Pages of different sizes are compared over their
//! common top-left part; the rest counts as unmatched.

use image::RgbaImage;

fn ink(img: &RgbaImage) -> (u32, u32, Vec<bool>) {
    let (w, h) = img.dimensions();
    let mut v = vec![false; (w * h) as usize];
    for (x, y, p) in img.enumerate_pixels() {
        let a = p[3] as u32;
        let over = |c: u8| (c as u32 * a + 255 * (255 - a)) / 255;
        let m = over(p[0]).min(over(p[1])).min(over(p[2]));
        v[(y * w + x) as usize] = m < 200;
    }
    (w, h, v)
}

/// (ink pixels of a, those with ink in b within `shift`).
fn matched(a: &(u32, u32, Vec<bool>), b: &(u32, u32, Vec<bool>), shift: i64) -> (u64, u64) {
    let (aw, ah, av) = a;
    let (bw, bh, bv) = b;
    let (mut total, mut hit) = (0u64, 0u64);
    for y in 0..*ah as i64 {
        for x in 0..*aw as i64 {
            if !av[(y * *aw as i64 + x) as usize] {
                continue;
            }
            total += 1;
            'near: for dy in -shift..=shift {
                for dx in -shift..=shift {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx >= 0 && ny >= 0 && nx < *bw as i64 && ny < *bh as i64 && bv[(ny * *bw as i64 + nx) as usize] {
                        hit += 1;
                        break 'near;
                    }
                }
            }
        }
    }
    (total, hit)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let jobs = args.first().expect("give jobs.tsv");
    let shift = args.iter().position(|a| a == "--shift").and_then(|i| args.get(i + 1)).map_or(1, |s| s.parse().expect("--shift"));
    for line in std::fs::read_to_string(jobs).expect("read jobs").lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 3 {
            continue;
        }
        // A page only one side has: the other is empty.
        let open = |p: &str| image::open(p).map_or((0, 0, Vec::new()), |i| ink(&i.to_rgba8()));
        let (r, c) = (open(f[1]), open(f[2]));
        let (ri, rm) = matched(&r, &c, shift);
        let (ci, cm) = matched(&c, &r, shift);
        let (p, q) = (rm as f64 / ri.max(1) as f64, cm as f64 / ci.max(1) as f64);
        let score = if ri == 0 && ci == 0 { 1.0 } else if p + q == 0.0 { 0.0 } else { 2.0 * p * q / (p + q) };
        println!("{}\t{ri}\t{rm}\t{ci}\t{cm}\t{score:.4}", f[0]);
    }
}
