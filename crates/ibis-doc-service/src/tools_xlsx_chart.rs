// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `xlsx_*` chart tools of stage B step 4 (design `2026-11-24-159-mcp-stage-b.md` 5 and 8): a chart of a range of cells is made the way 挿入 > グラフ makes it
//! (`ibis_formats::Source::new_sheet_chart`, the series taken from the cells as the window takes them), then its title, legend and frame are set. The chart
//! follows its cells (`xlsx_list_charts` tells which). Chart kinds Excel's new chart types (waterfall, treemap, ...), stock, surface, bubble and combo charts, and
//! the editing of a chart's series and axes are the window's: they are refused with `unsupported_operation`.

use ibis_doc_model::chart::{ChartText, ChartTitle, Legend, LegendPosition, Side};
use ibis_doc_model::{Command, EMU_PER_POINT, EmuRect, ShapeId, ShapeKind};
use ibis_formats::NewChartKind;
use serde_json::{Value, json};

use crate::a1::parse_range;
use crate::catalog::{Access, Tool, document_id, integer, object, string};
use crate::edit::{edited, execute, only_known};
use crate::error::ToolError;
use crate::service::{Args, Open, Service};
use crate::tools_xlsx::{sheet_id, sheets_of};

const KINDS: [(&str, NewChartKind); 17] = [
    ("column", NewChartKind::ClusteredColumn),
    ("stacked_column", NewChartKind::StackedColumn),
    ("percent_stacked_column", NewChartKind::PercentStackedColumn),
    ("bar", NewChartKind::ClusteredBar),
    ("stacked_bar", NewChartKind::StackedBar),
    ("percent_stacked_bar", NewChartKind::PercentStackedBar),
    ("line", NewChartKind::Line),
    ("stacked_line", NewChartKind::StackedLine),
    ("area", NewChartKind::Area),
    ("stacked_area", NewChartKind::StackedArea),
    ("pie", NewChartKind::Pie),
    ("doughnut", NewChartKind::Doughnut),
    ("scatter", NewChartKind::Scatter),
    ("scatter_smooth", NewChartKind::ScatterSmooth),
    ("scatter_lines", NewChartKind::ScatterLines),
    ("radar", NewChartKind::Radar),
    ("radar_filled", NewChartKind::RadarFilled),
];

fn frame_schema() -> Value {
    json!({ "type": "object", "properties": { "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number", "exclusiveMinimum": 0 }, "height": { "type": "number", "exclusiveMinimum": 0 } }, "additionalProperties": false, "description": "Points from the sheet's top left; what is left out is where and as big as 挿入 > グラフ puts it." })
}

fn legend_schema() -> Value {
    json!({ "enum": ["right", "left", "top", "bottom", "none"], "description": "Where the legend is; none takes it away." })
}

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "xlsx_insert_chart",
            description: "Insert a chart of a range of cells (the first row and column are taken as the names when they are text, and the series run along the columns unless the range is wider than tall, as Excel takes them). kind: column, stacked_column, percent_stacked_column, bar, stacked_bar, percent_stacked_bar, line, stacked_line, area, stacked_area, pie, doughnut, scatter, scatter_smooth, scatter_lines, radar, radar_filled. The chart follows its cells. Returns its shape id; xlsx_list_charts reads it.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                let kinds: Vec<&str> = KINDS.iter().map(|k| k.0).collect();
                object(
                    &[
                        document_id(),
                        ("sheet", string("The sheet the chart is put on.")),
                        ("kind", json!({ "enum": kinds })),
                        ("data_range", string("A1 range of the cells, on the same sheet.")),
                        ("series_in", json!({ "enum": ["columns", "rows"], "description": "Which way the series run (default: as Excel takes them)." })),
                        ("title", json!({ "type": ["string", "null"] })),
                        ("legend", legend_schema()),
                        ("frame", frame_schema()),
                    ],
                    &["document_id", "sheet", "kind", "data_range"],
                )
            },
            run: insert_chart,
        },
        Tool {
            name: "xlsx_set_chart",
            description: "Change a chart (its shape id from xlsx_list_charts): title (null takes it away), legend, frame (points). Its kind, series and axes are changed in the window (unsupported_operation here).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("sheet", string("The sheet the chart is on.")),
                        ("chart", integer("The chart's shape id.")),
                        ("title", json!({ "type": ["string", "null"] })),
                        ("legend", legend_schema()),
                        ("frame", frame_schema()),
                    ],
                    &["document_id", "sheet", "chart"],
                )
            },
            run: set_chart,
        },
        Tool {
            name: "xlsx_delete_chart",
            description: "Delete a chart (its shape id from xlsx_list_charts).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("sheet", string("The sheet the chart is on.")), ("chart", integer("The chart's shape id."))], &["document_id", "sheet", "chart"]),
            run: delete_chart,
        },
    ]
}

fn points(name: &str, v: &Value) -> Result<i64, ToolError> {
    let pt = v.as_f64().filter(|p| p.is_finite() && p.abs() <= 100_000.0).ok_or_else(|| ToolError::invalid(format!("frame.{name} is a number of points")))?;
    Ok((pt * EMU_PER_POINT as f64).round() as i64)
}

/// The frame argument over `base`: what is not given stays.
fn frame_over(base: EmuRect, frame: &Value) -> Result<EmuRect, ToolError> {
    let map = frame.as_object().ok_or_else(|| ToolError::invalid("frame is { x?, y?, width?, height? } in points"))?;
    let mut out = base;
    for (key, value) in map {
        let v = points(key, value)?;
        match key.as_str() {
            "x" => out.x = v,
            "y" => out.y = v,
            "width" if v > 0 => out.width = v,
            "height" if v > 0 => out.height = v,
            "width" | "height" => return Err(ToolError::invalid(format!("frame.{key} is above 0"))),
            other => return Err(ToolError::invalid(format!("frame.{other} is not x, y, width or height"))),
        }
    }
    Ok(out)
}

pub(crate) fn legend_of(chart: &mut ibis_doc_model::Chart, value: &Value) -> Result<(), ToolError> {
    let (position, side) = match value.as_str() {
        Some("none") => {
            chart.legend = None;
            return Ok(());
        }
        Some("right") => (LegendPosition::Right, Side::Right),
        Some("left") => (LegendPosition::Left, Side::Left),
        Some("top") => (LegendPosition::Top, Side::Top),
        Some("bottom") => (LegendPosition::Bottom, Side::Bottom),
        _ => return Err(ToolError::invalid("legend is right, left, top, bottom or none")),
    };
    match chart.legend.as_mut() {
        Some(l) => {
            l.source_position = Some(position);
            l.side = side;
            l.layout = None;
        }
        None => chart.legend = Some(Legend { source_position: Some(position), side, overlay: Some(false), layout: None, text: ChartText::default(), hidden: Vec::new() }),
    }
    Ok(())
}

pub(crate) fn title_of(chart: &mut ibis_doc_model::Chart, value: &Value) -> Result<(), ToolError> {
    match value {
        Value::Null => chart.title = None,
        Value::String(text) if !text.is_empty() => {
            match chart.title.as_mut() {
                Some(t) => t.text = text.clone(),
                None => chart.title = Some(ChartTitle { text: text.clone(), style: ChartText { size: 1400, ..Default::default() }, at: None }),
            }
        }
        _ => return Err(ToolError::invalid("title is text, or null")),
    }
    Ok(())
}

fn insert_chart(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "kind", "data_range", "series_in", "title", "legend", "frame"])?;
    let id = args.str_arg("document_id")?;
    let kind_name = args.str_arg("kind")?;
    let kind = match KINDS.iter().find(|k| k.0 == kind_name) {
        Some(k) => k.1,
        None => {
            let known = ["waterfall", "histogram", "pareto", "funnel", "box_whisker", "treemap", "sunburst", "bubble", "stock", "surface", "contour", "combo", "pie_of_pie", "bar_of_pie"];
            return Err(if known.contains(&kind_name) { ToolError::new("unsupported_operation", format!("a {kind_name} chart is made in the window")) } else { ToolError::invalid(format!("kind {kind_name} is not one of {}", KINDS.iter().map(|k| k.0).collect::<Vec<_>>().join(", "))) });
        }
    };
    let range = parse_range(args.str_arg("data_range")?)?;
    let by_columns = match args.opt_str("series_in")? {
        None => None,
        Some("columns") => Some(true),
        Some("rows") => Some(false),
        Some(_) => return Err(ToolError::invalid("series_in is columns or rows")),
    };
    let open = service.open_document(id)?;
    let (slide, name) = sheet_id(open, args.str_arg("sheet")?)?;
    let current = open.document.presentation().clone();
    let quoted = ibis_formats::quote_sheet(&name);
    let scatter = matches!(kind, NewChartKind::Scatter | NewChartKind::ScatterSmooth | NewChartKind::ScatterLines);
    // X-Y charts read the X column first: in columns unless the range is wider than tall.
    let by_columns = by_columns.unwrap_or(if scatter { range.last_row - range.first_row >= range.last_column - range.first_column } else { ibis_formats::preferred_series_columns(&current, &quoted, range) });
    let refs = if scatter { ibis_formats::scatter_refs_for_range(&current, &quoted, range, by_columns).or_else(|| ibis_formats::scatter_refs_for_range(&current, &quoted, range, !by_columns)) } else { None }
        .or_else(|| ibis_formats::refs_for_range(&current, &quoted, range, by_columns))
        .or_else(|| ibis_formats::refs_for_range(&current, &quoted, range, !by_columns))
        .filter(|r| !r.is_empty())
        .ok_or_else(|| ToolError::invalid("the range has no numbers to chart"))?;
    // Where it goes: under the data, or where `frame` says.
    let (sheet_frame, columns, rows, charts) = {
        let slide_ref = current.slide(slide).ok_or_else(|| ToolError::new("no_such_sheet", name.clone()))?;
        let (shape, sheet) = slide_ref.shapes.iter().find_map(|s| if let ShapeKind::Sheet(sh) = &s.kind { Some((s, sh)) } else { None }).ok_or_else(|| ToolError::new("no_such_sheet", name.clone()))?;
        let charts: Vec<EmuRect> = slide_ref.shapes.iter().filter(|s| matches!(s.kind, ShapeKind::Chart(_))).map(|s| s.frame).collect();
        (shape.frame, sheet.column_offsets(), sheet.row_offsets(), charts)
    };
    let mut frame = EmuRect::new(
        sheet_frame.x + columns.get(range.first_column as usize).copied().unwrap_or(0),
        sheet_frame.y + rows.get(range.last_row as usize + 1).copied().unwrap_or(0) + 190_500,
        4_572_000,
        2_743_200,
    );
    // Not on a chart already there: a half inch down and right of it, as Excel cascades them.
    while charts.iter().any(|f| (f.x - frame.x).abs() < 100_000 && (f.y - frame.y).abs() < 100_000) {
        frame.x += 457_200;
        frame.y += 457_200;
    }
    if let Some(f) = args.get("frame") {
        frame = frame_over(frame, f)?;
    }
    let shape_id = open.document.ids().new_shape_id();
    let mut shape = open.source.new_sheet_chart(&current, slide, shape_id, frame, kind, refs).map_err(|e| ToolError::new("edit_rejected", e.to_string()))?;
    if let ShapeKind::Chart(chart) = &mut shape.kind {
        if let Some(t) = args.get("title") {
            title_of(chart, t)?;
        }
        if let Some(l) = args.get("legend") {
            legend_of(chart, l)?;
        }
        // The theme's accent colours, one for each series, as the window colours a new chart.
        let accents: Vec<ibis_doc_model::Rgb> = (1..=6).filter_map(|n| current.theme_colors.iter().find(|(key, _)| *key == format!("accent{n}")).map(|(_, c)| *c)).collect();
        if accents.len() == 6 {
            for (i, series) in chart.groups.iter_mut().flat_map(|g| &mut g.series).enumerate() {
                let color = ibis_doc_model::Color::opaque(accents[i % accents.len()]);
                if !matches!(series.fill, ibis_doc_model::Fill::None) {
                    series.fill = ibis_doc_model::Fill::Solid(color);
                }
                if let Some(stroke) = series.stroke.as_mut() {
                    stroke.color = color;
                }
                if let Some(marker) = series.marker.as_mut() {
                    marker.fill = ibis_doc_model::Fill::Solid(color);
                }
            }
        }
    }
    let index = current.slide(slide).map_or(0, |s| s.shapes.len());
    execute(open, vec![Command::InsertShape { slide, shapes: vec![(index, shape)] }])?;
    Ok(edited(id, open, json!({ "chart": shape_id.0 }), Vec::new()))
}

/// The chart `shape` of the sheet `name`, and the slide it is on.
fn chart_of<'a>(open: &'a Open, args: &Value) -> Result<(ibis_doc_model::SlideId, ShapeId, &'a ibis_doc_model::Shape), ToolError> {
    let (slide, _) = sheet_id(open, args.str_arg("sheet")?)?;
    let shape_id = ShapeId(args.usize_arg("chart")? as u64);
    let found = open.document.presentation().slide(slide).and_then(|s| s.shapes.iter().find(|s| s.id == shape_id && matches!(s.kind, ShapeKind::Chart(_))));
    found.map(|s| (slide, shape_id, s)).ok_or_else(|| ToolError::new("no_such_shape", format!("no chart {} on that sheet (xlsx_list_charts)", shape_id.0)))
}

fn set_chart(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    if let Some(key) = ["kind", "series", "data_range", "axes"].into_iter().find(|k| args.get(*k).is_some()) {
        return Err(ToolError::new("unsupported_operation", format!("{key} of a chart is changed in the window")));
    }
    only_known(args, &["document_id", "sheet", "chart", "title", "legend", "frame"])?;
    let id = args.str_arg("document_id")?;
    if args.get("title").is_none() && args.get("legend").is_none() && args.get("frame").is_none() {
        return Err(ToolError::invalid("no setting is given"));
    }
    let open = service.open_document(id)?;
    sheets_of(open)?;
    let (slide, shape_id, shape) = chart_of(open, args)?;
    let mut commands = Vec::new();
    if args.get("title").is_some() || args.get("legend").is_some() {
        let ShapeKind::Chart(chart) = &shape.kind else { unreachable!("chart_of finds charts") };
        let mut chart = (**chart).clone();
        if let Some(t) = args.get("title") {
            title_of(&mut chart, t)?;
        }
        if let Some(l) = args.get("legend") {
            legend_of(&mut chart, l)?;
        }
        commands.push(Command::SetShapeKind { slide, shape: shape_id, kind: Box::new(ShapeKind::Chart(Box::new(chart))) });
    }
    if let Some(f) = args.get("frame") {
        commands.push(Command::SetFrame { slide, frames: vec![(shape_id, frame_over(shape.frame, f)?)], with_attached: true });
    }
    execute(open, commands)?;
    Ok(edited(id, open, json!({ "chart": shape_id.0 }), Vec::new()))
}

fn delete_chart(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "chart"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    sheets_of(open)?;
    let (slide, shape_id, _) = chart_of(open, args)?;
    execute(open, vec![Command::DeleteShape { slide, shapes: vec![shape_id], with_attached: true }])?;
    Ok(edited(id, open, json!({}), Vec::new()))
}
