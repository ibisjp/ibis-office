# SPDX-License-Identifier: AGPL-3.0-only
# Synthetic outlines, variation mappings and vertical substitutions for the tests of the blocks as a chain (GitHub issue #157): no third-party font input.
# Characters outside JIS X 0208 (so that they are not in the base block): U+3400, U+FF5F, U+20B9F, U+FE35; U+FF08 is in JIS X 0208 (the base block).
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib.tables._c_m_a_p import CmapSubtable

ORDER = ['.notdef', 'space', 'A', 'g3400', 'g3402', 'g3402.vert', 'gFF08', 'gFE35', 'g20B9F', 'ivs.a', 'ivs.b', 'ivs.c']
CMAP = {0x20: 'space', 0x41: 'A', 0x3400: 'g3400', 0xFF5F: 'g3402', 0xFF08: 'gFF08', 0xFE35: 'gFE35', 0x20B9F: 'g20B9F'}
fb = FontBuilder(1000, isTTF=True)
fb.setupGlyphOrder(ORDER)
fb.setupCharacterMap(CMAP)
glyphs = {}
for i, name in enumerate(ORDER):
    pen = TTGlyphPen(None)
    if name != 'space':
        pen.moveTo((20, 0)); pen.lineTo((300 + i * 40, 0))
        pen.lineTo((300 + i * 40, 400 + i * 50)); pen.lineTo((20, 400 + i * 50)); pen.closePath()
    glyphs[name] = pen.glyph()
fb.setupGlyf(glyphs)
fb.setupHorizontalMetrics({name: (1000, 20) for name in ORDER})
fb.setupHorizontalHeader(ascent=880, descent=-120)
fb.setupVerticalMetrics({name: (1000, 50 + i) for i, name in enumerate(ORDER)})
fb.setupVerticalHeader(ascent=500, descent=-500)
fb.setupNameTable({'familyName': 'Ibis Chain Fixture', 'styleName': 'Regular', 'uniqueFontIdentifier': 'ibis-chain-fixture', 'fullName': 'Ibis Chain Fixture Regular', 'psName': 'IbisChainFixture-Regular', 'version': 'Version 1.0', 'copyright': 'Synthetic test font', 'licenseDescription': 'CC0'})
fb.setupOS2(sTypoAscender=880, sTypoDescender=-120, usWinAscent=880, usWinDescent=120, usWeightClass=400, usWidthClass=5, fsSelection=0x40)
fb.setupPost()
addOpenTypeFeaturesFromString(fb.font, r'''
    languagesystem DFLT dflt;
    languagesystem hani dflt;
    languagesystem kana dflt;
    feature vert { sub \g3402 by \g3402.vert; sub \gFF08 by \gFE35; } vert;
    feature vrt2 { sub \g3402 by \g3402.vert; sub \gFF08 by \gFE35; } vrt2;
''')
variation = CmapSubtable.newSubtable(14)
variation.platformID, variation.platEncID, variation.language = 0, 5, 0
variation.cmap = {}
variation.uvsDict = {0xFE00: [(0x3400, 'ivs.a')], 0xE0100: [(0x3400, 'ivs.b'), (0x20B9F, 'ivs.c')]}
fb.font['cmap'].tables.append(variation)
fb.font['head'].created = fb.font['head'].modified = 3800000000
fb.font.recalcTimestamp = False
fb.save(Path(__file__).with_name('chain.ttf'))
