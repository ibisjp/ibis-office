// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ISO/IEC 15444-1 Annex B Tier-2 packet header and tag-tree encoder.

use crate::ebcot::EncodedCodeBlock;

/// Bit-level writer with ISO/IEC 15444-1 B.10.1 packet-header byte stuffing.
pub struct PacketBitWriter {
    bytes: Vec<u8>,
    current: u8,
    bits_free: u8,
}

impl PacketBitWriter {
    pub fn new() -> Self {
        Self {
            bytes: Vec::with_capacity(32),
            current: 0,
            bits_free: 8,
        }
    }

    pub fn write_bit(&mut self, bit: u8) {
        self.bits_free -= 1;
        if bit & 1 != 0 {
            self.current |= 1 << self.bits_free;
        }
        if self.bits_free == 0 {
            let emitted = self.current;
            self.bytes.push(emitted);
            self.current = 0;
            // After 0xFF, the MSB of the next byte is a stuffed 0 bit.
            self.bits_free = if emitted == 0xFF { 7 } else { 8 };
        }
    }

    pub fn write_bits(&mut self, val: u32, count: u32) {
        for i in (0..count).rev() {
            self.write_bit(((val >> i) & 1) as u8);
        }
    }

    pub fn finish(mut self) -> Vec<u8> {
        if self.bits_free < 8 || self.bytes.last() == Some(&0xFF) {
            self.bytes.push(self.current);
        }
        self.bytes
    }
}

/// Tag-tree encoder for a `width x height` code-block grid (ISO/IEC 15444-1 B.10.2).
struct TagTree {
    levels: Vec<(usize, usize, Vec<u32>, Vec<u32>, Vec<bool>)>,
}

impl TagTree {
    fn new(width: usize, height: usize, values: &[u32]) -> Self {
        let mut levels: Vec<(usize, usize, Vec<u32>, Vec<u32>, Vec<bool>)> = Vec::new();
        let mut w = width;
        let mut h = height;
        let mut cur_vals = values.to_vec();

        loop {
            let len = w * h;
            levels.push((w, h, cur_vals.clone(), vec![0; len], vec![false; len]));
            if w <= 1 && h <= 1 {
                break;
            }
            let nw = w.div_ceil(2);
            let nh = h.div_ceil(2);
            let mut next_vals = vec![u32::MAX; nw * nh];
            for y in 0..h {
                for x in 0..w {
                    let p_idx = (y / 2) * nw + (x / 2);
                    next_vals[p_idx] = next_vals[p_idx].min(cur_vals[y * w + x]);
                }
            }
            w = nw;
            h = nh;
            cur_vals = next_vals;
        }
        Self { levels }
    }

    /// Encode `(x, y)` up to `threshold`.
    fn encode(&mut self, w_writer: &mut PacketBitWriter, x: usize, y: usize, threshold: u32) {
        let num_levels = self.levels.len();
        let mut coords = Vec::with_capacity(num_levels);
        let mut cx = x;
        let mut cy = y;
        for _ in 0..num_levels {
            coords.push((cx, cy));
            cx /= 2;
            cy /= 2;
        }

        let mut low = 0u32;
        for lvl in (0..num_levels).rev() {
            let (lw, _, ref vals, ref mut states, ref mut known) = self.levels[lvl];
            let (lx, ly) = coords[lvl];
            let idx = ly * lw + lx;
            if states[idx] < low {
                states[idx] = low;
            }
            while states[idx] <= threshold && !known[idx] {
                if states[idx] == vals[idx] {
                    w_writer.write_bit(1);
                    known[idx] = true;
                } else {
                    w_writer.write_bit(0);
                    states[idx] += 1;
                }
            }
            low = states[idx];
        }
    }
}

/// Write a single-layer LRCP packet for one resolution level across subbands.
/// Each subband entry is `(cb_cols, cb_rows, &[EncodedCodeBlock])`.
pub fn write_packet(subbands: &[(usize, usize, &[EncodedCodeBlock])]) -> Vec<u8> {
    let any_non_empty = subbands
        .iter()
        .any(|(_, _, cbs)| cbs.iter().any(|cb| cb.num_passes > 0));
    let mut pw = PacketBitWriter::new();
    if !any_non_empty {
        pw.write_bit(0);
        return pw.finish();
    }
    pw.write_bit(1);

    let mut body = Vec::new();
    for &(cols, rows, cbs) in subbands {
        if cols == 0 || rows == 0 {
            continue;
        }
        let incl_vals: Vec<u32> = cbs
            .iter()
            .map(|cb| if cb.num_passes > 0 { 0 } else { 1 })
            .collect();
        let zb_vals: Vec<u32> = cbs.iter().map(|cb| cb.zero_bitplanes).collect();
        let mut incl_tree = TagTree::new(cols, rows, &incl_vals);
        let mut zb_tree = TagTree::new(cols, rows, &zb_vals);

        for y in 0..rows {
            for x in 0..cols {
                let idx = y * cols + x;
                let cb = &cbs[idx];
                incl_tree.encode(&mut pw, x, y, 0);
                if cb.num_passes == 0 {
                    continue;
                }
                zb_tree.encode(&mut pw, x, y, cb.zero_bitplanes);
                write_num_passes(&mut pw, cb.num_passes);

                let len = cb.coded_bytes.len() as u32;
                let pass_bits = 31 - cb.num_passes.leading_zeros();
                let needed_bits = if len == 0 { 1 } else { 32 - len.leading_zeros() };
                let lblock_base = 3 + pass_bits;
                let inc = needed_bits.saturating_sub(lblock_base);
                for _ in 0..inc {
                    pw.write_bit(1);
                }
                pw.write_bit(0);
                let length_bits = lblock_base + inc;
                pw.write_bits(len, length_bits);

                body.extend_from_slice(&cb.coded_bytes);
            }
        }
    }

    let mut out = pw.finish();
    out.extend_from_slice(&body);
    out
}

/// Write coding passes count per ISO/IEC 15444-1 Table B.4.
fn write_num_passes(pw: &mut PacketBitWriter, n: u32) {
    if n == 1 {
        pw.write_bit(0);
    } else if n == 2 {
        pw.write_bits(0b10, 2);
    } else if n <= 5 {
        pw.write_bits(0b11, 2);
        pw.write_bits(n - 3, 2);
    } else if n <= 36 {
        pw.write_bits(0b1111, 4);
        pw.write_bits(n - 6, 5);
    } else {
        let clamped = n.min(164);
        pw.write_bits(0b111111111, 9);
        pw.write_bits(clamped - 37, 7);
    }
}
