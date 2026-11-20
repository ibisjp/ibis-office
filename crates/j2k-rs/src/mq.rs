// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ISO/IEC 15444-1 Annex C MQ arithmetic encoder.

/// MQ probability estimation table (Table C.2 in ISO/IEC 15444-1).
/// Each entry: `(qe, nmps, nlps, switch_mps)`.
#[rustfmt::skip]
const QE_TABLE: [(u16, u8, u8, bool); 47] = [
    (0x5601, 1, 1, true),
    (0x3401, 2, 6, false),
    (0x1801, 3, 9, false),
    (0x0AC1, 4, 12, false),
    (0x0521, 5, 29, false),
    (0x0221, 38, 33, false),
    (0x5601, 7, 6, true),
    (0x5401, 8, 14, false),
    (0x4801, 9, 14, false),
    (0x3801, 10, 14, false),
    (0x3001, 11, 17, false),
    (0x2401, 12, 18, false),
    (0x1C01, 13, 20, false),
    (0x1601, 29, 21, false),
    (0x5601, 15, 14, true),
    (0x5401, 16, 14, false),
    (0x5101, 17, 15, false),
    (0x4801, 18, 16, false),
    (0x3801, 19, 17, false),
    (0x3401, 20, 18, false),
    (0x3001, 21, 19, false),
    (0x2801, 22, 19, false),
    (0x2401, 23, 20, false),
    (0x2201, 24, 21, false),
    (0x1C01, 25, 22, false),
    (0x1801, 26, 23, false),
    (0x1601, 27, 24, false),
    (0x1401, 28, 25, false),
    (0x1201, 29, 26, false),
    (0x1101, 30, 27, false),
    (0x0AC1, 31, 28, false),
    (0x09C1, 32, 29, false),
    (0x08A1, 33, 30, false),
    (0x0521, 34, 31, false),
    (0x0441, 35, 32, false),
    (0x02A1, 36, 33, false),
    (0x0221, 37, 34, false),
    (0x0141, 38, 35, false),
    (0x0111, 39, 36, false),
    (0x0085, 40, 37, false),
    (0x0049, 41, 38, false),
    (0x0025, 42, 39, false),
    (0x0015, 43, 40, false),
    (0x0009, 44, 41, false),
    (0x0005, 45, 42, false),
    (0x0001, 45, 43, false),
    (0x5601, 46, 46, false),
];

/// Context state for the MQ coder.
#[derive(Clone, Copy, Debug)]
pub struct MqContext {
    pub index: u8,
    pub mps: u8,
}

/// 19 EBCOT contexts initialized per ISO/IEC 15444-1 Table C.1 / E.1.
pub fn default_contexts() -> [MqContext; 19] {
    let mut ctxs = [MqContext { index: 0, mps: 0 }; 19];
    ctxs[0].index = 4; // ZC ctx 0
    ctxs[17].index = 3; // Run-length (AGG)
    ctxs[18].index = 46; // Uniform (UNI)
    ctxs
}

/// ISO/IEC 15444-1 MQ binary arithmetic encoder.
pub struct MqEncoder {
    a: u32,
    c: u32,
    ct: u32,
    buf: Vec<u8>,
    b: Option<u8>,
}

impl MqEncoder {
    pub fn new() -> Self {
        Self {
            a: 0x8000,
            c: 0,
            ct: 12,
            buf: Vec::with_capacity(256),
            b: None,
        }
    }

    #[inline]
    pub fn encode(&mut self, ctx: &mut MqContext, bit: u8) {
        if bit == ctx.mps {
            self.code_mps(ctx);
        } else {
            self.code_lps(ctx);
        }
    }

    #[inline]
    fn code_mps(&mut self, ctx: &mut MqContext) {
        let (qe, nmps, _, _) = QE_TABLE[ctx.index as usize];
        let qe = qe as u32;
        self.a -= qe;
        if (self.a & 0x8000) == 0 {
            if self.a < qe {
                self.a = qe;
            } else {
                self.c += qe;
            }
            ctx.index = nmps;
            self.renorm();
        } else {
            self.c += qe;
        }
    }

    #[inline]
    fn code_lps(&mut self, ctx: &mut MqContext) {
        let (qe, _, nlps, switch) = QE_TABLE[ctx.index as usize];
        let qe = qe as u32;
        self.a -= qe;
        if self.a < qe {
            self.c += qe;
        } else {
            self.a = qe;
        }
        if switch {
            ctx.mps ^= 1;
        }
        ctx.index = nlps;
        self.renorm();
    }

    #[inline]
    fn renorm(&mut self) {
        loop {
            self.a <<= 1;
            self.c <<= 1;
            self.ct -= 1;
            if self.ct == 0 {
                self.byte_out();
            }
            if (self.a & 0x8000) != 0 {
                break;
            }
        }
    }

    fn byte_out(&mut self) {
        match self.b {
            Some(0xFF) => {
                self.buf.push(0xFF);
                let new_b = (self.c >> 20) as u8;
                self.b = Some(new_b);
                self.c &= 0xFFFFF;
                self.ct = 7;
            }
            Some(prev) => {
                if (self.c & 0x8000000) != 0 {
                    let carried = prev.wrapping_add(1);
                    if carried == 0xFF {
                        self.c &= 0x7FFFFFF;
                        self.buf.push(0xFF);
                        let new_b = (self.c >> 20) as u8;
                        self.b = Some(new_b);
                        self.c &= 0xFFFFF;
                        self.ct = 7;
                    } else {
                        self.buf.push(carried);
                        let new_b = (self.c >> 19) as u8;
                        self.b = Some(new_b);
                        self.c &= 0x7FFFF;
                        self.ct = 8;
                    }
                } else {
                    self.buf.push(prev);
                    let new_b = (self.c >> 19) as u8;
                    self.b = Some(new_b);
                    self.c &= 0x7FFFF;
                    self.ct = 8;
                }
            }
            None => {
                // First byte: discard the dummy initial byte (ISO 15444-1 C.2.10).
                if (self.c & 0x8000000) != 0 {
                    // Carry into dummy byte is ignored; just mask.
                    self.c &= 0x7FFFFFF;
                }
                let new_b = (self.c >> 19) as u8;
                self.b = Some(new_b);
                self.c &= 0x7FFFF;
                self.ct = 8;
            }
        }
    }

    /// Flush per ISO/IEC 15444-1 Annex C.2.10 (`FLUSH`).
    pub fn finish(mut self) -> Vec<u8> {
        let tempc = self.c + self.a;
        self.c |= 0xFFFF;
        if self.c >= tempc {
            self.c -= 0x8000;
        }
        self.c <<= self.ct;
        self.byte_out();
        self.c <<= self.ct;
        self.byte_out();
        if let Some(last) = self.b
            && last != 0xFF
        {
            self.buf.push(last);
        }
        if self.buf.last() == Some(&0xFF) {
            self.buf.pop();
        }
        self.buf
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) struct TestMqDecoder<'a> {
        data: &'a [u8],
        bp: usize,
        c: u32,
        a: u32,
        ct: u32,
    }

    impl<'a> TestMqDecoder<'a> {
        pub(crate) fn new(data: &'a [u8]) -> Self {
            let mut d = Self {
                data,
                bp: 0,
                c: 0,
                a: 0x8000,
                ct: 0,
            };
            let b0 = d.data.first().copied().unwrap_or(0xFF) as u32;
            d.c = b0 << 16;
            d.byte_in();
            d.c <<= 7;
            d.ct -= 7;
            d.a = 0x8000;
            d
        }

        fn cur(&self) -> u8 {
            self.data.get(self.bp).copied().unwrap_or(0xFF)
        }

        fn next(&self) -> u8 {
            self.data.get(self.bp + 1).copied().unwrap_or(0xFF)
        }

        fn byte_in(&mut self) {
            if self.cur() == 0xFF {
                if self.next() > 0x8F {
                    self.c += 0xFF00;
                    self.ct = 8;
                } else {
                    self.bp += 1;
                    self.c += (self.cur() as u32) << 9;
                    self.ct = 7;
                }
            } else {
                self.bp += 1;
                self.c += (self.cur() as u32) << 8;
                self.ct = 8;
            }
        }

        pub(crate) fn decode(&mut self, ctx: &mut MqContext) -> u8 {
            let (qe, nmps, nlps, switch) = QE_TABLE[ctx.index as usize];
            let qe = qe as u32;
            self.a -= qe;
            let d;
            if (self.c >> 16) < qe {
                // LPS exchange? Standard ISO 15444-1 C.3.2
                if self.a < qe {
                    self.a = qe;
                    d = ctx.mps;
                    ctx.index = nmps;
                } else {
                    self.a = qe;
                    d = ctx.mps ^ 1;
                    if switch {
                        ctx.mps ^= 1;
                    }
                    ctx.index = nlps;
                }
                self.renorm();
            } else {
                self.c -= qe << 16;
                if (self.a & 0x8000) == 0 {
                    if self.a < qe {
                        d = ctx.mps ^ 1;
                        if switch {
                            ctx.mps ^= 1;
                        }
                        ctx.index = nlps;
                    } else {
                        d = ctx.mps;
                        ctx.index = nmps;
                    }
                    self.renorm();
                } else {
                    d = ctx.mps;
                }
            }
            d
        }

        fn renorm(&mut self) {
            loop {
                if self.ct == 0 {
                    self.byte_in();
                }
                self.a <<= 1;
                self.c <<= 1;
                self.ct -= 1;
                if (self.a & 0x8000) != 0 {
                    break;
                }
            }
        }
    }

    #[test]
    fn mq_roundtrips_arbitrary_sequences() {
        for seed in 1u32..=200 {
            let mut enc = MqEncoder::new();
            let mut enc_ctxs = default_contexts();
            let mut expected = Vec::new();
            let mut s = seed.wrapping_mul(747796405).wrapping_add(2891336453);
            let n = 10 + (seed % 300) as usize;
            for _ in 0..n {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                let ctx_idx = ((s >> 16) % 19) as usize;
                let bit = ((s >> 24) & 1) as u8;
                enc.encode(&mut enc_ctxs[ctx_idx], bit);
                expected.push((ctx_idx, bit));
            }
            let bytes = enc.finish();
            let mut dec = TestMqDecoder::new(&bytes);
            let mut dec_ctxs = default_contexts();
            for (i, (ctx_idx, bit)) in expected.into_iter().enumerate() {
                let got = dec.decode(&mut dec_ctxs[ctx_idx]);
                assert_eq!(got, bit, "seed={seed} step={i} bytes={bytes:02X?}");
            }
        }
    }
}
