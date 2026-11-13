// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The soft edge (`a:softEdge`) of PowerPoint, for pictures (`picture_svg`) and filled shapes (`shape_view`): one formula, one place.
//!
//! A shape's opacity mask is made from its outline as measured in #M88 and #M92 (`tasks/doc/m88-softedge-shapes.md`, `m92-...md`): the outline is
//! shrunk by `SHIFT` x rad PARALLEL to itself (the Euclidean distance to the outside, so a curve and a slanted edge are shrunk as a straight one;
//! a square erosion or a scaling about the centre is not that) and the shrunk shape is blurred by a Gaussian of `SIGMA` x rad. A shape thinner
//! than twice the shrink has nothing left and is not drawn. The mask is a bitmap made in the pixels the shape is drawn in (an outline mapped
//! through its rotation and flips, which the isotropic shrink and blur do not mind), the same picture on the screen, in a PDF and on the web.
//!
/// How far inside the edge a soft edge is half transparent, in radii (`rad`): PowerPoint's opacity rises along
/// the error function of the distance from the edge, alpha(t) = Phi((t - SHIFT * rad) / (SIGMA * rad)), the same
/// across and down so that a corner is the product of the two. MEASURED, not derived (2026-11-11 user decision: AGENTS.md, "実装の決まり"):
/// PowerPoint's pixels (Microsoft 365 build 16.0.20527, Slide.Export at 960, 1920 and 3840 px wide; a red picture and a red/blue striped
/// one, 160 x 120 pt, rad 2.5, 5, 10, 25 pt) fitted with the error function: the middle is at 0.86 to 0.88 of rad (0.85 for 2.5 pt up to 0.89
/// for 25 pt), the spread at 0.32 to 0.34 of rad, the mean squared error 0.3 to 0.6 %, the corner as the product with the greatest error
/// 0.9 %; at rad 10 pt alpha is 2 % at 1.82 pt, 50 % at 8.62 pt and 98 % at 15.18 pt. The colours are not blurred (red and blue stay apart),
/// only the opacity changes. The spread of a third of rad is "the radius is three deviations"; why the middle is at 0.87 of rad (a shrink by
/// rad would put it at 1.00) is NOT known (tasks/doc/m62-softedge-153-155/README.md). When the mechanism is found, replace it here.
/// A filled rectangle (160 x 120 pt, rad 2.5 to 25 pt) fades with the same numbers (the same page's #M65 section); an ellipse's half
/// transparent place goes further in as the radius grows (0.915 of rad at 10 pt, 0.985 across and 0.91 down at 25 pt), which is not handled.
pub(crate) const SHIFT: f64 = 0.87;
pub(crate) const SIGMA: f64 = 1.0 / 3.0;

use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use gpui_kit::{Bounds, Corners, RenderImage, Window, point, px, size};

use crate::blur_shadow::Polygons;

/// The most cells along the longer side of a soft edge's bitmap. An engineering limit, not a fitted number: the distance transform and the blur
/// are made on the CPU in `f32` buffers (a few of them, 4 bytes a cell) every time the shape's size changes (a drag), and the picture goes to
/// the GPU as one texture; 2048 x 2048 cells keep that under about 16 MB a buffer, as the other pictures made here (`blur_shadow.rs` stops at
/// 1500 cells). Beyond it the cells grow (the mask is smooth, its edge a few radii wide) and the picture is stretched back.
const MAX_CELLS: f32 = 2048.0;

/// Cells of the grid the mask is blurred on, across one `rad` (the soft edge's radius). The blur, and the distance read at the grid's centres, cost
/// less as the grid is coarser, so a drag (the mask is made again at every size) is quicker for a larger picture. The outline's place is the picture's
/// own (the distance is measured on its cells, the threshold is a ramp as wide as a grid cell: see `shape_mask`), so what the grid costs is the
/// blur's sampling and the bilinear read-back: after the Gaussian of deviation `rad / 3` the mask has no detail finer than a deviation (its spectrum
/// is at 1 % of the peak at one cycle per deviation, `sqrt(2 ln 100) / (2 pi sigma)`), and linear interpolation of a Gaussian-smoothed edge is off by
/// `step^2 / 8 x` its curvature (`0.24 / sigma^2` at most), 0.0013 at the largest step the tests try. A grid of `rad / 12` (a quarter of a deviation)
/// keeps the interpolation far under 0.01; the tests below show the error against the exact fade for grids of 1/3, 1/6 and 1/12.
const MASK_CELLS_PER_RAD: f32 = 12.0;

/// How many picture cells go into one cell of the mask's grid, for a soft edge of `rad` picture cells (1: the picture's own grid).
#[cfg(test)]
fn mask_step(rad: f32) -> usize {
    mask_step_for(rad, MASK_CELLS_PER_RAD)
}

fn mask_step_for(rad: f32, cells_per_rad: f32) -> usize {
    ((rad / cells_per_rad).round() as usize).max(1)
}

/// The mask of a shape whose coverage on the picture's grid is `coverage` (`cols` x `rows` cells): `mask_of_coverage`, but with the distance to the
/// outside measured exactly on the picture's cells and only at the centres of a grid `mask_step` times coarser, the threshold made a ramp as wide as
/// that grid's cells (what averaging the picture's cells' sharp ramp over them gives, with the edge's place kept), the blur made on the coarse grid and
/// the mask read back by bilinear interpolation. The outline's place is therefore the picture's, not the coarse grid's.
fn shape_mask(coverage: &[f32], cols: usize, rows: usize, rad: f32) -> Vec<f32> {
    shape_mask_with(coverage, cols, rows, rad, MASK_CELLS_PER_RAD)
}

fn shape_mask_with(coverage: &[f32], cols: usize, rows: usize, rad: f32, cells_per_rad: f32) -> Vec<f32> {
    let step = mask_step_for(rad, cells_per_rad);
    if step == 1 {
        return mask_of_coverage(coverage, cols, rows, rad);
    }
    let (coarse_cols, coarse_rows) = (cols.div_ceil(step) + 1, rows.div_ceil(step) + 1);
    let depth = coarse_inside_distance(coverage, cols, rows, step, coarse_cols, coarse_rows);
    let shrink = shrink(rad as f64) as f32;
    let width = step as f32;
    let mut coarse: Vec<f32> = depth.iter().map(|d| ((d - shrink - 0.5) / width + 0.5).clamp(0.0, 1.0)).collect();
    gaussian_exact(&mut coarse, coarse_cols, coarse_rows, deviation(rad as f64) as f32 / step as f32);
    // The picture's cell (i, j) has its centre at (i + 0.5) / step - 0.5 in the coarse cells' own indices.
    let at = |i: usize, coarse_size: usize| -> (usize, usize, f32) {
        let u = ((i as f32 + 0.5) / step as f32 - 0.5).clamp(0.0, (coarse_size - 1) as f32);
        let a = (u.floor() as usize).min(coarse_size - 1);
        (a, (a + 1).min(coarse_size - 1), u - a as f32)
    };
    // Along the rows first (once for each coarse row), then between two of those rows for each of the picture's.
    let columns: Vec<(usize, usize, f32)> = (0..cols).map(|i| at(i, coarse_cols)).collect();
    let mut wide = vec![0f32; coarse_rows * cols];
    for (row, out) in wide.chunks_exact_mut(cols).enumerate() {
        let line = &coarse[row * coarse_cols..(row + 1) * coarse_cols];
        for (o, &(c0, c1, fx)) in out.iter_mut().zip(&columns) {
            *o = line[c0] * (1.0 - fx) + line[c1] * fx;
        }
    }
    let mut mask = vec![0f32; cols * rows];
    for (j, out) in mask.chunks_exact_mut(cols).enumerate() {
        let (r0, r1, fy) = at(j, coarse_rows);
        let (top_row, bottom_row) = (&wide[r0 * cols..(r0 + 1) * cols], &wide[r1 * cols..(r1 + 1) * cols]);
        for ((o, t), b) in out.iter_mut().zip(top_row).zip(bottom_row) {
            *o = t * (1.0 - fy) + b * fy;
        }
    }
    mask
}

/// `inside_distance` (the exact Euclidean distance from a cell's centre to the centre of the nearest cell outside the shape, coverage under a half)
/// at the centres of the cells of a grid `step` times coarser (cell (i, j) of it has its centre at ((i + 0.5) step - 0.5, (j + 0.5) step - 0.5) in
/// the picture's cell indices), without making it for every cell. Down a column the nearest outside cell is found by two sweeps; along a row of the
/// coarse grid the lower envelope of the parabolas of those distances is built once and read at the coarse centres (Felzenszwalb and Huttenlocher).
fn coarse_inside_distance(coverage: &[f32], cols: usize, rows: usize, step: usize, coarse_cols: usize, coarse_rows: usize) -> Vec<f32> {
    const FAR: f32 = 1e9;
    let centre = |j: usize| (j as f32 + 0.5) * step as f32 - 0.5;
    // Squared distance down each column from the coarse row `j`'s centre to its nearest outside cell, for every coarse row.
    let mut down = vec![vec![FAR; cols]; coarse_rows];
    let mut last = vec![-1i64; cols];
    let mut next_row = 0usize;
    // (The picture's row `r` at or above the centre: `lo`; the one at or below: `lo + 1`.)
    let lo_of = |j: usize| (centre(j).floor().max(0.0) as usize).min(rows - 1);
    for r in 0..rows {
        for x in 0..cols {
            if coverage[r * cols + x] < 0.5 {
                last[x] = r as i64;
            }
        }
        while next_row < coarse_rows && lo_of(next_row) == r {
            let y = centre(next_row);
            for x in 0..cols {
                if last[x] >= 0 {
                    let d = y - last[x] as f32;
                    down[next_row][x] = d * d;
                }
            }
            next_row += 1;
        }
    }
    let mut next = vec![-1i64; cols];
    let mut j = coarse_rows;
    for r in (0..rows).rev() {
        for x in 0..cols {
            if coverage[r * cols + x] < 0.5 {
                next[x] = r as i64;
            }
        }
        while j > 0 && (lo_of(j - 1) + 1).min(rows - 1) == r {
            let y = centre(j - 1);
            for x in 0..cols {
                if next[x] >= 0 {
                    let d = next[x] as f32 - y;
                    down[j - 1][x] = down[j - 1][x].min(d * d);
                }
            }
            j -= 1;
        }
    }
    let n = cols;
    let (mut v, mut z) = (vec![0usize; n], vec![0f32; n + 1]);
    let mut depth = vec![0f32; coarse_cols * coarse_rows];
    for (jj, f) in down.iter().enumerate() {
        let mut k = 0usize;
        v[0] = 0;
        z[0] = f32::MIN;
        z[1] = f32::MAX;
        for q in 1..n {
            let mut s;
            loop {
                let p = v[k];
                s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
                if s <= z[k] {
                    k -= 1;
                } else {
                    break;
                }
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f32::MAX;
        }
        let mut k = 0usize;
        for i in 0..coarse_cols {
            let x = centre(i);
            while z[k + 1] < x {
                k += 1;
            }
            let p = x - v[k] as f32;
            let squared = p * p + f[v[k]];
            depth[jj * coarse_cols + i] = if squared >= FAR / 2.0 { f32::MAX } else { squared.sqrt() };
        }
    }
    depth
}

/// How far the outline is shrunk (the opacity's edge is half transparent there) for a soft edge of radius `rad` (any unit).
pub(crate) fn shrink(rad: f64) -> f64 {
    rad * SHIFT
}

/// The deviation of the Gaussian that blurs the shrunk outline, for a soft edge of radius `rad` (the same unit).
pub(crate) fn deviation(rad: f64) -> f64 {
    rad * SIGMA
}

/// One painted part of a shape (a band of its gradient, its fill, a piece of its line), as the polygons it covers (window pixels) and its
/// colour (straight, 0 to 1); painted over the layers before it. `pattern` keeps only the cells a pattern fill's tile has set.
pub(crate) struct Layer {
    pub outlines: Vec<Polygons>,
    pub rgba: [f32; 4],
    pub pattern: Option<Pattern>,
    /// A picture fill: its pixels take the place of `rgba` (the layer's alpha is the pixel's, times the coverage).
    pub image: Option<ImageSample>,
}

/// Where a picture fill's pixels come from for a soft edge: the decoded picture, how it sits on the shape's frame and how window pixels are turned
/// back to the frame's own coordinates (the shape's flips and rotation: `shape_view::Placement`).
#[derive(Clone)]
pub(crate) struct ImageSample {
    pub image: Arc<RenderImage>,
    pub kind: ImageKind,
    pub local: LocalMap,
}

/// How a picture sits on the frame (EMU, the frame's own coordinates).
#[derive(Clone, Copy)]
pub(crate) enum ImageKind {
    /// Stretched over `rect` (left, top, right, bottom: the fill's `fillRect` insets applied), showing the part `crop` (left, top, right, bottom fractions) of it.
    Stretch { rect: (f32, f32, f32, f32), crop: (f32, f32, f32, f32) },
    /// Tiles of `width` x `height` from (`x`, `y`), the odd ones mirrored along an axis when its flag is set.
    Tile { width: f32, height: f32, x: f32, y: f32, flip_x: bool, flip_y: bool },
}

/// The turn from window pixels back to the frame's own coordinates (EMU): the inverse of `Placement::map`.
#[derive(Clone, Copy)]
pub(crate) struct LocalMap {
    /// The frame's corner in the window, pixels, and pixels per EMU.
    pub origin: (f32, f32),
    pub scale: f32,
    pub width: f32,
    pub height: f32,
    pub flip_h: bool,
    pub flip_v: bool,
    pub sin: f32,
    pub cos: f32,
    pub shear: f32,
}

impl LocalMap {
    fn apply(&self, wx: f32, wy: f32) -> (f32, f32) {
        let (cx, cy) = (self.width / 2.0, self.height / 2.0);
        let (dx, dy) = ((wx - self.origin.0) / self.scale - cx, (wy - self.origin.1) / self.scale - cy);
        let (x, y) = (dx * self.cos + dy * self.sin, -dx * self.sin + dy * self.cos);
        let (x, y) = (x - self.shear * y + cx, y + cy);
        (if self.flip_h { self.width - x } else { x }, if self.flip_v { self.height - y } else { y })
    }
}

impl ImageSample {
    /// The picture's colour (straight, 0 to 1) at the window pixel (`wx`, `wy`): bilinear between the pixels, nothing outside the picture.
    fn at(&self, wx: f32, wy: f32) -> Option<[f32; 4]> {
        let (x, y) = self.local.apply(wx, wy);
        let size = self.image.size(0);
        let (w, h) = (size.width.0 as usize, size.height.0 as usize);
        // The sample's place in picture pixels (the centres of the pixels at the whole numbers), and, for a tiled picture, the flips of the odd tiles:
        // the pixels beside a tile's edge are the next tile's, so the edge is blended as the painter does (two pixels of both colours), not cut.
        let (fx, fy, tiled) = match self.kind {
            ImageKind::Stretch { rect: (l, t, r, b), crop: (cl, ct, cr, cb) } => {
                if r <= l || b <= t {
                    return None;
                }
                let (u, v) = ((x - l) / (r - l), (y - t) / (b - t));
                if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                    return None;
                }
                let (u, v) = (cl + u * (1.0 - cl - cr), ct + v * (1.0 - ct - cb));
                ((u * w as f32 - 0.5).clamp(0.0, (w.max(1) - 1) as f32), (v * h as f32 - 0.5).clamp(0.0, (h.max(1) - 1) as f32), None)
            }
            ImageKind::Tile { width, height, x: ax, y: ay, flip_x, flip_y } => {
                if width <= 0.0 || height <= 0.0 {
                    return None;
                }
                (((x - ax) / width) * w as f32 - 0.5, ((y - ay) / height) * h as f32 - 0.5, Some((flip_x, flip_y)))
            }
        };
        let bytes = self.image.as_bytes(0)?;
        if w == 0 || h == 0 || bytes.len() < w * h * 4 {
            return None;
        }
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as i64, y0 as i64);
        // The picture's pixel at (`gx`, `gy`) counted across the tiles: which tile, and the pixel of it (mirrored in an odd tile that flips).
        let pixel = |gx: i64, gy: i64| -> [f32; 4] {
            let (mut px, mut py) = (gx, gy);
            if let Some((flip_x, flip_y)) = tiled {
                let (col, row) = (gx.div_euclid(w as i64), gy.div_euclid(h as i64));
                px = gx.rem_euclid(w as i64);
                py = gy.rem_euclid(h as i64);
                if flip_x && col.rem_euclid(2) == 1 {
                    px = w as i64 - 1 - px;
                }
                if flip_y && row.rem_euclid(2) == 1 {
                    py = h as i64 - 1 - py;
                }
            }
            let i = (py.clamp(0, h as i64 - 1) as usize * w + px.clamp(0, w as i64 - 1) as usize) * 4;
            // BGRA, not premultiplied.
            [f32::from(bytes[i + 2]) / 255.0, f32::from(bytes[i + 1]) / 255.0, f32::from(bytes[i]) / 255.0, f32::from(bytes[i + 3]) / 255.0]
        };
        let (p00, p10, p01, p11) = (pixel(x0, y0), pixel(x0 + 1, y0), pixel(x0, y0 + 1), pixel(x0 + 1, y0 + 1));
        // Alpha-weighted, so a see-through pixel's colour does not bleed into its neighbours.
        let mut out = [0f32; 4];
        let weights = [(1.0 - tx) * (1.0 - ty), tx * (1.0 - ty), (1.0 - tx) * ty, tx * ty];
        let mut alpha = 0.0;
        for (p, w) in [p00, p10, p01, p11].iter().zip(weights) {
            alpha += p[3] * w;
            for k in 0..3 {
                out[k] += p[k] * p[3] * w;
            }
        }
        if alpha > 1e-6 {
            for k in 0..3 {
                out[k] /= alpha;
            }
        }
        out[3] = alpha;
        Some(out)
    }
}

/// A pattern fill's 8 x 8 tile of cells (`ibis_doc_model::pattern_tile`: a row a byte, the left cell the high bit), each `cell` window pixels, anchored at `origin`.
#[derive(Clone, Copy)]
pub(crate) struct Pattern {
    pub tile: [u8; 8],
    pub cell: f32,
    pub origin: (f32, f32),
}

thread_local! {
    static PICTURES: RefCell<HashMap<u64, Arc<RenderImage>>> = RefCell::new(HashMap::new());
}

/// The opacity (0 to 1) a soft edge of radius `rad` cells leaves on a shape whose coverage is `coverage` (`cols` x `rows` cells): the cells at
/// least `shrink(rad)` inside the edge, taken as 1 (blended over the cell the edge crosses), blurred by `deviation(rad)`.
pub(crate) fn mask_of_coverage(coverage: &[f32], cols: usize, rows: usize, rad: f32) -> Vec<f32> {
    let depth = inside_distance(coverage, cols, rows);
    let shrink = shrink(rad as f64) as f32;
    let mut mask: Vec<f32> = depth.iter().map(|d| (d - shrink).clamp(0.0, 1.0)).collect();
    let sigma = deviation(rad as f64) as f32;
    // (A few cells of deviation: the kernel itself; the boxes of `gaussian` are of whole odd widths and miss the deviation by up to 4 % there.)
    if sigma < SMALL_DEVIATION { gaussian_exact(&mut mask, cols, rows, sigma) } else { gaussian(&mut mask, cols, rows, sigma) }
    mask
}

/// Below this deviation (cells) the blur is a convolution with the kernel itself (`gaussian_exact`), above it boxes (`gaussian`).
const SMALL_DEVIATION: f32 = 6.0;

/// How far each cell's centre is from the nearest cell that is outside the shape (coverage under a half), in cells: the exact Euclidean
/// distance (Felzenszwalb and Huttenlocher's transform of the squared distance, one axis then the other). A cell outside is 0.
fn inside_distance(coverage: &[f32], cols: usize, rows: usize) -> Vec<f32> {
    const FAR: f32 = 1e9;
    let mut d: Vec<f32> = coverage.iter().map(|c| if *c >= 0.5 { FAR } else { 0.0 }).collect();
    let n = cols.max(rows);
    let (mut v, mut z, mut f) = (vec![0usize; n], vec![0f32; n + 1], vec![0f32; n]);
    let mut line = |get: &dyn Fn(usize) -> f32, len: usize, put: &mut dyn FnMut(usize, f32)| {
        for i in 0..len {
            f[i] = get(i);
        }
        let mut k = 0usize;
        v[0] = 0;
        z[0] = f32::MIN;
        z[1] = f32::MAX;
        for q in 1..len {
            let mut s;
            loop {
                let p = v[k];
                s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
                if s <= z[k] {
                    k -= 1;
                } else {
                    break;
                }
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f32::MAX;
        }
        let mut k = 0usize;
        for q in 0..len {
            while z[k + 1] < q as f32 {
                k += 1;
            }
            let p = v[k] as f32 - q as f32;
            put(q, p * p + f[v[k]]);
        }
    };
    for j in 0..rows {
        let row: Vec<f32> = d[j * cols..(j + 1) * cols].to_vec();
        let out = &mut d[j * cols..(j + 1) * cols];
        line(&|i| row[i], cols, &mut |i, x| out[i] = x);
    }
    for i in 0..cols {
        let column: Vec<f32> = (0..rows).map(|j| d[j * cols + i]).collect();
        let mut result = vec![0f32; rows];
        line(&|j| column[j], rows, &mut |j, x| result[j] = x);
        for j in 0..rows {
            d[j * cols + i] = result[j];
        }
    }
    d.iter().map(|x| if *x >= FAR / 2.0 { f32::MAX } else { x.sqrt() }).collect()
}

/// `mask` blurred in place by a Gaussian of `deviation` cells (cells outside are 0): four box blurs (Kovesi's widths), as the measurement used.
/// `mask` blurred by a Gaussian of `deviation` cells, as a convolution with the kernel itself (to three deviations) along the rows and then the
/// columns: exact to the cell, where `gaussian`'s boxes are of whole odd widths (their deviation is off by up to 4 % at a few cells). Used on the
/// coarse grid, which has few cells; the edges repeat the border cell.
fn gaussian_exact(mask: &mut [f32], cols: usize, rows: usize, deviation: f32) {
    if deviation < 0.3 {
        return;
    }
    let reach = (3.0 * deviation).ceil() as isize;
    let kernel: Vec<f32> = (-reach..=reach).map(|k| (-((k * k) as f32) / (2.0 * deviation * deviation)).exp()).collect();
    let total: f32 = kernel.iter().sum();
    let kernel: Vec<f32> = kernel.iter().map(|k| k / total).collect();
    let mut across = vec![0f32; cols * rows];
    for j in 0..rows {
        for i in 0..cols {
            let mut sum = 0.0;
            for (k, w) in kernel.iter().enumerate() {
                let x = (i as isize + k as isize - reach).clamp(0, cols as isize - 1) as usize;
                sum += w * mask[j * cols + x];
            }
            across[j * cols + i] = sum;
        }
    }
    for j in 0..rows {
        let out = &mut mask[j * cols..(j + 1) * cols];
        out.fill(0.0);
        for (k, w) in kernel.iter().enumerate() {
            let y = (j as isize + k as isize - reach).clamp(0, rows as isize - 1) as usize;
            for (o, a) in out.iter_mut().zip(&across[y * cols..(y + 1) * cols]) {
                *o += w * a;
            }
        }
    }
}

fn gaussian(mask: &mut [f32], cols: usize, rows: usize, deviation: f32) {
    if deviation < 0.3 {
        return;
    }
    const PASSES: usize = 4;
    let ideal = (12.0 * deviation * deviation / PASSES as f32 + 1.0).sqrt();
    let mut lower = ideal.floor() as i64;
    if lower % 2 == 0 {
        lower -= 1;
    }
    let lower = lower.max(1);
    let upper = lower + 2;
    let n = PASSES as f32;
    let narrow = ((12.0 * deviation * deviation - n * (lower * lower) as f32 - 4.0 * n * lower as f32 - 3.0 * n) / (-4.0 * lower as f32 - 4.0)).round().clamp(0.0, n) as usize;
    let mut scratch = vec![0f32; cols.max(rows)];
    for pass in 0..PASSES {
        let radius = (if pass < narrow { lower } else { upper } as usize - 1) / 2;
        for j in 0..rows {
            box_line(&mut mask[j * cols..(j + 1) * cols], &mut scratch, radius);
        }
        let mut column = vec![0f32; rows];
        for i in 0..cols {
            for j in 0..rows {
                column[j] = mask[j * cols + i];
            }
            box_line(&mut column, &mut scratch, radius);
            for j in 0..rows {
                mask[j * cols + i] = column[j];
            }
        }
    }
}

/// `line` replaced by the mean of the `2 * radius + 1` cells round each (cells beyond the ends are 0).
fn box_line(line: &mut [f32], scratch: &mut [f32], radius: usize) {
    let n = line.len();
    let width = (2 * radius + 1) as f32;
    let mut sum: f32 = line[..(radius + 1).min(n)].iter().sum();
    for i in 0..n {
        scratch[i] = sum / width;
        if i + radius + 1 < n {
            sum += line[i + radius + 1];
        }
        if i >= radius {
            sum -= line[i - radius];
        }
    }
    line.copy_from_slice(&scratch[..n]);
}

/// The opacity mask of an outline made as a shape's is (`shape_mask`), as a bitmap for a picture that is cut to an outline and masked (`picture_svg`):
/// `outlines` (closed polygons, any unit) and `rad` in the same unit. The bitmap's cells are `pitch` units from (`left`, `top`); outside it the
/// mask is 0. A picture is therefore faded as a filled shape is, whatever the outline (one formula; the SVG's `feMorphology` was a square erosion).
pub(crate) struct OutlineMask {
    pub left: f32,
    pub top: f32,
    pub pitch: f32,
    pub cols: usize,
    pub rows: usize,
    pub mask: Vec<f32>,
}

pub(crate) fn outline_mask(outlines: &[Polygons], rad: f32) -> Option<OutlineMask> {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in outlines.iter().flatten().flatten() {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    if !(x0 < x1 && y0 < y1 && x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) || !(rad > 0.0) {
        return None;
    }
    let pitch = 1.0f32.min(rad / MASK_CELLS_PER_RAD).max((x1 - x0) / MAX_CELLS).max((y1 - y0) / MAX_CELLS);
    let (left, top) = (x0 - 2.0 * pitch, y0 - 2.0 * pitch);
    let cols = ((x1 + 2.0 * pitch - left) / pitch).ceil() as usize + 1;
    let rows = ((y1 + 2.0 * pitch - top) / pitch).ceil() as usize + 1;
    let coverage = crate::blur_shadow::coverage(outlines, left, top, pitch, cols, rows);
    let mask = shape_mask(&coverage, cols, rows, rad / pitch);
    Some(OutlineMask { left, top, pitch, cols, rows, mask })
}

/// The shadow of a soft-edged shape (`a:outerShdw`): moved by (`dx`, `dy`) window pixels, blurred by a Gaussian of deviation `sigma` window pixels,
/// in `color` (whose alpha is the shadow's).
#[derive(Clone, Copy)]
pub(crate) struct Shadow {
    pub dx: f32,
    pub dy: f32,
    pub sigma: f32,
    pub color: gpui_kit::Hsla,
}

/// The cells a shape's picture is made on: the layers' bounding box grown by `margin` window pixels (and two cells), cells of `pitch` pixels.
struct Grid {
    left: f32,
    top: f32,
    pitch: f32,
    cols: usize,
    rows: usize,
}

fn grid_of(layers: &[Layer], scale_factor: f32, margin: f32) -> Option<Grid> {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for &(x, y) in layers.iter().flat_map(|l| l.outlines.iter()).flatten().flatten() {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    if !(x0 < x1 && y0 < y1 && x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
        return None;
    }
    let pitch = (1.0 / scale_factor.max(1.0)).max((x1 - x0 + 2.0 * margin) / MAX_CELLS).max((y1 - y0 + 2.0 * margin) / MAX_CELLS);
    let margin = margin + 2.0 * pitch;
    let (left, top) = ((x0 - margin).floor(), (y0 - margin).floor());
    let cols = ((x1 + margin - left) / pitch).ceil() as usize + 1;
    let rows = ((y1 + margin - top) / pitch).ceil() as usize + 1;
    Some(Grid { left, top, pitch, cols, rows })
}

/// Paint `layers` (a shape's fill and line, in this order) faded by a soft edge of radius `rad` window pixels: the layers' union is the shape whose
/// outline is shrunk and blurred (`mask_of_coverage`), the layers are drawn into one picture of the cells and its opacity is multiplied by the mask.
/// `scale_factor` is the window's (a cell is a device pixel unless the shape is larger than `MAX_CELLS` of them).
/// `shadow`: PowerPoint makes the shadow from the FADED shape (the mask applied: measured, #M92 section 5), so it is made from the same mask
/// and painted under the shape.
pub(crate) fn paint_shape(window: &mut Window, layers: &[Layer], rad: f32, scale_factor: f32, shadow: Option<Shadow>) {
    if rad < 0.0 {
        return;
    }
    if let Some(shadow) = shadow {
        paint_shadow(window, layers, rad, scale_factor, shadow);
    }
    let Some(Grid { left, top, pitch, cols, rows }) = grid_of(layers, scale_factor, 0.0) else {
        return;
    };
    // The picture depends on the outlines relative to its corner, so a shape that only moves is not made again.
    let mut hasher = DefaultHasher::new();
    let quantum = |v: f32| (v * 16.0).round() as i32;
    for (n, layer) in layers.iter().enumerate() {
        (n, layer.rgba.map(f32::to_bits)).hash(&mut hasher);
        if let Some(sample) = &layer.image {
            let l = &sample.local;
            (sample.image.id, quantum(l.origin.0 - left), quantum(l.origin.1 - top)).hash(&mut hasher);
            [l.scale, l.width, l.height, l.sin, l.cos, l.shear].map(f32::to_bits).hash(&mut hasher);
            (l.flip_h, l.flip_v).hash(&mut hasher);
            match sample.kind {
                ImageKind::Stretch { rect, crop } => (0u8, [rect.0, rect.1, rect.2, rect.3, crop.0, crop.1, crop.2, crop.3].map(f32::to_bits)).hash(&mut hasher),
                ImageKind::Tile { width, height, x, y, flip_x, flip_y } => (1u8, [width, height, x, y].map(f32::to_bits), flip_x, flip_y).hash(&mut hasher),
            }
        }
        if let Some(p) = &layer.pattern {
            (p.tile, p.cell.to_bits(), quantum(p.origin.0 - left), quantum(p.origin.1 - top)).hash(&mut hasher);
        }
        for (k, poly) in layer.outlines.iter().flatten().enumerate() {
            (k, poly.len()).hash(&mut hasher);
            for &(x, y) in poly {
                (quantum(x - left), quantum(y - top)).hash(&mut hasher);
            }
        }
    }
    (rad.to_bits(), pitch.to_bits(), cols, rows).hash(&mut hasher);
    let key = hasher.finish();
    let picture = PICTURES.with(|cache| cache.borrow().get(&key).cloned()).or_else(|| {
        let picture = Arc::new(RenderImage::new([image::Frame::new(shape_image(layers, left, top, pitch, cols, rows, rad / pitch)?)]));
        PICTURES.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.len() >= 64 {
                cache.clear();
            }
            cache.insert(key, picture.clone());
        });
        Some(picture)
    });
    if let Some(picture) = picture {
        let area = Bounds::new(point(px(left), px(top)), size(px(cols as f32 * pitch), px(rows as f32 * pitch)));
        let _ = window.paint_image(area, area, Corners::default(), picture, 0, false);
    }
}

/// The shadow of the faded shape: the union of the layers' coverage with the soft edge's mask applied, blurred and painted in the shadow's colour.
fn paint_shadow(window: &mut Window, layers: &[Layer], rad: f32, scale_factor: f32, shadow: Shadow) {
    let sigma = shadow.sigma.max(0.0);
    let Some(Grid { left, top, pitch, cols, rows }) = grid_of(layers, scale_factor, 3.0 * sigma) else {
        return;
    };
    let mut hasher = DefaultHasher::new();
    let quantum = |v: f32| (v * 16.0).round() as i32;
    for (k, poly) in layers.iter().flat_map(|l| l.outlines.iter()).flatten().enumerate() {
        (k, poly.len()).hash(&mut hasher);
        for &(x, y) in poly {
            (quantum(x - left), quantum(y - top)).hash(&mut hasher);
        }
    }
    (rad.to_bits(), pitch.to_bits(), cols, rows).hash(&mut hasher);
    crate::blur_shadow::paint_mask(window, hasher.finish(), (left + shadow.dx, top + shadow.dy), pitch, (cols, rows), sigma, shadow.color, || {
        let union = union_coverage(layers, left, top, pitch, cols, rows);
        Some(if rad > 0.0 { shape_mask(&union, cols, rows, rad / pitch) } else { union })
    });
}

/// The union of the layers' coverage on the cells (each cell 0 to 1).
fn union_coverage(layers: &[Layer], left: f32, top: f32, pitch: f32, cols: usize, rows: usize) -> Vec<f32> {
    let mut union = vec![0f32; cols * rows];
    for layer in layers {
        let cover = crate::blur_shadow::coverage(&layer.outlines, left, top, pitch, cols, rows);
        for (u, c) in union.iter_mut().zip(cover) {
            *u = u.max(c);
        }
    }
    union
}

/// The picture (BGRA, the colour not premultiplied: the painter multiplies the alpha in) of `layers` on the cells, faded by the soft edge.
/// Each layer is rasterized inside its own box of cells (a gradient has up to 256 bands, the whole picture each would be too much) and laid over
/// the ones before; the union of the layers is the shape whose outline is shrunk (`shape_mask`).
fn shape_image(layers: &[Layer], left: f32, top: f32, pitch: f32, cols: usize, rows: usize, rad: f32) -> Option<image::RgbaImage> {
    // The union of the layers' coverage: the shape whose outline is shrunk.
    let mut union = vec![0f32; cols * rows];
    // Premultiplied colour and alpha, laid over one layer after the other.
    let mut accumulated = vec![[0f32; 4]; cols * rows];
    for layer in layers {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in layer.outlines.iter().flatten().flatten() {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        if !(x0 < x1 && y0 < y1) || !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            continue;
        }
        let first_col = (((x0 - left) / pitch).floor().max(0.0)) as usize;
        let first_row = (((y0 - top) / pitch).floor().max(0.0)) as usize;
        let last_col = ((((x1 - left) / pitch).ceil() + 1.0).max(0.0) as usize).min(cols);
        let last_row = ((((y1 - top) / pitch).ceil() + 1.0).max(0.0) as usize).min(rows);
        if first_col >= last_col || first_row >= last_row {
            continue;
        }
        let (w, h) = (last_col - first_col, last_row - first_row);
        let cover = crate::blur_shadow::coverage(&layer.outlines, left + first_col as f32 * pitch, top + first_row as f32 * pitch, pitch, w, h);
        for j in 0..h {
            for i in 0..w {
                let mut c = cover[j * w + i];
                if c <= 0.0 {
                    continue;
                }
                let at = (first_row + j) * cols + first_col + i;
                union[at] = union[at].max(c);
                if let Some(p) = &layer.pattern {
                    let x = left + (first_col + i) as f32 * pitch + pitch / 2.0;
                    let y = top + (first_row + j) as f32 * pitch + pitch / 2.0;
                    let (cx, cy) = (((x - p.origin.0) / p.cell).floor() as i64, ((y - p.origin.1) / p.cell).floor() as i64);
                    if p.tile[cy.rem_euclid(8) as usize] & (0x80 >> cx.rem_euclid(8)) == 0 {
                        c = 0.0;
                    }
                }
                let rgba = match &layer.image {
                    Some(sample) => match sample.at(left + ((first_col + i) as f32 + 0.5) * pitch, top + ((first_row + j) as f32 + 0.5) * pitch) {
                        Some(color) => color,
                        None => continue,
                    },
                    None => layer.rgba,
                };
                let alpha = rgba[3] * c;
                let px = &mut accumulated[at];
                for k in 0..3 {
                    px[k] = rgba[k] * alpha + px[k] * (1.0 - alpha);
                }
                px[3] = alpha + px[3] * (1.0 - alpha);
            }
        }
    }
    // (A radius of 0 is no fade: the shape as it is, painted this way for what the painter's elements cannot draw.)
    let mask = if rad > 0.0 { shape_mask(&union, cols, rows, rad) } else { union };
    let mut bytes = Vec::with_capacity(cols * rows * 4);
    for i in 0..cols * rows {
        let [r, g, b, a] = accumulated[i];
        let (r, g, b) = if a > 1e-6 { (r / a, g / a, b / a) } else { (0.0, 0.0, 0.0) };
        let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        bytes.extend_from_slice(&[byte(b), byte(g), byte(r), byte(a * mask[i])]);
    }
    image::RgbaImage::from_raw(cols as u32, rows as u32, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiled picture's edge is blended across the tiles (PowerPoint mixes the two pixels beside the seam, tasks/doc/m104-softedge-picture-fill.md),
    /// not cut: a 2 x 1 picture, orange then blue, laid in tiles of 2 x 1 pixels (1 pixel = 1 unit), blends at the seam between two tiles, and a
    /// mirrored odd tile (flip) puts the same colour on both sides of the seam.
    #[test]
    fn a_tile_seam_is_blended_across_the_tiles() {
        let picture = image::RgbaImage::from_fn(2, 1, |x, _| if x == 0 { image::Rgba([0, 0, 255, 255]) } else { image::Rgba([255, 0, 0, 255]) });
        // (The bytes are BGRA: `from_fn` here is given them as they are laid.)
        let image = Arc::new(RenderImage::new([image::Frame::new(picture)]));
        let local = LocalMap { origin: (0.0, 0.0), scale: 1.0, width: 8.0, height: 4.0, flip_h: false, flip_v: false, sin: 0.0, cos: 1.0, shear: 0.0 };
        let plain = ImageSample { image: image.clone(), kind: ImageKind::Tile { width: 2.0, height: 1.0, x: 0.0, y: 0.0, flip_x: false, flip_y: false }, local };
        // Window x 1.5 to 2.5 is the seam (the centre of the last pixel of a tile at 1.5, of the first of the next at 2.5): half way at x = 2.
        let (a, b, c) = (plain.at(1.5, 0.5).unwrap(), plain.at(2.0, 0.5).unwrap(), plain.at(2.5, 0.5).unwrap());
        assert!((a[0] - c[0]).abs() > 0.99 && (a[2] - c[2]).abs() > 0.99, "two colours either side: {a:?} {c:?}");
        for k in [0, 2] {
            assert!((b[k] - (a[k] + c[k]) / 2.0).abs() < 0.01, "blended at the seam: {b:?}");
        }
        // Mirrored odd tiles: the same colour on both sides of the seam, so nothing changes across it.
        let flipped = ImageSample { kind: ImageKind::Tile { width: 2.0, height: 1.0, x: 0.0, y: 0.0, flip_x: true, flip_y: false }, ..plain };
        let (a, b) = (flipped.at(1.5, 0.5).unwrap(), flipped.at(2.5, 0.5).unwrap());
        assert!((a[0] - b[0]).abs() < 1e-3 && (a[2] - b[2]).abs() < 1e-3, "{a:?} {b:?}");
    }

    /// PowerPoint's measured fade for a soft edge of 10 pt (tasks/doc/m62-softedge-153-155/README.md): alpha is 2 % at 1.82 pt, 50 % at
    /// 8.62 pt and 98 % at 15.18 pt from the edge. The error function with the shrink and the deviation of this module is within 0.2 pt
    /// of those three places.
    #[test]
    fn the_fade_has_the_measured_places() {
        fn phi(x: f64) -> f64 {
            0.5 * (1.0 + libm_erf(x / std::f64::consts::SQRT_2))
        }
        // Abramowitz-Stegun 7.1.26, accurate to 1.5e-7.
        fn libm_erf(x: f64) -> f64 {
            let t = 1.0 / (1.0 + 0.3275911 * x.abs());
            let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * (-x * x).exp();
            y.copysign(x)
        }
        let rad = 10.0;
        let alpha = |t: f64| phi((t - shrink(rad)) / deviation(rad));
        for (t, want, tolerance) in [(1.82, 0.02, 0.012), (8.62, 0.5, 0.03), (15.18, 0.98, 0.012)] {
            assert!((alpha(t) - want).abs() < tolerance, "alpha({t}) = {}", alpha(t));
        }
        assert!((shrink(10.0) - 8.7).abs() < 1e-9 && (deviation(30.0) - 10.0).abs() < 1e-9);
    }

    /// The coverage of `inside(x, y)` (in points) on cells of a quarter point, 2 points of margin round `w` x `h`, and the mask of a soft edge of `rad` points.
    fn mask_of(w: f32, h: f32, rad: f32, inside: impl Fn(f32, f32) -> bool) -> (Vec<f32>, usize, usize) {
        let pitch = 0.25f32;
        let (cols, rows) = (((w + 4.0) / pitch) as usize, ((h + 4.0) / pitch) as usize);
        let coverage: Vec<f32> = (0..cols * rows).map(|i| if inside(((i % cols) as f32 + 0.5) * pitch - 2.0, ((i / cols) as f32 + 0.5) * pitch - 2.0) { 1.0 } else { 0.0 }).collect();
        (mask_of_coverage(&coverage, cols, rows, rad / pitch), cols, rows)
    }

    /// A straight edge: the same places as the picture's (10 pt: 2 % at 1.82 pt, 50 % at 8.7, 98 % at 15.18), and a corner is the product of its two edges.
    #[test]
    fn a_straight_edge_fades_as_measured() {
        let (mask, cols, rows) = mask_of(160.0, 120.0, 10.0, |x, y| (0.0..160.0).contains(&x) && (0.0..120.0).contains(&y));
        let at = |x: f32, y: f32| mask[((y + 2.0) / 0.25) as usize * cols + ((x + 2.0) / 0.25) as usize];
        for (t, want, tolerance) in [(1.82, 0.02, 0.015), (8.7, 0.5, 0.03), (15.18, 0.98, 0.015)] {
            assert!((at(t, 60.0) - want).abs() < tolerance, "alpha({t}) = {}", at(t, 60.0));
        }
        let corner = at(8.7, 8.7);
        assert!((corner - 0.25).abs() < 0.03, "a corner is the product of the edges: {corner}");
        assert!(rows > 0);
    }

    /// An edge at 45 degrees is shrunk as a straight one (Euclidean, not a square erosion, which would shrink it by 0.71 of that): the middle of the
    /// fade is 0.87 rad inside the edge.
    #[test]
    fn a_slanted_edge_is_shrunk_by_the_same_distance() {
        let rad = 10.0;
        let (mask, cols, _) = mask_of(160.0, 160.0, rad, |x, y| x + y > 40.0 && x + y < 280.0 && (x - y).abs() < 120.0);
        // Along the diagonal x + y from the edge x + y = 40: a point t inside is at distance t / sqrt(2) from the edge.
        let at = |t: f32| {
            let (x, y) = (20.0 + t * 0.5, 20.0 + t * 0.5);
            mask[((y + 2.0) / 0.25) as usize * cols + ((x + 2.0) / 0.25) as usize]
        };
        // (20 + t/2, 20 + t/2) is t / sqrt(2) from the edge.
        let half_at = (0.87 * rad) * 2.0f32.sqrt();
        assert!((at(half_at) - 0.5).abs() < 0.05, "alpha at the middle: {}", at(half_at));
        // (Phi(-1.05) = 0.15 and Phi(1.05) = 0.85 at 0.6 and 1.4 of the middle.)
        assert!(at(half_at * 0.6) < 0.2 && at(half_at * 1.4) > 0.8);
    }

    /// A shape thinner than twice the shrink is gone; a wider one keeps its middle.
    #[test]
    fn a_thin_shape_vanishes() {
        let rad = 10.0;
        let disc = |r: f32| move |x: f32, y: f32| (x - r).hypot(y - r) < r;
        let (small, ..) = mask_of(16.0, 16.0, rad, disc(8.0));
        assert!(small.iter().cloned().fold(0.0, f32::max) < 0.02, "a disc of radius 8 pt under a shrink of 8.7");
        let (big, cols, _) = mask_of(80.0, 80.0, rad, disc(40.0));
        assert!(big[((40.0 + 2.0) / 0.25) as usize * cols + ((40.0 + 2.0) / 0.25) as usize] > 0.98);
    }

    /// The time to make a mask while a shape is dragged larger: a 960 x 540 pt shape (the slide) on a 2x screen (1920 x 1080 cells), a soft edge of 12.7 pt.
    /// Printed (`cargo test -- --nocapture`); the bound is loose, to catch a mask that takes seconds.
    #[test]
    fn the_time_of_a_mask() {
        let (cols, rows) = (1924usize, 1084usize);
        let coverage: Vec<f32> = (0..cols * rows).map(|i| if (2..cols - 2).contains(&(i % cols)) && (2..rows - 2).contains(&(i / cols)) { 1.0 } else { 0.0 }).collect();
        let start = ibis_office_base::clock::Instant::now();
        let mask = mask_of_coverage(&coverage, cols, rows, 12.7 * 2.0);
        let took = start.elapsed();
        println!("mask of {cols} x {rows} cells: {took:?}");
        assert!(mask[rows / 2 * cols + cols / 2] > 0.99 && took.as_millis() < 2000);
    }

    /// The time to make a whole shape's picture while it is dragged larger (layers, mask, upload bytes): a 960 x 540 pt rectangle with a 12.7 pt soft edge on a 2x
    /// screen (1924 x 1084 cells with the margin). Printed (`cargo test --release -- --nocapture`).
    #[test]
    fn the_time_of_a_shape_picture() {
        let rect = |x0: f32, y0: f32, x1: f32, y1: f32| -> Vec<Polygons> { vec![vec![vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]]] };
        let layers = vec![Layer { outlines: rect(2.0, 2.0, 1922.0, 1082.0), rgba: [1.0, 0.0, 0.0, 1.0], pattern: None, image: None }];
        let start = ibis_office_base::clock::Instant::now();
        let image = shape_image(&layers, 0.0, 0.0, 1.0, 1924, 1084, 25.4).expect("a picture");
        println!("shape picture of 1924 x 1084 cells: {:?}", start.elapsed());
        assert_eq!(image.dimensions(), (1924, 1084));
        // The same on a 4K screen (3840 x 2160 and the margin, a soft edge twice as many cells wide).
        let layers = vec![Layer { outlines: rect(2.0, 2.0, 3842.0, 2162.0), rgba: [1.0, 0.0, 0.0, 1.0], pattern: None, image: None }];
        let start = ibis_office_base::clock::Instant::now();
        let image = shape_image(&layers, 0.0, 0.0, 1.0, 3844, 2164, 50.8).expect("a picture");
        println!("shape picture of 3844 x 2164 cells: {:?}", start.elapsed());
        assert_eq!(image.dimensions(), (3844, 2164));
    }

    /// The mask on the coarser grid against the mask on the picture's own cells: the largest difference over a rectangle, an ellipse and a turned triangle,
    /// at soft edges of 2.5 to 25 pt on cells of a quarter point, a point and 2 points, for grids of 3, 6 and 12 cells across the radius. Printed.
    #[test]
    fn the_coarse_mask_matches_the_full_one() {
        type Shape = (&'static str, f32, f32, Vec<(f32, f32)>);
        let ellipse: Vec<(f32, f32)> = (0..720).map(|k| { let a = k as f32 / 720.0 * std::f32::consts::TAU; (4.0 + 80.0 + 80.0 * a.cos(), 4.0 + 60.0 + 60.0 * a.sin()) }).collect();
        let shapes: Vec<Shape> = vec![
            ("rectangle", 160.0, 120.0, vec![(4.0, 4.0), (164.0, 4.0), (164.0, 124.0), (4.0, 124.0)]),
            ("ellipse", 160.0, 120.0, ellipse),
            ("triangle", 160.0, 140.0, vec![(84.0, 4.0), (164.0, 144.0), (4.0, 144.0)]),
        ];
        let mut worst = [0.0f32; 3];
        for (name, w, h, polygon) in &shapes {
            for pitch in [0.25f32, 1.0, 2.0] {
                for rad in [2.5f32, 5.0, 10.0, 25.0] {
                    let (cols, rows) = (((w + 8.0) / pitch) as usize, ((h + 8.0) / pitch) as usize);
                    let outlines = vec![vec![polygon.clone()]];
                    let cover = crate::blur_shadow::coverage(&outlines, 0.0, 0.0, pitch, cols, rows);
                    // The mask on the picture's own cells with the exact blur (the boxes of `gaussian` are themselves a percent or so off the Gaussian).
                    let full = {
                        let depth = inside_distance(&cover, cols, rows);
                        let shrink = shrink((rad / pitch) as f64) as f32;
                        let mut mask: Vec<f32> = depth.iter().map(|d| (d - shrink).clamp(0.0, 1.0)).collect();
                        gaussian_exact(&mut mask, cols, rows, deviation((rad / pitch) as f64) as f32);
                        mask
                    };
                    for (n, per_rad) in [3.0f32, 6.0, 12.0].into_iter().enumerate() {
                        let coarse = shape_mask_with(&cover, cols, rows, rad / pitch, per_rad);
                        let diff = full.iter().zip(&coarse).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
                        let mean = full.iter().zip(&coarse).map(|(a, b)| (a - b).abs()).sum::<f32>() / full.len() as f32;
                        println!("{name} pitch {pitch} rad {rad} 1/{per_rad}: step {}, largest difference {diff:.4}, mean {mean:.5}", mask_step_for(rad / pitch, per_rad));
                        worst[n] = worst[n].max(diff);
                    }
                }
            }
        }
        println!("largest difference by grid 1/3, 1/6, 1/12 of the radius: {worst:?}");
        assert!(worst[2] < 0.01, "the coarse mask differs from the exact one by {}", worst[2]);
    }

    /// The mask across a straight edge against the exact fade (the error function of the distance from the place where the shrunk edge is, with the
    /// deviation of this module): the full-resolution pipeline's own difference from it (its blur is of boxes of whole odd widths) and the coarse one's.
    #[test]
    fn the_masks_against_the_exact_fade() {
        fn erf(x: f64) -> f64 {
            let t = 1.0 / (1.0 + 0.3275911 * x.abs());
            let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * (-x * x).exp();
            y.copysign(x)
        }
        let polygon = vec![(4.0f32, 4.0f32), (404.0, 4.0), (404.0, 304.0), (4.0, 304.0)];
        let (cols, rows) = (408usize, 308usize);
        for (pitch, rad_pt) in [(1.0f32, 25.0f32), (0.5, 10.0), (0.25, 25.0), (1.0, 10.0)] {
            let rad = rad_pt / pitch;
            let (cols, rows) = ((cols as f32 / pitch) as usize, (rows as f32 / pitch) as usize);
            let edge = (4.0 / pitch).round();
            let cover = crate::blur_shadow::coverage(&[vec![polygon.iter().map(|(x, y)| (x / 1.0, y / 1.0)).collect()]], 0.0, 0.0, pitch, cols, rows);
            let full = mask_of_coverage(&cover, cols, rows, rad);
            let coarse = shape_mask(&cover, cols, rows, rad);
            let (mut e_full, mut e_coarse) = (0.0f64, 0.0f64);
            for x in 0..(edge as usize + (3.0 * rad) as usize + 4) {
                let exact = 0.5 * (1.0 + erf(((x as f64) - (edge as f64 - 1.0 + shrink(rad as f64) + 0.5)) / (deviation(rad as f64) * std::f64::consts::SQRT_2)));
                let at = (rows / 2) * cols + x;
                e_full = e_full.max((full[at] as f64 - exact).abs());
                e_coarse = e_coarse.max((coarse[at] as f64 - exact).abs());
            }
            println!("EXACT pitch {pitch} rad {rad_pt} pt: full {e_full:.4}, coarse {e_coarse:.4} (step {})", mask_step(rad));
        }
    }
}
