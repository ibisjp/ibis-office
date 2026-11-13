// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An old presentation (`ppt.rs`) written as the `.pptx` the rest of the crate reads: one master (with one layout)
//! for each main master of the file, and the slides with their shapes, pictures and texts. Text formatting the
//! file takes from the master is written out on the text itself.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::fmt::Write as _;

use crate::Error;
use crate::officeart_xml::{ArtContext, fill_xml, geometry_xml, line_xml, shadow_xml};
use crate::ppt::{Cf, Field, Interactive, MasterStyle, PageIr, Pf, PptIr, Props, ShapeIr, SlideShow, TextColor, TextIr, read_ppt};

const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;

/// The default color scheme of a page that has none (the Office one).
const DEFAULT_SCHEME: [u32; 8] = [0xFFFFFF, 0x000000, 0x808080, 0x000000, 0xBBE0E3, 0x333399, 0x00CCCC, 0x3333CC];

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c if (c as u32) < 0x20 && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn rels(items: &[(String, String, String)]) -> String {
    // (A target that begins with `ext:` is outside the package.)
    let list: String = items
        .iter()
        .map(|(id, kind, target)| match target.strip_prefix("ext:") {
            Some(url) => format!(r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{}" TargetMode="External"/>"#, esc(url)),
            None => format!(r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{target}"/>"#),
        })
        .collect();
    format!(r#"{HEAD}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{list}</Relationships>"#)
}

fn rel(id: &str, kind: &str, target: &str) -> (String, String, String) {
    (id.to_string(), kind.to_string(), target.to_string())
}

/// What is known while one page is written.
struct Ctx<'a> {
    ir: &'a PptIr,
    scheme: [u32; 8],
    /// The formatting of the page's master (by text type), over the document's defaults.
    styles: &'a HashMap<u16, MasterStyle>,
    /// The pictures this page uses: store index → relationship id.
    pictures: HashMap<usize, String>,
    next_rel: Cell<u32>,
    /// Relationships the page's links need: id, kind and target.
    extra_rels: RefCell<Vec<(String, String, String)>>,
    /// The ids of the slides, with their place in the file (for links to a slide).
    slide_places: &'a HashMap<u32, usize>,
    /// Whether the page being written is a master.
    #[allow(dead_code)]
    master: bool,
    /// The page's background when it is one flat color (see `ArtContext::flat_background`).
    background: Option<u32>,
}

impl Ctx<'_> {
    fn rgb(&self, c: TextColor) -> u32 {
        match c {
            TextColor::Rgb(v) => v,
            TextColor::Scheme(i) => self.scheme.get(i as usize).copied().unwrap_or(0),
        }
    }

    /// The `a:hlinkClick` for a click that does `i`, if it can be written.
    fn link_xml(&self, i: Interactive) -> Option<String> {
        let rid = |kind: &str, target: String| -> String {
            let mut rels = self.extra_rels.borrow_mut();
            if let Some((id, _, _)) = rels.iter().find(|(_, k, t)| k == kind && *t == target) {
                return id.clone();
            }
            let id = format!("rId{}", self.next_rel.get());
            self.next_rel.set(self.next_rel.get() + 1);
            rels.push((id.clone(), kind.to_string(), target));
            id
        };
        match i.action {
            3 => {
                let jump = match i.jump {
                    1 => "nextslide",
                    2 => "previousslide",
                    3 => "firstslide",
                    4 => "lastslide",
                    6 => "endshow",
                    _ => return None,
                };
                Some(format!(r#"<a:hlinkClick r:id="" action="ppaction://hlinkshowjump?jump={jump}"/>"#))
            }
            4 => {
                let target = self.ir.hyperlinks.get(&i.link_id)?;
                if i.link_type == 7 {
                    // A slide: its identifier is the target's first number.
                    let id: u32 = target.split(',').next()?.trim().parse().ok()?;
                    let place = self.slide_places.get(&id)?;
                    let r = rid("slide", format!("slides/slide{}.xml", place + 1));
                    Some(format!(r#"<a:hlinkClick r:id="{r}" action="ppaction://hlinksldjump"/>"#))
                } else if target.is_empty() {
                    None
                } else {
                    let r = rid("hyperlink", format!("ext:{target}"));
                    Some(format!(r#"<a:hlinkClick r:id="{r}"/>"#))
                }
            }
            _ => None,
        }
    }

    fn color_xml(&self, c: TextColor) -> String {
        format!(r#"<a:solidFill><a:srgbClr val="{:06X}"/></a:solidFill>"#, self.rgb(c))
    }

    /// Whether the typeface has Japanese characters. The font's character set (`lfCharSet`, MS-PPT 2.9.11) says: SHIFTJIS 128, HANGUL 129,
    /// GB2312 134 and CHINESEBIG5 136 are East Asian; any other set but DEFAULT_CHARSET (1) is a Latin (or symbol) typeface. DEFAULT_CHARSET
    /// does not say; then the name does: one with East Asian characters (the PC's Japanese typefaces carry their Japanese names), the last resort
    /// since the PC the file came from is not here to look at the typeface's cmap.
    fn has_east_asian_glyphs(&self, index: u16) -> bool {
        match self.ir.font_charsets.get(index as usize).copied() {
            Some(128 | 129 | 134 | 136) => true,
            Some(1) | None => self.ir.fonts.get(index as usize).is_some_and(|n| n.chars().any(|c| c as u32 >= 0x2e80)),
            Some(_) => false,
        }
    }

    fn font(&self, index: Option<u16>) -> Option<&str> {
        let name = self.ir.fonts.get(index? as usize)?.as_str();
        (!name.is_empty()).then_some(name)
    }

    /// The master formatting of a level of a kind of text: the master's over the document's default.
    fn level_style(&self, kind: u32, level: u16) -> (Pf, Cf) {
        // A kind without styles of its own takes those of the kind it is a size of.
        let family = match kind {
            6 => 0,
            5 | 7 | 8 => 1,
            k => k as u16,
        };
        let pick = |map: &HashMap<u16, MasterStyle>| map.get(&family).and_then(|s| s.levels.get(level as usize).cloned());
        // The text of a text box (the "other" kind) on a slide takes the document's default style, not the master's (the master's
        // "other" style is for the text of the master's own shapes): 030's text boxes are centered by the default over a master that says left.
        let from_master = if kind == 4 && !self.master { None } else { pick(self.styles) };
        let from_default = pick(&self.ir.default_props_styles());
        let (mut pf, cf) = match (from_master, from_default) {
            (Some((pf, cf)), Some((dpf, dcf))) => (pf.over(&dpf), cf.over(&dcf)),
            (Some(x), None) | (None, Some(x)) => x,
            (None, None) => (Pf::default(), Cf::default()),
        };
        // The spacing of a level the style leaves out is the one of the level above (PowerPoint's own pptx of 032.ppt: the second level of the
        // body style, which the file gives no space before, has the first level's).
        // [MS-PPT] (TextMasterStyleAtom, TextMasterStyleLevel) states no such rule (looked for 2026-11-10); the old-file description of
        // PowerPoint 97-2003 (old-file-viewer.bobpritchett.com/docs/powerpoint-format) says each level is stored as the differences from the level above.
        // The bullet of a level that does not say whether it has one is the one of the level above too (046.ppt: the body style's second
        // level gives no bullet flag, and PowerPoint's pptx of it has the check marks of the paragraphs' bullet character).
        if level > 0
            && (pf.space_before.is_none() || pf.space_after.is_none() || pf.line_spacing.is_none() || pf.has_bullet.is_none())
        {
            let (above, _) = self.level_style(kind, level - 1);
            pf.space_before = pf.space_before.or(above.space_before);
            pf.space_after = pf.space_after.or(above.space_after);
            pf.line_spacing = pf.line_spacing.or(above.line_spacing);
            if pf.has_bullet.is_none() {
                pf.has_bullet = above.has_bullet;
                pf.bullet_char = pf.bullet_char.or(above.bullet_char);
                pf.bullet_font = pf.bullet_font.or(above.bullet_font);
                pf.bullet_size = pf.bullet_size.or(above.bullet_size);
                pf.bullet_color = pf.bullet_color.or(above.bullet_color);
            }
        }
        (pf, cf)
    }
}

impl PptIr {
    fn default_props_styles(&self) -> HashMap<u16, MasterStyle> {
        self.defaults.clone()
    }
}

fn units_to_emu(v: i64) -> i64 {
    v * 1587 + v / 2
}

/// A paragraph-properties element (`tag` is `pPr`, or `lvl1pPr` and so on), with a `defRPr` from `cf` when it is given.
fn ppr_xml(ctx: &Ctx, tag: &str, pf: &Pf, cf: Option<&Cf>, auto: Option<(u16, u16)>) -> String {
    let mut attrs = String::new();
    let left = i64::from(pf.left_margin.unwrap_or(0));
    if pf.left_margin.is_some() {
        let _ = write!(attrs, r#" marL="{}""#, units_to_emu(left));
    }
    if let Some(indent) = pf.indent {
        let _ = write!(attrs, r#" indent="{}""#, units_to_emu(i64::from(indent) - left));
    }
    if let Some(align) = pf.align {
        let a = match align {
            1 => "ctr",
            2 => "r",
            3 => "just",
            4 => "dist",
            _ => "l",
        };
        let _ = write!(attrs, r#" algn="{a}""#);
    }
    let mut kids = String::new();
    let spacing = |name: &str, v: i16| -> String {
        if v >= 0 {
            format!(r#"<a:{name}><a:spcPct val="{}"/></a:{name}>"#, i64::from(v) * 1000)
        } else {
            format!(r#"<a:{name}><a:spcPts val="{}"/></a:{name}>"#, (-i64::from(v)) * 100 / 8)
        }
    };
    if let Some(v) = pf.line_spacing {
        kids += &spacing("lnSpc", v);
    }
    if let Some(v) = pf.space_before {
        kids += &spacing("spcBef", v);
    }
    if let Some(v) = pf.space_after {
        kids += &spacing("spcAft", v);
    }
    if pf.has_bullet == Some(true) {
        if let Some(color) = pf.bullet_color {
            kids += &format!("<a:buClr><a:srgbClr val=\"{:06X}\"/></a:buClr>", ctx.rgb(color));
        }
        if let Some(size) = pf.bullet_size {
            if size > 0 {
                let _ = write!(kids, r#"<a:buSzPct val="{}"/>"#, i64::from(size) * 1000);
            } else if size < 0 {
                let _ = write!(kids, r#"<a:buSzPts val="{}"/>"#, i64::from(-size) * 100);
            }
        }
        if let (Some(font), None) = (ctx.font(pf.bullet_font), auto) {
            let _ = write!(kids, r#"<a:buFont typeface="{}"/>"#, esc(font));
        }
        if let Some((scheme, start)) = auto {
            let kind = auto_number_type(scheme);
            let at = if start > 1 { format!(r#" startAt="{start}""#) } else { String::new() };
            let _ = write!(kids, r#"<a:buFont typeface="+mj-lt"/><a:buAutoNum type="{kind}"{at}/>"#);
        } else {
            let ch = pf.bullet_char.and_then(|c| char::from_u32(u32::from(c))).filter(|c| *c != '\0').unwrap_or('•');
            let _ = write!(kids, r#"<a:buChar char="{}"/>"#, esc(&ch.to_string()));
        }
    } else if pf.has_bullet == Some(false) {
        kids += "<a:buNone/>";
    }
    if let Some(tabs) = &pf.tabs {
        kids += "<a:tabLst>";
        for (position, kind) in tabs {
            let algn = match kind {
                1 => "ctr",
                2 => "r",
                3 => "dec",
                _ => "l",
            };
            let _ = write!(kids, r#"<a:tab pos="{}" algn="{algn}"/>"#, units_to_emu(i64::from(left) + i64::from(*position)));
        }
        kids += "</a:tabLst>";
    }
    if let Some(cf) = cf {
        kids += &rpr_xml(ctx, "defRPr", cf, "");
    }
    format!("<a:{tag}{attrs}>{kids}</a:{tag}>")
}

/// A run-properties element.
fn rpr_xml(ctx: &Ctx, tag: &str, cf: &Cf, extra_attrs: &str) -> String {
    let mut attrs = String::from(r#" lang="ja-JP" altLang="en-US""#);
    if let Some(size) = cf.size {
        let hundredths = cf.size_hundredths.filter(|h| h / 100 == size).map_or(u32::from(size) * 100, u32::from);
        let _ = write!(attrs, r#" sz="{hundredths}""#);
    }
    if let Some(b) = cf.bold {
        let _ = write!(attrs, r#" b="{}""#, u8::from(b));
    }
    if let Some(i) = cf.italic {
        let _ = write!(attrs, r#" i="{}""#, u8::from(i));
    }
    if let Some(u) = cf.underline {
        let _ = write!(attrs, r#" u="{}""#, if u { "sng" } else { "none" });
    }
    if let Some(p) = cf.position {
        if p != 0 {
            let _ = write!(attrs, r#" baseline="{}""#, i32::from(p) * 1000);
        }
    }
    if let Some(spc) = cf.spacing {
        let _ = write!(attrs, r#" spc="{spc}""#);
    }
    attrs += extra_attrs;
    let mut kids = String::new();
    if let Some(color) = cf.color {
        kids += &ctx.color_xml(color);
    }
    if let Some(font) = ctx.font(cf.ansi_font.or(cf.font)) {
        let _ = write!(kids, r#"<a:latin typeface="{}"/>"#, esc(font));
    }
    // A run that names a Latin typeface only (Times New Roman) and no East Asian one is drawn with the East Asian typeface of the
    // text style it would have inherited: PowerPoint's PDF of such a run (107.ppt, 4th slide, Times New Roman and the default
    // MS PGothic) uses MS PGothic. A run that names a typeface with Japanese characters (Meiryo) keeps it for all its characters.
    let ea = match (cf.east_asian_font, cf.font) {
        (Some(e), _) => Some(e),
        (None, Some(f)) if ctx.font(Some(f)).is_some() && !ctx.has_east_asian_glyphs(f) => cf.inherited_east_asian_font.or(cf.font),
        _ => cf.font,
    };
    if let Some(font) = ctx.font(ea) {
        // Shift-JIS (`lfCharSet` 128) is `charset="-128"`: PowerPoint draws such a typeface it lacks in Yu Gothic (m56-missing-font-standins.md).
        // A typeface of DEFAULT_CHARSET (1) with Japanese characters in its name is drawn the same way (082's "AR P丸ゴシック体E", lfCharSet 1:
        // PowerPoint's own pptx writes charset="-128" for it and its PDF draws Yu Gothic), as `has_east_asian_glyphs` decides for it.
        let charset = if ea.is_some_and(|i| ctx.ir.font_charsets.get(i as usize) == Some(&128) || (ctx.ir.font_charsets.get(i as usize) == Some(&1) && ctx.has_east_asian_glyphs(i))) { r#" charset="-128""# } else { "" };
        let _ = write!(kids, r#"<a:ea typeface="{}"{charset}/>"#, esc(font));
    }
    if let Some(font) = ctx.font(cf.symbol_font) {
        let _ = write!(kids, r#"<a:sym typeface="{}"/>"#, esc(font));
    }
    if let Some(link) = cf.link.and_then(|l| ctx.link_xml(l)) {
        kids += &link;
    }
    if kids.is_empty() { format!("<a:{tag}{attrs}/>") } else { format!("<a:{tag}{attrs}>{kids}</a:{tag}>") }
}

/// The text of a shape as a `p:txBody` / `a:txBody` content: `a:bodyPr`, `a:lstStyle` and the paragraphs.
fn text_body(ctx: &Ctx, shape: &ShapeIr, text: &TextIr) -> String {
    let p = &shape.props;
    let d = &ctx.ir.default_props;
    let get = |id: u16| p.get(id).or_else(|| d.get(id));
    let inset = |id: u16, default: i64| get(id).map_or(default, |v| i64::from(v as i32));
    let anchor = match get(0x87).unwrap_or(0) {
        1 | 4 => "ctr",
        2 | 5 => "b",
        _ => "t",
    };
    let anchor_ctr = if matches!(get(0x87), Some(3..=5)) { r#" anchorCtr="1""# } else { "" };
    let wrap = if get(0x85) == Some(2) { "none" } else { "square" };
    let vert = match get(0x88).unwrap_or(0) {
        1 | 3 => r#" vert="vert""#,
        2 => r#" vert="vert270""#,
        _ => "",
    };
    let fit = if p.flag(0xBF, 1) == Some(true) { "<a:spAutoFit/>" } else { "" };
    let mut out = format!(
        r#"<a:bodyPr wrap="{wrap}" lIns="{}" tIns="{}" rIns="{}" bIns="{}" rtlCol="0" anchor="{anchor}"{anchor_ctr}{vert}>{fit}</a:bodyPr><a:lstStyle/>"#,
        inset(0x81, 91440),
        inset(0x82, 45720),
        inset(0x83, 91440),
        inset(0x84, 45720),
    );
    if text.paragraphs.is_empty() {
        out += "<a:p><a:endParaRPr lang=\"ja-JP\" altLang=\"en-US\"/></a:p>";
    }
    for para in &text.paragraphs {
        let (mpf, mcf) = ctx.level_style(text.kind, para.level);
        let pf = para.pf.over(&mpf);
        let mut ppr_attrs = String::new();
        if para.level > 0 {
            let _ = write!(ppr_attrs, r#" lvl="{}""#, para.level);
        }
        let ppr = ppr_xml(ctx, "pPr", &pf, None, para.auto_number);
        let ppr = if ppr_attrs.is_empty() { ppr } else { ppr.replacen("<a:pPr", &format!("<a:pPr{ppr_attrs}"), 1) };
        out += "<a:p>";
        out += &ppr;
        let mut last_cf = mcf.clone();
        for (run, cf, field) in &para.runs {
            let eff = cf.over(&mcf);
            last_cf = eff.clone();
            match field {
                Some(Field::SlideNumber) => {
                    let _ = write!(out, r#"<a:fld id="{{B6F15528-21DE-4FAA-801E-634DDDAF4B2B}}" type="slidenum">{}<a:t>‹#›</a:t></a:fld>"#, rpr_xml(ctx, "rPr", &eff, ""));
                }
                Some(Field::DateTime) => {
                    let _ = write!(out, r#"<a:fld id="{{B6F15528-21DE-4FAA-801E-634DDDAF4B2C}}" type="datetime1">{}<a:t>{}</a:t></a:fld>"#, rpr_xml(ctx, "rPr", &eff, ""), esc(run));
                }
                None => {
                    // A vertical tab is a line break.
                    for (i, piece) in run.split('\u{B}').enumerate() {
                        if i > 0 {
                            let _ = write!(out, "<a:br>{}</a:br>", rpr_xml(ctx, "rPr", &eff, ""));
                        }
                        if !piece.is_empty() {
                            let _ = write!(out, "<a:r>{}<a:t>{}</a:t></a:r>", rpr_xml(ctx, "rPr", &eff, ""), esc(piece));
                        }
                    }
                }
            }
        }
        // The paragraph mark has its own formatting; an empty paragraph is as high as that says.
        if para.runs.is_empty() {
            last_cf = para.end_cf.over(&mcf);
        }
        out += &rpr_xml(ctx, "endParaRPr", &last_cf, "");
        out += "</a:p>";
    }
    out
}

/// The placeholder `type` (and `idx`) a placement id (`PlaceholderEnum`) is.
fn placeholder_kind(id: u8) -> Option<(&'static str, Option<u32>)> {
    Some(match id {
        0x01 | 0x0D | 0x11 => ("title", None),
        0x02 | 0x0E | 0x12 | 0x0C | 0x06 => ("body", Some(1)),
        0x03 | 0x0F => ("ctrTitle", None),
        0x04 | 0x10 => ("subTitle", Some(1)),
        0x05 | 0x0B => ("sldImg", None),
        0x07 => ("dt", Some(10)),
        0x08 => ("sldNum", Some(12)),
        0x09 => ("ftr", Some(11)),
        0x0A => ("hdr", None),
        0x13..=0x19 => ("obj", Some(1)),
        _ => return None,
    })
}

/// The properties of the placeholder of a master that a slide's unplaced placeholder (`PlaceholderEnum` `kind`) takes after.
fn master_placeholder_props<'a>(ir: &'a PptIr, page: &PageIr, kind: u8) -> Option<&'a Props> {
    // The placeholder kinds of the master to look for, the closest first (a title master that has no center title has the title).
    let master_kinds: &[u8] = match kind {
        0x0D | 0x11 => &[0x01],
        0x0E | 0x12 => &[0x02],
        0x0F => &[0x03, 0x01],
        0x10 => &[0x04, 0x02],
        _ => return None,
    };
    let master = ir.masters.iter().find(|m| m.id == page.master_ref)?;
    master_kinds.iter().find_map(|k| master.shapes.iter().find(|s| s.placeholder == Some(*k)).map(|s| &s.props))
}

/// A page's context for a shape that takes no document defaults: only its own properties count.
struct OwnOnly<'a, 'b>(&'a mut Ctx<'b>, Props);

impl ArtContext for OwnOnly<'_, '_> {
    fn defaults(&self) -> &Props {
        &self.1
    }

    fn color(&self, colorref: u32) -> u32 {
        self.0.color(colorref)
    }

    fn picture(&mut self, index: usize) -> Option<String> {
        Ctx::picture(self.0, index)
    }

    fn flat_background(&self) -> Option<u32> {
        self.0.background
    }
}

impl ArtContext for Ctx<'_> {
    fn defaults(&self) -> &Props {
        &self.ir.default_props
    }

    fn color(&self, colorref: u32) -> u32 {
        self.rgb(crate::ppt::art_color_of(colorref))
    }

    fn picture(&mut self, index: usize) -> Option<String> {
        Ctx::picture(self, index)
    }

    fn flat_background(&self) -> Option<u32> {
        self.background
    }
}

/// The color of a page's background when it is a plain fill (a gradient or picture fill: None). A page that has no background of its own
/// is white.
fn flat_background(ctx: &Ctx, page: &PageIr) -> Option<u32> {
    let Some(bg) = &page.background else { return Some(0xFFFFFF) };
    match bg.props.get(0x180).unwrap_or(0) {
        0 => Some(ctx.color(bg.props.get(0x181).or_else(|| ctx.ir.default_props.get(0x181)).unwrap_or(0xFFFFFF))),
        _ => None,
    }
}

impl Ctx<'_> {
    /// The relationship id of the picture `index` (1-based in the store), adding it for this page.
    fn picture(&mut self, index: usize) -> Option<String> {
        self.ir.blips.get(index - 1)?.as_ref()?;
        if let Some(r) = self.pictures.get(&index) {
            return Some(r.clone());
        }
        let id = format!("rId{}", self.next_rel.get());
        self.next_rel.set(self.next_rel.get() + 1);
        self.pictures.insert(index, id.clone());
        Some(id)
    }
}

/// A shape as XML. `space` is the (offset, extent) it is in when it is a child of a group, else `None`.
fn shape_xml(ctx: &mut Ctx, shape: &ShapeIr, page: &PageIr, outline: &[TextIr], depth: usize) -> String {
    let id = shape.id.max(2);
    let name = if shape.name.is_empty() { format!("Shape {id}") } else { esc(&shape.name) };
    let Some((x, y, w, h)) = shape.frame else {
        return String::new();
    };
    let rotation = shape.props.get(0x04).map_or(0, |v| ((f64::from(v as i32) / 65536.0) * 60000.0).round() as i64);
    // The rectangle a .ppt gives a shape turned by a quarter (45 to 135 or 225 to 315 degrees) is the one it covers turned; DrawingML's
    // is the one before the turn: the same center, width and height swapped.
    // (Measured on PowerPoint's PDFs of the master top bands of one presentation; Apache POI's HSLF swaps the same. No statement found in [MS-ODRAW]
    // for the rotation property (0x0004) or OfficeArtClientAnchor.)
    let turned = rotation.rem_euclid(21_600_000);
    let (x, y, w, h) = if !shape.is_group && ((2_700_000..8_100_000).contains(&turned) || (13_500_000..18_900_000).contains(&turned)) {
        (x + (w - h) / 2, y + (h - w) / 2, h, w)
    } else {
        (x, y, w, h)
    };
    let mut xfrm_attrs = String::new();
    if rotation != 0 {
        let _ = write!(xfrm_attrs, r#" rot="{}""#, rotation.rem_euclid(21_600_000));
    }
    if shape.flip_h {
        xfrm_attrs += r#" flipH="1""#;
    }
    if shape.flip_v {
        xfrm_attrs += r#" flipV="1""#;
    }
    if shape.is_group {
        let (cx, cy, cw, ch) = shape.child_space.unwrap_or((x, y, w, h));
        let kids: String = shape.children.iter().map(|c| shape_xml(ctx, c, page, outline, depth + 1)).collect();
        return format!(
            r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm{xfrm_attrs}><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/><a:chOff x="{cx}" y="{cy}"/><a:chExt cx="{cw}" cy="{ch}"/></a:xfrm></p:grpSpPr>{kids}</p:grpSp>"#
        );
    }
    let xfrm = format!(r#"<a:xfrm{xfrm_attrs}><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm>"#);
    // A picture: a picture frame with a picture, or any shape with only a picture.
    let picture_index = shape.props.get(0x104).map(|v| v as usize).filter(|i| *i > 0);
    if let (Some(index), true) = (picture_index, shape.spt == 75 || shape.props.get(0x180).is_none()) {
        if shape.text.is_none() && shape.placeholder.is_none() {
            if let Some(rid) = ctx.picture(index) {
                // A picture that gives neither a line color nor a line flag has no line (as for a shape below).
                let no_line = shape.props.get(0x1C0).is_none() && shape.props.flag(0x1FF, 3).is_none();
                let line = if no_line { line_xml(ctx, &shape.props.with_flag(0x1FF, 3, false), false) } else { line_xml(ctx, &shape.props, false) };
                // Cropped: the fractions of the picture left out at each side (16.16 fixed point).
                let crop = |id: u16| shape.props.int(id).map_or(0, |v| (f64::from(v) / 65536.0 * 100_000.0).round() as i64);
                let src_rect = if (0x100..=0x103).any(|id| crop(id) != 0) { format!(r#"<a:srcRect l="{}" t="{}" r="{}" b="{}"/>"#, crop(0x102), crop(0x100), crop(0x103), crop(0x101)) } else { String::new() };
                return format!(
                    r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="{name}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{rid}"/>{src_rect}<a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{xfrm}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{line}</p:spPr></p:pic>"#
                );
            }
        }
    }
    // Its text: its own, or what the slide list holds for it.
    let text: Option<TextIr> = match (&shape.text, shape.outline_ref) {
        (Some(t), _) => Some(t.clone()),
        (None, Some(i)) => outline.get(i).cloned(),
        _ => None,
    };
    let ph = shape.placeholder.and_then(placeholder_kind);
    let is_line = matches!(shape.spt, 20) || shape.is_connector || (32..=40).contains(&shape.spt);
    let geometry = geometry_xml(shape.spt, &shape.props);
    // A shape with no text in the binary format may have it in its DrawingML (text in columns).
    let raw_body = if text.is_none() { shape.props.complex(0x03A9).filter(|b| crate::ppt::shape_xml_is_current(b, shape.frame)).and_then(crate::ppt::shape_xml_text_body) } else { None };
    let textual = text.is_some() || ph.is_some() || raw_body.is_some();
    // A placeholder of no place takes what it does not set from the master's placeholder (no fill, no line), not from the
    // document's default shape.
    let (fill, line) = if shape.unplaced_placeholder {
        // What it does not say is the master placeholder's (076's centered title: the master's blue fill, not none).
        let props = match shape.unplaced_kind.and_then(|k| master_placeholder_props(ctx.ir, page, k)) {
            Some(base) => shape.props.with_missing_from(base, |id| (0x180..=0x1A0).contains(&id) || (0x1C0..=0x1E0).contains(&id) || id == 0x1BF || id == 0x1FF),
            None => shape.props.clone(),
        };
        let mut own = OwnOnly(ctx, Props::default());
        (if is_line { String::new() } else { fill_xml(&mut own, &props, true) }, line_xml(&own, &props, is_line))
    } else {
        // A shape that gives no fill color is white (MS-ODRAW fillColor), not the drawing group's default fill color (the scheme's
        // fill): PowerPoint leaves the color out of a file when the shape is white, and draws it white (corpus 150's full-slide rectangle).
        let defaults = ctx.ir.default_props.without(&[0x181, 0x183]);
        let mut own = OwnOnly(ctx, defaults);
        let fill = if is_line { String::new() } else { fill_xml(&mut own, &shape.props, textual) };
        // A shape that gives neither a line color nor whether it has a line has none (030's rounded rectangle: the drawing
        // group's default line is on, PowerPoint draws none). [MS-ODRAW] "Line Style Boolean Properties" gives the property block the
        // default 0x0 (fLine off) and says an omitted property has its default; that the drawing group's own default is left out here
        // is only measured (no statement found in MS-PPT or MS-ODRAW).
        let line_props = if !is_line && shape.props.get(0x1C0).is_none() && shape.props.flag(0x1FF, 3).is_none() { shape.props.with_flag(0x1FF, 3, false) } else { shape.props.clone() };
        (fill, line_xml(ctx, &line_props, !textual || is_line))
    };
    let sp_pr = format!("<p:spPr>{xfrm}{geometry}{fill}{line}{}</p:spPr>", shadow_xml(ctx, &shape.props));
    if is_line {
        return format!(r#"<p:cxnSp><p:nvCxnSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr>{sp_pr}</p:cxnSp>"#);
    }
    let nv_pr = match ph {
        Some((kind, Some(idx))) => format!(r#"<p:ph type="{kind}" idx="{idx}"/>"#),
        Some((kind, None)) => format!(r#"<p:ph type="{kind}"/>"#),
        None => String::new(),
    };
    let cnv_sp_pr = if ph.is_some() {
        r#"<p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr>"#.to_string()
    } else if shape.spt == 202 {
        r#"<p:cNvSpPr txBox="1"/>"#.to_string()
    } else {
        "<p:cNvSpPr/>".to_string()
    };
    let body = match &text {
        Some(t) => format!("<p:txBody>{}</p:txBody>", text_body(ctx, shape, t)),
        None => raw_body.unwrap_or_default(),
    };
    let link = shape.interactive.and_then(|i| ctx.link_xml(i)).unwrap_or_default();
    let c_nv_pr = if link.is_empty() { format!(r#"<p:cNvPr id="{id}" name="{name}"/>"#) } else { format!(r#"<p:cNvPr id="{id}" name="{name}">{link}</p:cNvPr>"#) };
    format!(r#"<p:sp><p:nvSpPr>{c_nv_pr}{cnv_sp_pr}<p:nvPr>{nv_pr}</p:nvPr></p:nvSpPr>{sp_pr}{body}</p:sp>"#)
}

/// The transition of a slide (`p:transition`) from its show information.
fn transition_xml(show: &SlideShow) -> String {
    let dir8 = |d: u8| ["l", "u", "r", "d", "lu", "ru", "ld", "rd"].get(d as usize).copied().unwrap_or("l");
    let dir4 = |d: u8| ["l", "u", "r", "d"].get(d as usize).copied().unwrap_or("l");
    let orient = |d: u8| if d == 0 { "horz" } else { "vert" };
    let effect = match show.effect {
        // (No transition at all is a cut that is not through black: nothing to write unless the timing says something.)
        0 if show.direction != 1 && show.auto_advance.is_none() && show.manual_advance => return String::new(),
        0 => format!("<p:cut{}/>", if show.direction == 1 { r#" thruBlk="1""# } else { "" }),
        1 => "<p:random/>".to_string(),
        2 => format!(r#"<p:blinds dir="{}"/>"#, if show.direction == 0 { "vert" } else { "horz" }),
        3 => format!(r#"<p:checker dir="{}"/>"#, orient(show.direction)),
        4 => format!(r#"<p:cover dir="{}"/>"#, dir8(show.direction)),
        5 => "<p:dissolve/>".to_string(),
        6 | 23 => format!("<p:fade{}/>", if show.direction == 1 { r#" thruBlk="1""# } else { "" }),
        7 => format!(r#"<p:pull dir="{}"/>"#, dir8(show.direction)),
        8 => format!(r#"<p:randomBar dir="{}"/>"#, orient(show.direction)),
        10 => format!(r#"<p:wipe dir="{}"/>"#, dir4(show.direction)),
        11 => format!(r#"<p:zoom dir="{}"/>"#, if show.direction == 0 { "out" } else { "in" }),
        13 => format!(r#"<p:split orient="{}" dir="{}"/>"#, if show.direction < 2 { "horz" } else { "vert" }, if show.direction % 2 == 0 { "out" } else { "in" }),
        17 => "<p:diamond/>".to_string(),
        18 => "<p:plus/>".to_string(),
        19 => "<p:wedge/>".to_string(),
        20 => format!(r#"<p:push dir="{}"/>"#, dir4(show.direction)),
        21 => format!(r#"<p:comb dir="{}"/>"#, orient(show.direction)),
        22 => "<p:newsflash/>".to_string(),
        26 => format!(r#"<p:wheel spokes="{}"/>"#, show.direction.max(1)),
        27 => "<p:circle/>".to_string(),
        _ => return String::new(),
    };
    let mut attrs = String::from(match show.speed {
        0 => r#" spd="slow""#,
        2 => r#" spd="fast""#,
        _ => r#" spd="med""#,
    });
    if !show.manual_advance {
        attrs += r#" advClick="0""#;
    }
    if let Some(ms) = show.auto_advance {
        let _ = write!(attrs, r#" advTm="{ms}""#);
    }
    format!("<p:transition{attrs}>{effect}</p:transition>")
}

/// A page's background as `p:bg`.
fn background_xml(ctx: &mut Ctx, page: &PageIr) -> String {
    let Some(bg) = &page.background else { return String::new() };
    let defaults = ctx.ir.default_props.without(&[0x181, 0x183]);
    let fill = fill_xml(&mut OwnOnly(ctx, defaults), &bg.props, false);
    format!("<p:bg><p:bgPr>{fill}<a:effectLst/></p:bgPr></p:bg>")
}

fn tree(shapes: &str) -> String {
    format!(
        r#"<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree>"#
    )
}

/// The theme of a page: Office's, with the page's color scheme.
fn theme_of(scheme: &[u32; 8]) -> String {
    let mut t = crate::new_file::theme();
    let hex = |v: u32| format!("{v:06X}");
    t = t.replacen(
        r#"<a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1>"#,
        &format!(r#"<a:dk1><a:srgbClr val="{}"/></a:dk1>"#, hex(scheme[1])),
        1,
    );
    t = t.replacen(
        r#"<a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>"#,
        &format!(r#"<a:lt1><a:srgbClr val="{}"/></a:lt1>"#, hex(scheme[0])),
        1,
    );
    for (name, old, new) in [("dk2", "44546A", scheme[3]), ("lt2", "E7E6E6", scheme[4]), ("accent1", "4472C4", scheme[5]), ("accent2", "ED7D31", scheme[6]), ("accent3", "A5A5A5", scheme[7])] {
        t = t.replacen(&format!(r#"<a:{name}><a:srgbClr val="{old}"/></a:{name}>"#), &format!(r#"<a:{name}><a:srgbClr val="{}"/></a:{name}>"#, hex(new)), 1);
    }
    t
}

/// A master's `p:txStyles` from its text styles (title, body and other text).
fn tx_styles(ctx: &Ctx) -> String {
    let style = |tag: &str, kind: u32| -> String {
        let levels: String = (0..5)
            .map(|level| {
                let (pf, cf) = ctx.level_style(kind, level);
                ppr_xml(ctx, &format!("lvl{}pPr", level + 1), &pf, Some(&cf), None)
            })
            .collect();
        format!("<p:{tag}>{levels}</p:{tag}>")
    };
    format!("<p:txStyles>{}{}{}</p:txStyles>", style("titleStyle", 0), style("bodyStyle", 1), style("otherStyle", 4))
}

/// The `.pptx` made from the old presentation in `data`.
pub fn to_pptx(data: &[u8]) -> Result<Vec<u8>, Error> {
    to_pptx_of(read_ppt(data)?)
}

/// The `.pptx` made from a presentation read.
pub(crate) fn to_pptx_of(ir: crate::ppt::PptIr) -> Result<Vec<u8>, Error> {
    let (width, height) = ir.size;
    let slide_places: HashMap<u32, usize> = ir.slides.iter().enumerate().map(|(i, s)| (s.id, i)).collect();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut content_overrides = String::new();
    let mut extensions: Vec<&'static str> = Vec::new();
    let mut master_rels: Vec<(String, String, String)> = Vec::new();
    let mut pres_rels: Vec<(String, String, String)> = Vec::new();
    let mut master_ids = String::new();
    let mut slide_ids = String::new();
    // Pictures the pages use, by store index: written once as media.
    let mut media: HashMap<usize, String> = HashMap::new();

    let masters: Vec<&PageIr> = if ir.masters.is_empty() { Vec::new() } else { ir.masters.iter().collect() };
    let page_media = |ctx: &Ctx, media: &mut HashMap<usize, String>, files: &mut Vec<(String, Vec<u8>)>, extensions: &mut Vec<&'static str>| -> Vec<(String, String, String)> {
        let mut rels_out = Vec::new();
        let mut used: Vec<(&usize, &String)> = ctx.pictures.iter().collect();
        used.sort();
        for (index, rid) in used {
            let Some(Some(blip)) = ir.blips.get(index - 1) else { continue };
            let name = media.entry(*index).or_insert_with(|| {
                let name = format!("image{}.{}", index, blip.ext);
                files.push((format!("ppt/media/{name}"), blip.data.clone()));
                if !extensions.contains(&blip.ext) {
                    extensions.push(blip.ext);
                }
                name
            });
            rels_out.push((rid.clone(), "image".to_string(), format!("../media/{name}")));
        }
        rels_out
    };
    // The masters: master i has theme i and layout i.
    let mut master_index_of: HashMap<u32, usize> = HashMap::new();
    for (i, m) in masters.iter().enumerate() {
        let n = i + 1;
        master_index_of.insert(m.id, i);
        let scheme = m.scheme.unwrap_or(DEFAULT_SCHEME);
        let mut ctx = Ctx { ir: &ir, scheme, styles: &m.styles, pictures: HashMap::new(), next_rel: Cell::new(3), extra_rels: RefCell::new(Vec::new()), slide_places: &slide_places, master: true, background: None };
        ctx.background = flat_background(&ctx, m);
        // The master's shapes: placeholders (as the master's) and the others.
        let mut shapes = String::new();
        let mut layout_shapes = String::new();
        let title_master = m.geom == 2;
        for s in &m.shapes {
            let mut shape = s.clone();
            // The master's placeholder kinds are the master's own; the layout of a title master has the center ones.
            let kind = shape.placeholder;
            if let Some(k) = kind {
                // A master has title, body, date, footer and number placeholders.
                let master_kind = match k {
                    0x03 => 0x01,
                    0x04 => 0x02,
                    other => other,
                };
                if title_master && matches!(k, 0x03 | 0x04) {
                    // (The title master's center title and subtitle are the layout's.)
                    let layout_kind = k;
                    let mut ls = shape.clone();
                    ls.placeholder = Some(layout_kind);
                    layout_shapes += &shape_xml(&mut ctx, &ls, m, &[], 0);
                }
                shape.placeholder = Some(master_kind);
            }
            shapes += &shape_xml(&mut ctx, &shape, m, &[], 0);
        }
        // The layout: placeholders of the master's kinds without a place (they take the master's), or the center title and subtitle.
        if !title_master {
            layout_shapes = String::from(
                r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="ja-JP" altLang="en-US"/></a:p></p:txBody></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Content"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="ja-JP" altLang="en-US"/></a:p></p:txBody></p:sp>"#,
            );
        }
        let bg = background_xml(&mut ctx, m);
        let master_xml = format!(
            r#"{HEAD}<p:sldMaster xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld>{bg}{}</p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="{}" r:id="rId1"/></p:sldLayoutIdLst>{}</p:sldMaster>"#,
            tree(&shapes),
            2_147_483_649u64 + (i as u64) * 2,
            tx_styles(&ctx)
        );
        let kind = if title_master { "title" } else { "obj" };
        let layout_xml = format!(
            r#"{HEAD}<p:sldLayout xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" type="{kind}" preserve="1"><p:cSld name="{}">{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
            if title_master { "Title Slide" } else { "Title and Content" },
            tree(&layout_shapes)
        );
        let mut rels_of_master = vec![rel("rId1", "slideLayout", &format!("../slideLayouts/slideLayout{n}.xml")), rel("rId2", "theme", &format!("../theme/theme{n}.xml"))];
        rels_of_master.extend(page_media(&ctx, &mut media, &mut files, &mut extensions));
        rels_of_master.extend(ctx.extra_rels.borrow().iter().cloned());
        files.push((format!("ppt/slideMasters/slideMaster{n}.xml"), master_xml.into_bytes()));
        files.push((format!("ppt/slideMasters/_rels/slideMaster{n}.xml.rels"), rels(&rels_of_master).into_bytes()));
        files.push((format!("ppt/slideLayouts/slideLayout{n}.xml"), layout_xml.into_bytes()));
        files.push((format!("ppt/slideLayouts/_rels/slideLayout{n}.xml.rels"), rels(&[rel("rId1", "slideMaster", &format!("../slideMasters/slideMaster{n}.xml"))]).into_bytes()));
        files.push((format!("ppt/theme/theme{n}.xml"), theme_of(&scheme).into_bytes()));
        let _ = write!(content_overrides, r#"<Override PartName="/ppt/slideMasters/slideMaster{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#);
        master_rels.push(rel(&format!("rId{n}"), "slideMaster", &format!("slideMasters/slideMaster{n}.xml")));
        let _ = write!(master_ids, r#"<p:sldMasterId id="{}" r:id="rId{n}"/>"#, 2_147_483_648u64 + (i as u64) * 2);
        // (Layout ids follow the master ids: they must all differ.)
    }
    // A file without a master gets the blank new-presentation's.
    if masters.is_empty() {
        return Err(Error::new("旧形式（.ppt）のプレゼンテーションに母艦（マスター）がありません"));
    }
    let master_count = masters.len();
    let mut have_notes = false;
    let mut notes_frames: Option<(Option<(i64, i64, i64, i64)>, Option<(i64, i64, i64, i64)>)> = None;
    for (i, slide) in ir.slides.iter().enumerate() {
        let n = i + 1;
        let master_i = master_index_of.get(&slide.master_ref).copied().unwrap_or(0);
        let master = masters[master_i];
        let own_scheme = if slide.follow_master_scheme { None } else { slide.scheme };
        let scheme = own_scheme.or(master.scheme).unwrap_or(DEFAULT_SCHEME);
        let mut ctx = Ctx { ir: &ir, scheme, styles: &master.styles, pictures: HashMap::new(), next_rel: Cell::new(2), extra_rels: RefCell::new(Vec::new()), slide_places: &slide_places, master: false, background: None };
        ctx.background = flat_background(&ctx, if slide.follow_master_background { master } else { slide });
        let shapes: String = slide.shapes.iter().map(|s| shape_xml(&mut ctx, s, slide, &slide.outline, 0)).collect();
        let bg = if slide.follow_master_background { String::new() } else { background_xml(&mut ctx, slide) };
        let mut show = String::new();
        if !slide.follow_master_objects {
            show += r#" showMasterSp="0""#;
        }
        if slide.show.is_some_and(|x| x.hidden) {
            show += r#" show="0""#;
        }
        let transition = slide.show.map(|x| transition_xml(&x)).unwrap_or_default();
        let timing = match slide.timing.as_deref().and_then(crate::ppt_timing::timing_xml) {
            Some(x) => x,
            None => {
                // PowerPoint 97 to 2000: only the effect of each shape.
                fn gather(shapes: &[crate::ppt::ShapeIr], out: &mut Vec<(u32, crate::ppt_timing::Anim)>) {
                    for s in shapes {
                        if let Some(a) = s.anim {
                            out.push((s.id, a));
                        }
                        gather(&s.children, out);
                    }
                }
                let mut effects = Vec::new();
                gather(&slide.shapes, &mut effects);
                crate::ppt_timing::simple_timing(&effects).unwrap_or_default()
            }
        };
        let transition = transition + &timing;
        let slide_xml = format!(
            r#"{HEAD}<p:sld xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"{show}><p:cSld>{bg}{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{transition}</p:sld>"#,
            tree(&shapes)
        );
        let mut slide_rels = vec![rel("rId1", "slideLayout", &format!("../slideLayouts/slideLayout{}.xml", master_i + 1))];
        slide_rels.extend(page_media(&ctx, &mut media, &mut files, &mut extensions));
        slide_rels.extend(ctx.extra_rels.borrow().iter().cloned());
        // The notes page, when it has text.
        if let Some(np) = ir.notes.get(&slide.notes_ref).filter(|np| np.shapes.iter().any(|s| s.placeholder == Some(0x0C) && (s.text.as_ref().is_some_and(|t| t.paragraphs.iter().any(|p| p.runs.iter().any(|r| !r.0.trim().is_empty()))) || s.outline_ref.is_some()))) {
            let mut nctx = Ctx { ir: &ir, scheme: np.scheme.unwrap_or(DEFAULT_SCHEME), styles: &np.styles, pictures: HashMap::new(), next_rel: Cell::new(3), extra_rels: RefCell::new(Vec::new()), slide_places: &slide_places, master: false, background: None };
            let nshapes: String = np.shapes.iter().filter(|s| matches!(s.placeholder, Some(0x0B | 0x0C))).map(|s| shape_xml(&mut nctx, s, np, &np.outline, 0)).collect();
            let notes_xml = format!(
                r#"{HEAD}<p:notes xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld>{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#,
                tree(&nshapes)
            );
            let mut notes_rels = vec![rel("rId1", "notesMaster", "../notesMasters/notesMaster1.xml"), rel("rId2", "slide", &format!("../slides/slide{n}.xml"))];
            notes_rels.extend(page_media(&nctx, &mut media, &mut files, &mut extensions));
            notes_rels.extend(nctx.extra_rels.borrow().iter().cloned());
            files.push((format!("ppt/notesSlides/notesSlide{n}.xml"), notes_xml.into_bytes()));
            files.push((format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"), rels(&notes_rels).into_bytes()));
            let _ = write!(content_overrides, r#"<Override PartName="/ppt/notesSlides/notesSlide{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml"/>"#);
            let id = format!("rId{}", ctx.next_rel.get().max(slide_rels.len() as u32 + 1));
            slide_rels.push(rel(&id, "notesSlide", &format!("../notesSlides/notesSlide{n}.xml")));
            have_notes = true;
            if notes_frames.is_none() {
                notes_frames = Some((np.shapes.iter().find(|s| s.placeholder == Some(0x0B)).and_then(|s| s.frame), np.shapes.iter().find(|s| s.placeholder == Some(0x0C)).and_then(|s| s.frame)));
            }
        }
        files.push((format!("ppt/slides/slide{n}.xml"), slide_xml.into_bytes()));
        files.push((format!("ppt/slides/_rels/slide{n}.xml.rels"), rels(&slide_rels).into_bytes()));
        let _ = write!(content_overrides, r#"<Override PartName="/ppt/slides/slide{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#);
        pres_rels.push(rel(&format!("rId{}", master_count + n), "slide", &format!("slides/slide{n}.xml")));
        let _ = write!(slide_ids, r#"<p:sldId id="{}" r:id="rId{}"/>"#, 255 + n, master_count + n);
    }
    let extra = master_count + ir.slides.len();
    pres_rels.extend(master_rels);
    pres_rels.push(rel(&format!("rId{}", extra + 1), "presProps", "presProps.xml"));
    pres_rels.push(rel(&format!("rId{}", extra + 2), "viewProps", "viewProps.xml"));
    pres_rels.push(rel(&format!("rId{}", extra + 3), "theme", "theme/theme1.xml"));
    let mut notes_master_list = String::new();
    if have_notes {
        // The notes master: the slide image and the notes text, where the notes pages have them.
        let (image, body) = notes_frames.unwrap_or((None, None));
        let frame = |f: Option<(i64, i64, i64, i64)>, d: (i64, i64, i64, i64)| {
            let (x, y, w, h) = f.unwrap_or(d);
            format!(r#"<p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>"#)
        };
        let shapes = format!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg" idx="2"/></p:nvPr></p:nvSpPr>{}</p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" sz="quarter" idx="3"/></p:nvPr></p:nvSpPr>{}<p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:endParaRPr lang="ja-JP" altLang="en-US"/></a:p></p:txBody></p:sp>"#,
            frame(image, (685_800, 1_143_000, 5_486_400, 3_086_100)),
            frame(body, (685_800, 4_400_550, 5_486_400, 3_600_450))
        );
        let notes_master = format!(
            r#"{HEAD}<p:notesMaster xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg>{}</p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/></p:notesMaster>"#,
            tree(&shapes)
        );
        let theme_n = master_count + 1;
        files.push(("ppt/notesMasters/notesMaster1.xml".to_string(), notes_master.into_bytes()));
        files.push(("ppt/notesMasters/_rels/notesMaster1.xml.rels".to_string(), rels(&[rel("rId1", "theme", &format!("../theme/theme{theme_n}.xml"))]).into_bytes()));
        files.push((format!("ppt/theme/theme{theme_n}.xml"), theme_of(&DEFAULT_SCHEME).into_bytes()));
        let _ = write!(content_overrides, r#"<Override PartName="/ppt/notesMasters/notesMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml"/><Override PartName="/ppt/theme/theme{theme_n}.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#);
        pres_rels.push(rel(&format!("rId{}", extra + 4), "notesMaster", "notesMasters/notesMaster1.xml"));
        notes_master_list = format!(r#"<p:notesMasterIdLst><p:notesMasterId r:id="rId{}"/></p:notesMasterIdLst>"#, extra + 4);
    }
    let first_num = if ir.first_slide_number == 1 { String::new() } else { format!(r#" firstSlideNum="{}""#, ir.first_slide_number) };
    let presentation = format!(
        r#"{HEAD}<p:presentation xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}" saveSubsetFonts="1"{first_num}><p:sldMasterIdLst>{master_ids}</p:sldMasterIdLst>{notes_master_list}<p:sldIdLst>{slide_ids}</p:sldIdLst><p:sldSz cx="{width}" cy="{height}"/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle><a:defPPr><a:defRPr lang="ja-JP"/></a:defPPr></p:defaultTextStyle></p:presentation>"#
    );
    let mut defaults = String::from(r#"<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#);
    for ext in &extensions {
        let mime = match *ext {
            "jpeg" => "image/jpeg",
            "png" => "image/png",
            "bmp" => "image/bmp",
            "tiff" => "image/tiff",
            "emf" => "image/x-emf",
            "wmf" => "image/x-wmf",
            _ => "application/octet-stream",
        };
        let _ = write!(defaults, r#"<Default Extension="{ext}" ContentType="{mime}"/>"#);
    }
    let content_types = format!(
        r#"{HEAD}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{defaults}<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/presProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presProps+xml"/><Override PartName="/ppt/viewProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml"/>{content_overrides}</Types>"#
    );
    // presentation.xml.rels points to theme1 once more (the first master's), as a presentation does.
    let mut writer = ibis_ooxml::zip::Writer::new();
    let mut add = |name: &str, data: &[u8]| writer.add(name, ibis_ooxml::zip::EntryMeta::default(), data).map_err(|e| Error::new(format!("{e}")));
    add("[Content_Types].xml", content_types.as_bytes())?;
    add("_rels/.rels", rels(&[rel("rId1", "officeDocument", "ppt/presentation.xml")]).as_bytes())?;
    add("ppt/presentation.xml", presentation.as_bytes())?;
    add("ppt/_rels/presentation.xml.rels", rels(&pres_rels).as_bytes())?;
    add("ppt/presProps.xml", format!(r#"{HEAD}<p:presentationPr xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"/>"#).as_bytes())?;
    add("ppt/viewProps.xml", format!(r#"{HEAD}<p:viewPr xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:normalViewPr><p:restoredLeft sz="15620"/><p:restoredTop sz="94660"/></p:normalViewPr></p:viewPr>"#).as_bytes())?;
    for (name, data) in &files {
        add(name, data)?;
    }
    writer.finish().map_err(|e| Error::new(format!("{e}")))
}

#[allow(dead_code)]
fn unused(_: &Props) {}

/// The DrawingML `ST_TextAutonumberScheme` of a `TextAutoNumberScheme` ([MS-PPT] TextAutoNumberSchemeEnum, 2.13.28): the enumeration's names
/// match the DrawingML ones one to one (ANM_CircleNumWDBWhitePlain is circleNumWdWhitePlain, the Chinese and Japanese/Korean ones are
/// the `ea1…` names); no value is without one. A value the table does not have is an Arabic numeral and period.
fn auto_number_type(scheme: u16) -> &'static str {
    match scheme {
        0x00 => "alphaLcPeriod",
        0x01 => "alphaUcPeriod",
        0x02 => "arabicParenR",
        0x03 => "arabicPeriod",
        0x04 => "romanLcParenBoth",
        0x05 => "romanLcParenR",
        0x06 => "romanLcPeriod",
        0x07 => "romanUcPeriod",
        0x08 => "alphaLcParenBoth",
        0x09 => "alphaLcParenR",
        0x0A => "alphaUcParenBoth",
        0x0B => "alphaUcParenR",
        0x0C => "arabicParenBoth",
        0x0D => "arabicPlain",
        0x0E => "romanUcParenBoth",
        0x0F => "romanUcParenR",
        0x10 => "ea1ChsPlain",
        0x11 => "ea1ChsPeriod",
        0x12 => "circleNumDbPlain",
        0x13 => "circleNumWdWhitePlain",
        0x14 => "circleNumWdBlackPlain",
        0x15 => "ea1ChtPlain",
        0x16 => "ea1ChtPeriod",
        0x17 => "arabic1Minus",
        0x18 => "arabic2Minus",
        0x19 => "hebrew2Minus",
        0x1A => "ea1JpnKorPlain",
        0x1B => "ea1JpnKorPeriod",
        0x1C => "arabicDbPlain",
        0x1D => "arabicDbPeriod",
        0x1E => "thaiAlphaPeriod",
        0x1F => "thaiAlphaParenR",
        0x20 => "thaiAlphaParenBoth",
        0x21 => "thaiNumPeriod",
        0x22 => "thaiNumParenR",
        0x23 => "thaiNumParenBoth",
        0x24 => "hindiAlphaPeriod",
        0x25 => "hindiNumPeriod",
        0x26 => "ea1JpnChsDbPeriod",
        0x27 => "hindiNumParenR",
        0x28 => "hindiAlpha1Period",
        _ => "arabicPeriod",
    }
}
