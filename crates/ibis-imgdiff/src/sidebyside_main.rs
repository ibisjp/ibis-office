// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ibis-sidebyside: two PNGs next to each other in one PNG, for reports that
//! compare the app with the real Office.
//!
//!   ibis-sidebyside <left.png> <right.png> --out both.png [--gap 16]
//!
//! The pictures are scaled to the same height (the taller one's), the left one
//! on the left, with a grey gap between them.

use std::process::ExitCode;

use image::{imageops::FilterType, Rgba, RgbaImage};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("ibis-sidebyside: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let (mut paths, mut out, mut gap) = (Vec::new(), None, 16u32);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = Some(args.next().ok_or("--out needs a value")?),
            "--gap" => gap = args.next().ok_or("--gap needs a value")?.parse().map_err(|_| "--gap takes an integer")?,
            _ => paths.push(arg),
        }
    }
    let (Some(out), [left, right]) = (out, paths.as_slice()) else {
        return Err("usage: ibis-sidebyside <left.png> <right.png> --out both.png [--gap 16]".into());
    };
    let load = |path: &String| image::open(path).map(|i| i.to_rgba8()).map_err(|e| format!("{path}: {e}"));
    let (a, b) = (load(left)?, load(right)?);
    let height = a.height().max(b.height());
    let fit = |i: RgbaImage| {
        if i.height() == height {
            i
        } else {
            let width = (i.width() as u64 * height as u64 / i.height() as u64).max(1) as u32;
            image::imageops::resize(&i, width, height, FilterType::Lanczos3)
        }
    };
    let (a, b) = (fit(a), fit(b));
    let mut both = RgbaImage::from_pixel(a.width() + gap + b.width(), height, Rgba([200, 200, 200, 255]));
    image::imageops::replace(&mut both, &a, 0, 0);
    image::imageops::replace(&mut both, &b, (a.width() + gap) as i64, 0);
    both.save(&out).map_err(|e| format!("{out}: {e}"))
}
