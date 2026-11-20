// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `docx_*` tools (design 6.2). Paragraphs are numbered in the body (0-based, the paragraphs of tables not counted: a table has its own number).

use ibis_doc_model::flow::{Block, FlowDocument, FlowParagraph, FlowRunKind, FlowTable};
use ibis_formats::Kind;
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, boolean, document_id, integer, object, string};
use crate::error::ToolError;
use crate::service::{Args, Open, Service, page};

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "docx_get_outline", description: "The headings of the document: paragraph number, level and text.", access: Access::Read, returns_content: true, schema: || object(&[document_id()], &["document_id"]), run: get_outline },
        Tool {
            name: "docx_get_text",
            description: "The text of the document as Markdown (headings, tables) or plain text, paged by lines.",
            access: Access::Read,
            returns_content: true,
            schema: || object(&[document_id(), ("format", json!({ "enum": ["markdown", "plain"], "description": "markdown (default) or plain." })), ("offset", integer("The first line.")), ("limit", integer("The most lines."))], &["document_id"]),
            run: get_text,
        },
        Tool {
            name: "docx_list_paragraphs",
            description: "The body paragraphs: number, id, style and text.",
            access: Access::Read,
            returns_content: true,
            schema: || object(&[document_id(), ("offset", integer("The first paragraph.")), ("limit", integer("The most paragraphs."))], &["document_id"]),
            run: list_paragraphs,
        },
        Tool { name: "docx_get_paragraph", description: "One paragraph with its style and the formatting of each run.", access: Access::Read, returns_content: true, schema: || object(&[document_id(), ("paragraph", integer("The paragraph number."))], &["document_id", "paragraph"]), run: get_paragraph },
        Tool { name: "docx_list_tables", description: "The tables of the body: number, rows and columns.", access: Access::Read, returns_content: false, schema: || object(&[document_id()], &["document_id"]), run: list_tables },
        Tool {
            name: "docx_find_text",
            description: "Where a text is in the body paragraphs.",
            access: Access::Read,
            returns_content: true,
            schema: || object(&[document_id(), ("query", string("The text to find.")), ("match_case", boolean("Match upper and lower case (default false).")), ("limit", integer("The most places."))], &["document_id", "query"]),
            run: find_text,
        },
    ]
}

/// The open document, which must be a docx kind.
pub(crate) fn flow_of(open: &mut Open) -> Result<&FlowDocument, ToolError> {
    if open.kind != Kind::Document {
        return Err(ToolError::new("unsupported_operation", "this tool is for a docx document"));
    }
    open.document.presentation().flow.as_deref().ok_or_else(|| ToolError::new("unsupported_operation", "this tool is for a docx document"))
}

/// The text of a paragraph (tabs as tab characters; what is not text is left out).
pub(crate) fn paragraph_text(p: &FlowParagraph) -> String {
    let mut text = String::new();
    for run in &p.runs {
        match &run.kind {
            FlowRunKind::Text(t) => text.push_str(t),
            FlowRunKind::Tab => text.push('\t'),
            _ => {}
        }
    }
    text
}

/// The body's paragraphs with their numbers.
pub(crate) fn body_paragraphs(flow: &FlowDocument) -> Vec<&FlowParagraph> {
    flow.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).collect()
}

fn body_tables(flow: &FlowDocument) -> Vec<&FlowTable> {
    flow.body.iter().filter_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).collect()
}

fn heading_level(style: Option<&str>) -> Option<usize> {
    let style = style?.trim();
    let lower = style.to_lowercase();
    let rest = lower.strip_prefix("heading").or_else(|| style.strip_prefix("見出し"))?;
    rest.trim().parse::<usize>().ok().filter(|n| (1..=9).contains(n))
}

fn cell_text(blocks: &[Block]) -> String {
    blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(paragraph_text(p)) } else { None }).collect::<Vec<_>>().join(" ").replace('|', "\\|")
}

fn table_lines(t: &FlowTable, markdown: bool) -> Vec<String> {
    let rows: Vec<Vec<String>> = t.rows.iter().map(|r| r.cells.iter().map(|c| cell_text(&c.blocks)).collect()).collect();
    let mut lines = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if markdown {
            lines.push(format!("| {} |", row.join(" | ")));
            if i == 0 {
                lines.push(format!("|{}|", vec![" --- "; row.len().max(1)].join("|")));
            }
        } else {
            lines.push(row.join("\t"));
        }
    }
    lines
}

fn get_outline(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let headings: Vec<Value> = body_paragraphs(flow)
        .iter()
        .enumerate()
        .filter_map(|(n, p)| heading_level(p.style.as_deref()).map(|level| json!({ "paragraph": n, "level": level, "text": paragraph_text(p) })))
        .collect();
    Ok(json!({ "headings": headings }))
}

fn get_text(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let markdown = match args.opt_str("format")? {
        None | Some("markdown") => true,
        Some("plain") => false,
        Some(other) => return Err(ToolError::invalid(format!("format {other} is not markdown or plain"))),
    };
    let page_bytes = service.limits.page_bytes;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let mut lines: Vec<String> = Vec::new();
    for block in &flow.body {
        match block {
            Block::Paragraph(p) => {
                let text = paragraph_text(p);
                lines.push(match (markdown, heading_level(p.style.as_deref())) {
                    (true, Some(level)) => format!("{} {text}", "#".repeat(level)),
                    _ => text,
                });
            }
            Block::Table(t) => lines.extend(table_lines(t, markdown)),
        }
    }
    // The lines of a page, and not more bytes than the page holds (a long line starts a page of its own).
    let offset = args.opt_usize("offset")?.unwrap_or(0);
    if offset > lines.len() {
        return Err(ToolError::new("out_of_range", format!("offset {offset} is past the {} lines", lines.len())));
    }
    let limit = args.opt_usize("limit")?.unwrap_or(usize::MAX);
    let (mut end, mut bytes) = (offset, 0usize);
    while end < lines.len() && end - offset < limit && (end == offset || bytes + lines[end].len() < page_bytes) {
        bytes += lines[end].len() + 1;
        end += 1;
    }
    let mut result = json!({ "text": lines[offset..end].join("\n"), "lines": lines.len() });
    if end < lines.len() {
        result["next_offset"] = json!(end);
    }
    Ok(result)
}

fn list_paragraphs(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let items: Vec<Value> = body_paragraphs(flow).iter().enumerate().map(|(n, p)| json!({ "paragraph": n, "para_id": p.id, "style": p.style, "text": paragraph_text(p) })).collect();
    let (shown, next) = page(args, &items, 200)?;
    let mut result = json!({ "paragraphs": shown, "total": items.len() });
    if let Some(next) = next {
        result["next_offset"] = json!(next);
    }
    Ok(result)
}

fn color_text(color: &ibis_doc_model::Rgb) -> String {
    format!("#{:06X}", color.0 & 0xFF_FFFF)
}

fn get_paragraph(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let n = args.usize_arg("paragraph")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let paragraphs = body_paragraphs(flow);
    let p = paragraphs.get(n).ok_or_else(|| ToolError::new("out_of_range", format!("paragraph {n}: the body has {} paragraphs", paragraphs.len())))?;
    let runs: Vec<Value> = p
        .runs
        .iter()
        .filter_map(|r| match &r.kind {
            FlowRunKind::Text(t) => Some(json!({
                "text": t,
                "bold": r.style.bold,
                "italic": r.style.italic,
                "underline": r.style.underline,
                "strike": r.style.strike,
                "size": r.style.size as f64 / 100.0,
                "color": color_text(&r.style.color),
                "font": r.style.font,
                "east_asian_font": r.style.east_asian_font,
            })),
            _ => None,
        })
        .collect();
    Ok(json!({ "paragraph": n, "para_id": p.id, "style": p.style, "text": paragraph_text(p), "runs": runs }))
}

fn list_tables(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let tables: Vec<Value> = body_tables(flow).iter().enumerate().map(|(n, t)| json!({ "table": n, "rows": t.rows.len(), "columns": t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0) })).collect();
    Ok(json!({ "tables": tables }))
}

fn find_text(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let query = args.str_arg("query")?;
    if query.is_empty() {
        return Err(ToolError::invalid("query is empty"));
    }
    let match_case = args.opt_bool("match_case")?.unwrap_or(false);
    let limit = args.opt_usize("limit")?.unwrap_or(100).max(1);
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let fold = |s: &str| if match_case { s.to_string() } else { s.to_lowercase() };
    let needle: Vec<char> = fold(query).chars().collect();
    let mut found = Vec::new();
    'outer: for (n, p) in body_paragraphs(flow).iter().enumerate() {
        let text: Vec<char> = fold(&paragraph_text(p)).chars().collect();
        let mut at = 0;
        while at + needle.len() <= text.len() {
            if text[at..at + needle.len()] == needle[..] {
                found.push(json!({ "paragraph": n, "offset": at, "length": needle.len() }));
                if found.len() >= limit {
                    break 'outer;
                }
                at += needle.len().max(1);
            } else {
                at += 1;
            }
        }
    }
    Ok(json!({ "matches": found }))
}

// ---- edits ----

use ibis_doc_model::flow::FlowThemeColor;
use ibis_doc_model::flow_command::{FlowCommand, FlowPos, RunChange, RunReset};
use ibis_doc_model::Command;

use crate::edit::{ColorArg, FontArg, edited, execute, only_known, parse_bool, parse_color, parse_font, parse_size};

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "docx_insert_text",
            description: "Insert text at a position of a body paragraph (no line breaks: use docx_insert_paragraph for a new paragraph).",
            access: Access::Edit,
            returns_content: false,
            schema: || object(&[document_id(), ("at", position_schema()), ("text", string("The text."))], &["document_id", "at", "text"]),
            run: insert_text,
        },
        Tool { name: "docx_delete_text", description: "Delete the text of a range.", access: Access::Edit, returns_content: false, schema: || object(&[document_id(), ("range", range_schema())], &["document_id", "range"]), run: delete_text },
        Tool {
            name: "docx_replace_text",
            description: "Replace every (or the first) place of a text in the body and the tables with another text, as one undo step.",
            access: Access::Edit,
            returns_content: false,
            schema: || object(&[document_id(), ("find", string("The text to find.")), ("replace", string("The new text (no line breaks).")), ("match_case", boolean("Match upper and lower case (default false).")), ("all", boolean("Replace every place (default true); false: the first one."))], &["document_id", "find", "replace"]),
            run: replace_text,
        },
        Tool {
            name: "docx_set_run_format",
            description: "Set the character format of a range: bold, italic, underline, strike, size (pt), color (\"#RRGGBB\" or { theme, brightness }), font (a name, or { latin, east_asian, complex_script }). What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: || {
                object(
                    &[document_id(), ("range", range_schema()), ("bold", boolean("Bold.")), ("italic", boolean("Italic.")), ("underline", boolean("Underline.")), ("strike", boolean("Strikethrough.")), ("size", json!({ "type": "number", "description": "Points, 1 to 409.5 in steps of 0.5." })), ("color", color_schema()), ("font", font_schema())],
                    &["document_id", "range"],
                )
            },
            run: set_run_format,
        },
        Tool {
            name: "docx_insert_table",
            description: "Insert a table at a position (the position's paragraph is split there), optionally with the text of its cells (rows of strings).",
            access: Access::Edit,
            returns_content: false,
            schema: || object(&[document_id(), ("at", position_schema()), ("rows", integer("Rows.")), ("columns", integer("Columns.")), ("cells", json!({ "type": "array", "description": "The text of the cells, row by row.", "items": { "type": "array", "items": { "type": "string" } } })), ("width", json!({ "type": "number", "description": "The table's width in points (default 432)." }))], &["document_id", "at", "rows", "columns"]),
            run: insert_table,
        },
    ]
}

fn position_schema() -> Value {
    json!({ "type": "object", "description": "A position: { \"paragraph\": the body paragraph number, \"offset\": characters from its start }.", "properties": { "paragraph": integer("Paragraph number."), "offset": integer("Characters from the start.") }, "required": ["paragraph", "offset"] })
}

fn range_schema() -> Value {
    json!({ "type": "object", "description": "{ \"from\": position, \"to\": position } (the end is not included).", "properties": { "from": position_schema(), "to": position_schema() }, "required": ["from", "to"] })
}

pub(crate) fn color_schema() -> Value {
    json!({ "description": "\"#RRGGBB\", or { \"theme\": text1|background1|text2|background2|accent1..accent6|hyperlink|followed_hyperlink, \"brightness\": -1..1 }.", "oneOf": [{ "type": "string", "pattern": "^#[0-9A-Fa-f]{6}$" }, { "type": "object", "properties": { "theme": { "type": "string" }, "brightness": { "type": "number", "minimum": -1, "maximum": 1 } }, "required": ["theme"] }] })
}

pub(crate) fn font_schema() -> Value {
    json!({ "description": "A typeface name for every script, or { latin, east_asian, complex_script }.", "oneOf": [{ "type": "string" }, { "type": "object", "properties": { "latin": { "type": "string" }, "east_asian": { "type": "string" }, "complex_script": { "type": "string" } } }] })
}

fn flow_pos(flow: &FlowDocument, value: &Value, name: &str) -> Result<FlowPos, ToolError> {
    let n = value.get("paragraph").and_then(Value::as_u64).ok_or_else(|| ToolError::invalid(format!("{name}.paragraph is the body paragraph number")))? as usize;
    let offset = value.get("offset").and_then(Value::as_u64).ok_or_else(|| ToolError::invalid(format!("{name}.offset is a number of characters")))? as usize;
    let paragraphs = body_paragraphs(flow);
    let p = paragraphs.get(n).ok_or_else(|| ToolError::new("out_of_range", format!("{name}: paragraph {n}; the body has {} paragraphs", paragraphs.len())))?;
    let length = p.runs.iter().map(|r| r.len()).sum::<usize>();
    if offset > length {
        return Err(ToolError::new("out_of_range", format!("{name}: offset {offset}; paragraph {n} has {length} characters")));
    }
    Ok(FlowPos::new(p.id, offset))
}

fn range_of(flow: &FlowDocument, args: &Value) -> Result<(FlowPos, FlowPos), ToolError> {
    let range = args.get("range").ok_or_else(|| ToolError::invalid("range is required"))?;
    let (from, to) = (flow_pos(flow, range.get("from").unwrap_or(&Value::Null), "range.from")?, flow_pos(flow, range.get("to").unwrap_or(&Value::Null), "range.to")?);
    let order = |p: &FlowPos| body_paragraphs(flow).iter().position(|q| q.id == p.para).unwrap_or(0);
    if (order(&from), from.offset) > (order(&to), to.offset) {
        return Err(ToolError::invalid("range.from is after range.to"));
    }
    Ok((from, to))
}

fn flow_edit(service: &mut Service, id: &str, commands: Vec<FlowCommand>, extra: Value) -> Result<Value, ToolError> {
    let open = service.open_document(id)?;
    execute(open, commands.into_iter().map(Command::Flow).collect())?;
    Ok(edited(id, open, extra, Vec::new()))
}

fn insert_text(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let text = args.str_arg("text")?;
    if text.contains(['\n', '\r']) {
        return Err(ToolError::invalid("text has a line break: use docx_insert_paragraph for a new paragraph"));
    }
    let open = service.open_document(id)?;
    let at = flow_pos(flow_of(open)?, args.get("at").unwrap_or(&Value::Null), "at")?;
    flow_edit(service, id, vec![FlowCommand::InsertText { at, text: text.to_string(), format: None }], json!({ "inserted": text.chars().count() }))
}

fn delete_text(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let (from, to) = range_of(flow_of(open)?, args)?;
    flow_edit(service, id, vec![FlowCommand::Delete { from, to }], json!({}))
}

fn replace_text(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let (find, replace) = (args.str_arg("find")?, args.str_arg("replace")?);
    if find.is_empty() {
        return Err(ToolError::invalid("find is empty"));
    }
    if replace.contains(['\n', '\r']) {
        return Err(ToolError::invalid("replace has a line break"));
    }
    let match_case = args.opt_bool("match_case")?.unwrap_or(false);
    let all = args.opt_bool("all")?.unwrap_or(true);
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let fold = |s: &str| -> Vec<char> { if match_case { s.chars().collect() } else { s.to_lowercase().chars().collect() } };
    let needle = fold(find);
    let mut commands = Vec::new();
    let mut count = 0usize;
    'all: for p in ibis_doc_model::all_paragraphs(flow) {
        // The characters the positions count: a text run's characters, one for anything else.
        let mut text: Vec<char> = Vec::new();
        for run in &p.runs {
            match &run.kind {
                FlowRunKind::Text(t) => text.extend(t.chars()),
                _ => text.extend(std::iter::repeat_n('\u{1}', run.len())),
            }
        }
        let haystack = fold(&text.iter().collect::<String>());
        if haystack.len() != text.len() {
            continue;
        }
        let mut found = Vec::new();
        let mut at = 0;
        while at + needle.len() <= haystack.len() {
            if haystack[at..at + needle.len()] == needle[..] {
                found.push(at);
                at += needle.len();
                if !all {
                    break;
                }
            } else {
                at += 1;
            }
        }
        // From the end, so that the offsets before stay what they were.
        for &start in found.iter().rev() {
            commands.push(FlowCommand::Delete { from: FlowPos::new(p.id, start), to: FlowPos::new(p.id, start + needle.len()) });
            if !replace.is_empty() {
                commands.push(FlowCommand::InsertText { at: FlowPos::new(p.id, start), text: replace.to_string(), format: None });
            }
            count += 1;
        }
        if !all && count > 0 {
            break 'all;
        }
    }
    if commands.is_empty() {
        return Ok(edited(id, open, json!({ "replaced": 0 }), Vec::new()));
    }
    flow_edit(service, id, vec![FlowCommand::Many(commands)], json!({ "replaced": count }))
}

fn theme_change(open: &Open, name: &str, brightness: f64) -> Result<RunChange, ToolError> {
    if matches!(name, "hyperlink" | "followed_hyperlink") {
        return Err(ToolError::new("unsupported_operation", "the hyperlink theme colours are not supported yet"));
    }
    // Word's themeTint (lighter) and themeShade (darker) are two hex digits: 255 * (1 - brightness) and 255 * (1 + brightness).
    let hex = |v: f64| format!("{:02X}", (v * 255.0).round() as u32);
    let (tint, shade) = if brightness > 0.0 { (Some(hex(1.0 - brightness)), None) } else if brightness < 0.0 { (None, Some(hex(1.0 + brightness))) } else { (None, None) };
    let theme_color = FlowThemeColor::new(name, tint, shade);
    let color = theme_color.resolve(&open.document.presentation().theme_colors).unwrap_or(ibis_doc_model::Rgb::BLACK);
    Ok(RunChange::ThemeColor { color, theme_color })
}

fn set_run_format(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "range", "bold", "italic", "underline", "strike", "size", "color", "font"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let (from, to) = range_of(flow_of(open)?, args)?;
    let mut changes = Vec::new();
    let mut warnings = Vec::new();
    // `null` takes a direct format away, so the styles' decide again (design 6.6).
    for (key, make, reset) in [("bold", RunChange::Bold as fn(bool) -> RunChange, RunReset::Bold), ("italic", RunChange::Italic, RunReset::Italic), ("underline", RunChange::Underline, RunReset::Underline), ("strike", RunChange::Strikethrough, RunReset::Strikethrough)] {
        if let Some(v) = args.get(key) {
            changes.push(match v {
                Value::Null => RunChange::Reset(reset),
                v => make(parse_bool(key, v)?),
            });
        }
    }
    if let Some(v) = args.get("size") {
        changes.push(match v {
            Value::Null => RunChange::Reset(RunReset::Size),
            v => RunChange::Size(parse_size("size", v)?),
        });
    }
    if let Some(v) = args.get("color") {
        changes.push(match parse_color("color", v)? {
            ColorArg::Rgb(rgb) => RunChange::Color(rgb),
            ColorArg::Theme { name, brightness } => theme_change(open, &name, brightness)?,
            ColorArg::Inherit => RunChange::Reset(RunReset::Color),
        });
    }
    if let Some(v) = args.get("font") {
        match parse_font("font", v)? {
            FontArg::All(name) => changes.push(RunChange::AllFonts(name)),
            FontArg::Inherit => changes.push(RunChange::Reset(RunReset::Fonts)),
            FontArg::Theme { major } => {
                // A run keeps the typeface's name (the theme's are resolved when the file is read): the theme's typefaces now are written.
                let fonts = open.document.presentation().theme_fonts.clone().ok_or_else(|| ToolError::new("unsupported_operation", "this document has no theme typefaces"))?;
                let set = if major { fonts.major } else { fonts.minor };
                if set.latin.is_none() && set.east_asian.is_none() && set.complex.is_none() {
                    return Err(ToolError::new("unsupported_operation", "the theme has no typeface of that kind"));
                }
                warnings.push(json!({ "code": "theme_font_resolved", "message": format!("the {} theme typeface was written as {} / {}; the text does not follow a later change of the theme", if major { "heading" } else { "body" }, set.latin.as_deref().unwrap_or("-"), set.east_asian.as_deref().unwrap_or("-")) }));
                changes.extend(set.latin.map(RunChange::Font));
                changes.extend(set.east_asian.map(RunChange::EastAsianFont));
                changes.extend(set.complex.map(RunChange::ComplexScriptFont));
            }
            FontArg::Split { latin, east_asian, complex_script } => {
                changes.extend(latin.map(RunChange::Font));
                changes.extend(east_asian.map(RunChange::EastAsianFont));
                changes.extend(complex_script.map(RunChange::ComplexScriptFont));
            }
        }
    }
    if changes.is_empty() {
        return Err(ToolError::invalid("no format is given (bold, italic, underline, strike, size, color or font)"));
    }
    let commands = changes.into_iter().map(|change| FlowCommand::SetRunFormat { from, to, change }).collect();
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({}), warnings))
}

fn insert_table(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let (rows, columns) = (args.usize_arg("rows")?, args.usize_arg("columns")?);
    if !(1..=500).contains(&rows) || !(1..=63).contains(&columns) {
        return Err(ToolError::new("out_of_range", "rows are 1 to 500 and columns 1 to 63"));
    }
    let width = match args.get("width") {
        None | Some(Value::Null) => 432.0,
        Some(v) => v.as_f64().filter(|w| (10.0..=2000.0).contains(w)).ok_or_else(|| ToolError::invalid("width is points, 10 to 2000"))?,
    };
    let cells: Vec<Vec<String>> = match args.get("cells") {
        None | Some(Value::Null) => Vec::new(),
        Some(v) => serde_json::from_value(v.clone()).map_err(|_| ToolError::invalid("cells is rows of strings"))?,
    };
    if cells.len() > rows || cells.iter().any(|r| r.len() > columns) {
        return Err(ToolError::new("out_of_range", "cells is larger than the table"));
    }
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let at = flow_pos(flow, args.get("at").unwrap_or(&Value::Null), "at")?;
    let before: std::collections::HashSet<u32> = flow.body.iter().filter_map(|b| if let Block::Table(t) = b { Some(t.id) } else { None }).collect();
    execute(open, vec![Command::Flow(FlowCommand::InsertTable { at, rows, columns, width: (width * 12700.0) as i64 })])?;
    if !cells.is_empty() {
        // The cells' text goes in the same undo step as the table.
        let flow = flow_of(open)?;
        let table = flow.body.iter().filter_map(|b| if let Block::Table(t) = b { Some(t) } else { None }).find(|t| !before.contains(&t.id));
        let mut fill = Vec::new();
        if let Some(table) = table {
            for (r, row) in cells.iter().enumerate() {
                for (c, text) in row.iter().enumerate() {
                    let paragraph = table.rows.get(r).and_then(|row| row.cells.get(c)).and_then(|cell| cell.blocks.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }));
                    if let (Some(p), false) = (paragraph, text.is_empty()) {
                        fill.push(Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p, 0), text: text.clone(), format: None }));
                    }
                }
            }
        }
        if !fill.is_empty() {
            open.document.amend_last(fill).map_err(|e| ToolError::new("edit_rejected", format!("{e:?}")))?;
        }
    }
    Ok(edited(id, open, json!({ "rows": rows, "columns": columns }), Vec::new()))
}
