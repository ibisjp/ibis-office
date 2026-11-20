# SPDX-License-Identifier: AGPL-3.0-only
# Synthetic outlines and variation mappings; no third-party font input.
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.pens.t2CharStringPen import T2CharStringPen
from fontTools.ttLib.tables._c_m_a_p import CmapSubtable

ORDER = ['.notdef', 'space', 'm', 'base', 'bmp', 'supplementary', 'composite']
CMAP = {0x20: 'space', 0x6D: 'm', 0x41: 'base', 0x42: 'bmp', 0x43: 'supplementary', 0x44: 'composite', 0x20000: 'base', 0x100001: 'supplementary'}
for truetype in [True, False]:
    fb = FontBuilder(1000, isTTF=truetype)
    fb.setupGlyphOrder(ORDER)
    fb.setupCharacterMap(CMAP)
    glyphs = {}
    for i, name in enumerate(ORDER):
        pen = TTGlyphPen(None) if truetype else T2CharStringPen(900, None)
        if name != 'space':
            pen.moveTo((20, 0)); pen.lineTo((300 + i * 30, 0))
            pen.lineTo((300 + i * 30, 400 + i * 40)); pen.lineTo((20, 400 + i * 40)); pen.closePath()
        glyphs[name] = pen.glyph() if truetype else pen.getCharString()
    if truetype:
        pen = TTGlyphPen(glyphs)
        pen.addComponent('base', (1, 0, 0, 1, 0, 0))
        pen.addComponent('bmp', (1, 0, 0, 1, 300, 100))
        glyphs['composite'] = pen.glyph()
        fb.setupGlyf(glyphs)
    else:
        fb.setupCFF('IbisSubsetFixture', {}, glyphs, {})
    fb.setupHorizontalMetrics({name: (900, 20) for name in ORDER})
    fb.setupHorizontalHeader(ascent=800, descent=-200)
    fb.setupVerticalMetrics({name: (1000, 50 + i) for i, name in enumerate(ORDER)})
    fb.setupVerticalHeader(ascent=500, descent=-500)
    fb.setupNameTable({'familyName': 'Ibis Subset Fixture', 'styleName': 'Regular', 'uniqueFontIdentifier': 'ibis-subset-fixture', 'fullName': 'Ibis Subset Fixture Regular', 'psName': 'IbisSubsetFixture-Regular', 'version': 'Version 1.0', 'copyright': 'Synthetic ibisOffice test outlines, 2026 masanork'})
    fb.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
    fb.setupPost()
    addOpenTypeFeaturesFromString(fb.font, r'''
        languagesystem DFLT dflt;
        feature vert { sub \base by bmp; } vert;
        feature liga { sub \base \base by supplementary; } liga;
        feature kern { pos \base \base -80; } kern;
    ''')
    variation = CmapSubtable.newSubtable(14)
    variation.platformID, variation.platEncID, variation.language = 0, 5, 0
    variation.cmap = {}
    variation.uvsDict = {0xFE00: [(0x41, 'bmp')], 0xE0100: [(0x41, 'supplementary')]}
    fb.font['cmap'].tables.append(variation)
    fb.font['head'].created = fb.font['head'].modified = 3800000000
    fb.font.recalcTimestamp = False
    fb.save(Path(__file__).with_name('ivs.ttf' if truetype else 'ivs-cff.otf'))
