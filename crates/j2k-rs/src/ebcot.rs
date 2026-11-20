// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ISO/IEC 15444-1 Annex D Tier-1 EBCOT code-block bit-plane encoder.

use crate::mq::{MqEncoder, default_contexts};

/// Result of Tier-1 encoding a single code-block.
#[derive(Clone, Debug)]
pub struct EncodedCodeBlock {
    pub num_passes: u32,
    pub zero_bitplanes: u32,
    pub coded_bytes: Vec<u8>,
}

/// Subband orientation for Zero Coding / Sign Coding LUTs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BandOrient {
    Ll,
    Hl,
    Lh,
    Hh,
}

impl BandOrient {
    pub fn from_u8(o: u8) -> Self {
        match o {
            0 => Self::Ll,
            1 => Self::Hl,
            2 => Self::Lh,
            _ => Self::Hh,
        }
    }
}

const SIG_FLAG: u8 = 1 << 0;
const SIGN_FLAG: u8 = 1 << 1;
const REFINED_FLAG: u8 = 1 << 2;
const CODED_THIS_BP: u8 = 1 << 3;

/// Encode a single code-block of wavelet coefficients (`width x height`)
/// given `mb` total magnitude bitplanes from `QCD`.
pub fn encode_codeblock(
    coeffs: &[i32],
    width: usize,
    height: usize,
    orient: BandOrient,
    mb: u32,
) -> EncodedCodeBlock {
    if width == 0 || height == 0 {
        return EncodedCodeBlock {
            num_passes: 0,
            zero_bitplanes: 0,
            coded_bytes: Vec::new(),
        };
    }

    let mut max_abs = 0u32;
    let mut mags = Vec::with_capacity(width * height);
    let mut signs = Vec::with_capacity(width * height);
    for &c in coeffs {
        let a = c.unsigned_abs();
        if a > max_abs {
            max_abs = a;
        }
        mags.push(a);
        signs.push(if c < 0 { 1u8 } else { 0u8 });
    }

    if max_abs == 0 {
        return EncodedCodeBlock {
            num_passes: 0,
            zero_bitplanes: 0,
            coded_bytes: Vec::new(),
        };
    }

    let active_bits = 32 - max_abs.leading_zeros();
    let zero_bitplanes = mb.saturating_sub(active_bits);

    let mut state = vec![0u8; width * height];
    let mut mq = MqEncoder::new();
    let mut ctxs = default_contexts();
    let mut num_passes = 0u32;

    // Top bitplane: Cleanup pass only.
    let top_bp = active_bits - 1;
    cleanup_pass(
        &mags,
        &signs,
        &mut state,
        width,
        height,
        orient,
        top_bp,
        &mut mq,
        &mut ctxs,
    );
    num_passes += 1;

    // Remaining bitplanes: SPP, MRP, Cleanup.
    for bp in (0..top_bp).rev() {
        for s in &mut state {
            *s &= !CODED_THIS_BP;
        }
        sig_prop_pass(
            &mags,
            &signs,
            &mut state,
            width,
            height,
            orient,
            bp,
            &mut mq,
            &mut ctxs,
        );
        num_passes += 1;

        mag_ref_pass(
            &mags,
            &mut state,
            width,
            height,
            bp,
            &mut mq,
            &mut ctxs,
        );
        num_passes += 1;

        cleanup_pass(
            &mags,
            &signs,
            &mut state,
            width,
            height,
            orient,
            bp,
            &mut mq,
            &mut ctxs,
        );
        num_passes += 1;
    }

    let coded_bytes = mq.finish();
    EncodedCodeBlock {
        num_passes,
        zero_bitplanes,
        coded_bytes,
    }
}

#[allow(clippy::too_many_arguments)]
fn sig_prop_pass(
    mags: &[u32],
    signs: &[u8],
    state: &mut [u8],
    w: usize,
    h: usize,
    orient: BandOrient,
    bp: u32,
    mq: &mut MqEncoder,
    ctxs: &mut [crate::mq::MqContext; 19],
) {
    let mask = 1u32 << bp;
    for y_stripe in (0..h).step_by(4) {
        let y_end = (y_stripe + 4).min(h);
        for x in 0..w {
            for y in y_stripe..y_end {
                let idx = y * w + x;
                if (state[idx] & SIG_FLAG) != 0 {
                    continue;
                }
                let nb = neighbor_sig_counts(state, w, h, x, y);
                if nb.total() == 0 {
                    continue;
                }
                let zc_ctx = zc_context(orient, nb);
                let bit = u8::from((mags[idx] & mask) != 0);
                mq.encode(&mut ctxs[zc_ctx], bit);
                state[idx] |= CODED_THIS_BP;
                if bit == 1 {
                    let (sc_ctx, xor) = sc_context(state, signs, w, h, x, y);
                    mq.encode(&mut ctxs[sc_ctx], signs[idx] ^ xor);
                    state[idx] |= SIG_FLAG;
                    if signs[idx] != 0 {
                        state[idx] |= SIGN_FLAG;
                    }
                }
            }
        }
    }
}

fn mag_ref_pass(
    mags: &[u32],
    state: &mut [u8],
    w: usize,
    h: usize,
    bp: u32,
    mq: &mut MqEncoder,
    ctxs: &mut [crate::mq::MqContext; 19],
) {
    let mask = 1u32 << bp;
    for y_stripe in (0..h).step_by(4) {
        let y_end = (y_stripe + 4).min(h);
        for x in 0..w {
            for y in y_stripe..y_end {
                let idx = y * w + x;
                if (state[idx] & SIG_FLAG) == 0 || (state[idx] & CODED_THIS_BP) != 0 {
                    continue;
                }
                let mr_ctx = if (state[idx] & REFINED_FLAG) != 0 {
                    16
                } else if neighbor_sig_counts(state, w, h, x, y).total() > 0 {
                    15
                } else {
                    14
                };
                let bit = u8::from((mags[idx] & mask) != 0);
                mq.encode(&mut ctxs[mr_ctx], bit);
                state[idx] |= REFINED_FLAG;
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cleanup_pass(
    mags: &[u32],
    signs: &[u8],
    state: &mut [u8],
    w: usize,
    h: usize,
    orient: BandOrient,
    bp: u32,
    mq: &mut MqEncoder,
    ctxs: &mut [crate::mq::MqContext; 19],
) {
    let mask = 1u32 << bp;
    for y_stripe in (0..h).step_by(4) {
        let y_end = (y_stripe + 4).min(h);
        let full_stripe = y_end - y_stripe == 4;
        for x in 0..w {
            let mut y_start = y_stripe;
            if full_stripe {
                let can_rl = (0..4).all(|dy| {
                    let y = y_stripe + dy;
                    let idx = y * w + x;
                    (state[idx] & (SIG_FLAG | CODED_THIS_BP)) == 0
                        && neighbor_sig_counts(state, w, h, x, y).total() == 0
                });
                if can_rl {
                    let first_sig = (0..4).find(|&dy| (mags[(y_stripe + dy) * w + x] & mask) != 0);
                    match first_sig {
                        None => {
                            mq.encode(&mut ctxs[17], 0);
                            continue;
                        }
                        Some(dy) => {
                            mq.encode(&mut ctxs[17], 1);
                            mq.encode(&mut ctxs[18], ((dy >> 1) & 1) as u8);
                            mq.encode(&mut ctxs[18], (dy & 1) as u8);
                            let y = y_stripe + dy;
                            let idx = y * w + x;
                            let (sc_ctx, xor) = sc_context(state, signs, w, h, x, y);
                            mq.encode(&mut ctxs[sc_ctx], signs[idx] ^ xor);
                            state[idx] |= SIG_FLAG;
                            if signs[idx] != 0 {
                                state[idx] |= SIGN_FLAG;
                            }
                            y_start = y + 1;
                        }
                    }
                }
            }

            for y in y_start..y_end {
                let idx = y * w + x;
                if (state[idx] & (SIG_FLAG | CODED_THIS_BP)) != 0 {
                    continue;
                }
                let nb = neighbor_sig_counts(state, w, h, x, y);
                let zc_ctx = zc_context(orient, nb);
                let bit = u8::from((mags[idx] & mask) != 0);
                mq.encode(&mut ctxs[zc_ctx], bit);
                if bit == 1 {
                    let (sc_ctx, xor) = sc_context(state, signs, w, h, x, y);
                    mq.encode(&mut ctxs[sc_ctx], signs[idx] ^ xor);
                    state[idx] |= SIG_FLAG;
                    if signs[idx] != 0 {
                        state[idx] |= SIGN_FLAG;
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Neighbors {
    h: u8,
    v: u8,
    d: u8,
}

impl Neighbors {
    #[inline]
    fn total(self) -> u8 {
        self.h + self.v + self.d
    }
}

#[inline]
fn is_sig(state: &[u8], w: usize, h: usize, x: isize, y: isize) -> u8 {
    if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
        return 0;
    }
    u8::from((state[y as usize * w + x as usize] & SIG_FLAG) != 0)
}

#[inline]
fn neighbor_sig_counts(state: &[u8], w: usize, h: usize, x: usize, y: usize) -> Neighbors {
    let xi = x as isize;
    let yi = y as isize;
    let hc = is_sig(state, w, h, xi - 1, yi) + is_sig(state, w, h, xi + 1, yi);
    let vc = is_sig(state, w, h, xi, yi - 1) + is_sig(state, w, h, xi, yi + 1);
    let dc = is_sig(state, w, h, xi - 1, yi - 1)
        + is_sig(state, w, h, xi + 1, yi - 1)
        + is_sig(state, w, h, xi - 1, yi + 1)
        + is_sig(state, w, h, xi + 1, yi + 1);
    Neighbors { h: hc, v: vc, d: dc }
}

/// Zero Coding context index `0..=8` per ISO/IEC 15444-1 Table D.1.
#[inline]
fn zc_context(orient: BandOrient, nb: Neighbors) -> usize {
    match orient {
        BandOrient::Ll | BandOrient::Lh => {
            if nb.h == 2 {
                8
            } else if nb.h == 1 {
                if nb.v >= 1 {
                    7
                } else if nb.d >= 1 {
                    6
                } else {
                    5
                }
            } else if nb.v == 2 {
                4
            } else if nb.v == 1 {
                3
            } else if nb.d >= 2 {
                2
            } else if nb.d == 1 {
                1
            } else {
                0
            }
        }
        BandOrient::Hl => {
            if nb.v == 2 {
                8
            } else if nb.v == 1 {
                if nb.h >= 1 {
                    7
                } else if nb.d >= 1 {
                    6
                } else {
                    5
                }
            } else if nb.h == 2 {
                4
            } else if nb.h == 1 {
                3
            } else if nb.d >= 2 {
                2
            } else if nb.d == 1 {
                1
            } else {
                0
            }
        }
        BandOrient::Hh => {
            let hv = nb.h + nb.v;
            if nb.d >= 3 {
                8
            } else if nb.d == 2 {
                if hv >= 1 { 7 } else { 6 }
            } else if nb.d == 1 {
                if hv >= 2 {
                    5
                } else if hv == 1 {
                    4
                } else {
                    3
                }
            } else if hv >= 2 {
                2
            } else if hv == 1 {
                1
            } else {
                0
            }
        }
    }
}

#[inline]
fn neighbor_sign_contrib(state: &[u8], signs: &[u8], w: usize, h: usize, x: isize, y: isize) -> i8 {
    if x < 0 || y < 0 || x >= w as isize || y >= h as isize {
        return 0;
    }
    let idx = y as usize * w + x as usize;
    if (state[idx] & SIG_FLAG) == 0 {
        0
    } else if signs[idx] != 0 {
        -1
    } else {
        1
    }
}

/// Sign Coding context (`9..=13`) and XOR prediction bit per ISO/IEC 15444-1 Table D.2/D.3.
#[inline]
fn sc_context(
    state: &[u8],
    signs: &[u8],
    w: usize,
    h: usize,
    x: usize,
    y: usize,
) -> (usize, u8) {
    let xi = x as isize;
    let yi = y as isize;
    let hc = (neighbor_sign_contrib(state, signs, w, h, xi - 1, yi)
        + neighbor_sign_contrib(state, signs, w, h, xi + 1, yi))
    .clamp(-1, 1);
    let vc = (neighbor_sign_contrib(state, signs, w, h, xi, yi - 1)
        + neighbor_sign_contrib(state, signs, w, h, xi, yi + 1))
    .clamp(-1, 1);

    let (ctx, xor) = match (hc, vc) {
        (1, 1) => (13, 0),
        (1, 0) => (12, 0),
        (1, -1) => (11, 0),
        (0, 1) => (10, 0),
        (0, 0) => (9, 0),
        (0, -1) => (10, 1),
        (-1, 1) => (11, 1),
        (-1, 0) => (12, 1),
        (-1, -1) => (13, 1),
        _ => (9, 0),
    };
    (ctx, xor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mq::tests::TestMqDecoder;

    pub(crate) fn decode_codeblock_test(
        bytes: &[u8],
        w: usize,
        h: usize,
        orient: BandOrient,
        mb: u32,
        zero_bitplanes: u32,
        num_passes: u32,
    ) -> Vec<i32> {
        if num_passes == 0 {
            return vec![0; w * h];
        }
        let active_bits = mb - zero_bitplanes;
        let mut mags = vec![0u32; w * h];
        let mut signs = vec![0u8; w * h];
        let mut state = vec![0u8; w * h];
        let mut mq = TestMqDecoder::new(bytes);
        let mut ctxs = default_contexts();
        let mut pass_idx = 0u32;

        for bp in (0..active_bits).rev() {
            if pass_idx >= num_passes {
                break;
            }
            if pass_idx > 0 {
                // SPP
                for s in &mut state {
                    *s &= !CODED_THIS_BP;
                }
                for y_stripe in (0..h).step_by(4) {
                    let y_end = (y_stripe + 4).min(h);
                    for x in 0..w {
                        for y in y_stripe..y_end {
                            let idx = y * w + x;
                            if (state[idx] & SIG_FLAG) != 0 {
                                continue;
                            }
                            let nb = neighbor_sig_counts(&state, w, h, x, y);
                            if nb.total() == 0 {
                                continue;
                            }
                            let zc_ctx = zc_context(orient, nb);
                            let bit = mq.decode(&mut ctxs[zc_ctx]);
                            state[idx] |= CODED_THIS_BP;
                            if bit == 1 {
                                mags[idx] |= 1 << bp;
                                let (sc_ctx, xor) = sc_context(&state, &signs, w, h, x, y);
                                signs[idx] = mq.decode(&mut ctxs[sc_ctx]) ^ xor;
                                state[idx] |= SIG_FLAG;
                            }
                        }
                    }
                }
                pass_idx += 1;
                if pass_idx >= num_passes {
                    break;
                }
                // MRP
                for y_stripe in (0..h).step_by(4) {
                    let y_end = (y_stripe + 4).min(h);
                    for x in 0..w {
                        for y in y_stripe..y_end {
                            let idx = y * w + x;
                            if (state[idx] & SIG_FLAG) == 0 || (state[idx] & CODED_THIS_BP) != 0 {
                                continue;
                            }
                            let mr_ctx = if (state[idx] & REFINED_FLAG) != 0 {
                                16
                            } else if neighbor_sig_counts(&state, w, h, x, y).total() > 0 {
                                15
                            } else {
                                14
                            };
                            let bit = mq.decode(&mut ctxs[mr_ctx]);
                            if bit == 1 {
                                mags[idx] |= 1 << bp;
                            }
                            state[idx] |= REFINED_FLAG;
                        }
                    }
                }
                pass_idx += 1;
                if pass_idx >= num_passes {
                    break;
                }
            }
            // Cleanup
            for y_stripe in (0..h).step_by(4) {
                let y_end = (y_stripe + 4).min(h);
                let full_stripe = y_end - y_stripe == 4;
                for x in 0..w {
                    let mut y_start = y_stripe;
                    if full_stripe {
                        let can_rl = (0..4).all(|dy| {
                            let y = y_stripe + dy;
                            let idx = y * w + x;
                            (state[idx] & (SIG_FLAG | CODED_THIS_BP)) == 0
                                && neighbor_sig_counts(&state, w, h, x, y).total() == 0
                        });
                        if can_rl {
                            let rl = mq.decode(&mut ctxs[17]);
                            if rl == 0 {
                                continue;
                            }
                            let b1 = mq.decode(&mut ctxs[18]);
                            let b0 = mq.decode(&mut ctxs[18]);
                            let dy = ((b1 << 1) | b0) as usize;
                            let y = y_stripe + dy;
                            let idx = y * w + x;
                            mags[idx] |= 1 << bp;
                            let (sc_ctx, xor) = sc_context(&state, &signs, w, h, x, y);
                            signs[idx] = mq.decode(&mut ctxs[sc_ctx]) ^ xor;
                            state[idx] |= SIG_FLAG;
                            y_start = y + 1;
                        }
                    }
                    for y in y_start..y_end {
                        let idx = y * w + x;
                        if (state[idx] & (SIG_FLAG | CODED_THIS_BP)) != 0 {
                            continue;
                        }
                        let nb = neighbor_sig_counts(&state, w, h, x, y);
                        let zc_ctx = zc_context(orient, nb);
                        let bit = mq.decode(&mut ctxs[zc_ctx]);
                        if bit == 1 {
                            mags[idx] |= 1 << bp;
                            let (sc_ctx, xor) = sc_context(&state, &signs, w, h, x, y);
                            signs[idx] = mq.decode(&mut ctxs[sc_ctx]) ^ xor;
                            state[idx] |= SIG_FLAG;
                        }
                    }
                }
            }
            pass_idx += 1;
        }
        mags.into_iter()
            .zip(signs)
            .map(|(m, s)| if s != 0 { -(m as i32) } else { m as i32 })
            .collect()
    }

    #[test]
    fn ebcot_roundtrips_xor_subbands() {
        let xor_img: Vec<i32> = (0..64).map(|i| (((i % 8) ^ (i / 8)) * 17) as i32 - 128).collect();
        let sbs = crate::dwt::forward_53_2d(&xor_img, 8, 8, 1);
        for sb in &sbs {
            let mb = 9 + match sb.orientation { 0 => 0, 1 | 2 => 1, _ => 2 };
            let orient = BandOrient::from_u8(sb.orientation);
            let cb = encode_codeblock(&sb.data, sb.width, sb.height, orient, mb);
            let dec = decode_codeblock_test(&cb.coded_bytes, sb.width, sb.height, orient, mb, cb.zero_bitplanes, cb.num_passes);
            assert_eq!(dec, sb.data, "orient={}", sb.orientation);
        }
    }
}
