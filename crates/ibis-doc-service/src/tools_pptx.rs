// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `pptx_*` tools (design 6.4). A slide is numbered from 1 (or named by its `slide_id`); a shape by its `shape_id`. Lengths are points.

use ibis_doc_model::{Presentation, Shape, ShapeKind, Slide};
use ibis_doc_model::text::TextBody;
use ibis_formats::Kind;
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, document_id, integer, object};
use crate::error::ToolError;
use crate::service::{Args, Open, Service};

const EMU_PER_POINT: f64 = 12700.0;

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "pptx_list_slides", description: "The slides: number, id, title (the first text), shapes, hidden and whether it has notes.", access: Access::Read, returns_content: true, schema: || object(&[document_id()], &["document_id"]), run: list_slides },
        Tool {
            name: "pptx_get_slide",
            description: "The shapes of a slide: id, kind, position and size (points) and text.",
            access: Access::Read,
            returns_content: true,
            schema: || object(&[document_id(), ("slide", integer("The slide's number, from 1."))], &["document_id", "slide"]),
            run: get_slide,
        },
        Tool {
            name: "pptx_get_shape_text",
            description: "The paragraphs of a shape's text with the formatting of each run.",
            access: Access::Read,
            returns_content: true,
            schema: || object(&[document_id(), ("slide", integer("The slide's number, from 1.")), ("shape", integer("The shape's shape_id."))], &["document_id", "slide", "shape"]),
            run: get_shape_text,
        },
        Tool { name: "pptx_get_notes", description: "The speaker notes of a slide.", access: Access::Read, returns_content: true, schema: || object(&[document_id(), ("slide", integer("The slide's number, from 1."))], &["document_id", "slide"]), run: get_notes },
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
            json!({ "slide": i + 1, "slide_id": s.id.0, "title": title, "shapes": s.shapes.len(), "hidden": s.hidden, "has_notes": !s.notes.is_empty() })
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
        .map(|s| json!({ "shape_id": s.id.0, "kind": kind_label(s), "x": points(s.frame.x), "y": points(s.frame.y), "width": points(s.frame.width), "height": points(s.frame.height), "text": shape_text(s) }))
        .collect();
    Ok(json!({ "slide": number, "slide_id": slide.id.0, "hidden": slide.hidden, "shapes": shapes }))
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
            let runs: Vec<Value> = p.runs.iter().map(|r| json!({ "text": r.text, "bold": r.style.bold, "italic": r.style.italic, "underline": r.style.underline, "size": r.style.font_size.map(|v| f64::from(v) / 100.0), "color": r.style.color.map(|c| format!("#{:06X}", c.0 & 0xFF_FFFF)), "font": r.style.font })).collect();
            json!({ "text": p.runs.iter().map(|r| r.text.as_str()).collect::<String>(), "runs": runs })
        })
        .collect();
    Ok(json!({ "slide": number, "shape_id": id, "paragraphs": paragraphs }))
}

fn get_notes(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let number = args.usize_arg("slide")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let slide = slide_at(presentation_of(open)?, number)?;
    Ok(json!({ "slide": number, "notes": slide.notes }))
}

// ---- edits ----

use ibis_doc_model::text::CharacterFormat;
use ibis_doc_model::{Command, EmuRect, ShapeId, SlideId};

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
            schema: || object(&[document_id(), slide_arg(), ("shape", integer("The shape's shape_id.")), ("text", string("The new text."))], &["document_id", "slide", "shape", "text"]),
            run: set_shape_text,
        },
        Tool {
            name: "pptx_replace_text",
            description: "Replace a text with another in the shapes of one slide or of every slide, as one undo step.",
            access: Access::Edit,
            returns_content: false,
            schema: || object(&[document_id(), ("find", string("The text to find.")), ("replace", string("The new text.")), ("slide", integer("Only this slide (from 1).")), ("match_case", boolean("Match upper and lower case (default false)."))], &["document_id", "find", "replace"]),
            run: replace_text,
        },
        Tool {
            name: "pptx_set_run_format",
            description: "Set the character format of a shape's text (all of it, or one paragraph's characters from..to): bold, italic, underline, size (pt), color (\"#RRGGBB\"; a theme colour is resolved to its colour), font (a typeface name). What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: || {
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
        Tool { name: "pptx_set_notes", description: "Set the speaker notes of a slide.", access: Access::Edit, returns_content: false, schema: || object(&[document_id(), slide_arg(), ("text", string("The notes."))], &["document_id", "slide", "text"]), run: set_notes },
        Tool {
            name: "pptx_add_slide",
            description: "Add a slide from a layout (a layout's name or number from 1, see pptx_list_layouts; default the first) at index (0-based; default the end).",
            access: Access::Edit,
            returns_content: false,
            schema: || object(&[document_id(), ("layout", string("The layout's name, or its number from 1.")), ("index", integer("Where it goes (0-based)."))], &["document_id"]),
            run: add_slide,
        },
        Tool { name: "pptx_delete_slide", description: "Delete a slide (not the last one).", access: Access::Edit, returns_content: false, schema: || object(&[document_id(), slide_arg()], &["document_id", "slide"]), run: delete_slide },
        Tool { name: "pptx_move_slide", description: "Move a slide to index (0-based).", access: Access::Edit, returns_content: false, schema: || object(&[document_id(), slide_arg(), ("index", integer("The new place (0-based)."))], &["document_id", "slide", "index"]), run: move_slide },
        Tool {
            name: "pptx_set_frame",
            description: "Move and resize shapes: frames is a list of { shape, x, y, width, height } in points; the shapes attached to them follow.",
            access: Access::Edit,
            returns_content: false,
            schema: || {
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
        Tool { name: "pptx_delete_shape", description: "Delete shapes (a list of shape_id) with the shapes attached to them.", access: Access::Edit, returns_content: false, schema: || object(&[document_id(), slide_arg(), ("shapes", json!({ "type": "array", "items": { "type": "integer" } }))], &["document_id", "slide", "shapes"]), run: delete_shape },
        Tool { name: "pptx_list_layouts", description: "The layouts a new slide can follow: number and name.", access: Access::Read, returns_content: false, schema: || object(&[document_id()], &["document_id"]), run: list_layouts },
    ]
}

fn slide_arg() -> (&'static str, Value) {
    ("slide", integer("The slide's number, from 1."))
}

fn slide_id(open: &Open, number: usize) -> Result<SlideId, ToolError> {
    Ok(slide_at(presentation_of(open)?, number)?.id)
}

fn pptx_edit(service: &mut Service, id: &str, commands: Vec<Command>, extra: Value) -> Result<Value, ToolError> {
    let open = service.open_document(id)?;
    execute(open, commands)?;
    Ok(edited(id, open, extra, Vec::new()))
}

fn shape_id(open: &Open, slide: usize, shape: u64) -> Result<(SlideId, ShapeId), ToolError> {
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
                // The text colour of a shape is kept as a colour, not as a theme reference: the theme's colour now is written.
                let flow_theme = ibis_doc_model::flow::FlowThemeColor::new(name.clone(), None::<String>, None::<String>);
                let base = flow_theme.resolve(&open.document.presentation().theme_colors).ok_or_else(|| ToolError::new("unsupported_operation", format!("the theme colour {name} is not in this presentation")))?;
                warnings.push(json!({ "code": "theme_color_resolved", "message": format!("{name} was written as its colour {:06X}; the text does not follow a later change of the theme", base.0 & 0xFF_FFFF) }));
                format.color = Some(shade_of(base, brightness));
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

/// A colour made lighter (`brightness` > 0, towards white) or darker (< 0, towards black) by that fraction.
fn shade_of(base: ibis_doc_model::Rgb, brightness: f64) -> ibis_doc_model::Rgb {
    let channel = |shift: u32| {
        let c = ((base.0 >> shift) & 0xFF) as f64;
        let v = if brightness >= 0.0 { c + (255.0 - c) * brightness } else { c * (1.0 + brightness) };
        v.round().clamp(0.0, 255.0) as u32
    };
    ibis_doc_model::Rgb((channel(16) << 16) | (channel(8) << 8) | channel(0))
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

fn add_slide(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let current = presentation_of(open)?.clone();
    if current.layouts.is_empty() {
        return Err(ToolError::new("unsupported_operation", "the presentation has no layout to make a slide from"));
    }
    let layout = match args.opt_str("layout")? {
        None => &current.layouts[0],
        Some(text) => match text.trim().parse::<usize>() {
            Ok(n) => n.checked_sub(1).and_then(|i| current.layouts.get(i)).ok_or_else(|| ToolError::new("out_of_range", format!("layout {n}; the presentation has {}", current.layouts.len())))?,
            Err(_) => current.layouts.iter().find(|l| l.name == text).or_else(|| current.layouts.iter().find(|l| l.name.eq_ignore_ascii_case(text))).ok_or_else(|| ToolError::invalid(format!("no layout named {text}")))?,
        },
    };
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
    pptx_edit(service, id, vec![Command::SetFrame { slide, frames, with_attached: true }], json!({}))
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
