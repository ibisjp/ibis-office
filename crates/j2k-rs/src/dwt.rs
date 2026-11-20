// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ISO/IEC 15444-1 Annex F & G Reversible 5-3 DWT and Reversible Color Transform (RCT).

/// Forward Reversible Color Transform (RCT): RGB -> (Y, Cb, Cr) (ISO 15444-1 G.2).
pub fn forward_rct(r: &[u8], g: &[u8], b: &[u8]) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let n = r.len();
    let mut y = Vec::with_capacity(n);
    let mut cb = Vec::with_capacity(n);
    let mut cr = Vec::with_capacity(n);
    for i in 0..n {
        let ri = r[i] as i32 - 128;
        let gi = g[i] as i32 - 128;
        let bi = b[i] as i32 - 128;
        let yi = (ri + 2 * gi + bi).div_euclid(4);
        let cbi = bi - gi;
        let cri = ri - gi;
        y.push(yi);
        cb.push(cbi);
        cr.push(cri);
    }
    (y, cb, cr)
}

/// Forward 1D reversible 5-3 lifting transform (even-origin `i0 = 0`).
/// Outputs `(low, high)` of lengths `((n + 1) / 2, n / 2)`.
pub fn forward_53_1d(src: &[i32]) -> (Vec<i32>, Vec<i32>) {
    let n = src.len();
    if n == 0 {
        return (Vec::new(), Vec::new());
    }
    if n == 1 {
        return (vec![src[0]], Vec::new());
    }
    let nl = n.div_ceil(2);
    let nh = n / 2;

    let mut h = vec![0i32; nh];
    for k in 0..nh {
        let left = src[2 * k];
        let right = if 2 * k + 2 < n {
            src[2 * k + 2]
        } else {
            src[2 * k]
        };
        h[k] = src[2 * k + 1] - ((left + right) >> 1);
    }

    let mut l = vec![0i32; nl];
    for k in 0..nl {
        let h_left = if k == 0 { h[0] } else { h[k - 1] };
        let h_right = if k < nh { h[k] } else { h[nh - 1] };
        l[k] = src[2 * k] + ((h_left + h_right + 2) >> 2);
    }
    (l, h)
}

/// Inverse 1D reversible 5-3 lifting transform (used in unit tests to verify exactness).
#[cfg(test)]
pub fn inverse_53_1d(l: &[i32], h: &[i32]) -> Vec<i32> {
    let nl = l.len();
    let nh = h.len();
    let n = nl + nh;
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return vec![l[0]];
    }
    let mut out = vec![0i32; n];
    for k in 0..nl {
        let h_left = if k == 0 { h[0] } else { h[k - 1] };
        let h_right = if k < nh { h[k] } else { h[nh - 1] };
        out[2 * k] = l[k] - ((h_left + h_right + 2) >> 2);
    }
    for k in 0..nh {
        let left = out[2 * k];
        let right = if 2 * k + 2 < n {
            out[2 * k + 2]
        } else {
            out[2 * k]
        };
        out[2 * k + 1] = h[k] + ((left + right) >> 1);
    }
    out
}

/// A 2D subband buffer.
#[derive(Clone, Debug)]
pub struct Subband {
    pub width: usize,
    pub height: usize,
    /// Orientation: 0 = LL, 1 = HL, 2 = LH, 3 = HH.
    pub orientation: u8,
    /// Resolution level (`0` = deepest LL, `1..=nl` = detail levels).
    pub level: u8,
    pub data: Vec<i32>,
}

/// Perform `levels` levels of forward 2D 5-3 DWT on a `width x height` component.
/// Returns subbands in codestream resolution order:
/// `[LL_0, HL_1, LH_1, HH_1, HL_2, LH_2, HH_2, ...]`
pub fn forward_53_2d(
    input: &[i32],
    width: usize,
    height: usize,
    levels: u8,
) -> Vec<Subband> {
    let mut ll = input.to_vec();
    let mut w = width;
    let mut h = height;
    let mut detail_levels: Vec<(Subband, Subband, Subband)> = Vec::with_capacity(levels as usize);

    for lvl in 0..levels {
        let nl_x = w.div_ceil(2);
        let nh_x = w / 2;
        let nl_y = h.div_ceil(2);
        let nh_y = h / 2;

        // Horizontal pass: split each row into L_x (nl_x) and H_x (nh_x).
        let mut lx = vec![0i32; nl_x * h];
        let mut hx = vec![0i32; nh_x * h];
        for y in 0..h {
            let row = &ll[y * w..(y + 1) * w];
            let (l, hi) = forward_53_1d(row);
            lx[y * nl_x..(y + 1) * nl_x].copy_from_slice(&l);
            if nh_x > 0 {
                hx[y * nh_x..(y + 1) * nh_x].copy_from_slice(&hi);
            }
        }

        // Vertical pass on L_x -> LL (nl_x x nl_y) and LH (nl_x x nh_y).
        let mut next_ll = vec![0i32; nl_x * nl_y];
        let mut lh = vec![0i32; nl_x * nh_y];
        let mut col = vec![0i32; h];
        for x in 0..nl_x {
            for y in 0..h {
                col[y] = lx[y * nl_x + x];
            }
            let (l, hi) = forward_53_1d(&col);
            for y in 0..nl_y {
                next_ll[y * nl_x + x] = l[y];
            }
            for y in 0..nh_y {
                lh[y * nl_x + x] = hi[y];
            }
        }

        // Vertical pass on H_x -> HL (nh_x x nl_y) and HH (nh_x x nh_y).
        let mut hl = vec![0i32; nh_x * nl_y];
        let mut hh = vec![0i32; nh_x * nh_y];
        for x in 0..nh_x {
            for y in 0..h {
                col[y] = hx[y * nh_x + x];
            }
            let (l, hi) = forward_53_1d(&col);
            for y in 0..nl_y {
                hl[y * nh_x + x] = l[y];
            }
            for y in 0..nh_y {
                hh[y * nh_x + x] = hi[y];
            }
        }

        let res_level = levels - lvl;
        detail_levels.push((
            Subband {
                width: nh_x,
                height: nl_y,
                orientation: 1, // HL
                level: res_level,
                data: hl,
            },
            Subband {
                width: nl_x,
                height: nh_y,
                orientation: 2, // LH
                level: res_level,
                data: lh,
            },
            Subband {
                width: nh_x,
                height: nh_y,
                orientation: 3, // HH
                level: res_level,
                data: hh,
            },
        ));

        ll = next_ll;
        w = nl_x;
        h = nl_y;
    }

    let mut out = Vec::with_capacity(1 + detail_levels.len() * 3);
    out.push(Subband {
        width: w,
        height: h,
        orientation: 0, // LL
        level: 0,
        data: ll,
    });
    for (hl, lh, hh) in detail_levels.into_iter().rev() {
        out.push(hl);
        out.push(lh);
        out.push(hh);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inverse_53_2d(subbands: &[Subband], width: usize, height: usize, levels: u8) -> Vec<i32> {
        let mut ll = subbands[0].data.clone();
        let mut w = subbands[0].width;
        let mut h = subbands[0].height;
        for lvl in 0..levels as usize {
            let hl = &subbands[1 + lvl * 3];
            let lh = &subbands[1 + lvl * 3 + 1];
            let hh = &subbands[1 + lvl * 3 + 2];
            let next_w = w + hl.width;
            let next_h = h + lh.height;

            // Vertical inverse on L_x (w x next_h) from LL (w x h) and LH (w x lh.height)
            let mut lx = vec![0i32; w * next_h];
            for x in 0..w {
                let l_col: Vec<i32> = (0..h).map(|y| ll[y * w + x]).collect();
                let h_col: Vec<i32> = (0..lh.height).map(|y| lh.data[y * w + x]).collect();
                let rec = inverse_53_1d(&l_col, &h_col);
                for y in 0..next_h {
                    lx[y * w + x] = rec[y];
                }
            }

            // Vertical inverse on H_x (hl.width x next_h) from HL and HH
            let mut hx = vec![0i32; hl.width * next_h];
            for x in 0..hl.width {
                let l_col: Vec<i32> = (0..hl.height).map(|y| hl.data[y * hl.width + x]).collect();
                let h_col: Vec<i32> = (0..hh.height).map(|y| hh.data[y * hh.width + x]).collect();
                let rec = inverse_53_1d(&l_col, &h_col);
                for y in 0..next_h {
                    hx[y * hl.width + x] = rec[y];
                }
            }

            // Horizontal inverse
            let mut out = vec![0i32; next_w * next_h];
            for y in 0..next_h {
                let l_row = &lx[y * w..(y + 1) * w];
                let h_row = &hx[y * hl.width..(y + 1) * hl.width];
                let rec = inverse_53_1d(l_row, h_row);
                out[y * next_w..(y + 1) * next_w].copy_from_slice(&rec);
            }
            ll = out;
            w = next_w;
            h = next_h;
        }
        assert_eq!((w, h), (width, height));
        ll
    }

    #[test]
    fn reversible_53_1d_and_2d_exact_for_odd_and_even_lengths() {
        for len in 1..=25 {
            let input: Vec<i32> = (0..len).map(|i| (i as i32 * 17) - 100).collect();
            let (l, h) = forward_53_1d(&input);
            let recon = inverse_53_1d(&l, &h);
            assert_eq!(recon, input, "failed for len={len}");
        }
        for (w, h, lvls) in [(8, 8, 1), (8, 8, 3), (17, 13, 3), (29, 23, 3)] {
            let input: Vec<i32> = (0..(w * h)).map(|i| ((i * 37) as i32) - 128).collect();
            let sbs = forward_53_2d(&input, w, h, lvls);
            let recon = inverse_53_2d(&sbs, w, h, lvls);
            assert_eq!(recon, input, "2D DWT failed at {w}x{h} lvls={lvls}");
        }
    }
}
