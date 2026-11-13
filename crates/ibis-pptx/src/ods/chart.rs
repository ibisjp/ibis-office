// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The charts of an `.ods` (an embedded object with `office:chart`, ODF 1.3 chapter 11) as the chart part of
//! the `.xlsx` made from it. The chart is read into the description the old workbooks' charts use
//! (`xls_chart`) and written by it: bars and columns, lines, areas, pies and rings, scatters, radars; their
//! series (the cells they read, or the data table the chart holds), colours, titles, legend and axes.

use std::collections::HashMap;

use roxmltree::{Document, Node};

use super::{TABLE, TEXT, attr, children, split_reference};
use crate::xls_chart::{Axis, Cell, Cells, Chart, Group, Kind, Ref, Series, Source};

const CHART: &str = "urn:oasis:names:tc:opendocument:xmlns:chart:1.0";
const STYLE: &str = super::STYLE;
const OFFICE: &str = super::OFFICE;

/// The sheets of the document by index (a `Ref`'s `ixti`), for reading the cells a series names.
pub(super) struct OdsCells<'a> {
    pub tables: Vec<(String, Node<'a, 'a>)>,
}

/// The value of a cell element.
fn cell_value(cell: Node) -> Cell {
    let number = |name: &str| attr(cell, OFFICE, name).and_then(|v| v.parse::<f64>().ok());
    match attr(cell, OFFICE, "value-type") {
        Some("float" | "percentage" | "currency") => number("value").map_or(Cell::Empty, Cell::Number),
        Some("string") => Cell::Text(attr(cell, OFFICE, "string-value").map(str::to_string).unwrap_or_else(|| super::cell_text(cell).0)),
        Some("boolean") => Cell::Number(if attr(cell, OFFICE, "boolean-value") == Some("true") { 1.0 } else { 0.0 }),
        Some("date") => attr(cell, OFFICE, "date-value").and_then(super::date_serial).map_or(Cell::Empty, Cell::Number),
        Some("time") => attr(cell, OFFICE, "time-value").and_then(super::time_serial).map_or(Cell::Empty, Cell::Number),
        _ => Cell::Empty,
    }
}

/// The cells of rows `r0..=r1`, columns `c0..=c1` of a table, row by row.
fn table_range(table: Node, r0: u32, r1: u32, c0: u32, c1: u32) -> Vec<Cell> {
    let width = (c1 - c0 + 1) as usize;
    let mut out = vec![Cell::Empty; width * (r1 - r0 + 1) as usize];
    let mut rows: Vec<Node> = Vec::new();
    for g in table.children().filter(|c| c.is_element()) {
        match g.tag_name().name() {
            "table-row" => rows.push(g),
            "table-row-group" | "table-header-rows" | "table-rows" => rows.extend(children(g, TABLE, "table-row")),
            _ => {}
        }
    }
    let mut row = 0u32;
    for r in rows {
        let repeat = attr(r, TABLE, "number-rows-repeated").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
        if row > r1 {
            break;
        }
        if row + repeat > r0 {
            let mut c = 0u32;
            for cell in r.children().filter(|n| n.is_element() && matches!(n.tag_name().name(), "table-cell" | "covered-table-cell")) {
                let cr = attr(cell, TABLE, "number-columns-repeated").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1);
                if c > c1 {
                    break;
                }
                let v = cell_value(cell);
                if v != Cell::Empty {
                    for k in 0..cr.min(1024) {
                        let (cc, _) = (c + k, 0);
                        if cc < c0 || cc > c1 {
                            continue;
                        }
                        for rr in row.max(r0)..(row + repeat).min(r1 + 1) {
                            out[(rr - r0) as usize * width + (cc - c0) as usize] = v.clone();
                        }
                    }
                }
                c += cr;
            }
        }
        row += repeat;
    }
    out
}

impl Cells for OdsCells<'_> {
    fn range(&self, r: &Ref) -> Option<(String, Vec<Cell>)> {
        let (name, table) = self.tables.get(r.ixti as usize)?;
        Some((name.clone(), table_range(*table, r.rows.0 as u32, r.rows.1 as u32, r.columns.0 as u32, r.columns.1 as u32)))
    }

    fn color(&self, index: usize) -> u32 {
        const PALETTE: [u32; 8] = [0x004586, 0xFF420E, 0xFFD320, 0x579D1C, 0x7E0021, 0x83CAFF, 0x314004, 0xAECF00];
        PALETTE[index % PALETTE.len()]
    }
}

/// A range address (`Data.B4:Data.B8`, `$'My Sheet'.$A$1:.$A$9`) as a [`Ref`] into `tables`.
fn parse_ref(text: &str, tables: &[(String, Node)]) -> Option<Ref> {
    let first = text.split_whitespace().next()?;
    let mut parts = first.split(':');
    let a = parts.next()?;
    let b = parts.next().unwrap_or(a);
    let (sheet, cell_a) = split_reference(a);
    let (_, cell_b) = split_reference(b);
    let (ca, ra) = super::cell_address(&cell_a)?;
    let (cb, rb) = super::cell_address(&cell_b)?;
    let ixti = match sheet {
        Some(name) => tables.iter().position(|(n, _)| *n == name)?,
        None => 0,
    };
    Some(Ref { ixti: ixti as u16, rows: (ra.min(rb) as u16, ra.max(rb) as u16), columns: (ca.min(cb) as u16, ca.max(cb) as u16) })
}

fn hex(text: &str) -> Option<u32> {
    u32::from_str_radix(text.trim().strip_prefix('#')?, 16).ok()
}

fn text_of(n: Node) -> String {
    children(n, TEXT, "p").map(|p| p.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

/// The chart of the sub-document `content` (`content.xml` of an embedded chart object) as a chart.
pub(super) fn read_chart(content: &str, tables: &[(String, Node)]) -> Option<Chart> {
    let doc = Document::parse_with_options(content, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }).ok()?;
    let root = doc.root_element();
    let body = children(root, OFFICE, "body").next()?;
    let chart_el = children(body, OFFICE, "chart").next().and_then(|c| children(c, CHART, "chart").next())?;
    // The automatic styles: each name's properties (attributes of the property elements, by local name).
    let mut styles: HashMap<String, HashMap<String, String>> = HashMap::new();
    for holder in children(root, OFFICE, "automatic-styles") {
        for st in holder.children().filter(|c| c.is_element() && c.tag_name().name() == "style") {
            let Some(name) = attr(st, STYLE, "name") else { continue };
            let entry = styles.entry(name.to_string()).or_default();
            for props in st.children().filter(|c| c.is_element()) {
                for a in props.attributes() {
                    entry.insert(a.name().to_string(), a.value().to_string());
                }
            }
        }
    }
    let style_of = |n: Node| -> HashMap<String, String> { attr(n, CHART, "style-name").and_then(|s| styles.get(s)).cloned().unwrap_or_default() };
    fn class_of<'a>(n: Node<'a, '_>) -> Option<&'a str> {
        attr(n, CHART, "class").map(|c| c.trim_start_matches("chart:"))
    }
    let chart_class = class_of(chart_el).unwrap_or("bar").to_string();
    let plot = children(chart_el, CHART, "plot-area").next()?;
    let plot_style = style_of(plot);
    let flag = |m: &HashMap<String, String>, k: &str| m.get(k).map(String::as_str) == Some("true");

    let mut out = Chart::default();
    out.title = children(chart_el, CHART, "title").next().map(text_of).filter(|t| !t.is_empty());
    out.legend = children(chart_el, CHART, "legend").next().and_then(|l| {
        let pos = attr(l, CHART, "legend-position").unwrap_or("end");
        let align = attr(l, CHART, "legend-align");
        Some(match (pos, align) {
            ("top", Some("end")) | ("end", Some("start")) => "tr",
            ("start", _) => "l",
            ("top", _) => "t",
            ("bottom", _) => "b",
            _ => "r",
        })
    });
    // The axes.
    let mut categories: Source = Source::None;
    let mut y_style = HashMap::new();
    for axis in children(plot, CHART, "axis") {
        let dim = attr(axis, CHART, "dimension").unwrap_or("");
        let st = style_of(axis);
        let target = if dim == "y" { &mut out.value } else { &mut out.category };
        if dim == "x" || dim == "y" {
            let num = |k: &str| st.get(k).and_then(|v| v.parse::<f64>().ok());
            *target = Axis { format: None, min: num("minimum"), max: num("maximum"), major: num("interval-major"), log: flag(&st, "logarithmic"), reversed: flag(&st, "reverse-direction"), grid: children(axis, CHART, "grid").next().is_some_and(|g| attr(g, CHART, "class") != Some("minor")), title: children(axis, CHART, "title").next().map(text_of).filter(|t| !t.is_empty()), display_unit: None };
        }
        if dim == "y" && y_style.is_empty() {
            y_style = st.clone();
        }
        if dim == "x" {
            if let Some(cat) = children(axis, CHART, "categories").next().and_then(|c| attr(c, TABLE, "cell-range-address")).and_then(|a| parse_ref(a, tables)) {
                categories = Source::Ref(cat);
            }
        }
    }
    // The groups: one for each class of series.
    let kind_of = |class: &str| -> Option<(Kind, bool, u16)> {
        Some(match class {
            "bar" => (Kind::Bar, false, 0),
            "line" => (Kind::Line, false, 0),
            "area" => (Kind::Area, false, 0),
            "circle" => (Kind::Pie, false, 0),
            "ring" => (Kind::Pie, false, 50),
            "scatter" | "bubble" => (Kind::Scatter, false, 0),
            "radar" | "net" => (Kind::Radar, false, 0),
            "filled-radar" | "filled-net" => (Kind::Radar, true, 0),
            _ => return None,
        })
    };
    let mut classes: Vec<String> = Vec::new();
    let default_class = chart_class.clone();
    for (i, s) in children(plot, CHART, "series").enumerate() {
        let class = class_of(s).map(str::to_string).unwrap_or_else(|| default_class.clone());
        let Some((kind, filled, hole)) = kind_of(&class) else { continue };
        let group = match classes.iter().position(|c| *c == class) {
            Some(g) => g,
            None => {
                classes.push(class.clone());
                out.groups.push(Group {
                    kind,
                    horizontal: kind == Kind::Bar && flag(&plot_style, "vertical"),
                    stacked: flag(&plot_style, "stacked"),
                    percent: flag(&plot_style, "percentage"),
                    overlap: y_style.get("overlap").and_then(|v| v.parse().ok()).unwrap_or(0),
                    gap: y_style.get("gap-width").and_then(|v| v.parse().ok()).unwrap_or(150),
                    hole,
                    angle: 0,
                    filled,
                });
                classes.len() - 1
            }
        };
        let st = style_of(s);
        let mut series = Series { group, fill: st.get("fill-color").and_then(|c| hex(c)), line: st.get("stroke-color").and_then(|c| hex(c)), smooth: plot_style.get("interpolation").is_some_and(|v| v != "none"), ..Default::default() };
        series.line_none = st.get("stroke").map(String::as_str) == Some("none") && matches!(kind, Kind::Line | Kind::Scatter);
        series.values = attr(s, CHART, "values-cell-range-address").and_then(|a| parse_ref(a, tables)).map_or(Source::None, Source::Ref);
        series.name = attr(s, CHART, "label-cell-address").and_then(|a| parse_ref(a, tables)).map_or(Source::None, Source::Ref);
        series.categories = if kind == Kind::Scatter { children(s, CHART, "domain").next().and_then(|d| attr(d, TABLE, "cell-range-address")).and_then(|a| parse_ref(a, tables)).map_or(Source::None, Source::Ref) } else { categories.clone() };
        // The colours of single points (a pie's slices).
        let mut at = 0u16;
        for p in children(s, CHART, "data-point") {
            let repeated = attr(p, CHART, "repeated").and_then(|v| v.parse::<u16>().ok()).unwrap_or(1);
            if let Some(c) = style_of(p).get("fill-color").and_then(|c| hex(c)) {
                for k in 0..repeated {
                    series.points.push((at + k, c));
                }
            }
            at += repeated;
        }
        let _ = i;
        out.series.push(series);
    }
    // A chart with a table of its own and no cells: the series read the table.
    if out.series.iter().any(|s| s.values == Source::None) {
        let local = chart_el.descendants().find(|n| n.is_element() && n.tag_name().name() == "table" && n.tag_name().namespace() == Some(TABLE))?;
        let rows = table_cells(local);
        let names: Vec<Cell> = rows.first().cloned().unwrap_or_default();
        let body_rows = &rows[1.min(rows.len())..];
        let cats: Vec<Cell> = body_rows.iter().map(|r| r.first().cloned().unwrap_or(Cell::Empty)).collect();
        for (k, s) in out.series.iter_mut().enumerate() {
            if s.values != Source::None {
                continue;
            }
            s.values = Source::Lit(body_rows.iter().map(|r| r.get(k + 1).cloned().unwrap_or(Cell::Empty)).collect());
            s.name = Source::Lit(vec![names.get(k + 1).cloned().unwrap_or(Cell::Empty)]);
            s.categories = Source::Lit(cats.clone());
        }
    }
    out.series.retain(|s| s.values != Source::None);
    if out.groups.is_empty() || out.series.is_empty() {
        return None;
    }
    Some(out)
}

/// The rows of a chart's own table (`table:table`), header rows first.
fn table_cells(table: Node) -> Vec<Vec<Cell>> {
    table
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "table-row" && n.tag_name().namespace() == Some(TABLE))
        .map(|r| {
            let mut row = Vec::new();
            for c in r.children().filter(|n| n.is_element() && n.tag_name().name() == "table-cell") {
                let repeat = attr(c, TABLE, "number-columns-repeated").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).min(256);
                let v = cell_value(c);
                for _ in 0..repeat {
                    row.push(v.clone());
                }
            }
            row
        })
        .collect()
}
