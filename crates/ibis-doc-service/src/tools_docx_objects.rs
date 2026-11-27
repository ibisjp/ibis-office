// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `docx_*` drawing objects of stage B step 5 (`tasks/doc/design/2026-11-24-159-mcp-stage-b.md` 4): text boxes and shapes, their properties, a picture's properties, the watermark,
//! and the charts a document has. They are the model's own commands (`InsertTextBox`, `InsertShape`, `SetShape*`, `SetPictureSize`, `SetPictureLook`, `SetWatermark`) that the screen uses
//! too; a shape or a picture is named by the position of its run (`docx_list_shapes` and `docx_get_paragraph` say where they are).

use ibis_doc_model::drawing::Geometry;
use ibis_doc_model::flow::{FlowRunKind, Watermark};
use ibis_doc_model::flow_command::{FlowCommand, FlowPos};
use ibis_doc_model::{Command, EmuSize, Rgb};
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, boolean, document_id, object, string};
use crate::edit::{ColorArg, edited, execute, only_known, parse_bool, parse_color};
use crate::error::ToolError;
use crate::service::{Args, Service};
use crate::tools_docx::{body_paragraphs, flow_of, flow_pos, position_schema};

const EMU_PER_POINT: f64 = 12_700.0;

fn emu(points: f64) -> i64 {
    (points * EMU_PER_POINT).round() as i64
}

fn size_schema(what: &str) -> Value {
    json!({ "type": "number", "exclusiveMinimum": 0, "maximum": 20000, "description": what })
}

fn rgb_arg(name: &str, value: &Value) -> Result<Option<Rgb>, ToolError> {
    match parse_color(name, value)? {
        ColorArg::Rgb(c) => Ok(Some(c)),
        ColorArg::Inherit => Ok(None),
        ColorArg::Theme { .. } => Err(ToolError::invalid(format!("{name} is \"#RRGGBB\" or null: a theme color is not taken here"))),
    }
}

/// A line: `{ "color": "#RRGGBB", "width": points }`, or `null` for none.
fn line_arg(name: &str, value: &Value) -> Result<Option<(Rgb, i64)>, ToolError> {
    if value.is_null() {
        return Ok(None);
    }
    let bad = || ToolError::invalid(format!("{name} is {{ \"color\": \"#RRGGBB\", \"width\": points }} or null"));
    let color = rgb_arg(name, value.get("color").ok_or_else(bad)?)?.ok_or_else(bad)?;
    let width = value.get("width").and_then(Value::as_f64).filter(|w| *w > 0.0 && *w < 1000.0).ok_or_else(bad)?;
    Ok(Some((color, emu(width))))
}

fn line_schema() -> Value {
    json!({ "type": ["object", "null"], "properties": { "color": { "type": "string", "pattern": "^#[0-9A-Fa-f]{6}$" }, "width": { "type": "number", "exclusiveMinimum": 0 } }, "required": ["color", "width"], "description": "{ color, width in points }, or null for no line." })
}

pub(crate) fn edit_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "docx_insert_text_box",
            description: "Insert a text box at a position (it floats): width and height in points, and its text (no line breaks; default empty). Read it with docx_list_shapes.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("at", position_schema()), ("width", size_schema("Points.")), ("height", size_schema("Points.")), ("text", string("The text (no line breaks)."))], &["document_id", "at", "width", "height"]),
            run: insert_text_box,
        },
        Tool {
            name: "docx_insert_shape",
            description: "Insert a shape at a position (it floats): kind rectangle, rounded_rectangle or ellipse, width and height in points, fill (\"#RRGGBB\"; default none) and line ({ color, width }; default none). Read it with docx_list_shapes.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[document_id(), ("at", position_schema()), ("kind", json!({ "enum": ["rectangle", "rounded_rectangle", "ellipse"] })), ("width", size_schema("Points.")), ("height", size_schema("Points.")), ("fill", json!({ "type": ["string", "null"], "pattern": "^#[0-9A-Fa-f]{6}$" })), ("line", line_schema())],
                    &["document_id", "at", "kind", "width", "height"],
                )
            },
            run: insert_shape,
        },
        Tool {
            name: "docx_set_shape_properties",
            description: "Set the properties of the shape or text box whose run is at a position (docx_list_shapes): width and height (points), rotation (degrees clockwise), fill (\"#RRGGBB\" or null), line ({ color, width } or null) and the text (no line breaks). What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("at", position_schema()),
                        ("width", size_schema("Points.")),
                        ("height", size_schema("Points.")),
                        ("rotation", json!({ "type": "number", "description": "Degrees clockwise." })),
                        ("fill", json!({ "type": ["string", "null"], "pattern": "^#[0-9A-Fa-f]{6}$" })),
                        ("line", line_schema()),
                        ("text", string("The text (no line breaks).")),
                    ],
                    &["document_id", "at"],
                )
            },
            run: set_shape_properties,
        },
        Tool {
            name: "docx_set_picture_properties",
            description: "Set the properties of the inline picture at a position (docx_get_paragraph, pictures): width and height in points (one of them alone keeps the proportions), alt_text ({ title, description }) and decorative. What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("at", position_schema()),
                        ("width", size_schema("Points.")),
                        ("height", size_schema("Points.")),
                        ("alt_text", json!({ "type": "object", "properties": { "title": { "type": "string" }, "description": { "type": "string" } } })),
                        ("decorative", boolean("The picture is decorative (no alternative text is needed).")),
                    ],
                    &["document_id", "at"],
                )
            },
            run: set_picture_properties,
        },
        Tool {
            name: "docx_set_watermark",
            description: "Set the watermark text drawn behind every page (null removes it): font, color (\"#RRGGBB\", default #C0C0C0), semitransparent (default true) and diagonal (default true). Read it with docx_get_page_setup.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[document_id(), ("text", json!({ "type": ["string", "null"] })), ("font", string("A typeface name (default Calibri).")), ("color", json!({ "type": "string", "pattern": "^#[0-9A-Fa-f]{6}$" })), ("semitransparent", boolean("Half transparent (default true).")), ("diagonal", boolean("Across the page (default true), else level."))],
                    &["document_id", "text"],
                )
            },
            run: set_watermark,
        },
        Tool {
            name: "docx_list_charts",
            description: "The charts in the body paragraphs: where they are (paragraph and character offset), their title, frame in points and the number of series groups and of series.",
            access: Access::Read,
            returns_content: true,
            schema: &|| object(&[document_id()], &["document_id"]),
            run: list_charts,
        },
    ]
}

fn run_one(service: &mut Service, id: &str, commands: Vec<FlowCommand>) -> Result<Value, ToolError> {
    let open = service.open_document(id)?;
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({}), Vec::new()))
}

fn at_of(service: &mut Service, args: &Value) -> Result<FlowPos, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    flow_pos(flow_of(open)?, args.get("at").unwrap_or(&Value::Null), "at")
}

fn number(args: &Value, name: &str) -> Result<Option<f64>, ToolError> {
    match args.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().filter(|n| n.is_finite() && *n > 0.0 && *n <= 20_000.0).map(Some).ok_or_else(|| ToolError::invalid(format!("{name} is a positive number of points"))),
    }
}

fn insert_text_box(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at", "width", "height", "text"])?;
    let (w, h) = (number(args, "width")?.ok_or_else(|| ToolError::invalid("width is required"))?, number(args, "height")?.ok_or_else(|| ToolError::invalid("height is required"))?);
    let text = args.opt_str("text")?.unwrap_or("").to_string();
    if text.contains(['\n', '\r']) {
        return Err(ToolError::invalid("text has a line break"));
    }
    let at = at_of(service, args)?;
    run_one(service, args.str_arg("document_id")?, vec![FlowCommand::InsertTextBox { at, size: EmuSize { width: emu(w), height: emu(h) }, text }])
}

fn insert_shape(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at", "kind", "width", "height", "fill", "line"])?;
    let (w, h) = (number(args, "width")?.ok_or_else(|| ToolError::invalid("width is required"))?, number(args, "height")?.ok_or_else(|| ToolError::invalid("height is required"))?);
    let geometry = match args.str_arg("kind")? {
        "rectangle" => Geometry::Rect,
        "rounded_rectangle" => Geometry::RoundRect { radius: 16_667 },
        "ellipse" => Geometry::Ellipse,
        other => return Err(ToolError::invalid(format!("kind {other} is not rectangle, rounded_rectangle or ellipse"))),
    };
    let fill = match args.get("fill") {
        None => None,
        Some(v) => rgb_arg("fill", v)?,
    };
    let line = match args.get("line") {
        None => None,
        Some(v) => line_arg("line", v)?,
    };
    let at = at_of(service, args)?;
    run_one(service, args.str_arg("document_id")?, vec![FlowCommand::InsertShape { at, size: EmuSize { width: emu(w), height: emu(h) }, geometry, fill, line }])
}

fn set_shape_properties(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at", "width", "height", "rotation", "fill", "line", "text"])?;
    let id = args.str_arg("document_id")?;
    let at = at_of(service, args)?;
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    // The shape the position names: its frame is what a width or a height not given stays.
    let paragraph = body_paragraphs(flow).into_iter().find(|p| p.id == at.para).ok_or_else(|| ToolError::new("out_of_range", "the position is not in a body paragraph"))?;
    let mut offset = 0usize;
    let mut found = None;
    for r in &paragraph.runs {
        if let FlowRunKind::Shape(s) = &r.kind {
            if offset == at.offset {
                found = Some(s.frame);
            }
        }
        offset += r.len();
    }
    let frame = found.ok_or_else(|| ToolError::new("out_of_range", "there is no shape at that position: see docx_list_shapes"))?;
    let mut commands = Vec::new();
    let (w, h) = (number(args, "width")?, number(args, "height")?);
    if w.is_some() || h.is_some() {
        commands.push(FlowCommand::SetShapeGeometry { at, size: Some(EmuSize { width: w.map_or(frame.width, emu), height: h.map_or(frame.height, emu) }), offset: None });
    }
    if let Some(r) = args.get("rotation").filter(|v| !v.is_null()) {
        let deg = r.as_f64().filter(|d| d.is_finite()).ok_or_else(|| ToolError::invalid("rotation is a number of degrees"))?;
        commands.push(FlowCommand::SetShapeRotation { at, rotation: (deg.rem_euclid(360.0) * 60_000.0).round() as i32 });
    }
    let fill = args.get("fill").map(|v| rgb_arg("fill", v)).transpose()?;
    let line = args.get("line").map(|v| line_arg("line", v)).transpose()?;
    if fill.is_some() || line.is_some() {
        commands.push(FlowCommand::SetShapeStyle { at, fill, line });
    }
    if let Some(text) = args.opt_str("text")? {
        if text.contains(['\n', '\r']) {
            return Err(ToolError::invalid("text has a line break"));
        }
        commands.push(FlowCommand::SetShapeText { at, text: text.to_string() });
    }
    if commands.is_empty() {
        return Err(ToolError::invalid("give at least one of width, height, rotation, fill, line, text"));
    }
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({}), Vec::new()))
}

fn set_picture_properties(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "at", "width", "height", "alt_text", "decorative"])?;
    let id = args.str_arg("document_id")?;
    let at = at_of(service, args)?;
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let paragraph = body_paragraphs(flow).into_iter().find(|p| p.id == at.para).ok_or_else(|| ToolError::new("out_of_range", "the position is not in a body paragraph"))?;
    let mut offset = 0usize;
    let mut found = None;
    for r in &paragraph.runs {
        if let FlowRunKind::Picture { size, look, .. } = &r.kind {
            if offset == at.offset {
                found = Some((*size, look.clone()));
            }
        }
        offset += r.len();
    }
    let (size, mut look) = found.ok_or_else(|| ToolError::new("out_of_range", "there is no inline picture at that position: see docx_get_paragraph (pictures)"))?;
    let mut commands = Vec::new();
    let (w, h) = (number(args, "width")?, number(args, "height")?);
    if w.is_some() || h.is_some() {
        let (cw, ch) = (size.width as f64, size.height as f64);
        let (nw, nh) = match (w, h) {
            (Some(w), Some(h)) => (emu(w) as f64, emu(h) as f64),
            (Some(w), None) => (emu(w) as f64, emu(w) as f64 * ch / cw.max(1.0)),
            (None, Some(h)) => (emu(h) as f64 * cw / ch.max(1.0), emu(h) as f64),
            (None, None) => (cw, ch),
        };
        commands.push(FlowCommand::SetPictureSize { at, size: EmuSize { width: nw.round().max(1.0) as i64, height: nh.round().max(1.0) as i64 } });
    }
    let mut look_changed = false;
    if let Some(alt) = args.get("alt_text").filter(|v| !v.is_null()) {
        let obj = alt.as_object().ok_or_else(|| ToolError::invalid("alt_text is { title, description }"))?;
        for (key, slot) in [("title", &mut look.alt_text.title), ("description", &mut look.alt_text.description)] {
            if let Some(v) = obj.get(key) {
                *slot = v.as_str().ok_or_else(|| ToolError::invalid(format!("alt_text.{key} is a string")))?.to_string();
            }
        }
        look_changed = true;
    }
    if let Some(v) = args.get("decorative") {
        look.decorative = parse_bool("decorative", v)?;
        look_changed = true;
    }
    if look_changed {
        commands.push(FlowCommand::SetPictureLook { at, look });
    }
    if commands.is_empty() {
        return Err(ToolError::invalid("give at least one of width, height, alt_text, decorative"));
    }
    execute(open, vec![Command::Flow(FlowCommand::Many(commands))])?;
    Ok(edited(id, open, json!({}), Vec::new()))
}

fn set_watermark(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "text", "font", "color", "semitransparent", "diagonal"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let flow = flow_of(open)?;
    let watermark = match args.get("text") {
        Some(Value::Null) => None,
        Some(Value::String(text)) if !text.trim().is_empty() && !text.contains(['\n', '\r']) => {
            let color = match args.get("color") {
                None => Rgb(0xC0_C0C0),
                Some(v) => rgb_arg("color", v)?.ok_or_else(|| ToolError::invalid("color is \"#RRGGBB\""))?,
            };
            let font = args.opt_str("font")?.filter(|f| !f.trim().is_empty()).unwrap_or("Calibri").to_string();
            let semitransparent = args.opt_bool("semitransparent")?.unwrap_or(true);
            let diagonal = args.opt_bool("diagonal")?.unwrap_or(true);
            Some(Watermark { text: text.clone(), font, color, semitransparent, rotation: if diagonal { 315 } else { 0 }, size: None })
        }
        _ => return Err(ToolError::invalid("text is one line of text, or null to remove the watermark")),
    };
    if watermark == flow.watermark {
        return Ok(edited(id, open, json!({ "changed": false }), Vec::new()));
    }
    execute(open, vec![Command::Flow(FlowCommand::SetWatermark { watermark, sections: Vec::new() })])?;
    Ok(edited(id, open, json!({ "changed": true }), Vec::new()))
}

fn list_charts(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let flow = flow_of(open)?;
    let pt = |e: i64| (e as f64 / EMU_PER_POINT * 100.0).round() / 100.0;
    let mut items = Vec::new();
    for (n, p) in body_paragraphs(flow).iter().enumerate() {
        let mut offset = 0usize;
        for r in &p.runs {
            if let FlowRunKind::Shape(s) = &r.kind {
                if let ibis_doc_model::ShapeKind::Chart(chart) = &s.kind {
                    items.push(json!({
                        "paragraph": n,
                        "offset": offset,
                        "title": chart.title.as_ref().map(|t| t.text.clone()),
                        "frame": { "width": pt(s.frame.width), "height": pt(s.frame.height) },
                        "groups": chart.groups.len(),
                        "series": chart.groups.iter().map(|g| g.series.len()).sum::<usize>(),
                        "legend": chart.legend.is_some(),
                    }));
                }
            }
            offset += r.len();
        }
    }
    Ok(json!({ "charts": items }))
}
