// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! DrawingML chart parts (`ppt/charts/chartN.xml`) to [`Chart`].
//!
//! Values come from the caches the part stores next to its formulas; the
//! embedded workbook is not read.

use ibis_doc_model::{
    Axis, AxisCrosses, AxisKind, AxisSourceKind, Chart, ChartDataTable, TimeScale, TimeUnit, ChartGroup, ChartText, ChartTitle, ChartType, Color, DataLabels, Fill,
    FractionRect, GradientStop, Grouping, LabelPiece, LabelPosition, Legend, LegendPosition, LineEnd, Marker,
    MarkerSource, MarkerSourceSymbol, MarkerSymbol, Number, OfPieKind, OfPieSettings, OfPieSplit, PointLabel, RadarStyle, Series, Side, Stroke,
    Trendline, TrendlineType,
    ErrorBarDirection, ErrorBarType, ErrorBarValueType, ErrorBars,
};
use roxmltree::Node;

use crate::style::RunProps;
use crate::theme::Colors;
use crate::xml::{attr, attr_bool, attr_i64, child, children, elements, path};

/// `c:foo val="…"`.
fn val<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    child(node, name).and_then(|n| attr(n, "val"))
}

// Crossing metadata is chart-namespace specific. Foreign same-name nodes
// remain opaque in original XML and must never supply typed values.
const CHART_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
fn unqualified_val<'a>(node: Node<'a, '_>) -> Option<&'a str> {
    node.attributes().find(|a| a.name() == "val" && a.namespace().is_none()).map(|a| a.value())
}
fn axis_val<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    elements(node).find(|n| n.has_tag_name((CHART_NS, name)))
        .and_then(unqualified_val)
}

fn val_bool(node: Node, name: &str) -> Option<bool> {
    child(node, name).map(|n| attr_bool(n, "val").unwrap_or(true))
}

fn val_f64(node: Node, name: &str) -> Option<f64> {
    val(node, name)?.trim().parse().ok()
}

/// Read a chart part's root (`c:chartSpace`).
pub(crate) fn read(space: Node, colors: &Colors) -> Option<Chart> {
    read_in(space, colors, true)
}

/// `read`; `legacy_border`: a chart of the old defaults has their thin grey
/// border (Excel draws it; PowerPoint draws none, as measured on PCHT).
pub(crate) fn read_in(space: Node, colors: &Colors, legacy_border: bool) -> Option<Chart> {
    if crate::chart_ex::is_chart_ex(space) {
        return crate::chart_ex::read(space, colors, legacy_border);
    }
    let chart = child(space, "chart")?;
    let plot = child(chart, "plotArea")?;
    // A chart with no formatting of its own (as Excel 2007 wrote them) takes
    // that version's defaults: black text, an 18 pt bold title, a thin grey
    // border, and black axis lines and gridlines. (Its text may still have
    // a size of its own, `c:txPr`: PowerPoint draws such a chart so too, as
    // measured on the coverage suite PCHT.)
    let legacy = child(space, "spPr").is_none();
    // A chart Excel itself wrote names its style twice, in
    // `mc:AlternateContent` (`c14:style` and a `c:style` fallback); the
    // coverage suite's generated charts have none, and openpyxl writes a bare
    // `c:style` (its 10 pt titles are measured in a user's private workbook).
    let excel_wrote = space.descendants().any(|n| n.is_element() && n.tag_name().name() == "AlternateContent" && n.descendants().any(|m| m.is_element() && m.tag_name().name() == "style"));
    // A slide's chart that names its style (PowerPoint's `c14:style` 102 / `c:style` 2) has that style's formats: lines of the theme's text colour tinted 75%, bold titles
    // (PowerPoint 16.0.20527.20088's PDF of a chart with no `c:spPr` anywhere; one that names no style has black lines and titles of regular weight).
    let powerpoint_styled = legacy && !legacy_border && excel_wrote;
    // Its text is in the theme's body fonts (MS P Gothic for Japanese when
    // the theme names no East Asian font).
    let default_text = if legacy {
        let theme = colors.theme();
        let latin = theme.typeface("+mn-lt");
        let east_asian = theme.typeface("+mn-ea").filter(|f| Some(f) != latin.as_ref()).or_else(|| {
            val(space, "lang").is_some_and(|l| l.starts_with("ja")).then(|| "ＭＳ Ｐゴシック".to_string())
        });
        ChartText { color: Some(Color::opaque(ibis_doc_model::Rgb::BLACK)), font: latin, east_asian_font: east_asian, ..ChartText::default() }
    } else {
        ChartText::default()
    };
    let base = text_style(child(space, "txPr"), &default_text, colors);
    let style = child(space, "style")
        .or_else(|| path(space, &["AlternateContent", "Fallback", "style"]))
        .or_else(|| path(space, &["AlternateContent", "Choice", "style"]))
        .and_then(|n| attr(n, "val"))
        .and_then(|v| {
            let num = v.trim().parse::<u32>().ok()?;
            Some(if num > 100 { num - 100 } else { num })
        });
    let mono_accent = style
        .filter(|s| (1..=48).contains(s) && (2..=7).contains(&((s - 1) % 8)))
        .map(|s| ((s - 1) % 8 - 1) as usize);

    let mut groups = Vec::new();
    let mut series_index = 0usize;
    for node in elements(plot) {
        let kind = match node.tag_name().name() {
            "barChart" | "bar3DChart" => {
                let grouping = grouping(node);
                let stacked = grouping != Grouping::Standard;
                ChartType::Bar {
                    horizontal: val(node, "barDir") == Some("bar"),
                    gap_width: val_f64(node, "gapWidth").unwrap_or(150.0).max(0.0) as u32,
                    overlap: val_f64(node, "overlap").unwrap_or(if stacked { 100.0 } else { 0.0 }) as i32,
                }
            }
            "ofPieChart" => ChartType::OfPie { kind: match val(node, "ofPieType") {
                Some("pie") => Some(OfPieKind::Pie),
                Some("bar") => Some(OfPieKind::Bar),
                _ => None,
            } },
            "radarChart" => ChartType::Radar { style: match val(node, "radarStyle") {
                Some("standard") => Some(RadarStyle::Standard),
                Some("marker") => Some(RadarStyle::Marker),
                Some("filled") => Some(RadarStyle::Filled),
                // Original XML preserves absence, val-less and unknown forms.
                _ => None,
            } },
            "lineChart" | "line3DChart" => ChartType::Line,
            "areaChart" | "area3DChart" => ChartType::Area,
            "pieChart" | "pie3DChart" => ChartType::Pie { first_angle: val_f64(node, "firstSliceAng").unwrap_or(0.0) as u32 },
            "doughnutChart" => ChartType::Doughnut {
                first_angle: val_f64(node, "firstSliceAng").unwrap_or(0.0) as u32,
                hole: val_f64(node, "holeSize").unwrap_or(50.0).clamp(0.0, 90.0) as u32,
            },
            "scatterChart" => ChartType::Scatter { smooth: matches!(val(node, "scatterStyle"), Some("smooth" | "smoothMarker")) },
            "bubbleChart" => ChartType::Bubble { scale: val_f64(node, "bubbleScale").unwrap_or(100.0).clamp(0.0, 300.0) as u32 },
            "stockChart" => ChartType::Stock {
                up_down_bars: child(node, "upDownBars").is_some(),
                hi_low_lines: child(node, "hiLowLines").is_some(),
                drop_lines: child(node, "dropLines").is_some(),
            },
            "surfaceChart" => ChartType::Surface {
                is_3d: false,
                wireframe: val_bool(node, "wireframe").unwrap_or(false),
            },
            "surface3DChart" => ChartType::Surface {
                is_3d: true,
                wireframe: val_bool(node, "wireframe").unwrap_or(false),
            },
            _ => continue,
        };
        // Colours by point apply to a single series (a pie's always).
        let single = children(node, "ser").count() == 1 || matches!(kind, ChartType::Pie { .. } | ChartType::Doughnut { .. } | ChartType::OfPie { .. });
        let vary = single && if node.has_tag_name("ofPieChart") {
            // Retention-only OfPie must not interpret an invalid Boolean as true.
            child(node, "varyColors").and_then(|n| attr_bool(n, "val").or_else(|| attr(n, "val").is_none().then_some(true))).unwrap_or(false)
        } else { val_bool(node, "varyColors").unwrap_or(false) };
        // A scatter's series are lines with markers unless its style says
        // markers only (`marker`: no line unless the series has one).
        let markers_only = node.has_tag_name("scatterChart") && matches!(val(node, "scatterStyle"), Some("marker" | "none"));
        let line_like = matches!(kind, ChartType::Line) || (matches!(kind, ChartType::Scatter { .. }) && !markers_only);
        // Retained-only groups must not advance supported groups' palette/name
        // fallback indices (these groups used to be skipped by this reader).
        let retained_only = matches!(kind, ChartType::Radar { .. } | ChartType::OfPie { .. });
        let mut retained_index = 0;
        let mut last_categories = None;
        let series = children(node, "ser")
            .map(|ser| {
                let index = val_f64(ser, "idx").map(|v| v as usize).unwrap_or(if retained_only { retained_index } else { series_index });
                retained_index += 1;
                if !retained_only { series_index += 1; }
                let mut series = read_series(ser, index, line_like, vary, &base, colors, mono_accent, &mut last_categories);
                // Excel for Mac draws no marker for a series of an unformatted
                // chart whose `c:marker` names a symbol but gives it no `spPr`
                // (measured on a user's private workbook: four line
                // charts of 69 points, the truth PDF has plain lines).
                if legacy && legacy_border && excel_wrote && ser_marker_unformatted(ser) {
                    if let Some(m) = series.marker.as_mut() {
                        m.symbol = MarkerSymbol::None;
                    }
                }
                if matches!(kind, ChartType::Line) && series.marker.is_none() && val_bool(node, "marker").unwrap_or(false) {
                    series.marker = Some(auto_marker(index, colors, mono_accent));
                }
                let scatter = matches!(kind, ChartType::Scatter { .. } | ChartType::Bubble { .. });
                if scatter {
                    // The x values (text ones count 1, 2, 3, …), and bubbles' sizes.
                    let numbers = |name: &str| -> Vec<Option<Number>> {
                        child(ser, name)
                            .map(cache_points)
                            .map(|(v, _)| v.into_iter().map(|v| v.and_then(|t| t.trim().parse().ok()).map(Number)).collect())
                            .unwrap_or_default()
                    };
                    let x = numbers("xVal");
                    series.x_values = if x.iter().all(Option::is_some) { x } else { Vec::new() };
                    series.bubble_sizes = numbers("bubbleSize");
                    series.bubble_3d = matches!(kind, ChartType::Bubble { .. }) && child(ser, "bubble3D").is_some_and(|n| attr_bool(n, "val").unwrap_or(true));
                    // Scatter markers: the series' own, else the automatic
                    // ones (marker-only styles, and lines with markers).
                    let style = val(node, "scatterStyle").unwrap_or("marker");
                    if series.marker.is_none() && matches!(kind, ChartType::Scatter { .. }) && !matches!(style, "line" | "smooth" | "none") {
                        series.marker = Some(auto_marker(index, colors, mono_accent));
                    }
                    if matches!(kind, ChartType::Bubble { .. }) {
                        series.stroke = None;
                    }
                    if markers_only && child(ser, "spPr").and_then(|p| child(p, "ln")).is_none() {
                        series.stroke = None;
                    }
                }
                // A series without a name is called by its number, in the
                // chart's language ("系列1" in Japanese).
                if series.name.is_empty() && child(ser, "tx").is_none() {
                    let number = val_f64(ser, "idx").unwrap_or(index as f64) as usize + 1;
                    let japanese = val(space, "lang").is_some_and(|l| l.starts_with("ja"));
                    series.name = if japanese { format!("系列{number}") } else { format!("Series{number}") };
                }
                series
            })
            .collect();
        let axes = children(node, "axId").filter(|a| a.tag_name().namespace() == Some(CHART_NS)).filter_map(|a| unqualified_val(a).and_then(|v| v.parse::<u32>().ok())).collect();
        let of_pie = if node.has_tag_name("ofPieChart") {
            Some(OfPieSettings {
                split: match val(node, "splitType") {
                    Some("cust") => Some(OfPieSplit::Custom),
                    Some("percent") => Some(OfPieSplit::Percent),
                    Some("pos") => Some(OfPieSplit::Position),
                    Some("val") => Some(OfPieSplit::Value),
                    _ => None,
                },
                split_position: val(node, "splitPos").and_then(|v| v.parse().ok()).map(Number),
                second_size: val(node, "secondPieSize").and_then(|v| v.parse().ok()),
                gap_width: val(node, "gapWidth").and_then(|v| v.parse().ok()),
                vary_colors: child(node, "varyColors").and_then(|n| attr_bool(n, "val").or_else(|| attr(n, "val").is_none().then_some(true))),
                custom_points: child(node, "custSplit").map(|n| children(n, "secondPiePt").map(|p| attr(p, "val").and_then(|v| v.parse().ok())).collect()),
                connector: child(node, "serLines").map(|n| path(n, &["spPr", "ln"]).and_then(|ln| stroke(ln, colors))),
            })
        } else if matches!(node.tag_name().name(), "barChart" | "bar3DChart") {
            child(node, "serLines").map(|n| OfPieSettings {
                connector: Some(path(n, &["spPr", "ln"]).and_then(|ln| stroke(ln, colors))),
                ..OfPieSettings::default()
            })
        } else {
            None
        };
        let three_d = node.tag_name().name().ends_with("3DChart") && !node.has_tag_name("surface3DChart");
        let three_d = three_d.then(|| {
            let view = child(chart, "view3D");
            let get = |name: &str| view.and_then(|v| val_f64(v, name));
            ibis_doc_model::View3D {
                rot_x: get("rotX").unwrap_or(15.0) as i32,
                rot_y: get("rotY").unwrap_or(20.0) as i32,
                right_angle_axes: view.and_then(|v| val_bool(v, "rAngAx")).unwrap_or(true),
                depth_percent: get("depthPercent").unwrap_or(100.0).clamp(20.0, 2000.0) as u32,
                height_percent: get("hPercent").map(|v| v.clamp(5.0, 500.0) as u32),
                series_in_depth: val(node, "grouping") == Some("standard"),
                gap_depth: val_f64(node, "gapDepth").map_or(150, |v| v.clamp(0.0, 500.0) as u32),
            }
        });
        groups.push(ChartGroup { kind, of_pie, grouping: grouping(node), series, axes, three_d });
    }
    if groups.is_empty() {
        return None;
    }

    let mut axes: Vec<Axis> = elements(plot)
        .filter(|n| n.tag_name().namespace() == Some(CHART_NS) && matches!(n.tag_name().name(), "catAx" | "valAx" | "dateAx" | "serAx"))
        .map(|n| read_axis(n, &base, colors, legacy, legacy_border && excel_wrote, powerpoint_styled))
        .collect();
    // A category axis of several levels (`c:multiLvlStrRef`): the outer levels, over the labels of the innermost.
    if let Some(axis) = axes.iter_mut().find(|a| a.kind == AxisKind::Category && !a.deleted) {
        axis.category_levels = outer_levels(plot);
    }

    // A category axis with a minimum or maximum (1-based category numbers)
    // shows only those categories.
    for axis in elements(plot).filter(|n| n.has_tag_name((CHART_NS, "catAx"))) {
        let scaling = child(axis, "scaling");
        let min = scaling.and_then(|s| val_f64(s, "min")).map(|v| (v.max(1.0) as usize) - 1);
        let max = scaling.and_then(|s| val_f64(s, "max")).map(|v| v.max(1.0) as usize);
        if min.is_none() && max.is_none() {
            continue;
        }
        // Groups on a deleted secondary category axis follow the shown one.
        let id = axis_val(axis, "axId").and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0) as u32;
        let hidden: Vec<u32> = elements(plot)
            .filter(|n| n.has_tag_name((CHART_NS, "catAx")) && val_bool(*n, "delete").unwrap_or(false))
            .filter_map(|n| axis_val(n, "axId").and_then(|v| v.trim().parse::<f64>().ok()).map(|v| v as u32))
            .collect();
        for group in groups.iter_mut().filter(|g| !matches!(g.kind, ChartType::Radar { .. } | ChartType::OfPie { .. }) && (g.axes.contains(&id) || g.axes.iter().any(|a| hidden.contains(a)))) {
            for series in &mut group.series {
                let end = max.unwrap_or(usize::MAX);
                let start = min.unwrap_or(0);
                let cut = |len: usize| (start.min(len), end.min(len).max(start.min(len)));
                let (a, b) = cut(series.values.len());
                series.values = series.values[a..b].to_vec();
                let (a, b) = cut(series.categories.len());
                series.categories = series.categories[a..b].to_vec();
                series.point_fills = series.point_fills.iter().filter(|(i, _)| *i >= start).map(|(i, f)| (i - start, f.clone())).collect();
                if let Some(labels) = &mut series.labels {
                    labels.points = labels.points.iter().filter(|p| p.index >= start).cloned().map(|mut p| {
                        p.index -= start;
                        p
                    }).collect();
                }
            }
        }
    }

    // A date axis places categories by their dates: the points without a
    // date (rows past the data) and those outside its minimum and maximum
    // are not shown.
    let deleted_dates: Vec<u32> = elements(plot)
        .filter(|n| n.has_tag_name((CHART_NS, "dateAx")) && val_bool(*n, "delete").unwrap_or(false))
        .filter_map(|n| axis_val(n, "axId").and_then(|v| v.trim().parse::<f64>().ok()).map(|v| v as u32))
        .collect();
    let shown_date = elements(plot).any(|n| n.has_tag_name((CHART_NS, "dateAx")) && !val_bool(n, "delete").unwrap_or(false));
    for axis in elements(plot).filter(|n| n.has_tag_name((CHART_NS, "dateAx"))) {
        let id = axis_val(axis, "axId").and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0) as u32;
        // Groups on a deleted secondary date axis follow the shown one.
        if shown_date && deleted_dates.contains(&id) {
            continue;
        }
        let on_axis = |g: &ChartGroup| !matches!(g.kind, ChartType::Radar { .. } | ChartType::OfPie { .. }) && (g.axes.contains(&id) || (shown_date && g.axes.iter().any(|a| deleted_dates.contains(a))));
        let scaling = child(axis, "scaling");
        let (min, max) = (scaling.and_then(|s| val_f64(s, "min")), scaling.and_then(|s| val_f64(s, "max")));
        let dates: Option<Vec<Option<f64>>> = elements(plot)
            .filter(|n| children(*n, "axId").filter(|a| a.tag_name().namespace() == Some(CHART_NS)).any(|a| unqualified_val(a).and_then(|v| v.parse::<f64>().ok()) == Some(id as f64)))
            .flat_map(|n| children(n, "ser"))
            .find_map(|ser| child(ser, "cat"))
            .map(|cat| cache_points(cat).0.into_iter().map(|p| p.and_then(|v| v.trim().parse::<f64>().ok())).collect());
        let Some(dates) = dates else { continue };
        let keep: Vec<bool> =
            dates.iter().map(|d| d.is_some_and(|d| min.is_none_or(|m| d >= m) && max.is_none_or(|m| d <= m))).collect();
        // The time scale, with the dates of the points shown.
        let unit = |v: Option<&str>| match v {
            Some("months") => TimeUnit::Months,
            Some("years") => TimeUnit::Years,
            _ => TimeUnit::Days,
        };
        if let Some(a) = axes.iter_mut().find(|a| a.id == id) {
            let base = unit(val(axis, "baseTimeUnit"));
            let is_auto = val_bool(axis, "auto").unwrap_or(false);
            let major = if is_auto {
                None
            } else {
                val_f64(axis, "majorUnit").map(|u| (Number(u), val(axis, "majorTimeUnit").map_or(base, |m| unit(Some(m)))))
            };
            let shown = dates.iter().zip(&keep).filter(|(_, k)| **k).map(|(d, _)| d.map(Number)).collect();
            a.time = Some(TimeScale { base, major, dates: shown });
            // Japanese Excel shows the built-in short date (m/d/yy) year
            // first.
            if matches!(a.format.as_str(), "m/d/yy" | "m/d/yyyy" | "mm-dd-yy") && val(space, "lang").is_some_and(|l| l.starts_with("ja")) {
                a.format = "yyyy/m/d".into();
            }
            if a.format == "General" {
                // Linked to the source: the cached dates' format.
                if let Some(code) = elements(plot)
                    .flat_map(|n| children(n, "ser"))
                    .find_map(|ser| child(ser, "cat"))
                    .and_then(|c| c.descendants().find(|n| n.is_element() && n.tag_name().name() == "formatCode"))
                    .and_then(|n| n.text())
                {
                    a.format = code.to_string();
                }
            }
        }
        // The axis's own number format, unless it follows the data's.
        let format = child(axis, "numFmt")
            .filter(|f| !attr_bool(*f, "sourceLinked").unwrap_or(false))
            .and_then(|f| attr(f, "formatCode"))
            .map(str::to_string);
        if let Some(code) = &format {
            let labels: Vec<String> = dates.iter().map(|d| d.map(|d| ibis_doc_model::format_number(d, code)).unwrap_or_default()).collect();
            for group in groups.iter_mut().filter(|g| on_axis(g)) {
                for series in &mut group.series {
                    if !series.categories.is_empty() {
                        series.categories = labels.clone();
                    }
                }
            }
        }
        if keep.iter().all(|k| *k) {
            continue;
        }
        let kept = |i: usize| keep.get(i).copied().unwrap_or(false);
        // New indices of the points kept.
        let renumber: Vec<Option<usize>> = {
            let mut next = 0;
            (0..keep.len()).map(|i| kept(i).then(|| { next += 1; next - 1 })).collect()
        };
        let new_index = |i: usize| renumber.get(i).copied().flatten();
        for group in groups.iter_mut().filter(|g| on_axis(g)) {
            for series in &mut group.series {
                series.values = series.values.iter().enumerate().filter(|(i, _)| kept(*i)).map(|(_, v)| *v).collect();
                series.categories = series.categories.iter().enumerate().filter(|(i, _)| kept(*i)).map(|(_, c)| c.clone()).collect();
                series.point_fills = series.point_fills.iter().filter_map(|(i, f)| Some((new_index(*i)?, f.clone()))).collect();
                series.point_strokes = series.point_strokes.iter().filter_map(|(i, f)| Some((new_index(*i)?, f.clone()))).collect();
                series.point_explosions = series.point_explosions.iter().filter_map(|(i, e)| Some((new_index(*i)?, *e))).collect();
                if let Some(labels) = &mut series.labels {
                    labels.points = labels
                        .points
                        .iter()
                        .filter_map(|p| {
                            let mut p = p.clone();
                            p.index = new_index(p.index)?;
                            Some(p)
                        })
                        .collect();
                }
            }
        }
    }

    let legend = elements(chart).find(|n| n.has_tag_name((CHART_NS, "legend"))).map(|l| Legend {
        source_position: match axis_val(l, "legendPos") {
            Some("b") => Some(LegendPosition::Bottom),
            Some("l") => Some(LegendPosition::Left),
            Some("r") => Some(LegendPosition::Right),
            Some("t") => Some(LegendPosition::Top),
            Some("tr") => Some(LegendPosition::TopRight),
            _ => None,
        },
        side: side(axis_val(l, "legendPos").unwrap_or("r")),
        // A present Boolean element without val follows the existing true default;
        // invalid values stay unsupported, not silently converted to true.
        overlay: child(l, "overlay").and_then(|n| attr_bool(n, "val").or_else(|| attr(n, "val").is_none().then_some(true))),
        layout: path(l, &["layout", "manualLayout"]).and_then(manual_rect),
        text: text_style(child(l, "txPr"), &base, colors),
        hidden: children(l, "legendEntry")
            .filter(|e| val_bool(*e, "delete").unwrap_or(false))
            .filter_map(|e| val_f64(e, "idx").map(|v| v as usize))
            .collect(),
    });

    // The title: its own text, or the only series' name.
    let title = child(chart, "title").filter(|_| !val_bool(chart, "autoTitleDeleted").unwrap_or(false)).and_then(|t| {
        let title_default = if legacy { ChartText { size: 1800, bold: legacy_border || excel_wrote, ..base.clone() } } else { ChartText { size: 1400, ..base.clone() } };
        let mut style = text_style(child(t, "txPr"), &title_default, colors);
        let rich = path(t, &["tx", "rich"]);
        if let Some(def) = rich.and_then(|r| r.descendants().find(|n| n.is_element() && n.tag_name().name() == "defRPr")) {
            style = run_style(def, &style, colors);
        }
        if let Some(rpr) = rich.and_then(|r| r.descendants().find(|n| n.is_element() && n.tag_name().name() == "rPr")) {
            style = run_style(rpr, &style, colors);
        }
        // A title of text of its own, with no size stated, is in the body's
        // size in the old defaults (Excel draws openpyxl's titles so: 10 pt
        // bold; the 18 pt is of an automatic title).
        // A chart Excel itself wrote keeps its automatic 18 pt (measured on
        // a user's private workbook: an 18 pt bold title with an empty `rPr`).
        if legacy && !excel_wrote && rich.is_some_and(|r| !r.descendants().any(|n| n.is_element() && matches!(n.tag_name().name(), "defRPr" | "rPr") && attr(n, "sz").is_some())) && child(t, "txPr").is_none() {
            style.size = base.size;
        }
        let text = match rich {
            Some(r) => children(r, "p")
                .map(|p| p.descendants().filter(|n| n.is_element() && n.tag_name().name() == "t").filter_map(|n| n.text()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n"),
            None => {
                if let Some(tx) = child(t, "tx") {
                    cache_points(tx).0.into_iter().flatten().collect::<Vec<_>>().join("\n")
                } else {
                    let series: Vec<&Series> = groups.iter().flat_map(|g| &g.series).collect();
                    if series.len() == 1 && !series[0].name.is_empty() {
                        series[0].name.clone()
                    } else {
                        "Chart Title".to_string()
                    }
                }
            }
        };
        let at = path(t, &["layout", "manualLayout"]).and_then(|m| Some((fraction(val_f64(m, "x")?), fraction(val_f64(m, "y")?))));
        (!text.trim().is_empty()).then_some(ChartTitle { text, style, at })
    });

    let plot_area = path(plot, &["layout", "manualLayout"])
        .filter(|m| val(*m, "layoutTarget") == Some("inner"))
        .and_then(manual_rect);
    let (background, border) = shape_props(child(space, "spPr"), colors);
    let border = match (legacy, border) {
        (true, None) if legacy_border => Some(Some(Stroke::solid(Color::opaque(ibis_doc_model::Rgb(0x868686)), 9_525))),
        (_, border) => border,
    };
    // A chart area with no format of its own is white on a sheet (and in a document), clear in a slide (PowerPoint's PDF of a 3-D chart over a
    // shape: the shape shows through; PowerPoint 16.0.20527.20088).
    let background = background.or((legacy && legacy_border).then_some(Fill::Solid(Color::WHITE)));

    let (plot_background, plot_border) = shape_props(child(plot, "spPr"), colors);
    let plot_border = plot_border.flatten();
    // A 3-D chart's walls (`c:backWall`, else `c:sideWall`) are its plot's ground.
    let plot_background = plot_background.or_else(|| {
        ["backWall", "sideWall"].into_iter().find_map(|wall| child(chart, wall).and_then(|w| shape_props(child(w, "spPr"), colors).0))
    });
    let disp_blanks_as = child(chart, "dispBlanksAs").and_then(|n| attr(n, "val")).and_then(|v| match v {
        "span" => Some(ibis_doc_model::DispBlanksAs::Span),
        "zero" => Some(ibis_doc_model::DispBlanksAs::Zero),
        "gap" => Some(ibis_doc_model::DispBlanksAs::Gap),
        _ => None,
    });
    let plot_vis_only = child(chart, "plotVisOnly").and_then(|n| attr_bool(n, "val")).unwrap_or(true);
    let data_table = child(plot, "dTable").map(|dt| ChartDataTable {
        show_horz_border: val_bool(dt, "showHorzBorder").unwrap_or(true),
        show_vert_border: val_bool(dt, "showVertBorder").unwrap_or(true),
        show_outline: val_bool(dt, "showOutline").unwrap_or(true),
        show_keys: val_bool(dt, "showKeys").unwrap_or(false),
    });
    let pivot_source = child(space, "pivotSource")
        .or_else(|| child(chart, "pivotSource"))
        .and_then(|ps| child(ps, "name"))
        .and_then(|n| n.text().map(|t| t.to_string()));
    let groups_len: usize = groups.iter().map(|g| g.series.len()).sum();
    // A trendline with no format of its own: a thin solid line in the text colour (the PDF of a chart that names style 2: 0.75pt, black).
    if powerpoint_styled {
        let text = colors.scheme("tx1").unwrap_or(ibis_doc_model::Rgb::BLACK);
        for series in groups.iter_mut().flat_map(|g| g.series.iter_mut()) {
            if let Some(trendline) = series.trendline.as_mut().filter(|t| t.stroke.is_none()) {
                trendline.stroke = Some(Stroke::solid(Color::opaque(text), 9_525));
            }
        }
    }
    Some(Chart {
        groups,
        axes,
        legend,
        title,
        plot_area,
        background: background.unwrap_or_default(),
        border: border.flatten(),
        plot_background: plot_background.unwrap_or_default(),
        plot_border,
        disp_blanks_as,
        plot_vis_only,
        data_table,
        style,
        pivot_source,
        // An absent element is a square chart area (the schema's default is
        // true, but Excel writes it when it means round).
        rounded_corners: val_bool(space, "roundedCorners").unwrap_or(false),
        title_overlay: child(chart, "title").and_then(|t| val_bool(t, "overlay")).unwrap_or(false),
        ex: None,
        ex_format: None,
        excel_layout: false,
        word_layout: false,
        cell_refs: cell_refs(plot, &groups_len),
        ex_source: None,
    })
}

/// The references (`c:f`) of each series of the plot area, in the order the groups hold them; none when they are not
/// as many as the series read.
fn cell_refs(plot: Node, series_read: &usize) -> Vec<ibis_doc_model::SeriesCells> {
    let formula = |node: Option<Node>| node.and_then(|n| n.descendants().find(|d| d.is_element() && d.tag_name().name() == "f")).and_then(|f| f.text()).map(|t| t.to_string()).filter(|t| !t.is_empty());
    let refs: Vec<ibis_doc_model::SeriesCells> = elements(plot)
        .filter(|g| crate::save::pptx_chart::GROUP_ELEMENTS.contains(&g.tag_name().name()))
        .flat_map(|g| elements(g).filter(|s| s.tag_name().name() == "ser").collect::<Vec<_>>())
        .map(|ser| ibis_doc_model::SeriesCells {
            name: formula(child(ser, "tx")),
            categories: formula(child(ser, "cat").or_else(|| child(ser, "xVal"))),
            values: formula(child(ser, "val").or_else(|| child(ser, "yVal"))),
            sizes: formula(child(ser, "bubbleSize")),
        })
        .collect();
    if refs.len() == *series_read && refs.iter().any(|r| r.values.is_some() || r.categories.is_some()) { refs } else { Vec::new() }
}

fn grouping(node: Node) -> Grouping {
    match val(node, "grouping") {
        Some("stacked") => Grouping::Stacked,
        Some("percentStacked") => Grouping::PercentStacked,
        _ => Grouping::Standard,
    }
}

pub(crate) fn side(value: &str) -> Side {
    match value {
        "l" => Side::Left,
        "r" => Side::Right,
        "t" | "tr" => Side::Top,
        _ => Side::Bottom,
    }
}

fn fraction(v: f64) -> i32 {
    (v * 100_000.0).round() as i32
}

fn manual_rect(layout: Node) -> Option<FractionRect> {
    Some(FractionRect {
        x: fraction(val_f64(layout, "x")?),
        y: fraction(val_f64(layout, "y")?),
        width: fraction(val_f64(layout, "w")?),
        height: fraction(val_f64(layout, "h")?),
    })
}

/// Fill and outline from `c:spPr`. The outer `None`s mean "not stated".
pub(crate) fn shape_props(sp_pr: Option<Node>, colors: &Colors) -> (Option<Fill>, Option<Option<Stroke>>) {
    let Some(sp_pr) = sp_pr else { return (None, None) };
    let fill = elements(sp_pr).find_map(|n| fill(n, colors));
    let stroke = child(sp_pr, "ln").map(|ln| stroke(ln, colors));
    (fill, stroke)
}

fn fill(node: Node, colors: &Colors) -> Option<Fill> {
    match node.tag_name().name() {
        "noFill" => Some(Fill::None),
        "solidFill" => colors.in_parent(node, None).map(Fill::Solid),
        "gradFill" => {
            let mut stops: Vec<GradientStop> = child(node, "gsLst")
                .into_iter()
                .flat_map(|l| children(l, "gs"))
                .filter_map(|gs| {
                    Some(GradientStop {
                        position: attr_i64(gs, "pos").unwrap_or(0).clamp(0, 100_000) as u32,
                        color: colors.in_parent(gs, None)?,
                    })
                })
                .collect();
            stops.sort_by_key(|s| s.position);
            let lin = child(node, "lin");
            let angle = lin.and_then(|l| attr_i64(l, "ang")).unwrap_or(5_400_000) as i32;
            let scaled = lin.and_then(|l| attr(l, "scaled")).map_or(false, |v| v == "1" || v == "true");
            (!stops.is_empty()).then_some(Fill::Linear { angle, scaled, stops })
        }
        "pattFill" => {
            let fg = child(node, "fgClr").and_then(|c| colors.in_parent(c, None)).unwrap_or(Color::BLACK);
            let bg = child(node, "bgClr").and_then(|c| colors.in_parent(c, None)).unwrap_or(Color::WHITE);
            let preset = attr(node, "prst").unwrap_or("pct5");
            Some(match ibis_doc_model::pattern_tile(preset) {
                Some(_) => Fill::Pattern { preset: preset.to_string(), foreground: fg, background: bg },
                None => Fill::Solid(fg),
            })
        }
        _ => None,
    }
}

/// `a:ln`; `None` for "no line".
fn stroke(ln: Node, colors: &Colors) -> Option<Stroke> {
    let color = match elements(ln).find_map(|n| fill(n, colors)) {
        Some(Fill::Solid(c)) => c,
        Some(Fill::Linear { stops, .. }) => stops.first()?.color,
        Some(_) => return None,
        // An outline without a fill stated uses the text colour.
        None => Color::opaque(ibis_doc_model::Rgb(0x595959)),
    };
    let end = |name| {
        let e = child(ln, name);
        LineEnd::parse(e.and_then(|e| attr(e, "type")), e.and_then(|e| attr(e, "w")), e.and_then(|e| attr(e, "len")))
    };
    let dash = val(ln, "prstDash").and_then(ibis_doc_model::LineDash::parse).unwrap_or_default();
    Some(Stroke {
        color,
        width: attr_i64(ln, "w").unwrap_or(Stroke::DEFAULT_WIDTH),
        dashed: dash != ibis_doc_model::LineDash::Solid,
        head: end("headEnd"),
        tail: end("tailEnd"),
        dash,
        cap: ibis_doc_model::LineCap::Flat,
        join: ibis_doc_model::LineJoin::Miter,
        compound: ibis_doc_model::LineCompound::Single,
        alignment: ibis_doc_model::LineAlignment::Center,
        fill: None,
    })
}

/// Text formatting from a `c:txPr` (its first paragraph's `a:defRPr`), over `base`.
fn text_style(tx_pr: Option<Node>, base: &ChartText, colors: &Colors) -> ChartText {
    let Some(def) = tx_pr.and_then(|t| path(t, &["p", "pPr", "defRPr"])) else { return base.clone() };
    run_style(def, base, colors)
}

fn run_style(rpr: Node, base: &ChartText, colors: &Colors) -> ChartText {
    let mut run = RunProps::default();
    run.merge(rpr, colors);
    let hidden = child(rpr, "noFill").is_some();
    ChartText {
        size: run.size.unwrap_or(base.size),
        bold: run.bold.unwrap_or(base.bold),
        color: if hidden { None } else { run.color.or(base.color) },
        // Each script's typeface falls back to the inherited one, not to the
        // other script's.
        font: run.latin.clone().or_else(|| base.font.clone()),
        east_asian_font: run.east_asian.clone().or_else(|| base.east_asian_font.clone()),
    }
}

/// Category labels or values of a `c:cat` / `c:val` / `c:tx`, by point index.
/// The outer levels of the first series' categories of several levels (`c:multiLvlStrCache`: its first `c:lvl` is
/// the innermost, which is what the series' categories are), nearest first: each level's groups as (first
/// category, how many categories, name) for the groups that start at a named point and run to the next one.
fn outer_levels(plot: Node) -> Vec<Vec<(usize, usize, String)>> {
    let Some(cache) = plot.descendants().find(|n| n.is_element() && n.tag_name().name() == "multiLvlStrCache") else { return Vec::new() };
    let total = val_f64(cache, "ptCount").unwrap_or(0.0).max(0.0) as usize;
    children(cache, "lvl")
        .skip(1)
        .map(|lvl| {
            let mut starts: Vec<(usize, String)> = children(lvl, "pt")
                .filter_map(|pt| Some((attr_i64(pt, "idx")?.max(0) as usize, child(pt, "v")?.text()?.to_string())))
                .collect();
            starts.sort_by_key(|(i, _)| *i);
            (0..starts.len())
                .map(|k| {
                    let end = starts.get(k + 1).map_or(total, |next| next.0);
                    (starts[k].0, end.saturating_sub(starts[k].0).max(1), starts[k].1.clone())
                })
                .collect()
        })
        .collect()
}

fn cache_points(node: Node) -> (Vec<Option<String>>, Option<String>) {
    let cache = node.descendants().find(|n| {
        n.is_element() && matches!(n.tag_name().name(), "strCache" | "numCache" | "strLit" | "numLit" | "lvl")
    });
    let Some(cache) = cache else {
        // A literal value, as in `c:tx/c:v`.
        return (vec![child(node, "v").and_then(|v| v.text()).map(str::to_string)], None);
    };
    let count = val_f64(cache, "ptCount").unwrap_or(0.0).max(0.0) as usize;
    let mut points = vec![None; count];
    for pt in children(cache, "pt") {
        let idx = attr_i64(pt, "idx").unwrap_or(0).max(0) as usize;
        if idx >= points.len() {
            points.resize(idx + 1, None);
        }
        points[idx] = child(pt, "v").and_then(|v| v.text()).map(str::to_string);
    }
    let format = child(cache, "formatCode").and_then(|f| f.text()).map(str::to_string);
    (points, format)
}

fn default_color(index: usize, colors: &Colors, mono_accent: Option<usize>) -> Color {
    colors.named(&format!("accent{}", mono_accent.unwrap_or(index % 6 + 1)))
}

/// The colour of point `index` when the colours vary by point: the six
/// accents, then each again darker and lighter by turns (Excel's: the second
/// round 76 % of the colour, the third mixed with white at 77 %, then 53 %,
/// 54 %, …).
fn varied_color(index: usize, colors: &Colors) -> Color {
    let base = default_color(index, colors, None);
    let round = index / 6;
    if round == 0 {
        return base;
    }
    let step = (round + 1) / 2;
    let rgb = base.rgb;
    let map = |c: u8| -> u8 {
        if round % 2 == 1 {
            // Darker: shade.
            let f = (0.76f32).powi(step as i32).max(0.5);
            (c as f32 * f).round() as u8
        } else {
            // Lighter: tint.
            let t = (0.77f32).powi(step as i32).max(0.45);
            (255.0 - (255.0 - c as f32) * t).round() as u8
        }
    };
    Color { rgb: ibis_doc_model::Rgb(((map((rgb.0 >> 16) as u8) as u32) << 16) | ((map((rgb.0 >> 8) as u8) as u32) << 8) | map(rgb.0 as u8) as u32), ..base }
}

/// The categories of the series read last: its points and format code, and
/// the labels made of them (a chart's series share their categories, and
/// formatting thousands of dates for each takes long).
type LastCategories = Option<((Vec<Option<String>>, Option<String>, bool), Vec<String>)>;

fn read_series(ser: Node, index: usize, line_like: bool, vary: bool, base: &ChartText, colors: &Colors, mono_accent: Option<usize>, last: &mut LastCategories) -> Series {
    let name = child(ser, "tx").map(|t| cache_points(t).0.into_iter().flatten().collect::<Vec<_>>().join(" ")).unwrap_or_default();
    // Numeric categories (such as dates) are shown with their format code.
    let categories = child(ser, "cat")
        .map(|c| {
            let (points, format) = cache_points(c);
            let numeric = c.descendants().any(|n| n.is_element() && matches!(n.tag_name().name(), "numCache" | "numLit"));
            let key = (points, format, numeric);
            if let Some((k, labels)) = last.as_ref()
                && *k == key
            {
                return labels.clone();
            }
            let labels: Vec<String> = key
                .0
                .iter()
                .map(|p| match (numeric, p.as_deref().and_then(|v| v.trim().parse::<f64>().ok()), &key.1) {
                    (true, Some(v), Some(code)) => ibis_doc_model::format_number(v, code),
                    _ => p.clone().unwrap_or_default(),
                })
                .collect();
            *last = Some((key, labels.clone()));
            labels
        })
        .unwrap_or_default();
    let (values, format) = child(ser, "val").or_else(|| child(ser, "yVal")).map(cache_points).unwrap_or_default();
    let values = values.into_iter().map(|v| v.and_then(|t| t.trim().parse().ok()).map(Number)).collect::<Vec<_>>();

    let (fill, stroke_props) = shape_props(child(ser, "spPr"), colors);
    let accent = default_color(index, colors, mono_accent);
    let (fill, stroke) = if line_like {
        // A line series is drawn by its outline; 2.25 pt in the theme colour by default.
        // A line without a width stated is 2.25 pt, as without a line at all.
        let stroke = match stroke_props {
            Some(Some(mut stroke)) => {
                let ln = child(ser, "spPr").and_then(|p| child(p, "ln"));
                if ln.and_then(|ln| attr_i64(ln, "w")).is_none() {
                    stroke.width = 28_575;
                }
                // A line of no colour stated (`<a:ln><a:prstDash/></a:ln>`) is
                // the automatic one: the series' colour.
                if ln.is_some_and(|ln| !elements(ln).any(|n| matches!(n.tag_name().name(), "solidFill" | "gradFill" | "pattFill"))) {
                    stroke.color = accent;
                }
                Some(stroke)
            }
            Some(None) => None,
            None => Some(Stroke::solid(accent, 28_575)),
        };
        (Fill::None, stroke)
    } else {
        // An outline of no colour stated (`<a:ln><a:prstDash/></a:ln>`) is
        // the automatic one: none round a bar (Excel draws none).
        let automatic = child(ser, "spPr").and_then(|p| child(p, "ln")).is_some_and(|ln| !elements(ln).any(|n| matches!(n.tag_name().name(), "solidFill" | "noFill" | "gradFill" | "pattFill")));
        (fill.unwrap_or(Fill::Solid(accent)), if automatic { None } else { stroke_props.flatten() })
    };

    let mut point_strokes: Vec<(usize, Option<Stroke>)> = children(ser, "dPt")
        .filter_map(|p| Some((val_f64(p, "idx")? as usize, shape_props(child(p, "spPr"), colors).1?)))
        .collect();
    let explosion = val_f64(ser, "explosion").unwrap_or(0.0).clamp(0.0, 400.0) as u32;
    let point_explosions: Vec<(usize, u32)> = children(ser, "dPt")
        .filter_map(|p| Some((val_f64(p, "idx")? as usize, val_f64(p, "explosion")?.clamp(0.0, 400.0) as u32)))
        .collect();
    let mut point_fills: Vec<(usize, Fill)> = children(ser, "dPt")
        .filter_map(|p| {
            let idx = val_f64(p, "idx")? as usize;
            Some((idx, shape_props(child(p, "spPr"), colors).0?))
        })
        .collect();
    if vary {
        // Each point takes the next theme colour unless it has its own.
        for i in 0..values.len() {
            if !point_fills.iter().any(|(idx, _)| *idx == i) {
                point_fills.push((i, Fill::Solid(varied_color(i, colors))));
            }
        }
        // A line's segments take them too (the line leading to the point).
        if line_like && let Some(line) = &stroke {
            for i in 1..values.len() {
                let mut s = line.clone();
                s.color = varied_color(i, colors);
                point_strokes.push((i, Some(s)));
            }
        }
    }

    let marker = elements(ser).find(|n| n.has_tag_name((CHART_NS, "marker"))).map(|m| {
        let symbol = match axis_val(m, "symbol") {
            Some("none") => MarkerSymbol::None,
            Some("square") => MarkerSymbol::Square,
            Some("diamond") => MarkerSymbol::Diamond,
            Some("triangle") => MarkerSymbol::Triangle,
            _ => MarkerSymbol::Circle,
        };
        let fill = shape_props(child(m, "spPr"), colors).0.unwrap_or(Fill::Solid(accent));
        let source = MarkerSource {
            symbol: match axis_val(m, "symbol") {
                Some("auto") => Some(MarkerSourceSymbol::Auto),
                Some("circle") => Some(MarkerSourceSymbol::Circle),
                Some("dash") => Some(MarkerSourceSymbol::Dash),
                Some("diamond") => Some(MarkerSourceSymbol::Diamond),
                Some("dot") => Some(MarkerSourceSymbol::Dot),
                Some("none") => Some(MarkerSourceSymbol::None),
                Some("picture") => Some(MarkerSourceSymbol::Picture),
                Some("plus") => Some(MarkerSourceSymbol::Plus),
                Some("square") => Some(MarkerSourceSymbol::Square),
                Some("star") => Some(MarkerSourceSymbol::Star),
                Some("triangle") => Some(MarkerSourceSymbol::Triangle),
                Some("x") => Some(MarkerSourceSymbol::Cross),
                _ => None,
            },
            size: axis_val(m, "size").and_then(|v| v.parse().ok()),
        };
        Marker { source: Some(source), symbol, size: axis_val(m, "size").and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(5.0).clamp(2.0, 72.0) as u32, fill }
    });
    let cell_range = dlbl_range_cache(ser);
    let labels = child(ser, "dLbls").and_then(|d| read_labels_with_range(d, base, colors, &cell_range));
    let trendline = child(ser, "trendline").map(|t| {
        let kind = axis_val(t, "trendlineType")
            .and_then(TrendlineType::from_ooxml_str)
            .unwrap_or(TrendlineType::Linear);
        let order = axis_val(t, "order")
            .and_then(|v| v.parse().ok())
            .unwrap_or(2)
            .clamp(2, 6);
        let period = axis_val(t, "period")
            .and_then(|v| v.parse().ok())
            .unwrap_or(2)
            .max(2);
        let disp_eq = child(t, "dispEq")
            .and_then(|n| n.attribute("val"))
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false);
        let disp_r_sqr = child(t, "dispRSqr")
            .and_then(|n| n.attribute("val"))
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false);
        let stroke = shape_props(child(t, "spPr"), colors).1.flatten();
        Trendline {
            kind,
            order,
            period,
            disp_eq,
            disp_r_sqr,
            stroke,
        }
    });
    let error_bars = child(ser, "errBars").map(|e| {
        let direction = axis_val(e, "errDir")
            .and_then(ErrorBarDirection::from_ooxml_str)
            .unwrap_or(ErrorBarDirection::Y);
        let bar_type = axis_val(e, "errBarType")
            .and_then(ErrorBarType::from_ooxml_str)
            .unwrap_or(ErrorBarType::Both);
        let val_type = axis_val(e, "errValType")
            .and_then(ErrorBarValueType::from_ooxml_str)
            .unwrap_or(ErrorBarValueType::FixedValue);
        let no_end_cap = child(e, "noEndCap")
            .map(|n| attr_bool(n, "val").unwrap_or(true))
            .unwrap_or(false);
        let value = val_f64(e, "val").map(Number);
        let stroke = shape_props(child(e, "spPr"), colors).1.flatten();
        let numbers_of = |tag: &str| -> Vec<Option<Number>> {
            child(e, tag)
                .map(cache_points)
                .map(|(pts, _)| pts.into_iter().map(|p| p.and_then(|t| t.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).map(Number)).collect())
                .unwrap_or_default()
        };
        let plus = numbers_of("plus");
        let minus = numbers_of("minus");
        ErrorBars {
            direction,
            bar_type,
            val_type,
            no_end_cap,
            value,
            stroke,
            plus,
            minus,
        }
    });
    Series {
        name,
        categories,
        values,
        format: format.unwrap_or_else(|| "General".into()),
        fill,
        stroke,
        point_fills,
        point_strokes,
        explosion,
        point_explosions,
        marker,
        labels,
        trendline,
        error_bars,
        x_values: Vec::new(),
        bubble_sizes: Vec::new(),
        bubble_3d: false,
    }
}

/// The marker PowerPoint gives a series that names none (`c:marker` left
/// out): by the series' number, a diamond, a square, a triangle, then
/// circles, 7 pt, in the series' colour.
fn auto_marker(index: usize, colors: &Colors, mono_accent: Option<usize>) -> Marker {
    let symbol = match index % 4 {
        0 => MarkerSymbol::Diamond,
        1 => MarkerSymbol::Square,
        2 => MarkerSymbol::Triangle,
        _ => MarkerSymbol::Circle,
    };
    Marker { source: None, symbol, size: 7, fill: Fill::Solid(default_color(index, colors, mono_accent)) }
}

fn label_position(value: &str) -> LabelPosition {
    match value {
        "ctr" => LabelPosition::Center,
        "inEnd" => LabelPosition::InsideEnd,
        "inBase" => LabelPosition::InsideBase,
        "outEnd" => LabelPosition::OutsideEnd,
        "t" => LabelPosition::Above,
        "b" => LabelPosition::Below,
        "l" => LabelPosition::Left,
        "r" => LabelPosition::Right,
        _ => LabelPosition::BestFit,
    }
}

fn dlbl_range_cache(ser: Node) -> std::collections::HashMap<usize, String> {
    let Some(cache) = ser.descendants().find(|n| n.is_element() && n.tag_name().name() == "dlblRangeCache") else {
        return std::collections::HashMap::new();
    };
    children(cache, "pt")
        .filter_map(|pt| {
            let idx: usize = attr(pt, "idx")?.trim().parse().ok()?;
            let v = child(pt, "v").and_then(|v| v.text()).unwrap_or("").to_string();
            Some((idx, v))
        })
        .collect()
}

#[cfg(test)]
fn read_labels(node: Node, base: &ChartText, colors: &Colors) -> Option<DataLabels> {
    read_labels_with_range(node, base, colors, &std::collections::HashMap::new())
}

fn read_labels_with_range(node: Node, base: &ChartText, colors: &Colors, cell_range: &std::collections::HashMap<usize, String>) -> Option<DataLabels> {
    if val_bool(node, "delete").unwrap_or(false) {
        return None;
    }
    let style = text_style(child(node, "txPr"), base, colors);
    let (fill, border) = shape_props(child(node, "spPr"), colors);
    let fill = fill.unwrap_or_default();
    let border = border.flatten();
    let format = child(node, "numFmt")
        .filter(|f| !attr_bool(*f, "sourceLinked").unwrap_or(false))
        .and_then(|f| attr(f, "formatCode"))
        .map(str::to_string);
    let series_show_value = val_bool(node, "showVal").unwrap_or(false);
    let series_show_percent = val_bool(node, "showPercent").unwrap_or(false);
    let series_show_category = val_bool(node, "showCatName").unwrap_or(false);
    let series_show_series_name = val_bool(node, "showSerName").unwrap_or(false);
    let points = children(node, "dLbl")
        .filter_map(|d| {
            let index = val_f64(d, "idx")? as usize;
            if val_bool(d, "delete").unwrap_or(false) {
                return Some(PointLabel { index, deleted: true, ..Default::default() });
            }
            let offset = path(d, &["layout", "manualLayout"])
                .map(|m| (fraction(val_f64(m, "x").unwrap_or(0.0)), fraction(val_f64(m, "y").unwrap_or(0.0))))
                .unwrap_or((0, 0));
            let rich = path(d, &["tx", "rich"]);
            let text = rich.map(|r| rich_pieces(r, cell_range.get(&index).map(String::as_str)));
            let first_run = rich.and_then(|r| r.descendants().find(|n| n.is_element() && n.tag_name().name() == "rPr"));
            let point_style = first_run
                .map(|rpr| run_style(rpr, &text_style(child(d, "txPr"), &style, colors), colors))
                .or_else(|| child(d, "txPr").map(|t| text_style(Some(t), &style, colors)));
            let (point_fill, point_border) = shape_props(child(d, "spPr"), colors);
            let show_value = val_bool(d, "showVal");
            let show_percent = val_bool(d, "showPercent");
            let show_category = val_bool(d, "showCatName");
            let show_series_name = val_bool(d, "showSerName");
            // A point's label may be switched off with showVal = 0 and nothing else.
            let shows_anything = show_value.unwrap_or(series_show_value)
                || show_percent.unwrap_or(series_show_percent)
                || show_category.unwrap_or(series_show_category)
                || show_series_name.unwrap_or(series_show_series_name);
            Some(PointLabel {
                index,
                deleted: text.is_none() && !shows_anything,
                show_value,
                show_percent,
                show_category,
                show_series_name,
                offset,
                text,
                style: point_style,
                fill: point_fill,
                border: point_border,
                position: val(d, "dLblPos").map(label_position),
            })
        })
        .collect();
    Some(DataLabels {
        show_value: series_show_value,
        show_percent: series_show_percent,
        show_category: series_show_category,
        show_series_name: series_show_series_name,
        separator: child(node, "separator").and_then(|s| s.text()).map(str::to_string),
        position: val(node, "dLblPos").map(label_position),
        format,
        style,
        fill,
        border,
        points,
    })
}

/// The text of a rich data label, fields kept as fields.
fn rich_pieces(rich: Node, cell_range: Option<&str>) -> Vec<LabelPiece> {
    let mut pieces = Vec::new();
    for (i, p) in children(rich, "p").enumerate() {
        if i > 0 {
            pieces.push(LabelPiece::Text("\n".into()));
        }
        for n in elements(p) {
            match n.tag_name().name() {
                "r" => pieces.push(LabelPiece::Text(child(n, "t").and_then(|t| t.text()).unwrap_or("").to_string())),
                "fld" => pieces.push(match attr(n, "type") {
                    Some("VALUE") => LabelPiece::Value,
                    Some("PERCENTAGE") => LabelPiece::Percent,
                    Some("CATEGORYNAME") => LabelPiece::Category,
                    Some("SERIESNAME") => LabelPiece::SeriesName,
                    Some("CELLRANGE") => LabelPiece::Text(
                        cell_range
                            .map(str::to_string)
                            .unwrap_or_else(|| child(n, "t").and_then(|t| t.text()).unwrap_or("").to_string()),
                    ),
                    _ => LabelPiece::Text(child(n, "t").and_then(|t| t.text()).unwrap_or("").to_string()),
                }),
                "br" => pieces.push(LabelPiece::Text("\n".into())),
                _ => {}
            }
        }
    }
    pieces
}

/// The divisor of `c:dispUnits`.
fn display_unit(node: Node) -> Option<f64> {
    if let Some(custom) = val_f64(node, "custUnit") {
        return (custom > 0.0).then_some(custom);
    }
    Some(match val(node, "builtInUnit")? {
        "hundreds" => 1e2,
        "thousands" => 1e3,
        "tenThousands" => 1e4,
        "hundredThousands" => 1e5,
        "millions" => 1e6,
        "tenMillions" => 1e7,
        "hundredMillions" => 1e8,
        "billions" => 1e9,
        "trillions" => 1e12,
        _ => return None,
    })
}

/// Every how many categories a label is shown: `c:tickLblSkip`, or a date
/// axis's major unit counted in its base unit.
fn label_skip(node: Node) -> u32 {
    if let Some(skip) = val_f64(node, "tickLblSkip") {
        return skip.max(1.0) as u32;
    }
    if node.has_tag_name((CHART_NS, "dateAx")) {
        let base = val(node, "baseTimeUnit").unwrap_or("days");
        let major = val(node, "majorTimeUnit").unwrap_or(base);
        if let Some(unit) = val_f64(node, "majorUnit") {
            let factor = match (base, major) {
                (b, m) if b == m => 1.0,
                ("months", "years") => 12.0,
                ("days", "months") => 30.0,
                ("days", "years") => 365.0,
                _ => 1.0,
            };
            return (unit * factor).round().max(1.0) as u32;
        }
    }
    0
}

fn ser_marker_unformatted(ser: Node) -> bool {
    child(ser, "marker").is_some_and(|m| child(m, "spPr").is_none_or(|p| child(p, "solidFill").is_none() && child(p, "ln").and_then(|l| child(l, "solidFill")).is_none()))
}

/// The theme's text colour tinted 75% (`a:schemeClr tx1` with `a:tint 75000`): the lines (gridlines, axes, tick marks) of PowerPoint's chart style 2
/// (0x898989 on black text: the 0.537 gray of PowerPoint 16.0.20527.20088's PDF of a chart that names style 2).
fn styled_line_color(colors: &Colors) -> Color {
    let xml = r#"<a:schemeClr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" val="tx1"><a:tint val="75000"/></a:schemeClr>"#;
    roxmltree::Document::parse(xml).ok().and_then(|doc| colors.resolve(doc.root_element(), None)).unwrap_or(Color::opaque(ibis_doc_model::Rgb(0x898989)))
}

fn read_axis(node: Node, base: &ChartText, colors: &Colors, legacy: bool, legacy_border: bool, powerpoint_styled: bool) -> Axis {
    let scaling = child(node, "scaling");
    // Lines with no format: black in a chart of the old defaults.
    // Excel draws them grey (measured on the same file: gridlines and axis
    // line match the chart border's 0x868686).
    let plain = if legacy && legacy_border { ibis_doc_model::Rgb(0x868686) } else if legacy { ibis_doc_model::Rgb::BLACK } else { ibis_doc_model::Rgb(0xd9d9d9) };
    let styled_line = powerpoint_styled.then(|| styled_line_color(colors));
    let plain_color = styled_line.unwrap_or(Color::opaque(plain));
    let gridlines = child(node, "majorGridlines").map(|g| {
        path(g, &["spPr", "ln"]).map(|ln| stroke(ln, colors)).unwrap_or_else(|| Some(Stroke::solid(plain_color, 9_525)))
    });
    let minor_gridlines = child(node, "minorGridlines").map(|g| {
        path(g, &["spPr", "ln"])
            .map(|ln| stroke(ln, colors))
            .unwrap_or_else(|| Some(Stroke::solid(Color::opaque(ibis_doc_model::Rgb(0xf2f2f2)), 9_525)))
    });
    Axis {
        id: axis_val(node, "axId").and_then(|v| v.trim().parse::<f64>().ok()).unwrap_or(0.0) as u32,
        source_id: axis_val(node, "axId").and_then(|v| v.parse().ok()),
        source_kind: match node.tag_name().name() {
            "valAx" => AxisSourceKind::Value,
            "dateAx" => AxisSourceKind::Date,
            "serAx" => AxisSourceKind::Series,
            _ => AxisSourceKind::Category,
        },
        crosses: match axis_val(node, "crosses") {
            Some("autoZero") => Some(AxisCrosses::AutoZero),
            Some("max") => Some(AxisCrosses::Maximum),
            Some("min") => Some(AxisCrosses::Minimum),
            _ => None,
        },
        crosses_at: axis_val(node, "crossesAt").and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| v.is_finite()).map(Number),
        cross_axis_id: axis_val(node, "crossAx").and_then(|v| v.parse().ok()),
        kind: if node.has_tag_name((CHART_NS, "valAx")) { AxisKind::Value } else if node.has_tag_name((CHART_NS, "serAx")) { AxisKind::Series } else { AxisKind::Category },
        side: side(val(node, "axPos").unwrap_or("b")),
        deleted: val_bool(node, "delete").unwrap_or(false),
        reversed: scaling.and_then(|s| val(s, "orientation")) == Some("maxMin"),
        log_base: scaling.and_then(|s| val_f64(s, "logBase")).map(Number),
        min: scaling.and_then(|s| val_f64(s, "min")).map(Number),
        max: scaling.and_then(|s| val_f64(s, "max")).map(Number),
        major_unit: val_f64(node, "majorUnit").map(Number),
        display_unit: child(node, "dispUnits").and_then(display_unit).map(Number),
        format: child(node, "numFmt").and_then(|f| attr(f, "formatCode")).unwrap_or("General").to_string(),
        show_labels: val(node, "tickLblPos") != Some("none"),
        labels_high: val(node, "tickLblPos") == Some("high"),
        labels_low: val(node, "tickLblPos") == Some("low"),
        // −60 000 000 is "not set"; the chart's own layout decides.
        label_rotation: path(node, &["txPr", "bodyPr"])
            .and_then(|b| attr_i64(b, "rot"))
            .filter(|r| *r != -60_000_000)
            .unwrap_or(0) as i32,
        fixed_label_rotation: path(node, &["txPr", "bodyPr"])
            .and_then(|b| attr_i64(b, "rot"))
            .is_some_and(|r| r != -60_000_000),
        label_skip: label_skip(node),
        date: node.has_tag_name((CHART_NS, "dateAx")),
        time: None,
        ticks_outside: matches!(val(node, "majorTickMark"), Some("out" | "cross")),
        line: match child(node, "spPr") {
            // (A vertical value axis of the old defaults has no line of its own; Word and Excel draw the
            // horizontal axis only: measured against Word with DCHT.)
            None if legacy && !powerpoint_styled && node.has_tag_name((CHART_NS, "valAx")) && matches!(val(node, "axPos"), Some("l" | "r")) => None,
            None if legacy => Some(Stroke::solid(if powerpoint_styled { plain_color } else { Color::opaque(if legacy_border { ibis_doc_model::Rgb(0x868686) } else { ibis_doc_model::Rgb::BLACK }) }, 9_525)),
            sp => shape_props(sp, colors).1.flatten(),
        },
        gridlines: gridlines.flatten(),
        minor_gridlines: minor_gridlines.flatten(),
        minor_unit: val_f64(node, "minorUnit").filter(|u| *u > 0.0).map(Number),
        text: text_style(child(node, "txPr"), base, colors),
        title: child(node, "title").and_then(|t| {
            let title_default = ChartText { size: 1000, bold: powerpoint_styled, ..base.clone() };
            let mut style = text_style(child(t, "txPr"), &title_default, colors);
            let rich = path(t, &["tx", "rich"]);
            if let Some(def) = rich.and_then(|r| r.descendants().find(|n| n.is_element() && n.tag_name().name() == "defRPr")) {
                style = run_style(def, &style, colors);
            }
            if let Some(rpr) = rich.and_then(|r| r.descendants().find(|n| n.is_element() && n.tag_name().name() == "rPr")) {
                style = run_style(rpr, &style, colors);
            }
            let text = match rich {
                Some(r) => children(r, "p")
                    .map(|p| p.descendants().filter(|n| n.is_element() && n.tag_name().name() == "t").filter_map(|n| n.text()).collect::<String>())
                    .collect::<Vec<_>>()
                    .join("\n"),
                None => path(t, &["tx", "strRef", "strCache"])
                    .map(|sc| children(sc, "pt").filter_map(|pt| child(pt, "v").and_then(|v| v.text())).collect::<Vec<_>>().join("\n"))
                    .unwrap_or_default(),
            };
            let at = path(t, &["layout", "manualLayout"]).and_then(|m| Some((fraction(val_f64(m, "x")?), fraction(val_f64(m, "y")?))));
            (!text.trim().is_empty()).then_some(ChartTitle { text, style, at })
        }),
        stagger_labels: false,
        title_rotation: child(node, "title").and_then(|t| path(t, &["tx", "rich", "bodyPr"])).and_then(|b| attr_i64(b, "rot")).map(|r| r as i32),
        category_levels: Vec::new(),
    }
}

#[cfg(test)]
mod label_separator_tests {
    use super::read_labels;
    use crate::theme::Colors;
    use ibis_doc_model::ChartText;

    #[test]
    fn separator_is_read_as_written() {
        let xml = r#"<c:dLbls xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart"><c:showVal val="1"/><c:showPercent val="1"/><c:separator>
</c:separator></c:dLbls>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let labels = read_labels(doc.root_element(), &ChartText::default(), &Colors::new(&crate::theme::Theme::default(), None, &[])).unwrap();
        assert_eq!(labels.separator.as_deref(), Some("\n"));
    }
}

#[cfg(test)]
mod multi_level_axis_tests {
    use super::read_in;
    use crate::theme::Colors;
    use ibis_doc_model::{AxisKind, LineDash};

    fn read(xml: &str) -> ibis_doc_model::Chart {
        let doc = roxmltree::Document::parse(xml).unwrap();
        read_in(doc.root_element(), &Colors::new(&crate::theme::Theme::default(), None, &[]), false).expect("a chart")
    }

    const HEAD: &str = r#"<c:chartSpace xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><c:chart><c:plotArea>"#;

    /// GitHub #22 (xlsx 109): a year over its months (`c:multiLvlStrRef`), a value axis' title set upright at its
    /// own place, and the plot area's outline.
    #[test]
    fn a_category_axis_of_two_levels_a_horizontal_axis_title_and_a_plot_outline() {
        let chart = read(&format!(
            r#"{HEAD}<c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:idx val="0"/><c:order val="0"/>
            <c:cat><c:multiLvlStrRef><c:f>S!$A$1:$B$3</c:f><c:multiLvlStrCache><c:ptCount val="3"/>
              <c:lvl><c:pt idx="0"><c:v>7月</c:v></c:pt><c:pt idx="1"><c:v>8月</c:v></c:pt><c:pt idx="2"><c:v>1月</c:v></c:pt></c:lvl>
              <c:lvl><c:pt idx="0"><c:v>2025年</c:v></c:pt><c:pt idx="2"><c:v>2026年</c:v></c:pt></c:lvl>
            </c:multiLvlStrCache></c:multiLvlStrRef></c:cat>
            <c:val><c:numRef><c:f>S!$C$1:$C$3</c:f><c:numCache><c:ptCount val="3"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt><c:pt idx="2"><c:v>3</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser>
            <c:axId val="1"/><c:axId val="2"/></c:barChart>
            <c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx>
            <c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/>
              <c:title><c:tx><c:rich><a:bodyPr rot="0" vert="horz"/><a:p><a:r><a:t>％</a:t></a:r></a:p></c:rich></c:tx>
                <c:layout><c:manualLayout><c:xMode val="edge"/><c:yMode val="edge"/><c:x val="0.0146"/><c:y val="0.0526"/></c:manualLayout></c:layout></c:title>
              <c:crossAx val="1"/></c:valAx>
            <c:spPr><a:noFill/><a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></c:spPr>
            </c:plotArea></c:chart></c:chartSpace>"#
        ));
        let category = chart.axes.iter().find(|a| a.kind == AxisKind::Category).unwrap();
        assert_eq!(category.category_levels, vec![vec![(0, 2, "2025年".to_string()), (2, 1, "2026年".to_string())]]);
        // The series' categories are the innermost level's.
        assert_eq!(chart.groups[0].series[0].categories, vec!["7月", "8月", "1月"]);
        let value = chart.axes.iter().find(|a| a.kind == AxisKind::Value).unwrap();
        assert_eq!(value.title_rotation, Some(0));
        assert!(value.title.as_ref().is_some_and(|t| t.at.is_some()));
        assert_eq!(chart.plot_border.as_ref().map(|b| b.width), Some(12700));
    }

    /// GitHub #22 (xlsx 111): a series' line of `a:prstDash` "dash" keeps its dash.
    #[test]
    fn a_series_line_keeps_its_dash() {
        let chart = read(&format!(
            r#"{HEAD}<c:lineChart><c:grouping val="standard"/><c:ser><c:idx val="0"/><c:order val="0"/>
            <c:spPr><a:ln w="12700"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:prstDash val="dash"/></a:ln></c:spPr>
            <c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt></c:numLit></c:val></c:ser>
            <c:axId val="1"/><c:axId val="2"/></c:lineChart>
            <c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx>
            <c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="1"/></c:valAx>
            </c:plotArea></c:chart></c:chartSpace>"#
        ));
        assert_eq!(chart.groups[0].series[0].stroke.as_ref().map(|s| s.dash), Some(LineDash::Dash));
    }

    /// Every line type (`a:prstDash`) of a series' line, a value axis' gridlines and the axes' lines is read, at each
    /// of the places the chart draws a line from.
    #[test]
    fn every_dash_type_is_read_for_a_series_the_gridlines_and_the_axes() {
        let types = [
            ("solid", LineDash::Solid), ("dot", LineDash::Dot), ("dash", LineDash::Dash), ("lgDash", LineDash::LgDash),
            ("dashDot", LineDash::DashDot), ("lgDashDot", LineDash::LgDashDot), ("lgDashDotDot", LineDash::LgDashDotDot),
            ("sysDash", LineDash::SysDash), ("sysDot", LineDash::SysDot), ("sysDashDot", LineDash::SysDashDot),
            ("sysDashDotDot", LineDash::SysDashDotDot),
        ];
        for (name, want) in types {
            let ln = |width: i64| format!(r#"<c:spPr><a:ln w="{width}"><a:solidFill><a:srgbClr val="808080"/></a:solidFill><a:prstDash val="{name}"/></a:ln></c:spPr>"#);
            let chart = read(&format!(
                r#"{HEAD}<c:lineChart><c:grouping val="standard"/><c:ser><c:idx val="0"/><c:order val="0"/>{series}
                <c:val><c:numLit><c:ptCount val="2"/><c:pt idx="0"><c:v>1</c:v></c:pt><c:pt idx="1"><c:v>2</c:v></c:pt></c:numLit></c:val></c:ser>
                <c:axId val="1"/><c:axId val="2"/></c:lineChart>
                <c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/>{cat_line}<c:crossAx val="2"/></c:catAx>
                <c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/>
                  <c:majorGridlines>{grid}</c:majorGridlines>{val_line}<c:crossAx val="1"/></c:valAx>
                </c:plotArea></c:chart></c:chartSpace>"#,
                series = ln(12_700), cat_line = ln(9_525), grid = ln(3_175), val_line = ln(6_350),
            ));
            let dash = |s: &Option<ibis_doc_model::Stroke>| s.as_ref().map(|s| s.dash);
            assert_eq!(dash(&chart.groups[0].series[0].stroke), Some(want), "series {name}");
            let cat = chart.axes.iter().find(|a| a.kind == AxisKind::Category).unwrap();
            let val = chart.axes.iter().find(|a| a.kind == AxisKind::Value).unwrap();
            assert_eq!(dash(&cat.line), Some(want), "category axis {name}");
            assert_eq!(dash(&val.line), Some(want), "value axis {name}");
            assert_eq!(dash(&val.gridlines), Some(want), "gridlines {name}");
            // Each at its own width.
            assert_eq!(val.gridlines.as_ref().map(|s| s.width), Some(3_175), "gridlines' width {name}");
            assert_eq!(chart.groups[0].series[0].stroke.as_ref().map(|s| s.width), Some(12_700), "series' width {name}");
        }
    }
}

