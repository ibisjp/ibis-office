// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `xlsx_*` setters of stage B step 2 (design `2026-11-24-159-mcp-stage-b.md` 5 and 8): borders, hidden sheets rows and columns, the sheet's
//! view, page setup, protection, calculation and the document's properties. Each call is one undo step; each has a read in `tools_xlsx_read`.

use ibis_doc_model::sheet::{Border, CellRange, SheetPrint, SheetProtection, WorkbookProtection};
use ibis_doc_model::sheet_command::{Axis, Edge, SheetCommand, StyleChange};
use ibis_doc_model::{EMU_PER_POINT, Rgb};
use serde_json::{Value, json};

use crate::a1::parse_range;
use crate::catalog::{Access, Tool, boolean, document_id, object, string};
use crate::edit::{ColorArg, edited, execute, only_known, parse_bool, parse_color};
use crate::error::ToolError;
use crate::service::{Args, Open, Service};
use crate::tools_xlsx::{sheet_edit, sheet_id, sheets_of, span_args};
use crate::tools_xlsx_read::{border_from_name, theme_color_resolution};

fn sheet_arg() -> (&'static str, Value) {
    ("sheet", string("The sheet's name."))
}

fn at_schema(rows: bool) -> Value {
    if rows {
        json!({ "type": "integer", "minimum": 1, "description": "The first row (from 1)." })
    } else {
        json!({ "description": "The first column: a letter such as C, or a number from 1.", "type": ["string", "integer"] })
    }
}

fn span_schema(rows: bool) -> Value {
    object(&[document_id(), sheet_arg(), ("at", at_schema(rows)), ("count", json!({ "type": "integer", "minimum": 1, "description": "How many (default 1)." }))], &["document_id", "sheet", "at"])
}

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "xlsx_set_borders",
            description: "Draw or remove borders on a range. edges: top, bottom, left, right, outline, inside_horizontal, inside_vertical, all, diagonal_up, diagonal_down (default all). style: thin, medium, thick, double, dotted, dashed, hair, dash_dot, dash_dot_dot, medium_dashed, medium_dash_dot, medium_dash_dot_dot, slant_dash_dot, or none (removes). Read the result with xlsx_get_cells kind format.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        sheet_arg(),
                        ("range", string("A1 range.")),
                        ("edges", json!({ "type": "array", "minItems": 1, "items": { "enum": ["top", "bottom", "left", "right", "outline", "inside_horizontal", "inside_vertical", "all", "diagonal_up", "diagonal_down"] } })),
                        ("style", string("The line style (default thin); none removes the line.")),
                        ("color", json!({ "description": "\"#RRGGBB\" (default black). A theme colour is written as the colour it has now.", "type": ["string", "object"] })),
                    ],
                    &["document_id", "sheet", "range"],
                )
            },
            run: set_borders,
        },
        Tool { name: "xlsx_show_sheet", description: "Show a hidden sheet again (not a very hidden one).", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), sheet_arg()], &["document_id", "sheet"]), run: show_sheet },
        Tool { name: "xlsx_hide_sheet", description: "Hide a sheet (at least one stays shown). xlsx_list_sheets tells the state.", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), sheet_arg()], &["document_id", "sheet"]), run: hide_sheet },
        Tool { name: "xlsx_hide_rows", description: "Hide rows from row `at` (from 1). xlsx_get_used_range lists the hidden rows.", access: Access::Edit, returns_content: false, schema: &|| span_schema(true), run: hide_rows },
        Tool { name: "xlsx_show_rows", description: "Show the hidden rows among those from row `at` (from 1) at the height they had.", access: Access::Edit, returns_content: false, schema: &|| span_schema(true), run: show_rows },
        Tool { name: "xlsx_hide_columns", description: "Hide columns from column `at` (a letter such as C, or a number from 1).", access: Access::Edit, returns_content: false, schema: &|| span_schema(false), run: hide_columns },
        Tool { name: "xlsx_show_columns", description: "Show the hidden columns among those from column `at` at the width they had.", access: Access::Edit, returns_content: false, schema: &|| span_schema(false), run: show_columns },
        Tool {
            name: "xlsx_set_freeze_panes",
            description: "Freeze the rows above and the columns left of a cell (it is the first cell that scrolls: B2 freezes row 1 and column A); null (or A1) unfreezes. xlsx_get_sheet_view reads it.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), sheet_arg(), ("cell", json!({ "type": ["string", "null"], "description": "A1 cell, or null." }))], &["document_id", "sheet", "cell"]),
            run: set_freeze_panes,
        },
        Tool {
            name: "xlsx_set_sheet_view",
            description: "Set how a sheet is shown: gridlines, headings, zoom (10 to 400 percent; null is 100), tab_color (\"#RRGGBB\", or null for none), show_formulas, show_zeros. What is not given is not changed. xlsx_get_sheet_view reads it.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        sheet_arg(),
                        ("gridlines", boolean("Show the grid lines.")),
                        ("headings", boolean("Show the row numbers and column letters.")),
                        ("zoom", json!({ "type": ["integer", "null"], "minimum": 10, "maximum": 400 })),
                        ("tab_color", json!({ "description": "\"#RRGGBB\" or null.", "type": ["string", "object", "null"] })),
                        ("show_formulas", boolean("Show formulas in place of their values.")),
                        ("show_zeros", boolean("Show zero values.")),
                    ],
                    &["document_id", "sheet"],
                )
            },
            run: set_sheet_view,
        },
        Tool {
            name: "xlsx_set_page_setup",
            description: "Set how a sheet is printed. paper (a4, a3, a5, b4, b5, letter, legal, hagaki, or { width, height } in points), orientation, margins ({ left, right, top, bottom, header, footer } in points, any of them), scale (10 to 400), fit_to ({ pages_wide, pages_tall }, 0 for as many as it takes; null goes back to scale), print_area (A1 range or null), print_titles (rows and columns that repeat, such as \"$1:$2\", \"$A:$B\", \"$A:$B,$1:$2\", or null), header and footer (Excel's header and footer text with codes such as &C and &P), center_horizontally, center_vertically, print_gridlines, print_headings, black_and_white, page_order (down_then_over, over_then_down). xlsx_get_page_setup reads it.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                let size = || json!({ "type": "number", "minimum": 0 });
                object(
                    &[
                        document_id(),
                        sheet_arg(),
                        ("paper", json!({ "description": "A paper's name, or { width, height } in points.", "type": ["string", "object"] })),
                        ("orientation", json!({ "enum": ["portrait", "landscape"] })),
                        ("margins", json!({ "type": "object", "properties": { "left": size(), "right": size(), "top": size(), "bottom": size(), "header": size(), "footer": size() }, "additionalProperties": false })),
                        ("scale", json!({ "type": "integer", "minimum": 10, "maximum": 400 })),
                        ("fit_to", json!({ "type": ["object", "null"], "properties": { "pages_wide": { "type": "integer", "minimum": 0 }, "pages_tall": { "type": "integer", "minimum": 0 } }, "additionalProperties": false })),
                        ("print_area", json!({ "type": ["string", "null"] })),
                        ("print_titles", json!({ "type": ["string", "null"] })),
                        ("header", json!({ "type": ["string", "null"] })),
                        ("footer", json!({ "type": ["string", "null"] })),
                        ("center_horizontally", boolean("Centre across the page.")),
                        ("center_vertically", boolean("Centre down the page.")),
                        ("print_gridlines", boolean("Print the grid lines.")),
                        ("print_headings", boolean("Print the row and column headings.")),
                        ("black_and_white", boolean("Print in black and white.")),
                        ("page_order", json!({ "enum": ["down_then_over", "over_then_down"] })),
                    ],
                    &["document_id", "sheet"],
                )
            },
            run: set_page_setup,
        },
        Tool {
            name: "xlsx_set_protection",
            description: "Protect or unprotect a sheet (or, without sheet, the workbook's structure). No password is taken or changed: a sheet or workbook that has a password cannot be changed here. allow (a sheet): booleans format_cells, format_columns, format_rows, insert_columns, insert_rows, insert_hyperlinks, delete_columns, delete_rows, sort, auto_filter, pivot_tables, objects, scenarios, select_locked, select_unlocked. xlsx_list_sheets reads the protection.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                let flags: Vec<(&str, Value)> = ALLOWED.iter().map(|k| (*k, json!({ "type": "boolean" }))).collect();
                let allow = object(&flags, &[]);
                object(&[document_id(), ("sheet", string("The sheet's name; without it, the workbook.")), ("value", boolean("Protect (true) or unprotect (false).")), ("allow", allow)], &["document_id", "value"])
            },
            run: set_protection,
        },
        Tool {
            name: "xlsx_set_calculation",
            description: "Set how the workbook calculates: mode (automatic, automatic_except_tables, manual), iterative (true allows circular references), max_iterations and max_change. app_get_document_info reads it.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("mode", json!({ "enum": ["automatic", "automatic_except_tables", "manual"] })),
                        ("iterative", boolean("Iterative calculation.")),
                        ("max_iterations", json!({ "type": "integer", "minimum": 1, "maximum": 32767 })),
                        ("max_change", json!({ "type": "number", "minimum": 0 })),
                    ],
                    &["document_id"],
                )
            },
            run: set_calculation,
        },
        Tool {
            name: "xlsx_set_document_properties",
            description: "Set the document's properties: title, subject, author, keywords, description, category, company, manager (null clears one). app_get_document_info reads them.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                let text = || json!({ "type": ["string", "null"] });
                object(&[document_id(), ("title", text()), ("subject", text()), ("author", text()), ("keywords", text()), ("description", text()), ("category", text()), ("company", text()), ("manager", text())], &["document_id"])
            },
            run: set_document_properties,
        },
    ]
}

const EDGES: [&str; 10] = ["top", "bottom", "left", "right", "outline", "inside_horizontal", "inside_vertical", "all", "diagonal_up", "diagonal_down"];

fn rows_of(r: &CellRange, first: u32, last: u32) -> CellRange {
    CellRange { first_row: first, last_row: last, ..*r }
}

fn columns_of(r: &CellRange, first: u32, last: u32) -> CellRange {
    CellRange { first_column: first, last_column: last, ..*r }
}

/// The colour of something that cannot hold a theme reference: its RGB (the theme colour as it is now), and a warning saying so.
pub(crate) fn rgb_of(open: &Open, name: &str, value: &Value, warnings: &mut Vec<Value>) -> Result<Rgb, ToolError> {
    Ok(match parse_color(name, value)? {
        ColorArg::Rgb(rgb) => rgb,
        ColorArg::Inherit => return Err(ToolError::invalid(format!("{name} is \"#RRGGBB\""))),
        ColorArg::Theme { name: theme, brightness } => {
            warnings.push(json!({ "code": "theme_color_resolved", "message": format!("{name}: the theme colour {theme} was written as the colour it has now") }));
            theme_color_resolution(open, &theme, brightness)?
        }
    })
}

fn set_borders(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "range", "edges", "style", "color"])?;
    let id = args.str_arg("document_id")?;
    let range = parse_range(args.str_arg("range")?)?;
    let style = args.opt_str("style")?.unwrap_or("thin");
    let line = border_from_name(style).ok_or_else(|| ToolError::invalid(format!("style {style} is not a border style (thin, medium, thick, double, dotted, dashed, hair, dash_dot, dash_dot_dot, medium_dashed, medium_dash_dot, medium_dash_dot_dot, slant_dash_dot, none)")))?;
    let edges: Vec<&str> = match args.get("edges") {
        None | Some(Value::Null) => vec!["all"],
        Some(Value::Array(list)) if !list.is_empty() => list.iter().map(|e| e.as_str().filter(|e| EDGES.contains(e)).ok_or_else(|| ToolError::invalid(format!("edges are {}", EDGES.join(", "))))).collect::<Result<_, _>>()?,
        Some(_) => return Err(ToolError::invalid("edges is a list of edge names")),
    };
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    let mut warnings = Vec::new();
    let color = match args.get("color") {
        None | Some(Value::Null) => Rgb::BLACK,
        Some(v) => rgb_of(open, "color", v, &mut warnings)?,
    };
    let border = line.map(|line| Border { line, color });
    // `(range, edge)` pairs; the same edge twice is harmless.
    let mut parts: Vec<(CellRange, Edge)> = Vec::new();
    for edge in edges {
        let outline = matches!(edge, "outline" | "all");
        if outline || edge == "top" {
            parts.push((rows_of(&range, range.first_row, range.first_row), Edge::Top));
        }
        if outline || edge == "bottom" {
            parts.push((rows_of(&range, range.last_row, range.last_row), Edge::Bottom));
        }
        if outline || edge == "left" {
            parts.push((columns_of(&range, range.first_column, range.first_column), Edge::Left));
        }
        if outline || edge == "right" {
            parts.push((columns_of(&range, range.last_column, range.last_column), Edge::Right));
        }
        if matches!(edge, "inside_horizontal" | "all") && range.last_row > range.first_row {
            parts.push((rows_of(&range, range.first_row, range.last_row - 1), Edge::Bottom));
            parts.push((rows_of(&range, range.first_row + 1, range.last_row), Edge::Top));
        }
        if matches!(edge, "inside_vertical" | "all") && range.last_column > range.first_column {
            parts.push((columns_of(&range, range.first_column, range.last_column - 1), Edge::Right));
            parts.push((columns_of(&range, range.first_column + 1, range.last_column), Edge::Left));
        }
        if edge == "diagonal_up" {
            parts.push((range, Edge::DiagonalUp));
        }
        if edge == "diagonal_down" {
            parts.push((range, Edge::DiagonalDown));
        }
    }
    let commands: Vec<ibis_doc_model::Command> = parts.into_iter().map(|(r, e)| ibis_doc_model::Command::Sheet(SheetCommand::SetFormat { sheet, ranges: vec![r], change: StyleChange::Border(e, border) })).collect();
    execute(open, commands)?;
    Ok(edited(id, open, json!({}), warnings))
}

fn workbook_index(open: &Open, name: &str) -> Result<usize, ToolError> {
    open.document.presentation().workbook.as_ref().and_then(|w| w.sheet_index(name)).ok_or_else(|| ToolError::new("no_such_sheet", format!("no sheet named {name}")))
}

fn hide_sheet(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let (slide, name) = sheet_id(open, args.str_arg("sheet")?)?;
    if sheets_of(open)?.len() <= 1 {
        return Err(ToolError::new("unsupported_operation", "the last shown sheet cannot be hidden"));
    }
    let index = workbook_index(open, &name)?;
    let shape = open.document.ids().new_shape_id();
    sheet_edit(service, id, vec![SheetCommand::HideSheet { sheet: slide, index, hidden: true, shape }], json!({ "sheet": name, "sheet_state": "hidden" }))
}

fn show_sheet(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::sheet::SheetState;
    let id = args.str_arg("document_id")?;
    let name = args.str_arg("sheet")?;
    let open = service.open_document(id)?;
    sheets_of(open)?;
    let index = workbook_index(open, name)?;
    let workbook = open.document.presentation().workbook.as_ref().expect("a workbook has its record");
    let ws = &workbook.sheets[index];
    match ws.state {
        SheetState::Visible => return Err(ToolError::invalid(format!("{} is shown already", ws.name))),
        SheetState::VeryHidden => return Err(ToolError::new("unsupported_operation", "a very hidden sheet is shown only by a macro")),
        SheetState::Hidden => {}
    }
    if ws.other || ws.hidden.is_none() {
        return Err(ToolError::new("unsupported_operation", "this sheet is not a worksheet that can be shown"));
    }
    let shown_name = ws.name.clone();
    let (slide, shape) = {
        let ids = open.document.ids();
        (ids.new_slide_id(), ids.new_shape_id())
    };
    sheet_edit(service, id, vec![SheetCommand::HideSheet { sheet: slide, index, hidden: false, shape }], json!({ "sheet": shown_name, "sheet_state": "visible" }))
}

fn hide_or_show(service: &mut Service, args: &Value, rows: bool, hidden: bool) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let (first, count) = span_args(args, rows)?;
    let open = service.open_document(id)?;
    let (sheet, name) = sheet_id(open, args.str_arg("sheet")?)?;
    let sheets = sheets_of(open)?;
    let data = sheets.iter().find(|(_, n, _)| *n == name).map(|(_, _, s)| *s).expect("the sheet was found");
    let sizes = if rows { data.rows.len() } else { data.columns.len() } as u32;
    let last = first + count - 1;
    if last >= sizes {
        return Err(ToolError::new("out_of_range", format!("the sheet has {sizes} {} laid out; {} {} is past them", if rows { "rows" } else { "columns" }, if rows { "row" } else { "column" }, last + 1)));
    }
    let size_of = |i: u32| if rows { data.rows[i as usize] } else { data.columns[i as usize] };
    if (first..=last).all(|i| (size_of(i) == 0) == hidden) {
        return Err(ToolError::invalid(if hidden { "they are hidden already" } else { "none of them is hidden" }));
    }
    let axis = if rows { Axis::Rows } else { Axis::Columns };
    sheet_edit(service, id, vec![SheetCommand::SetHidden { sheet, axis, first, last, hidden }], json!({}))
}

fn hide_rows(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    hide_or_show(service, args, true, true)
}

fn show_rows(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    hide_or_show(service, args, true, false)
}

fn hide_columns(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    hide_or_show(service, args, false, true)
}

fn show_columns(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    hide_or_show(service, args, false, false)
}

fn set_freeze_panes(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "cell"])?;
    let id = args.str_arg("document_id")?;
    let frozen = match args.get("cell") {
        Some(Value::Null) => None,
        Some(Value::String(text)) => {
            let r = parse_range(text)?;
            if r.first_row != r.last_row || r.first_column != r.last_column {
                return Err(ToolError::invalid("cell is one cell such as B2"));
            }
            Some((r.first_column, r.first_row)).filter(|&(c, r)| c > 0 || r > 0)
        }
        _ => return Err(ToolError::invalid("cell is an A1 cell such as B2, or null")),
    };
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    sheet_edit(service, id, vec![SheetCommand::Freeze { sheet, frozen }], json!({}))
}

fn set_sheet_view(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "sheet", "gridlines", "headings", "zoom", "tab_color", "show_formulas", "show_zeros"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let (sheet, _) = sheet_id(open, args.str_arg("sheet")?)?;
    let mut commands = Vec::new();
    let mut warnings = Vec::new();
    if let Some(v) = args.get("gridlines") {
        commands.push(SheetCommand::SetGridLines { sheet, shown: parse_bool("gridlines", v)? });
    }
    if let Some(v) = args.get("headings") {
        commands.push(SheetCommand::SetHeadings { sheet, shown: parse_bool("headings", v)? });
    }
    if let Some(v) = args.get("zoom") {
        let zoom = match v {
            Value::Null => None,
            v => Some(v.as_u64().filter(|z| (10..=400).contains(z)).ok_or_else(|| ToolError::invalid("zoom is 10 to 400 percent, or null"))? as u16),
        };
        commands.push(SheetCommand::SetZoom { sheet, zoom: zoom.filter(|z| *z != 100) });
    }
    if let Some(v) = args.get("tab_color") {
        let color = match v {
            Value::Null => None,
            v => Some(rgb_of(open, "tab_color", v, &mut warnings)?),
        };
        commands.push(SheetCommand::SetTabColor { sheet, color });
    }
    if let Some(v) = args.get("show_formulas") {
        commands.push(SheetCommand::SetShowFormulas { sheet, shown: parse_bool("show_formulas", v)? });
    }
    if let Some(v) = args.get("show_zeros") {
        commands.push(SheetCommand::SetShowZeros { sheet, shown: parse_bool("show_zeros", v)? });
    }
    if commands.is_empty() {
        return Err(ToolError::invalid("no setting is given"));
    }
    execute(open, commands.into_iter().map(ibis_doc_model::Command::Sheet).collect())?;
    Ok(edited(id, open, json!({}), warnings))
}

fn points_to_emu(name: &str, value: &Value) -> Result<i64, ToolError> {
    let pt = value.as_f64().filter(|p| (0.0..=14_400.0).contains(p)).ok_or_else(|| ToolError::invalid(format!("{name} is a number of points, 0 to 14400")))?;
    Ok((pt * EMU_PER_POINT as f64).round() as i64)
}

fn paper_size(value: &Value) -> Result<(i64, i64), ToolError> {
    let mm = |w: i64, h: i64| (w * 36_000, h * 36_000);
    match value {
        Value::String(s) => Ok(match s.as_str() {
            "a4" => mm(210, 297),
            "a3" => mm(297, 420),
            "a5" => mm(148, 210),
            "b4" => mm(257, 364),
            "b5" => mm(182, 257),
            "letter" => (ibis_doc_model::EMU_PER_INCH * 17 / 2, ibis_doc_model::EMU_PER_INCH * 11),
            "legal" => (216 * 36_000, 356 * 36_000),
            "hagaki" => SheetPrint::HAGAKI,
            other => return Err(ToolError::invalid(format!("paper {other} is not a4, a3, a5, b4, b5, letter, legal or hagaki"))),
        }),
        Value::Object(map) => {
            let get = |k: &str| map.get(k).ok_or_else(|| ToolError::invalid("paper is a name or { width, height }")).and_then(|v| points_to_emu(k, v));
            let (w, h) = (get("width")?, get("height")?);
            if w == 0 || h == 0 || map.len() != 2 {
                return Err(ToolError::invalid("paper is a name or { width, height } in points"));
            }
            Ok((w.min(h), w.max(h)))
        }
        _ => Err(ToolError::invalid("paper is a name or { width, height }")),
    }
}

/// The reference `Print_Titles` holds: `$1:$2`, `$A:$B` or both, written with the sheet's name.
fn print_titles_formula(sheet_name: &str, text: &str) -> Result<String, ToolError> {
    let bad = || ToolError::invalid("print_titles is rows and/or columns such as \"$1:$2\", \"$A:$B\" or \"$A:$B,$1:$2\"");
    let quoted = ibis_calc::formula::quote_sheet(sheet_name);
    let mut parts = Vec::new();
    for part in text.split(',') {
        let part = part.trim().replace('$', "");
        let (a, b) = part.split_once(':').unwrap_or((part.as_str(), part.as_str()));
        let rows = a.chars().all(|c| c.is_ascii_digit()) && b.chars().all(|c| c.is_ascii_digit()) && !a.is_empty() && !b.is_empty();
        let columns = a.chars().all(|c| c.is_ascii_alphabetic()) && b.chars().all(|c| c.is_ascii_alphabetic()) && !a.is_empty() && !b.is_empty();
        if rows {
            if a.parse::<u32>().ok().filter(|n| *n >= 1).is_none() || b.parse::<u32>().ok().filter(|n| *n >= 1).is_none() {
                return Err(bad());
            }
            parts.push(format!("{quoted}!${}:${}", a, b));
        } else if columns {
            parts.push(format!("{quoted}!${}:${}", a.to_ascii_uppercase(), b.to_ascii_uppercase()));
        } else {
            return Err(bad());
        }
    }
    if parts.is_empty() || parts.len() > 2 {
        return Err(bad());
    }
    Ok(parts.join(","))
}

fn set_page_setup(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(
        args,
        &["document_id", "sheet", "paper", "orientation", "margins", "scale", "fit_to", "print_area", "print_titles", "header", "footer", "center_horizontally", "center_vertically", "print_gridlines", "print_headings", "black_and_white", "page_order"],
    )?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let (slide, name) = sheet_id(open, args.str_arg("sheet")?)?;
    let current: SheetPrint = sheets_of(open)?.iter().find(|(_, n, _)| *n == name).map(|(_, _, s)| s.print.clone()).expect("the sheet was found");
    let mut print = current;
    let mut changed = false;
    if let Some(v) = args.get("paper") {
        print.paper = paper_size(v)?;
        changed = true;
    }
    if let Some(v) = args.get("orientation") {
        print.landscape = match v.as_str() {
            Some("portrait") => false,
            Some("landscape") => true,
            _ => return Err(ToolError::invalid("orientation is portrait or landscape")),
        };
        changed = true;
    }
    if let Some(v) = args.get("margins") {
        let map = v.as_object().filter(|m| !m.is_empty()).ok_or_else(|| ToolError::invalid("margins is { left?, right?, top?, bottom?, header?, footer? } in points"))?;
        for (key, value) in map {
            let emu = points_to_emu(key, value)?;
            match key.as_str() {
                "left" => print.margins[0] = emu,
                "right" => print.margins[1] = emu,
                "top" => print.margins[2] = emu,
                "bottom" => print.margins[3] = emu,
                "header" => print.header_margin = emu,
                "footer" => print.footer_margin = emu,
                other => return Err(ToolError::invalid(format!("margins.{other} is not left, right, top, bottom, header or footer"))),
            }
        }
        changed = true;
    }
    if let Some(v) = args.get("scale") {
        print.scale = v.as_u64().filter(|s| (10..=400).contains(s)).ok_or_else(|| ToolError::invalid("scale is 10 to 400 percent"))? as u32;
        print.fit_to = None;
        changed = true;
    }
    if let Some(v) = args.get("fit_to") {
        print.fit_to = match v {
            Value::Null => None,
            Value::Object(map) => {
                let get = |k: &str| map.get(k).and_then(Value::as_u64).filter(|n| *n <= 32767).ok_or_else(|| ToolError::invalid("fit_to is { pages_wide, pages_tall } (0 for as many as it takes)"));
                Some((get("pages_wide")? as u32, get("pages_tall")? as u32))
            }
            _ => return Err(ToolError::invalid("fit_to is { pages_wide, pages_tall } or null")),
        };
        changed = true;
    }
    let text = |key: &str| -> Result<Option<Option<String>>, ToolError> {
        match args.get(key) {
            None => Ok(None),
            Some(Value::Null) => Ok(Some(None)),
            Some(Value::String(s)) => Ok(Some(Some(s.clone()))),
            Some(_) => Err(ToolError::invalid(format!("{key} is text or null"))),
        }
    };
    if let Some(h) = text("header")? {
        print.header_footer.odd_header = h;
        changed = true;
    }
    if let Some(f) = text("footer")? {
        print.header_footer.odd_footer = f;
        changed = true;
    }
    for (key, slot) in [("center_horizontally", 0), ("center_vertically", 1), ("print_gridlines", 2), ("print_headings", 3), ("black_and_white", 4)] {
        if let Some(v) = args.get(key) {
            let b = parse_bool(key, v)?;
            match slot {
                0 => print.centered.0 = b,
                1 => print.centered.1 = b,
                2 => print.grid_lines = b,
                3 => print.headings = b,
                _ => print.black_and_white = b,
            }
            changed = true;
        }
    }
    if let Some(v) = args.get("page_order") {
        print.over_then_down = match v.as_str() {
            Some("down_then_over") => false,
            Some("over_then_down") => true,
            _ => return Err(ToolError::invalid("page_order is down_then_over or over_then_down")),
        };
        changed = true;
    }
    let area = match args.get("print_area") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(parse_range(s)?)),
        Some(_) => return Err(ToolError::invalid("print_area is an A1 range or null")),
    };
    let titles = match args.get("print_titles") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => Some(Some(print_titles_formula(&name, s)?)),
        Some(_) => return Err(ToolError::invalid("print_titles is rows and/or columns, or null")),
    };
    if !changed && area.is_none() && titles.is_none() {
        return Err(ToolError::invalid("no setting is given"));
    }
    let index = workbook_index(open, &name)?;
    let mut commands = vec![SheetCommand::SetPrint { sheet: slide, print, area }];
    if let Some(formula) = titles {
        commands.push(SheetCommand::SetName { name: "_xlnm.Print_Titles".into(), sheet: Some(index), formula });
    }
    // A name that is not there cannot be deleted: `null` with no titles set is a no-op, not an error.
    if matches!(commands.last(), Some(SheetCommand::SetName { formula: None, .. })) {
        let has = open.document.presentation().workbook.as_ref().is_some_and(|w| w.names.iter().any(|n| n.name.eq_ignore_ascii_case("_xlnm.Print_Titles") && n.sheet == Some(index)));
        if !has {
            commands.pop();
        }
    }
    sheet_edit(service, id, commands, json!({}))
}

const ALLOWED: [&str; 15] = ["select_locked", "select_unlocked", "format_cells", "format_columns", "format_rows", "insert_columns", "insert_rows", "insert_hyperlinks", "delete_columns", "delete_rows", "sort", "auto_filter", "pivot_tables", "objects", "scenarios"];

pub(crate) fn allowed_json(a: &ibis_doc_model::sheet::ProtectionAllowed) -> Value {
    json!({
        "select_locked": a.select_locked, "select_unlocked": a.select_unlocked, "format_cells": a.format_cells, "format_columns": a.format_columns, "format_rows": a.format_rows,
        "insert_columns": a.insert_columns, "insert_rows": a.insert_rows, "insert_hyperlinks": a.insert_hyperlinks, "delete_columns": a.delete_columns, "delete_rows": a.delete_rows,
        "sort": a.sort, "auto_filter": a.auto_filter, "pivot_tables": a.pivot_tables, "objects": a.objects, "scenarios": a.scenarios,
    })
}

fn set_protection(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    if args.get("password").is_some() {
        return Err(ToolError::invalid("this tool takes no password: protect or unprotect only what has none"));
    }
    only_known(args, &["document_id", "sheet", "value", "allow"])?;
    let id = args.str_arg("document_id")?;
    let value = parse_bool("value", args.get("value").ok_or_else(|| ToolError::invalid("value is required"))?)?;
    let open = service.open_document(id)?;
    let locked = |has_password: bool| ToolError::new("unsupported_operation", if has_password { "it has a password, which this tool neither takes nor drops" } else { "" });
    match args.opt_str("sheet")? {
        Some(sheet_name) => {
            let (slide, name) = sheet_id(open, sheet_name)?;
            let existing = sheets_of(open)?.iter().find(|(_, n, _)| *n == name).and_then(|(_, _, s)| s.protection.clone());
            if existing.as_ref().is_some_and(|p| p.hash.is_some() || p.legacy.is_some()) {
                return Err(locked(true));
            }
            let mut protection = existing.unwrap_or_default();
            if let Some(allow) = args.get("allow") {
                let map = allow.as_object().ok_or_else(|| ToolError::invalid("allow is an object of booleans"))?;
                for (key, v) in map {
                    let b = parse_bool(key, v)?;
                    let a = &mut protection.allowed;
                    match key.as_str() {
                        "select_locked" => a.select_locked = b,
                        "select_unlocked" => a.select_unlocked = b,
                        "format_cells" => a.format_cells = b,
                        "format_columns" => a.format_columns = b,
                        "format_rows" => a.format_rows = b,
                        "insert_columns" => a.insert_columns = b,
                        "insert_rows" => a.insert_rows = b,
                        "insert_hyperlinks" => a.insert_hyperlinks = b,
                        "delete_columns" => a.delete_columns = b,
                        "delete_rows" => a.delete_rows = b,
                        "sort" => a.sort = b,
                        "auto_filter" => a.auto_filter = b,
                        "pivot_tables" => a.pivot_tables = b,
                        "objects" => a.objects = b,
                        "scenarios" => a.scenarios = b,
                        other => return Err(ToolError::invalid(format!("allow.{other} is not one of {}", ALLOWED.join(", ")))),
                    }
                }
            } else if value {
                // Nothing said: Excel's default (select cells only) unless it was protected already.
            }
            let protection: Option<SheetProtection> = value.then_some(protection);
            sheet_edit(service, id, vec![SheetCommand::SetProtection { sheet: slide, protection }], json!({ "sheet": name, "protected": value }))
        }
        None => {
            if args.get("allow").is_some() {
                return Err(ToolError::invalid("allow is for a sheet"));
            }
            sheets_of(open)?;
            let existing = open.document.presentation().workbook.as_ref().and_then(|w| w.protection.clone());
            if existing.as_ref().is_some_and(|p| p.hash.is_some() || p.legacy.is_some()) {
                return Err(locked(true));
            }
            let protection = value.then(|| WorkbookProtection { lock_structure: true, ..existing.unwrap_or_default() });
            sheet_edit(service, id, vec![SheetCommand::SetWorkbookProtection { protection }], json!({ "workbook_protected": value }))
        }
    }
}

fn set_calculation(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::sheet::{CalcMode, SheetFloat};
    only_known(args, &["document_id", "mode", "iterative", "max_iterations", "max_change"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    sheets_of(open)?;
    let workbook = open.document.presentation().workbook.as_ref().expect("a workbook has its record");
    let (iterate, max_iterations, max_change) = (workbook.iterate, workbook.max_iterations, workbook.max_change);
    let mut commands = Vec::new();
    if let Some(v) = args.get("mode") {
        let mode = match v.as_str() {
            Some("automatic") => CalcMode::Auto,
            Some("automatic_except_tables") => CalcMode::AutoNoTable,
            Some("manual") => CalcMode::Manual,
            _ => return Err(ToolError::invalid("mode is automatic, automatic_except_tables or manual")),
        };
        commands.push(SheetCommand::SetCalculationMode { mode });
    }
    if args.get("iterative").is_some() || args.get("max_iterations").is_some() || args.get("max_change").is_some() {
        let iterate = match args.get("iterative") {
            Some(v) => parse_bool("iterative", v)?,
            None => iterate,
        };
        let max_iterations = match args.get("max_iterations") {
            Some(v) => v.as_u64().filter(|n| (1..=32767).contains(n)).ok_or_else(|| ToolError::invalid("max_iterations is 1 to 32767"))? as u32,
            None => max_iterations,
        };
        let max_change = match args.get("max_change") {
            Some(v) => SheetFloat(v.as_f64().filter(|n| *n >= 0.0).ok_or_else(|| ToolError::invalid("max_change is a number from 0"))?),
            None => max_change,
        };
        commands.push(SheetCommand::SetIterativeCalculation { iterate, max_iterations, max_change });
    }
    if commands.is_empty() {
        return Err(ToolError::invalid("no setting is given"));
    }
    sheet_edit(service, id, commands, json!({}))
}

fn set_document_properties(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "title", "subject", "author", "keywords", "description", "category", "company", "manager"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    sheets_of(open)?;
    let mut props = open.document.presentation().workbook.as_ref().expect("a workbook has its record").properties.clone();
    let mut any = false;
    for key in ["title", "subject", "author", "keywords", "description", "category", "company", "manager"] {
        let Some(v) = args.get(key) else { continue };
        let value = match v {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            _ => return Err(ToolError::invalid(format!("{key} is text or null"))),
        };
        match key {
            "title" => props.title = value,
            "subject" => props.subject = value,
            "author" => props.creator = value,
            "keywords" => props.keywords = value,
            "description" => props.description = value,
            "category" => props.category = value,
            "company" => props.company = value,
            _ => props.manager = value,
        }
        any = true;
    }
    if !any {
        return Err(ToolError::invalid("no property is given"));
    }
    sheet_edit(service, id, vec![SheetCommand::SetDocumentProperties(props)], json!({}))
}
