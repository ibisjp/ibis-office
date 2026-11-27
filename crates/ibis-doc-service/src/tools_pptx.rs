// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `pptx_*` tools (design 6.4). A slide is numbered from 1 (or named by its `slide_id`); a shape by its `shape_id`. Lengths are points.

use ibis_doc_model::{Presentation, Shape, ShapeKind, Slide, Stroke};
use ibis_doc_model::text::TextBody;
use ibis_formats::Kind;
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, document_id, integer, object};
use crate::error::ToolError;
use crate::service::{Args, Open, Service};

const EMU_PER_POINT: f64 = 12700.0;

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "pptx_list_slides", description: "The slides: number, id, title (the first text), shapes, hidden and whether it has notes.", access: Access::Read, returns_content: true, schema: &|| object(&[document_id()], &["document_id"]), run: list_slides },
        Tool {
            name: "pptx_get_slide",
            description: "The shapes of a slide: id, kind, position and size (points) and text.",
            access: Access::Read,
            returns_content: true,
            schema: &|| object(&[document_id(), ("slide", integer("The slide's number, from 1."))], &["document_id", "slide"]),
            run: get_slide,
        },
        Tool {
            name: "pptx_get_shape_text",
            description: "The paragraphs of a shape's text with the formatting of each run.",
            access: Access::Read,
            returns_content: true,
            schema: &|| object(&[document_id(), ("slide", integer("The slide's number, from 1.")), ("shape", integer("The shape's shape_id."))], &["document_id", "slide", "shape"]),
            run: get_shape_text,
        },
        Tool { name: "pptx_get_theme", description: "The theme's twelve colours (dk1, lt1, dk2, lt2, accent1..accent6, hlink, folHlink as #RRGGBB) and its heading and body typefaces.", access: Access::Read, returns_content: true, schema: &|| object(&[document_id()], &["document_id"]), run: get_theme },
        Tool { name: "pptx_list_comments", description: "The comments of a slide: author, text, time, position (points) and whether it is resolved.", access: Access::Read, returns_content: true, schema: &|| object(&[document_id(), ("slide", integer("The slide's number, from 1."))], &["document_id", "slide"]), run: list_comments },
        Tool { name: "pptx_get_transition", description: "The slide transition of a slide: effect, direction, duration (seconds), advance on click and advance after (seconds); null when it has none.", access: Access::Read, returns_content: true, schema: &|| object(&[document_id(), ("slide", integer("The slide's number, from 1."))], &["document_id", "slide"]), run: get_transition },
        Tool { name: "pptx_list_animations", description: "The animations of a slide in order: shape, class (entrance, emphasis, exit, path, media), look, start (on_click, with_previous, after_previous), delay and duration (seconds), and PowerPoint's preset id and subtype.", access: Access::Read, returns_content: true, schema: &|| object(&[document_id(), ("slide", integer("The slide's number, from 1."))], &["document_id", "slide"]), run: list_animations },
        Tool { name: "pptx_get_table", description: "The cells of a table shape: its column widths and row heights (points) and the text, fill and span of each cell.", access: Access::Read, returns_content: true, schema: &|| object(&[document_id(), ("slide", integer("The slide's number, from 1.")), ("shape", integer("The table's shape_id."))], &["document_id", "slide", "shape"]), run: get_table },
        Tool { name: "pptx_get_notes", description: "The speaker notes of a slide.", access: Access::Read, returns_content: true, schema: &|| object(&[document_id(), ("slide", integer("The slide's number, from 1."))], &["document_id", "slide"]), run: get_notes },
    ]
}

pub(crate) fn presentation_of(open: &Open) -> Result<&Presentation, ToolError> {
    if open.kind != Kind::Presentation {
        return Err(ToolError::new("unsupported_operation", "this tool is for a pptx presentation"));
    }
    Ok(open.document.presentation())
}

pub(crate) fn slide_at<'a>(presentation: &'a Presentation, number: usize) -> Result<&'a Slide, ToolError> {
    number.checked_sub(1).and_then(|i| presentation.slides.get(i)).ok_or_else(|| ToolError::new("out_of_range", format!("slide {number}: the presentation has {} slides", presentation.slides.len())))
}

pub(crate) fn body_text(body: &TextBody) -> String {
    body.paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

fn kind_label(shape: &Shape) -> &'static str {
    match &shape.kind {
        ShapeKind::TextBox(_) => "text_box",
        ShapeKind::Auto(_) => "shape",
        ShapeKind::Picture(_) => "picture",
        ShapeKind::Line(_) => "line",
        ShapeKind::Chart(_) => "chart",
        ShapeKind::Table(_) => "table",
        ShapeKind::Placeholder(_) => "placeholder",
        _ => "other",
    }
}

fn shape_text(shape: &Shape) -> Option<String> {
    shape.text_body().map(body_text).filter(|t| !t.is_empty())
}

fn points(emu: i64) -> f64 {
    (emu as f64 / EMU_PER_POINT * 100.0).round() / 100.0
}

fn list_slides(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let presentation = presentation_of(open)?;
    let slides: Vec<Value> = presentation
        .slides
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let title = s.shapes.iter().find_map(shape_text).map(|t| t.lines().next().unwrap_or("").to_string());
            let layout = s.layout.and_then(|l| presentation.layouts.iter().position(|x| x.id == l));
            json!({ "slide": i + 1, "slide_id": s.id.0, "title": title, "shapes": s.shapes.len(), "hidden": s.hidden, "has_notes": !s.notes.is_empty(), "layout": layout.map(|n| n + 1), "layout_name": layout.map(|n| presentation.layouts[n].name.clone()) })
        })
        .collect();
    Ok(json!({ "slides": slides }))
}

fn get_slide(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let number = args.usize_arg("slide")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let slide = slide_at(presentation_of(open)?, number)?;
    let shapes: Vec<Value> = slide
        .shapes
        .iter()
        .map(|s| json!({ "shape_id": s.id.0, "kind": kind_label(s), "x": points(s.frame.x), "y": points(s.frame.y), "width": points(s.frame.width), "height": points(s.frame.height), "text": shape_text(s), "fill": fill_json(s), "line": line_of(s), "name": slide.shape_names.get(&s.id), "hidden": slide.hidden_shapes.contains(&s.id), "rotation": f64::from(s.rotation) / 60_000.0 }))
        .collect();
    fn group_json(g: &ibis_doc_model::ShapeGroup) -> Value {
        let children: Vec<Value> = g.children.iter().map(|c| match c {
            ibis_doc_model::GroupMember::Shape(s) => json!({ "shape_id": s.0 }),
            ibis_doc_model::GroupMember::Group(inner) => json!({ "group": inner.native_id }),
        }).collect();
        json!({ "group": g.native_id, "name": g.name, "children": children })
    }
    fn walk(g: &ibis_doc_model::ShapeGroup, out: &mut Vec<Value>) {
        out.push(group_json(g));
        for c in &g.children {
            if let ibis_doc_model::GroupMember::Group(inner) = c {
                walk(inner, out);
            }
        }
    }
    let mut groups = Vec::new();
    for g in &slide.groups {
        walk(g, &mut groups);
    }
    Ok(json!({ "slide": number, "slide_id": slide.id.0, "hidden": slide.hidden, "shapes": shapes, "groups": groups }))
}

/// The solid fill of a shape, as a colour (see [`colour_json`]); `null` for no fill or another kind of fill.
fn line_of(shape: &Shape) -> Value {
    match &shape.kind {
        ShapeKind::Auto(a) => crate::tools_pptx_b::line_json(a.stroke.as_ref()),
        ShapeKind::Line(l) => crate::tools_pptx_b::line_json(Some(l)),
        _ => Value::Null,
    }
}

fn fill_json(shape: &Shape) -> Value {
    match &shape.kind {
        ibis_doc_model::model::ShapeKind::Auto(a) => match &a.fill {
            Fill::Solid(c) => colour_json(c.rgb, c.theme_ref()),
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

fn get_shape_text(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let (number, id) = (args.usize_arg("slide")?, args.usize_arg("shape")? as u64);
    let open = service.open_document(args.str_arg("document_id")?)?;
    let slide = slide_at(presentation_of(open)?, number)?;
    let shape = slide.shapes.iter().find(|s| s.id.0 == id).ok_or_else(|| ToolError::new("no_such_shape", format!("no shape {id} on slide {number}")))?;
    let body = shape.text_body().ok_or_else(|| ToolError::new("unsupported_operation", format!("shape {id} has no text")))?;
    let paragraphs: Vec<Value> = body
        .paragraphs
        .iter()
        .map(|p| {
            let runs: Vec<Value> = p.runs.iter().map(|r| json!({ "text": r.text, "bold": r.style.bold, "italic": r.style.italic, "underline": r.style.underline, "size": r.style.font_size.map(|v| f64::from(v) / 100.0), "color": r.style.color.map(|c| colour_json(c, r.style.color_theme.as_ref())), "font": r.style.font })).collect();
            json!({ "text": p.runs.iter().map(|r| r.text.as_str()).collect::<String>(), "runs": runs, "alignment": p.align.map(|a| format!("{a:?}").to_lowercase()), "level": p.level, "indent_left": points(p.margin_left), "indent_first_line": points(p.indent), "space_before": f64::from(p.space_before) / 100.0, "space_after": f64::from(p.space_after) / 100.0, "bullet": p.bullet.is_some() })
        })
        .collect();
    let frame = json!({ "vertical_alignment": format!("{:?}", body.anchor).to_lowercase(), "margins": { "left": points(body.insets.left), "top": points(body.insets.top), "right": points(body.insets.right), "bottom": points(body.insets.bottom) }, "wrap": body.wrap, "autofit": match body.autofit { ibis_doc_model::text::TextAutofit::None => "none", ibis_doc_model::text::TextAutofit::Shrink => "shrink", ibis_doc_model::text::TextAutofit::ResizeShape => "resize_shape" }, "columns": body.columns });
    Ok(json!({ "slide": number, "shape_id": id, "paragraphs": paragraphs, "text_frame": frame }))
}

fn get_theme(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let presentation = presentation_of(open)?;
    let colors: serde_json::Map<String, Value> = presentation.theme_colors.iter().map(|(n, c)| (n.clone(), json!(format!("#{:06X}", c.0 & 0xFF_FFFF)))).collect();
    let fonts = presentation.theme_fonts.as_ref().map(|f| {
        let set = |s: &ibis_doc_model::text::FontSet| json!({ "latin": s.latin, "east_asian": s.east_asian, "complex_script": s.complex });
        json!({ "major": set(&f.major), "minor": set(&f.minor) })
    });
    Ok(json!({ "colors": colors, "fonts": fonts }))
}

fn list_comments(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let number = args.usize_arg("slide")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let slide = slide_at(presentation_of(open)?, number)?;
    let comments: Vec<Value> = slide
        .comments
        .iter()
        .map(|c| json!({ "author": c.author.name, "text": c.text, "created": c.created, "x": c.position.map(|p| points(p.x)), "y": c.position.map(|p| points(p.y)), "shape_id": c.shape.map(|s| s.0), "resolved": c.resolved }))
        .collect();
    Ok(json!({ "slide": number, "comments": comments }))
}

fn get_transition(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let number = args.usize_arg("slide")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let slide = slide_at(presentation_of(open)?, number)?;
    let transition = slide.transition.as_ref().map(|t| json!({ "effect": t.effect, "direction": t.direction, "duration": f64::from(t.duration) / 1000.0, "advance_on_click": t.on_click, "advance_after": t.after.map(|a| f64::from(a) / 1000.0) }));
    Ok(json!({ "slide": number, "transition": transition }))
}

fn list_animations(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::{AnimationClass, AnimationStart};
    let number = args.usize_arg("slide")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let slide = slide_at(presentation_of(open)?, number)?;
    let animations: Vec<Value> = slide
        .animations
        .iter()
        .map(|a| {
            let class = match a.class {
                AnimationClass::Entrance => "entrance",
                AnimationClass::Emphasis => "emphasis",
                AnimationClass::Exit => "exit",
                AnimationClass::Path => "path",
                AnimationClass::Media => "media",
            };
            let start = match a.start {
                AnimationStart::OnClick => "on_click",
                AnimationStart::WithPrevious => "with_previous",
                AnimationStart::AfterPrevious => "after_previous",
            };
            json!({ "shape_id": a.shape.0, "class": class, "look": format!("{:?}", a.look).split('(').next().unwrap_or("").to_lowercase(), "start": start, "delay": f64::from(a.delay) / 1000.0, "duration": f64::from(a.duration) / 1000.0, "preset_id": a.preset.0, "preset_subtype": a.preset.1 })
        })
        .collect();
    Ok(json!({ "slide": number, "animations": animations }))
}

fn get_table(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let (number, id) = (args.usize_arg("slide")?, args.usize_arg("shape")? as u64);
    let open = service.open_document(args.str_arg("document_id")?)?;
    let slide = slide_at(presentation_of(open)?, number)?;
    let shape = slide.shapes.iter().find(|s| s.id.0 == id).ok_or_else(|| ToolError::new("no_such_shape", format!("no shape {id} on slide {number}")))?;
    let ShapeKind::Table(table) = &shape.kind else { return Err(ToolError::new("unsupported_operation", format!("shape {id} is not a table"))) };
    let rows: Vec<Value> = table
        .rows
        .iter()
        .map(|row| {
            let cells: Vec<Value> = row
                .cells
                .iter()
                .map(|c| {
                    let fill = match &c.fill {
                        ibis_doc_model::Fill::Solid(color) => colour_json(color.rgb, color.theme_ref()),
                        _ => Value::Null,
                    };
                    json!({ "text": body_text(&c.text), "fill": fill, "column_span": c.column_span, "row_span": c.row_span, "merged": c.merged })
                })
                .collect();
            json!({ "height": points(row.height), "cells": cells })
        })
        .collect();
    Ok(json!({ "slide": number, "shape_id": id, "columns": table.columns.iter().map(|w| points(*w)).collect::<Vec<_>>(), "rows": rows }))
}

fn get_notes(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let number = args.usize_arg("slide")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let slide = slide_at(presentation_of(open)?, number)?;
    Ok(json!({ "slide": number, "notes": slide.notes }))
}

// ---- edits ----

use ibis_doc_model::text::CharacterFormat;
use ibis_doc_model::{Color, Command, EmuRect, Fill, ShapeId, SlideId};

use crate::catalog::{boolean, string};
use crate::edit::{ColorArg, FontArg, edited, execute, only_known, parse_bool, parse_color, parse_font, parse_size};
use crate::tools_docx::{color_schema, font_schema};

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "pptx_set_shape_text",
            description: "Replace the text of a shape (lines are paragraphs).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), slide_arg(), ("shape", integer("The shape's shape_id.")), ("text", string("The new text."))], &["document_id", "slide", "shape", "text"]),
            run: set_shape_text,
        },
        Tool {
            name: "pptx_replace_text",
            description: "Replace a text with another in the shapes of one slide or of every slide, as one undo step.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("find", string("The text to find.")), ("replace", string("The new text.")), ("slide", integer("Only this slide (from 1).")), ("match_case", boolean("Match upper and lower case (default false)."))], &["document_id", "find", "replace"]),
            run: replace_text,
        },
        Tool {
            name: "pptx_set_run_format",
            description: "Set the character format of a shape's text (all of it, or one paragraph's characters from..to): bold, italic, underline, size (pt), color (\"#RRGGBB\", or a theme colour { theme, brightness? } which the text keeps as a reference, so it follows a later change of the theme), font (a typeface name). What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("shape", integer("The shape's shape_id.")),
                        ("range", json!({ "type": "object", "description": "{ paragraph, from, to }: characters of one paragraph (the end is not included). Omit for all the text.", "properties": { "paragraph": integer("Paragraph (0-based)."), "from": integer("First character."), "to": integer("After the last character.") }, "required": ["paragraph", "from", "to"] })),
                        ("bold", boolean("Bold.")),
                        ("italic", boolean("Italic.")),
                        ("underline", boolean("Underline.")),
                        ("size", json!({ "type": "number", "description": "Points, 1 to 409.5 in steps of 0.5." })),
                        ("color", color_schema()),
                        ("font", font_schema()),
                    ],
                    &["document_id", "slide", "shape"],
                )
            },
            run: set_run_format,
        },
        Tool { name: "pptx_set_notes", description: "Set the speaker notes of a slide.", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), slide_arg(), ("text", string("The notes."))], &["document_id", "slide", "text"]), run: set_notes },
        Tool {
            name: "pptx_add_slide",
            description: "Add a slide from a layout (a layout's name or number from 1, see pptx_list_layouts; default the first) at index (0-based; default the end).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("layout", string("The layout's name, or its number from 1.")), ("index", integer("Where it goes (0-based)."))], &["document_id"]),
            run: add_slide,
        },
        Tool { name: "pptx_delete_slide", description: "Delete a slide (not the last one).", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), slide_arg()], &["document_id", "slide"]), run: delete_slide },
        Tool { name: "pptx_move_slide", description: "Move a slide to index (0-based).", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), slide_arg(), ("index", integer("The new place (0-based)."))], &["document_id", "slide", "index"]), run: move_slide },
        Tool {
            name: "pptx_set_frame",
            description: "Move and resize shapes: frames is a list of { shape, x, y, width, height } in points; the shapes attached to them follow.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("frames", json!({ "type": "array", "items": { "type": "object", "properties": { "shape": integer("shape_id."), "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" } }, "required": ["shape", "x", "y", "width", "height"] } })),
                    ],
                    &["document_id", "slide", "frames"],
                )
            },
            run: set_frame,
        },
        Tool { name: "pptx_delete_shape", description: "Delete shapes (a list of shape_id) with the shapes attached to them.", access: Access::Edit, returns_content: false, schema: &|| object(&[document_id(), slide_arg(), ("shapes", json!({ "type": "array", "items": { "type": "integer" } }))], &["document_id", "slide", "shapes"]), run: delete_shape },
        Tool {
            name: "pptx_insert_shape",
            description: "Add a shape to a slide with its frame in points and optional text: kind is text_box, a preset shape's name (ECMA-376 ST_ShapeType, such as rect, roundRect, ellipse, triangle, rightArrow, star5) or line (from the frame's top-left to its bottom-right corner). A shape gets the theme's accent 1 fill and outline. The result has its shape_id.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("kind", string("text_box, a preset shape's name, or line.")),
                        ("frame", json!({ "type": "object", "properties": { "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" } }, "required": ["x", "y", "width", "height"] })),
                        ("text", string("The text (paragraphs separated by line breaks).")),
                    ],
                    &["document_id", "slide", "kind", "frame"],
                )
            },
            run: insert_shape,
        },
        Tool {
            name: "pptx_insert_picture",
            description: "Add a PNG or JPEG picture (a file under an allowed folder) to a slide. frame gives x, y, width and height in points, each optional: a missing width or height keeps the picture's proportion (neither: its size in pixels, 96 per inch, made to fit the slide), a missing x or y centres it. The result has its shape_id.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("path", string("The picture file (.png, .jpg, .jpeg).")),
                        ("frame", json!({ "type": "object", "properties": { "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" } } })),
                    ],
                    &["document_id", "slide", "path"],
                )
            },
            run: insert_picture,
        },
        Tool {
            name: "pptx_set_fill",
            description: "Fill shapes with a colour (\"#RRGGBB\", or a theme colour { theme, brightness } kept as a reference), or null for no fill; or with a gradient { angle (degrees), colors: [colour, ...] } (two or more, evenly spaced). transparency (0 to 1) makes the fill see-through. A picture fill is not supported yet. A text box that is filled becomes a rectangle with its text.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), slide_arg(), ("shapes", json!({ "type": "array", "minItems": 1, "items": { "type": "integer" }, "description": "shape_id of each." })), ("color", color_schema()), ("gradient", json!({ "type": "object", "properties": { "angle": { "type": "number" }, "colors": { "type": "array", "items": color_schema() } }, "required": ["colors"] })), ("transparency", json!({ "type": "number", "description": "0 (opaque) to 1." }))], &["document_id", "slide", "shapes"]),
            run: set_fill,
        },
        Tool { name: "pptx_list_layouts", description: "The layouts a new slide can follow: number and name.", access: Access::Read, returns_content: false, schema: &|| object(&[document_id()], &["document_id"]), run: list_layouts },
    ]
}

pub(crate) fn slide_arg() -> (&'static str, Value) {
    ("slide", integer("The slide's number, from 1."))
}

pub(crate) fn slide_id(open: &Open, number: usize) -> Result<SlideId, ToolError> {
    Ok(slide_at(presentation_of(open)?, number)?.id)
}

pub(crate) fn pptx_edit(service: &mut Service, id: &str, commands: Vec<Command>, extra: Value) -> Result<Value, ToolError> {
    let open = service.open_document(id)?;
    execute(open, commands)?;
    Ok(edited(id, open, extra, Vec::new()))
}

pub(crate) fn shape_id(open: &Open, slide: usize, shape: u64) -> Result<(SlideId, ShapeId), ToolError> {
    let s = slide_at(presentation_of(open)?, slide)?;
    let found = s.shapes.iter().find(|x| x.id.0 == shape).ok_or_else(|| ToolError::new("no_such_shape", format!("no shape {shape} on slide {slide}")))?;
    Ok((s.id, found.id))
}

fn set_shape_text(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let (slide, shape) = {
        let open = service.open_document(id)?;
        shape_id(open, args.usize_arg("slide")?, args.usize_arg("shape")? as u64)?
    };
    pptx_edit(service, id, vec![Command::SetText { slide, shape, text: args.str_arg("text")?.to_string() }], json!({}))
}

fn replace_text(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let (find, replace) = (args.str_arg("find")?, args.str_arg("replace")?);
    if find.is_empty() {
        return Err(ToolError::invalid("find is empty"));
    }
    let match_case = args.opt_bool("match_case")?.unwrap_or(false);
    let only = args.opt_usize("slide")?;
    let open = service.open_document(id)?;
    let presentation = presentation_of(open)?;
    if let Some(n) = only {
        slide_at(presentation, n)?;
    }
    let fold = |s: &str| -> Vec<char> { if match_case { s.chars().collect() } else { s.to_lowercase().chars().collect() } };
    let needle = fold(find);
    let mut commands = Vec::new();
    let mut count = 0usize;
    for (i, slide) in presentation.slides.iter().enumerate() {
        if only.is_some_and(|n| n != i + 1) {
            continue;
        }
        for shape in &slide.shapes {
            let Some(body) = shape.text_body() else { continue };
            let text: Vec<char> = body_text(body).chars().collect();
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
                } else {
                    at += 1;
                }
            }
            for &start in found.iter().rev() {
                commands.push(Command::ReplaceTextRange { slide: slide.id, shape: shape.id, at: start, remove: needle.len(), insert: replace.to_string() });
                count += 1;
            }
        }
    }
    if commands.is_empty() {
        return Ok(edited(id, open, json!({ "replaced": 0 }), Vec::new()));
    }
    pptx_edit(service, id, commands, json!({ "replaced": count }))
}

fn set_run_format(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "shape", "range", "bold", "italic", "underline", "size", "color", "font"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let (number, shape_n) = (args.usize_arg("slide")?, args.usize_arg("shape")? as u64);
    let (slide, shape) = shape_id(open, number, shape_n)?;
    let body = slide_at(presentation_of(open)?, number)?.shapes.iter().find(|s| s.id == shape).and_then(|s| s.text_body()).ok_or_else(|| ToolError::new("unsupported_operation", format!("shape {shape_n} has no text")))?;
    // Characters count over the paragraphs joined by one line break each.
    let lengths: Vec<usize> = body.paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.chars().count()).sum()).collect();
    let (start, end) = match args.get("range") {
        None | Some(Value::Null) => (0, lengths.iter().sum::<usize>() + lengths.len().saturating_sub(1)),
        Some(range) => {
            let get = |k: &str| range.get(k).and_then(Value::as_u64).map(|n| n as usize).ok_or_else(|| ToolError::invalid(format!("range.{k} is a number")));
            let (p, from, to) = (get("paragraph")?, get("from")?, get("to")?);
            let length = *lengths.get(p).ok_or_else(|| ToolError::new("out_of_range", format!("paragraph {p}; the shape has {}", lengths.len())))?;
            if from > to || to > length {
                return Err(ToolError::new("out_of_range", format!("range {from}..{to}; paragraph {p} has {length} characters")));
            }
            let base: usize = lengths[..p].iter().map(|l| l + 1).sum();
            (base + from, base + to)
        }
    };
    let mut format = CharacterFormat::default();
    let mut warnings = Vec::new();
    let mut any = false;
    // `null` takes the format back to what the styles give (design 6.6).
    if let Some(v) = args.get("bold") {
        match v {
            Value::Null => format.reset.bold = true,
            v => format.bold = Some(parse_bool("bold", v)?),
        }
        any = true;
    }
    if let Some(v) = args.get("italic") {
        match v {
            Value::Null => format.reset.italic = true,
            v => format.italic = Some(parse_bool("italic", v)?),
        }
        any = true;
    }
    if let Some(v) = args.get("underline") {
        match v {
            Value::Null => format.reset.underline = true,
            v => format.underline = Some(parse_bool("underline", v)?),
        }
        any = true;
    }
    if let Some(v) = args.get("size") {
        match v {
            Value::Null => format.reset.size = true,
            v => format.font_size = Some(parse_size("size", v)?),
        }
        any = true;
    }
    if let Some(v) = args.get("color") {
        match parse_color("color", v)? {
            ColorArg::Rgb(rgb) => format.color = Some(rgb),
            ColorArg::Inherit => format.reset.color = true,
            ColorArg::Theme { name, brightness } => {
                // The run keeps the reference (`a:schemeClr`), so a later change of the theme changes the text colour.
                let c = theme_colour(open, &name, brightness)?;
                format.color = Some(c.rgb);
                format.color_theme = c.theme;
            }
        }
        any = true;
    }
    if let Some(v) = args.get("font") {
        match parse_font("font", v)? {
            FontArg::All(name) => format.script_fonts = Some((Some(name.clone()), Some(name))),
            FontArg::Split { latin, east_asian, .. } => format.script_fonts = Some((latin, east_asian)),
            FontArg::Inherit => format.font = Some(None),
            FontArg::Theme { major } => {
                // A run keeps the typeface's name (the theme's `+mj-lt`, `+mn-ea` … are read as the names the theme gives): the theme's typefaces now are written.
                let fonts = open.document.presentation().theme_fonts.clone().ok_or_else(|| ToolError::new("unsupported_operation", "this presentation has no theme typefaces"))?;
                let set = if major { fonts.major } else { fonts.minor };
                if set.latin.is_none() && set.east_asian.is_none() {
                    return Err(ToolError::new("unsupported_operation", "the theme has no typeface of that kind"));
                }
                warnings.push(json!({ "code": "theme_font_resolved", "message": format!("the {} theme typeface was written as {} / {}; the text does not follow a later change of the theme", if major { "heading" } else { "body" }, set.latin.as_deref().unwrap_or("-"), set.east_asian.as_deref().unwrap_or("-")) }));
                format.script_fonts = Some((set.latin, set.east_asian));
            }
        }
        any = true;
    }
    if !any {
        return Err(ToolError::invalid("no format is given (bold, italic, underline, size, color or font)"));
    }
    execute(open, vec![Command::SetCharacterFormat { slide, shape, start, end, format }])?;
    Ok(edited(id, open, json!({}), warnings))
}

/// The theme colour `name` (an `MsoThemeColorIndex` name of design 6.6) at `brightness`, as a colour that follows the theme.
pub(crate) fn theme_colour(open: &Open, name: &str, brightness: f64) -> Result<Color, ToolError> {
    let slot = ibis_doc_model::ThemeSlot::from_name(match name {
        "text1" => "tx1",
        "background1" => "bg1",
        "text2" => "tx2",
        "background2" => "bg2",
        "hyperlink" => "hlink",
        "followed_hyperlink" => "folHlink",
        other => other,
    })
    .ok_or_else(|| ToolError::invalid(format!("unknown theme colour {name}")))?;
    let reference = ibis_doc_model::ThemeRef::with_brightness(slot, brightness as f32).ok_or_else(|| ToolError::invalid("brightness"))?;
    reference.resolve(&open.document.presentation().theme_colors).ok_or_else(|| ToolError::new("unsupported_operation", format!("the theme colour {name} is not in this presentation")))
}

/// A colour for the result of a read: `"#RRGGBB"`, or `{ theme, brightness?, resolved }` for a colour that follows the theme.
pub(crate) fn colour_json(rgb: ibis_doc_model::Rgb, theme: Option<&ibis_doc_model::ThemeRef>) -> Value {
    let hex = format!("#{:06X}", rgb.0 & 0xFF_FFFF);
    let Some(t) = theme.filter(|t| t.resolved_rgb == rgb) else { return json!(hex) };
    let name = match t.name.name() {
        "tx1" => "text1",
        "bg1" => "background1",
        "tx2" => "text2",
        "bg2" => "background2",
        "hlink" => "hyperlink",
        "folHlink" => "followed_hyperlink",
        other => other,
    };
    // The transforms of the `TintAndShade` form are a brightness; the transparency is its own number. A reference with other transforms is
    // returned with its theme and the colour it works out to.
    let mut out = json!({ "theme": name, "resolved": hex });
    if let Some(b) = t.brightness() {
        out["brightness"] = json!((f64::from(b) * 1000.0).round() / 1000.0);
    }
    if t.resolved_alpha < 255 {
        out["transparency"] = json!(((1.0 - f64::from(t.resolved_alpha) / 255.0) * 100.0).round() / 100.0);
    }
    out
}

fn set_notes(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let slide = slide_id(service.open_document(id)?, args.usize_arg("slide")?)?;
    pptx_edit(service, id, vec![Command::SetNotes { slide, notes: args.str_arg("text")?.to_string() }], json!({}))
}

fn list_layouts(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let layouts: Vec<Value> = presentation_of(open)?.layouts.iter().enumerate().map(|(i, l)| json!({ "layout": i + 1, "name": l.name })).collect();
    Ok(json!({ "layouts": layouts }))
}

/// The layout a tool argument names: its number from 1 or its name (the first when `text` is none).
pub(crate) fn find_layout<'a>(current: &'a Presentation, text: Option<&str>) -> Result<&'a ibis_doc_model::Layout, ToolError> {
    Ok(match text {
        None => &current.layouts[0],
        Some(text) => match text.trim().parse::<usize>() {
            Ok(n) => n.checked_sub(1).and_then(|i| current.layouts.get(i)).ok_or_else(|| ToolError::new("out_of_range", format!("layout {n}; the presentation has {}", current.layouts.len())))?,
            Err(_) => current.layouts.iter().find(|l| l.name == text).or_else(|| current.layouts.iter().find(|l| l.name.eq_ignore_ascii_case(text))).ok_or_else(|| ToolError::invalid(format!("no layout named {text}")))?,
        },
    })
}

fn add_slide(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let current = presentation_of(open)?.clone();
    if current.layouts.is_empty() {
        return Err(ToolError::new("unsupported_operation", "the presentation has no layout to make a slide from"));
    }
    let layout = find_layout(&current, args.opt_str("layout")?)?;
    let index = args.opt_usize("index")?.unwrap_or(current.slides.len());
    if index > current.slides.len() {
        return Err(ToolError::new("out_of_range", format!("index {index}; the presentation has {} slides", current.slides.len())));
    }
    let layout_id = layout.id;
    let (slide_new, mut spare) = {
        let ids = open.document.ids();
        (ids.new_slide_id(), (0..64).map(|_| ids.new_shape_id()).collect::<Vec<_>>().into_iter())
    };
    let slide = open.source.new_slide(&current, index, slide_new, layout_id, || spare.next().unwrap_or(ShapeId(u64::MAX))).map_err(|e| ToolError::new("edit_rejected", e.to_string()))?;
    pptx_edit(service, id, vec![Command::InsertSlide { index, slide }], json!({ "slide": index + 1 }))
}

fn delete_slide(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let presentation = presentation_of(open)?;
    if presentation.slides.len() <= 1 {
        return Err(ToolError::new("unsupported_operation", "the last slide cannot be deleted"));
    }
    let slide = slide_at(presentation, args.usize_arg("slide")?)?.id;
    pptx_edit(service, id, vec![Command::DeleteSlide { slide }], json!({}))
}

fn move_slide(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let presentation = presentation_of(open)?;
    let slide = slide_at(presentation, args.usize_arg("slide")?)?.id;
    let index = args.usize_arg("index")?;
    if index >= presentation.slides.len() {
        return Err(ToolError::new("out_of_range", format!("index {index}; the presentation has {} slides", presentation.slides.len())));
    }
    pptx_edit(service, id, vec![Command::MoveSlide { slide, index }], json!({}))
}

fn set_frame(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let list = args.get("frames").and_then(Value::as_array).filter(|l| !l.is_empty()).ok_or_else(|| ToolError::invalid("frames is a list of { shape, x, y, width, height }"))?;
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let mut frames = Vec::new();
    for item in list {
        let num = |k: &str| item.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| ToolError::invalid(format!("frames[].{k} is a number of points")));
        let shape = item.get("shape").and_then(Value::as_u64).ok_or_else(|| ToolError::invalid("frames[].shape is a shape_id"))?;
        let (_, shape) = shape_id(open, number, shape)?;
        let (x, y, w, h) = (num("x")?, num("y")?, num("width")?, num("height")?);
        if w < 0.0 || h < 0.0 {
            return Err(ToolError::invalid("width and height are not negative"));
        }
        let emu = |pt: f64| (pt * 12700.0).round() as i64;
        frames.push((shape, EmuRect::new(emu(x), emu(y), emu(w), emu(h))));
    }
    // A connector glued to a moved shape follows it, in the same undo step (as the window's move does).
    let follow = ibis_doc_model::connector::follow_commands(slide_at(presentation_of(open)?, number)?, &frames);
    let mut commands = vec![Command::SetFrame { slide, frames, with_attached: true }];
    commands.extend(follow);
    pptx_edit(service, id, commands, json!({}))
}

fn delete_shape(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let list = args.get("shapes").and_then(Value::as_array).filter(|l| !l.is_empty()).ok_or_else(|| ToolError::invalid("shapes is a list of shape_id"))?;
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let mut shapes = Vec::new();
    for item in list {
        let shape = item.as_u64().ok_or_else(|| ToolError::invalid("shapes is a list of shape_id"))?;
        shapes.push(shape_id(open, number, shape)?.1);
    }
    pptx_edit(service, id, vec![Command::DeleteShape { slide, shapes, with_attached: true }], json!({}))
}

fn insert_shape(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let kind = args.str_arg("kind")?.to_string();
    let frame = args.get("frame").ok_or_else(|| ToolError::invalid("frame is { x, y, width, height } in points"))?;
    let num = |k: &str| frame.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| ToolError::invalid(format!("frame.{k} is a number of points")));
    let (x, y, w, h) = (num("x")?, num("y")?, num("width")?, num("height")?);
    if w <= 0.0 || h <= 0.0 {
        return Err(ToolError::invalid("frame.width and frame.height are above 0"));
    }
    let text = args.opt_str("text")?.unwrap_or("");
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let shape_n = open.document.ids().new_shape_id();
    let emu = |pt: f64| (pt * EMU_PER_POINT).round() as i64;
    let frame = EmuRect::new(emu(x), emu(y), emu(w), emu(h));
    let shape = if kind == "text_box" {
        Shape::text_box(shape_n, frame, text)
    } else {
        // A preset shape or a line takes the theme's accent 1 (the fill) and its darker outline, as PowerPoint's default shape.
        let accent = theme_colour(open, "accent1", 0.0)?;
        let outline = theme_colour(open, "accent1", -0.25)?;
        let stroke = Stroke::solid(outline, 12_700);
        if kind == "line" {
            Shape::new(shape_n, frame, ShapeKind::Line(stroke))
        } else {
            let geometry = ibis_formats::geometry::preset(&kind).ok_or_else(|| ToolError::invalid(format!("kind is text_box, line or a preset shape's name; {kind} is none")))?;
            let mut auto = ibis_doc_model::AutoShape::new(geometry, Fill::Solid(accent), Some(stroke));
            let style = ibis_doc_model::text::TextStyle { color: ibis_doc_model::Rgb::WHITE, align: ibis_doc_model::text::TextAlign::Center, ..Default::default() };
            let mut body = TextBody::plain(text, style);
            body.anchor = ibis_doc_model::text::VerticalAnchor::Middle;
            auto.text = Some(body);
            Shape::new(shape_n, frame, ShapeKind::Auto(auto))
        }
    };
    let index = slide_at(presentation_of(open)?, number)?.shapes.len();
    pptx_edit(service, id, vec![Command::InsertShape { slide, shapes: vec![(index, shape)] }], json!({ "shape": shape_n.0 }))
}

fn set_fill(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let list = args.get("shapes").and_then(Value::as_array).filter(|l| !l.is_empty()).ok_or_else(|| ToolError::invalid("shapes is a list of shape_id"))?;
    only_known(args, &["document_id", "slide", "shapes", "color", "gradient", "transparency", "picture"])?;
    if args.get("picture").is_some() {
        return Err(ToolError::new("unsupported_operation", "a picture fill is not supported by this tool yet"));
    }
    let alpha = match args.get("transparency") {
        None | Some(Value::Null) => 255u8,
        Some(v) => ((1.0 - v.as_f64().filter(|t| (0.0..=1.0).contains(t)).ok_or_else(|| ToolError::invalid("transparency is 0 to 1"))?) * 255.0).round() as u8,
    };
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let warnings: Vec<Value> = Vec::new();
    let paint = |arg: ColorArg| -> Result<Option<Color>, ToolError> {
        Ok(match arg {
            ColorArg::Rgb(rgb) => Some(Color { alpha, ..Color::opaque(rgb) }),
            ColorArg::Inherit => None,
            ColorArg::Theme { name, brightness } => {
                let base = theme_colour(open, &name, brightness)?;
                match base.theme {
                    // The see-through colour stays a reference: the transparency is one more transform (`a:alpha`).
                    Some(t) if alpha < 255 => {
                        let mut mods: Vec<ibis_doc_model::ThemeMod> = t.mods().collect();
                        mods.push(ibis_doc_model::ThemeMod::new("alpha", (f64::from(alpha) / 255.0 * 100_000.0).round() as i32).expect("alpha is a transform"));
                        let themed = ibis_doc_model::ThemeRef::new(t.name, t.via, &mods).ok_or_else(|| ToolError::invalid("too many transforms"))?;
                        Some(themed.resolve(&open.document.presentation().theme_colors).unwrap_or(base))
                    }
                    _ => Some(base),
                }
            }
        })
    };
    let fill = if let Some(g) = args.get("gradient") {
        let colors = g.get("colors").and_then(Value::as_array).filter(|c| c.len() >= 2).ok_or_else(|| ToolError::invalid("gradient.colors is two or more colours"))?;
        let angle = g.get("angle").and_then(Value::as_f64).unwrap_or(0.0).rem_euclid(360.0);
        let mut stops = Vec::new();
        for (i, c) in colors.iter().enumerate() {
            let color = paint(parse_color("gradient.colors[]", c)?)?.ok_or_else(|| ToolError::invalid("gradient.colors[] is a colour"))?;
            stops.push(ibis_doc_model::GradientStop { position: (100_000 * i as u32) / (colors.len() as u32 - 1), color });
        }
        Fill::Linear { angle: (angle * 60_000.0).round() as i32 % 21_600_000, scaled: false, stops }
    } else {
        let color = parse_color("color", args.get("color").ok_or_else(|| ToolError::invalid("color or gradient is required (color null: no fill)"))?)?;
        match paint(color)? {
            Some(c) => Fill::Solid(c),
            None => Fill::None,
        }
    };
    let mut commands = Vec::new();
    for item in list {
        let shape = item.as_u64().ok_or_else(|| ToolError::invalid("shapes is a list of shape_id"))?;
        let (_, shape) = shape_id(open, number, shape)?;
        commands.push(Command::SetFill { slide, shape, fill: fill.clone() });
    }
    execute(open, commands)?;
    Ok(edited(id, open, json!({}), warnings))
}

fn insert_picture(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let frame = args.get("frame").filter(|f| !f.is_null());
    let field = |k: &str| -> Result<Option<f64>, ToolError> {
        match frame.and_then(|f| f.get(k)).filter(|v| !v.is_null()) {
            None => Ok(None),
            Some(v) => v.as_f64().filter(|n| n.is_finite()).map(Some).ok_or_else(|| ToolError::invalid(format!("frame.{k} is a number of points"))),
        }
    };
    let (x, y) = (field("x")?, field("y")?);
    let (width, height) = (crate::picture_file::length_arg(frame.and_then(|f| f.get("width")), "frame.width")?, crate::picture_file::length_arg(frame.and_then(|f| f.get("height")), "frame.height")?);
    let picture = crate::picture_file::read(service, args.str_arg("path")?)?;
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let page = {
        let presentation = presentation_of(open)?;
        let size = presentation.size_of(slide_at(presentation, number)?);
        (size.width as f64 / EMU_PER_POINT, size.height as f64 / EMU_PER_POINT)
    };
    let (mut w, mut h) = crate::picture_file::size_points(width, height, picture.pixels);
    if width.is_none() && height.is_none() {
        let scale = (page.0 / w).min(page.1 / h).min(1.0);
        w *= scale;
        h *= scale;
    }
    let (x, y) = (x.unwrap_or((page.0 - w) / 2.0), y.unwrap_or((page.1 - h) / 2.0));
    let emu = |pt: f64| (pt * EMU_PER_POINT).round() as i64;
    let media = open.document.ids().add_media(picture.format, picture.data);
    let shape_n = open.document.ids().new_shape_id();
    let mut shape_picture = ibis_doc_model::Picture::new(media);
    shape_picture.name = format!("Picture {}", shape_n.0);
    let shape = Shape::new(shape_n, EmuRect::new(emu(x), emu(y), emu(w), emu(h)), ShapeKind::Picture(shape_picture));
    let index = slide_at(presentation_of(open)?, number)?.shapes.len();
    pptx_edit(service, id, vec![Command::InsertShape { slide, shapes: vec![(index, shape)] }, Command::SetAspectLocked { slide, shape: shape_n, locked: true }], json!({ "shape": shape_n.0, "x": x, "y": y, "width": w, "height": h }))
}
