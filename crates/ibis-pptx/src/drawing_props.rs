// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! How DrawingML shape properties (`a:spPr`, `p:style`) are read: the fill, the line and the effects.
//!
//! A slide's shape, a picture, a SmartArt drawing's shape and a Word drawing (`wps:spPr`, `pic:spPr`) are all
//! written in the same DrawingML, so they are all read by these functions (the theme's line and effect styles,
//! gradients, the preset shadows, the reflection and so on). Nothing here knows which format it reads.

use ibis_doc_model::{Color, Fill, GradientPath, GradientStop, LineEnd, MediaFormat, MediaId, Stroke, TileAlign};
use roxmltree::Node;

use crate::reader::{blip_target, drawable, drawing_child, fill_element, unqualified_attribute};
use crate::theme::{Colors, effect_style, fill_style, line_style};
use crate::xml::{attr, attr_i64, child, elements};

struct PresetShadowDefaults {
    blur: i64,
    dist: i64,
    dir: i32,
    sx: i32,
    sy: i32,
    kx: i32,
    ky: i32,
    align: TileAlign,
}


fn preset_shadow_defaults(prst: &str) -> PresetShadowDefaults {
    let (blur, dist, dir) = match prst {
        "shdw1" => (0, 76200, 18_900_000),
        "shdw2" => (0, 76200, 2_700_000),
        "shdw3" => (0, 76200, 13_500_000),
        "shdw4" => (0, 76200, 8_100_000),
        "shdw5" => (0, 76200, 5_400_000),
        "shdw6" => (0, 76200, 16_200_000),
        "shdw7" => (0, 76200, 0),
        "shdw8" => (0, 76200, 10_800_000),
        "shdw9" => (0, 76200, 18_900_000),
        "shdw10" => (0, 76200, 2_700_000),
        "shdw11" => (0, 76200, 13_500_000),
        "shdw12" => (0, 76200, 8_100_000),
        "shdw13" => (0, 76200, 18_900_000),
        "shdw14" => (0, 76200, 2_700_000),
        "shdw15" => (0, 76200, 13_500_000),
        "shdw16" => (0, 76200, 8_100_000),
        "shdw17" => (0, 76200, 5_400_000),
        "shdw18" => (0, 76200, 16_200_000),
        "shdw19" => (0, 76200, 2_700_000),
        "shdw20" => (0, 76200, 18_900_000),
        _ => (0, 76200, 2_700_000),
    };
    let (sx, sy, kx, ky, align) = match prst {
        "shdw3" => (100_000, 50_000, 2_453_400, 0, TileAlign::Bottom),
        "shdw4" => (100_000, 50_000, -2_453_400, 0, TileAlign::Bottom),
        "shdw7" => (100_000, -50_000, 2_453_400, 0, TileAlign::Bottom),
        "shdw8" => (100_000, -50_000, -2_453_400, 0, TileAlign::Bottom),
        "shdw9" => (75_000, 75_000, 0, 0, TileAlign::TopLeft),
        "shdw10" => (125_000, 125_000, 0, 0, TileAlign::BottomRight),
        "shdw11" => (100_000, 50_000, 2_453_400, 0, TileAlign::Bottom),
        "shdw12" => (100_000, 50_000, -2_453_400, 0, TileAlign::Bottom),
        "shdw15" => (100_000, -50_000, 2_453_400, 0, TileAlign::Bottom),
        "shdw16" => (100_000, -50_000, -2_453_400, 0, TileAlign::Bottom),
        "shdw17" | "shdw18" => (100_000, 100_000, 0, 0, TileAlign::Center),
        "shdw19" => (100_000, 50_000, 0, 0, TileAlign::Bottom),
        "shdw20" => (100_000, -100_000, 0, 0, TileAlign::Bottom),
        _ => (100_000, 100_000, 0, 0, TileAlign::Bottom),
    };
    PresetShadowDefaults { blur, dist, dir, sx, sy, kx, ky, align }
}


/// The effects of an `a:effectLst`.
#[derive(Default)]
pub(crate) struct Effects {
    pub shadow: Option<ibis_doc_model::Shadow>,
    pub inner_shadow: Option<ibis_doc_model::Shadow>,
    pub glow: Option<(Color, i64)>,
    pub soft_edge: Option<i64>,
    pub reflection: Option<ibis_doc_model::Reflection>,
}

/// The effects `effect_lst` (an `a:effectLst`) lists. `ref_color` is the colour `phClr` stands for (the style's
/// `effectRef` colour).
pub(crate) fn effects(colors: &Colors, effect_lst: Option<Node>, ref_color: Option<Color>) -> Effects {
    let Some(list) = effect_lst else { return Effects::default() };
    let shadow = drawing_child(list, "outerShdw").or_else(|| drawing_child(list, "prstShdw")).and_then(|s| {
        let is_prst = s.tag_name().name() == "prstShdw";
        let defaults = if is_prst {
            preset_shadow_defaults(attr(s, "prst").unwrap_or(""))
        } else {
            PresetShadowDefaults { blur: 0, dist: 0, dir: 0, sx: 100_000, sy: 100_000, kx: 0, ky: 0, align: TileAlign::Bottom }
        };
        Some(ibis_doc_model::Shadow {
            color: colors.in_parent(s, ref_color)?,
            blur: attr_i64(s, "blurRad").unwrap_or(defaults.blur),
            distance: attr_i64(s, "dist").unwrap_or(defaults.dist),
            direction: attr_i64(s, "dir").unwrap_or(defaults.dir as i64) as i32,
            sx: attr_i64(s, "sx").map(|v| v as i32).unwrap_or(defaults.sx),
            sy: attr_i64(s, "sy").map(|v| v as i32).unwrap_or(defaults.sy),
            kx: attr_i64(s, "kx").map(|v| v as i32).unwrap_or(defaults.kx),
            ky: attr_i64(s, "ky").map(|v| v as i32).unwrap_or(defaults.ky),
            align: attr(s, "algn").map(TileAlign::parse).unwrap_or(defaults.align),
            rot_with_shape: attr(s, "rotWithShape").map(|v| v != "0").unwrap_or(true),
        })
    });
    let glow = drawing_child(list, "glow").and_then(|g| {
        let radius = attr_i64(g, "rad").unwrap_or(0);
        (radius > 0).then(|| colors.in_parent(g, ref_color).map(|c| (c, radius))).flatten()
    });
    let inner_shadow = drawing_child(list, "innerShdw").and_then(|s| {
        Some(ibis_doc_model::Shadow {
            color: colors.in_parent(s, ref_color)?,
            blur: attr_i64(s, "blurRad").unwrap_or(0),
            distance: attr_i64(s, "dist").unwrap_or(0),
            direction: attr_i64(s, "dir").unwrap_or(0) as i32,
            sx: 100_000,
            sy: 100_000,
            kx: 0,
            ky: 0,
            align: TileAlign::Bottom,
            rot_with_shape: true,
        })
    });
    let soft_edge = drawing_child(list, "softEdge").and_then(|e| attr_i64(e, "rad")).filter(|&r| r > 0);
    // Defaults: ECMA-376 Part 4 第 5 版 dml-main.xsd の CT_ReflectionEffect.
    let reflection = drawing_child(list, "reflection").map(|e| ibis_doc_model::Reflection {
        blur: attr_i64(e, "blurRad").unwrap_or(0),
        start_alpha: attr_i64(e, "stA").unwrap_or(100_000) as i32,
        end_alpha: attr_i64(e, "endA").unwrap_or(0) as i32,
        start_pos: attr_i64(e, "stPos").unwrap_or(0) as i32,
        end_pos: attr_i64(e, "endPos").unwrap_or(100_000) as i32,
        distance: attr_i64(e, "dist").unwrap_or(0),
        direction: attr_i64(e, "dir").unwrap_or(0) as i32,
        fade_direction: attr_i64(e, "fadeDir").unwrap_or(5_400_000) as i32,
        sx: attr_i64(e, "sx").unwrap_or(100_000) as i32,
        sy: attr_i64(e, "sy").unwrap_or(100_000) as i32,
        kx: attr_i64(e, "kx").unwrap_or(0) as i32,
        ky: attr_i64(e, "ky").unwrap_or(0) as i32,
        align: attr(e, "algn").map(TileAlign::parse).unwrap_or(TileAlign::Bottom),
        rot_with_shape: attr(e, "rotWithShape").is_none_or(|v| v != "0" && v != "false"),
    });
    Effects { shadow, inner_shadow, glow, soft_edge, reflection }
}

pub(crate) fn parse_grad_fill(colors: &crate::theme::Colors, node: Node, placeholder: Option<Color>) -> Option<Fill> {
let mut stops: Vec<GradientStop> = drawing_child(node, "gsLst")
    .into_iter()
    .flat_map(|l| elements(l).filter(|n| n.has_tag_name(("http://schemas.openxmlformats.org/drawingml/2006/main", "gs"))))
    .filter_map(|gs| {
        let color = colors.in_parent(gs, placeholder)?;
        Some(GradientStop { position: unqualified_attribute(gs, "pos").and_then(|v|v.trim().parse::<i64>().ok()).unwrap_or(0).clamp(0, 100_000) as u32, color })
    })
    .collect();
stops.sort_by_key(|s| s.position);
if stops.is_empty() {
    return None;
}
if let Some(path) = drawing_child(node, "path") {
    let kind = unqualified_attribute(path, "path").and_then(GradientPath::parse).unwrap_or(GradientPath::Circle);
    let rect = drawing_child(path, "fillToRect");
    if let Some(r) = rect {
        let has_any_attr = ["l", "t", "r", "b"].iter().any(|&k| unqualified_attribute(r, k).is_some());
        if !has_any_attr {
            return Some(match kind {
                GradientPath::Circle => Fill::Path { path: kind, focus: [0, 0, 100_000, 100_000], stops },
                _ => Fill::Solid(stops[0].color),
            });
        }
    }
    let focus = ["l", "t", "r", "b"].map(|k| rect.and_then(|r| unqualified_attribute(r, k)).and_then(|v|v.trim().parse::<i64>().ok()).unwrap_or(0) as i32);
    return Some(Fill::Path { path: kind, focus, stops });
}
let lin = drawing_child(node, "lin");
let angle = lin.and_then(|l| unqualified_attribute(l, "ang")).and_then(|v|v.trim().parse::<i64>().ok()).unwrap_or(5_400_000) as i32;
let scaled = lin.and_then(|l| unqualified_attribute(l, "scaled")).map_or(false, |v| v == "1" || v == "true");
let flip = ibis_doc_model::TileFlip::parse(unqualified_attribute(node, "flip").unwrap_or("none"));
if let Some(tr) = drawing_child(node, "tileRect") {
    let tile = ["l", "t", "r", "b"].map(|k| {
        unqualified_attribute(tr, k)
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(0) as i32
    });
    return Some(Fill::TileLinear { angle, scaled, stops, tile, flip });
}
let mut angle = angle;
match flip {
    ibis_doc_model::TileFlip::X => angle = 10_800_000 - angle,
    ibis_doc_model::TileFlip::Y => angle = -angle,
    ibis_doc_model::TileFlip::Xy => angle = angle + 10_800_000,
    _ => {}
}
angle = angle.rem_euclid(21_600_000);
Some(Fill::Linear { angle, scaled, stops })
}


/// A fill element (`a:solidFill`, `a:gradFill`, …). `placeholder` is the
/// colour `phClr` stands for; `group` is the fill `a:grpFill` refers to.
pub(crate) fn fill(colors: &Colors, node: Node, placeholder: Option<Color>, group: Option<&Fill>, media: &mut dyn FnMut(&str) -> Option<(MediaId, MediaFormat)>) -> Option<Fill> {
    match node.tag_name().name() {
        "noFill" => Some(Fill::None),
        "solidFill" => Some(Fill::Solid(colors.in_parent(node, placeholder)?)),
        "gradFill" => parse_grad_fill(colors, node, placeholder),
        "blipFill" => {
            let embed = child(node, "blip").and_then(blip_target)?;
            let (media, format) = media(embed)?;
            if !drawable(format) {
                return Some(Fill::None);
            }
            let mut blip = ibis_doc_model::BlipFill::new(media);
            blip.rot_with_shape = unqualified_attribute(node, "rotWithShape").is_none_or(|v| v != "0" && v != "false");
            if let Some(r) = child(node, "srcRect") {
                let v = |name| attr_i64(r, name).unwrap_or(0) as i32;
                blip.crop = ibis_doc_model::Crop { left: v("l"), top: v("t"), right: v("r"), bottom: v("b") };
            }
            blip.fill_rect = child(node, "stretch").and_then(|s| child(s, "fillRect")).map(|r| ["l", "t", "r", "b"].map(|k| attr_i64(r, k).unwrap_or(0) as i32)).filter(|r| *r != [0, 0, 0, 0]);
            if let Some(b) = child(node, "blip") {
                blip.duotone = drawing_child(b, "duotone").and_then(|d| {
                    let colors: Vec<_> = elements(d).filter_map(|c| colors.resolve(c, placeholder)).collect();
                    Some((colors.first()?.rgb, colors.get(1)?.rgb))
                });
                blip.luminance = drawing_child(b, "lum").and_then(|l| {
                    let bright = attr_i64(l, "bright").unwrap_or(0).clamp(-100_000, 100_000) as i32;
                    let contrast = attr_i64(l, "contrast").unwrap_or(0).clamp(-100_000, 100_000) as i32;
                    (bright != 0 || contrast != 0).then_some((bright, contrast))
                });
                blip.effects.grayscale = drawing_child(b, "grayscl").is_some();
                blip.effects.bilevel = drawing_child(b, "biLevel").map(|l| attr_i64(l, "thresh").unwrap_or(50_000).clamp(0, 100_000) as i32);
                blip.effects.opacity = drawing_child(b, "alphaModFix")
                    .map(|a| attr_i64(a, "amt").unwrap_or(100_000))
                    .or_else(|| drawing_child(b, "alphaRepl").and_then(|a| attr_i64(a, "a")))
                    .map(|v| v.clamp(0, 100_000) as i32)
                    .filter(|v| *v != 100_000);
                blip.effects.replace = drawing_child(b, "clrRepl").and_then(|r| elements(r).next()).and_then(|c| colors.resolve(c, placeholder)).map(|c| c.rgb);
            }
            let tile = child(node, "tile").map(|tile| ibis_doc_model::PictureTile {
                tx: attr_i64(tile, "tx").unwrap_or(0),
                ty: attr_i64(tile, "ty").unwrap_or(0),
                sx: attr_i64(tile, "sx").unwrap_or(100_000) as i32,
                sy: attr_i64(tile, "sy").unwrap_or(100_000) as i32,
                flip: ibis_doc_model::TileFlip::parse(unqualified_attribute(tile, "flip").unwrap_or("none")),
                align: ibis_doc_model::TileAlign::parse(unqualified_attribute(tile, "algn").unwrap_or("tl")),
            });
            blip.tile = tile;
            if blip.is_plain() {
                Some(match tile {
                    Some(t) => Fill::TileImage { media, tx: t.tx, ty: t.ty, sx: t.sx, sy: t.sy, flip: t.flip, align: t.align },
                    None => Fill::Image(media),
                })
            } else {
                Some(Fill::Blip(Box::new(blip)))
            }
        }
        "pattFill" => {
            let fg = drawing_child(node, "fgClr").and_then(|c| colors.in_parent(c, placeholder)).unwrap_or(Color::BLACK);
            let bg = drawing_child(node, "bgClr").and_then(|c| colors.in_parent(c, placeholder)).unwrap_or(Color::WHITE);
            let preset = unqualified_attribute(node, "prst").unwrap_or("pct5");
            Some(match ibis_doc_model::pattern_tile(preset) {
                Some(_) => Fill::Pattern { preset: preset.to_string(), foreground: fg, background: bg },
                None => Fill::Solid(fg),
            })
        }
        "grpFill" => Some(group.cloned().unwrap_or_default()),
        _ => None,
    }
}


/// The fill a shape style (`p:style/a:fillRef`) gives.
pub(crate) fn style_fill(colors: &Colors, theme: Option<Node>, style: Option<Node>, media: &mut dyn FnMut(&str) -> Option<(MediaId, MediaFormat)>) -> Fill {
    let Some(reference) = style.and_then(|s| child(s, "fillRef")) else { return Fill::None };
    let idx = attr_i64(reference, "idx").unwrap_or(0).max(0) as usize;
    if idx == 0 {
        return Fill::None;
    }
    let color = colors.in_parent(reference, None);
    let themed = theme.and_then(|t| fill_style(t, idx)).and_then(|f| fill(colors, f, color, None, media));
    themed.or(color.map(Fill::Solid)).unwrap_or_default()
}


pub(crate) fn stroke(colors: &Colors, theme: Option<Node>, ln: Option<Node>, style: Option<Node>) -> Option<Stroke> {
    let reference = style.and_then(|s| child(s, "lnRef"));
    let ref_color = reference.and_then(|r| colors.in_parent(r, None));
    let ref_idx = reference.and_then(|r| attr_i64(r, "idx")).unwrap_or(0).max(0) as usize;
    let themed = if ref_idx > 0 { theme.and_then(|t| line_style(t, ref_idx)) } else { None };

    // `Some(None)` is an explicit "no line".
    let color_of = |ln: Node| -> Option<Option<Color>> {
        let fill = fill_element(ln)?;
        Some(match fill.tag_name().name() {
            "noFill" => None,
            "solidFill" => colors.in_parent(fill, ref_color),
            "gradFill" => child(fill, "gsLst").and_then(|l| elements(l).next()).and_then(|gs| colors.in_parent(gs, ref_color)),
            _ => ref_color,
        })
    };
    // A style's line reference with index 0 is "no line", whatever
    // colour it names.
    let referenced = if ref_idx > 0 { ref_color.map(Some) } else { None };
    let color = ln.and_then(color_of).or_else(|| themed.and_then(color_of)).or(referenced)??;
    let width = ln.and_then(|l| attr_i64(l, "w")).or_else(|| themed.and_then(|l| attr_i64(l, "w"))).unwrap_or(Stroke::DEFAULT_WIDTH);
    let dash = ln.and_then(|l| drawing_child(l, "prstDash")).or_else(|| themed.and_then(|l| drawing_child(l, "prstDash")));
    let end = |name| {
        let e = ln.and_then(|l| drawing_child(l, name));
        LineEnd::parse(e.and_then(|e| attr(e, "type")), e.and_then(|e| attr(e, "w")), e.and_then(|e| attr(e, "len")))
    };
    let dash_kind = dash.and_then(|d| attr(d, "val")).and_then(ibis_doc_model::LineDash::parse).unwrap_or_default();
    // A `cap` of its own, else the theme line's; with none, PowerPoint
    // draws flat ends (the coverage suite PSHP; the standard says `sq`).
    let cap = ln.and_then(|l| attr(l, "cap")).or_else(|| themed.and_then(|l| attr(l, "cap")));
    let cap = match cap {
        Some("rnd") => ibis_doc_model::LineCap::Round,
        Some("sq") => ibis_doc_model::LineCap::Square,
        _ => ibis_doc_model::LineCap::Flat,
    };
    let join_of = |l: Node| {
        if drawing_child(l, "round").is_some() {
            Some(ibis_doc_model::LineJoin::Round)
        } else if drawing_child(l, "bevel").is_some() {
            Some(ibis_doc_model::LineJoin::Bevel)
        } else if drawing_child(l, "miter").is_some() {
            Some(ibis_doc_model::LineJoin::Miter)
        } else {
            None
        }
    };
    let compound = ln.and_then(|l| attr(l, "cmpd")).or_else(|| themed.and_then(|l| attr(l, "cmpd"))).and_then(ibis_doc_model::LineCompound::parse).unwrap_or_default();
    let default_join = ibis_doc_model::LineJoin::Miter;
    let join = ln.and_then(join_of).or_else(|| themed.and_then(join_of)).unwrap_or(default_join);
    let line_fill = ln.and_then(fill_element).and_then(|f| {
        if f.tag_name().name() == "gradFill" {
            parse_grad_fill(colors, f, ref_color)
        } else {
            None
        }
    });
    Some(Stroke {
        color,
        width,
        dashed: dash_kind != ibis_doc_model::LineDash::Solid,
        head: end("headEnd"),
        tail: end("tailEnd"),
        dash: dash_kind,
        cap,
        join,
        compound,
        alignment: ln.and_then(|l| attr(l, "algn")).or_else(|| themed.and_then(|l| attr(l, "algn"))).and_then(ibis_doc_model::LineAlignment::parse).unwrap_or_default(),
        fill: line_fill,
    })
}


/// The effects and the bevel of a shape. `sp_prs` are the shape's `a:spPr` and those it inherits (a placeholder's
/// layout and master), the shape's own first; `style` is its `p:style`/`wps:style`, whose `effectRef` names the theme's
/// effect style that applies when no `a:spPr` lists effects (and none has an `a:effectDag`).
pub(crate) fn shape_effects<'d, 'i>(colors: &Colors, theme: Option<Node<'d, 'i>>, sp_prs: &[Node<'d, 'i>], style: Option<Node<'d, 'i>>) -> (Effects, Option<ibis_doc_model::Bevel>) {
    let effect_ref = style.and_then(|s| child(s, "effectRef"));
    let effect_idx = effect_ref.and_then(|r| attr_i64(r, "idx")).unwrap_or(0).max(0) as usize;
    let effect_ref_color = effect_ref.and_then(|r| colors.in_parent(r, None));
    let themed_effect_style = if effect_idx > 0 { theme.and_then(|t| effect_style(t, effect_idx)) } else { None };
    let has_effect_dag = sp_prs.iter().any(|p| drawing_child(*p, "effectDag").is_some());
    let effect_lst = sp_prs
        .iter()
        .find_map(|p| drawing_child(*p, "effectLst"))
        .or_else(|| (!has_effect_dag).then(|| themed_effect_style.and_then(|s| drawing_child(s, "effectLst"))).flatten());
    let sp3d_node = sp_prs.iter().find_map(|p| drawing_child(*p, "sp3d")).or_else(|| themed_effect_style.and_then(|s| drawing_child(s, "sp3d")));
    let bevel = sp3d_node.and_then(|s| drawing_child(s, "bevelT")).map(|b| {
        let preset = attr(b, "prst").and_then(ibis_doc_model::BevelPreset::parse).unwrap_or_default();
        let width = attr_i64(b, "w").unwrap_or(ibis_doc_model::Bevel::DEFAULT_SIZE);
        let height = attr_i64(b, "h").unwrap_or(ibis_doc_model::Bevel::DEFAULT_SIZE);
        ibis_doc_model::Bevel { preset, width, height }
    });
    (effects(colors, effect_lst, effect_ref_color), bevel)
}

/// The colour an `a:clrChange` makes clear (a change to a colour with no opacity: 透明色を指定).
pub(crate) fn clear_color(colors: &Colors, clr_change: Node) -> Option<ibis_doc_model::Rgb> {
    let to = child(clr_change, "clrTo").and_then(|t| elements(t).next())?;
    let clear = child(to, "alpha").and_then(|a| attr_i64(a, "val")).is_some_and(|v| v == 0);
    let from = child(clr_change, "clrFrom").and_then(|f| colors.in_parent(f, None))?;
    clear.then_some(from.rgb)
}

/// The colour and opacity effects an `a:blip` lists: grey, black and white, one colour for all, a colour changed, the
/// opacity, the saturation.
pub(crate) fn blip_effects(colors: &Colors, blip: Node) -> ibis_doc_model::BlipEffects {
    let mut e = ibis_doc_model::BlipEffects::default();
    e.grayscale = drawing_child(blip, "grayscl").is_some();
    e.bilevel = drawing_child(blip, "biLevel").map(|l| attr_i64(l, "thresh").unwrap_or(50_000).clamp(0, 100_000) as i32);
    e.replace = drawing_child(blip, "clrRepl").and_then(|r| elements(r).next()).and_then(|c| colors.resolve(c, None)).map(|c| c.rgb);
    e.change = drawing_child(blip, "clrChange").and_then(|c| {
        let to = child(c, "clrTo").and_then(|t| elements(t).next())?;
        let transparent = child(to, "alpha").and_then(|a| attr_i64(a, "val")).is_some_and(|v| v == 0);
        if transparent {
            return None;
        }
        let from = child(c, "clrFrom").and_then(|f| colors.in_parent(f, None))?;
        Some((from.rgb, colors.resolve(to, None)?.rgb))
    });
    e.opacity = drawing_child(blip, "alphaModFix")
        .map(|a| attr_i64(a, "amt").unwrap_or(100_000))
        .or_else(|| drawing_child(blip, "alphaRepl").and_then(|a| attr_i64(a, "a")))
        .map(|v| v.clamp(0, 100_000) as i32)
        .filter(|v| *v != 100_000);
    e.saturation = drawing_child(blip, "satMod").and_then(|s| attr_i64(s, "amt")).map(|v| v.clamp(0, 400_000) as i32);
    e
}
