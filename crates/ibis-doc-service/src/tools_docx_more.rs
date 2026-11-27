// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `docx_*` insertions and review tools of stage B step 4 (`tasks/doc/design/2026-11-24-159-mcp-stage-b.md` 4): a page break, a page number, a field, a content control, the
//! text of the header and footer, comments and the tracked changes' accept and reject. They are the model's own commands (`FlowCommand::InsertField`, `AddHeader`, and
//! `add_comment`, `delete_comment`, `resolve_revisions` of `flow_command`), which the screen uses too. Read back with `docx_get_paragraph`, `docx_get_page_setup`,
//! `docx_list_comments`, `docx_list_revisions` and `docx_list_content_controls`.

use ibis_doc_model::flow::{Block, FlowRunKind};
use ibis_doc_model::flow_command::{self, CommentAuthor, FlowCommand, FlowPos};
use ibis_doc_model::Command;
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, boolean, document_id, integer, object, string};
use crate::edit::{edited, execute, only_known};
use crate::error::ToolError;
use crate::service::{Args, Service};
use crate::tools_docx::{flow_of, flow_pos, position_schema, range_of, range_schema};

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "docx_insert_page_break",
            description: "Insert a page break at a position (the text after it goes on the next page).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("at", position_schema())], &["document_id", "at"]),
            run: insert_page_break,
        },
        Tool {
            name: "docx_insert_page_number",
            description: "Insert the page number (a PAGE field) at a position.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("at", position_schema())], &["document_id", "at"]),
            run: insert_page_number,
        },
        Tool {
            name: "docx_insert_field",
            description: "Insert a field at a position: its instruction as Word writes it after the field name (for example \"DATE\", \"SEQ Figure\" or \"REF bookmark\"), with the text it shows now (shown, may be empty).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("at", position_schema()), ("instruction", string("The field's instruction.")), ("shown", string("What the field shows (default empty)."))], &["document_id", "at", "instruction"]),
            run: insert_field,
        },
        Tool {
            name: "docx_insert_content_control",
            description: "Insert a content control at a position: kind text, rich_text, checkbox, date, drop_down_list or combo_box. text is a text control's content (empty: its prompt); items are the entries of a list; checked is a checkbox's state; date (YYYY-MM-DD, default today) and date_format (default yyyy/M/d) are a date control's. Read it back with docx_list_content_controls.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("at", position_schema()),
                        ("kind", json!({ "enum": ["text", "rich_text", "checkbox", "date", "drop_down_list", "combo_box"] })),
                        ("text", string("A text control's content.")),
                        ("items", json!({ "type": "array", "minItems": 1, "items": { "type": "string" }, "description": "The entries of a list." })),
                        ("checked", boolean("A checkbox's state (default false).")),
                        ("date", string("A date control's date, YYYY-MM-DD (default today).")),
                        ("date_format", string("A date control's format (default yyyy/M/d).")),
                    ],
                    &["document_id", "at", "kind"],
                )
            },
            run: insert_content_control,
        },
        Tool {
            name: "docx_set_header",
            description: "Set the text of the document's header (the last section's default one; it is made if the document has none). One paragraph of text (no line breaks); a header of several paragraphs is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("text", string("The header's text (empty clears it)."))], &["document_id", "text"]),
            run: |service, args| set_header_or_footer(service, args, false),
        },
        Tool {
            name: "docx_set_footer",
            description: "Set the text of the document's footer (the last section's default one; it is made if the document has none). One paragraph of text (no line breaks); a footer of several paragraphs is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("text", string("The footer's text (empty clears it)."))], &["document_id", "text"]),
            run: |service, args| set_header_or_footer(service, args, true),
        },
        Tool {
            name: "docx_add_comment",
            description: "Put a comment on the text of a range (not empty). author (default \"Assistant\") and date (UTC, as 2026-11-24T09:00:00Z; default now) are written with it. The result has the comment's id.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("range", range_schema()), ("text", string("The comment's text.")), ("author", string("Who writes it.")), ("date", string("When, UTC."))], &["document_id", "range", "text"]),
            run: add_comment,
        },
        Tool {
            name: "docx_delete_comment",
            description: "Delete a comment (an id from docx_list_comments), its replies and their marks.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("id", integer("The comment's id."))], &["document_id", "id"]),
            run: delete_comment,
        },
        Tool {
            name: "docx_apply_revisions",
            description: "Accept (accept true) or reject (false) the tracked changes: those in a range, or all of them without one. Read them with docx_list_revisions.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("accept", boolean("true: accept; false: reject.")), ("range", range_schema())], &["document_id", "accept"]),
            run: apply_revisions,
        },
    ]
}

fn run_commands(service: &mut Service, id: &str, commands: Vec<FlowCommand>, extra: Value) -> Result<Value, ToolError> {
    let open = service.open_document(id)?;
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, extra, Vec::new()))
}

fn at_of(service: &mut Service, args: &Value) -> Result<FlowPos, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    flow_pos(flow_of(open)?, args.get("at").unwrap_or(&Value::Null), "at")
}

fn insert_page_break(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at"])?;
    let at = at_of(service, args)?;
    run_commands(service, args.str_arg("document_id")?, vec![FlowCommand::InsertField { at, kind: FlowRunKind::PageBreak }], json!({}))
}

fn insert_page_number(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at"])?;
    let at = at_of(service, args)?;
    run_commands(service, args.str_arg("document_id")?, vec![FlowCommand::InsertPageNumber { at }], json!({}))
}

fn insert_field(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at", "instruction", "shown"])?;
    let instruction = args.str_arg("instruction")?.trim().to_string();
    if instruction.is_empty() || instruction.contains(['\n', '\r']) {
        return Err(ToolError::invalid("instruction is one line of the field's code, such as \"SEQ Figure\""));
    }
    let shown = args.opt_str("shown")?.unwrap_or("").to_string();
    if shown.contains(['\n', '\r']) {
        return Err(ToolError::invalid("shown has a line break"));
    }
    let at = at_of(service, args)?;
    run_commands(service, args.str_arg("document_id")?, vec![FlowCommand::InsertField { at, kind: FlowRunKind::Field { instruction, shown } }], json!({}))
}

fn insert_content_control(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at", "kind", "text", "items", "checked", "date", "date_format"])?;
    let kind = args.str_arg("kind")?;
    let text = args.opt_str("text")?;
    let items: Vec<String> = match args.get("items") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(list)) => list.iter().map(|v| v.as_str().map(str::to_string).ok_or_else(|| ToolError::invalid("items is a list of strings"))).collect::<Result<_, _>>()?,
        Some(_) => return Err(ToolError::invalid("items is a list of strings")),
    };
    if matches!(kind, "drop_down_list" | "combo_box") && items.is_empty() {
        return Err(ToolError::invalid(format!("a {kind} needs items")));
    }
    if !matches!(kind, "drop_down_list" | "combo_box") && !items.is_empty() {
        return Err(ToolError::invalid("items are for drop_down_list and combo_box"));
    }
    if text.is_some_and(|t| t.contains(['\n', '\r'])) {
        return Err(ToolError::invalid("text has a line break"));
    }
    let at = at_of(service, args)?;
    let prompt = || "Click or tap here to enter text.".to_string();
    let run = match kind {
        "text" => FlowRunKind::PlainText { text: text.map_or_else(prompt, str::to_string), placeholder: text.is_none() },
        "rich_text" => FlowRunKind::RichText { text: text.map_or_else(prompt, str::to_string), placeholder: text.is_none() },
        "checkbox" => FlowRunKind::CheckBox { checked: args.opt_bool("checked")?.unwrap_or(false), checked_char: '\u{2611}', unchecked_char: '\u{2610}' },
        "date" => {
            let format = args.opt_str("date_format")?.unwrap_or("yyyy/M/d").to_string();
            let time = match args.opt_str("date")? {
                None => ibis_doc_model::date_field::LocalTime::from_seconds(crate::clock::unix_seconds()),
                Some(d) => {
                    let bad = || ToolError::invalid("date is YYYY-MM-DD");
                    let parts: Vec<&str> = d.split('-').collect();
                    let [y, m, day] = parts[..] else { return Err(bad()) };
                    let (y, m, day): (i64, u32, u32) = (y.parse().map_err(|_| bad())?, m.parse().map_err(|_| bad())?, day.parse().map_err(|_| bad())?);
                    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
                        return Err(bad());
                    }
                    // Midnight of that day: the seconds of the civil date (the calendar's own conversion).
                    let mut t = ibis_doc_model::date_field::LocalTime::from_seconds(0);
                    t.year = y;
                    t.month = m;
                    t.day = day;
                    ibis_doc_model::date_field::LocalTime::from_seconds(t.to_seconds())
                }
            };
            let shown = ibis_doc_model::date_field::format_picture(&time, &format);
            FlowRunKind::DatePicker { full_date: time.to_iso_date(), date_format: format, text: shown }
        }
        "drop_down_list" | "combo_box" => FlowRunKind::DropDownList { items: items.iter().map(|i| (i.clone(), i.clone())).collect(), selected: 0, combo: kind == "combo_box" },
        other => return Err(ToolError::invalid(format!("kind {other} is not text, rich_text, checkbox, date, drop_down_list or combo_box"))),
    };
    run_commands(service, args.str_arg("document_id")?, vec![FlowCommand::InsertField { at, kind: run }], json!({}))
}

fn set_header_or_footer(service: &mut Service, args: &Value, footer: bool) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "text"])?;
    let id = args.str_arg("document_id")?;
    let text = args.str_arg("text")?;
    if text.contains(['\n', '\r']) {
        return Err(ToolError::invalid("text has a line break: a header or footer is set one paragraph at a time"));
    }
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let blocks = if footer { &flow.footer } else { &flow.header };
    let mut commands = Vec::new();
    let paragraph = match blocks.as_slice() {
        [] => {
            // The new paragraph's id is the next free one (`new_id` is the largest in the document and one more).
            let new = flow.new_id();
            commands.push(FlowCommand::AddHeader { footer });
            new
        }
        [Block::Paragraph(p)] => {
            let length = p.runs.iter().map(|r| r.len()).sum::<usize>();
            if length > 0 {
                commands.push(FlowCommand::Delete { from: FlowPos::new(p.id, 0), to: FlowPos::new(p.id, length) });
            }
            p.id
        }
        _ => return Err(ToolError::new("unsupported_operation", "the header or footer has more than one paragraph or something else in it")),
    };
    if !text.is_empty() {
        commands.push(FlowCommand::InsertText { at: FlowPos::new(paragraph, 0), text: text.to_string(), format: None });
    }
    if commands.is_empty() {
        return Ok(edited(id, open, json!({ "changed": false }), Vec::new()));
    }
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({ "changed": true }), Vec::new()))
}

fn add_comment(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "range", "text", "author", "date"])?;
    let id = args.str_arg("document_id")?;
    let text = args.str_arg("text")?;
    if text.trim().is_empty() || text.contains(['\n', '\r']) {
        return Err(ToolError::invalid("text is one line of the comment"));
    }
    let author = args.opt_str("author")?.filter(|a| !a.trim().is_empty()).unwrap_or("Assistant").to_string();
    let initials: String = author.split_whitespace().filter_map(|w| w.chars().next()).take(3).collect::<String>().to_uppercase();
    let date = match args.opt_str("date")? {
        Some(d) => {
            let ok = d.len() == 20 && d.ends_with('Z') && d.as_bytes()[10] == b'T' && d.bytes().enumerate().all(|(i, b)| if matches!(i, 4 | 7) { b == b'-' } else if matches!(i, 13 | 16) { b == b':' } else if matches!(i, 10 | 19) { true } else { b.is_ascii_digit() });
            if !ok {
                return Err(ToolError::invalid("date is UTC as 2026-11-24T09:00:00Z"));
            }
            d.to_string()
        }
        None => crate::clock::now_utc(),
    };
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let (from, to) = range_of(flow, args)?;
    if from == to {
        return Err(ToolError::invalid("the range is empty"));
    }
    let comment_id = flow.comments.iter().map(|c| c.id + 1).max().unwrap_or(0);
    let commands = flow_command::add_comment(flow, from, to, &CommentAuthor { author, initials, date }, text).ok_or_else(|| ToolError::invalid("the range is empty"))?;
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({ "comment": comment_id }), Vec::new()))
}

fn delete_comment(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "id"])?;
    let id = args.str_arg("document_id")?;
    let comment = args.usize_arg("id")? as u32;
    let open = service.open_document(id)?;
    let command = flow_command::delete_comment(flow_of(open)?, comment).ok_or_else(|| ToolError::new("out_of_range", format!("no comment with id {comment}: see docx_list_comments")))?;
    execute(open, vec![Command::Flow(command)])?;
    Ok(edited(id, open, json!({}), Vec::new()))
}

fn apply_revisions(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "accept", "range"])?;
    let id = args.str_arg("document_id")?;
    let accept = crate::edit::parse_bool("accept", args.get("accept").ok_or_else(|| ToolError::invalid("accept is required"))?)?;
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    if flow.lock_tracking {
        return Err(ToolError::new("unsupported_operation", "the document's change tracking is locked"));
    }
    let range = if args.get("range").is_some_and(|r| !r.is_null()) { Some(range_of(flow, args)?) } else { None };
    let commands = flow_command::resolve_revisions(flow, range, accept);
    if commands.is_empty() {
        return Ok(edited(id, open, json!({ "changed": false }), Vec::new()));
    }
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({ "changed": true }), Vec::new()))
}
