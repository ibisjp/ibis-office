// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `docx_*` structure editing of stage B step 3 (`tasks/doc/design/2026-11-24-159-mcp-stage-b.md` 4 and 8): lists, and the rows, columns and format of a table.
//! Each is read back by `docx_get_paragraph` (`list`) and `docx_get_table`; what is not given is not changed; one call is one undo step.

use ibis_doc_model::flow::{FlowAlign, FlowTable, TableLook};
use ibis_doc_model::flow_command::{FlowCommand, ParaChange, TableEdit};
use ibis_doc_model::{Command, Rgb};
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, document_id, integer, object};
use crate::edit::{ColorArg, edited, execute, only_known, parse_bool, parse_color};
use crate::error::ToolError;
use crate::service::{Args, Service};
use crate::tools_docx::{body_paragraphs, body_tables, flow_of};

const EMU_PER_POINT: f64 = 12_700.0;

fn at_count(description: &str) -> [(&'static str, Value); 2] {
    [("at", integer(description)), ("count", json!({ "type": "integer", "minimum": 1, "maximum": 1000, "description": "How many (default 1)." }))]
}

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "docx_set_numbering",
            description: "Make body paragraphs items of a bulleted or numbered list, or take them out of a list (kind none). format (numbers): decimal, lower_letter, upper_letter, lower_roman, upper_roman; start: the first number. level (0 to 8) is the item's level. Read it back with docx_get_paragraph (list).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("paragraphs", json!({ "type": "array", "minItems": 1, "items": { "type": "integer", "minimum": 0 }, "description": "Paragraph numbers." })),
                        ("kind", json!({ "enum": ["none", "bullet", "number"] })),
                        ("format", json!({ "enum": ["decimal", "lower_letter", "upper_letter", "lower_roman", "upper_roman"] })),
                        ("start", json!({ "type": "integer", "minimum": 0 })),
                        ("level", json!({ "type": "integer", "minimum": 0, "maximum": 8 })),
                    ],
                    &["document_id", "paragraphs", "kind"],
                )
            },
            run: set_numbering,
        },
        Tool {
            name: "docx_insert_table_rows",
            description: "Insert rows like row `at` (empty cells) before or after it (position, default after) in a table (the number from docx_list_tables).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                let [at, count] = at_count("The row (0-based) the new ones are like and next to.");
                object(&[document_id(), ("table", integer("The table's number (0-based).")), at, count, ("position", json!({ "enum": ["before", "after"] }))], &["document_id", "table", "at"])
            },
            run: insert_table_rows,
        },
        Tool {
            name: "docx_delete_table_rows",
            description: "Delete `count` rows from row `at` of a table. To delete every row use docx_delete_table.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                let [at, count] = at_count("The first row (0-based).");
                object(&[document_id(), ("table", integer("The table's number (0-based).")), at, count], &["document_id", "table", "at"])
            },
            run: delete_table_rows,
        },
        Tool {
            name: "docx_insert_table_columns",
            description: "Insert columns like grid column `at` before or after it (position, default after); the columns are narrowed to keep the table's width.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                let [at, count] = at_count("The grid column (0-based) the new ones are like and next to.");
                object(&[document_id(), ("table", integer("The table's number (0-based).")), at, count, ("position", json!({ "enum": ["before", "after"] }))], &["document_id", "table", "at"])
            },
            run: insert_table_columns,
        },
        Tool {
            name: "docx_delete_table_columns",
            description: "Delete `count` grid columns from column `at` of a table. To delete every column use docx_delete_table.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                let [at, count] = at_count("The first grid column (0-based).");
                object(&[document_id(), ("table", integer("The table's number (0-based).")), at, count], &["document_id", "table", "at"])
            },
            run: delete_table_columns,
        },
        Tool {
            name: "docx_delete_table",
            description: "Delete a table (the number from docx_list_tables).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("table", integer("The table's number (0-based)."))], &["document_id", "table"]),
            run: delete_table,
        },
        Tool {
            name: "docx_merge_table_cells",
            description: "Merge the cell at row `row`, cell `cell` (its index in the row) with the `count` cells to its right (default 1) into one cell spanning their grid columns; their paragraphs are joined. A cell in a vertical merge cannot be merged. Read it back with docx_get_table (cells: span).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("table", integer("The table's number (0-based).")), ("row", integer("The row (0-based).")), ("cell", integer("The cell's index in the row (0-based).")), ("count", json!({ "type": "integer", "minimum": 1, "maximum": 62, "description": "How many cells to the right (default 1)." }))], &["document_id", "table", "row", "cell"]),
            run: merge_table_cells,
        },
        Tool {
            name: "docx_split_table_cell",
            description: "Split a cell that spans several grid columns into one cell for each (the first keeps the text). Read it back with docx_get_table.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("table", integer("The table's number (0-based).")), ("row", integer("The row (0-based).")), ("cell", integer("The cell's index in the row (0-based)."))], &["document_id", "table", "row", "cell"]),
            run: split_table_cell,
        },
        Tool {
            name: "docx_set_table_format",
            description: "Set the format of a table: alignment (left|center|right), column_widths (points, one per grid column), style_options ({ first_row, last_row, first_column, last_column, banded_rows, banded_columns } booleans of the table style), cell_fill ({ cells: [{ row, cell }], color: \"#RRGGBB\" or null for no fill }; cell is the index in the row). What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("table", integer("The table's number (0-based).")),
                        ("alignment", json!({ "enum": ["left", "center", "right"] })),
                        ("column_widths", json!({ "type": "array", "items": { "type": "number", "exclusiveMinimum": 0 }, "description": "Points, one per grid column." })),
                        ("style_options", json!({ "type": "object", "properties": { "first_row": { "type": "boolean" }, "last_row": { "type": "boolean" }, "first_column": { "type": "boolean" }, "last_column": { "type": "boolean" }, "banded_rows": { "type": "boolean" }, "banded_columns": { "type": "boolean" } } })),
                        ("cell_fill", json!({ "type": "object", "properties": { "cells": { "type": "array", "items": { "type": "object", "properties": { "row": { "type": "integer", "minimum": 0 }, "cell": { "type": "integer", "minimum": 0 } }, "required": ["row", "cell"] } }, "color": { "type": ["string", "null"], "pattern": "^#[0-9A-Fa-f]{6}$" } }, "required": ["cells", "color"] })),
                    ],
                    &["document_id", "table"],
                )
            },
            run: set_table_format,
        },
    ]
}

fn numbering_format(name: &str) -> Option<&'static str> {
    Some(match name {
        "decimal" => "decimal",
        "lower_letter" => "lowerLetter",
        "upper_letter" => "upperLetter",
        "lower_roman" => "lowerRoman",
        "upper_roman" => "upperRoman",
        _ => return None,
    })
}

fn set_numbering(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "paragraphs", "kind", "format", "start", "level"])?;
    let id = args.str_arg("document_id")?;
    let list = args.get("paragraphs").and_then(Value::as_array).filter(|l| !l.is_empty()).ok_or_else(|| ToolError::invalid("paragraphs is a list of paragraph numbers"))?;
    let kind = args.str_arg("kind")?;
    let format = args.opt_str("format")?;
    let start = args.opt_usize("start")?;
    let level = args.opt_usize("level")?;
    if !matches!(kind, "none" | "bullet" | "number") {
        return Err(ToolError::invalid(format!("kind {kind} is not none, bullet or number")));
    }
    if kind != "number" && (format.is_some() || start.is_some()) {
        return Err(ToolError::invalid("format and start are for kind number"));
    }
    if kind == "none" && level.is_some() {
        return Err(ToolError::invalid("level is for a list item"));
    }
    let format = format.map(|f| numbering_format(f).ok_or_else(|| ToolError::invalid(format!("format {f} is not decimal, lower_letter, upper_letter, lower_roman or upper_roman")))).transpose()?;
    if level.is_some_and(|l| l > 8) {
        return Err(ToolError::invalid("level is 0 to 8"));
    }
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let paragraphs = body_paragraphs(flow);
    let mut targets = Vec::new();
    for item in list {
        let n = item.as_u64().ok_or_else(|| ToolError::invalid("paragraphs is a list of paragraph numbers"))? as usize;
        targets.push(paragraphs.get(n).ok_or_else(|| ToolError::new("out_of_range", format!("paragraph {n}: the body has {} paragraphs", paragraphs.len())))?.id);
    }
    let mut commands = Vec::new();
    let change = if kind == "none" {
        ParaChange::List(None)
    } else {
        let bullet = kind == "bullet";
        let numbering = &flow.styles.numbering;
        let wants_new = format.is_some() || start.is_some();
        let num_id = match numbering.find(bullet).filter(|_| !wants_new) {
            Some(existing) => existing,
            None => {
                let (mut with, new_id) = numbering.with_new_list(bullet);
                if wants_new {
                    let abstract_id = with.nums.get(&new_id).map(|n| n.abstract_id).ok_or_else(|| ToolError::new("edit_rejected", "the new list has no definition"))?;
                    if let Some(first) = with.abstracts.get_mut(&abstract_id).and_then(|a| a.levels.first_mut()) {
                        if let Some(f) = format {
                            first.format = f.to_string();
                            first.text = "%1.".to_string();
                        }
                        if let Some(s) = start {
                            first.start = s as u32;
                        }
                    }
                }
                commands.push(FlowCommand::SetNumbering { numbering: with });
                new_id
            }
        };
        ParaChange::List(Some(num_id))
    };
    for para in &targets {
        commands.push(FlowCommand::SetParagraphFormat { first: *para, last: *para, change: change.clone() });
        if let Some(l) = level {
            commands.push(FlowCommand::SetParagraphFormat { first: *para, last: *para, change: ParaChange::Level(l as u32) });
        }
    }
    let count = targets.len();
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({ "paragraphs": count }), Vec::new()))
}

/// The table `n` of the body: its id, the number of rows and of grid columns.
fn table_of(service: &mut Service, args: &Value) -> Result<(u32, usize, usize), ToolError> {
    let n = args.usize_arg("table")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let tables = body_tables(flow);
    let t: &FlowTable = tables.get(n).ok_or_else(|| ToolError::new("out_of_range", format!("table {n}: the body has {} tables", tables.len())))?;
    Ok((t.id, t.rows.len(), t.columns.len()))
}

fn count_of(args: &Value) -> Result<usize, ToolError> {
    Ok(args.opt_usize("count")?.unwrap_or(1).clamp(1, 1000))
}

fn run_table_edits(service: &mut Service, args: &Value, table: u32, edits: Vec<TableEdit>, extra: Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let commands: Vec<FlowCommand> = edits.into_iter().map(|edit| FlowCommand::EditTable { table, edit }).collect();
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, extra, Vec::new()))
}

fn position_below(args: &Value) -> Result<bool, ToolError> {
    match args.opt_str("position")? {
        None | Some("after") => Ok(true),
        Some("before") => Ok(false),
        Some(other) => Err(ToolError::invalid(format!("position {other} is not before or after"))),
    }
}

fn insert_table_rows(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "table", "at", "count", "position"])?;
    let (table, rows, _) = table_of(service, args)?;
    let (at, count, below) = (args.usize_arg("at")?, count_of(args)?, position_below(args)?);
    if at >= rows {
        return Err(ToolError::new("out_of_range", format!("row {at}: the table has {rows} rows")));
    }
    // Each insert goes next to the same row, so the new ones come out in a row.
    run_table_edits(service, args, table, (0..count).map(|_| TableEdit::InsertRow { row: at, below }).collect(), json!({ "inserted": count }))
}

fn delete_table_rows(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "table", "at", "count"])?;
    let (table, rows, _) = table_of(service, args)?;
    let (at, count) = (args.usize_arg("at")?, count_of(args)?);
    if at >= rows || at + count > rows {
        return Err(ToolError::new("out_of_range", format!("rows {at} to {}: the table has {rows} rows", at + count - 1)));
    }
    if count == rows {
        return Err(ToolError::invalid("these are every row: use docx_delete_table"));
    }
    run_table_edits(service, args, table, vec![TableEdit::DeleteRows { first: at, last: at + count - 1 }], json!({ "deleted": count }))
}

fn insert_table_columns(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "table", "at", "count", "position"])?;
    let (table, _, columns) = table_of(service, args)?;
    let (at, count, right) = (args.usize_arg("at")?, count_of(args)?, position_below(args)?);
    if at >= columns {
        return Err(ToolError::new("out_of_range", format!("column {at}: the table has {columns} grid columns")));
    }
    run_table_edits(service, args, table, (0..count).map(|_| TableEdit::InsertColumn { column: at, right }).collect(), json!({ "inserted": count }))
}

fn delete_table_columns(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "table", "at", "count"])?;
    let (table, _, columns) = table_of(service, args)?;
    let (at, count) = (args.usize_arg("at")?, count_of(args)?);
    if at >= columns || at + count > columns {
        return Err(ToolError::new("out_of_range", format!("columns {at} to {}: the table has {columns} grid columns", at + count - 1)));
    }
    if count == columns {
        return Err(ToolError::invalid("these are every column: use docx_delete_table"));
    }
    // Each delete takes the column now at `at`.
    run_table_edits(service, args, table, (0..count).map(|_| TableEdit::DeleteColumn { column: at }).collect(), json!({ "deleted": count }))
}

fn delete_table(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "table"])?;
    let (table, _, _) = table_of(service, args)?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    execute(open, vec![Command::Flow(FlowCommand::DeleteTable { table })])?;
    Ok(edited(id, open, json!({}), Vec::new()))
}

fn set_table_format(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "table", "alignment", "column_widths", "style_options", "cell_fill"])?;
    let n = args.usize_arg("table")?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let tables = body_tables(flow);
    let t = tables.get(n).ok_or_else(|| ToolError::new("out_of_range", format!("table {n}: the body has {} tables", tables.len())))?;
    let table = t.id;
    let mut edits = Vec::new();
    if let Some(a) = args.opt_str("alignment")? {
        edits.push(TableEdit::Align(match a {
            "left" => FlowAlign::Left,
            "center" => FlowAlign::Center,
            "right" => FlowAlign::Right,
            other => return Err(ToolError::invalid(format!("alignment {other} is not left, center or right"))),
        }));
    }
    if let Some(widths) = args.get("column_widths").filter(|v| !v.is_null()) {
        let list = widths.as_array().ok_or_else(|| ToolError::invalid("column_widths is a list of points"))?;
        if list.len() != t.columns.len() {
            return Err(ToolError::invalid(format!("column_widths has {} values; the table has {} grid columns", list.len(), t.columns.len())));
        }
        for (column, w) in list.iter().enumerate() {
            let w = w.as_f64().filter(|w| w.is_finite() && *w > 0.0 && *w < 100_000.0).ok_or_else(|| ToolError::invalid("column_widths are positive numbers of points"))?;
            edits.push(TableEdit::ColumnWidth { column, width: (w * EMU_PER_POINT).round() as i64 });
        }
    }
    if let Some(options) = args.get("style_options").filter(|v| !v.is_null()) {
        let mut look: TableLook = t.style.as_ref().map(|s| s.look).ok_or_else(|| ToolError::new("unsupported_operation", "the table has no table style: style_options are its options"))?;
        for (name, slot) in [("first_row", &mut look.first_row), ("last_row", &mut look.last_row), ("first_column", &mut look.first_column), ("last_column", &mut look.last_column)] {
            if let Some(v) = options.get(name) {
                *slot = parse_bool(&format!("style_options.{name}"), v)?;
            }
        }
        // The look says whether the bands are off.
        if let Some(v) = options.get("banded_rows") {
            look.no_hband = !parse_bool("style_options.banded_rows", v)?;
        }
        if let Some(v) = options.get("banded_columns") {
            look.no_vband = !parse_bool("style_options.banded_columns", v)?;
        }
        edits.push(TableEdit::Look(look));
    }
    if let Some(fill) = args.get("cell_fill").filter(|v| !v.is_null()) {
        let bad = || ToolError::invalid("cell_fill is { cells: [{ row, cell }], color: \"#RRGGBB\" or null }");
        let cells_json = fill.get("cells").and_then(Value::as_array).filter(|c| !c.is_empty()).ok_or_else(bad)?;
        let mut cells = Vec::new();
        for c in cells_json {
            let (row, cell) = (c.get("row").and_then(Value::as_u64).ok_or_else(bad)? as usize, c.get("cell").and_then(Value::as_u64).ok_or_else(bad)? as usize);
            let r = t.rows.get(row).ok_or_else(|| ToolError::new("out_of_range", format!("row {row}: the table has {} rows", t.rows.len())))?;
            if cell >= r.cells.len() {
                return Err(ToolError::new("out_of_range", format!("cell {cell}: row {row} has {} cells", r.cells.len())));
            }
            cells.push((row, cell));
        }
        let color: Option<Rgb> = match parse_color("cell_fill.color", fill.get("color").ok_or_else(bad)?)? {
            ColorArg::Rgb(c) => Some(c),
            ColorArg::Inherit => None,
            ColorArg::Theme { .. } => return Err(ToolError::invalid("cell_fill.color is \"#RRGGBB\" or null")),
        };
        edits.push(TableEdit::Shading { cells, color });
    }
    if edits.is_empty() {
        return Err(ToolError::invalid("give at least one of alignment, column_widths, style_options, cell_fill"));
    }
    let commands: Vec<FlowCommand> = edits.into_iter().map(|edit| FlowCommand::EditTable { table, edit }).collect();
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({}), Vec::new()))
}

/// The table `table` of the body as the model's copy and where it is: the id's place, for replacing the table with an edited one.
fn replace_table(service: &mut Service, args: &Value, edit: impl FnOnce(&ibis_doc_model::flow::FlowDocument, &FlowTable) -> Result<FlowTable, ToolError>) -> Result<Value, ToolError> {
    let n = args.usize_arg("table")?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let tables = body_tables(flow);
    let t = tables.get(n).ok_or_else(|| ToolError::new("out_of_range", format!("table {n}: the body has {} tables", tables.len())))?;
    let edited_table = edit(flow, t)?;
    let (container, index) = ibis_doc_model::flow_command::locate(flow, t.id).ok_or_else(|| ToolError::new("edit_rejected", "the table is not in the document"))?;
    let command = FlowCommand::ReplaceBlockRange { container, index, remove: 1, blocks: vec![ibis_doc_model::flow::Block::Table(edited_table)] };
    execute(open, vec![Command::Flow(command)])?;
    Ok(edited(id, open, json!({}), Vec::new()))
}

fn cell_args(args: &Value) -> Result<(usize, usize), ToolError> {
    Ok((args.usize_arg("row")?, args.usize_arg("cell")?))
}

fn merge_table_cells(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "table", "row", "cell", "count"])?;
    let (row, cell) = cell_args(args)?;
    let count = args.opt_usize("count")?.unwrap_or(1).clamp(1, 62);
    replace_table(service, args, |_, t| {
        let r = t.rows.get(row).ok_or_else(|| ToolError::new("out_of_range", format!("row {row}: the table has {} rows", t.rows.len())))?;
        if cell >= r.cells.len() {
            return Err(ToolError::new("out_of_range", format!("cell {cell}: row {row} has {} cells", r.cells.len())));
        }
        if cell + count >= r.cells.len() {
            return Err(ToolError::new("out_of_range", format!("cells {cell} to {}: row {row} has {} cells", cell + count, r.cells.len())));
        }
        let mut table = t.clone();
        for _ in 0..count {
            table = ibis_doc_model::flow_command::merge_cells_right(&table, row, cell).map_err(|why| ToolError::new("unsupported_operation", why))?;
        }
        Ok(table)
    })
}

fn split_table_cell(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "table", "row", "cell"])?;
    let (row, cell) = cell_args(args)?;
    replace_table(service, args, |flow, t| {
        let r = t.rows.get(row).ok_or_else(|| ToolError::new("out_of_range", format!("row {row}: the table has {} rows", t.rows.len())))?;
        if cell >= r.cells.len() {
            return Err(ToolError::new("out_of_range", format!("cell {cell}: row {row} has {} cells", r.cells.len())));
        }
        ibis_doc_model::flow_command::split_cell(flow, t, row, cell).map_err(|why| ToolError::new("unsupported_operation", why))
    })
}
