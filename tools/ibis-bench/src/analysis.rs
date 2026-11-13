// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Frame analysis shared by capture and offline replay. No app or OS state changes.
use serde::{Deserialize, Serialize};

pub const WIDTH: usize = 192;
pub const HEIGHT: usize = 120;
pub const CHANGE: f64 = 0.002;

#[derive(Clone, Copy, Debug)]
pub struct Area(pub f64, pub f64, pub f64, pub f64);
pub const FULL: Area = Area(0.0, 0.0, 1.0, 1.0);
pub const DOC: Area = Area(0.05, 0.25, 0.95, 0.95);
pub const SLIDE: Area = Area(0.32, 0.30, 0.72, 0.78);
pub const SLIDES: Area = Area(0.005, 0.15, 0.75, 0.95);

impl Area {
    fn cells(self) -> impl Iterator<Item = usize> {
        let Self(l, t, r, b) = self;
        ((t * HEIGHT as f64) as usize..(b * HEIGHT as f64) as usize).flat_map(move |y| {
            ((l * WIDTH as f64) as usize..(r * WIDTH as f64) as usize).map(move |x| y * WIDTH + x)
        })
    }
}

/// Sixteen evenly spaced samples per cell, matching bench.swift's online grid.
#[cfg_attr(not(windows), allow(dead_code))] // Used by `capture` (Windows only).
pub fn grid(bgra: &[u8], width: usize, height: usize, stride: usize) -> anyhow::Result<Vec<u8>> {
    anyhow::ensure!(
        width > 0 && height > 0 && stride >= width * 4,
        "invalid frame dimensions"
    );
    anyhow::ensure!(
        bgra.len() >= stride * (height - 1) + width * 4,
        "truncated frame"
    );
    let mut result = vec![0; WIDTH * HEIGHT];
    let cell_width = (width / WIDTH).max(1);
    let cell_height = (height / HEIGHT).max(1);
    for gy in 0..HEIGHT {
        for gx in 0..WIDTH {
            let mut sum = 0u32;
            for sy in 0..4 {
                for sx in 0..4 {
                    let x = (gx * width / WIDTH + sx * cell_width / 4).min(width - 1);
                    let y = (gy * height / HEIGHT + sy * cell_height / 4).min(height - 1);
                    let i = y * stride + x * 4;
                    sum +=
                        (29 * bgra[i] as u32 + 150 * bgra[i + 1] as u32 + 77 * bgra[i + 2] as u32)
                            >> 8;
                }
            }
            result[gy * WIDTH + gx] = (sum / 16) as u8;
        }
    }
    Ok(result)
}

pub fn changed(a: &[u8], b: &[u8], area: Area) -> f64 {
    assert_eq!(a.len(), WIDTH * HEIGHT);
    assert_eq!(b.len(), WIDTH * HEIGHT);
    let mut count = 0;
    let mut total = 0;
    for i in area.cells() {
        count += usize::from(a[i].abs_diff(b[i]) > 24);
        total += 1;
    }
    count as f64 / total as f64
}

#[cfg_attr(not(windows), allow(dead_code))] // Used by `screen` (Windows only).
pub fn has_content(grid: &[u8], area: Area) -> bool {
    let cells: Vec<_> = area.cells().collect();
    cells.iter().filter(|&&i| grid[i] < 200).count() as f64 / cells.len() as f64 > 0.005
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Sample {
    pub time: f64,
    pub received: f64,
    pub grid: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Analysis {
    pub frames: usize,
    pub first_change: Option<f64>,
    pub end_frame: Option<usize>,
    pub end: Option<f64>,
    pub window_end_frame: Option<usize>,
    pub window_end: Option<f64>,
    pub last_frame: usize,
    pub changes: Vec<(usize, f64, f64)>,
    pub doc_changes: Vec<(usize, f64, f64)>,
}

pub fn analyze(samples: &[Sample], t0: f64, area: Area) -> anyhow::Result<Analysis> {
    anyhow::ensure!(!samples.is_empty(), "capture returned no frames");
    anyhow::ensure!(
        samples
            .iter()
            .all(|s| s.grid.len() == WIDTH * HEIGHT && s.time.is_finite()),
        "invalid capture grid or timestamp"
    );
    anyhow::ensure!(
        samples.windows(2).all(|w| w[0].time <= w[1].time),
        "capture timestamps out of order"
    );
    let mut changes = Vec::new();
    let mut doc_changes = Vec::new();
    // A pre-trigger frame is mandatory: never count an arbitrary first frame as completion.
    for i in 1..samples.len() {
        let time = samples[i].time - t0;
        if time <= 0.0 {
            continue;
        }
        let fraction = changed(&samples[i].grid, &samples[i - 1].grid, FULL);
        let doc = changed(&samples[i].grid, &samples[i - 1].grid, area);
        if fraction > CHANGE {
            changes.push((i, time, fraction));
        }
        if doc > CHANGE {
            doc_changes.push((i, time, doc));
        }
    }
    Ok(Analysis {
        frames: samples.len(),
        first_change: changes.first().map(|v| v.1),
        end_frame: doc_changes.last().map(|v| v.0),
        end: doc_changes.last().map(|v| v.1),
        window_end_frame: changes.last().map(|v| v.0),
        window_end: changes.last().map(|v| v.1),
        last_frame: samples.len() - 1,
        changes,
        doc_changes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_caret_and_toolbar_but_tracks_document_completion() {
        let baseline = vec![255; WIDTH * HEIGHT];
        let mut toolbar = baseline.clone();
        toolbar[..WIDTH * 10].fill(0);
        let mut document = toolbar.clone();
        document[WIDTH * 60..WIDTH * 70].fill(0);
        let mut caret = document.clone();
        caret[WIDTH * 80 + 80] = 0;
        let samples: Vec<_> = [baseline, toolbar, document, caret]
            .into_iter()
            .enumerate()
            .map(|(i, grid)| Sample {
                time: i as f64,
                received: i as f64,
                grid,
            })
            .collect();
        let result = analyze(&samples, 0.5, DOC).unwrap();
        assert_eq!(result.first_change, Some(0.5));
        assert_eq!(result.end, Some(1.5));
        assert_eq!(result.end_frame, Some(2));
    }
    #[test]
    fn sampling_matches_mac_cell_origin_and_integer_spacing() {
        let (width, height) = (1920, 1200);
        let mut pixels = vec![0u8; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                pixels[(y * width + x) * 4 + 1] = (x % 256) as u8;
            }
        }
        // Mac samples x=0,2,5,7 in the first ten-pixel cell, whose
        // green luma values are 0,1,2,4. Center-offset sampling gives 2.
        assert_eq!(grid(&pixels, width, height, width * 4).unwrap()[0], 1);
    }
    #[test]
    fn stride_padding_and_small_frames_are_supported() {
        assert_eq!(
            grid(&[0, 0, 255, 255, 99, 99, 99, 99], 1, 1, 8).unwrap(),
            vec![76; WIDTH * HEIGHT]
        );
        assert!(grid(&[0; 3], 1, 1, 4).is_err());
    }
    #[test]
    fn no_change_is_not_success_and_bad_timestamps_are_rejected() {
        let make = |time| Sample {
            time,
            received: time,
            grid: vec![255; WIDTH * HEIGHT],
        };
        assert_eq!(
            analyze(&[make(0.0), make(1.0)], 0.5, DOC).unwrap().end,
            None
        );
        assert!(analyze(&[make(1.0), make(0.0)], 0.5, DOC).is_err());
    }
}
