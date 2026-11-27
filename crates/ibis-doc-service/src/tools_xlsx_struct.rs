// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `xlsx_*` structure tools of stage B step 3 (design `2026-11-24-159-mcp-stage-b.md` 5 and 8): copying and moving cells, replacing text, copying
//! a sheet, names, data validation, conditional formats, filters and tables, links and comments, removing duplicates and subtotals. Each call is one
//! undo step and has a read in `tools_xlsx_read`.

use ibis_doc_model::sheet::{AutoFilter, CellRange, CellStyle, CfKind, CfRule, CfStyle, CfValue, ColumnFilter, ConditionalFormat, Hyperlink, SheetComment, SheetTable, SubtotalParams, TableStyleInfo, Validation};
use ibis_doc_model::sheet_command::{CellEdit, CellInput, PutCell, SheetCommand};
use ibis_doc_model::{CellValue, Command, Rgb};
use serde_json::{Value, json};

use crate::a1::{column_name, parse_range, range_text};
use crate::catalog::{Access, Tool, boolean, document_id, integer, object, string};
use crate::edit::{edited, execute, only_known, parse_bool};
use crate::error::ToolError;
use crate::service::{Args, Open, Service};
use crate::tools_xlsx::{column_arg, sheet_edit, sheet_id, sheets_of};
use crate::tools_xlsx_set::rgb_of;

fn sheet_arg() -> (&'static str, Value) {
    ("sheet", string("The sheet's name."))
}

fn ranges_arg() -> Value {
    json!({ "description": "An A1 range, or a list of them.", "oneOf": [{ "type": "string" }, { "type": "array", "minItems": 1, "items": { "type": "string" } }] })
}

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "xlsx_copy_cells",
            description: "Copy the cells of a range to where `to` (a cell, the top left of the copy) is, on the same sheet or another (to_sheet). what: all (default: values, formulas and formats; relative references move), values, formulas, formats. The range it covers is replaced.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), sheet_arg(), ("range", string("The A1 range to copy.")), ("to", string("The A1 cell the copy starts at.")), ("to_sheet", string("The sheet it goes to (default: the same).")), ("what", json!({ "enum": ["all", "values", "formulas", "formats"] }))], &["document_id", "sheet", "range", "to"]),
            run: copy_cells,
        },
        Tool {
            name: "xlsx_move_cells",
            description: "Move the cells of a range (values, formulas and formats) so that its top left is at `to`; the formulas that refer to them follow. On the same sheet.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), sheet_arg(), ("range", string("The A1 range to move.")), ("to", string("The A1 cell the range's top left goes to."))], &["document_id", "sheet", "range", "to"]),
            run: move_cells,
        },
        Tool {
            name: "xlsx_replace_text",
            description: "Replace text in the cells that hold text (not numbers or formulas) of a sheet, or of every sheet. whole_cell replaces only a cell that is just `find`. Returns how many cells changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("find", string("The text to find.")), ("replace", string("The text that takes its place.")), ("sheet", string("Only this sheet.")), ("match_case", boolean("Match upper and lower case (default false).")), ("whole_cell", boolean("Match the whole cell (default false)."))], &["document_id", "find", "replace"]),
            run: replace_text,
        },
        Tool {
            name: "xlsx_copy_sheet",
            description: "Copy a sheet (cells, formats, sizes, merged cells, names; not its drawings) right after it, or at index (0-based). name defaults to Excel's \"Sheet (2)\".",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), sheet_arg(), ("name", string("The copy's name.")), ("index", integer("Where the copy goes (0-based)."))], &["document_id", "sheet"]),
            run: copy_sheet,
        },
        Tool {
            name: "xlsx_add_name",
            description: "Define a name for a range, a constant or a formula: refers_to such as =Sheet1!$A$1:$B$3 or =0.1; scope is a sheet's name (default: the workbook). xlsx_list_names reads them.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("name", string("The name (letters, digits, _ and .; not like a cell address).")), ("refers_to", string("What it stands for.")), ("scope", string("A sheet's name; without it the workbook."))], &["document_id", "name", "refers_to"]),
            run: add_name,
        },
        Tool {
            name: "xlsx_delete_name",
            description: "Delete a defined name (of the workbook, or of the sheet named in scope).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("name", string("The name.")), ("scope", string("A sheet's name; without it the workbook."))], &["document_id", "name"]),
            run: delete_name,
        },
        Tool {
            name: "xlsx_set_validation",
            description: "Put a data validation rule on ranges (rules that lie inside them go). kind: list (items, or formula1 such as =$A$1:$A$5 or a formula), whole, decimal, date, time, text_length (operator between, not_between, equal, not_equal, greater_than, less_than, greater_than_or_equal, less_than_or_equal; formula1 and, for between, formula2), custom (formula1), any (only the messages); null takes the rule away. A relative reference in a formula is written for the first cell of the first range. xlsx_list_validations reads them.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        sheet_arg(),
                        ("range", ranges_arg()),
                        ("kind", json!({ "enum": ["list", "whole", "decimal", "date", "time", "text_length", "custom", "any", null] })),
                        ("operator", string("For whole, decimal, date, time and text_length (default between).")),
                        ("items", json!({ "type": "array", "items": { "type": ["string", "number"] }, "description": "A list's items." })),
                        ("formula1", string("A bound, a range or a formula.")),
                        ("formula2", string("The upper bound of between and not_between.")),
                        ("ignore_blank", boolean("Blank cells pass (default true).")),
                        ("dropdown", boolean("A list shows its arrow in the cell (default true).")),
                        ("message", json!({ "type": ["object", "null"], "properties": { "title": { "type": "string" }, "text": { "type": "string" } }, "description": "Shown when the cell is selected." })),
                        ("error", json!({ "type": ["object", "null"], "properties": { "style": { "enum": ["stop", "warning", "information"] }, "title": { "type": "string" }, "text": { "type": "string" } }, "description": "Shown for a value the rule refuses." })),
                    ],
                    &["document_id", "sheet", "range", "kind"],
                )
            },
            run: set_validation,
        },
        Tool {
            name: "xlsx_add_conditional_format",
            description: "Add a conditional format over ranges. rule.type: cell_value (operator, formula1, formula2), expression (formula, relative to the first range's top left cell), top_bottom (rank, percent, bottom), above_average (above, equal), unique, duplicate, color_scale (points of two or three { type: min|max|num|percent|percentile|formula, value?, color }), data_bar (color, gradient?, min?, max? as { type, value? }), icon_set (icon_set such as 3TrafficLights1, thresholds?, reverse?, show_value?). format: color, bold, italic, underline, strike, fill, number_format (for the rules that give a format). stop: stop if true. xlsx_list_conditional_formats reads them.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        sheet_arg(),
                        ("range", ranges_arg()),
                        ("rule", json!({ "type": "object", "properties": { "type": { "enum": ["cell_value", "expression", "top_bottom", "above_average", "unique", "duplicate", "color_scale", "data_bar", "icon_set"] } }, "required": ["type"] })),
                        ("format", json!({ "type": "object", "properties": { "color": { "type": "string" }, "fill": { "type": "string" }, "bold": { "type": "boolean" }, "italic": { "type": "boolean" }, "underline": { "type": "boolean" }, "strike": { "type": "boolean" }, "number_format": { "type": "string" } }, "additionalProperties": false })),
                        ("stop", boolean("Stop if true.")),
                    ],
                    &["document_id", "sheet", "range", "rule"],
                )
            },
            run: add_conditional_format,
        },
        Tool {
            name: "xlsx_delete_conditional_format",
            description: "Delete a conditional format by its index (xlsx_list_conditional_formats), with all its rules.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), sheet_arg(), ("index", integer("The format's index."))], &["document_id", "sheet", "index"]),
            run: delete_conditional_format,
        },
        Tool {
            name: "xlsx_set_auto_filter",
            description: "Put the auto filter on a range (its first row the headings), or take it away (range null). columns: [{ column, values }] keeps the rows whose shown text is one of the values (\"\" keeps blanks). xlsx_list_tables reads it.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        sheet_arg(),
                        ("range", json!({ "type": ["string", "null"] })),
                        ("columns", json!({ "type": "array", "items": { "type": "object", "properties": { "column": { "type": ["string", "integer"] }, "values": { "type": "array", "items": { "type": "string" } } }, "required": ["column", "values"] } })),
                    ],
                    &["document_id", "sheet", "range"],
                )
            },
            run: set_auto_filter,
        },
        Tool {
            name: "xlsx_set_table",
            description: "Make a table of a range whose first row holds the headings (text, no two the same), change a table (by name, or by the range it has): name, style (a table style name such as TableStyleMedium2, or null), first_column, last_column, row_stripes, column_stripes; or remove: true turns it back into a range. xlsx_list_tables reads them.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        sheet_arg(),
                        ("range", string("The table's range (with its heading row).")),
                        ("name", string("The table's name (for a new one: default Table1...).")),
                        ("style", json!({ "type": ["string", "null"] })),
                        ("first_column", boolean("Emphasise the first column.")),
                        ("last_column", boolean("Emphasise the last column.")),
                        ("row_stripes", boolean("Banded rows.")),
                        ("column_stripes", boolean("Banded columns.")),
                        ("remove", boolean("Turn the table back into a range.")),
                    ],
                    &["document_id", "sheet"],
                )
            },
            run: set_table,
        },
        Tool {
            name: "xlsx_set_hyperlink",
            description: "Put a hyperlink on a range: url (a web address or file) or location (a place in the workbook such as Sheet2!A1), tooltip. Without url and location it takes the links away. xlsx_list_hyperlinks reads them.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), sheet_arg(), ("range", string("A1 range.")), ("url", json!({ "type": ["string", "null"] })), ("location", json!({ "type": ["string", "null"] })), ("tooltip", json!({ "type": ["string", "null"] }))], &["document_id", "sheet", "range"]),
            run: set_hyperlink,
        },
        Tool {
            name: "xlsx_add_comment",
            description: "Put a note on a cell (a note that is there is replaced). xlsx_list_comments reads them.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), sheet_arg(), ("cell", string("An A1 cell.")), ("text", string("The note's text.")), ("author", string("Who wrote it (default empty)."))], &["document_id", "sheet", "cell", "text"]),
            run: add_comment,
        },
        Tool {
            name: "xlsx_delete_comment",
            description: "Delete the note of a cell.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), sheet_arg(), ("cell", string("An A1 cell."))], &["document_id", "sheet", "cell"]),
            run: delete_comment,
        },
        Tool {
            name: "xlsx_remove_duplicates",
            description: "Remove the rows of a range whose values in `columns` (letters inside the range; default all) repeat an earlier row's; the rows left move up. Returns how many rows went.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), sheet_arg(), ("range", string("A1 range.")), ("columns", json!({ "type": "array", "minItems": 1, "items": { "type": ["string", "integer"] } })), ("has_header", boolean("The first row is a heading and stays (default false)."))], &["document_id", "sheet", "range"]),
            run: remove_duplicates,
        },
        Tool {
            name: "xlsx_apply_subtotal",
            description: "Insert subtotal rows: for each change of the value in group_by, apply `function` (sum, average, count, counta, max, min, product, stdev, stdevp, var, varp) to `columns`. The range's first row is the headings. An earlier subtotal is replaced unless replace_existing is false.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        sheet_arg(),
                        ("range", string("A1 range with the headings row.")),
                        ("group_by", json!({ "type": ["string", "integer"], "description": "A column inside the range." })),
                        ("function", json!({ "enum": ["sum", "average", "count", "counta", "max", "min", "product", "stdev", "stdevp", "var", "varp"] })),
                        ("columns", json!({ "type": "array", "minItems": 1, "items": { "type": ["string", "integer"] } })),
                        ("replace_existing", boolean("Replace earlier subtotals (default true).")),
                        ("summary_below", boolean("The totals go below the data (default true).")),
                    ],
                    &["document_id", "sheet", "range", "group_by", "function", "columns"],
                )
            },
            run: apply_subtotal,
        },
    ]
}

fn cell_arg(name: &str, value: &str) -> Result<(u32, u32), ToolError> {
    let r = parse_range(value)?;
    if r.first_row != r.last_row || r.first_column != r.last_column {
        return Err(ToolError::invalid(format!("{name} is one cell such as B2")));
    }
    Ok((r.first_row, r.first_column))
}

fn sheet_data<'a>(open: &'a Open, name: &str) -> Result<&'a ibis_doc_model::sheet::Sheet, ToolError> {
    Ok(crate::tools_xlsx_read::sheet_named(open, name)?.1)
}

fn put_of(cell: Option<&ibis_doc_model::sheet::SheetCell>, drow: i64, dcol: i64, what: &str, style: Option<CellStyle>, row: u32, column: u32) -> PutCell {
    let input = match cell {
        None => CellInput::Clear,
        Some(c) => match (&c.formula, what) {
            (Some(f), "all" | "formulas") => CellInput::Formula(ibis_calc::formula::shift(f, drow, dcol)),
            _ if c.value == CellValue::Empty => CellInput::Clear,
            _ => CellInput::Value(c.value.clone()),
        },
    };
    PutCell { row, column, input, style }
}

fn copy_cells(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "to", "to_sheet", "what"])?;
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let (to_row, to_column) = cell_arg("to", args.str_arg("to")?)?;
    let what = args.opt_str("what")?.unwrap_or("all");
    if !matches!(what, "all" | "values" | "formulas" | "formats") {
        return Err(ToolError::invalid("what is all, values, formulas or formats"));
    }
    let (rows, columns) = (range.last_row - range.first_row + 1, range.last_column - range.first_column + 1);
    if u64::from(rows) * u64::from(columns) > 100_000 {
        return Err(ToolError::new("out_of_range", "more than 100000 cells in one call"));
    }
    if to_row + rows > 1_048_576 || to_column + columns > 16_384 {
        return Err(ToolError::new("out_of_range", "the copy goes past the sheet"));
    }
    let open = service.open_document(id)?;
    let (_, source_name) = sheet_id(open, args.str_arg("sheet")?)?;
    let (target, _) = sheet_id(open, args.opt_str("to_sheet")?.unwrap_or(args.str_arg("sheet")?))?;
    let source = sheet_data(open, &source_name)?;
    let normal = crate::tools_xlsx::normal_style(open);
    let target_name = args.opt_str("to_sheet")?.unwrap_or(&source_name).to_string();
    let target_sheet = sheet_data(open, &target_name)?;
    let mut cells = Vec::with_capacity((rows * columns) as usize);
    let (drow, dcol) = (i64::from(to_row) - i64::from(range.first_row), i64::from(to_column) - i64::from(range.first_column));
    for r in 0..rows {
        for c in 0..columns {
            // A hidden formula (#275) is copied as its value only, as Excel pastes it.
            let copied = source.cell(range.first_row + r, range.first_column + c).map(|cell| source.copy_of(cell));
            let from = copied.as_ref();
            let (row, column) = (to_row + r, to_column + c);
            let style_of = |c: Option<&ibis_doc_model::sheet::SheetCell>| c.and_then(|c| source.styles.get(c.style as usize).cloned()).unwrap_or_else(|| normal.clone());
            cells.push(match what {
                "all" => put_of(from, drow, dcol, what, Some(style_of(from)), row, column),
                "values" | "formulas" => put_of(from, drow, dcol, what, None, row, column),
                // formats: the target keeps what it holds.
                _ => put_of(target_sheet.cell(row, column), 0, 0, "all", Some(style_of(from)), row, column),
            });
        }
    }
    sheet_edit(service, id, vec![SheetCommand::PasteCells { sheet: target, cells }], json!({ "cells": rows * columns, "copied_to": range_text(&CellRange { first_row: to_row, first_column: to_column, last_row: to_row + rows - 1, last_column: to_column + columns - 1 }) }))
}

fn move_cells(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "to"])?;
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let (row, column) = cell_arg("to", args.str_arg("to")?)?;
    if row + (range.last_row - range.first_row) >= 1_048_576 || column + (range.last_column - range.first_column) >= 16_384 {
        return Err(ToolError::new("out_of_range", "the range goes past the sheet"));
    }
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    sheet_edit(service, id, vec![SheetCommand::MoveCells { sheet, from: range, row, column }], json!({}))
}

fn replace_text(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "find", "replace", "sheet", "match_case", "whole_cell"])?;
    let id = args.str_arg("document_id")?;
    let find = args.str_arg("find")?;
    if find.is_empty() {
        return Err(ToolError::invalid("find is empty"));
    }
    let replace = args.str_arg("replace")?;
    let match_case = args.opt_bool("match_case")?.unwrap_or(false);
    let whole = args.opt_bool("whole_cell")?.unwrap_or(false);
    let open = service.open_document(id)?;
    let names: Vec<String> = match args.opt_str("sheet")? {
        Some(n) => vec![sheet_id(open, n)?.1],
        None => sheets_of(open)?.into_iter().map(|(_, n, _)| n).collect(),
    };
    let fold = |s: &str| if match_case { s.to_string() } else { s.to_lowercase() };
    let needle = fold(find);
    let mut commands = Vec::new();
    let mut changed = 0usize;
    for name in &names {
        let (slide, _) = sheet_id(open, name)?;
        let sheet = sheet_data(open, name)?;
        let mut edits = Vec::new();
        for cell in sheet.cells.iter() {
            let CellValue::Text(text) = &cell.value else { continue };
            if cell.formula.is_some() {
                continue;
            }
            let new = if whole {
                if fold(text) == needle { Some(replace.to_string()) } else { None }
            } else {
                replace_in(text, find, replace, match_case)
            };
            if let Some(new) = new.filter(|n| n != text) {
                edits.push(CellEdit { row: cell.row, column: cell.column, input: CellInput::Value(CellValue::Text(new)) });
            }
        }
        changed += edits.len();
        if !edits.is_empty() {
            commands.push(SheetCommand::SetCells { sheet: slide, edits });
        }
    }
    if commands.is_empty() {
        return Ok(edited(id, open, json!({ "replaced": 0 }), Vec::new()));
    }
    sheet_edit(service, id, commands, json!({ "replaced": changed }))
}

/// `text` with every `find` (as written, or in any case) replaced; `None` when it is not in it.
fn replace_in(text: &str, find: &str, replace: &str, match_case: bool) -> Option<String> {
    if match_case {
        return text.contains(find).then(|| text.replace(find, replace));
    }
    let (lower_text, lower_find): (Vec<char>, Vec<char>) = (text.chars().flat_map(char::to_lowercase).collect(), find.chars().flat_map(char::to_lowercase).collect());
    // Lower-casing keeps the length of the text in characters for what is written here; when it does not, only the exact match is replaced.
    let chars: Vec<char> = text.chars().collect();
    if lower_text.len() != chars.len() {
        return text.contains(find).then(|| text.replace(find, replace));
    }
    let mut out = String::new();
    let (mut i, mut found) = (0, false);
    while i < chars.len() {
        if i + lower_find.len() <= chars.len() && lower_text[i..i + lower_find.len()] == lower_find[..] {
            out.push_str(replace);
            i += lower_find.len();
            found = true;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    found.then_some(out)
}

fn copy_sheet(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "name", "index"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let (slide, base) = sheet_id(open, args.str_arg("sheet")?)?;
    let name = match args.opt_str("name")? {
        Some(n) => {
            let n = n.trim().to_string();
            if n.is_empty() || n.chars().count() > 31 || n.contains(['\\', '/', '?', '*', '[', ']', ':']) {
                return Err(ToolError::invalid("a sheet's name is 1 to 31 characters, without \\ / ? * [ ] :"));
            }
            if sheets_of(open)?.iter().any(|(_, s, _)| s.eq_ignore_ascii_case(&n)) {
                return Err(ToolError::invalid(format!("a sheet named {n} is there already")));
            }
            n
        }
        None => ibis_doc_model::sheet_ops::copy_sheet_name(open.document.presentation(), &base),
    };
    let count = sheets_of(open)?.len();
    let index = args.opt_usize("index")?;
    if index.is_some_and(|i| i >= count + 1) {
        return Err(ToolError::new("out_of_range", format!("index {}; the workbook has {count} sheets", index.unwrap_or(0))));
    }
    let (new_slide, shape) = {
        let ids = open.document.ids();
        (ids.new_slide_id(), ids.new_shape_id())
    };
    let mut commands = vec![SheetCommand::CopySheet { sheet: slide, name: name.clone(), new_slide, shape }];
    if let Some(to) = index {
        commands.push(SheetCommand::MoveSheet { sheet: new_slide, to: to.min(count) });
    }
    sheet_edit(service, id, commands, json!({ "sheet": name }))
}

/// A defined name's rules: letters, digits, `_`, `.`, `\`; starts with a letter, `_` or `\`; not a cell address; at most 255 characters.
fn valid_name(name: &str) -> Result<(), ToolError> {
    let bad = |why: &str| ToolError::invalid(format!("{name:?} is not a name: {why}"));
    let mut chars = name.chars();
    let first = chars.next().ok_or_else(|| bad("empty"))?;
    if !(first.is_alphabetic() || first == '_' || first == '\\') || name.chars().count() > 255 {
        return Err(bad("it starts with a letter or _ and has 255 characters at most"));
    }
    if !name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\')) {
        return Err(bad("letters, digits, _ and . only"));
    }
    if name.eq_ignore_ascii_case("r") || name.eq_ignore_ascii_case("c") || parse_range(name).is_ok() {
        return Err(bad("it looks like a cell address"));
    }
    if name.to_ascii_lowercase().starts_with("_xlnm.") {
        return Err(bad("print areas and titles are set by xlsx_set_page_setup"));
    }
    Ok(())
}

fn scope_index(open: &Open, scope: Option<&str>) -> Result<Option<usize>, ToolError> {
    match scope {
        None => Ok(None),
        Some(s) if s.eq_ignore_ascii_case("workbook") => Ok(None),
        Some(s) => open.document.presentation().workbook.as_ref().and_then(|w| w.sheet_index(s)).map(Some).ok_or_else(|| ToolError::new("no_such_sheet", format!("no sheet named {s}"))),
    }
}

fn add_name(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "name", "refers_to", "scope"])?;
    let id = args.str_arg("document_id")?;
    let name = args.str_arg("name")?.to_string();
    valid_name(&name)?;
    let formula = args.str_arg("refers_to")?.trim().trim_start_matches('=').trim().to_string();
    if formula.is_empty() {
        return Err(ToolError::invalid("refers_to is empty"));
    }
    let open = service.open_document(id)?;
    sheets_of(open)?;
    let scope = scope_index(open, args.opt_str("scope")?)?;
    let workbook = open.document.presentation().workbook.as_ref().expect("a workbook has its record");
    if workbook.names.iter().any(|n| n.name.eq_ignore_ascii_case(&name) && n.sheet == scope) {
        return Err(ToolError::invalid(format!("the name {name} is there already: delete it first")));
    }
    sheet_edit(service, id, vec![SheetCommand::SetName { name: name.clone(), sheet: scope, formula: Some(formula) }], json!({ "name": name }))
}

fn delete_name(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "name", "scope"])?;
    let id = args.str_arg("document_id")?;
    let name = args.str_arg("name")?.to_string();
    if name.to_ascii_lowercase().starts_with("_xlnm.") {
        return Err(ToolError::invalid("print areas and titles are set by xlsx_set_page_setup"));
    }
    let open = service.open_document(id)?;
    sheets_of(open)?;
    let scope = scope_index(open, args.opt_str("scope")?)?;
    let workbook = open.document.presentation().workbook.as_ref().expect("a workbook has its record");
    if !workbook.names.iter().any(|n| n.name.eq_ignore_ascii_case(&name) && n.sheet == scope) {
        return Err(ToolError::invalid(format!("no name {name} in that scope (xlsx_list_names)")));
    }
    sheet_edit(service, id, vec![SheetCommand::SetName { name: name.clone(), sheet: scope, formula: None }], json!({ "name": name }))
}

/// The ranges argument: a string or a list of strings.
fn ranges_of(name: &str, value: Option<&Value>) -> Result<Vec<CellRange>, ToolError> {
    let bad = || ToolError::invalid(format!("{name} is an A1 range or a list of them"));
    let ranges: Vec<CellRange> = match value {
        Some(Value::String(s)) => vec![parse_range(s)?],
        Some(Value::Array(list)) if !list.is_empty() && list.len() <= 64 => list.iter().map(|v| parse_range(v.as_str().ok_or_else(bad)?)).collect::<Result<_, _>>()?,
        _ => return Err(bad()),
    };
    Ok(ranges)
}

const OPERATORS: [(&str, &str); 8] = [("between", "between"), ("not_between", "notBetween"), ("equal", "equal"), ("not_equal", "notEqual"), ("greater_than", "greaterThan"), ("less_than", "lessThan"), ("greater_than_or_equal", "greaterThanOrEqual"), ("less_than_or_equal", "lessThanOrEqual")];

fn operator_of(value: Option<&str>) -> Result<String, ToolError> {
    let text = value.unwrap_or("between");
    OPERATORS.iter().find(|(a, _)| *a == text).map(|(_, b)| (*b).to_string()).ok_or_else(|| ToolError::invalid(format!("operator is one of {}", OPERATORS.iter().map(|o| o.0).collect::<Vec<_>>().join(", "))))
}

fn text_of(args: &Value, key: &str) -> Result<Option<String>, ToolError> {
    Ok(args.opt_str(key)?.map(|s| s.trim().trim_start_matches('=').trim().to_string()).filter(|s| !s.is_empty()))
}

fn set_validation(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "kind", "operator", "items", "formula1", "formula2", "ignore_blank", "dropdown", "message", "error"])?;
    let id = args.str_arg("document_id")?;
    let ranges = ranges_of("range", args.get("range"))?;
    let kind = match args.get("kind") {
        Some(Value::Null) => None,
        Some(Value::String(k)) => Some(k.as_str()),
        _ => return Err(ToolError::invalid("kind is required: list, whole, decimal, date, time, text_length, custom, any, or null")),
    };
    let rule = match kind {
        None => None,
        Some(kind) => {
            let model_kind = match kind {
                "list" | "whole" | "decimal" | "date" | "time" | "custom" => kind,
                "text_length" => "textLength",
                "any" => "none",
                other => return Err(ToolError::invalid(format!("kind {other} is not list, whole, decimal, date, time, text_length, custom or any"))),
            };
            let (mut formula1, mut formula2) = (text_of(args, "formula1")?, text_of(args, "formula2")?);
            let mut operator = String::new();
            match kind {
                "list" => {
                    if let Some(items) = args.get("items") {
                        let list = items.as_array().filter(|l| !l.is_empty()).ok_or_else(|| ToolError::invalid("items is a list of texts or numbers"))?;
                        let texts: Vec<String> = list.iter().map(|v| match v {
                            Value::String(s) => Ok(s.clone()),
                            Value::Number(n) => Ok(n.to_string()),
                            _ => Err(ToolError::invalid("items are texts or numbers")),
                        }).collect::<Result<_, _>>()?;
                        if texts.iter().any(|t| t.contains(',') || t.contains('"')) {
                            return Err(ToolError::invalid("an item holds no comma or quote; use formula1 with a range for those"));
                        }
                        let joined = texts.join(",");
                        if joined.chars().count() > 255 {
                            return Err(ToolError::invalid("the items are longer than 255 characters; put them in cells and use formula1"));
                        }
                        formula1 = Some(format!("\"{joined}\""));
                    }
                    if formula1.is_none() {
                        return Err(ToolError::invalid("a list needs items or formula1"));
                    }
                    formula2 = None;
                }
                "custom" => {
                    if formula1.is_none() {
                        return Err(ToolError::invalid("custom needs formula1"));
                    }
                    formula2 = None;
                }
                "any" => {
                    formula1 = None;
                    formula2 = None;
                }
                _ => {
                    operator = operator_of(args.opt_str("operator")?)?;
                    if formula1.is_none() {
                        return Err(ToolError::invalid("formula1 is the bound"));
                    }
                    if matches!(operator.as_str(), "between" | "notBetween") && formula2.is_none() {
                        return Err(ToolError::invalid("between and not_between need formula2"));
                    }
                    if !matches!(operator.as_str(), "between" | "notBetween") {
                        formula2 = None;
                    }
                }
            }
            let prompt = match args.get("message") {
                None | Some(Value::Null) => None,
                Some(m) => Some((m.get("title").and_then(Value::as_str).unwrap_or("").to_string(), m.get("text").and_then(Value::as_str).unwrap_or("").to_string())),
            };
            let error = match args.get("error") {
                None | Some(Value::Null) => None,
                Some(e) => {
                    let style = e.get("style").and_then(Value::as_str).unwrap_or("stop");
                    if !matches!(style, "stop" | "warning" | "information") {
                        return Err(ToolError::invalid("error.style is stop, warning or information"));
                    }
                    Some((style.to_string(), e.get("title").and_then(Value::as_str).unwrap_or("").to_string(), e.get("text").and_then(Value::as_str).unwrap_or("").to_string()))
                }
            };
            Some(Validation {
                ranges: ranges.clone(),
                kind: model_kind.to_string(),
                operator,
                formula1,
                formula2,
                allow_blank: args.opt_bool("ignore_blank")?.unwrap_or(true),
                dropdown: args.opt_bool("dropdown")?.unwrap_or(true),
                prompt,
                error,
            })
        }
    };
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    sheet_edit(service, id, vec![SheetCommand::SetValidation { sheet, ranges, rule }], json!({}))
}

fn cf_value(name: &str, value: &Value) -> Result<CfValue, ToolError> {
    let kind = value.get("type").and_then(Value::as_str).filter(|k| matches!(*k, "min" | "max" | "num" | "percent" | "percentile" | "formula")).ok_or_else(|| ToolError::invalid(format!("{name}.type is min, max, num, percent, percentile or formula")))?;
    let text = match value.get("value") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => return Err(ToolError::invalid(format!("{name}.value is a number or text"))),
    };
    if !matches!(kind, "min" | "max") && text.is_none() {
        return Err(ToolError::invalid(format!("{name}.value is required for {kind}")));
    }
    Ok(CfValue { kind: kind.to_string(), value: text, gte: true })
}

fn cf_color(open: &Open, name: &str, value: Option<&Value>, warnings: &mut Vec<Value>) -> Result<Rgb, ToolError> {
    rgb_of(open, name, value.ok_or_else(|| ToolError::invalid(format!("{name} is required")))?, warnings)
}

fn cf_style(open: &Open, format: &Value, warnings: &mut Vec<Value>) -> Result<CfStyle, ToolError> {
    only_known(format, &["color", "fill", "bold", "italic", "underline", "strike", "number_format"])?;
    let mut style = CfStyle::default();
    if let Some(v) = format.get("color") {
        style.color = Some(rgb_of(open, "format.color", v, warnings)?);
    }
    if let Some(v) = format.get("fill") {
        style.fill = Some(rgb_of(open, "format.fill", v, warnings)?);
    }
    for (key, slot) in [("bold", 0), ("italic", 1), ("strike", 3)] {
        if let Some(v) = format.get(key) {
            let b = parse_bool(key, v)?;
            match slot {
                0 => style.bold = Some(b),
                1 => style.italic = Some(b),
                _ => style.strike = Some(b),
            }
        }
    }
    if let Some(v) = format.get("underline") {
        style.underline = Some(if parse_bool("underline", v)? { ibis_doc_model::sheet::CellUnderline::Single } else { ibis_doc_model::sheet::CellUnderline::None });
    }
    if let Some(v) = format.get("number_format") {
        style.number_format = Some(v.as_str().filter(|s| !s.is_empty()).ok_or_else(|| ToolError::invalid("format.number_format is a format code"))?.to_string());
    }
    Ok(style)
}

fn add_conditional_format(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "rule", "format", "stop"])?;
    let id = args.str_arg("document_id")?;
    let ranges = ranges_of("range", args.get("range"))?;
    let rule = args.get("rule").filter(|r| r.is_object()).ok_or_else(|| ToolError::invalid("rule is an object with a type"))?;
    let kind_name = rule.get("type").and_then(Value::as_str).ok_or_else(|| ToolError::invalid("rule.type is required"))?;
    let stop = args.opt_bool("stop")?.unwrap_or(false);
    let open = service.open_document(id)?;
    let (sheet, name) = sheet_id(open, args.str_arg("sheet")?)?;
    let mut warnings = Vec::new();
    let known = |keys: &[&str]| -> Result<(), ToolError> {
        let mut all = vec!["type"];
        all.extend_from_slice(keys);
        only_known(rule, &all).map_err(|e| ToolError::invalid(format!("rule: {}", e.message)))
    };
    let text = |key: &str| -> Result<Option<String>, ToolError> { Ok(rule.get(key).and_then(Value::as_str).map(|s| s.trim().trim_start_matches('=').trim().to_string()).filter(|s| !s.is_empty())) };
    let needs_format = |kind: &str| !matches!(kind, "color_scale" | "data_bar" | "icon_set");
    let kind = match kind_name {
        "cell_value" => {
            known(&["operator", "formula1", "formula2"])?;
            let operator = operator_of(rule.get("operator").and_then(Value::as_str))?;
            let f1 = text("formula1")?.ok_or_else(|| ToolError::invalid("rule.formula1 is required"))?;
            let mut formulas = vec![f1];
            if matches!(operator.as_str(), "between" | "notBetween") {
                formulas.push(text("formula2")?.ok_or_else(|| ToolError::invalid("between and not_between need rule.formula2"))?);
            }
            CfKind::CellIs { operator, formulas }
        }
        "expression" => {
            known(&["formula"])?;
            CfKind::Expression(text("formula")?.ok_or_else(|| ToolError::invalid("rule.formula is required"))?)
        }
        "top_bottom" => {
            known(&["rank", "percent", "bottom"])?;
            let rank = rule.get("rank").and_then(Value::as_u64).filter(|r| (1..=1000).contains(r)).ok_or_else(|| ToolError::invalid("rule.rank is 1 to 1000"))? as u32;
            CfKind::Top10 { rank, percent: rule.get("percent").and_then(Value::as_bool).unwrap_or(false), bottom: rule.get("bottom").and_then(Value::as_bool).unwrap_or(false) }
        }
        "above_average" => {
            known(&["above", "equal"])?;
            CfKind::AboveAverage { above: rule.get("above").and_then(Value::as_bool).unwrap_or(true), equal: rule.get("equal").and_then(Value::as_bool).unwrap_or(false), std_dev: 0 }
        }
        "unique" => {
            known(&[])?;
            CfKind::Unique
        }
        "duplicate" => {
            known(&[])?;
            CfKind::Duplicate
        }
        "color_scale" => {
            known(&["points"])?;
            let points = rule.get("points").and_then(Value::as_array).filter(|p| (2..=3).contains(&p.len())).ok_or_else(|| ToolError::invalid("rule.points is two or three { type, value?, color }"))?;
            let mut out = Vec::new();
            for (i, p) in points.iter().enumerate() {
                out.push((cf_value(&format!("rule.points[{i}]"), p)?, cf_color(open, &format!("rule.points[{i}].color"), p.get("color"), &mut warnings)?));
            }
            CfKind::ColorScale(out)
        }
        "data_bar" => {
            known(&["color", "gradient", "min", "max"])?;
            let min = rule.get("min").map(|v| cf_value("rule.min", v)).transpose()?.unwrap_or(CfValue { kind: "min".into(), value: None, gte: true });
            let max = rule.get("max").map(|v| cf_value("rule.max", v)).transpose()?.unwrap_or(CfValue { kind: "max".into(), value: None, gte: true });
            CfKind::data_bar(min, max, cf_color(open, "rule.color", rule.get("color"), &mut warnings)?, rule.get("gradient").and_then(Value::as_bool).unwrap_or(true))
        }
        "icon_set" => {
            known(&["icon_set", "thresholds", "reverse", "show_value"])?;
            let set = rule.get("icon_set").and_then(Value::as_str).filter(|s| s.chars().next().is_some_and(|c| ('3'..='5').contains(&c))).ok_or_else(|| ToolError::invalid("rule.icon_set is a set such as 3TrafficLights1, 4Arrows or 5Rating"))?;
            let count = set.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(3) as usize;
            let values = match rule.get("thresholds") {
                Some(Value::Array(list)) => {
                    if list.len() != count {
                        return Err(ToolError::invalid(format!("rule.thresholds has {count} entries for {set}")));
                    }
                    list.iter().enumerate().map(|(i, v)| cf_value(&format!("rule.thresholds[{i}]"), v)).collect::<Result<Vec<_>, _>>()?
                }
                // Excel's own: the icons share the range in equal percents.
                _ => (0..count).map(|i| CfValue { kind: "percent".into(), value: Some((i * 100 / count).to_string()), gte: true }).collect(),
            };
            CfKind::icon_set(set, values, rule.get("reverse").and_then(Value::as_bool).unwrap_or(false), rule.get("show_value").and_then(Value::as_bool).unwrap_or(true))
        }
        other => return Err(ToolError::invalid(format!("rule.type {other} is not cell_value, expression, top_bottom, above_average, unique, duplicate, color_scale, data_bar or icon_set"))),
    };
    let style = match args.get("format") {
        None | Some(Value::Null) => None,
        Some(f) => {
            if !needs_format(kind_name) {
                return Err(ToolError::new("unsupported_operation", format!("a {kind_name} rule gives no format")));
            }
            Some(cf_style(open, f, &mut warnings)?)
        }
    };
    if needs_format(kind_name) && style.is_none() {
        return Err(ToolError::invalid(format!("a {kind_name} rule needs a format")));
    }
    let sheet_data = sheet_data(open, &name)?;
    let priority = sheet_data.conditional.iter().flat_map(|f| f.rules.iter().map(|r| r.priority)).max().unwrap_or(0) + 1;
    let format = ConditionalFormat { source_index: None, ranges, rules: vec![CfRule { kind, priority, stop, style }] };
    execute(open, vec![Command::Sheet(SheetCommand::AddConditional { sheet, format })])?;
    let index = sheet_data_len(open, &name)? - 1;
    Ok(edited(id, open, json!({ "index": index }), warnings))
}

fn sheet_data_len(open: &Open, name: &str) -> Result<usize, ToolError> {
    Ok(sheet_data(open, name)?.conditional.len())
}

fn delete_conditional_format(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "index"])?;
    let id = args.str_arg("document_id")?;
    let index = args.usize_arg("index")?;
    let open = service.open_document(id)?;
    let (sheet, name) = sheet_id(open, args.str_arg("sheet")?)?;
    let rules = sheet_data(open, &name)?.conditional.get(index).map(|f| f.rules.len()).ok_or_else(|| ToolError::new("out_of_range", format!("no conditional format {index}")))?;
    // The last rule of a format takes the format with it: the rules go from the end so that the index stays.
    let commands = (0..rules).rev().map(|rule| SheetCommand::DeleteConditionalRule { sheet, index, rule }).collect();
    sheet_edit(service, id, commands, json!({}))
}

fn set_auto_filter(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "columns"])?;
    let id = args.str_arg("document_id")?;
    let filter = match args.get("range") {
        Some(Value::Null) => {
            if args.get("columns").is_some() {
                return Err(ToolError::invalid("columns goes with a range"));
            }
            None
        }
        Some(Value::String(text)) => {
            let range = parse_range(text)?;
            if range.first_row == range.last_row && range.first_column == range.last_column {
                return Err(ToolError::invalid("range is the headings and the rows under them"));
            }
            let mut columns = Vec::new();
            if let Some(list) = args.get("columns") {
                for item in list.as_array().ok_or_else(|| ToolError::invalid("columns is a list of { column, values }"))? {
                    let column = column_arg("columns[].column", item.get("column").unwrap_or(&Value::Null))?;
                    if column < range.first_column || column > range.last_column {
                        return Err(ToolError::new("out_of_range", format!("columns[].column {} is outside the range", column_name(column))));
                    }
                    let values: Vec<String> = item.get("values").and_then(Value::as_array).ok_or_else(|| ToolError::invalid("columns[].values is a list of texts"))?.iter().map(|v| v.as_str().map(str::to_string).ok_or_else(|| ToolError::invalid("columns[].values are texts"))).collect::<Result<_, _>>()?;
                    columns.push((column, ColumnFilter::Values(values)));
                }
            }
            Some(AutoFilter { range, columns })
        }
        _ => return Err(ToolError::invalid("range is an A1 range, or null")),
    };
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    sheet_edit(service, id, vec![SheetCommand::SetAutoFilter { sheet, filter }], json!({}))
}

fn overlaps(a: CellRange, b: CellRange) -> bool {
    a.first_row <= b.last_row && b.first_row <= a.last_row && a.first_column <= b.last_column && b.first_column <= a.last_column
}

fn set_table(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "name", "style", "first_column", "last_column", "row_stripes", "column_stripes", "remove"])?;
    let id = args.str_arg("document_id")?;
    let range = args.opt_str("range")?.map(parse_range).transpose()?;
    let name = args.opt_str("name")?;
    let open = service.open_document(id)?;
    let (slide, sheet_name) = sheet_id(open, args.str_arg("sheet")?)?;
    let data = sheet_data(open, &sheet_name)?;
    // The table to change: by name, or the one over the range.
    let existing = data.tables.iter().position(|t| name.is_some_and(|n| t.name.eq_ignore_ascii_case(n)) || (name.is_none() && range.is_some_and(|r| t.range == r)));
    if args.opt_bool("remove")?.unwrap_or(false) {
        let index = existing.ok_or_else(|| ToolError::invalid("no such table (xlsx_list_tables)"))?;
        return sheet_edit(service, id, vec![SheetCommand::SetTable { sheet: slide, index, table: None, cells: Vec::new() }], json!({}));
    }
    let taken: Vec<String> = sheets_of(open)?.iter().flat_map(|(_, _, s)| s.tables.iter().map(|t| t.name.clone())).collect();
    let mut table = match existing {
        Some(i) => {
            if range.is_some_and(|r| r != data.tables[i].range) {
                return Err(ToolError::new("unsupported_operation", "the range of a table is changed in the window (Resize Table); remove it and make it again"));
            }
            data.tables[i].clone()
        }
        None => {
            let range = range.ok_or_else(|| ToolError::invalid("range is required to make a table"))?;
            if range.first_row == range.last_row {
                return Err(ToolError::invalid("a table is a heading row and at least one row"));
            }
            if data.tables.iter().any(|t| overlaps(t.range, range)) {
                return Err(ToolError::invalid("tables cannot overlap"));
            }
            if data.merges.iter().any(|m| overlaps(*m, range)) {
                return Err(ToolError::invalid("a range with merged cells cannot be a table"));
            }
            let mut columns = Vec::new();
            for column in range.first_column..=range.last_column {
                let heading = match data.cell(range.first_row, column).map(|c| &c.value) {
                    Some(CellValue::Text(t)) if !t.is_empty() => t.clone(),
                    _ => return Err(ToolError::invalid(format!("{}{} is not a heading: the first row holds the headings as text", column_name(column), range.first_row + 1))),
                };
                if columns.iter().any(|c: &String| c.eq_ignore_ascii_case(&heading)) {
                    return Err(ToolError::invalid(format!("the heading {heading} is there twice")));
                }
                columns.push(heading);
            }
            let mut n = 1;
            let new_name = loop {
                let candidate = format!("Table{n}");
                if !taken.iter().any(|t| t.eq_ignore_ascii_case(&candidate)) {
                    break candidate;
                }
                n += 1;
            };
            SheetTable {
                name: new_name,
                range,
                header_rows: 1,
                totals_rows: 0,
                columns,
                style: Some(TableStyleInfo { name: "TableStyleMedium2".into(), row_stripes: true, ..Default::default() }),
                filter: Some(AutoFilter { range, columns: Vec::new() }),
                totals: Vec::new(),
            }
        }
    };
    if let Some(n) = name.filter(|n| !n.eq_ignore_ascii_case(&table.name) || existing.is_none()) {
        valid_name(n)?;
        if taken.iter().any(|t| t.eq_ignore_ascii_case(n) && !(existing.is_some() && t.eq_ignore_ascii_case(&table.name))) {
            return Err(ToolError::invalid(format!("a table named {n} is there already")));
        }
        table.name = n.to_string();
    }
    if let Some(v) = args.get("style") {
        match v {
            Value::Null => table.style = None,
            Value::String(s) if !s.is_empty() => table.style.get_or_insert_with(TableStyleInfo::default).name = s.clone(),
            _ => return Err(ToolError::invalid("style is a table style name, or null")),
        }
    }
    for key in ["first_column", "last_column", "row_stripes", "column_stripes"] {
        if let Some(v) = args.get(key) {
            let b = parse_bool(key, v)?;
            let style = table.style.as_mut().ok_or_else(|| ToolError::invalid(format!("{key} needs a table style")))?;
            match key {
                "first_column" => style.first_column = b,
                "last_column" => style.last_column = b,
                "row_stripes" => style.row_stripes = b,
                _ => style.column_stripes = b,
            }
        }
    }
    let index = existing.unwrap_or(data.tables.len());
    if existing.is_some() && table == data.tables[index] {
        return Err(ToolError::invalid("nothing changes"));
    }
    let result = json!({ "table": table.name.clone(), "range": range_text(&table.range) });
    // The look of the table's style laid on the cells, as the window does it.
    let theme = open.document.presentation().workbook.as_ref().map(|w| w.theme_colors.clone()).unwrap_or_default();
    let cells = match existing {
        Some(i) => ibis_formats::restyled_cells(data, &data.tables[i], &table, &theme, &[]),
        None => ibis_formats::styled_cells(data, data, &table, &theme, &[]),
    };
    sheet_edit(service, id, vec![SheetCommand::SetTable { sheet: slide, index, table: Some(table), cells }], result)
}

fn set_hyperlink(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "url", "location", "tooltip"])?;
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let (url, location, tooltip) = (args.opt_str("url")?.map(str::to_string), args.opt_str("location")?.map(str::to_string), args.opt_str("tooltip")?.map(str::to_string));
    if url.as_deref().is_some_and(str::is_empty) || location.as_deref().is_some_and(str::is_empty) {
        return Err(ToolError::invalid("url and location are not empty"));
    }
    if url.is_some() && location.is_some() {
        return Err(ToolError::invalid("a link is a url or a location, not both"));
    }
    if url.is_none() && location.is_none() && tooltip.is_some() {
        return Err(ToolError::invalid("a tooltip goes with a url or a location"));
    }
    let link = (url.is_some() || location.is_some()).then(|| Hyperlink { range, target: url, location, tooltip });
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    sheet_edit(service, id, vec![SheetCommand::SetHyperlink { sheet, range, link }], json!({}))
}

fn add_comment(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "cell", "text", "author"])?;
    let id = args.str_arg("document_id")?;
    let (row, column) = cell_arg("cell", args.str_arg("cell")?)?;
    let text = args.str_arg("text")?;
    if text.is_empty() {
        return Err(ToolError::invalid("text is empty"));
    }
    let comment = SheetComment { row, column, author: args.opt_str("author")?.unwrap_or("").to_string(), text: text.to_string(), runs: Vec::new(), rect: None };
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    sheet_edit(service, id, vec![SheetCommand::SetComment { sheet, row, column, comment: Some(comment) }], json!({}))
}

fn delete_comment(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "cell"])?;
    let id = args.str_arg("document_id")?;
    let (row, column) = cell_arg("cell", args.str_arg("cell")?)?;
    let open = service.open_document(id)?;
    let (sheet, name) = sheet_id(open, args.str_arg("sheet")?)?;
    if !sheet_data(open, &name)?.comments.iter().any(|c| c.row == row && c.column == column) {
        return Err(ToolError::invalid("that cell has no note (xlsx_list_comments)"));
    }
    sheet_edit(service, id, vec![SheetCommand::SetComment { sheet, row, column, comment: None }], json!({}))
}

fn column_list(name: &str, value: Option<&Value>, range: &CellRange) -> Result<Vec<u32>, ToolError> {
    let columns: Vec<u32> = match value {
        None | Some(Value::Null) => (range.first_column..=range.last_column).collect(),
        Some(Value::Array(list)) if !list.is_empty() && list.len() <= 64 => list.iter().map(|v| column_arg(name, v)).collect::<Result<_, _>>()?,
        _ => return Err(ToolError::invalid(format!("{name} is a list of columns"))),
    };
    if let Some(c) = columns.iter().find(|c| **c < range.first_column || **c > range.last_column) {
        return Err(ToolError::new("out_of_range", format!("{name}: column {} is outside the range", column_name(*c))));
    }
    Ok(columns)
}

fn remove_duplicates(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "columns", "has_header"])?;
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let header = args.opt_bool("has_header")?.unwrap_or(false);
    let columns = column_list("columns", args.get("columns"), &range)?;
    let open = service.open_document(id)?;
    let (sheet, name) = sheet_id(open, args.str_arg("sheet")?)?;
    let first = range.first_row + u32::from(header);
    if first > range.last_row {
        return Err(ToolError::invalid("the range has no row under its heading"));
    }
    // How many rows repeat an earlier one (the values as shown, as Excel compares them).
    let data = sheet_data(open, &name)?;
    let mut seen = std::collections::HashSet::new();
    let mut removed = 0;
    for row in first..=range.last_row {
        let key: Vec<String> = columns.iter().map(|c| data.cell(row, *c).map_or(String::new(), |x| x.text.to_lowercase())).collect();
        if !seen.insert(key) {
            removed += 1;
        }
    }
    if removed == 0 {
        return Err(ToolError::invalid("no row repeats an earlier one"));
    }
    sheet_edit(service, id, vec![SheetCommand::RemoveDuplicates { sheet, range, columns, header }], json!({ "removed": removed }))
}

fn apply_subtotal(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "group_by", "function", "columns", "replace_existing", "summary_below"])?;
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    if range.last_row <= range.first_row {
        return Err(ToolError::invalid("the range is the headings row and the rows under it"));
    }
    let group_by = column_arg("group_by", args.get("group_by").unwrap_or(&Value::Null))?;
    if group_by < range.first_column || group_by > range.last_column {
        return Err(ToolError::new("out_of_range", "group_by is outside the range"));
    }
    let code = match args.str_arg("function")? {
        "average" => 1,
        "count" => 2,
        "counta" => 3,
        "max" => 4,
        "min" => 5,
        "product" => 6,
        "stdev" => 7,
        "stdevp" => 8,
        "sum" => 9,
        "var" => 10,
        "varp" => 11,
        other => return Err(ToolError::invalid(format!("function {other} is not sum, average, count, counta, max, min, product, stdev, stdevp, var or varp"))),
    };
    let subtotal_columns = column_list("columns", Some(args.get("columns").ok_or_else(|| ToolError::invalid("columns is required"))?), &range)?;
    let params = SubtotalParams { range, group_by_column: group_by, function_code: code, subtotal_columns, replace_existing: args.opt_bool("replace_existing")?.unwrap_or(true), summary_below: args.opt_bool("summary_below")?.unwrap_or(true), page_breaks: false };
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    sheet_edit(service, id, vec![SheetCommand::ApplySubtotal { sheet, params }], json!({}))
}
