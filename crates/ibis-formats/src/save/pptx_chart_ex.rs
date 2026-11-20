// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The data of a chartEx chart (`cx:chartSpace`: waterfall, histogram, Pareto,
//! funnel…) edited in its workbook: the references (`cx:f`) its data take
//! their names, categories and values from, read from the part, and the
//! caches (`cx:lvl`) rewritten from the workbook. Other changes to such a
//! chart: its title and legend are written too (`write_chart_format`); the
//! rest of its format is not written yet.

use ibis_doc_model::Presentation;
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::pptx_chart::REL_PACKAGE;
use super::pptx_chart_data::{cells, number, parse_ref, shown, ChartData, SeriesRefs};
use crate::Error;
use crate::save::unsupported;

const CX_NS: &str = "http://schemas.microsoft.com/office/drawing/2014/chartex";

/// Whether `part` is a chartEx part.
pub(crate) fn is_chart_ex(package: &Package, part: &str) -> bool {
    package.xml(part).ok().is_some_and(|d| d.root_element().is_some_and(|r| d.namespace(r) == Some(CX_NS)))
}

fn data_node(doc: &XmlDocument) -> Option<NodeId> {
    let root = doc.root_element()?;
    doc.child(root, "chartData").and_then(|d| doc.child(d, "data"))
}

fn formula(doc: &XmlDocument, node: Option<NodeId>) -> Option<String> {
    node.and_then(|n| doc.child(n, "f")).map(|f| doc.text(f)).filter(|f| !f.is_empty())
}

/// The references of the chart's data: one series (the first): its name, categories, values.
pub(crate) fn chart_refs(doc: &XmlDocument) -> Vec<SeriesRefs> {
    let Some(data) = data_node(doc) else { return Vec::new() };
    let by_type = |tag: &str, kind: &str| doc.children_named(data, tag).find(|&d| doc.attr(d, "type") == Some(kind));
    let name = doc.root_element().and_then(|r| doc.path(r, &["chart", "plotArea", "plotAreaRegion"])).and_then(|region| doc.children_named(region, "series").next()).and_then(|s| doc.path(s, &["tx", "txData"])).and_then(|t| formula(doc, Some(t)));
    let categories = formula(doc, by_type("strDim", "cat"));
    // A treemap's and a sunburst's values are sizes.
    let values = formula(doc, by_type("numDim", "val").or_else(|| by_type("numDim", "size")));
    if values.is_none() {
        return Vec::new();
    }
    vec![SeriesRefs { name, categories, values, sizes: None }]
}

/// The chart part's workbook (embedded) and references.
pub(crate) fn read_chart_data(package: &Package, chart_part: &str) -> Result<ChartData, Error> {
    let refs = chart_refs(&package.xml(chart_part)?);
    let book = package.relationships(chart_part)?.into_iter().find(|r| r.kind == REL_PACKAGE && !r.external).and_then(|r| r.target_part(chart_part)).filter(|p| package.contains(p));
    match book {
        Some(book) if !refs.is_empty() => Ok(ChartData { workbook: package.read(&book)?.to_vec(), series: refs }),
        _ => Err(unsupported("ブックのないグラフのデータの編集")),
    }
}

fn prefix_of(doc: &XmlDocument, node: NodeId) -> String {
    doc.name(node).split_once(':').map(|(p, _)| format!("{p}:")).unwrap_or_default()
}

/// The caches of the data of `chart_part` rewritten from `data`'s workbook (by the references
/// of its first series), and the workbook put in the file.
pub(crate) fn write_chart_data(package: &mut Package, chart_part: &str, data: &ChartData) -> Result<(), Error> {
    let opened = crate::open(data.workbook.clone()).map_err(|e| Error::new(format!("グラフのデータのブックを開けません: {e}")))?;
    let workbook: Presentation = opened.presentation;
    let refs = data.series.first().ok_or_else(|| unsupported("グラフのデータの範囲"))?;
    let doc = package.xml_mut(chart_part)?;
    let node = data_node(doc).ok_or_else(|| unsupported("このグラフのデータの変更"))?;
    for dim in doc.elements(node).filter(|&d| matches!(doc.local_name(d), "strDim" | "numDim")).collect::<Vec<_>>() {
        let (numeric, kind) = (doc.local_name(dim) == "numDim", doc.attr(dim, "type").unwrap_or("").to_string());
        let reference = match (kind.as_str(), numeric) {
            ("cat", false) => &refs.categories,
            ("val" | "size", true) => &refs.values,
            _ => continue,
        };
        let Some(f) = reference else { continue };
        let list = cells(&workbook, f).ok_or_else(|| unsupported("グラフのデータの範囲"))?;
        // Categories over several columns (a treemap's or a sunburst's levels): a `cx:lvl` for each column, the last
        // column (the innermost level) first.
        let rows = parse_ref(f).map_or(list.len(), |(_, r)| (r.last_row - r.first_row + 1) as usize).max(1);
        let columns: Vec<&[Option<&ibis_doc_model::SheetCell>]> = if !numeric && list.len() > rows { list.chunks(rows).rev().collect() } else { vec![&list[..]] };
        let p = prefix_of(doc, dim);
        let format = doc.child(dim, "lvl").and_then(|l| doc.attr(l, "formatCode")).map(str::to_string);
        for old in doc.children(dim).to_vec() {
            if doc.is_element(old) && matches!(doc.local_name(old), "f" | "lvl") {
                doc.detach(old);
            }
        }
        let fnode = doc.insert_element(dim, 0, &format!("{p}f"));
        doc.set_text(fnode, f);
        for column in columns {
            let end = doc.children(dim).len();
            let lvl = doc.insert_element(dim, end, &format!("{p}lvl"));
            doc.set_attr(lvl, "ptCount", &column.len().to_string());
            if numeric {
                doc.set_attr(lvl, "formatCode", format.as_deref().unwrap_or("General"));
            }
            for (i, cell) in column.iter().enumerate() {
                let text = if numeric { number(*cell).map(super::pptx_chart::number_text) } else { Some(shown(*cell)).filter(|t| !t.is_empty()) };
                if let Some(text) = text {
                    let pt = doc.append_element(lvl, &format!("{p}pt"));
                    doc.set_attr(pt, "idx", &i.to_string());
                    doc.set_text(pt, &text);
                }
            }
        }
    }
    // The series' name.
    let txdata = {
        let region = doc.root_element().and_then(|r| doc.path(r, &["chart", "plotArea", "plotAreaRegion"]));
        let first = region.and_then(|region| doc.children_named(region, "series").next());
        first.and_then(|s| doc.path(s, &["tx", "txData"]))
    };
    if let (Some(f), Some(txdata)) = (&refs.name, txdata) {
        let name = cells(&workbook, f).map(|c| c.into_iter().map(shown).filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" ")).unwrap_or_default();
        let p = prefix_of(doc, txdata);
        for old in doc.children(txdata).to_vec() {
            doc.detach(old);
        }
        let fnode = doc.append_element(txdata, &format!("{p}f"));
        doc.set_text(fnode, f);
        let v = doc.append_element(txdata, &format!("{p}v"));
        doc.set_text(v, &name);
    }
    let book = package.relationships(chart_part)?.into_iter().find(|r| r.kind == REL_PACKAGE && !r.external).and_then(|r| r.target_part(chart_part)).ok_or_else(|| unsupported("ブックのないグラフのデータの編集"))?;
    package.set_bytes(&book, data.workbook.clone());
    Ok(())
}

/// The title and the legend of chartEx chart `part` made those of `after` (from `before`); any other
/// difference between the two is not written (an error, unless the data was edited: then it is the data's).
pub(crate) fn write_chart_format(package: &mut Package, part: &str, before: &ibis_doc_model::Chart, after: &ibis_doc_model::Chart, data_edited: bool) -> Result<(), Error> {
    use ibis_doc_model::LegendPosition;
    let mut rest = after.clone();
    rest.title = before.title.clone();
    rest.legend = before.legend.clone();
    // The data labels, the value axis' range and the fills of the series are written too (`ex_format`); they change the model
    // only through it (what the old kinds' menus change in the groups is not written).
    rest.ex_format = before.ex_format.clone();
    // What the file's reading of those makes of the groups and the axes (its data labels, its axis' range, its fills) is theirs.
    if before.ex_format.is_some() {
        rest.groups = before.groups.clone();
        rest.axes = before.axes.clone();
        rest.ex = before.ex.clone();
    }
    if rest != *before && !data_edited {
        return Err(unsupported("この種類のグラフの変更"));
    }
    let (format_before, format_after) = (before.ex_format.clone().unwrap_or_default(), after.ex_format.clone().unwrap_or_default());
    if before.title == after.title && before.legend == after.legend && format_before == format_after {
        return Ok(());
    }
    if format_before != format_after {
        write_format(package, part, &format_before, &format_after)?;
    }
    if before.title == after.title && before.legend == after.legend {
        return Ok(());
    }
    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| unsupported("このグラフの変更"))?;
    let chart = doc.child(root, "chart").ok_or_else(|| unsupported("このグラフの変更"))?;
    let p = prefix_of(doc, chart);
    // The title: first in `cx:chart`.
    if before.title != after.title {
        match (&after.title, doc.child(chart, "title")) {
            (None, Some(old)) => doc.detach(old),
            (Some(title), existing) => {
                let node = match existing {
                    Some(n) => n,
                    None => {
                        let n = doc.insert_element(chart, 0, &format!("{p}title"));
                        doc.set_attr(n, "pos", "t");
                        doc.set_attr(n, "align", "ctr");
                        doc.set_attr(n, "overlay", "0");
                        n
                    }
                };
                // The text as plain text (a rich one is replaced by it).
                if let Some(tx) = doc.child(node, "tx") {
                    doc.detach(tx);
                }
                let tx = doc.insert_element(node, 0, &format!("{p}tx"));
                let data = doc.append_element(tx, &format!("{p}txData"));
                let v = doc.append_element(data, &format!("{p}v"));
                doc.set_text(v, &title.text);
            }
            (None, None) => {}
        }
    }
    // The legend: after `cx:plotArea`.
    if before.legend != after.legend {
        match (&after.legend, doc.child(chart, "legend")) {
            (None, Some(old)) => doc.detach(old),
            (Some(legend), existing) => {
                let pos = match legend.source_position {
                    Some(LegendPosition::Top) => "t",
                    Some(LegendPosition::Bottom) => "b",
                    Some(LegendPosition::Left) => "l",
                    _ => "r",
                };
                let node = match existing {
                    Some(n) => n,
                    None => {
                        let at = doc.children(chart).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "plotArea").map_or(doc.children(chart).len(), |i| i + 1);
                        let n = doc.insert_element(chart, at, &format!("{p}legend"));
                        doc.set_attr(n, "align", "ctr");
                        doc.set_attr(n, "overlay", "0");
                        n
                    }
                };
                doc.set_attr(node, "pos", pos);
            }
            (None, None) => {}
        }
    }
    Ok(())
}

/// The first series (`cx:series`) of the chart part.
fn first_series(doc: &XmlDocument) -> Option<NodeId> {
    let root = doc.root_element()?;
    let region = doc.path(root, &["chart", "plotArea", "plotAreaRegion"])?;
    doc.children_named(region, "series").next()
}

/// `cx:series` children come in this order: tx, spPr, valueColors, valueColorPositions, dataPt, dataLabels, dataId, layoutPr,
/// axisId, extLst; where a new `name` goes among them.
fn series_slot(doc: &XmlDocument, series: NodeId, name: &str) -> usize {
    const ORDER: [&str; 10] = ["tx", "spPr", "valueColors", "valueColorPositions", "dataPt", "dataLabels", "dataId", "layoutPr", "axisId", "extLst"];
    let rank = |n: &str| ORDER.iter().position(|o| *o == n).unwrap_or(ORDER.len());
    let mine = rank(name);
    doc.children(series).iter().position(|&c| doc.is_element(c) && rank(doc.local_name(c)) > mine).unwrap_or(doc.children(series).len())
}

fn solid_fill(rgb: ibis_doc_model::Rgb) -> String {
    format!("{:06X}", rgb.0 & 0xFF_FFFF)
}

/// The data labels, the value axis' range and the fills of the series written from `after` (they differ from `before`).
fn write_format(package: &mut Package, part: &str, before: &ibis_doc_model::ExFormat, after: &ibis_doc_model::ExFormat) -> Result<(), Error> {
    use ibis_doc_model::LabelPosition;
    let doc = package.xml_mut(part)?;
    let series = first_series(doc).ok_or_else(|| unsupported("このグラフの変更"))?;
    let p = prefix_of(doc, series);
    // The data labels.
    if before.labels != after.labels {
        let old = doc.child(series, "dataLabels");
        match (&after.labels, old) {
            (None, Some(old)) => doc.detach(old),
            (None, None) => {}
            (Some(labels), existing) => {
                let node = match existing {
                    Some(n) => n,
                    None => {
                        let at = series_slot(doc, series, "dataLabels");
                        doc.insert_element(series, at, &format!("{p}dataLabels"))
                    }
                };
                match labels.position {
                    Some(LabelPosition::OutsideEnd) => doc.set_attr(node, "pos", "outEnd"),
                    Some(LabelPosition::InsideEnd) => doc.set_attr(node, "pos", "inEnd"),
                    Some(LabelPosition::Center) => doc.set_attr(node, "pos", "ctr"),
                    Some(LabelPosition::InsideBase) => doc.set_attr(node, "pos", "inBase"),
                    _ => doc.remove_attr(node, "pos"),
                }
                if let Some(old) = doc.child(node, "visibility") {
                    doc.detach(old);
                }
                // After `numFmt`, `spPr` and `txPr`.
                let at = doc.children(node).iter().position(|&c| doc.is_element(c) && !matches!(doc.local_name(c), "numFmt" | "spPr" | "txPr")).unwrap_or(doc.children(node).len());
                let v = doc.insert_element(node, at, &format!("{p}visibility"));
                let flag = |b: bool| if b { "1" } else { "0" };
                doc.set_attr(v, "seriesName", flag(labels.series_name));
                doc.set_attr(v, "categoryName", flag(labels.category));
                doc.set_attr(v, "value", flag(labels.value));
            }
        }
    }
    // The value axis' range: the first axis with a `cx:valScaling`.
    if (before.axis_min, before.axis_max, before.axis_major) != (after.axis_min, after.axis_max, after.axis_major) {
        let root = doc.root_element().ok_or_else(|| unsupported("このグラフの変更"))?;
        let plot = doc.path(root, &["chart", "plotArea"]).ok_or_else(|| unsupported("このグラフの変更"))?;
        let scaling = doc.children_named(plot, "axis").find_map(|a| doc.child(a, "valScaling")).ok_or_else(|| unsupported("このグラフの軸の変更"))?;
        for (name, v) in [("min", after.axis_min), ("max", after.axis_max), ("majorUnit", after.axis_major)] {
            match v {
                Some(ibis_doc_model::Number(v)) => doc.set_attr(scaling, name, &super::pptx_chart::number_text(v)),
                None => doc.remove_attr(scaling, name),
            }
        }
    }
    // The fills.
    if before.fills != after.fills {
        let waterfall = doc.attr(series, "layoutId") == Some("waterfall");
        if waterfall {
            let roles = waterfall_roles_of(doc, series);
            for role in 0..3 {
                let (was, now) = (before.fills.get(role).copied().flatten(), after.fills.get(role).copied().flatten());
                if was == now {
                    continue;
                }
                let points: Vec<usize> = roles.iter().enumerate().filter(|(_, r)| **r == Some(role)).map(|(i, _)| i).collect();
                for old in doc.children_named(series, "dataPt").collect::<Vec<_>>() {
                    if doc.attr(old, "idx").and_then(|i| i.parse::<usize>().ok()).is_some_and(|i| points.contains(&i)) {
                        doc.detach(old);
                    }
                }
                if let Some(rgb) = now {
                    for i in points {
                        // By point number among the data points.
                        let before_it = doc.children(series).iter().position(|&c| doc.is_element(c) && ((doc.local_name(c) == "dataPt" && doc.attr(c, "idx").and_then(|x| x.parse::<usize>().ok()).is_some_and(|x| x > i)) || !matches!(doc.local_name(c), "tx" | "spPr" | "valueColors" | "valueColorPositions" | "dataPt")));
                        let at = before_it.unwrap_or(doc.children(series).len());
                        let pt = doc.insert_element(series, at, &format!("{p}dataPt"));
                        doc.set_attr(pt, "idx", &i.to_string());
                        let sp = doc.append_element(pt, &format!("{p}spPr"));
                        let fill = doc.append_element(sp, "a:solidFill");
                        let c = doc.append_element(fill, "a:srgbClr");
                        doc.set_attr(c, "val", &solid_fill(rgb));
                    }
                }
            }
        } else {
            let now = after.fills.first().copied().flatten();
            if let Some(old) = doc.child(series, "spPr") {
                doc.detach(old);
            }
            if let Some(rgb) = now {
                let at = series_slot(doc, series, "spPr");
                let sp = doc.insert_element(series, at, &format!("{p}spPr"));
                let fill = doc.append_element(sp, "a:solidFill");
                let c = doc.append_element(fill, "a:srgbClr");
                doc.set_attr(c, "val", &solid_fill(rgb));
            }
        }
    }
    Ok(())
}

/// The owner of each point of a waterfall (0 an increase, 1 a decrease, 2 a total), from the part's values and subtotals.
fn waterfall_roles_of(doc: &XmlDocument, series: NodeId) -> Vec<Option<usize>> {
    let numbers: Vec<Option<f64>> = (|| {
        let root = doc.root_element()?;
        let data = doc.path(root, &["chartData", "data"])?;
        let dim = doc.children_named(data, "numDim").find(|&d| doc.attr(d, "type") == Some("val"))?;
        let lvl = doc.child(dim, "lvl")?;
        let count: usize = doc.attr(lvl, "ptCount").and_then(|c| c.parse().ok()).unwrap_or(0);
        let mut points = vec![None; count];
        for pt in doc.children_named(lvl, "pt") {
            let idx: usize = doc.attr(pt, "idx").and_then(|i| i.parse().ok())?;
            if idx >= points.len() {
                points.resize(idx + 1, None);
            }
            points[idx] = doc.text(pt).trim().parse().ok();
        }
        Some(points)
    })()
    .unwrap_or_default();
    let subtotals: Vec<usize> = doc
        .path(series, &["layoutPr", "subtotals"])
        .map(|s| doc.children_named(s, "idx").filter_map(|i| doc.attr(i, "val").and_then(|v| v.parse().ok())).collect())
        .unwrap_or_default();
    crate::chart_ex::waterfall_roles(&numbers, &subtotals)
}
