// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! グラフの種類の変更: a chart's one group turned into another of the bar
//! (縦棒・横棒), line (折れ線) and area (面) kinds, and into another
//! grouping (集合・積み上げ・100% 積み上げ), in the chart part: the group
//! element renamed and given the children its new kind has, its series
//! theirs (a line's `c:marker`, a bar's `c:invertIfNegative`, the fill of a
//! bar or area becoming the line's colour and back), and the axes' places.

use ibis_doc_model::{ChartGroup, ChartType, Grouping};
use ibis_ooxml::{NodeId, XmlDocument};

use crate::Error;
use crate::save::unsupported;

const BAR: &[&str] = &["barDir", "grouping", "varyColors", "ser", "dLbls", "gapWidth", "overlap", "serLines", "axId", "extLst"];
const LINE: &[&str] = &["grouping", "varyColors", "ser", "dLbls", "dropLines", "hiLowLines", "upDownBars", "marker", "smooth", "axId", "extLst"];
const AREA: &[&str] = &["grouping", "varyColors", "ser", "dLbls", "dropLines", "axId", "extLst"];
const PIE: &[&str] = &["varyColors", "ser", "dLbls", "firstSliceAng", "extLst"];
const DOUGHNUT: &[&str] = &["varyColors", "ser", "dLbls", "firstSliceAng", "holeSize", "extLst"];
const SCATTER: &[&str] = &["scatterStyle", "varyColors", "ser", "dLbls", "axId", "extLst"];
const BUBBLE: &[&str] = &["varyColors", "ser", "dLbls", "bubbleScale", "showNegBubbles", "sizeRepresents", "axId", "extLst"];
const BAR_SER: &[&str] = &["idx", "order", "tx", "spPr", "invertIfNegative", "pictureOptions", "dPt", "dLbls", "trendline", "errBars", "cat", "val", "shape", "extLst"];
const LINE_SER: &[&str] = &["idx", "order", "tx", "spPr", "marker", "dPt", "dLbls", "trendline", "errBars", "cat", "val", "smooth", "extLst"];
const AREA_SER: &[&str] = &["idx", "order", "tx", "spPr", "pictureOptions", "dPt", "dLbls", "trendline", "errBars", "cat", "val", "extLst"];
const PIE_SER: &[&str] = &["idx", "order", "tx", "spPr", "explosion", "dPt", "dLbls", "cat", "val", "extLst"];
const DOUGHNUT_SER: &[&str] = &["idx", "order", "tx", "spPr", "explosion", "dPt", "dLbls", "cat", "val", "extLst"];
const SCATTER_SER: &[&str] = &["idx", "order", "tx", "spPr", "marker", "dPt", "dLbls", "trendline", "errBars", "xVal", "yVal", "smooth", "extLst"];
const BUBBLE_SER: &[&str] = &["idx", "order", "tx", "spPr", "invertIfNegative", "dPt", "dLbls", "trendline", "errBars", "xVal", "yVal", "bubbleSize", "bubble3D", "extLst"];
const SURFACE: &[&str] = &["wireframe", "ser", "bandFmts", "axId", "extLst"];
const SURFACE_SER: &[&str] = &["idx", "order", "tx", "spPr", "cat", "val", "extLst"];

#[derive(Clone, Copy, PartialEq)]
enum Family {
    Bar,
    Line,
    Area,
    Pie,
    Doughnut,
    Scatter,
    Bubble,
    Surface,
}

/// Whether a group of kind `old` becoming `new` is for `convert_group`
/// (not a kind it has no family for, which stays the same kind: a radar
/// chart's style or an of-pie chart's settings, written by the caller).
pub(crate) fn is_conversion(old: ChartType, new: ChartType) -> bool {
    family(old).is_some() || family(new).is_some() || std::mem::discriminant(&old) != std::mem::discriminant(&new)
}

fn family(kind: ChartType) -> Option<Family> {
    match kind {
        ChartType::Bar { .. } => Some(Family::Bar),
        ChartType::Line => Some(Family::Line),
        ChartType::Area => Some(Family::Area),
        ChartType::Pie { .. } => Some(Family::Pie),
        ChartType::Doughnut { .. } => Some(Family::Doughnut),
        ChartType::Scatter { .. } => Some(Family::Scatter),
        ChartType::Bubble { .. } => Some(Family::Bubble),
        ChartType::Surface { .. } => Some(Family::Surface),
        _ => None,
    }
}

fn prefix(doc: &XmlDocument, node: NodeId) -> String {
    doc.name(node).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default()
}

/// A new child `name` of `parent`, where the schema's `order` puts it.
fn insert_ordered(doc: &mut XmlDocument, parent: NodeId, name: &str, order: &[&str]) -> NodeId {
    let pos = order.iter().position(|n| *n == name).unwrap_or(order.len());
    let at = doc
        .children(parent)
        .iter()
        .rposition(|&c| doc.is_element(c) && order.iter().position(|n| *n == doc.local_name(c)).is_some_and(|p| p <= pos))
        .map_or(0, |i| i + 1);
    let full = format!("{}{name}", prefix(doc, parent));
    doc.insert_element(parent, at, &full)
}

/// The child `name`, made if there is none; `val` set.
fn set_val(doc: &mut XmlDocument, parent: NodeId, name: &str, order: &[&str], val: &str) {
    let node = match doc.child(parent, name) {
        Some(n) => n,
        None => insert_ordered(doc, parent, name, order),
    };
    doc.set_attr(node, "val", val);
}

fn remove(doc: &mut XmlDocument, parent: NodeId, names: &[&str]) {
    let found: Vec<NodeId> = doc.elements(parent).filter(|&n| names.contains(&doc.local_name(n))).collect();
    for n in found {
        doc.detach(n);
    }
}

fn grouping_value(family: Family, grouping: Grouping, is_3d: bool) -> &'static str {
    match (family, grouping) {
        (Family::Bar, Grouping::Standard) => if is_3d { "standard" } else { "clustered" },
        (_, Grouping::Standard) => "standard",
        (_, Grouping::Stacked) => "stacked",
        (_, Grouping::PercentStacked) => "percentStacked",
    }
}

/// Turn the group at `node` (of `plot`, whose only group it is) from `old`
/// into `new`.
pub(crate) fn convert_group(doc: &mut XmlDocument, plot: NodeId, node: NodeId, old: &ChartGroup, new: &ChartGroup) -> Result<(), Error> {
    let (Some(from), Some(to)) = (family(old.kind), family(new.kind)) else { return Err(unsupported("この種類のグラフの種類の変更")) };
    let is_pie_family = |f: Family| matches!(f, Family::Pie | Family::Doughnut);
    let is_scatter_family = |f: Family| matches!(f, Family::Scatter | Family::Bubble);
    let is_bar_line_area = |f: Family| matches!(f, Family::Bar | Family::Line | Family::Area);
    let is_surface_family = |f: Family| matches!(f, Family::Surface);
    if (is_pie_family(from) != is_pie_family(to))
        || (is_scatter_family(from) != is_scatter_family(to))
        || (is_bar_line_area(from) != is_bar_line_area(to))
        || (is_surface_family(from) != is_surface_family(to))
    {
        return Err(unsupported("異なる系統のグラフへの変更"));
    }
    let is_3d = new.three_d.is_some();
    let (name, order, ser_order) = match to {
        Family::Bar => (if is_3d { "bar3DChart" } else { "barChart" }, BAR, BAR_SER),
        Family::Line => (if is_3d { "line3DChart" } else { "lineChart" }, LINE, LINE_SER),
        Family::Area => (if is_3d { "area3DChart" } else { "areaChart" }, AREA, AREA_SER),
        Family::Pie => (if is_3d { "pie3DChart" } else { "pieChart" }, PIE, PIE_SER),
        Family::Doughnut => ("doughnutChart", DOUGHNUT, DOUGHNUT_SER),
        Family::Scatter => ("scatterChart", SCATTER, SCATTER_SER),
        Family::Bubble => ("bubbleChart", BUBBLE, BUBBLE_SER),
        Family::Surface => {
            let ChartType::Surface { is_3d, .. } = new.kind else { unreachable!() };
            if is_3d {
                ("surface3DChart", SURFACE, SURFACE_SER)
            } else {
                ("surfaceChart", SURFACE, SURFACE_SER)
            }
        }
    };
    let pfx = prefix(doc, node);
    doc.rename(node, &format!("{pfx}{name}"));
    // The group's own children.
    match to {
        Family::Bar => {
            let ChartType::Bar { horizontal, gap_width, overlap } = new.kind else { unreachable!() };
            set_val(doc, node, "barDir", order, if horizontal { "bar" } else { "col" });
            set_val(doc, node, "gapWidth", order, &gap_width.to_string());
            if overlap != 0 {
                set_val(doc, node, "overlap", order, &overlap.to_string());
            } else {
                remove(doc, node, &["overlap"]);
            }
            remove(doc, node, &["marker", "smooth", "dropLines", "hiLowLines", "upDownBars", "firstSliceAng", "holeSize", "bubbleScale", "showNegBubbles", "sizeRepresents"]);
        }
        Family::Line => {
            remove(doc, node, &["barDir", "gapWidth", "overlap", "serLines", "dropLines", "firstSliceAng", "holeSize", "bubbleScale", "showNegBubbles", "sizeRepresents"]);
            set_val(doc, node, "marker", order, "1");
        }
        Family::Area => {
            remove(doc, node, &["barDir", "gapWidth", "overlap", "serLines", "marker", "smooth", "hiLowLines", "upDownBars", "firstSliceAng", "holeSize", "bubbleScale", "showNegBubbles", "sizeRepresents"]);
        }
        Family::Pie => {
            let ChartType::Pie { first_angle } = new.kind else { unreachable!() };
            set_val(doc, node, "varyColors", order, "1");
            if first_angle > 0 {
                set_val(doc, node, "firstSliceAng", order, &first_angle.to_string());
            } else {
                remove(doc, node, &["firstSliceAng"]);
            }
            remove(doc, node, &["holeSize", "marker", "smooth", "dropLines", "hiLowLines", "upDownBars", "barDir", "gapWidth", "overlap", "serLines", "grouping", "axId", "bubbleScale", "showNegBubbles", "sizeRepresents"]);
        }
        Family::Doughnut => {
            let ChartType::Doughnut { first_angle, hole } = new.kind else { unreachable!() };
            set_val(doc, node, "varyColors", order, "1");
            if first_angle > 0 {
                set_val(doc, node, "firstSliceAng", order, &first_angle.to_string());
            } else {
                remove(doc, node, &["firstSliceAng"]);
            }
            set_val(doc, node, "holeSize", order, &hole.to_string());
            remove(doc, node, &["marker", "smooth", "dropLines", "hiLowLines", "upDownBars", "barDir", "gapWidth", "overlap", "serLines", "grouping", "axId", "bubbleScale", "showNegBubbles", "sizeRepresents"]);
        }
        Family::Scatter => {
            let ChartType::Scatter { smooth } = new.kind else { unreachable!() };
            remove(doc, node, &["bubbleScale", "showNegBubbles", "sizeRepresents", "grouping"]);
            set_val(doc, node, "scatterStyle", order, if smooth { "smoothMarker" } else { "marker" });
            set_val(doc, node, "varyColors", order, "0");
        }
        Family::Bubble => {
            let ChartType::Bubble { scale } = new.kind else { unreachable!() };
            remove(doc, node, &["scatterStyle", "grouping"]);
            set_val(doc, node, "varyColors", order, "0");
            set_val(doc, node, "bubbleScale", order, &scale.to_string());
            set_val(doc, node, "showNegBubbles", order, "0");
            set_val(doc, node, "sizeRepresents", order, "area");
        }
        Family::Surface => {
            let ChartType::Surface { wireframe, .. } = new.kind else { unreachable!() };
            set_val(doc, node, "wireframe", order, if wireframe { "1" } else { "0" });
            remove(doc, node, &["grouping", "varyColors", "dropLines", "hiLowLines", "upDownBars", "barDir", "gapWidth", "overlap", "serLines", "marker", "smooth", "firstSliceAng", "holeSize", "scatterStyle", "bubbleScale", "showNegBubbles", "sizeRepresents"]);
        }
    }
    if to != Family::Pie && to != Family::Doughnut && to != Family::Scatter && to != Family::Bubble && to != Family::Surface {
        set_val(doc, node, "grouping", order, grouping_value(to, new.grouping, is_3d));
    }
    // Its series.
    if from != to {
        let sers: Vec<NodeId> = doc.elements(node).filter(|&n| doc.local_name(n) == "ser").collect();
        for ser in sers {
            remove(doc, ser, &["invertIfNegative", "pictureOptions", "shape", "marker", "smooth", "bubbleSize", "bubble3D"]);
            match to {
                Family::Bar => set_val(doc, ser, "invertIfNegative", ser_order, "0"),
                Family::Line => {
                    let marker = insert_ordered(doc, ser, "marker", ser_order);
                    let symbol = doc.append_element(marker, &format!("{pfx}symbol"));
                    doc.set_attr(symbol, "val", "none");
                    set_val(doc, ser, "smooth", ser_order, "0");
                }
                Family::Scatter => {
                    let ChartType::Scatter { smooth } = new.kind else { unreachable!() };
                    let marker = insert_ordered(doc, ser, "marker", ser_order);
                    let symbol = doc.append_element(marker, &format!("{pfx}symbol"));
                    doc.set_attr(symbol, "val", "circle");
                    let size = doc.append_element(marker, &format!("{pfx}size"));
                    doc.set_attr(size, "val", "5");
                    set_val(doc, ser, "smooth", ser_order, if smooth { "1" } else { "0" });
                }
                Family::Bubble => {
                    set_val(doc, ser, "invertIfNegative", ser_order, "0");
                    set_val(doc, ser, "bubble3D", ser_order, "0");
                    if doc.child(ser, "bubbleSize").is_none() {
                        let count = doc.child(ser, "yVal").or_else(|| doc.child(ser, "val")).and_then(|y| {
                            doc.descendants(y).into_iter().find(|&n| matches!(doc.local_name(n), "numCache" | "strCache")).and_then(|c| {
                                doc.child(c, "ptCount").and_then(|p| doc.attr(p, "val")).and_then(|v| v.parse::<usize>().ok())
                            })
                        }).unwrap_or(4);
                        let bs = insert_ordered(doc, ser, "bubbleSize", ser_order);
                        let lit = doc.append_element(bs, &format!("{pfx}numLit"));
                        let fmt = doc.append_element(lit, &format!("{pfx}formatCode"));
                        doc.set_text(fmt, "General");
                        let pt_cnt = doc.append_element(lit, &format!("{pfx}ptCount"));
                        doc.set_attr(pt_cnt, "val", &count.to_string());
                        for i in 0..count {
                            let pt = doc.append_element(lit, &format!("{pfx}pt"));
                            doc.set_attr(pt, "idx", &i.to_string());
                            let v = doc.append_element(pt, &format!("{pfx}v"));
                            doc.set_text(v, "10");
                        }
                    }
                }
                Family::Area | Family::Pie | Family::Doughnut | Family::Surface => {}
            }
            convert_series_paint(doc, ser, from == Family::Line, to == Family::Line);
        }
    } else {
        match to {
            Family::Scatter => {
                let ChartType::Scatter { smooth } = new.kind else { unreachable!() };
                set_val(doc, node, "scatterStyle", order, if smooth { "smoothMarker" } else { "marker" });
                let sers: Vec<NodeId> = doc.elements(node).filter(|&n| doc.local_name(n) == "ser").collect();
                for ser in sers {
                    set_val(doc, ser, "smooth", ser_order, if smooth { "1" } else { "0" });
                }
            }
            Family::Bubble => {
                let ChartType::Bubble { scale } = new.kind else { unreachable!() };
                set_val(doc, node, "bubbleScale", order, &scale.to_string());
            }
            Family::Surface => {
                let ChartType::Surface { wireframe, .. } = new.kind else { unreachable!() };
                set_val(doc, node, "wireframe", order, if wireframe { "1" } else { "0" });
            }
            _ => {}
        }
    }
    // The axes: where they are, and how the value axis meets the categories (only for axis-bearing charts).
    if to != Family::Pie && to != Family::Doughnut && to != Family::Scatter && to != Family::Bubble && to != Family::Surface {
        let horizontal = matches!(new.kind, ChartType::Bar { horizontal: true, .. });
        let axes: Vec<NodeId> = doc.elements(plot).filter(|&n| matches!(doc.local_name(n), "catAx" | "dateAx" | "valAx")).collect();
        for axis in axes {
            let category = doc.local_name(axis) != "valAx";
            if let Some(pos) = doc.child(axis, "axPos") {
                let at = match (category, horizontal) {
                    (true, false) | (false, true) => "b",
                    (true, true) | (false, false) => "l",
                };
                // Only the axes that are at the bottom or the left move.
                if matches!(doc.attr(pos, "val"), Some("b" | "l")) {
                    doc.set_attr(pos, "val", at);
                }
            }
            if !category && (old.grouping == Grouping::PercentStacked) != (new.grouping == Grouping::PercentStacked) {
                // The scale of the other grouping does not fit this one.
                if let Some(scaling) = doc.child(axis, "scaling") {
                    remove(doc, scaling, &["max", "min"]);
                }
                let percent = new.grouping == Grouping::PercentStacked;
                if let Some(fmt) = doc.child(axis, "numFmt") {
                    doc.set_attr(fmt, "formatCode", if percent { "0%" } else { "General" });
                    doc.set_attr(fmt, "sourceLinked", if percent { "0" } else { "1" });
                }
            }
            if !category && let Some(cross) = doc.child(axis, "crossBetween") {
                doc.set_attr(cross, "val", if to == Family::Area { "midCat" } else { "between" });
            }
        }
    }
    Ok(())
}

/// A bar's or area's fill becomes a line's colour (`a:ln`), and back.
fn convert_series_paint(doc: &mut XmlDocument, ser: NodeId, from_line: bool, to_line: bool) {
    let Some(sp) = doc.child(ser, "spPr") else { return };
    // `a:ln` and `a:round` are DrawingML's, not the chart's (`c:`).
    let pfx = match doc.root_element().and_then(|root| doc.lookup_prefix(root, super::pptx_chart::DRAWING_NS).map(|p| (root, p))) {
        Some((_, p)) if p.is_empty() => String::new(),
        Some((_, p)) => format!("{p}:"),
        None => {
            if let Some(root) = doc.root_element() {
                doc.set_attr(root, "xmlns:a", super::pptx_chart::DRAWING_NS);
            }
            "a:".to_string()
        }
    };
    if to_line && !from_line {
        let fill = doc.child(sp, "solidFill").or_else(|| doc.child(sp, "gradFill"));
        // A bar's own outline (often none) is not the line's.
        if let Some(ln) = doc.child(sp, "ln") {
            doc.detach(ln);
        }
        let Some(fill) = fill else { return };
        let copy = doc.deep_clone(fill);
        doc.detach(fill);
        let ln = doc.append_element(sp, &format!("{pfx}ln"));
        doc.set_attr(ln, "w", "28575");
        doc.set_attr(ln, "cap", "rnd");
        let at = doc.children(ln).len();
        doc.insert(ln, at, copy);
        doc.append_element(ln, &format!("{pfx}round"));
    } else if from_line && !to_line {
        let Some(ln) = doc.child(sp, "ln") else { return };
        if let Some(fill) = doc.child(ln, "solidFill")
            && doc.child(sp, "solidFill").is_none()
        {
            let copy = doc.deep_clone(fill);
            let at = doc.children(sp).iter().position(|&c| c == ln).unwrap_or(0);
            doc.insert(sp, at, copy);
        }
        doc.detach(ln);
    }
}
