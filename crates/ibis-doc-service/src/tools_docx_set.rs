// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `docx_*` setters of stage B step 2 (`tasks/doc/design/2026-11-24-159-mcp-stage-b.md` 4 and 8): paragraph format, page setup, links, tracked-change recording, the
//! document's properties and the page color. Each is read back by a tool of step 1 (`tools_docx_read.rs`); what is not given is not changed; one call is one undo step.

use ibis_doc_model::flow::{CellDirection, DocGridType, FlowAlign, LineHeight, Link, PageColumns};
use ibis_doc_model::flow_command::{FlowCommand, ParaChange};
use ibis_doc_model::{Command, Rgb};
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, boolean, document_id, integer, object};
use crate::edit::{ColorArg, edited, execute, only_known, parse_bool, parse_color};
use crate::error::ToolError;
use crate::service::{Args, Service};
use crate::tools_docx::{body_paragraphs, flow_of, range_of, range_schema};

const EMU_PER_POINT: f64 = 12_700.0;
const EMU_PER_MM: f64 = 36_000.0;

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "docx_set_paragraph_format",
            description: "Set the format of body paragraphs: alignment, indents (points; indent_first_line negative hangs), space before and after (points), line spacing ({ rule: multiple|exactly|at_least, value }: a multiple of single spacing, or points), keep_with_next, keep_together, page_break_before, widow_control. What is not given is not changed. Read it back with docx_get_paragraph.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("paragraphs", json!({ "type": "array", "minItems": 1, "items": { "type": "integer", "minimum": 0 }, "description": "Paragraph numbers." })),
                        ("alignment", json!({ "enum": ["left", "center", "right", "justify", "distribute"] })),
                        ("indent_left", json!({ "type": "number", "description": "Points." })),
                        ("indent_right", json!({ "type": "number", "description": "Points." })),
                        ("indent_first_line", json!({ "type": "number", "description": "Points; negative for a hanging indent." })),
                        ("space_before", json!({ "type": "number", "minimum": 0, "description": "Points." })),
                        ("space_after", json!({ "type": "number", "minimum": 0, "description": "Points." })),
                        ("line_spacing", json!({ "type": "object", "properties": { "rule": { "enum": ["multiple", "exactly", "at_least"] }, "value": { "type": "number", "exclusiveMinimum": 0 } }, "required": ["rule", "value"] })),
                        ("keep_with_next", boolean("Keep with the next paragraph.")),
                        ("keep_together", boolean("Keep the lines of the paragraph together.")),
                        ("page_break_before", boolean("Start a new page before the paragraph.")),
                        ("widow_control", boolean("Widow and orphan control.")),
                    ],
                    &["document_id", "paragraphs"],
                )
            },
            run: set_paragraph_format,
        },
        Tool {
            name: "docx_set_page_setup",
            description: "Set the page setup of the document's last (or only) section: paper ({ width, height } in points, or a name a4|a3|a5|b4|b5|letter|legal), orientation, margins ({ top, bottom, left, right } points), header_distance and footer_distance (points), columns ({ count, space }), text_direction (horizontal|vertical), grid ({ type: none|lines|lines_and_chars|chars, line_pitch?, char_space? } points). What is not given is not changed. Read it back with docx_get_page_setup.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("section", integer("The section (0-based); only the last one can be set today.")),
                        ("paper", json!({ "oneOf": [{ "enum": ["a3", "a4", "a5", "b4", "b5", "letter", "legal"] }, { "type": "object", "properties": { "width": { "type": "number", "exclusiveMinimum": 0 }, "height": { "type": "number", "exclusiveMinimum": 0 } }, "required": ["width", "height"] }] })),
                        ("orientation", json!({ "enum": ["portrait", "landscape"] })),
                        ("margins", json!({ "type": "object", "properties": { "top": { "type": "number", "minimum": 0 }, "bottom": { "type": "number", "minimum": 0 }, "left": { "type": "number", "minimum": 0 }, "right": { "type": "number", "minimum": 0 } } })),
                        ("header_distance", json!({ "type": "number", "minimum": 0 })),
                        ("footer_distance", json!({ "type": "number", "minimum": 0 })),
                        ("columns", json!({ "type": "object", "properties": { "count": { "type": "integer", "minimum": 1, "maximum": 12 }, "space": { "type": "number", "minimum": 0 }, "separator": { "type": "boolean" } } })),
                        ("text_direction", json!({ "enum": ["horizontal", "vertical"] })),
                        ("grid", json!({ "type": "object", "properties": { "type": { "enum": ["none", "lines", "lines_and_chars", "chars"] }, "line_pitch": { "type": "number", "exclusiveMinimum": 0 }, "char_space": { "type": "number" } } })),
                    ],
                    &["document_id"],
                )
            },
            run: set_page_setup,
        },
        Tool {
            name: "docx_set_hyperlink",
            description: "Make the text of a range a link to a url or a bookmark of the document, or take the link off (url and bookmark both null).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("range", range_schema()), ("url", json!({ "type": ["string", "null"] })), ("bookmark", json!({ "type": ["string", "null"] }))], &["document_id", "range"]),
            run: set_hyperlink,
        },
        Tool {
            name: "docx_set_track_changes",
            description: "Turn the recording of tracked changes on or off. Read it with docx_list_revisions (tracking).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("value", boolean("true: changes are recorded."))], &["document_id", "value"]),
            run: set_track_changes,
        },
        Tool {
            name: "docx_set_document_properties",
            description: "Set the document's properties: title, subject, author, keywords, description, category, company, manager (a string; null clears it). What is not given is not changed. Read them with app_get_document_info.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[document_id(), ("title", json!({ "type": ["string", "null"] })), ("subject", json!({ "type": ["string", "null"] })), ("author", json!({ "type": ["string", "null"] })), ("keywords", json!({ "type": ["string", "null"] })), ("description", json!({ "type": ["string", "null"] })), ("category", json!({ "type": ["string", "null"] })), ("company", json!({ "type": ["string", "null"] })), ("manager", json!({ "type": ["string", "null"] }))],
                    &["document_id"],
                )
            },
            run: set_document_properties,
        },
        Tool {
            name: "docx_set_page_color",
            description: "The color of the pages (\"#RRGGBB\"; null: none). Shown on the screen only, not printed. Read it with docx_get_page_setup.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("color", json!({ "type": ["string", "null"], "pattern": "^#[0-9A-Fa-f]{6}$" }))], &["document_id", "color"]),
            run: set_page_color,
        },
    ]
}

fn points(args: &Value, name: &str) -> Result<Option<i64>, ToolError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let n = v.as_f64().filter(|n| n.is_finite() && n.abs() < 100_000.0).ok_or_else(|| ToolError::invalid(format!("{name} is a number of points")))?;
            Ok(Some((n * EMU_PER_POINT).round() as i64))
        }
    }
}

fn set_paragraph_format(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "paragraphs", "alignment", "indent_left", "indent_right", "indent_first_line", "space_before", "space_after", "line_spacing", "keep_with_next", "keep_together", "page_break_before", "widow_control"])?;
    let id = args.str_arg("document_id")?;
    let list = args.get("paragraphs").and_then(Value::as_array).filter(|l| !l.is_empty()).ok_or_else(|| ToolError::invalid("paragraphs is a list of paragraph numbers"))?;
    let mut changes = Vec::new();
    if let Some(a) = args.opt_str("alignment")? {
        changes.push(ParaChange::Alignment(match a {
            "left" => FlowAlign::Left,
            "center" => FlowAlign::Center,
            "right" => FlowAlign::Right,
            "justify" => FlowAlign::Justify,
            "distribute" => FlowAlign::Distribute,
            other => return Err(ToolError::invalid(format!("alignment {other} is not left, center, right, justify or distribute"))),
        }));
    }
    if let Some(v) = points(args, "indent_left")? {
        changes.push(ParaChange::LeftIndent(v));
    }
    if let Some(v) = points(args, "indent_right")? {
        changes.push(ParaChange::RightIndent(v));
    }
    if let Some(v) = points(args, "indent_first_line")? {
        changes.push(ParaChange::FirstLineIndent(v));
    }
    if let Some(v) = points(args, "space_before")? {
        if v < 0 {
            return Err(ToolError::invalid("space_before is not negative"));
        }
        changes.push(ParaChange::SpaceBefore(v));
    }
    if let Some(v) = points(args, "space_after")? {
        if v < 0 {
            return Err(ToolError::invalid("space_after is not negative"));
        }
        changes.push(ParaChange::SpaceAfter(v));
    }
    if let Some(spec) = args.get("line_spacing").filter(|v| !v.is_null()) {
        let bad = || ToolError::invalid("line_spacing is { \"rule\": \"multiple\"|\"exactly\"|\"at_least\", \"value\": a positive number }");
        let value = spec.get("value").and_then(Value::as_f64).filter(|v| *v > 0.0 && *v < 10_000.0).ok_or_else(bad)?;
        changes.push(ParaChange::LineSpacing(match spec.get("rule").and_then(Value::as_str).ok_or_else(bad)? {
            "multiple" => LineHeight::Auto((value * 240.0).round() as u32),
            "exactly" => LineHeight::Exact((value * EMU_PER_POINT).round() as i64),
            "at_least" => LineHeight::AtLeast((value * EMU_PER_POINT).round() as i64),
            _ => return Err(bad()),
        }));
    }
    if let Some(v) = args.get("keep_with_next") {
        changes.push(ParaChange::KeepWithNext(parse_bool("keep_with_next", v)?));
    }
    if let Some(v) = args.get("keep_together") {
        changes.push(ParaChange::KeepTogether(parse_bool("keep_together", v)?));
    }
    if let Some(v) = args.get("page_break_before") {
        changes.push(ParaChange::PageBreakBefore(parse_bool("page_break_before", v)?));
    }
    if let Some(v) = args.get("widow_control") {
        changes.push(ParaChange::WidowControl(parse_bool("widow_control", v)?));
    }
    if changes.is_empty() {
        return Err(ToolError::invalid("give at least one of alignment, indent_*, space_*, line_spacing, keep_with_next, keep_together, page_break_before, widow_control"));
    }
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let paragraphs = body_paragraphs(flow);
    let mut commands = Vec::new();
    for item in list {
        let n = item.as_u64().ok_or_else(|| ToolError::invalid("paragraphs is a list of paragraph numbers"))? as usize;
        let p = paragraphs.get(n).ok_or_else(|| ToolError::new("out_of_range", format!("paragraph {n}: the body has {} paragraphs", paragraphs.len())))?;
        for change in &changes {
            commands.push(FlowCommand::SetParagraphFormat { first: p.id, last: p.id, change: change.clone() });
        }
    }
    let count = list.len();
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({ "paragraphs": count }), Vec::new()))
}

/// A paper by name, in millimetres (ISO 216 A series, JIS B series, US Letter and Legal).
fn paper_mm(name: &str) -> Option<(f64, f64)> {
    Some(match name {
        "a3" => (297.0, 420.0),
        "a4" => (210.0, 297.0),
        "a5" => (148.0, 210.0),
        "b4" => (257.0, 364.0),
        "b5" => (182.0, 257.0),
        "letter" => (215.9, 279.4),
        "legal" => (215.9, 355.6),
        _ => return None,
    })
}

fn set_page_setup(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "section", "paper", "orientation", "margins", "header_distance", "footer_distance", "columns", "text_direction", "grid"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let sections = ibis_doc_model::flow_command::sections(flow).len();
    if let Some(section) = args.opt_usize("section")? {
        if section >= sections {
            return Err(ToolError::new("out_of_range", format!("section {section}: the document has {sections} sections")));
        }
        if section + 1 != sections {
            return Err(ToolError::new("unsupported_operation", "only the last section's page setup can be set"));
        }
    }
    let mut page = flow.page;
    match args.get("paper") {
        None | Some(Value::Null) => {}
        Some(Value::String(name)) => {
            let (w, h) = paper_mm(name).ok_or_else(|| ToolError::invalid(format!("paper {name} is not a3, a4, a5, b4, b5, letter or legal")))?;
            // The orientation stays: a landscape page is the paper turned.
            let (w, h) = ((w * EMU_PER_MM).round() as i64, (h * EMU_PER_MM).round() as i64);
            page.size = if page.size.width > page.size.height { ibis_doc_model::EmuSize { width: h, height: w } } else { ibis_doc_model::EmuSize { width: w, height: h } };
        }
        Some(spec) => {
            let (w, h) = (points(spec, "width")?, points(spec, "height")?);
            let (Some(w), Some(h)) = (w, h) else { return Err(ToolError::invalid("paper is a name or { width, height } in points")) };
            if w <= 0 || h <= 0 {
                return Err(ToolError::invalid("paper's width and height are positive"));
            }
            page.size = ibis_doc_model::EmuSize { width: w, height: h };
        }
    }
    match args.opt_str("orientation")? {
        None => {}
        Some(o @ ("portrait" | "landscape")) => {
            let landscape = o == "landscape";
            if (page.size.width > page.size.height) != landscape {
                page.size = ibis_doc_model::EmuSize { width: page.size.height, height: page.size.width };
            }
        }
        Some(other) => return Err(ToolError::invalid(format!("orientation {other} is not portrait or landscape"))),
    }
    if let Some(m) = args.get("margins").filter(|v| !v.is_null()) {
        for (name, slot) in [("top", &mut page.top), ("bottom", &mut page.bottom), ("left", &mut page.left), ("right", &mut page.right)] {
            if let Some(v) = points(m, name)? {
                if v < 0 {
                    return Err(ToolError::invalid(format!("margins.{name} is not negative")));
                }
                *slot = v;
            }
        }
    }
    if let Some(v) = points(args, "header_distance")? {
        page.header = v.max(0);
    }
    if let Some(v) = points(args, "footer_distance")? {
        page.footer = v.max(0);
    }
    if let Some(c) = args.get("columns").filter(|v| !v.is_null()) {
        let mut columns: PageColumns = page.columns;
        if let Some(n) = c.get("count").and_then(Value::as_u64) {
            if !(1..=PageColumns::MAX as u64).contains(&n) {
                return Err(ToolError::invalid(format!("columns.count is 1 to {}", PageColumns::MAX)));
            }
            columns.count = n as u8;
            columns.widths = None;
        }
        if let Some(v) = points(c, "space")? {
            columns.space = v.max(0);
        }
        if let Some(v) = c.get("separator") {
            columns.separator = parse_bool("columns.separator", v)?;
        }
        page.columns = columns;
    }
    match args.opt_str("text_direction")? {
        None => {}
        Some("horizontal") => page.direction = CellDirection::LrTb,
        Some("vertical") => page.direction = CellDirection::TbRl,
        Some(other) => return Err(ToolError::invalid(format!("text_direction {other} is not horizontal or vertical"))),
    }
    if let Some(g) = args.get("grid").filter(|v| !v.is_null()) {
        if let Some(kind) = g.get("type").and_then(Value::as_str) {
            page.doc_grid_type = match kind {
                "none" => DocGridType::Default,
                "lines" => DocGridType::Lines,
                "lines_and_chars" => DocGridType::LinesAndChars,
                "chars" => DocGridType::SnapToChars,
                other => return Err(ToolError::invalid(format!("grid.type {other} is not none, lines, lines_and_chars or chars"))),
            };
        }
        if let Some(v) = points(g, "line_pitch")? {
            if v <= 0 {
                return Err(ToolError::invalid("grid.line_pitch is positive"));
            }
            page.line_pitch = Some(v);
        }
        if let Some(v) = points(g, "char_space")? {
            page.char_space = Some(v);
        }
        if page.doc_grid_type == DocGridType::Default {
            page.line_pitch = None;
            page.char_space = None;
        }
    }
    if page == flow.page {
        return Ok(edited(id, open, json!({ "changed": false }), Vec::new()));
    }
    execute(open, vec![Command::Flow(FlowCommand::SetPage { page })])?;
    Ok(edited(id, open, json!({ "changed": true }), Vec::new()))
}

fn set_hyperlink(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "range", "url", "bookmark"])?;
    let id = args.str_arg("document_id")?;
    let (url, bookmark) = (args.get("url").and_then(Value::as_str), args.get("bookmark").and_then(Value::as_str));
    let link = match (url, bookmark) {
        (Some(_), Some(_)) => return Err(ToolError::invalid("give a url or a bookmark, not both")),
        (Some(u), None) if u.trim().is_empty() => return Err(ToolError::invalid("url is empty")),
        (Some(u), None) => Some(Link::Url(u.to_string())),
        (None, Some(b)) if b.trim().is_empty() => return Err(ToolError::invalid("bookmark is empty")),
        (None, Some(b)) => Some(Link::Anchor(b.to_string())),
        (None, None) => None,
    };
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let (from, to) = range_of(flow, args)?;
    if from.para != to.para {
        return Err(ToolError::new("unsupported_operation", "a link is in one paragraph: give a range inside one paragraph"));
    }
    if from.offset == to.offset {
        return Err(ToolError::invalid("the range is empty"));
    }
    if let Some(Link::Anchor(name)) = &link {
        let known = body_paragraphs(flow).iter().any(|p| p.bookmarks.iter().any(|b| b == name));
        if !known {
            return Err(ToolError::invalid(format!("the document has no bookmark {name}")));
        }
    }
    execute(open, vec![Command::Flow(FlowCommand::SetLink { from, to, link })])?;
    Ok(edited(id, open, json!({}), Vec::new()))
}

fn set_track_changes(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "value"])?;
    let id = args.str_arg("document_id")?;
    let value = parse_bool("value", args.get("value").ok_or_else(|| ToolError::invalid("value is required"))?)?;
    let open = service.open_document(id)?;
    if flow_of(open)?.track_changes == value {
        return Ok(edited(id, open, json!({ "changed": false }), Vec::new()));
    }
    execute(open, vec![Command::Flow(FlowCommand::SetTrackChanges(value))])?;
    Ok(edited(id, open, json!({ "changed": true }), Vec::new()))
}

fn set_document_properties(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "title", "subject", "author", "keywords", "description", "category", "company", "manager"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let mut properties = flow_of(open)?.properties.clone();
    let before = properties.clone();
    for (name, slot) in [
        ("title", &mut properties.title),
        ("subject", &mut properties.subject),
        ("author", &mut properties.creator),
        ("keywords", &mut properties.keywords),
        ("description", &mut properties.description),
        ("category", &mut properties.category),
        ("company", &mut properties.company),
        ("manager", &mut properties.manager),
    ] {
        match args.get(name) {
            None => {}
            Some(Value::Null) => *slot = None,
            Some(Value::String(s)) => *slot = Some(s.clone()),
            Some(_) => return Err(ToolError::invalid(format!("{name} is a string or null"))),
        }
    }
    if properties == before {
        return Ok(edited(id, open, json!({ "changed": false }), Vec::new()));
    }
    execute(open, vec![Command::Flow(FlowCommand::SetDocumentProperties(properties))])?;
    Ok(edited(id, open, json!({ "changed": true }), Vec::new()))
}

fn set_page_color(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "color"])?;
    let id = args.str_arg("document_id")?;
    let color: Option<Rgb> = match parse_color("color", args.get("color").ok_or_else(|| ToolError::invalid("color is required: \"#RRGGBB\" or null"))?)? {
        ColorArg::Rgb(c) => Some(c),
        ColorArg::Inherit => None,
        ColorArg::Theme { .. } => return Err(ToolError::invalid("color is \"#RRGGBB\" or null: a page color is not a theme color")),
    };
    let open = service.open_document(id)?;
    if flow_of(open)?.page_color == color {
        return Ok(edited(id, open, json!({ "changed": false }), Vec::new()));
    }
    execute(open, vec![Command::Flow(FlowCommand::SetPageColor(color))])?;
    Ok(edited(id, open, json!({ "changed": true }), Vec::new()))
}
