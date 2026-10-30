// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! グラフのデータの編集: a chart's changed data written into its chart part
//! (`c:chartSpace`): series names, text categories and values, in the
//! caches the chart is drawn from (`c:strCache`, `c:numCache`, or the
//! literals `c:strLit`, `c:numLit`), its title's text, its legend (shown
//! or not, its side) and its series' data labels (shown or not). The points stay
//! as many as they were. Anything else changed is refused.
//!
//! The groups of the model are the chart elements of the plot area in their
//! order (`barChart`, `lineChart`, …, as `chart.rs` reads them) and their
//! series the `c:ser` of each.

use ibis_doc_model::{Axis, Chart, Number, Series};
use ibis_ooxml::{NodeId, Package, XmlDocument};

use crate::Error;
use crate::save::unsupported;

pub(crate) const CHART_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
pub(crate) const DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

/// The plot area's chart elements the reader reads, in its order.
pub(crate) const GROUP_ELEMENTS: &[&str] = &[
    "barChart", "bar3DChart", "ofPieChart", "radarChart", "lineChart", "line3DChart", "areaChart", "area3DChart", "pieChart", "pie3DChart", "doughnutChart",
    "scatterChart", "bubbleChart", "stockChart", "surfaceChart", "surface3DChart",
];

/// A value written as the caches hold them (the shortest form that reads
/// back the same).
pub(crate) fn number_text(v: f64) -> String {
    format!("{v}")
}

/// One changed cell of the chart's data, for its embedded workbook: the
/// reference the chart names (`Sheet1!$B$2:$B$5`), the point's index in
/// it, and the new value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CellChange {
    Number(String, usize, Option<f64>),
    Text(String, usize, String),
}

/// The cache (or literal) under `node` (`c:tx`, `c:cat`, `c:val`…), and
/// the formula it was taken from, if any.
fn cache(doc: &XmlDocument, node: NodeId) -> Option<(NodeId, Option<String>)> {
    let caches = ["strCache", "numCache", "strLit", "numLit"];
    let found = doc.descendants(node).into_iter().find(|&n| caches.contains(&doc.local_name(n)))?;
    let formula = doc.descendants(node).into_iter().find(|&n| doc.local_name(n) == "f").map(|f| doc.text(f));
    Some((found, formula))
}

/// Set point `idx` of `cache` to `value` (`None` leaves it empty).
fn set_point(doc: &mut XmlDocument, cache: NodeId, idx: usize, value: Option<&str>) -> Result<(), Error> {
    let existing = doc.elements(cache).find(|&p| doc.local_name(p) == "pt" && doc.attr(p, "idx").and_then(|v| v.parse::<usize>().ok()) == Some(idx));
    match (existing, value) {
        (Some(pt), Some(v)) => {
            let vnode = doc.child(pt, "v").ok_or_else(|| unsupported("値の無いグラフの点"))?;
            doc.set_text(vnode, v);
        }
        (Some(pt), None) => doc.detach(pt),
        (None, None) => {}
        (None, Some(v)) => {
            // A new point before the first with a larger index (and before an extLst).
            let prefix = doc.name(cache).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
            let at = doc
                .children(cache)
                .iter()
                .position(|&c| {
                    doc.is_element(c)
                        && ((doc.local_name(c) == "pt" && doc.attr(c, "idx").and_then(|v| v.parse::<usize>().ok()).is_some_and(|i| i > idx)) || doc.local_name(c) == "extLst")
                })
                .unwrap_or(doc.children(cache).len());
            let pt = doc.insert_element(cache, at, &format!("{prefix}pt"));
            doc.set_attr(pt, "idx", &idx.to_string());
            let vnode = doc.append_element(pt, &format!("{prefix}v"));
            doc.set_text(vnode, v);
        }
    }
    Ok(())
}

/// `c:dLblPos/@val` of a label position.
fn label_pos_str(pos: ibis_doc_model::LabelPosition) -> &'static str {
    use ibis_doc_model::LabelPosition::*;
    match pos {
        Center => "ctr",
        InsideEnd => "inEnd",
        InsideBase => "inBase",
        OutsideEnd => "outEnd",
        Above => "t",
        Below => "b",
        Left => "l",
        Right => "r",
        BestFit => "bestFit",
    }
}

/// Map label position to a valid OOXML `ST_DLblPos` value for the given chart type.
/// In ECMA-376 Part 1 §21.2.2.48, line/area/scatter/radar/stock charts only allow:
/// b, ctr, l, r, t, bestFit (outEnd, inEnd, inBase are invalid and cause Office repair).
fn safe_label_pos_str(pos: ibis_doc_model::LabelPosition, chart_kind: &str) -> &'static str {
    use ibis_doc_model::LabelPosition::*;
    match chart_kind {
        "lineChart" | "line3DChart" | "areaChart" | "area3DChart" | "scatterChart" | "bubbleChart" | "radarChart" | "stockChart" | "surfaceChart" | "surface3DChart" => {
            match pos {
                OutsideEnd | InsideEnd => "t",
                InsideBase => "b",
                Center => "ctr",
                Above => "t",
                Below => "b",
                Left => "l",
                Right => "r",
                BestFit => "bestFit",
            }
        }
        _ => label_pos_str(pos),
    }
}

/// A series' data labels (`c:dLbls`): taken out, position / values / categories / percent / series name updated,
/// or written new.
pub(crate) fn write_labels(doc: &mut XmlDocument, ser: NodeId, _old: Option<&ibis_doc_model::DataLabels>, new: Option<&ibis_doc_model::DataLabels>) -> Result<(), Error> {
    let existing = doc.child(ser, "dLbls");
    let prefix = doc.name(ser).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
    let chart_kind = doc.parent(ser).map(|p| doc.local_name(p).to_string()).unwrap_or_default();
    let Some(b) = new else {
        if let Some(d) = existing {
            doc.detach(d);
        }
        return Ok(());
    };

    if let Some(d) = existing {
        // In-place update of position and boolean visibility flags.
        match b.position {
            Some(pos) => {
                let pos_val = safe_label_pos_str(pos, &chart_kind);
                if let Some(p) = doc.child(d, "dLblPos") {
                    doc.set_attr(p, "val", pos_val);
                } else {
                    const AFTER_POS: &[&str] = &[
                        "showLegendKey", "showVal", "showCatName", "showSerName", "showPercent",
                        "showBubbleSize", "separator", "showLeaderLines", "leaderLines", "extLst",
                    ];
                    let at = doc.children(d).iter().position(|&c| doc.is_element(c) && AFTER_POS.contains(&doc.local_name(c))).unwrap_or(doc.children(d).len());
                    let p = doc.insert_element(d, at, &format!("{prefix}dLblPos"));
                    doc.set_attr(p, "val", pos_val);
                }
            }
            None => {
                if let Some(p) = doc.child(d, "dLblPos") {
                    doc.detach(p);
                }
            }
        }

        // Also ensure any individual point dLblPos does not contain invalid values for this chart type
        if matches!(chart_kind.as_str(), "lineChart" | "line3DChart" | "areaChart" | "area3DChart" | "scatterChart" | "bubbleChart" | "radarChart" | "stockChart" | "surfaceChart" | "surface3DChart") {
            let pos_nodes: Vec<NodeId> = doc.descendants(d).into_iter().filter(|&n| doc.is_element(n) && doc.local_name(n) == "dLblPos").collect();
            for dlbl in pos_nodes {
                let current_val = doc.attr(dlbl, "val").map(|s| s.to_string());
                match current_val.as_deref() {
                    Some("outEnd" | "inEnd") => doc.set_attr(dlbl, "val", "t"),
                    Some("inBase") => doc.set_attr(dlbl, "val", "b"),
                    _ => {}
                }
            }
        }

        for (name, on) in [
            ("showVal", b.show_value),
            ("showCatName", b.show_category),
            ("showSerName", b.show_series_name),
            ("showPercent", b.show_percent),
        ] {
            if let Some(node) = doc.child(d, name) {
                doc.set_attr(node, "val", if on { "1" } else { "0" });
            } else {
                let node = doc.append_element(d, &format!("{prefix}{name}"));
                doc.set_attr(node, "val", if on { "1" } else { "0" });
            }
        }
        return Ok(());
    }

    // New c:dLbls: insert before what follows it in series (CT_BarSer, CT_LineSer, CT_PieSer…).
    const AFTER: &[&str] = &["trendline", "errBars", "cat", "val", "xVal", "yVal", "smooth", "bubbleSize", "bubble3D", "shape", "extLst"];
    let at = doc.children(ser).iter().position(|&c| doc.is_element(c) && AFTER.contains(&doc.local_name(c))).unwrap_or(doc.children(ser).len());
    let labels = doc.insert_element(ser, at, &format!("{prefix}dLbls"));

    if let Some(pos) = b.position {
        let p = doc.append_element(labels, &format!("{prefix}dLblPos"));
        doc.set_attr(p, "val", safe_label_pos_str(pos, &chart_kind));
    }
    for (name, on) in [
        ("showLegendKey", false),
        ("showVal", b.show_value),
        ("showCatName", b.show_category),
        ("showSerName", b.show_series_name),
        ("showPercent", b.show_percent),
        ("showBubbleSize", false),
    ] {
        let n = doc.append_element(labels, &format!("{prefix}{name}"));
        doc.set_attr(n, "val", if on { "1" } else { "0" });
    }
    Ok(())
}

pub(crate) fn write_trendline(
    doc: &mut XmlDocument,
    ser: NodeId,
    trendline: Option<&ibis_doc_model::Trendline>,
) -> Result<(), Error> {
    let existing = doc.child(ser, "trendline");
    let prefix = doc.name(ser).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
    let Some(t) = trendline else {
        if let Some(d) = existing {
            doc.detach(d);
        }
        return Ok(());
    };

    let tl = if let Some(d) = existing {
        while let Some(c) = doc.children(d).first().copied() {
            doc.detach(c);
        }
        d
    } else {
        const AFTER: &[&str] = &["errBars", "cat", "val", "xVal", "yVal", "bubbleSize", "bubble3D", "smooth", "extLst"];
        let at = doc.children(ser).iter().position(|&c| doc.is_element(c) && AFTER.contains(&doc.local_name(c))).unwrap_or(doc.children(ser).len());
        doc.insert_element(ser, at, &format!("{prefix}trendline"))
    };

    let type_node = doc.append_element(tl, &format!("{prefix}trendlineType"));
    doc.set_attr(type_node, "val", t.kind.as_ooxml_str());

    if t.kind == ibis_doc_model::TrendlineType::Polynomial {
        let order_node = doc.append_element(tl, &format!("{prefix}order"));
        doc.set_attr(order_node, "val", &t.order.to_string());
    }

    if t.kind == ibis_doc_model::TrendlineType::MovingAverage {
        let period_node = doc.append_element(tl, &format!("{prefix}period"));
        doc.set_attr(period_node, "val", &t.period.to_string());
    }

    if t.disp_r_sqr {
        let rsqr_node = doc.append_element(tl, &format!("{prefix}dispRSqr"));
        doc.set_attr(rsqr_node, "val", "1");
    }

    if t.disp_eq {
        let eq_node = doc.append_element(tl, &format!("{prefix}dispEq"));
        doc.set_attr(eq_node, "val", "1");
    }

    Ok(())
}

pub(crate) fn write_error_bars(doc: &mut XmlDocument, ser: NodeId, err_bars: Option<&ibis_doc_model::ErrorBars>) -> Result<(), Error> {
    let existing = doc.child(ser, "errBars");
    let prefix = doc.name(ser).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
    let Some(eb) = err_bars else {
        if let Some(d) = existing {
            doc.detach(d);
        }
        return Ok(());
    };

    let eb_node = if let Some(d) = existing {
        while let Some(c) = doc.children(d).first().copied() {
            doc.detach(c);
        }
        d
    } else {
        const AFTER: &[&str] = &["cat", "val", "xVal", "yVal", "bubbleSize", "bubble3D", "smooth", "shape", "extLst"];
        let at = doc.children(ser).iter().position(|&c| doc.is_element(c) && AFTER.contains(&doc.local_name(c))).unwrap_or(doc.children(ser).len());
        doc.insert_element(ser, at, &format!("{prefix}errBars"))
    };

    let dir_node = doc.append_element(eb_node, &format!("{prefix}errDir"));
    doc.set_attr(dir_node, "val", eb.direction.as_ooxml_str());

    let type_node = doc.append_element(eb_node, &format!("{prefix}errBarType"));
    doc.set_attr(type_node, "val", eb.bar_type.as_ooxml_str());

    let val_type_node = doc.append_element(eb_node, &format!("{prefix}errValType"));
    doc.set_attr(val_type_node, "val", eb.val_type.as_ooxml_str());

    if eb.no_end_cap {
        let cap_node = doc.append_element(eb_node, &format!("{prefix}noEndCap"));
        doc.set_attr(cap_node, "val", "1");
    }

    if let Some(v) = eb.value {
        let val_node = doc.append_element(eb_node, &format!("{prefix}val"));
        doc.set_attr(val_node, "val", &v.0.to_string());
    }

    Ok(())
}

pub(crate) fn write_data_table(doc: &mut XmlDocument, plot: NodeId, table: Option<&ibis_doc_model::ChartDataTable>) -> Result<(), Error> {
    let existing = doc.child(plot, "dTable");
    let prefix = doc.name(plot).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
    let Some(t) = table else {
        if let Some(d) = existing {
            doc.detach(d);
        }
        return Ok(());
    };

    let dt = if let Some(d) = existing {
        d
    } else {
        const AFTER: &[&str] = &["spPr", "extLst"];
        let at = doc.children(plot).iter().position(|&c| doc.is_element(c) && AFTER.contains(&doc.local_name(c))).unwrap_or(doc.children(plot).len());
        doc.insert_element(plot, at, &format!("{prefix}dTable"))
    };

    for (name, on) in [
        ("showHorzBorder", t.show_horz_border),
        ("showVertBorder", t.show_vert_border),
        ("showOutline", t.show_outline),
        ("showKeys", t.show_keys),
    ] {
        if let Some(node) = doc.child(dt, name) {
            doc.set_attr(node, "val", if on { "1" } else { "0" });
        } else {
            let node = doc.append_element(dt, &format!("{prefix}{name}"));
            doc.set_attr(node, "val", if on { "1" } else { "0" });
        }
    }
    Ok(())
}

/// `c:legendPos/@val` of a side.
fn legend_pos(p: ibis_doc_model::LegendPosition) -> &'static str {
    use ibis_doc_model::LegendPosition::*;
    match p {
        Bottom => "b",
        Left => "l",
        Right => "r",
        Top => "t",
        TopRight => "tr",
    }
}

/// The number of points `cache` says it has.
fn point_count(doc: &XmlDocument, cache: NodeId) -> Option<usize> {
    doc.child(cache, "ptCount").and_then(|c| doc.attr(c, "val")).and_then(|v| v.parse().ok())
}

/// A new element, not yet in the tree.
fn detached(doc: &mut XmlDocument, name: &str) -> NodeId {
    let e = doc.insert_element(XmlDocument::ROOT, 0, name);
    doc.detach(e);
    e
}

pub(crate) fn write_solid_fill(doc: &mut XmlDocument, parent: NodeId, color: ibis_doc_model::Color, a: &str) {
    for child in doc.elements(parent).collect::<Vec<_>>() {
        let name = doc.local_name(child);
        if matches!(name, "noFill" | "solidFill" | "gradFill" | "blipFill" | "pattFill") {
            doc.detach(child);
        }
    }
    let solid = detached(doc, &format!("{a}:solidFill"));
    let clr = detached(doc, &format!("{a}:srgbClr"));
    doc.set_attr(clr, "val", &format!("{:06X}", color.rgb.0 & 0xFFFFFF));
    doc.insert(solid, 0, clr);
    let ln = doc.elements(parent).find(|&x| doc.local_name(x) == "ln");
    match ln {
        Some(before) => doc.insert_before(before, solid),
        None => doc.insert(parent, 0, solid),
    }
}

pub(crate) fn write_series_marker(doc: &mut XmlDocument, ser: NodeId, marker: Option<&ibis_doc_model::chart::Marker>, c: &str, a: &str) -> Result<(), Error> {
    use ibis_doc_model::chart::MarkerSymbol;
    // ECMA-376 dml-chart.xsd: marker belongs to Line/Scatter/Radar series (Stock uses LineSer).
    if marker.is_some() && !doc.parent(ser).is_some_and(|p|matches!(doc.local_name(p),"lineChart"|"line3DChart"|"scatterChart"|"radarChart"|"stockChart")) {
        return Err(unsupported("この種類のグラフのマーカー"));
    }
    let existing = doc.child(ser, "marker");
    match marker {
        None => {
            if let Some(m) = existing {
                doc.detach(m);
            }
        }
        Some(m) => {
            let node = match existing {
                Some(x) => x,
                None => {
                    let x = detached(doc, &format!("{c}marker"));
                    let at = doc.elements(ser).find(|&x| ["dPt", "dLbls", "trendline", "errBars", "cat", "val", "smooth", "extLst"].contains(&doc.local_name(x)));
                    match at {
                        Some(p) => doc.insert_before(p, x),
                        None => {
                            let n = doc.children(ser).len();
                            doc.insert(ser, n, x);
                        }
                    }
                    x
                }
            };
            let sym_str = match m.symbol {
                MarkerSymbol::None => "none",
                MarkerSymbol::Circle => "circle",
                MarkerSymbol::Square => "square",
                MarkerSymbol::Diamond => "diamond",
                MarkerSymbol::Triangle => "triangle",
            };
            match doc.child(node, "symbol") {
                Some(s) => doc.set_attr(s, "val", sym_str),
                None => {
                    let s = detached(doc, &format!("{c}symbol"));
                    doc.set_attr(s, "val", sym_str);
                    doc.insert(node, 0, s);
                }
            }
            if m.symbol != MarkerSymbol::None {
                match doc.child(node, "size") {
                    Some(s) => doc.set_attr(s, "val", &m.size.to_string()),
                    None => {
                        let s = detached(doc, &format!("{c}size"));
                        doc.set_attr(s, "val", &m.size.to_string());
                        let at = doc.children(node).len();
                        doc.insert(node, at, s);
                    }
                }
                if let ibis_doc_model::Fill::Solid(color) = &m.fill {
                    let sp_pr = match doc.child(node, "spPr") {
                        Some(p) => p,
                        None => {
                            let p = detached(doc, &format!("{c}spPr"));
                            let at = doc.children(node).len();
                            doc.insert(node, at, p);
                            p
                        }
                    };
                    write_solid_fill(doc, sp_pr, *color, a);
                }
            }
        }
    }
    Ok(())
}

pub(crate) fn write_series_stroke(doc: &mut XmlDocument, ser: NodeId, stroke: Option<&ibis_doc_model::Stroke>, c: &str, a: &str) -> Result<(), Error> {
    let sp_pr = match doc.child(ser, "spPr") {
        Some(p) => p,
        None => {
            let p = detached(doc, &format!("{c}spPr"));
            let at = doc.elements(ser).find(|&x| ["marker", "dPt", "dLbls", "trendline", "errBars", "cat", "val", "smooth", "extLst"].contains(&doc.local_name(x)));
            match at {
                Some(before) => doc.insert_before(before, p),
                None => {
                    let n = doc.children(ser).len();
                    doc.insert(ser, n, p);
                }
            }
            p
        }
    };
    match stroke {
        None => {
            if let Some(ln) = doc.child(sp_pr, "ln") {
                doc.detach(ln);
            }
            let ln_new = detached(doc, &format!("{a}:ln"));
            let no_fill = detached(doc, &format!("{a}:noFill"));
            doc.insert(ln_new, 0, no_fill);
            doc.insert(sp_pr, 0, ln_new);
        }
        Some(s) => {
            let ln = match doc.child(sp_pr, "ln") {
                Some(l) => l,
                None => {
                    let l = detached(doc, &format!("{a}:ln"));
                    doc.insert(sp_pr, 0, l);
                    l
                }
            };
            doc.set_attr(ln, "w", &s.width.to_string());
            write_solid_fill(doc, ln, s.color, a);
        }
    }
    Ok(())
}

pub(crate) fn write_series_fill(doc: &mut XmlDocument, ser: NodeId, fill: &ibis_doc_model::Fill, c: &str, a: &str) -> Result<(), Error> {
    let sp_pr = match doc.child(ser, "spPr") {
        Some(p) => p,
        None => {
            let p = detached(doc, &format!("{c}spPr"));
            let at = doc.elements(ser).find(|&x| ["marker", "dPt", "dLbls", "trendline", "errBars", "cat", "val", "smooth", "extLst"].contains(&doc.local_name(x)));
            match at {
                Some(before) => doc.insert_before(before, p),
                None => {
                    let n = doc.children(ser).len();
                    doc.insert(ser, n, p);
                }
            }
            p
        }
    };
    match fill {
        ibis_doc_model::Fill::Solid(color) => {
            write_solid_fill(doc, sp_pr, *color, a);
        }
        ibis_doc_model::Fill::None => {
            for child in doc.elements(sp_pr).collect::<Vec<_>>() {
                let name = doc.local_name(child);
                if matches!(name, "noFill" | "solidFill" | "gradFill" | "blipFill" | "pattFill") {
                    doc.detach(child);
                }
            }
            let no_fill = detached(doc, &format!("{a}:noFill"));
            let ln = doc.elements(sp_pr).find(|&x| doc.local_name(x) == "ln");
            match ln {
                Some(before) => doc.insert_before(before, no_fill),
                None => doc.insert(sp_pr, 0, no_fill),
            }
        }
        _ => {}
    }
    Ok(())
}

pub(crate) fn write_plot_background(doc: &mut XmlDocument, plot: NodeId, fill: &ibis_doc_model::Fill, c: &str, a: &str) -> Result<(), Error> {
    let existing = doc.child(plot, "spPr");
    match fill {
        ibis_doc_model::Fill::None => {
            if let Some(sp_pr) = existing {
                for child in doc.elements(sp_pr).collect::<Vec<_>>() {
                    let name = doc.local_name(child);
                    if matches!(name, "noFill" | "solidFill" | "gradFill" | "blipFill" | "pattFill") {
                        doc.detach(child);
                    }
                }
                let no_fill = detached(doc, &format!("{a}:noFill"));
                let ln = doc.elements(sp_pr).find(|&x| doc.local_name(x) == "ln");
                match ln {
                    Some(before) => doc.insert_before(before, no_fill),
                    None => doc.insert(sp_pr, 0, no_fill),
                }
            }
        }
        ibis_doc_model::Fill::Solid(color) => {
            let sp_pr = match existing {
                Some(p) => p,
                None => {
                    let p = detached(doc, &format!("{c}spPr"));
                    let at = doc.elements(plot).find(|&x| ["extLst"].contains(&doc.local_name(x)));
                    match at {
                        Some(before) => doc.insert_before(before, p),
                        None => {
                            let n = doc.children(plot).len();
                            doc.insert(plot, n, p);
                        }
                    }
                    p
                }
            };
            write_solid_fill(doc, sp_pr, *color, a);
        }
        _ => {}
    }
    Ok(())
}

/// Write the changes from `old` to `new` into the chart part `part`;
/// returns the cells of its data that changed.
pub(crate) fn write_chart(package: &mut Package, part: &str, old: &Chart, new: &Chart) -> Result<Vec<CellChange>, Error> {
    // Only data and the title's text change here.
    let refuse = |_tag: &str| unsupported("このグラフの変更（データ・タイトルの文字のほか）");
    if old.groups.len() != new.groups.len() {
        return Err(refuse("group_count"));
    }
    let mut changes = Vec::new();
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{part} が空です")))?;
    let chart = doc.child(root, "chart").ok_or_else(|| refuse("chart_node"))?;
    let plot = doc.child(chart, "plotArea").ok_or_else(|| refuse("plot_node"))?;
    let c_prefix = doc.lookup_prefix(root, CHART_NS).map(|p| if p.is_empty() { String::new() } else { format!("{p}:") }).unwrap_or_else(|| {
        doc.name(chart).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default()
    });
    let a_prefix = match doc.lookup_prefix(root, DRAWING_NS) {
        Some(p) => p,
        None => {
            doc.set_attr(root, "xmlns:a", DRAWING_NS);
            "a".into()
        }
    };
    let group_nodes: Vec<NodeId> = doc.elements(plot).filter(|&n| GROUP_ELEMENTS.contains(&doc.local_name(n))).collect();
    if group_nodes.len() != old.groups.len() {
        return Err(refuse("group_nodes_count"));
    }
    let mut retyped = false;
    for ((node, a), b) in group_nodes.into_iter().zip(&old.groups).zip(&new.groups) {
        // グラフの種類の変更: the one group of the chart turned into another
        // bar, line or area kind (or grouping).
        let converted;
        let a = if (a.kind != b.kind || a.grouping != b.grouping || a.three_d != b.three_d) && old.groups.len() == 1 {
            super::pptx_chart_type::convert_group(doc, plot, node, a, b)?;
            retyped = true;
            converted = ibis_doc_model::ChartGroup { kind: b.kind, grouping: b.grouping, three_d: b.three_d, ..a.clone() };
            &converted
        } else {
            a
        };
        // The group's own settings (its kind, grouping, axes, …) stay.
        if a.series.len() != b.series.len() || (ibis_doc_model::ChartGroup { series: b.series.clone(), ..a.clone() }) != *b {
            return Err(refuse("group_settings"));
        }
        let sers: Vec<NodeId> = doc.elements(node).filter(|&n| doc.local_name(n) == "ser").collect();
        if sers.len() != a.series.len() {
            return Err(refuse("series_count"));
        }
        for ((ser, s), t) in sers.into_iter().zip(&a.series).zip(&b.series) {
            // Nothing but the name, text categories, values, labels, trendline, error_bars, fill, stroke, marker, x_values and bubble_sizes.
            let mut rest = Series {
                name: t.name.clone(),
                categories: t.categories.clone(),
                values: t.values.clone(),
                labels: t.labels.clone(),
                trendline: t.trendline.clone(),
                error_bars: t.error_bars.clone(),
                fill: t.fill.clone(),
                stroke: t.stroke.clone(),
                marker: t.marker.clone(),
                x_values: t.x_values.clone(),
                bubble_sizes: t.bubble_sizes.clone(),
                bubble_3d: t.bubble_3d,
                ..s.clone()
            };
            if retyped {
                // A line's colour, a bar's fill and a line's markers are
                // what the new kind made of them (read back by the app).
                (rest.fill, rest.stroke, rest.marker) = (t.fill.clone(), t.stroke.clone(), t.marker.clone());
                rest.bubble_sizes = t.bubble_sizes.clone();
                rest.x_values = t.x_values.clone();
            }
            if rest != *t {
                return Err(refuse("series_rest"));
            }
            if s.fill != t.fill {
                write_series_fill(doc, ser, &t.fill, &c_prefix, &a_prefix)?;
            }
            if s.stroke != t.stroke {
                write_series_stroke(doc, ser, t.stroke.as_ref(), &c_prefix, &a_prefix)?;
            }
            if s.marker != t.marker {
                write_series_marker(doc, ser, if retyped && !b.kind.supports_markers() { None } else { t.marker.as_ref() }, &c_prefix, &a_prefix)?;
            }
            if s.labels != t.labels {
                write_labels(doc, ser, s.labels.as_ref(), t.labels.as_ref())?;
            }
            if s.trendline != t.trendline {
                write_trendline(doc, ser, t.trendline.as_ref())?;
            }
            if s.error_bars != t.error_bars {
                write_error_bars(doc, ser, t.error_bars.as_ref())?;
            }
            if s.name != t.name {
                let tx = doc.child(ser, "tx").ok_or_else(|| unsupported("名前の無い系列の名前"))?;
                let (c, f) = cache(doc, tx).ok_or_else(|| unsupported("この系列の名前"))?;
                set_point(doc, c, 0, Some(&t.name))?;
                if let Some(f) = f {
                    changes.push(CellChange::Text(f, 0, t.name.clone()));
                }
            }
            if s.categories != t.categories {
                let cat = doc.child(ser, "cat").ok_or_else(|| unsupported("項目の無い系列の項目"))?;
                let (c, f) = cache(doc, cat).ok_or_else(|| unsupported("この系列の項目"))?;
                if !matches!(doc.local_name(c), "strCache" | "strLit") || point_count(doc, c) != Some(t.categories.len()) || s.categories.len() != t.categories.len() {
                    return Err(unsupported("数値・日付の項目、項目の数の変更"));
                }
                for (i, (x, y)) in s.categories.iter().zip(&t.categories).enumerate() {
                    if x != y {
                        set_point(doc, c, i, Some(y))?;
                        if let Some(f) = &f {
                            changes.push(CellChange::Text(f.clone(), i, y.clone()));
                        }
                    }
                }
            }
            if s.values != t.values {
                let val = doc.child(ser, "val").or_else(|| doc.child(ser, "yVal")).ok_or_else(|| unsupported("値の無い系列"))?;
                let (c, f) = cache(doc, val).ok_or_else(|| unsupported("この系列の値"))?;
                if s.values.len() != t.values.len() || point_count(doc, c).is_some_and(|n| n != t.values.len()) {
                    return Err(unsupported("値の数の変更"));
                }
                for (i, (x, y)) in s.values.iter().zip(&t.values).enumerate() {
                    if x != y {
                        set_point(doc, c, i, y.map(|Number(v)| number_text(v)).as_deref())?;
                        if let Some(f) = &f {
                            changes.push(CellChange::Number(f.clone(), i, y.map(|Number(v)| v)));
                        }
                    }
                }
            }
            if !retyped && s.x_values != t.x_values {
                let xval = doc.child(ser, "xVal").ok_or_else(|| unsupported("X値の無い系列"))?;
                let (c, f) = cache(doc, xval).ok_or_else(|| unsupported("この系列のX値"))?;
                if s.x_values.len() != t.x_values.len() || point_count(doc, c).is_some_and(|n| n != t.x_values.len()) {
                    return Err(unsupported("X値の数の変更"));
                }
                for (i, (x, y)) in s.x_values.iter().zip(&t.x_values).enumerate() {
                    if x != y {
                        set_point(doc, c, i, y.map(|Number(v)| number_text(v)).as_deref())?;
                        if let Some(f) = &f {
                            changes.push(CellChange::Number(f.clone(), i, y.map(|Number(v)| v)));
                        }
                    }
                }
            }
            if !retyped && matches!(b.kind, ibis_doc_model::ChartType::Bubble { .. }) && s.bubble_sizes != t.bubble_sizes {
                let bval = doc.child(ser, "bubbleSize").ok_or_else(|| unsupported("サイズ値の無いバブル系列"))?;
                let (c, f) = cache(doc, bval).ok_or_else(|| unsupported("この系列のサイズ値"))?;
                if s.bubble_sizes.len() != t.bubble_sizes.len() || point_count(doc, c).is_some_and(|n| n != t.bubble_sizes.len()) {
                    return Err(unsupported("サイズ値の数の変更"));
                }
                for (i, (x, y)) in s.bubble_sizes.iter().zip(&t.bubble_sizes).enumerate() {
                    if x != y {
                        set_point(doc, c, i, y.map(|Number(v)| number_text(v)).as_deref())?;
                        if let Some(f) = &f {
                            changes.push(CellChange::Number(f.clone(), i, y.map(|Number(v)| v)));
                        }
                    }
                }
            }
        }
    }
    // The title's text: one run in its first paragraph (lines as paragraphs
    // are not written yet), or adding / removing the title.
    match (&old.title, &new.title) {
        (Some(_), None) => {
            if let Some(title) = doc.child(chart, "title") {
                doc.detach(title);
            }
            let c_prefix = doc.name(chart).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
            let auto_del = match doc.child(chart, "autoTitleDeleted") {
                Some(d) => d,
                None => {
                    let at = doc.children(chart).iter().position(|&c| c == plot).unwrap_or(0);
                    doc.insert_element(chart, at, &format!("{c_prefix}autoTitleDeleted"))
                }
            };
            doc.set_attr(auto_del, "val", "1");
        }
        (None, Some(b)) => {
            if let Some(auto_del) = doc.child(chart, "autoTitleDeleted") {
                doc.detach(auto_del);
            }
            let c_prefix = doc.name(chart).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
            let a_colon = if a_prefix.is_empty() { String::new() } else { format!("{a_prefix}:") };

            let title = doc.insert_element(chart, 0, &format!("{c_prefix}title"));
            let tx = doc.append_element(title, &format!("{c_prefix}tx"));
            let rich = doc.append_element(tx, &format!("{c_prefix}rich"));
            doc.append_element(rich, &format!("{a_colon}bodyPr"));
            doc.append_element(rich, &format!("{a_colon}lstStyle"));

            for line in b.text.split('\n') {
                let p = doc.append_element(rich, &format!("{a_colon}p"));
                let ppr = doc.append_element(p, &format!("{a_colon}pPr"));
                let def = doc.append_element(ppr, &format!("{a_colon}defRPr"));
                if b.style.size > 0 {
                    doc.set_attr(def, "sz", &b.style.size.to_string());
                }
                if b.style.bold {
                    doc.set_attr(def, "b", "1");
                }
                let r = doc.append_element(p, &format!("{a_colon}r"));
                let rpr = doc.append_element(r, &format!("{a_colon}rPr"));
                doc.set_attr(rpr, "lang", "ja-JP");
                if b.style.size > 0 {
                    doc.set_attr(rpr, "sz", &b.style.size.to_string());
                }
                if b.style.bold {
                    doc.set_attr(rpr, "b", "1");
                }
                let t = doc.append_element(r, &format!("{a_colon}t"));
                doc.set_text(t, line);
            }

            doc.append_element(title, &format!("{c_prefix}layout"));
            let overlay = doc.append_element(title, &format!("{c_prefix}overlay"));
            doc.set_attr(overlay, "val", "0");
        }
        (Some(a), Some(b)) if a != b => {
            let matches_except_layout = (ibis_doc_model::ChartTitle { text: b.text.clone(), at: b.at, ..a.clone() }) == *b && !b.text.contains('\n');
            if !matches_except_layout {
                return Err(refuse("title_matches_except_layout"));
            }
            if let Some(auto_del) = doc.child(chart, "autoTitleDeleted") {
                doc.detach(auto_del);
            }
            let title = doc.child(chart, "title").ok_or_else(|| refuse("title_node"))?;
            if b.at.is_none() {
                if let Some(layout) = doc.child(title, "layout") {
                    doc.detach(layout);
                }
            }
            let runs: Vec<NodeId> = doc.descendants(title).into_iter().filter(|&n| doc.local_name(n) == "r").collect();
            let Some((&first, others)) = runs.split_first() else { return Err(unsupported("文字の無いグラフのタイトル")) };
            let t = doc.child(first, "t").ok_or_else(|| refuse("title_t"))?;
            doc.set_text(t, &b.text);
            for &r in others {
                doc.detach(r);
            }
            // Further paragraphs (lines) in rich text go. Do NOT touch p elements outside rich (e.g. in txPr).
            let rich = doc.descendants(title).into_iter().find(|&n| doc.local_name(n) == "rich");
            if let Some(rich) = rich {
                let paragraphs: Vec<NodeId> = doc.children(rich).iter().copied().filter(|&n| doc.local_name(n) == "p").collect();
                for &p in paragraphs.iter().skip(1) {
                    doc.detach(p);
                }
            }
        }
        (a, b) if a != b => return Err(refuse("title_mismatch")),
        _ => {}
    }
    // The legend: shown or not, and on which side. One that only moves
    // keeps its own formatting; one where there was none is written new
    // (the app takes the file's reading of it). Other changes are refused.
    let only_moved = |a: &ibis_doc_model::Legend, b: &ibis_doc_model::Legend| b.layout.is_none() && (ibis_doc_model::Legend { source_position: b.source_position, side: b.side, layout: None, ..a.clone() }) == *b;
    // A side the file would not read back from the position is refused.
    if let Some(b) = &new.legend
        && old.legend.as_ref() != Some(b)
        && b.source_position.is_none_or(|p| crate::chart::side(legend_pos(p)) != b.side)
    {
        return Err(refuse("legend_source_pos"));
    }
    match (&old.legend, &new.legend) {
        (Some(_), None) => {
            if let Some(legend) = doc.child(chart, "legend") {
                doc.detach(legend);
            }
        }
        (Some(a), Some(b)) if a != b && only_moved(a, b) => {
            let legend = doc.child(chart, "legend").ok_or_else(|| refuse("legend_node"))?;
            let pos = match doc.child(legend, "legendPos") {
                Some(p) => p,
                None => {
                    let prefix = doc.name(legend).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
                    doc.insert_element(legend, 0, &format!("{prefix}legendPos"))
                }
            };
            doc.set_attr(pos, "val", legend_pos(b.source_position.ok_or_else(|| refuse("legend_b_source_pos"))?));
            if let Some(layout) = doc.child(legend, "layout") {
                doc.detach(layout);
            }
        }
        (Some(a), Some(b)) if a != b => return Err(refuse("legend_mismatch")),
        (None, Some(b)) => {
            let at = doc.children(chart).iter().position(|&c| c == plot).map_or(doc.children(chart).len(), |i| i + 1);
            let prefix = doc.name(chart).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
            let legend = doc.insert_element(chart, at, &format!("{prefix}legend"));
            let pos = doc.append_element(legend, &format!("{prefix}legendPos"));
            doc.set_attr(pos, "val", legend_pos(b.source_position.ok_or_else(|| refuse("legend_new_source_pos"))?));
            let overlay = doc.append_element(legend, &format!("{prefix}overlay"));
            doc.set_attr(overlay, "val", if b.overlay == Some(true) { "1" } else { "0" });
        }
        _ => {}
    }
    if old.data_table != new.data_table {
        write_data_table(doc, plot, new.data_table.as_ref())?;
    }
    if old.style != new.style {
        let root = doc.root_element().ok_or_else(|| refuse("root_node"))?;
        let prefix = doc.name(root).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();
        let existing = doc.child(root, "style");
        let alt_content = doc.elements(root).find(|&x| doc.local_name(x) == "AlternateContent");
        match new.style {
            Some(style_id) => {
                if let Some(alt) = alt_content {
                    doc.detach(alt);
                }
                match existing {
                    Some(node) => doc.set_attr(node, "val", &style_id.to_string()),
                    None => {
                        let at = doc.children(root).iter().position(|&c| c == chart).unwrap_or(0);
                        let node = doc.insert_element(root, at, &format!("{prefix}style"));
                        doc.set_attr(node, "val", &style_id.to_string());
                    }
                }
            }
            None => {
                if let Some(node) = existing {
                    doc.detach(node);
                }
                if let Some(alt) = alt_content {
                    doc.detach(alt);
                }
            }
        }
    }
    if old.axes != new.axes {
        if old.axes.len() != new.axes.len() || old.axes.iter().zip(&new.axes).any(|(a, b)| a.id != b.id || a.kind != b.kind) {
            return Err(refuse("axes_len_or_kind"));
        }
        for (a, b) in old.axes.iter().zip(&new.axes) {
            let mut rest_ax = ibis_doc_model::Axis {
                reversed: a.reversed,
                log_base: a.log_base,
                min: a.min,
                max: a.max,
                major_unit: a.major_unit,
                minor_unit: a.minor_unit,
                display_unit: a.display_unit,
                side: if retyped { a.side } else { b.side },
                format: if retyped { a.format.clone() } else { b.format.clone() },
                ..b.clone()
            };
            if retyped {
                rest_ax.side = a.side;
                rest_ax.format = a.format.clone();
                if (old.groups[0].grouping == ibis_doc_model::Grouping::PercentStacked)
                    != (new.groups[0].grouping == ibis_doc_model::Grouping::PercentStacked)
                {
                    rest_ax.min = a.min;
                    rest_ax.max = a.max;
                }
            }
            if rest_ax != *a {
                return Err(refuse("axes_rest"));
            }
        }
        write_axes(doc, plot, &old.axes, &new.axes)?;
    }
    if old.plot_background != new.plot_background {
        write_plot_background(doc, plot, &new.plot_background, &c_prefix, &a_prefix)?;
    }
    // The rest must be as it was.
    let rest = Chart {
        groups: old.groups.clone(),
        title: old.title.clone(),
        legend: old.legend.clone(),
        data_table: old.data_table,
        style: old.style,
        axes: old.axes.clone(),
        plot_background: old.plot_background.clone(),
        plot_border: old.plot_border.clone(),
        cell_refs: old.cell_refs.clone(),
        ..new.clone()
    };
    if rest != *old {
        return Err(refuse("chart_rest"));
    }
    Ok(changes)
}

const AXIS_ELEMENTS: &[&str] = &["valAx", "catAx", "dateAx", "serAx"];

/// Divisor to builtInUnit string.
fn builtin_unit_name(divisor: f64) -> Option<&'static str> {
    if (divisor - 1e2).abs() < 1e-4 {
        Some("hundreds")
    } else if (divisor - 1e3).abs() < 1e-4 {
        Some("thousands")
    } else if (divisor - 1e4).abs() < 1e-4 {
        Some("tenThousands")
    } else if (divisor - 1e5).abs() < 1e-4 {
        Some("hundredThousands")
    } else if (divisor - 1e6).abs() < 1e-4 {
        Some("millions")
    } else if (divisor - 1e7).abs() < 1e-4 {
        Some("tenMillions")
    } else if (divisor - 1e8).abs() < 1e-4 {
        Some("hundredMillions")
    } else if (divisor - 1e9).abs() < 1e-4 {
        Some("billions")
    } else if (divisor - 1e12).abs() < 1e-4 {
        Some("trillions")
    } else {
        None
    }
}

fn place_element(doc: &mut XmlDocument, parent: NodeId, name: &str, before: &[&str]) -> NodeId {
    let at = doc.children(parent).iter().position(|&c| doc.is_element(c) && before.contains(&doc.local_name(c))).unwrap_or(doc.children(parent).len());
    doc.insert_element(parent, at, name)
}

/// Write axis scaling, units, and orientation changes.
pub(crate) fn write_axes(doc: &mut XmlDocument, plot: NodeId, _old: &[Axis], new_axes: &[Axis]) -> Result<(), Error> {
    let axis_nodes: Vec<NodeId> = doc.elements(plot).filter(|&n| AXIS_ELEMENTS.contains(&doc.local_name(n))).collect();
    for ax_node in axis_nodes {
        let Some(ax_id_node) = doc.child(ax_node, "axId") else { continue };
        let Some(ax_id) = doc.attr(ax_id_node, "val").and_then(|v| v.trim().parse::<f64>().ok()).map(|v| v as u32) else { continue };
        let Some(axis) = new_axes.iter().find(|a| a.id == ax_id) else { continue };
        let prefix = doc.name(ax_node).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default();

        // 1. c:scaling
        let scaling = match doc.child(ax_node, "scaling") {
            Some(s) => s,
            None => place_element(doc, ax_node, &format!("{prefix}scaling"), &["delete", "axPos", "majorGridlines", "minorGridlines", "title", "numFmt", "majorTickMark", "minorTickMark", "tickLblPos", "spPr", "txPr", "crossAx", "crosses", "crossesAt", "crossBetween", "majorUnit", "minorUnit", "majorTimeUnit", "minorTimeUnit", "dispUnits", "extLst"]),
        };

        // logBase
        match axis.log_base {
            Some(Number(base)) => {
                match doc.child(scaling, "logBase") {
                    Some(lb) => doc.set_attr(lb, "val", &base.to_string()),
                    None => {
                        let lb = place_element(doc, scaling, &format!("{prefix}logBase"), &["orientation", "max", "min", "extLst"]);
                        doc.set_attr(lb, "val", &base.to_string());
                    }
                }
            }
            None => {
                if let Some(lb) = doc.child(scaling, "logBase") {
                    doc.detach(lb);
                }
            }
        }

        // orientation: minMax or maxMin
        let want_orientation = if axis.reversed { "maxMin" } else { "minMax" };
        match doc.child(scaling, "orientation") {
            Some(orient) => doc.set_attr(orient, "val", want_orientation),
            None => {
                let orient = place_element(doc, scaling, &format!("{prefix}orientation"), &["max", "min", "extLst"]);
                doc.set_attr(orient, "val", want_orientation);
            }
        }

        // max
        match axis.max {
            Some(Number(max_val)) => {
                match doc.child(scaling, "max") {
                    Some(m) => doc.set_attr(m, "val", &max_val.to_string()),
                    None => {
                        let m = place_element(doc, scaling, &format!("{prefix}max"), &["min", "extLst"]);
                        doc.set_attr(m, "val", &max_val.to_string());
                    }
                }
            }
            None => {
                if let Some(m) = doc.child(scaling, "max") {
                    doc.detach(m);
                }
            }
        }

        // min (mathematically undefined and invalid in Office if min <= 0 when log_base is set)
        let min_value = match (axis.min, axis.log_base) {
            (Some(Number(min_val)), Some(_)) if min_val <= 0.0 => None,
            (min, _) => min,
        };
        match min_value {
            Some(Number(min_val)) => {
                match doc.child(scaling, "min") {
                    Some(m) => doc.set_attr(m, "val", &min_val.to_string()),
                    None => {
                        let m = place_element(doc, scaling, &format!("{prefix}min"), &["extLst"]);
                        doc.set_attr(m, "val", &min_val.to_string());
                    }
                }
            }
            None => {
                if let Some(m) = doc.child(scaling, "min") {
                    doc.detach(m);
                }
            }
        }

        // 2. c:majorUnit
        match axis.major_unit {
            Some(Number(unit_val)) => {
                match doc.child(ax_node, "majorUnit") {
                    Some(u) => doc.set_attr(u, "val", &unit_val.to_string()),
                    None => {
                        let u = place_element(doc, ax_node, &format!("{prefix}majorUnit"), &["minorUnit", "majorTimeUnit", "minorTimeUnit", "dispUnits", "extLst"]);
                        doc.set_attr(u, "val", &unit_val.to_string());
                    }
                }
            }
            None => {
                if let Some(u) = doc.child(ax_node, "majorUnit") {
                    doc.detach(u);
                }
            }
        }

        // 3. c:minorUnit
        match axis.minor_unit {
            Some(Number(unit_val)) => {
                match doc.child(ax_node, "minorUnit") {
                    Some(u) => doc.set_attr(u, "val", &unit_val.to_string()),
                    None => {
                        let u = place_element(doc, ax_node, &format!("{prefix}minorUnit"), &["majorTimeUnit", "minorTimeUnit", "dispUnits", "extLst"]);
                        doc.set_attr(u, "val", &unit_val.to_string());
                    }
                }
            }
            None => {
                if let Some(u) = doc.child(ax_node, "minorUnit") {
                    doc.detach(u);
                }
            }
        }

        // 4. c:dispUnits
        match axis.display_unit {
            Some(Number(disp_val)) => {
                let disp = match doc.child(ax_node, "dispUnits") {
                    Some(d) => d,
                    None => place_element(doc, ax_node, &format!("{prefix}dispUnits"), &["extLst"]),
                };
                if let Some(builtin) = builtin_unit_name(disp_val) {
                    if let Some(cust) = doc.child(disp, "custUnit") {
                        doc.detach(cust);
                    }
                    match doc.child(disp, "builtInUnit") {
                        Some(b) => doc.set_attr(b, "val", builtin),
                        None => {
                            let b = doc.insert_element(disp, 0, &format!("{prefix}builtInUnit"));
                            doc.set_attr(b, "val", builtin);
                        }
                    }
                } else {
                    if let Some(b) = doc.child(disp, "builtInUnit") {
                        doc.detach(b);
                    }
                    match doc.child(disp, "custUnit") {
                        Some(c) => doc.set_attr(c, "val", &disp_val.to_string()),
                        None => {
                            let c = doc.insert_element(disp, 0, &format!("{prefix}custUnit"));
                            doc.set_attr(c, "val", &disp_val.to_string());
                        }
                    }
                }
            }
            None => {
                if let Some(disp) = doc.child(ax_node, "dispUnits") {
                    doc.detach(disp);
                }
            }
        }
    }
    Ok(())
}

pub(crate) const REL_PACKAGE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package";

/// A cell reference's column and row (0-based) from `$B$2` / `B2`.
pub(crate) fn cell_address(text: &str) -> Option<(u32, u32)> {
    let t = text.replace('$', "");
    let split = t.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = t.split_at(split);
    if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let column = letters.to_ascii_uppercase().bytes().fold(0u32, |n, b| n * 26 + (b - b'A' + 1) as u32) - 1;
    let row = digits.parse::<u32>().ok()?.checked_sub(1)?;
    Some((column, row))
}

/// The sheet and the cell of point `index` of the range `formula`
/// (`Sheet1!$B$2:$B$5`, `'売上 2026'!$A$2`): down a column or along a row.
fn point_cell(formula: &str, index: usize) -> Option<(String, u32, u32)> {
    let (sheet, range) = formula.rsplit_once('!')?;
    let sheet = sheet.trim_matches('\'').replace("''", "'");
    let (first, last) = range.split_once(':').unwrap_or((range, range));
    let ((c0, r0), (c1, r1)) = (cell_address(first)?, cell_address(last)?);
    let i = u32::try_from(index).ok()?;
    if c0 == c1 && r0 + i <= r1 {
        Some((sheet, c0, r0 + i))
    } else if r0 == r1 && c0 + i <= c1 {
        Some((sheet, c0 + i, r0))
    } else {
        None
    }
}

/// The changed cells written into the chart's embedded workbook (the data
/// PowerPoint's データの編集 opens), if it has one. A cell the chart's
/// references do not name plainly is refused.
pub(crate) fn write_embedded_cells(package: &mut Package, chart_part: &str, cells: &[CellChange]) -> Result<(), Error> {
    if cells.is_empty() {
        return Ok(());
    }
    let Some(part) = package.relationships(chart_part)?.into_iter().find(|r| r.kind == REL_PACKAGE && !r.external).and_then(|r| r.target_part(chart_part)) else {
        return Ok(());
    };
    let bytes = package.read(&part)?;
    let opened = crate::open(bytes).map_err(|e| Error::new(format!("グラフのデータのブックを開けません: {e}")))?;
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    let mut edits: Vec<(ibis_doc_model::SlideId, ibis_doc_model::CellEdit)> = Vec::new();
    for change in cells {
        let (formula, index, value) = match change {
            CellChange::Number(f, i, v) => (f, *i, v.map_or(ibis_doc_model::CellInput::Clear, |v| ibis_doc_model::CellInput::Value(ibis_doc_model::CellValue::Number(v)))),
            CellChange::Text(f, i, t) => (f, *i, ibis_doc_model::CellInput::Value(ibis_doc_model::CellValue::Text(t.clone()))),
        };
        let (sheet, column, row) = point_cell(formula, index).ok_or_else(|| unsupported("グラフのデータの範囲"))?;
        let slide = doc.presentation().slides.iter().find(|s| s.name.as_deref() == Some(sheet.as_str())).map(|s| s.id).ok_or_else(|| unsupported("グラフのデータのシート"))?;
        edits.push((slide, ibis_doc_model::CellEdit { row, column, input: value }));
    }
    let mut by_sheet: Vec<(ibis_doc_model::SlideId, Vec<ibis_doc_model::CellEdit>)> = Vec::new();
    for (slide, edit) in edits {
        match by_sheet.iter_mut().find(|(s, _)| *s == slide) {
            Some((_, list)) => list.push(edit),
            None => by_sheet.push((slide, vec![edit])),
        }
    }
    let commands = by_sheet.into_iter().map(|(slide, edits)| ibis_doc_model::Command::Sheet(ibis_doc_model::SheetCommand::SetCells { slide, edits })).collect();
    doc.execute(commands).map_err(|e| Error::new(format!("グラフのデータのブックを変えられません: {e:?}")))?;
    let saved = opened.source.save(doc.presentation(), crate::SaveOptions::default())?;
    package.set_bytes(&part, saved);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_name_their_points_cells() {
        assert_eq!(point_cell("Sheet1!$B$2:$B$5", 2), Some(("Sheet1".into(), 1, 3)));
        assert_eq!(point_cell("'売上 2026'!$B$1:$D$1", 1), Some(("売上 2026".into(), 2, 0)));
        assert_eq!(point_cell("Sheet1!$B$1", 0), Some(("Sheet1".into(), 1, 0)));
        assert_eq!(point_cell("Sheet1!$B$2:$B$5", 4), None);
        assert_eq!(cell_address("AA10"), Some((26, 9)));
    }
}
