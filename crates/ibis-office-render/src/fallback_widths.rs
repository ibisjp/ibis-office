// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Advance widths of a typeface that this PC does not have, for when another font stands in for it.
//!
//! A substitute font (Hiragino Sans on macOS, a bundled font elsewhere) does not set characters as wide as the typeface
//! the document names, so a line breaks in another place than in Office. The widths of a Japanese typeface that comes
//! with Windows and Office are facts of its font file (`hmtx`, `unitsPerEm = 2048`), listed here so that every
//! platform sets the line as Windows does. The Windows fonts themselves are not bundled.
//!
//! `UD デジタル 教科書体 NK` (`UDDigiKyokashoN-R.ttc` index 2, and `-B` for Bold; Windows 10 and later, Office): the
//! NK family sets its kana, its full-width space and punctuation proportionally (the ideographs stay 1 em). In
//! PowerPoint's PDF of pptx 043 (24 pt text) the full-width space is 12 pt (0.5 em), the circled digit 24 pt, and
//! `桃太郎は、それぞれ何％善か悪かを` 354.6 pt (14.78 em, against 16 em at the full width), which is what these
//! widths add up to. The `N` family (all full-width) and `NP` (proportional Latin too) are other fonts of the file
//! and are not listed.

/// Advance widths from a font's `hmtx`, in `1/2048` em.
#[derive(Debug)]
pub(crate) struct AdvanceTable {
    /// `(first code point, count, advance)`: runs of characters whose advance is not 1 em, sorted by code point.
    runs: &'static [(u32, u16, u16)],
    /// Blocks in which a character that is not in `runs` is 1 em wide (the font is full-width there).
    blocks: &'static [(u32, u32)],
}

const UNITS_PER_EM: f32 = 2048.0;

impl AdvanceTable {
    /// The advance of `c` in ems, `None` when the table does not say (the font's own glyph for it is not known).
    pub fn advance(&self, c: char) -> Option<f32> {
        let u = c as u32;
        let at = self.runs.partition_point(|&(start, _, _)| start <= u);
        if let Some(&(start, count, width)) = at.checked_sub(1).and_then(|i| self.runs.get(i))
            && u < start + u32::from(count)
        {
            return Some(f32::from(width) / UNITS_PER_EM);
        }
        self.blocks.iter().any(|&(a, b)| (a..=b).contains(&u)).then_some(1.0)
    }
}

/// Hiragana and Katakana with the CJK symbols, the full-width forms (without the half-width kana) and the full-width signs.
const FULL_WIDTH_BLOCKS: &[(u32, u32)] = &[(0x3000, 0x30ff), (0xff01, 0xff5e), (0xffe0, 0xffe5)];

/// The table for the typeface `name` (as a document names it) in regular or bold, when the typeface is one listed here.
pub(crate) fn fallback_advance_table(name: &str, bold: bool) -> Option<&'static AdvanceTable> {
    let n = name.trim();
    let family = n.strip_prefix("UD デジタル 教科書体 NK").or_else(|| n.strip_prefix("UD Digi Kyokasho NK"))?; // i18n-ok（フォント名の照合に使う語（フォント名そのもの）。訳さない）
    // "-R" and "-B" are the regular and bold files; the family alone is regular, and bold when the run is bold.
    let bold = match family.trim() {
        "-B" => true,
        "-R" => false,
        _ => bold,
    };
    Some(if bold { &UD_KYOKASHO_NK_BOLD } else { &UD_KYOKASHO_NK_REGULAR })
}

static UD_KYOKASHO_NK_REGULAR: AdvanceTable = AdvanceTable { runs: UD_KYOKASHO_NK_R_RUNS, blocks: FULL_WIDTH_BLOCKS };
static UD_KYOKASHO_NK_BOLD: AdvanceTable = AdvanceTable { runs: UD_KYOKASHO_NK_B_RUNS, blocks: FULL_WIDTH_BLOCKS };

const UD_KYOKASHO_NK_R_RUNS: &[(u32, u16, u16)] = &[
    (0x2010, 1, 1126), (0x2016, 1, 1434), (0x2018, 2, 799), (0x201c, 2, 1188), (0x2032, 1, 573), (0x2033, 1, 911), (0x203e, 1, 1024),
    (0x20ac, 1, 1352), (0x2103, 1, 1915), (0x2212, 1, 1556), (0x221d, 2, 1802), (0x2225, 1, 1352), (0x2234, 2, 1556), (0x223d, 1, 1802),
    (0x2252, 1, 1556), (0x2260, 2, 1556), (0x2266, 2, 1556), (0x226a, 2, 1556), (0x2640, 1, 1597), (0x2642, 1, 1987), (0x3000, 3, 1024),
    (0x3003, 1, 1475), (0x3005, 1, 1700), (0x3006, 1, 1659), (0x3007, 1, 1720), (0x3008, 2, 1270), (0x300a, 2, 1331), (0x300c, 2, 1270),
    (0x300e, 2, 1331), (0x3010, 2, 1270), (0x3014, 2, 1270), (0x301c, 1, 1679), (0x301d, 1, 1085), (0x301f, 1, 1085), (0x3041, 1, 1536),
    (0x3042, 1, 1884), (0x3043, 1, 1659), (0x3044, 1, 1905), (0x3045, 1, 1270), (0x3046, 1, 1495), (0x3047, 1, 1413), (0x3048, 1, 1741),
    (0x3049, 1, 1577), (0x304a, 1, 1884), (0x304b, 1, 2007), (0x304d, 1, 1659), (0x304e, 1, 1802), (0x304f, 1, 1352), (0x3050, 1, 1659),
    (0x3051, 1, 1802), (0x3052, 1, 1966), (0x3053, 1, 1618), (0x3054, 1, 1823), (0x3055, 1, 1659), (0x3056, 1, 1884), (0x3057, 2, 1597),
    (0x3059, 1, 1802), (0x305a, 1, 1925), (0x305b, 1, 1966), (0x305c, 1, 2007), (0x305d, 1, 1720), (0x305e, 1, 1802), (0x305f, 2, 1884),
    (0x3061, 1, 1741), (0x3062, 1, 1843), (0x3063, 1, 1536), (0x3064, 1, 1946), (0x3065, 1, 1966), (0x3066, 1, 1761), (0x3067, 1, 2007),
    (0x3068, 1, 1577), (0x3069, 1, 1823), (0x306a, 2, 1864), (0x306c, 1, 1987), (0x306d, 2, 1946), (0x306f, 1, 1884), (0x3070, 1, 2007),
    (0x3071, 1, 1987), (0x3072, 1, 1925), (0x3073, 2, 1987), (0x3075, 6, 1966), (0x307b, 1, 1905), (0x307c, 1, 2028), (0x307e, 1, 1679),
    (0x307f, 1, 1925), (0x3080, 1, 1905), (0x3081, 1, 1966), (0x3082, 1, 1679), (0x3083, 1, 1577), (0x3084, 1, 1925), (0x3085, 1, 1618),
    (0x3086, 1, 1966), (0x3087, 1, 1454), (0x3088, 1, 1700), (0x3089, 1, 1638), (0x308a, 1, 1516), (0x308b, 1, 1700), (0x308c, 1, 1966),
    (0x308d, 1, 1720), (0x308e, 1, 1618), (0x308f, 1, 1966), (0x3090, 1, 1925), (0x3091, 1, 1946), (0x3092, 1, 1782), (0x3093, 1, 1864),
    (0x3094, 1, 1700), (0x3095, 1, 1761), (0x3096, 1, 1577), (0x309b, 1, 492), (0x309c, 1, 369), (0x309d, 1, 1393), (0x309e, 1, 1475),
    (0x30a1, 1, 1495), (0x30a2, 1, 1802), (0x30a3, 1, 1311), (0x30a4, 1, 1556), (0x30a5, 1, 1475), (0x30a6, 1, 1761), (0x30a7, 1, 1597),
    (0x30a8, 1, 1925), (0x30a9, 1, 1475), (0x30aa, 1, 1761), (0x30ab, 1, 1802), (0x30ac, 1, 1946), (0x30ad, 1, 1802), (0x30ae, 1, 1884),
    (0x30af, 1, 1597), (0x30b0, 1, 1884), (0x30b1, 1, 1802), (0x30b2, 1, 1884), (0x30b3, 1, 1720), (0x30b4, 1, 1864), (0x30b5, 1, 1925),
    (0x30b6, 1, 2007), (0x30b7, 1, 1802), (0x30b8, 1, 1833), (0x30b9, 1, 1782), (0x30ba, 1, 1905), (0x30bb, 1, 1884), (0x30bc, 1, 1987),
    (0x30bd, 1, 1659), (0x30be, 1, 1905), (0x30bf, 1, 1638), (0x30c0, 1, 1905), (0x30c1, 1, 1864), (0x30c2, 1, 1946), (0x30c3, 1, 1495),
    (0x30c4, 1, 1802), (0x30c5, 1, 1987), (0x30c6, 1, 1843), (0x30c7, 1, 1987), (0x30c8, 1, 1331), (0x30c9, 1, 1475), (0x30ca, 1, 1843),
    (0x30cb, 1, 1946), (0x30cc, 1, 1597), (0x30cd, 1, 1823), (0x30ce, 1, 1454), (0x30cf, 3, 2007), (0x30d2, 1, 1638), (0x30d3, 1, 1782),
    (0x30d4, 1, 1802), (0x30d5, 1, 1618), (0x30d6, 2, 1884), (0x30d8, 3, 2007), (0x30db, 1, 1843), (0x30dc, 1, 1925), (0x30dd, 1, 1905),
    (0x30de, 1, 1843), (0x30df, 1, 1516), (0x30e0, 1, 1761), (0x30e1, 1, 1536), (0x30e2, 1, 1823), (0x30e3, 1, 1495), (0x30e4, 1, 1843),
    (0x30e5, 1, 1597), (0x30e6, 1, 1946), (0x30e7, 1, 1331), (0x30e8, 1, 1597), (0x30e9, 1, 1679), (0x30ea, 1, 1475), (0x30eb, 1, 1905),
    (0x30ec, 1, 1618), (0x30ed, 1, 1802), (0x30ee, 1, 1434), (0x30ef, 1, 1720), (0x30f0, 1, 1966), (0x30f1, 1, 1925), (0x30f2, 1, 1577),
    (0x30f3, 1, 1700), (0x30f4, 1, 1925), (0x30f5, 1, 1536), (0x30f6, 1, 1495), (0x30f7, 1, 1946), (0x30f8, 1, 1987), (0x30f9, 1, 1966),
    (0x30fa, 1, 1843), (0x30fb, 1, 1024), (0x30fc, 1, 1884), (0x30fd, 1, 1434), (0x30fe, 1, 1516), (0xff01, 1, 983), (0xff02, 1, 901),
    (0xff03, 1, 1638), (0xff04, 1, 1352), (0xff05, 1, 1884), (0xff06, 1, 1679), (0xff07, 1, 573), (0xff08, 2, 1270), (0xff0a, 1, 1843),
    (0xff0b, 1, 1556), (0xff0c, 1, 1024), (0xff0d, 1, 1556), (0xff0e, 1, 1024), (0xff10, 10, 1352), (0xff1a, 2, 1024), (0xff1c, 3, 1556),
    (0xff1f, 1, 1434), (0xff20, 1, 1720), (0xff21, 1, 1434), (0xff22, 1, 1470), (0xff23, 1, 1499), (0xff24, 1, 1536), (0xff25, 1, 1327),
    (0xff26, 1, 1268), (0xff27, 1, 1548), (0xff28, 1, 1532), (0xff29, 1, 819), (0xff2a, 1, 1053), (0xff2b, 1, 1440), (0xff2c, 1, 1262),
    (0xff2d, 1, 1835), (0xff2e, 1, 1532), (0xff2f, 1, 1577), (0xff30, 1, 1368), (0xff31, 1, 1577), (0xff32, 1, 1393), (0xff33, 1, 1339),
    (0xff34, 1, 1393), (0xff35, 1, 1536), (0xff36, 1, 1393), (0xff37, 1, 1884), (0xff38, 1, 1393), (0xff39, 1, 1372), (0xff3a, 1, 1352),
    (0xff3b, 1, 1270), (0xff3d, 1, 1270), (0xff41, 1, 1245), (0xff42, 1, 1249), (0xff43, 1, 1106), (0xff44, 1, 1249), (0xff45, 1, 1208),
    (0xff46, 1, 881), (0xff47, 1, 1190), (0xff48, 1, 1212), (0xff49, 1, 573), (0xff4a, 1, 707), (0xff4b, 1, 1114), (0xff4c, 1, 573),
    (0xff4d, 1, 1638), (0xff4e, 1, 1208), (0xff4f, 1, 1188), (0xff50, 1, 1253), (0xff51, 1, 1251), (0xff52, 1, 850), (0xff53, 1, 1073),
    (0xff54, 1, 881), (0xff55, 1, 1208), (0xff56, 1, 1085), (0xff57, 1, 1536), (0xff58, 1, 1085), (0xff59, 1, 1075), (0xff5a, 1, 1044),
    (0xff5b, 1, 1270), (0xff5c, 1, 983), (0xff5d, 1, 1270), (0xff5e, 1, 1679), (0xff61, 63, 1024), (0xffe0, 2, 1352), (0xffe5, 1, 1352),
    (0xffe8, 1, 1024),
];

const UD_KYOKASHO_NK_B_RUNS: &[(u32, u16, u16)] = &[
    (0x2010, 1, 1188), (0x2016, 1, 1434), (0x2018, 2, 897), (0x201c, 2, 1286), (0x2030, 1, 2122), (0x2032, 1, 586), (0x2033, 1, 967),
    (0x203e, 1, 1024), (0x20ac, 1, 1450), (0x2103, 1, 1995), (0x2212, 1, 1655), (0x221d, 2, 1901), (0x2225, 1, 1393), (0x2234, 2, 1704),
    (0x223d, 1, 1901), (0x2252, 1, 1655), (0x2260, 2, 1655), (0x2266, 2, 1655), (0x226a, 2, 1655), (0x2640, 1, 1696), (0x3000, 3, 1147),
    (0x3003, 1, 1622), (0x3005, 1, 1786), (0x3006, 1, 1757), (0x3007, 1, 1868), (0x3008, 2, 1307), (0x300a, 2, 1356), (0x300c, 2, 1307),
    (0x300e, 2, 1393), (0x3010, 2, 1307), (0x3014, 2, 1307), (0x301c, 1, 1827), (0x301d, 1, 1159), (0x301f, 1, 1159), (0x3041, 1, 1610),
    (0x3042, 1, 1933), (0x3043, 1, 1720), (0x3044, 1, 1966), (0x3045, 1, 1352), (0x3046, 1, 1556), (0x3047, 1, 1511), (0x3048, 1, 1772),
    (0x3049, 1, 1638), (0x304a, 1, 1946), (0x304b, 1, 2017), (0x304c, 1, 2028), (0x304d, 1, 1716), (0x304e, 1, 1884), (0x304f, 1, 1425),
    (0x3050, 1, 1733), (0x3051, 1, 1839), (0x3052, 1, 1999), (0x3053, 1, 1667), (0x3054, 1, 1860), (0x3055, 1, 1733), (0x3056, 1, 1921),
    (0x3057, 1, 1597), (0x3058, 1, 1683), (0x3059, 1, 1851), (0x305a, 1, 1950), (0x305b, 1, 1991), (0x305c, 1, 2032), (0x305d, 1, 1769),
    (0x305e, 1, 1888), (0x305f, 1, 1909), (0x3060, 1, 1946), (0x3061, 1, 1765), (0x3062, 1, 1880), (0x3063, 1, 1610), (0x3064, 1, 1970),
    (0x3065, 1, 1991), (0x3066, 1, 1798), (0x3067, 1, 2007), (0x3068, 1, 1638), (0x3069, 1, 1860), (0x306a, 1, 1937), (0x306b, 1, 1925),
    (0x306c, 1, 2023), (0x306d, 1, 1995), (0x306e, 1, 1982), (0x306f, 1, 1933), (0x3070, 1, 2032), (0x3071, 1, 2023), (0x3072, 1, 1993),
    (0x3073, 2, 1999), (0x3075, 3, 2015), (0x3078, 1, 1991), (0x3079, 2, 2003), (0x307b, 1, 1954), (0x307c, 1, 2040), (0x307e, 1, 1753),
    (0x307f, 1, 1987), (0x3080, 1, 1966), (0x3081, 1, 1991), (0x3082, 1, 1710), (0x3083, 1, 1651), (0x3084, 1, 1950), (0x3085, 1, 1710),
    (0x3086, 1, 2015), (0x3087, 1, 1475), (0x3088, 1, 1761), (0x3089, 1, 1663), (0x308a, 1, 1602), (0x308b, 1, 1749), (0x308c, 1, 2003),
    (0x308d, 1, 1769), (0x308e, 1, 1720), (0x308f, 1, 2003), (0x3090, 1, 1974), (0x3091, 1, 1970), (0x3092, 1, 1819), (0x3093, 1, 1901),
    (0x3094, 1, 1786), (0x3095, 1, 1774), (0x3096, 1, 1638), (0x309b, 1, 479), (0x309c, 1, 418), (0x309d, 1, 1470), (0x309e, 1, 1640),
    (0x30a1, 1, 1569), (0x30a2, 1, 1851), (0x30a3, 1, 1370), (0x30a4, 1, 1581), (0x30a5, 1, 1573), (0x30a6, 1, 1823), (0x30a7, 1, 1679),
    (0x30a8, 1, 1974), (0x30a9, 1, 1548), (0x30aa, 1, 1823), (0x30ab, 1, 1839), (0x30ac, 1, 1970), (0x30ad, 1, 1827), (0x30ae, 1, 1950),
    (0x30af, 1, 1634), (0x30b0, 1, 1905), (0x30b1, 1, 1827), (0x30b2, 1, 1970), (0x30b3, 1, 1794), (0x30b4, 2, 1950), (0x30b7, 1, 1851),
    (0x30b8, 1, 1901), (0x30b9, 1, 1831), (0x30ba, 1, 1991), (0x30bb, 1, 1896), (0x30bc, 1, 2007), (0x30bd, 1, 1696), (0x30be, 1, 1946),
    (0x30bf, 1, 1663), (0x30c0, 1, 1937), (0x30c1, 1, 1876), (0x30c2, 1, 1997), (0x30c3, 1, 1556), (0x30c4, 1, 1839), (0x30c5, 1, 1995),
    (0x30c6, 1, 1892), (0x30c8, 1, 1430), (0x30c9, 1, 1604), (0x30ca, 1, 1868), (0x30cb, 1, 1982), (0x30cc, 1, 1634), (0x30cd, 1, 1860),
    (0x30ce, 1, 1503), (0x30cf, 3, 2019), (0x30d2, 1, 1712), (0x30d3, 1, 1882), (0x30d4, 1, 1876), (0x30d5, 1, 1679), (0x30d6, 2, 1966),
    (0x30d8, 1, 2007), (0x30d9, 2, 2019), (0x30db, 1, 1905), (0x30dc, 1, 2011), (0x30dd, 1, 1954), (0x30de, 1, 1892), (0x30df, 1, 1589),
    (0x30e0, 1, 1823), (0x30e1, 1, 1610), (0x30e2, 1, 1872), (0x30e3, 1, 1569), (0x30e4, 1, 1868), (0x30e5, 1, 1638), (0x30e6, 1, 1970),
    (0x30e7, 1, 1434), (0x30e8, 1, 1671), (0x30e9, 1, 1753), (0x30ea, 1, 1548), (0x30eb, 1, 1942), (0x30ec, 1, 1692), (0x30ed, 1, 1901),
    (0x30ee, 1, 1507), (0x30ef, 1, 1794), (0x30f0, 1, 1991), (0x30f1, 1, 1974), (0x30f2, 1, 1675), (0x30f3, 1, 1774), (0x30f4, 1, 1962),
    (0x30f5, 1, 1593), (0x30f6, 1, 1569), (0x30f7, 1, 1970), (0x30f8, 1, 2028), (0x30f9, 1, 2015), (0x30fa, 1, 1886), (0x30fb, 1, 1196),
    (0x30fc, 1, 1909), (0x30fd, 1, 1491), (0x30fe, 1, 1618), (0xff01, 1, 1081), (0xff02, 1, 975), (0xff03, 1, 1688), (0xff04, 1, 1450),
    (0xff05, 1, 1982), (0xff06, 1, 1753), (0xff07, 1, 623), (0xff08, 2, 1307), (0xff0a, 1, 1843), (0xff0b, 1, 1655), (0xff0c, 1, 1147),
    (0xff0d, 1, 1655), (0xff0e, 1, 1147), (0xff10, 10, 1450), (0xff1a, 2, 1147), (0xff1c, 3, 1655), (0xff1f, 1, 1569), (0xff20, 1, 1843),
    (0xff21, 1, 1483), (0xff22, 1, 1524), (0xff23, 1, 1507), (0xff24, 1, 1626), (0xff25, 1, 1391), (0xff26, 1, 1331), (0xff27, 1, 1626),
    (0xff28, 1, 1622), (0xff29, 1, 967), (0xff2a, 1, 1130), (0xff2b, 1, 1497), (0xff2c, 1, 1305), (0xff2d, 1, 1974), (0xff2e, 1, 1622),
    (0xff2f, 1, 1638), (0xff30, 1, 1434), (0xff31, 1, 1696), (0xff32, 1, 1481), (0xff33, 1, 1397), (0xff34, 1, 1417), (0xff35, 1, 1626),
    (0xff36, 1, 1417), (0xff37, 1, 1946), (0xff38, 1, 1466), (0xff39, 1, 1409), (0xff3a, 1, 1413), (0xff3b, 1, 1307), (0xff3d, 1, 1307),
    (0xff41, 1, 1321), (0xff42, 1, 1319), (0xff43, 1, 1126), (0xff44, 1, 1321), (0xff45, 1, 1208), (0xff46, 1, 967), (0xff47, 1, 1266),
    (0xff48, 1, 1319), (0xff49, 1, 672), (0xff4a, 1, 823), (0xff4b, 1, 1198), (0xff4c, 1, 672), (0xff4d, 1, 1790), (0xff4e, 1, 1319),
    (0xff4f, 1, 1217), (0xff50, 2, 1321), (0xff52, 1, 969), (0xff53, 1, 1106), (0xff54, 1, 918), (0xff55, 1, 1319), (0xff56, 1, 1122),
    (0xff57, 1, 1622), (0xff58, 1, 1098), (0xff59, 1, 1130), (0xff5a, 1, 1044), (0xff5b, 1, 1307), (0xff5c, 1, 983), (0xff5d, 1, 1307),
    (0xff5e, 1, 1827), (0xff61, 63, 1024), (0xffe0, 2, 1450), (0xffe5, 1, 1450), (0xffe8, 1, 1024),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The widths PowerPoint's PDF of pptx 043 (24 pt, `UD デジタル 教科書体 NK-R`) has: a full-width space 12 pt, a circled digit 24 pt,
    /// and 14.78 em for `桃太郎は、それぞれ何％善か悪かを` (354.6 pt).
    #[::core::prelude::v1::test]
    fn regular_nk_widths_add_up_to_powerpoints_line() {
        let t = fallback_advance_table("UD デジタル 教科書体 NK-R", false).unwrap(); // i18n-ok（フォント名の試験）
        assert_eq!(t.advance('\u{3000}'), Some(0.5));
        let sum: f32 = "桃太郎は、それぞれ何％善か悪かを".chars().map(|c| t.advance(c).unwrap_or(1.0)).sum();
        // The PDF's 354.6 pt ends at the ink of を; the advances add up to 355.43 pt (its right side bearing is 0.8 pt).
        assert!((sum * 24.0 - 355.43).abs() < 0.05, "{}", sum * 24.0);
    }

    #[::core::prelude::v1::test]
    fn bold_and_family_names() {
        let b = fallback_advance_table("UD デジタル 教科書体 NK-B", false).unwrap(); // i18n-ok（フォント名の試験）
        let r = fallback_advance_table("UD デジタル 教科書体 NK-R", false).unwrap(); // i18n-ok（フォント名の試験）
        assert_ne!(b.advance('あ'), r.advance('あ'));
        assert_eq!(fallback_advance_table("UD デジタル 教科書体 NK", true).unwrap().advance('あ'), b.advance('あ')); // i18n-ok（フォント名の試験）
        assert!(fallback_advance_table("UD デジタル 教科書体 N-R", false).is_none()); // i18n-ok（フォント名の試験）
        assert!(fallback_advance_table("Arial", false).is_none());
    }
}
