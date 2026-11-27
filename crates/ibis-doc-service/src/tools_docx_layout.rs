// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `docx_*` tools of stage B step 6 (the "要移動" of `tasks/doc/design/2026-11-24-159-mcp-stage-b.md` 8): a chart, a table of contents, and the wrap of a picture. The screen
//! and the tools call the same pieces: `Source::new_doc_chart` (`ibis-formats`) makes the chart with its embedded workbook, `flow_command::toc_entries` the entries of a table of
//! contents, `FlowCommand::SetPictureWrap` turns a picture into a floating one.
//!
//! A table of contents made here has no page numbers: they come from laying the pages out, which needs the window's text system (`ibis-office-docx`'s `flow_layout`, which draws
//! with GPUI), and this crate has none. The field is marked to be made again (`w:dirty`), so Word makes the table again with its page numbers when it opens the file, and the
//! screen's 目次の更新 does it too.

use ibis_doc_model::flow::FlowRunKind;
use ibis_doc_model::flow_command::{self, FlowCommand, FlowPos, TocInstruction};
use ibis_doc_model::sheet::CellRange;
use ibis_doc_model::sheet_command::{CellEdit, SheetCommand};
use ibis_doc_model::{Chart, Command, EmuSize, PictureWrapType, Presentation, Shape, ShapeId, ShapeKind};
use ibis_formats::{ChartData, NewChartKind, SaveOptions};
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, document_id, integer, object};
use crate::edit::{edited, execute, only_known};
use crate::error::ToolError;
use crate::service::{Args, Service};
use crate::tools_docx::{body_paragraphs, flow_of, flow_pos, position_schema};
use crate::tools_xlsx::input_of;
use crate::tools_xlsx_chart::{legend_of, title_of};

const EMU_PER_POINT: f64 = 12_700.0;

/// The chart kinds a document takes (`Insert > Chart`), by the names the tools use (the same as `xlsx_insert_chart`'s).
pub(crate) const CHART_KINDS: [(&str, NewChartKind); 17] = [
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

/// The size 挿入 > グラフ gives a chart in a document (6 x 3.5 inches).
const CHART_SIZE: EmuSize = EmuSize::new(5_486_400, 3_200_400);

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "docx_insert_chart",
            description: "Insert a chart at a position, as 挿入 > グラフ does: kind column, stacked_column, percent_stacked_column, bar, stacked_bar, percent_stacked_bar, line, stacked_line, area, stacked_area, pie, doughnut, scatter, scatter_smooth, scatter_lines, radar or radar_filled; width and height in points (default 432 x 252). data is the numbers to chart as rows of cells like a selection in a sheet: the first row names the series, the first column the categories, the rest are numbers (series_in says whether the series are the columns, the default, or the rows); the chart is kept with them in an embedded workbook. Without data it has the kind's sample data. title (a string; null: none) and legend (right, left, top, bottom, none) are as xlsx_insert_chart takes them. Read it with docx_list_charts.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("at", position_schema()),
                        ("kind", json!({ "enum": CHART_KINDS.iter().map(|k| k.0).collect::<Vec<_>>() })),
                        ("width", json!({ "type": "number", "exclusiveMinimum": 0, "maximum": 20000, "description": "Points." })),
                        ("height", json!({ "type": "number", "exclusiveMinimum": 0, "maximum": 20000, "description": "Points." })),
                        ("data", json!({ "type": "array", "minItems": 2, "items": { "type": "array", "minItems": 2, "items": { "type": ["number", "string", "boolean", "null"] } }, "description": "Rows of cells: the first row names the series, the first column the categories, the rest are numbers." })),
                        ("series_in", json!({ "enum": ["columns", "rows"], "description": "Whether each column or each row is a series (default: as the data suggests)." })),
                        ("title", json!({ "type": ["string", "null"], "description": "The chart's title; null for none." })),
                        ("legend", json!({ "enum": ["right", "left", "top", "bottom", "none"] })),
                    ],
                    &["document_id", "at", "kind"],
                )
            },
            run: insert_chart,
        },
        Tool {
            name: "docx_set_chart_data",
            description: "Replace the data of the chart at a position (docx_list_charts gives paragraph and offset), as the chart's data grid does: data is rows of cells like a selection in a sheet (the first row names the series, the first column the categories, the rest are numbers; series_in says whether the series are the columns, the default, or the rows). The chart keeps its kind and formatting, and its embedded workbook is made from the data. One undo step.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("at", position_schema()),
                        ("data", json!({ "type": "array", "minItems": 2, "items": { "type": "array", "minItems": 2, "items": { "type": ["number", "string", "boolean", "null"] } }, "description": "Rows of cells: the first row names the series, the first column the categories, the rest are numbers." })),
                        ("series_in", json!({ "enum": ["columns", "rows"], "description": "Whether each column or each row is a series (default: as the data suggests)." })),
                    ],
                    &["document_id", "at", "data"],
                )
            },
            run: set_chart_data,
        },
        Tool {
            name: "docx_set_table_of_contents",
            description: "Insert a table of contents of the headings of levels `from` to `to` (default 1 to 3) before a body paragraph (the position's paragraph), or make the one that starts there again (replace). It has no page numbers yet: the field is marked to be made again, which Word does when it opens the file and the window's 目次の更新 does.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("paragraph", integer("The body paragraph (0-based) the table goes before.")),
                        ("from", json!({ "type": "integer", "minimum": 1, "maximum": 9, "description": "The first heading level (default 1)." })),
                        ("to", json!({ "type": "integer", "minimum": 1, "maximum": 9, "description": "The last heading level (default 3)." })),
                        ("replace", json!({ "type": "boolean", "description": "The paragraph is the first of a table of contents: make it again." })),
                    ],
                    &["document_id", "paragraph"],
                )
            },
            run: set_table_of_contents,
        },
        Tool {
            name: "docx_set_picture_wrap",
            description: "Set how text wraps around the picture (or floating shape) whose run is at a position: inline, square, tight, through, top_and_bottom, behind_text or in_front_of_text. A picture that was inline floats afterwards (its run is then a floating shape: read it with docx_list_shapes, the position stays).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("at", position_schema()), ("wrap", json!({ "enum": ["inline", "square", "tight", "through", "top_and_bottom", "behind_text", "in_front_of_text"] }))], &["document_id", "at", "wrap"]),
            run: set_picture_wrap,
        },
    ]
}

/// The rows of `data` as the sheet of a new workbook, and the workbook as the chart's data over them: the workbook as saved with the series' references, and the model of it.
pub(crate) fn workbook_of(rows: &[Vec<Value>], scatter: bool, series_in: Option<bool>, sample: &Chart) -> Result<(Chart, ChartData), ToolError> {
    let bytes = ibis_formats::blank_file(ibis_formats::Kind::Workbook).map_err(|e| ToolError::new("edit_rejected", e.to_string()))?;
    let opened = ibis_formats::open(bytes).map_err(|e| ToolError::new("edit_rejected", e.to_string()))?;
    let first = opened.presentation.slides.first().ok_or_else(|| ToolError::new("edit_rejected", "the new workbook has no sheet"))?;
    let (sheet, name) = (first.id, first.name.clone().unwrap_or_else(|| "Sheet1".to_string()));
    let mut edits = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            edits.push(CellEdit { row: r as u32, column: c as u32, input: input_of(value)? });
        }
    }
    let mut doc = ibis_doc_model::Document::new(opened.presentation.clone());
    doc.execute(vec![Command::Sheet(SheetCommand::SetCells { sheet, edits })]).map_err(|e| ToolError::new("edit_rejected", format!("{e:?}")))?;
    let book: Presentation = doc.presentation().clone();
    let quoted = ibis_formats::quote_sheet(&name);
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0) as u32;
    let range = CellRange { first_row: 0, first_column: 0, last_row: rows.len() as u32 - 1, last_column: columns - 1 };
    // X-Y charts read the X column first: in columns unless the range is wider than tall (as xlsx_insert_chart does).
    let by_columns = series_in.unwrap_or(if scatter { range.last_row - range.first_row >= range.last_column - range.first_column } else { ibis_formats::preferred_series_columns(&book, &quoted, range) });
    let refs = if scatter { ibis_formats::scatter_refs_for_range(&book, &quoted, range, by_columns).or_else(|| ibis_formats::scatter_refs_for_range(&book, &quoted, range, !by_columns)) } else { None }
        .or_else(|| ibis_formats::refs_for_range(&book, &quoted, range, by_columns))
        .or_else(|| ibis_formats::refs_for_range(&book, &quoted, range, !by_columns))
        .filter(|r| !r.is_empty())
        .ok_or_else(|| ToolError::invalid("data has no numbers to chart: the first row names the series, the first column the categories, the rest are numbers"))?;
    let workbook = opened.source.save(&book, SaveOptions::default()).map_err(|e| ToolError::new("edit_rejected", e.to_string()))?;
    let chart = ibis_formats::chart_from_data(sample, &book, &refs);
    Ok((chart, ChartData { workbook, series: refs }))
}

/// `data` of a chart tool: rows of cells (at least 2 by 2, at most 1000 by 100).
pub(crate) fn chart_rows(args: &Value) -> Result<Vec<Vec<Value>>, ToolError> {
    let Some(Value::Array(list)) = args.get("data") else { return Err(ToolError::invalid("data is rows (arrays) of cells")) };
    let rows: Vec<Vec<Value>> = list.iter().map(|r| r.as_array().cloned().ok_or_else(|| ToolError::invalid("data is rows (arrays) of cells"))).collect::<Result<_, _>>()?;
    if rows.len() < 2 || rows.iter().any(|r| r.len() < 2) {
        return Err(ToolError::invalid("data is at least 2 rows of at least 2 cells"));
    }
    if rows.len() > 1_000 || rows.iter().any(|r| r.len() > 100) {
        return Err(ToolError::new("out_of_range", "data is at most 1000 rows by 100 columns"));
    }
    Ok(rows)
}

/// `series_in` of a chart tool: true for columns, false for rows.
pub(crate) fn series_in_arg(args: &Value) -> Result<Option<bool>, ToolError> {
    match args.opt_str("series_in")? {
        None => Ok(None),
        Some("columns") => Ok(Some(true)),
        Some("rows") => Ok(Some(false)),
        Some(_) => Err(ToolError::invalid("series_in is columns or rows")),
    }
}

/// Whether the chart's first group is an X-Y chart (its data is read X first).
pub(crate) fn is_scatter(chart: &Chart) -> bool {
    chart.groups.first().is_some_and(|g| matches!(g.kind, ibis_doc_model::ChartType::Scatter { .. }))
}

fn set_chart_data(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at", "data", "series_in"])?;
    let id = args.str_arg("document_id")?;
    let rows = chart_rows(args)?;
    let series_in = series_in_arg(args)?;
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let at = flow_pos(flow, args.get("at").unwrap_or(&Value::Null), "at")?;
    // The chart is the run at that offset of that paragraph.
    let found = body_paragraphs(flow).into_iter().find(|p| p.id == at.para).and_then(|p| {
        let mut offset = 0usize;
        for r in &p.runs {
            if offset == at.offset {
                if let FlowRunKind::Shape(s) = &r.kind {
                    if let ShapeKind::Chart(c) = &s.kind {
                        return Some((s.id, (**c).clone()));
                    }
                }
            }
            offset += r.len();
        }
        None
    });
    let (shape_id, chart) = found.ok_or_else(|| ToolError::new("no_such_shape", "there is no chart at that position (docx_list_charts)"))?;
    if chart.ex.is_some() {
        return Err(ToolError::new("unsupported_operation", "the data of this kind of chart is not changed by this tool"));
    }
    let (target, data) = workbook_of(&rows, is_scatter(&chart), series_in, &chart)?;
    open.source.set_chart_data(shape_id, target.clone(), data);
    execute(open, vec![Command::Flow(FlowCommand::SetChart { at, chart: Box::new(target) })])?;
    Ok(edited(id, open, json!({ "chart": shape_id.0 }), Vec::new()))
}

fn insert_chart(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at", "kind", "width", "height", "data", "series_in", "title", "legend"])?;
    let id = args.str_arg("document_id")?;
    let kind_name = args.str_arg("kind")?;
    let kind = CHART_KINDS.iter().find(|k| k.0 == kind_name).map(|k| k.1).ok_or_else(|| ToolError::invalid(format!("kind {kind_name} is not one of {}", CHART_KINDS.iter().map(|k| k.0).collect::<Vec<_>>().join(", "))))?;
    let dim = |name: &str, default: i64| -> Result<i64, ToolError> {
        match args.get(name) {
            None | Some(Value::Null) => Ok(default),
            Some(v) => v.as_f64().filter(|n| n.is_finite() && *n > 0.0 && *n <= 20_000.0).map(|n| (n * EMU_PER_POINT).round() as i64).ok_or_else(|| ToolError::invalid(format!("{name} is a positive number of points"))),
        }
    };
    let size = EmuSize::new(dim("width", CHART_SIZE.width)?, dim("height", CHART_SIZE.height)?);
    let series_in = match args.opt_str("series_in")? {
        None => None,
        Some("columns") => Some(true),
        Some("rows") => Some(false),
        Some(_) => return Err(ToolError::invalid("series_in is columns or rows")),
    };
    let rows: Option<Vec<Vec<Value>>> = match args.get("data") {
        None | Some(Value::Null) => None,
        Some(Value::Array(list)) => {
            let rows: Vec<Vec<Value>> = list.iter().map(|r| r.as_array().cloned().ok_or_else(|| ToolError::invalid("data is rows (arrays) of cells"))).collect::<Result<_, _>>()?;
            if rows.len() < 2 || rows.iter().any(|r| r.len() < 2) {
                return Err(ToolError::invalid("data is at least 2 rows of at least 2 cells"));
            }
            if rows.len() > 1_000 || rows.iter().any(|r| r.len() > 100) {
                return Err(ToolError::new("out_of_range", "data is at most 1000 rows by 100 columns"));
            }
            Some(rows)
        }
        Some(_) => return Err(ToolError::invalid("data is rows (arrays) of cells")),
    };
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let at = flow_pos(flow, args.get("at").unwrap_or(&Value::Null), "at")?;
    let shape_id = ShapeId(flow.new_shape_id());
    let current = open.document.presentation().clone();
    let shape = open.source.new_doc_chart(&current, at, shape_id, size, kind).map_err(|e| ToolError::new("unsupported_operation", e.to_string()))?;
    let ShapeKind::Chart(sample) = &shape.kind else { return Err(ToolError::new("edit_rejected", "the new chart is not a chart")) };
    let mut chart: Chart = (**sample).clone();
    let scatter = matches!(kind, NewChartKind::Scatter | NewChartKind::ScatterSmooth | NewChartKind::ScatterLines);
    let mut kept = None;
    if let Some(rows) = &rows {
        let (made, data) = workbook_of(rows, scatter, series_in, &chart)?;
        chart = made;
        kept = Some(data);
    }
    if let Some(t) = args.get("title") {
        title_of(&mut chart, t)?;
    }
    if let Some(l) = args.get("legend") {
        legend_of(&mut chart, l)?;
    }
    let changed = chart != **sample;
    // The data is kept for the chart saving writes (as the window's data panel does), then the chart is put in.
    if let Some(data) = kept {
        open.source.set_chart_data(shape_id, chart.clone(), data);
    }
    let mut commands = vec![FlowCommand::InsertChart { at, shape: Box::new(Shape { floating: None, ..shape }) }];
    if changed {
        commands.push(FlowCommand::SetChart { at, chart: Box::new(chart) });
    }
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({ "chart": shape_id.0 }), Vec::new()))
}

fn set_table_of_contents(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "paragraph", "from", "to", "replace"])?;
    let id = args.str_arg("document_id")?;
    let n = args.usize_arg("paragraph")?;
    let level = |name: &str, default: u32| -> Result<u32, ToolError> {
        match args.opt_usize(name)? {
            None => Ok(default),
            Some(v) if (1..=9).contains(&v) => Ok(v as u32),
            Some(_) => Err(ToolError::invalid(format!("{name} is a heading level, 1 to 9"))),
        }
    };
    let (from, to) = (level("from", 1)?, level("to", 3)?);
    if from > to {
        return Err(ToolError::invalid("from is after to"));
    }
    let replace = args.opt_bool("replace")?.unwrap_or(false);
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let paragraphs = body_paragraphs(flow);
    let before = paragraphs.get(n).ok_or_else(|| ToolError::new("out_of_range", format!("paragraph {n}: the body has {} paragraphs", paragraphs.len())))?.id;
    if replace && paragraphs[n].field_start.is_none() {
        return Err(ToolError::invalid("replace: the paragraph is not the first of a table of contents"));
    }
    let width = flow.page.size.width - flow.page.left - flow.page.right;
    let instruction = TocInstruction { heading_levels: Some((from, to)), ..Default::default() };
    let (mut entries, bookmarks) = flow_command::toc_entries_with_instruction(flow, &instruction, &|_| None, flow.new_id(), width);
    if entries.is_empty() {
        return Err(ToolError::new("unsupported_operation", format!("the document has no heading of levels {from} to {to} to make a table of contents of")));
    }
    // The page numbers come from laying the pages out, which this crate cannot: Word and the window make the field again.
    if let Some(first) = entries.first_mut() {
        first.field_dirty = true;
    }
    let mut commands: Vec<FlowCommand> = flow_command::missing_toc_styles_for_levels(&flow.styles, instruction.all_levels()).into_iter().map(|(id, s)| FlowCommand::SetStyle { id, style: Some(s) }).collect();
    commands.push(FlowCommand::SetTableOfContents { before, replace, entries, bookmarks });
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({}), vec![json!({ "code": "no_page_numbers", "message": "the table of contents has no page numbers yet: Word makes them when it opens the file, and the window's 目次の更新 does" })]))
}

fn set_picture_wrap(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at", "wrap"])?;
    let id = args.str_arg("document_id")?;
    let wrap = match args.str_arg("wrap")? {
        "inline" => PictureWrapType::Inline,
        "square" => PictureWrapType::Square,
        "tight" => PictureWrapType::Tight,
        "through" => PictureWrapType::Through,
        "top_and_bottom" => PictureWrapType::TopAndBottom,
        "behind_text" => PictureWrapType::BehindText,
        "in_front_of_text" => PictureWrapType::InFrontOfText,
        other => return Err(ToolError::invalid(format!("wrap {other} is not inline, square, tight, through, top_and_bottom, behind_text or in_front_of_text"))),
    };
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let at: FlowPos = flow_pos(flow, args.get("at").unwrap_or(&Value::Null), "at")?;
    // The run at the position must be a picture or a shape: the command turns a picture into a floating shape and changes a shape's wrap.
    let paragraph = body_paragraphs(flow).into_iter().find(|p| p.id == at.para).ok_or_else(|| ToolError::new("out_of_range", "the position is not in a body paragraph"))?;
    let mut offset = 0usize;
    let mut found = false;
    for r in &paragraph.runs {
        if matches!(r.kind, FlowRunKind::Picture { .. } | FlowRunKind::Shape(_)) && offset == at.offset {
            found = true;
        }
        offset += r.len();
    }
    if !found {
        return Err(ToolError::new("out_of_range", "there is no picture or shape at that position: see docx_get_paragraph (pictures) and docx_list_shapes"));
    }
    execute(open, vec![Command::Flow(FlowCommand::SetPictureWrap { at, wrap })])?;
    Ok(edited(id, open, json!({}), Vec::new()))
}
