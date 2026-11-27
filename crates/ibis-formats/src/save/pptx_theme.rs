// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! テーマの色とフォント: the colour scheme and the font scheme of the themes
//! of a presentation's masters, written from the model's
//! `Presentation::theme_colors` and `theme_fonts`.

use std::fmt::Write as _;

use ibis_doc_model::{ObjectDefaults, Rgb, ShapeDefault, ThemeFonts};
use ibis_ooxml::{NodeId, Package};

use super::pptx_new::{color_xml, fill_xml, line_xml, Prefixes};
use crate::Error;

const REL_SLIDE_MASTER: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster";
const REL_THEME: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme";

/// The theme parts of the masters of `presentation`.
pub(crate) fn theme_parts(package: &Package, presentation: &str) -> Result<Vec<String>, Error> {
    let mut out = Vec::new();
    for master in package.relationships(presentation)?.into_iter().filter(|r| r.kind == REL_SLIDE_MASTER && !r.external).filter_map(|r| r.target_part(presentation)) {
        for rel in package.relationships(&master)?.into_iter().filter(|r| r.kind == REL_THEME && !r.external) {
            if let Some(part) = rel.target_part(&master)
                && package.contains(&part)
                && !out.contains(&part)
            {
                out.push(part);
            }
        }
    }
    Ok(out)
}

/// Write `colors` (by scheme name) and `fonts` into a theme XML document.
pub(crate) fn write_theme_elements(doc: &mut ibis_ooxml::XmlDocument, colors: &[(String, Rgb)], fonts: Option<&ThemeFonts>) {
    let Some(root) = doc.root_element() else { return };
    let Some(elements) = doc.child(root, "themeElements") else { return };
    if let Some(scheme) = doc.child(elements, "clrScheme") {
        for (name, rgb) in colors {
            let Some(entry) = doc.child(scheme, name) else { continue };
            let Some(value) = doc.elements(entry).next() else { continue };
            let hex = format!("{:06X}", rgb.0 & 0xffffff);
            if doc.local_name(value) == "srgbClr" {
                if doc.attr(value, "val").is_some_and(|v| v.eq_ignore_ascii_case(&hex)) { continue }
            } else {
                // A system colour (`a:sysClr`) becomes a plain one.
                let name = match doc.prefix(value) { Some(p) => format!("{p}:srgbClr"), None => "srgbClr".to_string() };
                doc.rename(value, &name);
                doc.remove_attr(value, "lastClr");
            }
            doc.set_attr(value, "val", &hex);
        }
    }
    let (Some(fonts), Some(scheme)) = (fonts, doc.child(elements, "fontScheme")) else { return };
    for (group, set) in [("majorFont", &fonts.major), ("minorFont", &fonts.minor)] {
        let Some(group) = doc.child(scheme, group) else { continue };
        for (script, face) in [("latin", &set.latin), ("ea", &set.east_asian)] {
            if let (Some(node), Some(face)) = (doc.child(group, script), face) {
                doc.set_attr(node, "typeface", face);
            }
        }
        // The font for Japanese text, when the East Asian one is named.
        if let Some(face) = &set.east_asian
            && !face.is_empty()
        {
            let jpan: Vec<_> = doc.children_named(group, "font").filter(|&f| doc.attr(f, "script") == Some("Jpan")).collect();
            for node in jpan {
                doc.set_attr(node, "typeface", face);
            }
        }
    }
}

fn effect_list_xml(x: &Prefixes, shape: &ShapeDefault) -> Result<String, Error> {
    let a = &x.a;
    let mut effects = String::new();
    if let Some((color, radius)) = shape.glow {
        write!(effects, "<{a}glow rad=\"{radius}\">{}</{a}glow>", color_xml(x, color)).unwrap();
    }
    if let Some(shadow) = &shape.shadow {
        let mut attrs = format!(" blurRad=\"{}\" dist=\"{}\" dir=\"{}\"", shadow.blur, shadow.distance, shadow.direction);
        if shadow.sx != 100_000 { write!(attrs, " sx=\"{}\"", shadow.sx).unwrap(); }
        if shadow.sy != 100_000 { write!(attrs, " sy=\"{}\"", shadow.sy).unwrap(); }
        if shadow.kx != 0 { write!(attrs, " kx=\"{}\"", shadow.kx).unwrap(); }
        if shadow.ky != 0 { write!(attrs, " ky=\"{}\"", shadow.ky).unwrap(); }
        if shadow.align != ibis_doc_model::TileAlign::Bottom {
            let algn = match shadow.align {
                ibis_doc_model::TileAlign::TopLeft => "tl",
                ibis_doc_model::TileAlign::Top => "t",
                ibis_doc_model::TileAlign::TopRight => "tr",
                ibis_doc_model::TileAlign::Left => "l",
                ibis_doc_model::TileAlign::Center => "ctr",
                ibis_doc_model::TileAlign::Right => "r",
                ibis_doc_model::TileAlign::BottomLeft => "bl",
                ibis_doc_model::TileAlign::Bottom => "b",
                ibis_doc_model::TileAlign::BottomRight => "br",
            };
            write!(attrs, " algn=\"{algn}\"").unwrap();
        }
        if !shadow.rot_with_shape { write!(attrs, " rotWithShape=\"0\"").unwrap(); }
        write!(effects, "<{a}outerShdw{attrs}>{}</{a}outerShdw>", color_xml(x, shadow.color)).unwrap();
    }
    if let Some(r) = &shape.reflection {
        let algn = match r.align {
            ibis_doc_model::TileAlign::TopLeft => "tl",
            ibis_doc_model::TileAlign::Top => "t",
            ibis_doc_model::TileAlign::TopRight => "tr",
            ibis_doc_model::TileAlign::Left => "l",
            ibis_doc_model::TileAlign::Center => "ctr",
            ibis_doc_model::TileAlign::Right => "r",
            ibis_doc_model::TileAlign::BottomLeft => "bl",
            ibis_doc_model::TileAlign::Bottom => "b",
            ibis_doc_model::TileAlign::BottomRight => "br",
        };
        write!(
            effects,
            "<{a}reflection blurRad=\"{}\" stA=\"{}\" stPos=\"{}\" endA=\"{}\" endPos=\"{}\" dist=\"{}\" dir=\"{}\" fadeDir=\"{}\" sx=\"{}\" sy=\"{}\" kx=\"{}\" ky=\"{}\" algn=\"{algn}\" rotWithShape=\"{}\"/>",
            r.blur, r.start_alpha, r.start_pos, r.end_alpha, r.end_pos, r.distance, r.direction, r.fade_direction, r.sx, r.sy, r.kx, r.ky, u8::from(r.rot_with_shape)
        ).unwrap();
    }
    if let Some(radius) = shape.soft_edge {
        write!(effects, "<{a}softEdge rad=\"{radius}\"/>").unwrap();
    }
    Ok(if effects.is_empty() { effects } else { format!("<{a}effectLst>{effects}</{a}effectLst>") })
}

/// `a:spDef` (the shape) or `a:txDef` (the text box) of `a:objectDefaults`: the fill, line and effects in `a:spPr`, the anchor, margins and wrapping
/// in `a:bodyPr`, the text in `a:lstStyle`. A shape's definition gets the `a:style` PowerPoint's own has (the theme's references for the line, fill,
/// effects and font; the values above are explicit, so the references only stand behind them); a text box's has none.
fn write_shape_default(doc: &mut ibis_ooxml::XmlDocument, x: &Prefixes, obj_def_node: NodeId, name: &str, shape_def: &ShapeDefault, text_box: bool) -> Result<(), Error> {
    let a = &x.a;
    let def_node = if let Some(n) = doc.child(obj_def_node, name) {
        n
    } else {
        let n = x.import(doc, &format!("<{a}{name}><{a}spPr/><{a}bodyPr/><{a}lstStyle/></{a}{name}>"))?;
        // spDef, lnDef, txDef, in that order (CT_ObjectStyleDefaults).
        let order = ["spDef", "lnDef", "txDef"];
        let rank = order.iter().position(|o| *o == name).unwrap_or(0);
        let before = doc.elements(obj_def_node).find(|&c| order.iter().position(|o| *o == doc.local_name(c)).is_some_and(|r| r > rank));
        match before {
            Some(next) => doc.insert_before(next, n),
            None => {
                let count = doc.children(obj_def_node).len();
                doc.insert(obj_def_node, count, n);
            }
        }
        n
    };

    let sp_pr_node = match doc.child(def_node, "spPr") {
        Some(n) => n,
        None => doc.append_element(def_node, &format!("{a}spPr")),
    };
    let to_remove: Vec<NodeId> = doc.elements(sp_pr_node).filter(|&c| matches!(doc.local_name(c), "noFill" | "solidFill" | "gradFill" | "blipFill" | "pattFill" | "ln" | "effectLst")).collect();
    for n in to_remove {
        doc.detach(n);
    }
    let fill_node = x.import(doc, &fill_xml(x, &shape_def.fill)?)?;
    doc.insert(sp_pr_node, doc.children(sp_pr_node).len(), fill_node);
    let ln_node = x.import(doc, &line_xml(x, shape_def.stroke.as_ref()))?;
    doc.insert(sp_pr_node, doc.children(sp_pr_node).len(), ln_node);
    let effects_str = effect_list_xml(x, shape_def)?;
    if !effects_str.is_empty() {
        let effect_node = x.import(doc, &effects_str)?;
        doc.insert(sp_pr_node, doc.children(sp_pr_node).len(), effect_node);
    }

    // The body: its attributes are set, what else the file has in it (an autofit) stays.
    let body_pr = match doc.child(def_node, "bodyPr") {
        Some(n) => n,
        None => doc.append_element(def_node, &format!("{a}bodyPr")),
    };
    let i = shape_def.insets;
    doc.set_attr(body_pr, "wrap", if shape_def.wrap { "square" } else { "none" });
    doc.set_attr(body_pr, "lIns", &i.left.to_string());
    doc.set_attr(body_pr, "tIns", &i.top.to_string());
    doc.set_attr(body_pr, "rIns", &i.right.to_string());
    doc.set_attr(body_pr, "bIns", &i.bottom.to_string());
    doc.set_attr(body_pr, "rtlCol", "0");
    doc.set_attr(body_pr, "anchor", match shape_def.anchor {
        ibis_doc_model::VerticalAnchor::Top => "t",
        ibis_doc_model::VerticalAnchor::Middle => "ctr",
        ibis_doc_model::VerticalAnchor::Bottom => "b",
    });

    let lst_style_node = match doc.child(def_node, "lstStyle") {
        Some(n) => {
            let old: Vec<NodeId> = doc.children(n).to_vec();
            for c in old {
                doc.detach(c);
            }
            n
        }
        None => doc.append_element(def_node, &format!("{a}lstStyle")),
    };
    let algn_attr = match shape_def.text.align {
        ibis_doc_model::TextAlign::Left => " algn=\"l\"",
        ibis_doc_model::TextAlign::Right => " algn=\"r\"",
        ibis_doc_model::TextAlign::Justify => " algn=\"just\"",
        _ => " algn=\"ctr\"",
    };
    let bold_attr = if shape_def.text.bold { " b=\"1\"" } else { " b=\"0\"" };
    let italic_attr = if shape_def.text.italic { " i=\"1\"" } else { " i=\"0\"" };
    let size_attr = format!(" sz=\"{}\"", shape_def.text.font_size);
    let mut text_colour = ibis_doc_model::Color::opaque(shape_def.text.color);
    text_colour.theme = shape_def.text.color_theme;
    let color_xml = format!("<{a}solidFill>{}</{a}solidFill>", color_xml(x, text_colour));
    let font_xml = shape_def.text.font.as_deref().map(|f| format!("<{a}latin typeface=\"{}\"/>", xml_escape_attr(f))).unwrap_or_default();
    let def_p_pr_str = format!("<{a}defPPr{algn_attr}><{a}defRPr{bold_attr}{italic_attr}{size_attr}>{color_xml}{font_xml}</{a}defRPr></{a}defPPr>");
    let def_p_pr_node = x.import(doc, &def_p_pr_str)?;
    doc.insert(lst_style_node, 0, def_p_pr_node);

    // `a:style` (optional; when there it must hold all four references).
    match doc.child(def_node, "style") {
        Some(n) if doc.children(n).is_empty() => doc.detach(n),
        _ => {}
    }
    if !text_box && doc.child(def_node, "style").is_none() {
        let style = x.import(doc, &format!("<{a}style><{a}lnRef idx=\"1\"><{a}schemeClr val=\"accent1\"/></{a}lnRef><{a}fillRef idx=\"3\"><{a}schemeClr val=\"accent1\"/></{a}fillRef><{a}effectRef idx=\"2\"><{a}schemeClr val=\"accent1\"/></{a}effectRef><{a}fontRef idx=\"minor\"><{a}schemeClr val=\"lt1\"/></{a}fontRef></{a}style>"))?;
        let count = doc.children(def_node).len();
        doc.insert(def_node, count, style);
    }
    Ok(())
}

fn xml_escape_attr(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('"', "&quot;")
}

pub(crate) fn write_object_defaults(doc: &mut ibis_ooxml::XmlDocument, defaults: &ObjectDefaults) -> Result<(), Error> {
    if defaults.shape.is_none() && defaults.line.is_none() && defaults.text_box.is_none() {
        return Ok(());
    }
    let Some(root) = doc.root_element() else { return Ok(()) };
    let x = Prefixes::at(doc, root);
    let a = &x.a;

    let obj_def_node = if let Some(n) = doc.child(root, "objectDefaults") {
        n
    } else {
        let obj_def_str = format!("<{a}objectDefaults/>");
        let new_node = x.import(doc, &obj_def_str)?;
        if let Some(theme_elem) = doc.child(root, "themeElements") {
            doc.insert_after(theme_elem, new_node);
        } else {
            let count = doc.children(root).len();
            doc.insert(root, count, new_node);
        }
        new_node
    };

    if let Some(shape_def) = &defaults.shape {
        write_shape_default(doc, &x, obj_def_node, "spDef", shape_def, false)?;
    }
    if let Some(text_box) = &defaults.text_box {
        write_shape_default(doc, &x, obj_def_node, "txDef", text_box, true)?;
    }

    if let Some(stroke) = &defaults.line {
        let ln_def_node = if let Some(n) = doc.child(obj_def_node, "lnDef") {
            n
        } else {
            let ln_def_str = format!("<{a}lnDef><{a}spPr/><{a}bodyPr/><{a}lstStyle/></{a}lnDef>");
            let n = x.import(doc, &ln_def_str)?;
            // After spDef, before txDef.
            match doc.child(obj_def_node, "txDef") {
                Some(next) => doc.insert_before(next, n),
                None => {
                    let count = doc.children(obj_def_node).len();
                    doc.insert(obj_def_node, count, n);
                }
            }
            n
        };

        let sp_pr_node = if let Some(n) = doc.child(ln_def_node, "spPr") {
            n
        } else {
            doc.append_element(ln_def_node, &format!("{a}spPr"))
        };

        let to_remove: Vec<NodeId> = doc.elements(sp_pr_node).filter(|&c| doc.local_name(c) == "ln").collect();
        for n in to_remove {
            doc.detach(n);
        }

        let ln_str = line_xml(&x, Some(stroke));
        let ln_node = x.import(doc, &ln_str)?;
        doc.insert(sp_pr_node, doc.children(sp_pr_node).len(), ln_node);

        if doc.child(ln_def_node, "bodyPr").is_none() {
            doc.append_element(ln_def_node, &format!("{a}bodyPr"));
        }
        if doc.child(ln_def_node, "lstStyle").is_none() {
            doc.append_element(ln_def_node, &format!("{a}lstStyle"));
        }
        match doc.child(ln_def_node, "style") {
            Some(n) if doc.children(n).is_empty() => doc.detach(n),
            _ => {}
        }
        if doc.child(ln_def_node, "style").is_none() {
            let style = x.import(doc, &format!("<{a}style><{a}lnRef idx=\"1\"><{a}schemeClr val=\"accent1\"/></{a}lnRef><{a}fillRef idx=\"0\"><{a}schemeClr val=\"accent1\"/></{a}fillRef><{a}effectRef idx=\"0\"><{a}schemeClr val=\"accent1\"/></{a}effectRef><{a}fontRef idx=\"minor\"><{a}schemeClr val=\"tx1\"/></{a}fontRef></{a}style>"))?;
            let count = doc.children(ln_def_node).len();
            doc.insert(ln_def_node, count, style);
        }
    }

    Ok(())
}

/// Write `defaults` into every theme.
pub(crate) fn write_defaults(package: &mut Package, presentation: &str, defaults: &ObjectDefaults) -> Result<(), Error> {
    for part in theme_parts(package, presentation)? {
        let doc = package.xml_mut(&part)?;
        write_object_defaults(doc, defaults)?;
    }
    Ok(())
}

/// Write `colors` (by scheme name) and `fonts` into every theme.
pub(crate) fn write(package: &mut Package, presentation: &str, colors: &[(String, Rgb)], fonts: Option<&ThemeFonts>) -> Result<(), Error> {
    for part in theme_parts(package, presentation)? {
        let doc = package.xml_mut(&part)?;
        write_theme_elements(doc, colors, fonts);
    }
    Ok(())
}

/// Write theme format/effects scheme (`a:fmtScheme`) into a theme document.
pub(crate) fn write_theme_effects(doc: &mut ibis_ooxml::XmlDocument, preset_id_or_name: &str) -> Result<(), Error> {
    use crate::theme_effects::{find_effect_preset, generate_fmt_scheme_xml};
    let Some(preset) = find_effect_preset(preset_id_or_name) else { return Ok(()) };
    let Some(root) = doc.root_element() else { return Ok(()) };
    let Some(elements) = doc.child(root, "themeElements") else { return Ok(()) };
    let x = Prefixes::at(doc, root);
    let fmt_xml = generate_fmt_scheme_xml(preset);
    let new_fmt = x.import(doc, &fmt_xml)?;
    if let Some(old_fmt) = doc.child(elements, "fmtScheme") {
        doc.insert_after(old_fmt, new_fmt);
        doc.detach(old_fmt);
    } else {
        let count = doc.children(elements).len();
        doc.insert(elements, count, new_fmt);
    }
    Ok(())
}

/// Write theme effects into every theme of the presentation.
pub(crate) fn write_effects(package: &mut Package, presentation: &str, effects: Option<&str>) -> Result<(), Error> {
    let Some(effects) = effects else { return Ok(()) };
    for part in theme_parts(package, presentation)? {
        let doc = package.xml_mut(&part)?;
        write_theme_effects(doc, effects)?;
    }
    Ok(())
}
