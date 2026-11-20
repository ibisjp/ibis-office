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
        Tool { name: "xlsx_list_sheets", description: "The sheets of the workbook: name, used range and whether it is shown.", access: Access::Read, returns_content: false, schema: || object(&[document_id()], &["document_id"]), run: list_sheets },
        Tool { name: "xlsx_get_used_range", description: "The range of a sheet that holds anything.", access: Access::Read, returns_content: false, schema: || object(&[document_id(), ("sheet", string("The sheet's name."))], &["document_id", "sheet"]), run: get_used_range },
        Tool {
            name: "xlsx_get_cells",
            description: "The cells of a range, row by row: value (calculated), formula, or text (as shown). Paged by rows.",
            access: Access::Read,
            returns_content: true,
            schema: || {
                object(
                    &[
                        document_id(),
                        ("sheet", string("The sheet's name.")),
                        ("range", string("A1 range, e.g. B2:D10.")),
                        ("kind", json!({ "enum": ["value", "formula", "text"], "description": "value (default), formula or text (as shown)." })),
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
            schema: || object(&[document_id(), ("query", string("The text to find.")), ("sheet", string("Only this sheet.")), ("match_case", boolean("Match upper and lower case (default false).")), ("limit", integer("The most cells."))], &["document_id", "query"]),
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
    let sheets: Vec<Value> = sheets_of(open)?.iter().map(|(_, name, sheet)| json!({ "name": name, "used_range": used_range(sheet).map(|r| range_text(&r)), "cells": sheet.cells.len() })).collect();
    Ok(json!({ "sheets": sheets }))
}

fn get_used_range(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let name = args.str_arg("sheet")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let sheets = sheets_of(open)?;
    let (_, name, sheet) = find_sheet(&sheets, name)?;
    Ok(json!({ "sheet": name, "used_range": used_range(sheet).map(|r| range_text(&r)) }))
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
            "formula" => cell.formula.as_ref().map_or_else(|| value_json(&cell.value), |f| Value::String(format!("={f}"))),
            "text" => Value::String(cell.text.clone()),
            "format" => {
                let style = sheet.styles.get(cell.style as usize).cloned().unwrap_or_default();
                json!({
                    "bold": style.bold,
                    "italic": style.italic,
                    "underline": style.underline != ibis_doc_model::CellUnderline::None,
                    "size": f64::from(style.font_size) / 100.0,
                    "font": style.font,
                    "color": format!("#{:06X}", style.color.0 & 0xFF_FFFF),
                    "fill": style.fill.map(|f| format!("#{:06X}", f.0 & 0xFF_FFFF)),
                })
            }
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
use ibis_doc_model::sheet_command::{CellEdit, CellInput, SheetCommand, StyleChange};
use ibis_doc_model::{Command, SlideId};

use crate::edit::{ColorArg, FontArg, edited, execute, only_known, parse_bool, parse_color, parse_font, parse_size};
use crate::tools_docx::{color_schema, font_schema};

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "xlsx_set_cells",
            description: "Put values in a range: rows of numbers, strings, true/false or null (clears the cell). A string that starts with = is a formula. A single value fills the whole range; otherwise the rows must be as large as the range. What depends on them is calculated again.",
            access: Access::Edit,
            returns_content: false,
            schema: || {
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
            schema: || object(&[document_id(), ("sheet", string("The sheet's name.")), ("range", string("A1 range.")), ("what", json!({ "enum": ["contents", "formats", "all"] }))], &["document_id", "sheet", "range", "what"]),
            run: clear_cells,
        },
        Tool {
            name: "xlsx_set_format",
            description: "Set the format of a range: bold, italic, underline, size (pt), color, font, fill, number_format (Excel's format code), horizontal_alignment, vertical_alignment, wrap_text. What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: || {
                object(
                    &[
                        document_id(),
                        ("sheet", string("The sheet's name.")),
                        ("range", string("A1 range.")),
                        ("bold", boolean("Bold.")),
                        ("italic", boolean("Italic.")),
                        ("underline", boolean("Underline.")),
                        ("size", json!({ "type": "number", "description": "Points, 1 to 409.5 in steps of 0.5." })),
                        ("color", color_schema()),
                        ("font", font_schema()),
                        ("fill", color_schema()),
                        ("number_format", string("Excel's number format code, e.g. 0.00 or yyyy/m/d.")),
                        ("horizontal_alignment", json!({ "enum": ["general", "left", "center", "right", "justify"] })),
                        ("vertical_alignment", json!({ "enum": ["top", "center", "bottom", "justify"] })),
                        ("wrap_text", boolean("Wrap the text in the cell.")),
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
            schema: || object(&[document_id(), ("name", string("The new sheet's name.")), ("index", integer("Where it goes (0-based)."))], &["document_id", "name"]),
            run: add_sheet,
        },
        Tool { name: "xlsx_delete_sheet", description: "Delete a sheet (not the last one).", access: Access::Edit, returns_content: false, schema: || object(&[document_id(), ("sheet", string("The sheet's name."))], &["document_id", "sheet"]), run: delete_sheet },
        Tool { name: "xlsx_rename_sheet", description: "Rename a sheet.", access: Access::Edit, returns_content: false, schema: || object(&[document_id(), ("sheet", string("The sheet's name.")), ("name", string("The new name."))], &["document_id", "sheet", "name"]), run: rename_sheet },
        Tool { name: "xlsx_calculate_workbook", description: "Calculate every formula of the workbook now.", access: Access::Edit, returns_content: false, schema: || object(&[document_id()], &["document_id"]), run: calculate_workbook },
    ]
}

fn sheet_id(open: &Open, name: &str) -> Result<(SlideId, String), ToolError> {
    let sheets = sheets_of(open)?;
    let (index, name, _) = find_sheet(&sheets, name)?;
    Ok((open.document.presentation().slides[*index].id, name.clone()))
}

fn sheet_edit(service: &mut Service, id: &str, commands: Vec<SheetCommand>, extra: Value) -> Result<Value, ToolError> {
    let open = service.open_document(id)?;
    execute(open, commands.into_iter().map(Command::Sheet).collect())?;
    Ok(edited(id, open, extra, Vec::new()))
}

fn input_of(value: &Value) -> Result<CellInput, ToolError> {
    Ok(match value {
        Value::Null => CellInput::Clear,
        Value::Bool(b) => CellInput::Value(CellValue::Bool(*b)),
        Value::Number(n) => CellInput::Value(CellValue::Number(n.as_f64().ok_or_else(|| ToolError::invalid("a number is out of range"))?)),
        Value::String(s) => match s.strip_prefix('=') {
            Some(formula) if !formula.is_empty() => CellInput::Formula(formula.to_string()),
            _ => CellInput::Value(CellValue::Text(s.clone())),
        },
        _ => return Err(ToolError::invalid("a cell is a number, a string, true/false or null")),
    })
}

fn set_cells(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let (rows, columns) = ((range.last_row - range.first_row + 1) as usize, (range.last_column - range.first_column + 1) as usize);
    let values = args.get("values").ok_or_else(|| ToolError::invalid("values is required"))?;
    let grid: Vec<Vec<Value>> = match values {
        Value::Array(outer) => {
            let grid: Vec<Vec<Value>> = outer.iter().map(|r| r.as_array().cloned().ok_or_else(|| ToolError::invalid("values is rows (arrays) of cells"))).collect::<Result<_, _>>()?;
            if grid.len() != rows || grid.iter().any(|r| r.len() != columns) {
                return Err(ToolError::new("out_of_range", format!("the range {} is {rows} rows by {columns} columns; values is {} rows", args.str_arg("range")?, grid.len())));
            }
            grid
        }
        single => vec![vec![single.clone(); columns]; rows],
    };
    if rows * columns > 100_000 {
        return Err(ToolError::new("out_of_range", "more than 100000 cells in one call"));
    }
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    let mut edits = Vec::with_capacity(rows * columns);
    for (r, row) in grid.iter().enumerate() {
        for (c, value) in row.iter().enumerate() {
            edits.push(CellEdit { row: range.first_row + r as u32, column: range.first_column + c as u32, input: input_of(value)? });
        }
    }
    sheet_edit(service, id, vec![SheetCommand::SetCells { sheet, edits }], json!({ "cells": rows * columns }))
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

fn theme_of(name: &str) -> Result<u8, ToolError> {
    // SpreadsheetML's order: 0 lt1, 1 dk1, 2 lt2, 3 dk2, 4-9 accent1-6.
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
        _ => return Err(ToolError::new("unsupported_operation", "the hyperlink theme colours are not supported yet")),
    })
}

/// The workbook's Normal style: what a cell goes back to when a format is taken away (`null`, design 6.6).
fn normal_style(open: &Open) -> ibis_doc_model::CellStyle {
    open.document.presentation().workbook.as_ref().and_then(|w| w.default_style.clone()).unwrap_or_default()
}

fn color_change(open: &Open, arg: ColorArg, fill: bool) -> Result<StyleChange, ToolError> {
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
            if fill {
                StyleChange::Fill(None)
            } else {
                StyleChange::Color(normal_style(open).color)
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
    only_known(args, &["document_id", "sheet", "range", "bold", "italic", "underline", "size", "color", "font", "fill", "number_format", "horizontal_alignment", "vertical_alignment", "wrap_text"])?;
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    let normal = normal_style(open);
    let theme_fonts = open.document.presentation().workbook.as_ref().and_then(|w| w.theme_fonts.clone());
    let mut changes = Vec::new();
    let mut warnings = Vec::new();
    // `null` takes a format back to the Normal style's.
    for (key, make, base) in [("bold", StyleChange::Bold as fn(bool) -> StyleChange, normal.bold), ("italic", StyleChange::Italic, normal.italic), ("underline", StyleChange::Underline, normal.underline != ibis_doc_model::CellUnderline::None), ("wrap_text", StyleChange::WrapText, normal.wrap)] {
        if let Some(v) = args.get(key) {
            changes.push(match v {
                Value::Null => make(base),
                v => make(parse_bool(key, v)?),
            });
        }
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
