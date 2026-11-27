// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `docx_*` reading tools of stage B step 1 (`tasks/doc/design/2026-11-24-159-mcp-stage-b.md` 4 and 8): the page setup, the comments, the tracked
//! changes, the shapes and the content controls, and what `docx_get_paragraph` says more (the paragraph's format, its list, links, fields and bookmarks).
//! Every attribute a later setter changes can be read here first, so the assistant never overwrites a value it has not seen.

use ibis_doc_model::flow::{Block, CellDirection, DocGridType, FlowAlign, FlowDocument, FlowParagraph, FlowRunKind, LineHeight, Link, PartKind, RevisionKind};
use ibis_doc_model::flow_command;
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, document_id, integer, object};
use crate::error::ToolError;
use crate::service::{Args, Service, page};
use crate::tools_docx::{body_paragraphs, flow_of, paragraph_text};

const EMU_PER_POINT: f64 = 12_700.0;

fn pt(emu: i64) -> f64 {
    (emu as f64 / EMU_PER_POINT * 100.0).round() / 100.0
}

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "docx_get_page_setup",
            description: "The page setup of each section: paper size and orientation (points), margins, header and footer distances, gutter, columns, text direction, the character and line grid, and the text of its headers and footers; and the document's watermark and page color.",
            access: Access::Read,
            returns_content: true,
            schema: &|| object(&[document_id()], &["document_id"]),
            run: get_page_setup,
        },
        Tool {
            name: "docx_list_comments",
            description: "The comments of the document: id, author, date, text, the comment it answers, whether it is resolved, and the paragraph it is in.",
            access: Access::Read,
            returns_content: true,
            schema: &|| object(&[document_id(), ("offset", integer("The first comment.")), ("limit", integer("The most comments."))], &["document_id"]),
            run: list_comments,
        },
        Tool {
            name: "docx_list_revisions",
            description: "The tracked changes of the body (insertions, deletions, format changes): where they are (paragraph and character offset), who made them, when, and the text. Also whether changes are being tracked.",
            access: Access::Read,
            returns_content: true,
            schema: &|| object(&[document_id(), ("offset", integer("The first change.")), ("limit", integer("The most changes."))], &["document_id"]),
            run: list_revisions,
        },
        Tool {
            name: "docx_list_shapes",
            description: "The shapes and text boxes in the body paragraphs: where they are (paragraph and character offset), their kind, frame in points, rotation, whether they float, and their text.",
            access: Access::Read,
            returns_content: true,
            schema: &|| object(&[document_id()], &["document_id"]),
            run: list_shapes,
        },
        Tool {
            name: "docx_list_content_controls",
            description: "The content controls of the body (text, rich text, checkbox, date, drop-down list, picture, ...): where they are, kind, title, tag, value and the items of a list.",
            access: Access::Read,
            returns_content: true,
            schema: &|| object(&[document_id()], &["document_id"]),
            run: list_content_controls,
        },
    ]
}

fn alignment_name(a: FlowAlign) -> &'static str {
    match a {
        FlowAlign::Left => "left",
        FlowAlign::Center => "center",
        FlowAlign::Right => "right",
        FlowAlign::Justify => "justify",
        FlowAlign::Distribute => "distribute",
    }
}

fn direction_name(d: CellDirection) -> &'static str {
    match d {
        CellDirection::LrTb => "horizontal",
        CellDirection::TbRl | CellDirection::TbRlV => "vertical",
        _ => "vertical_left_to_right",
    }
}

fn blocks_text(blocks: &[Block]) -> String {
    blocks.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(paragraph_text(p)) } else { None }).collect::<Vec<_>>().join("\n")
}

fn grid_name(g: DocGridType) -> &'static str {
    match g {
        DocGridType::Default => "none",
        DocGridType::Lines => "lines",
        DocGridType::LinesAndChars => "lines_and_chars",
        DocGridType::SnapToChars => "chars",
    }
}

fn get_page_setup(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let mut sections = Vec::new();
    for (index, id) in flow_command::sections(flow).into_iter().enumerate() {
        let Some(setup) = flow_command::section_setup(flow, id) else { continue };
        let p = &setup.page;
        let (w, h) = (pt(p.size.width), pt(p.size.height));
        let part = |kind: PartKind| blocks_text(&flow_command::resolved_part(flow, id, kind));
        sections.push(json!({
            "section": index,
            "paper": { "width": w, "height": h },
            "orientation": if w > h { "landscape" } else { "portrait" },
            "margins": { "top": pt(p.top), "bottom": pt(p.bottom), "left": pt(p.left), "right": pt(p.right) },
            "header_distance": pt(p.header),
            "footer_distance": pt(p.footer),
            "gutter": { "size": pt(p.gutter), "at_top": p.gutter_at_top },
            "mirror_margins": p.mirror_margins,
            "columns": { "count": p.columns.count, "space": pt(p.columns.space), "separator": p.columns.separator },
            "text_direction": direction_name(p.direction),
            "grid": { "type": grid_name(p.doc_grid_type), "line_pitch": p.line_pitch.map(pt), "char_space": p.char_space.map(pt) },
            "first_page_number": p.first_page,
            "restart_numbering": setup.restart_numbering,
            "page_number_format": setup.page_number_format,
            "different_first_page": setup.title_page,
            "header": part(PartKind::Header),
            "footer": part(PartKind::Footer),
            "header_first": part(PartKind::HeaderFirst),
            "footer_first": part(PartKind::FooterFirst),
            "header_even": part(PartKind::HeaderEven),
            "footer_even": part(PartKind::FooterEven),
        }));
    }
    Ok(json!({
        "sections": sections,
        "different_odd_even": flow.even_and_odd,
        "watermark": flow.watermark.as_ref().map(|w| json!({ "text": w.text, "font": w.font, "color": format!("#{:06X}", w.color.0 & 0xFF_FFFF), "semitransparent": w.semitransparent, "rotation": w.rotation })),
        "page_color": flow.page_color.map(|c| format!("#{:06X}", c.0 & 0xFF_FFFF)),
    }))
}

/// For each body paragraph, the characters from its start to each run (a non-text run counts for its length).
fn runs_with_offsets(p: &FlowParagraph) -> impl Iterator<Item = (usize, &ibis_doc_model::flow::FlowRun)> {
    p.runs.iter().scan(0usize, |at, r| {
        let here = *at;
        *at += r.len();
        Some((here, r))
    })
}

fn list_comments(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    // The paragraph a comment starts in.
    let mut at_paragraph = std::collections::HashMap::new();
    for (n, p) in body_paragraphs(flow).iter().enumerate() {
        for r in &p.runs {
            if let FlowRunKind::CommentStart(id) | FlowRunKind::CommentRef(id) = r.kind {
                at_paragraph.entry(id).or_insert(n);
            }
        }
    }
    let items: Vec<Value> = flow
        .comments
        .iter()
        .map(|c| json!({ "id": c.id, "author": c.author, "initials": c.initials, "date": c.date, "text": blocks_text(&c.blocks), "reply_to": c.parent, "resolved": c.done, "paragraph": at_paragraph.get(&c.id) }))
        .collect();
    let (shown, next) = page(args, &items, 200)?;
    let mut result = json!({ "comments": shown, "total": items.len() });
    if let Some(next) = next {
        result["next_offset"] = json!(next);
    }
    Ok(result)
}

fn list_revisions(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let mut items = Vec::new();
    for (n, p) in body_paragraphs(flow).iter().enumerate() {
        for (offset, r) in runs_with_offsets(p) {
            let Some(rev) = &r.revision else { continue };
            let kind = match rev.kind {
                RevisionKind::Insert => "insert",
                RevisionKind::Delete => "delete",
                RevisionKind::Format(_) => "format",
                _ => "other",
            };
            let text = match &r.kind {
                FlowRunKind::Text(t) => t.clone(),
                _ => String::new(),
            };
            items.push(json!({ "paragraph": n, "offset": offset, "length": r.len(), "kind": kind, "author": rev.author, "date": rev.date, "text": text }));
        }
    }
    let (shown, next) = page(args, &items, 200)?;
    let mut result = json!({ "revisions": shown, "total": items.len(), "tracking": flow.track_changes });
    if let Some(next) = next {
        result["next_offset"] = json!(next);
    }
    Ok(result)
}

fn kind_label(kind: &ibis_doc_model::ShapeKind) -> &'static str {
    use ibis_doc_model::ShapeKind::*;
    match kind {
        TextBox(_) => "text_box",
        Auto(_) => "shape",
        Picture(_) => "picture",
        Line(_) => "line",
        Chart(_) => "chart",
        Table(_) => "table",
        _ => "other",
    }
}

fn list_shapes(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let mut items = Vec::new();
    for (n, p) in body_paragraphs(flow).iter().enumerate() {
        for (offset, r) in runs_with_offsets(p) {
            if let FlowRunKind::Shape(s) = &r.kind {
                let f = &s.frame;
                items.push(json!({
                    "paragraph": n,
                    "offset": offset,
                    "kind": kind_label(&s.kind),
                    "frame": { "x": pt(f.x), "y": pt(f.y), "width": pt(f.width), "height": pt(f.height) },
                    "rotation": s.rotation as f64 / 60_000.0,
                    "floating": s.floating.is_some(),
                    "wrap": s.floating.as_ref().map(wrap_name),
                    "text": s.text_body().map(|b| b.text()),
                    "fill": shape_fill(&s.kind),
                    "line": shape_line(&s.kind),
                }));
            }
        }
    }
    Ok(json!({ "shapes": items }))
}

fn list_content_controls(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let mut items = Vec::new();
    for (n, p) in body_paragraphs(flow).iter().enumerate() {
        for (offset, r) in runs_with_offsets(p) {
            use FlowRunKind::*;
            let (kind, value, list): (&str, Value, Value) = match &r.kind {
                PlainText { text, .. } => ("text", json!(text), Value::Null),
                RichText { text, .. } => ("rich_text", json!(text), Value::Null),
                CheckBox { checked, .. } => ("checkbox", json!(checked), Value::Null),
                DatePicker { full_date, text, .. } => ("date", json!({ "date": full_date, "shown": text }), Value::Null),
                DropDownList { items, selected, combo } => (if *combo { "combo_box" } else { "drop_down_list" }, json!(items.get(*selected).map(|i| i.0.clone())), json!(items.iter().map(|i| json!({ "text": i.0, "value": i.1 })).collect::<Vec<_>>())),
                PictureControl { .. } => ("picture", Value::Null, Value::Null),
                Group { text, .. } => ("group", json!(text), Value::Null),
                RepeatingSection { section_title, items, .. } => ("repeating_section", json!(section_title), json!(items)),
                BuildingBlockGallery { text, .. } => ("building_block", json!(text), Value::Null),
                _ => continue,
            };
            let info = r.control.as_deref();
            items.push(json!({ "paragraph": n, "offset": offset, "kind": kind, "title": info.map(|c| c.title.clone()), "tag": info.map(|c| c.tag.clone()), "value": value, "items": list }));
        }
    }
    Ok(json!({ "controls": items }))
}

/// What `docx_get_paragraph` says besides the runs: the paragraph's own format (as it shows, with its style's), its list, links, fields and bookmarks.
pub(crate) fn paragraph_details(flow: &FlowDocument, p: &FlowParagraph) -> Value {
    let line = match p.line {
        LineHeight::Auto(n) => json!({ "rule": "multiple", "value": n as f64 / 240.0 }),
        LineHeight::AtLeast(e) => json!({ "rule": "at_least", "value": pt(e) }),
        LineHeight::Exact(e) => json!({ "rule": "exactly", "value": pt(e) }),
    };
    let mut links = Vec::new();
    let mut fields = Vec::new();
    let mut comments = Vec::new();
    for (offset, r) in runs_with_offsets(p) {
        match &r.link {
            Some(Link::Url(u)) => links.push(json!({ "offset": offset, "length": r.len(), "url": u })),
            Some(Link::Anchor(a)) => links.push(json!({ "offset": offset, "length": r.len(), "bookmark": a })),
            None => {}
        }
        match &r.kind {
            FlowRunKind::PageNumber => fields.push(json!({ "offset": offset, "field": "page" })),
            FlowRunKind::FileName(shown) => fields.push(json!({ "offset": offset, "field": "file_name", "shown": shown })),
            FlowRunKind::PageRef { bookmark, shown } => fields.push(json!({ "offset": offset, "field": "page_ref", "bookmark": bookmark, "shown": shown })),
            FlowRunKind::Field { instruction, shown } => fields.push(json!({ "offset": offset, "field": instruction, "shown": shown })),
            FlowRunKind::CommentStart(id) => comments.push(*id),
            _ => {}
        }
    }
    let _ = flow;
    json!({
        "format": {
            "alignment": alignment_name(p.align),
            "indent_left": pt(p.left),
            "indent_right": pt(p.right),
            "indent_first_line": pt(p.first_line),
            "space_before": pt(p.space_before),
            "space_after": pt(p.space_after),
            "line_spacing": line,
            "keep_with_next": p.keep_with_next,
            "keep_together": p.keep_lines,
            "page_break_before": p.page_break_before,
            "widow_control": p.widow_control,
        },
        "list": p.list.as_ref().map(|l| json!({ "num_id": l.num_id, "level": l.level })),
        "links": links,
        "fields": fields,
        "bookmarks": p.bookmarks,
        "comments": comments,
    })
}

/// A paragraph style as `docx_list_styles` shows it: its id, name and base, and the format it sets itself (what it does not set it takes from its base).
pub(crate) fn style_json(id: &str, s: &ibis_doc_model::flow::FlowStyle) -> Value {
    let mut para = serde_json::Map::new();
    let f = &s.para;
    if let Some(a) = f.align {
        para.insert("alignment".into(), json!(alignment_name(a)));
    }
    for (name, v) in [("indent_left", f.left), ("indent_right", f.right), ("indent_first_line", f.first_line), ("space_before", f.before), ("space_after", f.after)] {
        if let Some(v) = v {
            para.insert(name.into(), json!(pt(v)));
        }
    }
    for (name, v) in [("keep_with_next", f.keep_next), ("keep_together", f.keep_lines), ("page_break_before", f.page_break_before), ("widow_control", f.widow_control)] {
        if let Some(v) = v {
            para.insert(name.into(), json!(v));
        }
    }
    let mut run = serde_json::Map::new();
    let r = &s.run;
    if let Some(v) = &r.font {
        run.insert("font".into(), json!(v));
    }
    if let Some(v) = &r.east_asian_font {
        run.insert("east_asian_font".into(), json!(v));
    }
    if let Some(v) = r.size {
        run.insert("size".into(), json!(v as f64 / 100.0));
    }
    for (name, v) in [("bold", r.bold), ("italic", r.italic), ("underline", r.underline), ("strike", r.strike)] {
        if let Some(v) = v {
            run.insert(name.into(), json!(v));
        }
    }
    if let Some(c) = r.color {
        run.insert("color".into(), json!(format!("#{:06X}", c.0 & 0xFF_FFFF)));
    }
    json!({ "id": id, "name": s.name, "based_on": s.based_on, "next": s.next, "paragraph_format": para, "run_format": run })
}

/// What `docx_get_table` says besides the text: the table's alignment, the grid columns' widths (points), the options of its style, and each cell's span, vertical merge and fill.
pub(crate) fn table_details(t: &ibis_doc_model::flow::FlowTable) -> Value {
    use ibis_doc_model::flow::VerticalMerge;
    let cells: Vec<Vec<Value>> = t
        .rows
        .iter()
        .map(|r| {
            r.cells
                .iter()
                .map(|c| {
                    let merge = match c.merge {
                        VerticalMerge::None => "none",
                        VerticalMerge::Start => "start",
                        VerticalMerge::Continue => "continue",
                    };
                    json!({ "span": c.span, "vertical_merge": merge, "fill": c.shading.map(|f| format!("#{:06X}", f.0 & 0xFF_FFFF)) })
                })
                .collect()
        })
        .collect();
    json!({
        "alignment": alignment_name(t.align),
        "column_widths": t.columns.iter().map(|w| pt(*w)).collect::<Vec<_>>(),
        "style_options": t.style.as_ref().map(|s| json!({ "first_row": s.look.first_row, "last_row": s.look.last_row, "first_column": s.look.first_column, "last_column": s.look.last_column, "banded_rows": !s.look.no_hband, "banded_columns": !s.look.no_vband })),
        "cells": cells,
    })
}

/// A shape's solid fill as "#RRGGBB" (`None`: no fill, or a gradient or picture fill).
fn shape_fill(kind: &ibis_doc_model::ShapeKind) -> Option<String> {
    match kind {
        ibis_doc_model::ShapeKind::Auto(a) => match &a.fill {
            ibis_doc_model::drawing::Fill::Solid(c) => Some(format!("#{:06X}", c.rgb.0 & 0xFF_FFFF)),
            _ => None,
        },
        _ => None,
    }
}

/// A shape's line: its color and width in points.
fn shape_line(kind: &ibis_doc_model::ShapeKind) -> Value {
    let stroke = match kind {
        ibis_doc_model::ShapeKind::Auto(a) => a.stroke.as_ref(),
        ibis_doc_model::ShapeKind::Line(s) => Some(s),
        _ => None,
    };
    stroke.map_or(Value::Null, |s| json!({ "color": format!("#{:06X}", s.color.rgb.0 & 0xFF_FFFF), "width": pt(s.width) }))
}

/// How text wraps around a floating shape, by the names `docx_set_picture_wrap` takes.
fn wrap_name(f: &ibis_doc_model::FloatingAnchor) -> &'static str {
    match (f.wrap.as_str(), f.behind_doc) {
        (_, true) => "behind_text",
        ("none", false) => "in_front_of_text",
        ("square", _) => "square",
        ("tight", _) => "tight",
        ("through", _) => "through",
        ("topAndBottom", _) => "top_and_bottom",
        _ => "other",
    }
}
