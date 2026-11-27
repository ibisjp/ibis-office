// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `xlsx_*` tools (design 6.3). A sheet is named by its tab; a range is A1 text.

use ibis_calc::CellValue;
use ibis_doc_model::ShapeKind;
use ibis_doc_model::sheet::{CellRange, Sheet};
use ibis_formats::Kind;
use serde_json::{Value, json};

use crate::a1::{column_name, parse_range, range_text};
use crate::catalog::{Access, Tool, boolean, document_id, integer, object, string};
use crate::error::ToolError;
use crate::service::{Args, Open, Service};

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "xlsx_list_sheets", description: "The sheets of the workbook in order, hidden ones too: name, state (visible, hidden, very_hidden), used range, whether the sheet is protected, and whether the workbook structure is locked.", access: Access::Read, returns_content: false, schema: &|| object(&[document_id()], &["document_id"]), run: list_sheets },
        Tool { name: "xlsx_get_used_range", description: "The range of a sheet that holds anything, and its hidden rows and columns.", access: Access::Read, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("The sheet's name."))], &["document_id", "sheet"]), run: get_used_range },
        Tool {
            name: "xlsx_get_cells",
            description: "The cells of a range, row by row: value (calculated), formula, or text (as shown). Formula mode returns the calculated value when sheet protection hides the formula. Paged by rows.",
            access: Access::Read,
            returns_content: true,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("sheet", string("The sheet's name.")),
                        ("range", string("A1 range, e.g. B2:D10.")),
                        ("kind", json!({ "enum": ["value", "formula", "text", "format"], "description": "value (default), formula, text (as shown) or format (the cell's format: font, colours, borders, alignment...)." })),
                        ("offset", integer("The first row of the page (0-based, from the range's first row).")),
                        ("limit", integer("The most rows.")),
                    ],
                    &["document_id", "sheet", "range"],
                )
            },
            run: get_cells,
        },
        Tool {
            name: "xlsx_find_text",
            description: "The cells whose shown text has a text.",
            access: Access::Read,
            returns_content: true,
            schema: &|| object(&[document_id(), ("query", string("The text to find.")), ("sheet", string("Only this sheet.")), ("match_case", boolean("Match upper and lower case (default false).")), ("limit", integer("The most cells."))], &["document_id", "query"]),
            run: find_text,
        },
    ]
}

/// The sheets of the open workbook: `(slide index, name, sheet)`.
pub(crate) fn sheets_of(open: &Open) -> Result<Vec<(usize, String, &Sheet)>, ToolError> {
    if open.kind != Kind::Workbook {
        return Err(ToolError::new("unsupported_operation", "this tool is for an xlsx workbook"));
    }
    Ok(open
        .document
        .presentation()
        .slides
        .iter()
        .enumerate()
        .filter_map(|(i, slide)| {
            slide.shapes.iter().find_map(|s| if let ShapeKind::Sheet(sheet) = &s.kind { Some((i, slide.name.clone().unwrap_or_else(|| format!("Sheet{}", i + 1)), &**sheet)) } else { None })
        })
        .collect())
}

pub(crate) fn find_sheet<'a>(sheets: &'a [(usize, String, &'a Sheet)], name: &str) -> Result<&'a (usize, String, &'a Sheet), ToolError> {
    sheets.iter().find(|(_, n, _)| n == name).or_else(|| sheets.iter().find(|(_, n, _)| n.eq_ignore_ascii_case(name))).ok_or_else(|| ToolError::new("no_such_sheet", format!("no sheet named {name}")))
}

fn used_range(sheet: &Sheet) -> Option<CellRange> {
    let mut cells = sheet.cells.iter().filter(|c| !matches!(c.value, CellValue::Empty) || c.formula.is_some());
    let first = cells.next()?;
    let mut r = CellRange { first_row: first.row, first_column: first.column, last_row: first.row, last_column: first.column };
    for c in cells {
        r.first_row = r.first_row.min(c.row);
        r.first_column = r.first_column.min(c.column);
        r.last_row = r.last_row.max(c.row);
        r.last_column = r.last_column.max(c.column);
    }
    Some(r)
}

pub(crate) fn value_json(value: &CellValue) -> Value {
    match value {
        CellValue::Empty => Value::Null,
        CellValue::Number(n) => serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number),
        CellValue::Text(t) => Value::String(t.clone()),
        CellValue::Bool(b) => Value::Bool(*b),
        CellValue::Error(e) => json!({ "error": format!("{e:?}") }),
    }
}

fn list_sheets(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let visible = sheets_of(open)?;
    let presentation = open.document.presentation();
    let mut sheets: Vec<Value> = Vec::new();
    match presentation.workbook.as_ref() {
        // Every sheet of the workbook in its order, the hidden ones too.
        Some(workbook) if !workbook.sheets.is_empty() => {
            for ws in &workbook.sheets {
                let shown = ws.slide.and_then(|id| visible.iter().find(|(i, _, _)| presentation.slides[*i].id == id)).map(|(_, _, s)| *s);
                let sheet: Option<&Sheet> = shown.or_else(|| ws.hidden.as_deref());
                let state = match ws.state {
                    ibis_doc_model::sheet::SheetState::Visible => "visible",
                    ibis_doc_model::sheet::SheetState::Hidden => "hidden",
                    ibis_doc_model::sheet::SheetState::VeryHidden => "very_hidden",
                };
                sheets.push(json!({
                    "name": ws.name,
                    "state": state,
                    "worksheet": !ws.other,
                    "used_range": sheet.and_then(used_range).map(|r| range_text(&r)),
                    "cells": sheet.map_or(0, |s| s.cells.len()),
                    "protected": sheet.is_some_and(|s| s.protection.is_some()),
                    "allowed": sheet.and_then(|s| s.protection.as_ref()).map(|p| crate::tools_xlsx_set::allowed_json(&p.allowed)),
                }));
            }
        }
        _ => {
            for (_, name, sheet) in &visible {
                sheets.push(json!({ "name": name, "state": "visible", "worksheet": true, "used_range": used_range(sheet).map(|r| range_text(&r)), "cells": sheet.cells.len(), "protected": sheet.protection.is_some() }));
            }
        }
    }
    let structure_locked = presentation.workbook.as_ref().and_then(|w| w.protection.as_ref()).is_some_and(|p| p.lock_structure);
    Ok(json!({ "sheets": sheets, "workbook_protected": structure_locked }))
}

/// Runs of consecutive numbers as `"3:5"`, a single one as `"9"`.
fn spans(numbers: &[u32], name: impl Fn(u32) -> String) -> Vec<String> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < numbers.len() {
        let mut j = i;
        while j + 1 < numbers.len() && numbers[j + 1] == numbers[j] + 1 {
            j += 1;
        }
        out.push(if i == j { name(numbers[i]) } else { format!("{}:{}", name(numbers[i]), name(numbers[j])) });
        i = j + 1;
    }
    out
}

fn get_used_range(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let name = args.str_arg("sheet")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let sheets = sheets_of(open)?;
    let (_, name, sheet) = find_sheet(&sheets, name)?;
    let hidden_rows: Vec<u32> = (0..sheet.rows.len() as u32).filter(|r| sheet.rows[*r as usize] == 0 || sheet.hidden_rows.contains_key(r)).collect();
    let hidden_columns: Vec<u32> = (0..sheet.columns.len() as u32).filter(|c| sheet.columns[*c as usize] == 0 || sheet.hidden_columns.contains_key(c)).collect();
    Ok(json!({ "sheet": name, "used_range": used_range(sheet).map(|r| range_text(&r)), "hidden_rows": spans(&hidden_rows, |n| (n + 1).to_string()), "hidden_columns": spans(&hidden_columns, column_name) }))
}

fn get_cells(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let range = parse_range(args.str_arg("range")?)?;
    let kind = args.opt_str("kind")?.unwrap_or("value");
    if !matches!(kind, "value" | "formula" | "text" | "format") {
        return Err(ToolError::invalid(format!("kind {kind} is not value, formula, text or format")));
    }
    let name = args.str_arg("sheet")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let sheets = sheets_of(open)?;
    let (_, name, sheet) = find_sheet(&sheets, name)?;
    let width = (range.last_column - range.first_column + 1) as usize;
    let height = (range.last_row - range.first_row + 1) as usize;
    let offset = args.opt_usize("offset")?.unwrap_or(0);
    if offset > height {
        return Err(ToolError::new("out_of_range", format!("offset {offset} is past the {height} rows")));
    }
    let limit = args.opt_usize("limit")?.unwrap_or((10_000 / width).max(1)).max(1);
    let end = (offset + limit).min(height);
    let mut rows: Vec<Vec<Value>> = vec![vec![Value::Null; width]; end - offset];
    for cell in sheet.cells.iter() {
        let (r, c) = (cell.row, cell.column);
        if r < range.first_row || r > range.last_row || c < range.first_column || c > range.last_column {
            continue;
        }
        let row = (r - range.first_row) as usize;
        if row < offset || row >= end {
            continue;
        }
        rows[row - offset][(c - range.first_column) as usize] = match kind {
            "formula" => sheet.visible_formula(cell)
                .map_or_else(|| value_json(&cell.value), |f| Value::String(format!("={f}"))),
            "text" => Value::String(cell.text.clone()),
            "format" => crate::tools_xlsx_read::format_json(&sheet.styles.get(cell.style as usize).cloned().unwrap_or_default()),
            _ => value_json(&cell.value),
        };
    }
    let mut result = json!({ "sheet": name, "range": range_text(&range), "kind": kind, "first_row": range.first_row + 1 + offset as u32, "first_column": column_name(range.first_column), "rows": rows });
    if end < height {
        result["next_offset"] = json!(end);
    }
    Ok(result)
}

fn find_text(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let query = args.str_arg("query")?;
    if query.is_empty() {
        return Err(ToolError::invalid("query is empty"));
    }
    let only = args.opt_str("sheet")?;
    let match_case = args.opt_bool("match_case")?.unwrap_or(false);
    let limit = args.opt_usize("limit")?.unwrap_or(100).max(1);
    let open = service.open_document(args.str_arg("document_id")?)?;
    let sheets = sheets_of(open)?;
    let fold = |s: &str| if match_case { s.to_string() } else { s.to_lowercase() };
    let needle = fold(query);
    let mut found = Vec::new();
    'outer: for (_, name, sheet) in &sheets {
        if only.is_some_and(|o| !o.eq_ignore_ascii_case(name)) {
            continue;
        }
        for cell in sheet.cells.iter() {
            if fold(&cell.text).contains(&needle) {
                found.push(json!({ "sheet": name, "cell": format!("{}{}", column_name(cell.column), cell.row + 1), "text": cell.text }));
                if found.len() >= limit {
                    break 'outer;
                }
            }
        }
    }
    Ok(json!({ "matches": found }))
}

// ---- edits ----

use ibis_doc_model::sheet::{CellAlign, CellVerticalAlign, ThemeColor};
use ibis_doc_model::sheet_command::{Axis, CellEdit, CellInput, SheetCommand, SortBy, StyleChange};
use ibis_doc_model::{Command, SlideId};

use crate::edit::{ColorArg, FontArg, edited, execute, only_known, parse_bool, parse_color, parse_font, parse_size};
use crate::tools_docx::{color_schema, font_schema};

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "xlsx_set_cells",
            description: "Put values in a range: rows of numbers, strings, true/false or null (clears the cell). A string starting with = followed by formula text is a formula (a string is otherwise text, as it is). A single value fills the whole range; otherwise the rows must be as large as the range. What depends on them is calculated again.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[document_id(), ("sheet", string("The sheet's name.")), ("range", string("A1 range; its first cell is where the values start.")), ("values", json!({ "description": "A value, or rows of values.", "oneOf": [{ "type": "array", "items": { "type": "array" } }, { "type": ["string", "number", "boolean", "null"] }] }))],
                    &["document_id", "sheet", "range", "values"],
                )
            },
            run: set_cells,
        },
        Tool {
            name: "xlsx_clear_cells",
            description: "Clear what is in a range: contents, formats, or all.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("sheet", string("The sheet's name.")), ("range", string("A1 range.")), ("what", json!({ "enum": ["contents", "formats", "all"] }))], &["document_id", "sheet", "range", "what"]),
            run: clear_cells,
        },
        Tool {
            name: "xlsx_set_format",
            description: "Set the format of a range: bold, italic, underline, strike, size (pt), color, font, fill, number_format (Excel's format code), horizontal_alignment, vertical_alignment, wrap_text, indent, text_rotation, shrink_to_fit, locked, hidden (formula hidden when the sheet is protected), style (a cell style from xlsx_list_cell_styles, applied first). Borders: xlsx_set_borders. What is not given is not changed; null takes a format back to the Normal style's.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("sheet", string("The sheet's name.")),
                        ("range", string("A1 range.")),
                        ("bold", boolean("Bold.")),
                        ("italic", boolean("Italic.")),
                        ("underline", json!({ "description": "true (single), false, or \"none\", \"single\", \"double\".", "oneOf": [{ "type": "boolean" }, { "enum": ["none", "single", "double"] }] })),
                        ("size", json!({ "type": "number", "description": "Points, 1 to 409.5 in steps of 0.5." })),
                        ("color", color_schema()),
                        ("font", font_schema()),
                        ("fill", color_schema()),
                        ("number_format", string("Excel's number format code, e.g. 0.00 or yyyy/m/d.")),
                        ("horizontal_alignment", json!({ "enum": ["general", "left", "center", "right", "justify"] })),
                        ("vertical_alignment", json!({ "enum": ["top", "center", "bottom", "justify"] })),
                        ("wrap_text", boolean("Wrap the text in the cell.")),
                        ("strike", boolean("Strike through.")),
                        ("indent", json!({ "type": "integer", "minimum": 0, "maximum": 15, "description": "Indent level." })),
                        ("text_rotation", json!({ "description": "Degrees from -90 (down) to 90 (up), or \"vertical\" (stacked letters).", "oneOf": [{ "type": "integer", "minimum": -90, "maximum": 90 }, { "enum": ["vertical"] }] })),
                        ("shrink_to_fit", boolean("Shrink the text to fit the cell's width.")),
                        ("locked", boolean("Locked while the sheet is protected.")),
                        ("hidden", boolean("The formula is hidden while the sheet is protected.")),
                        ("style", string("The name of a cell style (xlsx_list_cell_styles).")),
                    ],
                    &["document_id", "sheet", "range"],
                )
            },
            run: set_format,
        },
        Tool {
            name: "xlsx_add_sheet",
            description: "Add a sheet (at the end, or at index, 0-based).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("name", string("The new sheet's name.")), ("index", integer("Where it goes (0-based)."))], &["document_id", "name"]),
            run: add_sheet,
        },
        Tool { name: "xlsx_delete_sheet", description: "Delete a sheet (not the last one).", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("The sheet's name."))], &["document_id", "sheet"]), run: delete_sheet },
        Tool { name: "xlsx_rename_sheet", description: "Rename a sheet.", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("The sheet's name.")), ("name", string("The new name."))], &["document_id", "sheet", "name"]), run: rename_sheet },
        Tool { name: "xlsx_move_sheet", description: "Move a sheet to index (0-based).", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("The sheet's name.")), ("index", integer("The new place (0-based)."))], &["document_id", "sheet", "index"]), run: move_sheet },
        Tool { name: "xlsx_insert_rows", description: "Insert rows before row `at` (from 1); what is below moves down and the formulas follow.", access: Access::Edit, returns_content: false, schema: &|| structure_schema(true), run: insert_rows },
        Tool { name: "xlsx_insert_columns", description: "Insert columns before column `at` (a letter such as C, or a number from 1); what is on the right moves right and the formulas follow.", access: Access::Edit, returns_content: false, schema: &|| structure_schema(false), run: insert_columns },
        Tool { name: "xlsx_delete_rows", description: "Delete rows from row `at` (from 1); what is below moves up and the formulas follow.", access: Access::Edit, returns_content: false, schema: &|| structure_schema(true), run: delete_rows },
        Tool { name: "xlsx_delete_columns", description: "Delete columns from column `at` (a letter such as C, or a number from 1); what is on the right moves left and the formulas follow.", access: Access::Edit, returns_content: false, schema: &|| structure_schema(false), run: delete_columns },
        Tool {
            name: "xlsx_set_size",
            description: "Set the width of columns or the height of rows, in points (0 hides them).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("sheet", string("The sheet's name.")),
                        ("axis", json!({ "enum": ["column", "row"] })),
                        ("at", json!({ "description": "The first column (a letter such as C, or a number from 1) or row (from 1).", "type": ["string", "integer"] })),
                        ("count", json!({ "type": "integer", "minimum": 1, "description": "How many (default 1)." })),
                        ("size", json!({ "type": "number", "minimum": 0, "maximum": 1790, "description": "Points; 0 hides." })),
                    ],
                    &["document_id", "sheet", "axis", "at", "size"],
                )
            },
            run: set_size,
        },
        Tool { name: "xlsx_merge_cells", description: "Merge the cells of a range into one (the other cells' values are emptied); center puts the text in the middle.", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("The sheet's name.")), ("range", string("A1 range of at least two cells.")), ("center", boolean("Centre the text (default false)."))], &["document_id", "sheet", "range"]), run: merge_cells },
        Tool { name: "xlsx_unmerge_cells", description: "Split the merged cells of a range.", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("The sheet's name.")), ("range", string("A1 range."))], &["document_id", "sheet", "range"]), run: unmerge_cells },
        Tool {
            name: "xlsx_sort_range",
            description: "Sort the rows of a range by keys (column and order, the first is the main key). The formulas that point at the moved rows follow.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("sheet", string("The sheet's name.")),
                        ("range", string("A1 range of the rows to sort.")),
                        ("keys", json!({ "type": "array", "minItems": 1, "items": { "type": "object", "properties": { "column": { "description": "A column inside the range: a letter such as C, or a number from 1.", "type": ["string", "integer"] }, "order": { "enum": ["ascending", "descending"] } }, "required": ["column"] } })),
                        ("has_header", boolean("The range's first row is a heading and stays (default false).")),
                        ("match_case", boolean("Upper and lower case differ (default false).")),
                    ],
                    &["document_id", "sheet", "range", "keys"],
                )
            },
            run: sort_range,
        },
        Tool { name: "xlsx_calculate_workbook", description: "Calculate every formula of the workbook now.", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id()], &["document_id"]), run: calculate_workbook },
    ]
}

pub(crate) fn sheet_id(open: &Open, name: &str) -> Result<(SlideId, String), ToolError> {
    let sheets = sheets_of(open)?;
    let (index, name, _) = find_sheet(&sheets, name)?;
    Ok((open.document.presentation().slides[*index].id, name.clone()))
}

pub(crate) fn sheet_edit(service: &mut Service, id: &str, commands: Vec<SheetCommand>, extra: Value) -> Result<Value, ToolError> {
    let open = service.open_document(id)?;
    execute(open, commands.into_iter().map(Command::Sheet).collect())?;
    Ok(edited(id, open, extra, Vec::new()))
}

pub(crate) fn input_of(value: &Value) -> Result<CellInput, ToolError> {
    Ok(match value {
        Value::Null => CellInput::Clear,
        Value::Bool(b) => CellInput::Value(CellValue::Bool(*b)),
        Value::Number(n) => CellInput::Value(CellValue::Number(n.as_f64().ok_or_else(|| ToolError::invalid("a number is out of range"))?)),
        Value::String(s) => match ibis_calc::input::parse_program_text(s) {
            ibis_calc::input::Typed::Formula(formula) => CellInput::Formula(formula),
            ibis_calc::input::Typed::Value(value, _) => CellInput::Value(value),
        },
        _ => return Err(ToolError::invalid("a cell is a number, a string, true/false or null")),
    })
}

fn set_cells(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let range_arg = args.str_arg("range")?;
    let range = parse_range(range_arg)?;
    let rows = (range.last_row - range.first_row + 1) as usize;
    let columns = (range.last_column - range.first_column + 1) as usize;
    // The limit is checked from the range, before any cell is made (a range of a million rows with one value would otherwise be
    // expanded first).
    let count = rows.saturating_mul(columns);
    if count > 100_000 {
        return Err(ToolError::new("out_of_range", "more than 100000 cells in one call"));
    }
    let values = args.get("values").ok_or_else(|| ToolError::invalid("values is required"))?;
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    let mut edits = Vec::with_capacity(count);
    match values {
        Value::Array(grid) => {
            if grid.len() != rows {
                return Err(ToolError::new("out_of_range", format!("the range {range_arg} is {rows} rows by {columns} columns; values is {} rows", grid.len())));
            }
            for (r, row) in grid.iter().enumerate() {
                let row = row.as_array().ok_or_else(|| ToolError::invalid("values is rows (arrays) of cells"))?;
                if row.len() != columns {
                    return Err(ToolError::new("out_of_range", format!("row {} must have {columns} cells", r + 1)));
                }
                for (c, value) in row.iter().enumerate() {
                    edits.push(CellEdit { row: range.first_row + r as u32, column: range.first_column + c as u32, input: input_of(value)? });
                }
            }
        }
        single => {
            let input = input_of(single)?;
            for row in range.first_row..=range.last_row {
                for column in range.first_column..=range.last_column {
                    edits.push(CellEdit { row, column, input: input.clone() });
                }
            }
        }
    }
    sheet_edit(service, id, vec![SheetCommand::SetCells { sheet, edits }], json!({ "cells": count }))
}

fn clear_cells(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let what = args.str_arg("what")?;
    if !matches!(what, "contents" | "formats" | "all") {
        return Err(ToolError::invalid("what is contents, formats or all"));
    }
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    let mut commands = Vec::new();
    if what != "formats" {
        let mut edits = Vec::new();
        for cell in sheets_of(open)?.iter().find(|(i, _, _)| open.document.presentation().slides[*i].id == sheet).map(|(_, _, s)| s.cells.iter().cloned().collect::<Vec<_>>()).unwrap_or_default() {
            if cell.row >= range.first_row && cell.row <= range.last_row && cell.column >= range.first_column && cell.column <= range.last_column {
                edits.push(CellEdit { row: cell.row, column: cell.column, input: CellInput::Clear });
            }
        }
        if !edits.is_empty() {
            commands.push(SheetCommand::SetCells { sheet, edits });
        }
    }
    if what != "contents" {
        commands.push(SheetCommand::ClearFormats { sheet, ranges: vec![range] });
    }
    if commands.is_empty() {
        return Ok(edited(id, open, json!({ "cleared": 0 }), Vec::new()));
    }
    sheet_edit(service, id, commands, json!({}))
}

pub(crate) fn theme_of(name: &str) -> Result<u8, ToolError> {
    // SpreadsheetML's order: 0 lt1, 1 dk1, 2 lt2, 3 dk2, 4-9 accent1-6, 10 hlink, 11 folHlink.
    Ok(match name {
        "background1" => 0,
        "text1" => 1,
        "background2" => 2,
        "text2" => 3,
        "accent1" => 4,
        "accent2" => 5,
        "accent3" => 6,
        "accent4" => 7,
        "accent5" => 8,
        "accent6" => 9,
        "hyperlink" => 10,
        "followed_hyperlink" => 11,
        _ => return Err(ToolError::invalid(format!("{name} is not a theme colour"))),
    })
}

/// The workbook's Normal style: what a cell goes back to when a format is taken away (`null`, design 6.6).
pub(crate) fn normal_style(open: &Open) -> ibis_doc_model::CellStyle {
    open.document.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default()
}

pub(crate) fn color_change(open: &Open, arg: ColorArg, fill: bool) -> Result<StyleChange, ToolError> {
    Ok(match arg {
        ColorArg::Rgb(rgb) => {
            if fill {
                StyleChange::Fill(Some(rgb))
            } else {
                StyleChange::Color(rgb)
            }
        }
        ColorArg::Inherit => {
            // No fill; the text in the Normal style's colour.
            let normal = normal_style(open);
            match (fill, normal.color_theme) {
                (true, _) => StyleChange::Fill(None),
                (false, Some(theme)) => StyleChange::ColorTheme(normal.color, theme),
                (false, None) => StyleChange::Color(normal.color),
            }
        }
        ColorArg::Theme { name, brightness } => {
            // The tint is in millionths (800000: 80 % lighter, -250000: 25 % darker).
            let theme = ThemeColor { index: theme_of(&name)?, tint: (brightness * 1_000_000.0).round() as i32 };
            let rgb = theme.resolve(&open.document.presentation().theme_colors).unwrap_or(ibis_doc_model::Rgb::BLACK);
            if fill {
                StyleChange::FillTheme(rgb, theme)
            } else {
                StyleChange::ColorTheme(rgb, theme)
            }
        }
    })
}

fn set_format(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "bold", "italic", "underline", "size", "color", "font", "fill", "number_format", "horizontal_alignment", "vertical_alignment", "wrap_text", "strike", "indent", "text_rotation", "shrink_to_fit", "locked", "hidden", "style"])?;
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    let normal = normal_style(open);
    let theme_fonts = open.document.presentation().workbook.as_ref().and_then(|w| w.theme_fonts.clone());
    let mut changes = Vec::new();
    let mut warnings = Vec::new();
    // A named cell style first; what is given beside it wins.
    if let Some(v) = args.get("style") {
        let name = v.as_str().ok_or_else(|| ToolError::invalid("style is the name of a cell style"))?;
        let found = open.document.presentation().workbook.as_ref().and_then(|w| w.custom_cell_styles.iter().find(|s| s.name.eq_ignore_ascii_case(name)).cloned());
        let style = found.ok_or_else(|| ToolError::invalid(format!("no cell style named {name} (xlsx_list_cell_styles)")))?;
        changes.extend(style.to_style_changes());
    }
    // `null` takes a format back to the Normal style's.
    for (key, make, base) in [("bold", StyleChange::Bold as fn(bool) -> StyleChange, normal.bold), ("italic", StyleChange::Italic, normal.italic), ("underline", StyleChange::Underline, normal.underline != ibis_doc_model::CellUnderline::None), ("wrap_text", StyleChange::WrapText, normal.wrap), ("strike", StyleChange::Strikethrough, normal.strike), ("shrink_to_fit", StyleChange::ShrinkToFit, normal.shrink)] {
        if let Some(v) = args.get(key) {
            changes.push(match v {
                Value::Null => make(base),
                Value::String(s) if key == "underline" && matches!(s.as_str(), "none" | "single" | "double") => StyleChange::UnderlineKind(ibis_doc_model::CellUnderline::from_ooxml(s)),
                v => make(parse_bool(key, v)?),
            });
        }
    }
    if let Some(v) = args.get("locked") {
        changes.push(StyleChange::Locked(match v { Value::Null => !normal.unlocked, v => parse_bool("locked", v)? }));
    }
    if let Some(v) = args.get("hidden") {
        changes.push(StyleChange::FormulaHidden(match v { Value::Null => normal.formula_hidden, v => parse_bool("hidden", v)? }));
    }
    if let Some(v) = args.get("indent") {
        changes.push(StyleChange::IndentLevel(match v {
            Value::Null => normal.indent,
            v => v.as_u64().filter(|n| *n <= 15).ok_or_else(|| ToolError::invalid("indent is a level from 0 to 15"))? as u32,
        }));
    }
    if let Some(v) = args.get("text_rotation") {
        changes.push(StyleChange::Rotation(match v {
            Value::Null => normal.rotation,
            Value::String(s) if s == "vertical" => 255,
            // Excel's degrees (up positive); the model's 0-180 has 90 up and 180 down.
            v => match v.as_i64().filter(|n| (-90..=90).contains(n)) {
                Some(n) if n >= 0 => n as u8,
                Some(n) => (90 - n) as u8,
                None => return Err(ToolError::invalid("text_rotation is -90 to 90 degrees or \"vertical\"")),
            },
        }));
    }
    if let Some(v) = args.get("size") {
        changes.push(match v {
            Value::Null => StyleChange::FontSize(normal.font_size),
            v => StyleChange::FontSize(parse_size("size", v)?),
        });
    }
    if let Some(v) = args.get("color") {
        changes.push(color_change(open, parse_color("color", v)?, false)?);
    }
    if let Some(v) = args.get("fill") {
        changes.push(color_change(open, parse_color("fill", v)?, true)?);
    }
    if let Some(v) = args.get("font") {
        match parse_font("font", v)? {
            FontArg::All(name) => changes.push(StyleChange::Font(Some(name))),
            FontArg::Split { latin, east_asian, .. } => match latin.or(east_asian) {
                Some(name) => changes.push(StyleChange::Font(Some(name))),
                None => return Err(ToolError::invalid("font has no typeface")),
            },
            FontArg::Inherit => changes.push(StyleChange::Font(normal.font.clone())),
            FontArg::Theme { major } => {
                // A cell keeps the typeface's name, not a reference to the theme: the theme's typeface now is written.
                let fonts = theme_fonts.clone().ok_or_else(|| ToolError::new("unsupported_operation", "this workbook has no theme typefaces"))?;
                let set = if major { fonts.major } else { fonts.minor };
                let name = set.latin.or(set.east_asian).ok_or_else(|| ToolError::new("unsupported_operation", "the theme has no typeface of that kind"))?;
                warnings.push(json!({ "code": "theme_font_resolved", "message": format!("the {} theme typeface was written as {name}; the cells do not follow a later change of the theme", if major { "heading" } else { "body" }) }));
                changes.push(StyleChange::Font(Some(name)));
            }
        }
    }
    if let Some(v) = args.get("number_format") {
        changes.push(StyleChange::NumberFormat(v.as_str().filter(|s| !s.is_empty()).ok_or_else(|| ToolError::invalid("number_format is a format code"))?.to_string()));
    }
    if let Some(v) = args.get("horizontal_alignment") {
        changes.push(StyleChange::HorizontalAlignment(match v.as_str() {
            Some("general") => CellAlign::General,
            Some("left") => CellAlign::Left,
            Some("center") => CellAlign::Center,
            Some("right") => CellAlign::Right,
            Some("justify") => CellAlign::Justify,
            _ => return Err(ToolError::invalid("horizontal_alignment is general, left, center, right or justify")),
        }));
    }
    if let Some(v) = args.get("vertical_alignment") {
        changes.push(StyleChange::VerticalAlignment(match v.as_str() {
            Some("top") => CellVerticalAlign::Top,
            Some("center") => CellVerticalAlign::Center,
            Some("bottom") => CellVerticalAlign::Bottom,
            Some("justify") => CellVerticalAlign::Justify,
            _ => return Err(ToolError::invalid("vertical_alignment is top, center, bottom or justify")),
        }));
    }
    if changes.is_empty() {
        return Err(ToolError::invalid("no format is given"));
    }
    let commands: Vec<SheetCommand> = changes.into_iter().map(|change| SheetCommand::SetFormat { sheet, ranges: vec![range], change }).collect();
    execute(open, commands.into_iter().map(Command::Sheet).collect())?;
    Ok(edited(id, open, json!({}), warnings))
}

fn add_sheet(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let name = args.str_arg("name")?.trim().to_string();
    if name.is_empty() || name.chars().count() > 31 || name.contains(['\\', '/', '?', '*', '[', ']', ':']) {
        return Err(ToolError::invalid("a sheet's name is 1 to 31 characters, without \\ / ? * [ ] :"));
    }
    let open = service.open_document(id)?;
    let count = sheets_of(open)?.len();
    if sheets_of(open)?.iter().any(|(_, n, _)| n.eq_ignore_ascii_case(&name)) {
        return Err(ToolError::invalid(format!("a sheet named {name} is there already")));
    }
    let index = args.opt_usize("index")?.unwrap_or(count);
    if index > count {
        return Err(ToolError::new("out_of_range", format!("index {index}; the workbook has {count} sheets")));
    }
    let (slide, shape) = {
        let ids = open.document.ids();
        (ids.new_slide_id(), ids.new_shape_id())
    };
    sheet_edit(service, id, vec![SheetCommand::AddSheet { index, name: name.clone(), sheet: slide, shape }], json!({ "sheet": name, "index": index }))
}

fn delete_sheet(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let (sheet, name) = sheet_id(open, args.str_arg("sheet")?)?;
    if sheets_of(open)?.len() <= 1 {
        return Err(ToolError::new("unsupported_operation", "the last sheet cannot be deleted"));
    }
    sheet_edit(service, id, vec![SheetCommand::DeleteSheet { sheet }], json!({ "deleted": name }))
}

fn rename_sheet(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let new_name = args.str_arg("name")?.trim().to_string();
    if new_name.is_empty() || new_name.chars().count() > 31 || new_name.contains(['\\', '/', '?', '*', '[', ']', ':']) {
        return Err(ToolError::invalid("a sheet's name is 1 to 31 characters, without \\ / ? * [ ] :"));
    }
    let open = service.open_document(id)?;
    let (sheet, old) = sheet_id(open, args.str_arg("sheet")?)?;
    if !old.eq_ignore_ascii_case(&new_name) && sheets_of(open)?.iter().any(|(_, n, _)| n.eq_ignore_ascii_case(&new_name)) {
        return Err(ToolError::invalid(format!("a sheet named {new_name} is there already")));
    }
    sheet_edit(service, id, vec![SheetCommand::RenameSheet { sheet, name: new_name.clone() }], json!({ "sheet": new_name }))
}

fn calculate_workbook(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    sheets_of(open)?;
    sheet_edit(service, id, vec![SheetCommand::CalculateNow], json!({}))
}

fn structure_schema(rows: bool) -> Value {
    let at = if rows { json!({ "type": "integer", "minimum": 1, "description": "The row number (from 1)." }) } else { json!({ "description": "A column letter such as C, or a number from 1.", "type": ["string", "integer"] }) };
    object(&[document_id(), ("sheet", string("The sheet's name.")), ("at", at), ("count", json!({ "type": "integer", "minimum": 1, "description": "How many (default 1)." }))], &["document_id", "sheet", "at"])
}

/// A column named by a letter (`C`) or a number from 1; 0-based.
pub(crate) fn column_arg(name: &str, value: &Value) -> Result<u32, ToolError> {
    let bad = || ToolError::invalid(format!("{name} is a column letter such as C, or a number from 1 to 16384"));
    let n = match value {
        Value::String(s) if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic()) && s.len() <= 3 => s.to_ascii_uppercase().bytes().fold(0u64, |n, b| n * 26 + u64::from(b - b'A') + 1),
        Value::Number(n) => n.as_u64().ok_or_else(bad)?,
        _ => return Err(bad()),
    };
    if !(1..=16384).contains(&n) {
        return Err(bad());
    }
    Ok(n as u32 - 1)
}

/// `at` and `count` of the row or column tools, 0-based: `(first, count)`.
pub(crate) fn span_args(args: &Value, rows: bool) -> Result<(u32, u32), ToolError> {
    let at = args.get("at").ok_or_else(|| ToolError::invalid("at is required"))?;
    let first = if rows {
        let n = at.as_u64().filter(|n| (1..=1_048_576).contains(n)).ok_or_else(|| ToolError::invalid("at is a row number from 1"))?;
        n as u32 - 1
    } else {
        column_arg("at", at)?
    };
    let count = args.opt_usize("count")?.unwrap_or(1);
    let limit = if rows { 1_048_576 } else { 16384 };
    if count == 0 || first as usize + count > limit {
        return Err(ToolError::new("out_of_range", format!("count {count} from {} goes past the sheet", first + 1)));
    }
    Ok((first, count as u32))
}

fn structure(service: &mut Service, args: &Value, rows: bool, insert: bool) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let (at, count) = span_args(args, rows)?;
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    let axis = if rows { Axis::Rows } else { Axis::Columns };
    let command = if insert { SheetCommand::Insert { sheet, axis, at, count } } else { SheetCommand::Delete { sheet, axis, at, count } };
    sheet_edit(service, id, vec![command], json!({ if insert { "inserted" } else { "deleted" }: count }))
}

fn insert_rows(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    structure(service, args, true, true)
}

fn insert_columns(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    structure(service, args, false, true)
}

fn delete_rows(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    structure(service, args, true, false)
}

fn delete_columns(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    structure(service, args, false, false)
}

fn move_sheet(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let to = args.usize_arg("index")?;
    let open = service.open_document(id)?;
    let (sheet, name) = sheet_id(open, args.str_arg("sheet")?)?;
    let count = sheets_of(open)?.len();
    if to >= count {
        return Err(ToolError::new("out_of_range", format!("index {to}; the workbook has {count} sheets")));
    }
    sheet_edit(service, id, vec![SheetCommand::MoveSheet { sheet, to }], json!({ "sheet": name, "index": to }))
}

fn set_size(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "axis", "at", "count", "size"])?;
    let id = args.str_arg("document_id")?;
    let rows = match args.str_arg("axis")? {
        "row" => true,
        "column" => false,
        _ => return Err(ToolError::invalid("axis is column or row")),
    };
    let (first, count) = span_args(args, rows)?;
    let points = args.get("size").and_then(Value::as_f64).filter(|p| (0.0..=1790.0).contains(p)).ok_or_else(|| ToolError::invalid("size is a number of points, 0 (hidden) to 1790"))?;
    if rows && points > 409.5 {
        return Err(ToolError::invalid("a row is 409.5 points high at most"));
    }
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    let axis = if rows { Axis::Rows } else { Axis::Columns };
    sheet_edit(service, id, vec![SheetCommand::SetSize { sheet, axis, first, last: first + count - 1, size: (points * 12700.0).round() as i64 }], json!({}))
}

fn merge_cells(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    if range.first_row == range.last_row && range.first_column == range.last_column {
        return Err(ToolError::invalid("range is a single cell: merge needs at least two"));
    }
    let center = args.opt_bool("center")?.unwrap_or(false);
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    sheet_edit(service, id, vec![SheetCommand::Merge { sheet, range, center }], json!({}))
}

fn unmerge_cells(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    sheet_edit(service, id, vec![SheetCommand::Unmerge { sheet, range }], json!({}))
}

fn sort_range(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let list = args.get("keys").and_then(Value::as_array).filter(|l| !l.is_empty() && l.len() <= 64).ok_or_else(|| ToolError::invalid("keys is a list of { column, order? }"))?;
    let mut keys = Vec::new();
    for item in list {
        let column = column_arg("keys[].column", item.get("column").unwrap_or(&Value::Null))?;
        if column < range.first_column || column > range.last_column {
            return Err(ToolError::new("out_of_range", format!("keys[].column {} is outside the range", column_name(column))));
        }
        let ascending = match item.get("order").and_then(Value::as_str) {
            None | Some("ascending") => true,
            Some("descending") => false,
            _ => return Err(ToolError::invalid("keys[].order is ascending or descending")),
        };
        keys.push((column, SortBy::Value(ascending)));
    }
    let header = args.opt_bool("has_header")?.unwrap_or(false);
    let match_case = args.opt_bool("match_case")?.unwrap_or(false);
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    sheet_edit(service, id, vec![SheetCommand::SortByKeys { sheet, range, keys, header, match_case }], json!({}))
}
