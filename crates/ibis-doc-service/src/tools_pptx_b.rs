// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `pptx_*` tools of stage B (`tasks/doc/design/2026-11-24-159-mcp-stage-b.md` 6): lines, the order of shapes, and the properties of a
//! shape. One call is one step of the undo history; what the model cannot do is refused (`unsupported_operation`).

use ibis_doc_model::drawing::{EndSize, LineDash, LineEnd, Stroke};
use ibis_doc_model::{Color, Command, ShapeId, ShapeKind};
use serde_json::{Value, json};

use crate::catalog::{Access, Tool, boolean, document_id, integer, object, string};
use crate::edit::{ColorArg, only_known, parse_bool, parse_color};
use crate::error::ToolError;
use crate::service::{Args, Service};
use crate::tools_pptx::{colour_json, pptx_edit, presentation_of, shape_id, slide_arg, slide_at, slide_id};
use crate::tools_docx::color_schema;

/// The tools of this file, registered in one place (`catalog.rs`).
pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "pptx_set_line",
            description: "Set the outline of shapes and lines: color (\"#RRGGBB\" or a theme colour; null: no line), width (pt), dash (solid, dot, dash, long_dash, dash_dot, long_dash_dot, long_dash_dot_dot), arrow_head and arrow_tail (none, triangle, stealth, arrow, diamond, oval). What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("shapes", json!({ "type": "array", "items": { "type": "integer" }, "description": "shape_ids." })),
                        ("color", color_schema()),
                        ("width", json!({ "type": "number", "description": "Points." })),
                        ("dash", string("Dash pattern.")),
                        ("arrow_head", string("The start of the line.")),
                        ("arrow_tail", string("The end of the line.")),
                    ],
                    &["document_id", "slide", "shapes"],
                )
            },
            run: set_line,
        },
        Tool {
            name: "pptx_arrange_shapes",
            description: "Change the stacking order of shapes: order is front, back, forward or backward.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[document_id(), slide_arg(), ("shapes", json!({ "type": "array", "items": { "type": "integer" }, "description": "shape_ids." })), ("order", string("front, back, forward or backward."))],
                    &["document_id", "slide", "shapes", "order"],
                )
            },
            run: arrange_shapes,
        },
        Tool {
            name: "pptx_set_shape_properties",
            description: "Set the properties of a shape: name, alt_text (the description), decorative, hidden, rotation (degrees clockwise), flip ({ horizontal, vertical }) and aspect_locked. What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("shape", integer("The shape's shape_id.")),
                        ("name", string("The shape's name.")),
                        ("alt_text", string("The alternative text (description).")),
                        ("decorative", boolean("Marked as decorative.")),
                        ("hidden", boolean("Hidden.")),
                        ("rotation", json!({ "type": "number", "description": "Degrees clockwise." })),
                        ("flip", json!({ "type": "object", "properties": { "horizontal": { "type": "boolean" }, "vertical": { "type": "boolean" } } })),
                        ("aspect_locked", boolean("Keep the aspect ratio when resized.")),
                        ("hyperlink", json!({ "description": "A web address (text), { \"slide\": number } for a slide, or null to remove the link." })),
                    ],
                    &["document_id", "slide", "shape"],
                )
            },
            run: set_shape_properties,
        },
        Tool {
            name: "pptx_set_paragraph_format",
            description: "Set the paragraph format of a shape's text (all paragraphs, or the listed ones): alignment (left, center, right, justify, distributed), indent_left and indent_first_line (pt), space_before and space_after (pt), line_spacing (a multiple such as 1.5, or { points }), level (0 to 8) and bullet (none, bullet, numbered). What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("shape", integer("The shape's shape_id.")),
                        ("paragraphs", json!({ "type": "array", "items": { "type": "integer" }, "description": "Paragraphs (0-based). Omit for all." })),
                        ("alignment", string("left, center, right, justify or distributed.")),
                        ("indent_left", json!({ "type": "number", "description": "Points." })),
                        ("indent_first_line", json!({ "type": "number", "description": "Points; negative for hanging." })),
                        ("space_before", json!({ "type": "number", "description": "Points." })),
                        ("space_after", json!({ "type": "number", "description": "Points." })),
                        ("line_spacing", json!({ "description": "A multiple of the line (1 is single), or { \"points\": n }." })),
                        ("level", integer("Outline level, 0 to 8.")),
                        ("bullet", string("none, bullet or numbered.")),
                    ],
                    &["document_id", "slide", "shape"],
                )
            },
            run: set_paragraph_format,
        },
        Tool {
            name: "pptx_set_text_frame",
            description: "Set how a shape holds its text: vertical_alignment (top, middle, bottom), margins ({ left, top, right, bottom } in pt), wrap, autofit (none, shrink, resize_shape) and columns. What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("shape", integer("The shape's shape_id.")),
                        ("vertical_alignment", string("top, middle or bottom.")),
                        ("margins", json!({ "type": "object", "properties": { "left": { "type": "number" }, "top": { "type": "number" }, "right": { "type": "number" }, "bottom": { "type": "number" } } })),
                        ("wrap", boolean("Wrap lines at the shape's width.")),
                        ("autofit", string("none, shrink or resize_shape.")),
                        ("columns", integer("Number of columns, 1 or more.")),
                    ],
                    &["document_id", "slide", "shape"],
                )
            },
            run: set_text_frame,
        },
        Tool {
            name: "pptx_copy_shape",
            description: "Copy shapes to the same slide or to another (to_slide, 1-based), moved by offset points (default 10). The copies are put in front. Shapes in a group, charts, tables of a sheet and placeholders are not copied (unsupported_operation). The result lists the new shape_ids.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("shapes", json!({ "type": "array", "items": { "type": "integer" }, "description": "shape_ids." })),
                        ("to_slide", integer("The slide to copy to, from 1. Default the same slide.")),
                        ("offset", json!({ "type": "number", "description": "Points to the right and down. Default 10 on the same slide, 0 on another." })),
                    ],
                    &["document_id", "slide", "shapes"],
                )
            },
            run: copy_shape,
        },
        Tool {
            name: "pptx_group_shapes",
            description: "Group two or more shapes of a slide that are not in a group yet. The result has the new group's id.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), slide_arg(), ("shapes", json!({ "type": "array", "items": { "type": "integer" }, "description": "shape_ids." }))], &["document_id", "slide", "shapes"]),
            run: group_shapes,
        },
        Tool {
            name: "pptx_ungroup_shapes",
            description: "Undo a group (its id is in pptx_get_slide's groups); a group inside it stays a group.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), slide_arg(), ("group", string("The group's id (pptx_get_slide's groups[].group)."))], &["document_id", "slide", "group"]),
            run: ungroup_shapes,
        },
        Tool {
            name: "pptx_copy_slide",
            description: "Copy a slide as PowerPoint's duplicate: the copy follows the slide (or is put at index, 0-based) and has the same shapes and pictures. The result has the new slide's number.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), slide_arg(), ("index", integer("Where the copy goes (0-based). Default right after the slide."))], &["document_id", "slide"]),
            run: copy_slide,
        },
        Tool {
            name: "pptx_add_animation",
            description: "Add an animation effect to a shape, at the end of the slide's animations: effect (appear, fade, fly, zoom, wipe, spin; for fly and wipe a direction: from_left, from_right, from_top, from_bottom), class (entrance, emphasis, exit; default entrance), start (on_click, with_previous, after_previous; default on_click), delay and duration (seconds; default the effect's usual time).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("shape", integer("The shape's shape_id.")),
                        ("effect", string("appear, fade, fly, zoom, wipe or spin.")),
                        ("direction", string("For fly and wipe: from_left, from_right, from_top or from_bottom.")),
                        ("class", string("entrance, emphasis or exit.")),
                        ("start", string("on_click, with_previous or after_previous.")),
                        ("delay", json!({ "type": "number", "description": "Seconds." })),
                        ("duration", json!({ "type": "number", "description": "Seconds." })),
                    ],
                    &["document_id", "slide", "shape", "effect"],
                )
            },
            run: add_animation,
        },
        Tool {
            name: "pptx_add_comment",
            description: "Add a comment to a slide (or to a shape): text, author (default Assistant), shape (the shape it is on) and position ({ x, y } in points; a comment on the slide is at its top left by default). The time is now.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("text", string("The comment.")),
                        ("author", string("Who writes it. Default Assistant.")),
                        ("shape", integer("The shape's shape_id, for a comment on a shape.")),
                        ("position", json!({ "type": "object", "properties": { "x": { "type": "number" }, "y": { "type": "number" } } })),
                    ],
                    &["document_id", "slide", "text"],
                )
            },
            run: add_comment,
        },
        Tool {
            name: "pptx_align_shapes",
            description: "Line shapes up or spread them out: edge is left, center, right, top, middle, bottom, distribute_horizontally or distribute_vertically (three or more shapes, equal gaps). relative_to is selection (the box of the shapes; one shape is always lined up on the slide) or slide.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("shapes", json!({ "type": "array", "items": { "type": "integer" }, "description": "shape_ids." })),
                        ("edge", string("left, center, right, top, middle, bottom, distribute_horizontally or distribute_vertically.")),
                        ("relative_to", string("selection (default) or slide.")),
                    ],
                    &["document_id", "slide", "shapes", "edge"],
                )
            },
            run: align_shapes,
        },
        Tool {
            name: "pptx_insert_chart",
            description: "Add a chart to a slide, as 挿入 > グラフ does: kind column, stacked_column, percent_stacked_column, bar, stacked_bar, percent_stacked_bar, line, stacked_line, area, stacked_area, pie, doughnut, scatter, scatter_smooth, scatter_lines, radar or radar_filled; frame ({ x, y, width, height } in points; default the middle of the slide). data is the numbers to chart as rows of cells like a selection in a sheet: the first row names the series, the first column the categories, the rest are numbers (series_in says whether the series are the columns, the default, or the rows); the chart is kept with them in an embedded workbook. Without data it has the kind's sample data. title (a string; null: none) and legend (right, left, top, bottom, none) are as xlsx_insert_chart takes them. Read it with pptx_list_charts. The result has its shape_id.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("kind", json!({ "enum": crate::tools_docx_layout::CHART_KINDS.iter().map(|k| k.0).collect::<Vec<_>>() })),
                        ("frame", json!({ "type": "object", "properties": { "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" } } })),
                        ("data", json!({ "type": "array", "minItems": 2, "items": { "type": "array", "minItems": 2, "items": { "type": ["number", "string", "boolean", "null"] } }, "description": "Rows of cells: the first row names the series, the first column the categories, the rest are numbers." })),
                        ("series_in", json!({ "enum": ["columns", "rows"], "description": "Whether each column or each row is a series (default: as the data suggests)." })),
                        ("title", json!({ "type": ["string", "null"], "description": "The chart's title; null for none." })),
                        ("legend", json!({ "enum": ["right", "left", "top", "bottom", "none"] })),
                    ],
                    &["document_id", "slide", "kind"],
                )
            },
            run: insert_chart,
        },
        Tool {
            name: "pptx_set_chart_data",
            description: "Replace the data of a chart (pptx_list_charts gives its shape_id), as the chart's data grid does: data is rows of cells like a selection in a sheet (the first row names the series, the first column the categories, the rest are numbers; series_in says whether the series are the columns, the default, or the rows). The chart keeps its kind and formatting, and its embedded workbook is made from the data. One undo step.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("shape", integer("The chart's shape_id.")),
                        ("data", json!({ "type": "array", "minItems": 2, "items": { "type": "array", "minItems": 2, "items": { "type": ["number", "string", "boolean", "null"] } }, "description": "Rows of cells: the first row names the series, the first column the categories, the rest are numbers." })),
                        ("series_in", json!({ "enum": ["columns", "rows"], "description": "Whether each column or each row is a series (default: as the data suggests)." })),
                    ],
                    &["document_id", "slide", "shape", "data"],
                )
            },
            run: set_chart_data,
        },
        Tool {
            name: "pptx_list_charts",
            description: "The charts of a slide: shape_id, position and size (points), and the series (name, number of values).",
            access: Access::Read,
            returns_content: true,
            schema: &|| object(&[document_id(), slide_arg()], &["document_id", "slide"]),
            run: list_charts,
        },
        Tool {
            name: "pptx_insert_connector",
            description: "Add a connector (line) to a slide from start to end. Each end is { shape, site } (glued to that shape's connection site: a rectangle, rounded rectangle, diamond or text box has 4 (0 top, 1 left, 2 bottom, 3 right); an ellipse has 8) or { x, y } in points. A glued end follows its shape when the shape moves. kind is straight (default), elbow or curved; arrow is none (default), end, start or both. The result has its shape_id.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                let end = json!({ "type": "object", "description": "{ shape, site } or { x, y } in points." });
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("start", end.clone()),
                        ("end", end),
                        ("kind", json!({ "type": "string", "enum": ["straight", "elbow", "curved"] })),
                        ("arrow", json!({ "type": "string", "enum": ["none", "end", "start", "both"] })),
                    ],
                    &["document_id", "slide", "start", "end"],
                )
            },
            run: insert_connector,
        },
        Tool {
            name: "pptx_set_theme",
            description: "Replace colours and typefaces of the theme: colors is an object of scheme names (dk1, lt1, dk2, lt2, accent1..accent6, hlink, folHlink) to \"#RRGGBB\"; fonts is { major, minor } each { latin?, east_asian? }. What is not given is not changed. Colours that follow the theme (text and fills kept as theme colours) change with it. theme takes the colours and typefaces of another pptx or potx file (colors and fonts given with it win over the file's).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("colors", json!({ "type": "object", "description": "Scheme name to \"#RRGGBB\"." })),
                        ("fonts", json!({ "type": "object", "properties": { "major": { "type": "object" }, "minor": { "type": "object" } } })),
                        ("theme", string("The path of a pptx or potx in an allowed folder: its theme colours and typefaces are taken, as the theme panel does. colors and fonts given as well change what it took.")),
                    ],
                    &["document_id"],
                )
            },
            run: set_theme,
        },
        Tool {
            name: "pptx_insert_table",
            description: "Add a table to a slide: rows and columns, frame ({ x, y, width, height } in points; default across the slide) and cells (a list of rows of texts). The table has PowerPoint's default style. The result has its shape_id.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        slide_arg(),
                        ("rows", integer("Number of rows, 1 to 100.")),
                        ("columns", integer("Number of columns, 1 to 50.")),
                        ("frame", json!({ "type": "object", "properties": { "x": { "type": "number" }, "y": { "type": "number" }, "width": { "type": "number" }, "height": { "type": "number" } } })),
                        ("cells", json!({ "type": "array", "items": { "type": "array", "items": { "type": "string" } }, "description": "Rows of cell texts." })),
                    ],
                    &["document_id", "slide", "rows", "columns"],
                )
            },
            run: insert_table,
        },
        Tool {
            name: "pptx_set_table_cell",
            description: "Set a cell of a table (row and column from 0): text and fill (a colour, a theme colour, or null for no fill).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[document_id(), slide_arg(), ("shape", integer("The table's shape_id.")), ("row", integer("Row, from 0.")), ("column", integer("Column, from 0.")), ("text", string("The cell's text.")), ("fill", color_schema())],
                    &["document_id", "slide", "shape", "row", "column"],
                )
            },
            run: set_table_cell,
        },
        Tool {
            name: "pptx_set_slide_properties",
            description: "Set the properties of a slide: hidden (skipped in the show), name, and background (a colour \"#RRGGBB\" or a theme colour; null: back to the layout's). layout (a number from 1 or a name, as pptx_list_layouts) applies a layout as PowerPoint does: the placeholders follow it and the shapes that are not placeholders stay.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), slide_arg(), ("hidden", boolean("Skipped in the slide show.")), ("name", string("The slide's name (null: none).")), ("background", color_schema()), ("layout", string("A layout: its number from 1 or its name."))], &["document_id", "slide"]),
            run: set_slide_properties,
        },
        Tool {
            name: "pptx_set_slide_size",
            description: "Set the slide size: width and height in points (1 to 56 inches), or preset (16:9, 4:3, a4). The content keeps its place (scale_content is not supported).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("width", json!({ "type": "number", "description": "Points." })), ("height", json!({ "type": "number", "description": "Points." })), ("preset", string("16:9, 4:3 or a4.")), ("scale_content", boolean("Not supported (must be false or omitted)."))], &["document_id"]),
            run: set_slide_size,
        },
        Tool {
            name: "pptx_set_transition",
            description: "Set the slide transition of slides: effect (an id of pptx_get_transition, such as fade, push, wipe, morph; null: none), direction (an option of the effect), duration (seconds), advance_on_click and advance_after (seconds, null: not by itself).",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[
                        document_id(),
                        ("slides", json!({ "type": "array", "items": { "type": "integer" }, "description": "Slide numbers from 1." })),
                        ("effect", string("The effect's id, or null for none.")),
                        ("direction", string("The effect's option, such as l, r, u, d, in, out.")),
                        ("duration", json!({ "type": "number", "description": "Seconds." })),
                        ("advance_on_click", boolean("Move on at a click.")),
                        ("advance_after", json!({ "type": "number", "description": "Seconds; null for not by itself." })),
                    ],
                    &["document_id", "slides", "effect"],
                )
            },
            run: set_transition,
        },
        Tool {
            name: "pptx_delete_animation",
            description: "Delete the animations of a slide at these positions (0-based, as pptx_list_animations lists them), or all of them when index is omitted.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), slide_arg(), ("index", json!({ "type": "array", "items": { "type": "integer" }, "description": "Positions (0-based). Omit for all." }))], &["document_id", "slide"]),
            run: delete_animation,
        },
        Tool {
            name: "pptx_set_header_footer",
            description: "Set the header and footer marks of the presentation (what Insert > Header & Footer applies to every slide): date, slide_number, footer (booleans) and hide_on_title (not on title slides). What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| object(&[document_id(), ("date", boolean("Show the date.")), ("slide_number", boolean("Show the slide number.")), ("footer", boolean("Show the footer.")), ("hide_on_title", boolean("Not on title slides."))], &["document_id"]),
            run: set_header_footer,
        },
        Tool {
            name: "pptx_set_document_properties",
            description: "Set the document properties of the presentation: title, subject, author, keywords, description, category, company, manager (text, or null to clear). What is not given is not changed.",
            access: Access::Edit,
            returns_content: false,
            schema: &|| {
                object(
                    &[document_id(), ("title", string("Title.")), ("subject", string("Subject.")), ("author", string("Author.")), ("keywords", string("Keywords.")), ("description", string("Description.")), ("category", string("Category.")), ("company", string("Company.")), ("manager", string("Manager."))],
                    &["document_id"],
                )
            },
            run: set_document_properties,
        },
    ]
}

fn add_animation(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::{Animation, AnimationClass, AnimationLook, AnimationSide, AnimationStart};
    only_known(args, &["document_id", "slide", "shape", "effect", "direction", "class", "start", "delay", "duration"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let side = match args.get("direction").and_then(Value::as_str) {
        None => None,
        Some("from_left") => Some(AnimationSide::Left),
        Some("from_right") => Some(AnimationSide::Right),
        Some("from_top") => Some(AnimationSide::Top),
        Some("from_bottom") => Some(AnimationSide::Bottom),
        Some(_) => return Err(ToolError::invalid("direction is from_left, from_right, from_top or from_bottom")),
    };
    let look = match args.str_arg("effect")? {
        "appear" => AnimationLook::Appear,
        "fade" => AnimationLook::Fade,
        "fly" => AnimationLook::Fly(side.unwrap_or(AnimationSide::Bottom)),
        "wipe" => AnimationLook::Wipe(side.unwrap_or(AnimationSide::Left)),
        "zoom" => AnimationLook::Zoom,
        "spin" => AnimationLook::Spin(21_600_000),
        _ => return Err(ToolError::invalid("effect is appear, fade, fly, wipe, zoom or spin")),
    };
    let class = match args.opt_str("class")? {
        None | Some("entrance") => AnimationClass::Entrance,
        Some("emphasis") => AnimationClass::Emphasis,
        Some("exit") => AnimationClass::Exit,
        Some(_) => return Err(ToolError::invalid("class is entrance, emphasis or exit")),
    };
    let start = match args.opt_str("start")? {
        None | Some("on_click") => AnimationStart::OnClick,
        Some("with_previous") => AnimationStart::WithPrevious,
        Some("after_previous") => AnimationStart::AfterPrevious,
        Some(_) => return Err(ToolError::invalid("start is on_click, with_previous or after_previous")),
    };
    let seconds = |name: &str| -> Result<Option<u32>, ToolError> {
        match args.get(name) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v.as_f64().filter(|s| s.is_finite() && (0.0..=3600.0).contains(s)).map(|s| Some((s * 1000.0).round() as u32)).ok_or_else(|| ToolError::invalid(format!("{name} is seconds, 0 to 3600"))),
        }
    };
    let (delay, duration) = (seconds("delay")?, seconds("duration")?);
    let open = service.open_document(id)?;
    let (slide, shape) = shape_id(open, number, args.usize_arg("shape")? as u64)?;
    let mut animation = Animation::effect(shape, class, look);
    animation.start = start;
    if let Some(d) = delay {
        animation.delay = d;
    }
    if let Some(d) = duration {
        animation.duration = d.max(1);
    }
    let mut animations = slide_at(presentation_of(open)?, number)?.animations.clone();
    animations.push(animation);
    let index = animations.len() - 1;
    pptx_edit(service, id, vec![Command::SetAnimations { slide, animations }], json!({ "index": index }))
}

fn add_comment(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::slide_comment::{Author, SlideComment, guid_from_seed};
    only_known(args, &["document_id", "slide", "text", "author", "shape", "position"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let text = args.str_arg("text")?.to_string();
    if text.trim().is_empty() {
        return Err(ToolError::invalid("text is not empty"));
    }
    let name = match args.opt_str("author")? {
        Some(a) if !a.trim().is_empty() => a.trim().to_string(),
        // As the document tools' comments: the author is "Assistant", never the user's own name (it would be written into the file).
        _ => "Assistant".to_string(),
    };
    let initials: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase();
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let shape = match args.get("shape") {
        None | Some(Value::Null) => None,
        Some(v) => Some(shape_id(open, number, v.as_u64().ok_or_else(|| ToolError::invalid("shape is a shape_id"))?)?.1),
    };
    let emu = |pt: f64| (pt * 12700.0).round() as i64;
    let position = match args.get("position") {
        Some(p) => {
            let num = |k: &str| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| ToolError::invalid(format!("position.{k} is a number of points")));
            Some(ibis_doc_model::EmuPoint::new(emu(num("x")?), emu(num("y")?)))
        }
        None => shape.is_none().then(|| ibis_doc_model::EmuPoint::new(0, 0)),
    };
    let mut comments = slide_at(presentation_of(open)?, number)?.comments.clone();
    let seed = ((crate::clock::unix_seconds() as u64) << 20) ^ comments.len() as u64 ^ text.len() as u64;
    comments.push(SlideComment { id: guid_from_seed(seed), author: Author { name, initials }, created: crate::clock::now_utc().trim_end_matches('Z').to_string(), text, position, shape, resolved: false, replies: Vec::new() });
    let index = comments.len() - 1;
    pptx_edit(service, id, vec![Command::SetComments { slide, comments }], json!({ "index": index }))
}

fn align_shapes(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::shape_arrange::{self, AlignEdge, Arranged};
    only_known(args, &["document_id", "slide", "shapes", "edge", "relative_to"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let list = shape_list(args)?;
    let to_slide = match args.opt_str("relative_to")? {
        None | Some("selection") => false,
        Some("slide") => true,
        Some(_) => return Err(ToolError::invalid("relative_to is selection or slide")),
    };
    let edge = args.str_arg("edge")?.to_string();
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let sl = slide_at(presentation_of(open)?, number)?;
    let mut shapes: Vec<Arranged> = Vec::new();
    for item in list {
        let (_, shape) = shape_id(open, number, item)?;
        shapes.push(Arranged::of(sl.shapes.iter().find(|s| s.id == shape).ok_or_else(|| ToolError::new("no_such_shape", "shape"))?));
    }
    let size = presentation_of(open)?.size_of(sl);
    let commands = match edge.as_str() {
        "distribute_horizontally" | "distribute_vertically" => {
            if shapes.len() < 3 {
                return Err(ToolError::invalid("distributing takes three or more shapes"));
            }
            let horizontal = edge == "distribute_horizontally";
            shape_arrange::distribute(slide, &shapes, horizontal, if horizontal { size.width } else { size.height }, to_slide)
        }
        other => {
            let edge = match other {
                "left" => AlignEdge::Left,
                "center" => AlignEdge::Center,
                "right" => AlignEdge::Right,
                "top" => AlignEdge::Top,
                "middle" => AlignEdge::Middle,
                "bottom" => AlignEdge::Bottom,
                _ => return Err(ToolError::invalid("edge is left, center, right, top, middle, bottom, distribute_horizontally or distribute_vertically")),
            };
            shape_arrange::align(slide, &shapes, edge, ibis_doc_model::EmuRect::new(0, 0, size.width, size.height), to_slide)
        }
    };
    if commands.is_empty() {
        return Ok(crate::edit::edited(id, open, json!({}), vec![json!({ "code": "no_change", "message": "the shapes are already there" })]));
    }
    pptx_edit(service, id, commands, json!({}))
}

fn insert_chart(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_formats::NewChartKind;
    only_known(args, &["document_id", "slide", "kind", "frame", "data", "series_in", "title", "legend"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let kind_name = args.str_arg("kind")?;
    let kind = crate::tools_docx_layout::CHART_KINDS.iter().find(|k| k.0 == kind_name).map(|k| k.1).ok_or_else(|| ToolError::invalid(format!("kind {kind_name} is not one of {}", crate::tools_docx_layout::CHART_KINDS.iter().map(|k| k.0).collect::<Vec<_>>().join(", "))))?;
    let series_in = match args.opt_str("series_in")? {
        None => None,
        Some("columns") => Some(true),
        Some("rows") => Some(false),
        Some(_) => return Err(ToolError::invalid("series_in is columns or rows")),
    };
    let rows: Option<Vec<Vec<Value>>> = match args.get("data") {
        None | Some(Value::Null) => None,
        Some(Value::Array(list)) => {
            let rows: Vec<Vec<Value>> = list.iter().map(|r| r.as_array().cloned().ok_or_else(|| ToolError::invalid("data is rows (arrays) of cells"))).collect::<Result<_, _>>()?;
            if rows.len() < 2 || rows.iter().any(|r| r.len() < 2) {
                return Err(ToolError::invalid("data is at least 2 rows of at least 2 cells"));
            }
            if rows.len() > 1_000 || rows.iter().any(|r| r.len() > 100) {
                return Err(ToolError::new("out_of_range", "data is at most 1000 rows by 100 columns"));
            }
            Some(rows)
        }
        Some(_) => return Err(ToolError::invalid("data is rows (arrays) of cells")),
    };
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let size = presentation_of(open)?.size_of(slide_at(presentation_of(open)?, number)?);
    let emu = |pt: f64| (pt * 12700.0).round() as i64;
    let frame = match args.get("frame") {
        Some(f) => {
            let num = |k: &str| f.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).ok_or_else(|| ToolError::invalid(format!("frame.{k} is a number of points")));
            let (w, h) = (num("width")?, num("height")?);
            if w <= 0.0 || h <= 0.0 {
                return Err(ToolError::invalid("frame.width and frame.height are above 0"));
            }
            ibis_doc_model::EmuRect::new(emu(num("x")?), emu(num("y")?), emu(w), emu(h))
        }
        None => {
            let (w, h) = (size.width * 3 / 4, size.height * 3 / 4);
            ibis_doc_model::EmuRect::new((size.width - w) / 2, (size.height - h) / 2, w, h)
        }
    };
    let shape_n = open.document.ids().new_shape_id();
    let current = open.document.presentation().clone();
    let mut shape = open.source.new_chart_kind(&current, slide, shape_n, frame, kind).map_err(|e| ToolError::new("edit_rejected", e.to_string()))?;
    let ShapeKind::Chart(sample) = &shape.kind else { return Err(ToolError::new("edit_rejected", "the new chart is not a chart")) };
    let mut chart: ibis_doc_model::Chart = (**sample).clone();
    let scatter = matches!(kind, NewChartKind::Scatter | NewChartKind::ScatterSmooth | NewChartKind::ScatterLines);
    let mut kept = None;
    if let Some(rows) = &rows {
        let (made, data) = crate::tools_docx_layout::workbook_of(rows, scatter, series_in, &chart)?;
        chart = made;
        kept = Some(data);
    }
    if let Some(t) = args.get("title") {
        crate::tools_xlsx_chart::title_of(&mut chart, t)?;
    }
    if let Some(l) = args.get("legend") {
        crate::tools_xlsx_chart::legend_of(&mut chart, l)?;
    }
    // The data is kept for the chart saving writes (as the window's data panel does); the chart goes in as it is made.
    if let Some(data) = kept {
        open.source.set_chart_data(shape_n, chart.clone(), data);
    }
    shape.kind = ShapeKind::Chart(Box::new(chart));
    let index = slide_at(presentation_of(open)?, number)?.shapes.len();
    pptx_edit(service, id, vec![Command::InsertShape { slide, shapes: vec![(index, shape)] }], json!({ "shape": shape_n.0 }))
}

fn set_chart_data(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use crate::tools_docx_layout::{chart_rows, is_scatter, series_in_arg, workbook_of};
    only_known(args, &["document_id", "slide", "shape", "data", "series_in"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let rows = chart_rows(args)?;
    let series_in = series_in_arg(args)?;
    let open = service.open_document(id)?;
    let (slide, shape) = shape_id(open, number, args.usize_arg("shape")? as u64)?;
    let chart = match slide_at(presentation_of(open)?, number)?.shapes.iter().find(|s| s.id == shape).map(|s| &s.kind) {
        Some(ShapeKind::Chart(c)) => (**c).clone(),
        _ => return Err(ToolError::new("unsupported_operation", "the shape is not a chart")),
    };
    if chart.ex.is_some() {
        return Err(ToolError::new("unsupported_operation", "the data of this kind of chart is not changed by this tool"));
    }
    let (target, data) = workbook_of(&rows, is_scatter(&chart), series_in, &chart)?;
    // The chart as the file reads it back with these series, as the window's data panel does when the shape of the data changes.
    open.source.set_chart_data(shape, target.clone(), data.clone());
    let mut changed = presentation_of(open)?.clone();
    if let Some(t) = changed.slides.iter_mut().find(|s| s.id == slide).and_then(|s| s.shapes.iter_mut().find(|x| x.id == shape)) {
        t.kind = ShapeKind::Chart(Box::new(target));
    }
    let read = open.source.as_read(&changed, slide, shape).map_err(|e| ToolError::new("edit_rejected", e.to_string()))?;
    if let ShapeKind::Chart(c) = &read.kind {
        open.source.set_chart_data(shape, (**c).clone(), data);
    }
    pptx_edit(service, id, vec![Command::SetShapeKind { slide, shape, kind: Box::new(read.kind) }], json!({ "shape": shape.0 }))
}

fn list_charts(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let number = args.usize_arg("slide")?;
    let open = service.open_document(args.str_arg("document_id")?)?;
    let sl = slide_at(presentation_of(open)?, number)?;
    let pt = |e: i64| e as f64 / 12700.0;
    let charts: Vec<Value> = sl
        .shapes
        .iter()
        .filter_map(|s| match &s.kind {
            ShapeKind::Chart(c) => {
                let series: Vec<Value> = c.groups.iter().flat_map(|g| g.series.iter()).map(|x| json!({ "name": x.name, "values": x.values.len() })).collect();
                Some(json!({ "shape_id": s.id.0, "x": pt(s.frame.x), "y": pt(s.frame.y), "width": pt(s.frame.width), "height": pt(s.frame.height), "series": series }))
            }
            _ => None,
        })
        .collect();
    Ok(json!({ "slide": number, "charts": charts }))
}

fn insert_connector(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::connector::{frame_between, sites};
    only_known(args, &["document_id", "slide", "start", "end", "kind", "arrow"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let kind = args.opt_str("kind")?.unwrap_or("straight");
    let arrow = args.opt_str("arrow")?.unwrap_or("none");
    if !["straight", "elbow", "curved"].contains(&kind) {
        return Err(ToolError::invalid("kind is straight, elbow or curved"));
    }
    if !["none", "end", "start", "both"].contains(&arrow) {
        return Err(ToolError::invalid("arrow is none, end, start or both"));
    }
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let shapes = &slide_at(presentation_of(open)?, number)?.shapes;
    let index = shapes.len();
    // One end: a point and what it is glued to.
    let resolve = |name: &str| -> Result<((i64, i64), Option<(u64, u32)>), ToolError> {
        let e = args.get(name).filter(|v| v.is_object()).ok_or_else(|| ToolError::invalid(format!("{name} is {{ shape, site }} or {{ x, y }}")))?;
        if let Some(sid) = e.get("shape") {
            let sid = sid.as_u64().ok_or_else(|| ToolError::invalid(format!("{name}.shape is a shape_id")))?;
            let site = e.get("site").and_then(Value::as_u64).ok_or_else(|| ToolError::invalid(format!("{name}.site is the number of a connection site, from 0")))? as u32;
            let target = shapes.iter().find(|s| s.id.0 == sid).ok_or_else(|| ToolError::new("no_such_shape", format!("{name}.shape {sid}")))?;
            let all = sites(target);
            let point = all.get(site as usize).copied().ok_or_else(|| {
                if all.is_empty() {
                    ToolError::new("unsupported_operation", format!("shape {sid} has no connection sites that can be found (a turned shape, a picture, a table...)"))
                } else {
                    ToolError::new("out_of_range", format!("{name}.site {site}; the shape has {} sites", all.len()))
                }
            })?;
            Ok((point, Some((sid, site))))
        } else {
            let num = |k: &str| e.get(k).and_then(Value::as_f64).filter(|v| v.is_finite() && v.abs() < 100_000.0).map(|p| (p * 12700.0).round() as i64).ok_or_else(|| ToolError::invalid(format!("{name}.{k} is a number of points")));
            Ok(((num("x")?, num("y")?), None))
        }
    };
    let ((start, glue_start), (end, glue_end)) = (resolve("start")?, resolve("end")?);
    let (frame, flip_h, flip_v) = frame_between(start, end);
    let outline = crate::tools_pptx::theme_colour(open, "accent1", -0.25)?;
    let mut stroke = Stroke::solid(outline, 12_700);
    let tip = LineEnd::Triangle(EndSize { width: 1, length: 1 });
    if matches!(arrow, "end" | "both") {
        stroke.tail = tip;
    }
    if matches!(arrow, "start" | "both") {
        stroke.head = tip;
    }
    let shape_n = open.document.ids().new_shape_id();
    let mut shape = match kind {
        "straight" => ibis_doc_model::Shape::new(shape_n, frame, ShapeKind::Line(stroke)),
        _ => {
            let name = if kind == "elbow" { "bentConnector3" } else { "curvedConnector3" };
            let geometry = ibis_formats::geometry::preset(name).ok_or_else(|| ToolError::invalid(name))?;
            ibis_doc_model::Shape::new(shape_n, frame, ShapeKind::Auto(ibis_doc_model::AutoShape::new(geometry, ibis_doc_model::Fill::None, Some(stroke))))
        }
    };
    shape.flip_h = flip_h;
    shape.flip_v = flip_v;
    let mut commands = vec![Command::InsertShape { slide, shapes: vec![(index, shape)] }];
    if glue_start.is_some() || glue_end.is_some() {
        commands.push(Command::SetShapeGlue { slide, shape: shape_n, glue: Some(ibis_doc_model::Glue { start: glue_start, end: glue_end }) });
    }
    pptx_edit(service, id, commands, json!({ "shape": shape_n.0 }))
}

fn set_theme(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "colors", "fonts", "theme"])?;
    let from_file = match args.get("theme") {
        Some(theme) => {
            let path = theme.as_str().ok_or_else(|| ToolError::invalid("theme is the path of a pptx or potx whose colours and typefaces are taken"))?;
            Some(theme_of_file(service, path)?)
        }
        None => None,
    };
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let presentation = presentation_of(open)?;
    let mut colors = presentation.theme_colors.clone();
    let mut fonts = presentation.theme_fonts.clone();
    let mut any = false;
    if let Some((from_colors, from_fonts)) = from_file {
        for (name, rgb) in from_colors {
            match colors.iter_mut().find(|(n, _)| *n == name) {
                Some(entry) => entry.1 = rgb,
                None => colors.push((name, rgb)),
            }
        }
        if from_fonts.is_some() {
            fonts = from_fonts;
        }
        any = true;
    }
    if let Some(c) = args.get("colors") {
        let map = c.as_object().ok_or_else(|| ToolError::invalid("colors is an object of scheme names to \"#RRGGBB\""))?;
        for (name, value) in map {
            if !["dk1", "lt1", "dk2", "lt2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hlink", "folHlink"].contains(&name.as_str()) {
                return Err(ToolError::invalid(format!("{name} is not a scheme colour name")));
            }
            let rgb = match parse_color("colors", value)? {
                ColorArg::Rgb(rgb) => rgb,
                _ => return Err(ToolError::invalid("a scheme colour is \"#RRGGBB\"")),
            };
            match colors.iter_mut().find(|(n, _)| n == name) {
                Some(entry) => entry.1 = rgb,
                None => colors.push((name.clone(), rgb)),
            }
            any = true;
        }
    }
    if let Some(f) = args.get("fonts") {
        let mut theme = fonts.take().ok_or_else(|| ToolError::new("unsupported_operation", "this presentation has no theme typefaces"))?;
        for (key, set) in [("major", &mut theme.major), ("minor", &mut theme.minor)] {
            let Some(o) = f.get(key) else { continue };
            for (field, slot) in [("latin", &mut set.latin), ("east_asian", &mut set.east_asian)] {
                if let Some(v) = o.get(field) {
                    *slot = Some(v.as_str().ok_or_else(|| ToolError::invalid(format!("fonts.{key}.{field} is a typeface name")))?.to_string());
                    any = true;
                }
            }
        }
        fonts = Some(theme);
    }
    if !any {
        return Err(ToolError::invalid("no colour or typeface is given"));
    }
    // As the theme panel does: the shapes, backgrounds and layouts that follow the theme are drawn again in the new colours.
    let commands = open.source.retheme(presentation_of(open)?, colors, fonts).map_err(|e| ToolError::new("edit_rejected", e.to_string()))?;
    pptx_edit(service, id, commands, json!({}))
}

/// The theme colours and typefaces of the pptx or potx at `path` (a file in an allowed folder), as the theme panel takes them from another file.
fn theme_of_file(service: &Service, path: &str) -> Result<(Vec<(String, ibis_doc_model::Rgb)>, Option<ibis_doc_model::ThemeFonts>), ToolError> {
    let real = crate::paths::resolve(&service.allow_dirs, path, &["pptx", "pptm", "potx", "potm", "ppsx"], true)?;
    let data = std::fs::read(&real).map_err(|e| ToolError::new("file_not_found", e.to_string()))?;
    if data.len() as u64 > service.limits.max_file_size {
        return Err(ToolError::new("file_too_large", format!("{} bytes; the limit is {}", data.len(), service.limits.max_file_size)));
    }
    let opened = ibis_odf::open(data).map_err(|e| ToolError::new("unsupported_format", e.to_string()))?;
    Ok((opened.presentation.theme_colors.clone(), opened.presentation.theme_fonts.clone()))
}

fn insert_table(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "rows", "columns", "frame", "cells"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let rows = args.usize_arg("rows")?;
    let columns = args.usize_arg("columns")?;
    if !(1..=100).contains(&rows) || !(1..=50).contains(&columns) {
        return Err(ToolError::invalid("rows is 1 to 100 and columns is 1 to 50"));
    }
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let slide_width = presentation_of(open)?.slide_size.width;
    let emu = |pt: f64| (pt * 12700.0).round() as i64;
    let (x, y, w, h) = match args.get("frame") {
        Some(f) => {
            let num = |k: &str, d: f64| f.get(k).map_or(Ok(d), |v| v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| ToolError::invalid(format!("frame.{k} is a number of points"))));
            (emu(num("x", 36.0)?), emu(num("y", 108.0)?), emu(num("width", 0.0)?), emu(num("height", 0.0)?))
        }
        None => (emu(36.0), emu(108.0), 0, 0),
    };
    let width = if w > 0 { w } else { (slide_width - 2 * emu(36.0)).max(emu(72.0)) };
    let height = if h > 0 { h } else { emu(30.0) * rows as i64 };
    let mut table = ibis_doc_model::table::Table::new_grid(rows, columns, width, height);
    if let Some(cells) = args.get("cells") {
        let list = cells.as_array().ok_or_else(|| ToolError::invalid("cells is a list of rows of texts"))?;
        for (r, row) in list.iter().enumerate().take(rows) {
            let texts = row.as_array().ok_or_else(|| ToolError::invalid("cells is a list of rows of texts"))?;
            for (c, text) in texts.iter().enumerate().take(columns) {
                let text = text.as_str().ok_or_else(|| ToolError::invalid("a cell is text"))?;
                table.rows[r].cells[c].text.set_text(text);
            }
        }
    }
    let shape_n = open.document.ids().new_shape_id();
    let index = slide_at(presentation_of(open)?, number)?.shapes.len();
    let shape = ibis_doc_model::Shape::new(shape_n, ibis_doc_model::EmuRect::new(x, y, width, height), ShapeKind::Table(Box::new(table)));
    pptx_edit(service, id, vec![Command::InsertShape { slide, shapes: vec![(index, shape)] }], json!({ "shape": shape_n.0 }))
}

fn set_table_cell(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "shape", "row", "column", "text", "fill"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let (row, column) = (args.usize_arg("row")?, args.usize_arg("column")?);
    let open = service.open_document(id)?;
    let (slide, shape) = shape_id(open, number, args.usize_arg("shape")? as u64)?;
    let ShapeKind::Table(table) = slide_at(presentation_of(open)?, number)?.shapes.iter().find(|s| s.id == shape).map(|s| s.kind.clone()).ok_or_else(|| ToolError::new("no_such_shape", "shape"))? else {
        return Err(ToolError::new("unsupported_operation", "the shape is not a table"));
    };
    if table.rows.get(row).and_then(|r| r.cells.get(column)).is_none() {
        return Err(ToolError::new("out_of_range", format!("cell {row},{column}; the table has {} rows and {} columns", table.rows.len(), table.columns.len())));
    }
    let mut commands = Vec::new();
    if let Some(v) = args.get("fill") {
        let fill = match parse_color("fill", v)? {
            ColorArg::Rgb(rgb) => ibis_doc_model::Fill::Solid(Color::opaque(rgb)),
            ColorArg::Theme { name, brightness } => ibis_doc_model::Fill::Solid(crate::tools_pptx::theme_colour(open, &name, brightness)?),
            ColorArg::Inherit => ibis_doc_model::Fill::None,
        };
        let mut changed = (*table).clone();
        changed.rows[row].cells[column].fill = fill;
        commands.push(Command::SetShapeKind { slide, shape, kind: Box::new(ShapeKind::Table(Box::new(changed))) });
    }
    if let Some(v) = args.get("text") {
        let text = v.as_str().ok_or_else(|| ToolError::invalid("text is a string"))?.to_string();
        commands.push(Command::InCell { row, column, command: Box::new(Command::SetText { slide, shape, text }) });
    }
    if commands.is_empty() {
        return Err(ToolError::invalid("no cell property is given (text or fill)"));
    }
    pptx_edit(service, id, commands, json!({}))
}

fn set_slide_properties(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "hidden", "name", "background", "layout"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let sl = slide_at(presentation_of(open)?, number)?.clone();
    let mut commands = Vec::new();
    if let Some(v) = args.get("layout") {
        let text = match v {
            Value::String(t) => t.clone(),
            Value::Number(n) => n.to_string(),
            _ => return Err(ToolError::invalid("layout is a number from 1 or a name (pptx_list_layouts)")),
        };
        let current = presentation_of(open)?.clone();
        if current.layouts.is_empty() {
            return Err(ToolError::new("unsupported_operation", "the presentation has no layouts"));
        }
        let layout_id = crate::tools_pptx::find_layout(&current, Some(&text))?.id;
        let mut spare = {
            let ids = open.document.ids();
            (0..64).map(|_| ids.new_shape_id()).collect::<Vec<_>>().into_iter()
        };
        let new = open.source.with_layout_ids(&current, slide, layout_id, &mut || spare.next().unwrap_or(ShapeId(u64::MAX))).map_err(|e| ToolError::new("edit_rejected", e.to_string()))?;
        if Some(&new) != current.slide(slide) {
            commands.push(Command::SetSlide { slide: new });
        }
    }
    if let Some(v) = args.get("hidden") {
        let hidden = parse_bool("hidden", v)?;
        let mut copy = sl.clone();
        copy.hidden = hidden;
        commands.push(Command::SetSlide { slide: copy });
    }
    if let Some(v) = args.get("name") {
        let name = match v {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            _ => return Err(ToolError::invalid("name is text or null")),
        };
        commands.push(Command::SetSlideName { slide, name });
    }
    if let Some(v) = args.get("background") {
        let fill = match parse_color("background", v)? {
            ColorArg::Rgb(rgb) => ibis_doc_model::Fill::Solid(Color::opaque(rgb)),
            ColorArg::Theme { name, brightness } => ibis_doc_model::Fill::Solid(crate::tools_pptx::theme_colour(open, &name, brightness)?),
            ColorArg::Inherit => match sl.layout.and_then(|l| presentation_of(open).ok()?.layout(l)) {
                Some(layout) => layout.background.clone(),
                None => return Err(ToolError::new("unsupported_operation", "the slide has no layout to take its background from")),
            },
        };
        commands.push(Command::SetBackground { slide, fill });
    }
    if commands.is_empty() {
        return Err(ToolError::invalid("no property is given (layout, hidden, name or background)"));
    }
    pptx_edit(service, id, commands, json!({}))
}

fn set_slide_size(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "width", "height", "preset", "scale_content"])?;
    let id = args.str_arg("document_id")?;
    if args.get("scale_content").is_some_and(|v| v.as_bool() != Some(false) && !v.is_null()) {
        return Err(ToolError::new("unsupported_operation", "scaling the content with the slide size is not supported"));
    }
    let size = match args.get("preset").and_then(Value::as_str) {
        Some("16:9") => ibis_doc_model::EmuSize::WIDESCREEN,
        Some("4:3") => ibis_doc_model::EmuSize::new(9_144_000, 6_858_000),
        Some("a4") => ibis_doc_model::EmuSize::new(10_692_000, 7_560_000),
        Some(_) => return Err(ToolError::invalid("preset is 16:9, 4:3 or a4")),
        None => {
            let (w, h) = (points_arg(args, "width")?, points_arg(args, "height")?);
            match (w, h) {
                (Some(w), Some(h)) => ibis_doc_model::EmuSize::new(w, h),
                _ => return Err(ToolError::invalid("give width and height (points) or a preset")),
            }
        }
    };
    let open = service.open_document(id)?;
    presentation_of(open)?;
    pptx_edit(service, id, vec![Command::SetSlideSize { size }], json!({}))
}

fn set_transition(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::model::Transition;
    only_known(args, &["document_id", "slides", "effect", "direction", "duration", "advance_on_click", "advance_after"])?;
    let id = args.str_arg("document_id")?;
    let numbers: Vec<usize> = args
        .get("slides")
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty())
        .and_then(|l| l.iter().map(|v| v.as_u64().map(|n| n as usize)).collect::<Option<Vec<_>>>())
        .ok_or_else(|| ToolError::invalid("slides is a list of slide numbers"))?;
    let effect = match args.get("effect") {
        Some(Value::Null) => None,
        Some(Value::String(e)) => Some(ibis_doc_model::transition_catalog::effect_of(e).ok_or_else(|| ToolError::invalid(format!("unknown transition effect {e}")))?.id),
        _ => return Err(ToolError::invalid("effect is an effect id or null")),
    };
    let open = service.open_document(id)?;
    let mut commands = Vec::new();
    for n in numbers {
        let slide = slide_id(open, n)?;
        let Some(effect) = effect else {
            commands.push(Command::SetTransition { slide, transition: None });
            continue;
        };
        let current = slide_at(presentation_of(open)?, n)?.transition.clone();
        let mut t = current.filter(|t| t.effect == effect).unwrap_or_else(|| Transition::new(effect));
        t.effect = effect.to_string();
        if let Some(d) = args.get("direction") {
            t.direction = match d {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => return Err(ToolError::invalid("direction is text or null")),
            };
        }
        if let Some(v) = args.get("duration") {
            let sec = v.as_f64().filter(|s| s.is_finite() && (0.0..=60.0).contains(s)).ok_or_else(|| ToolError::invalid("duration is seconds, 0 to 60"))?;
            t.duration = (sec * 1000.0).round() as u32;
        }
        if let Some(v) = args.get("advance_on_click") {
            t.on_click = parse_bool("advance_on_click", v)?;
        }
        if let Some(v) = args.get("advance_after") {
            t.after = match v {
                Value::Null => None,
                v => Some((v.as_f64().filter(|s| s.is_finite() && (0.0..=86_400.0).contains(s)).ok_or_else(|| ToolError::invalid("advance_after is seconds or null"))? * 1000.0).round() as u32),
            };
        }
        commands.push(Command::SetTransition { slide, transition: Some(t) });
    }
    pptx_edit(service, id, commands, json!({}))
}

fn delete_animation(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "index"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let mut animations = slide_at(presentation_of(open)?, number)?.animations.clone();
    match args.get("index") {
        None | Some(Value::Null) => animations.clear(),
        Some(list) => {
            let mut indexes = list.as_array().and_then(|l| l.iter().map(|v| v.as_u64().map(|n| n as usize)).collect::<Option<Vec<_>>>()).ok_or_else(|| ToolError::invalid("index is a list of positions"))?;
            indexes.sort_unstable();
            indexes.dedup();
            if let Some(bad) = indexes.iter().find(|i| **i >= animations.len()) {
                return Err(ToolError::new("out_of_range", format!("animation {bad}; the slide has {}", animations.len())));
            }
            for i in indexes.into_iter().rev() {
                animations.remove(i);
            }
        }
    }
    pptx_edit(service, id, vec![Command::SetAnimations { slide, animations }], json!({}))
}

fn set_header_footer(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "date", "slide_number", "footer", "hide_on_title"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let mut marks = presentation_of(open)?.header_footer;
    let mut any = false;
    for (key, slot) in [("date", &mut marks.date), ("slide_number", &mut marks.number), ("footer", &mut marks.footer)] {
        if let Some(v) = args.get(key) {
            *slot = parse_bool(key, v)?;
            any = true;
        }
    }
    if let Some(v) = args.get("hide_on_title") {
        marks.on_title_slides = !parse_bool("hide_on_title", v)?;
        any = true;
    }
    if !any {
        return Err(ToolError::invalid("no setting is given"));
    }
    pptx_edit(service, id, vec![Command::SetHeaderFooterMarks { marks }], json!({}))
}

fn set_document_properties(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "title", "subject", "author", "keywords", "description", "category", "company", "manager"])?;
    let id = args.str_arg("document_id")?;
    let open = service.open_document(id)?;
    let mut props = presentation_of(open)?.properties.clone();
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
    pptx_edit(service, id, vec![Command::SetDocumentProperties { properties: props }], json!({}))
}

fn copy_shape(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "shapes", "to_slide", "offset"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let list = shape_list(args)?;
    let to_number = args.opt_usize("to_slide")?.unwrap_or(number);
    let offset = match args.get("offset") {
        None => if to_number == number { Some(10.0) } else { Some(0.0) },
        Some(v) => Some(v.as_f64().filter(|d| d.is_finite()).ok_or_else(|| ToolError::invalid("offset is a number of points"))?),
    }
    .map(|pt| (pt * 12700.0).round() as i64)
    .unwrap_or(0);
    let open = service.open_document(id)?;
    let target = slide_id(open, to_number)?;
    let mut copies = Vec::new();
    let mut chosen = Vec::new();
    for item in list {
        let (_, shape) = shape_id(open, number, item)?;
        let sl = slide_at(presentation_of(open)?, number)?;
        let original = sl.shapes.iter().find(|s| s.id == shape).ok_or_else(|| ToolError::new("no_such_shape", "shape"))?.clone();
        if sl.is_attached(shape) || !sl.attached_to(shape).is_empty() || sl.placeholders.contains_key(&shape) || !matches!(original.kind, ShapeKind::TextBox(_) | ShapeKind::Auto(_) | ShapeKind::Picture(_) | ShapeKind::Line(_) | ShapeKind::Table(_)) {
            return Err(ToolError::new("unsupported_operation", format!("shape {item} (a placeholder, a chart or another kind) cannot be copied by this tool")));
        }
        chosen.push(shape);
        copies.push(original);
    }
    // A shape of a group is copied with the group when the whole group is chosen (as the screen does), not alone.
    let source = slide_at(presentation_of(open)?, number)?;
    let groups = ibis_doc_model::shape_group::groups_of(&source.groups, &chosen);
    let mut in_group = std::collections::HashSet::new();
    for g in &source.groups {
        let mut l = Vec::new();
        ibis_doc_model::shape_group::leaves(g, &mut l);
        in_group.extend(l);
    }
    let mut kept = std::collections::HashSet::new();
    for g in &groups {
        let mut l = Vec::new();
        ibis_doc_model::shape_group::leaves(g, &mut l);
        kept.extend(l);
    }
    if let Some(alone) = chosen.iter().find(|s| in_group.contains(s) && !kept.contains(s)) {
        return Err(ToolError::new("unsupported_operation", format!("shape {} is in a group: choose the whole group to copy it", alone.0)));
    }
    let slide_number = open.document.presentation().slide_index(target).unwrap_or(0) + open.document.presentation().first_slide_number;
    let mut shapes = Vec::new();
    let start = slide_at(presentation_of(open)?, to_number)?.shapes.len();
    let mut new_ids = Vec::new();
    let mut id_map = std::collections::HashMap::new();
    for mut copy in copies {
        let new_id = open.document.ids().new_shape_id();
        id_map.insert(copy.id, new_id);
        copy.id = new_id;
        copy.frame = copy.frame.translated(offset, offset);
        if let Some(body) = copy.text_body_mut() {
            body.set_slide_number(slide_number);
        }
        new_ids.push(new_id.0);
        shapes.push((start + shapes.len(), copy));
    }
    let mut commands = vec![Command::InsertShape { slide: target, shapes }];
    if !groups.is_empty() {
        let mut all = slide_at(presentation_of(open)?, to_number)?.groups.clone();
        let mut used = ibis_doc_model::shape_group::native_ids(&all);
        all.extend(groups.iter().map(|g| ibis_doc_model::shape_group::copy_group(g, &id_map, &mut used)));
        commands.push(Command::SetShapeGroups { slide: target, groups: all });
    }
    pptx_edit(service, id, commands, json!({ "shapes": new_ids }))
}

fn group_shapes(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "shapes"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let list = shape_list(args)?;
    if list.len() < 2 {
        return Err(ToolError::invalid("shapes has two or more shape_ids"));
    }
    let open = service.open_document(id)?;
    let mut chosen = Vec::new();
    for item in &list {
        chosen.push(shape_id(open, number, *item)?.1);
    }
    chosen.dedup();
    let sl = slide_at(presentation_of(open)?, number)?;
    // As the screen groups: a group that holds a chosen shape joins whole (a group inside the new one).
    let grouping = ibis_doc_model::shape_group::group(sl, &chosen, |n| format!("Group {n}")).ok_or_else(|| ToolError::invalid("shapes has two or more things to group"))?;
    pptx_edit(service, id, grouping.commands, json!({ "group": grouping.native_id }))
}

fn ungroup_shapes(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::{GroupMember, ShapeGroup};
    only_known(args, &["document_id", "slide", "group"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let wanted = args.str_arg("group")?.to_string();
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let sl = slide_at(presentation_of(open)?, number)?;
    // A top-level group is replaced by the groups inside it.
    let mut found = false;
    let mut groups: Vec<ShapeGroup> = Vec::new();
    for g in &sl.groups {
        if g.native_id == wanted {
            found = true;
            for c in &g.children {
                if let GroupMember::Group(inner) = c {
                    groups.push((**inner).clone());
                }
            }
        } else {
            groups.push(g.clone());
        }
    }
    if !found {
        return Err(ToolError::new("unsupported_operation", format!("no top-level group {wanted} on slide {number}")));
    }
    pptx_edit(service, id, vec![Command::SetShapeGroups { slide, groups }], json!({}))
}

fn copy_slide(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "index"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let open = service.open_document(id)?;
    let original = slide_id(open, number)?;
    let count = presentation_of(open)?.slides.len();
    let index = args.opt_usize("index")?.unwrap_or(number);
    if index > count {
        return Err(ToolError::new("out_of_range", format!("index {index}; the presentation has {count} slides")));
    }
    let copy_id = open.document.ids().new_slide_id();
    let copy = presentation_of(open)?.duplicate_slide(original, copy_id).ok_or_else(|| ToolError::new("no_such_shape", "slide"))?;
    pptx_edit(service, id, vec![Command::InsertSlide { index, slide: copy }], json!({ "slide": index + 1 }))
}

fn points_arg(args: &Value, name: &str) -> Result<Option<i64>, ToolError> {
    match args.get(name) {
        None => Ok(None),
        Some(v) => v.as_f64().filter(|p| p.is_finite() && p.abs() < 100_000.0).map(|p| Some((p * 12700.0).round() as i64)).ok_or_else(|| ToolError::invalid(format!("{name} is a number of points"))),
    }
}

fn set_paragraph_format(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::text::{LineSpacing, ListKind, ParagraphFormat, TextAlign};
    only_known(args, &["document_id", "slide", "shape", "paragraphs", "alignment", "indent_left", "indent_first_line", "space_before", "space_after", "line_spacing", "level", "bullet"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let mut format = ParagraphFormat::default();
    let mut any = false;
    if let Some(v) = args.get("alignment") {
        format.align = Some(match v.as_str() {
            Some("left") => TextAlign::Left,
            Some("center") => TextAlign::Center,
            Some("right") => TextAlign::Right,
            Some("justify") => TextAlign::Justify,
            Some("distributed") => TextAlign::Distributed,
            _ => return Err(ToolError::invalid("alignment is left, center, right, justify or distributed")),
        });
        any = true;
    }
    if let Some(e) = points_arg(args, "indent_left")? {
        format.margin_left = Some(e.max(0));
        any = true;
    }
    if let Some(e) = points_arg(args, "indent_first_line")? {
        format.indent = Some(e);
        any = true;
    }
    // Spacing is in hundredths of a point.
    for (name, slot) in [("space_before", &mut format.space_before), ("space_after", &mut format.space_after)] {
        if let Some(e) = points_arg(args, name)? {
            *slot = Some((e.max(0) / 127) as u32);
            any = true;
        }
    }
    if let Some(v) = args.get("line_spacing") {
        let spacing = if let Some(m) = v.as_f64() {
            LineSpacing::Percent((m * 100_000.0).round().clamp(1.0, 1_000_000.0) as u32)
        } else if let Some(p) = v.get("points").and_then(Value::as_f64) {
            LineSpacing::Points((p * 100.0).round().clamp(1.0, 1_000_000.0) as u32)
        } else {
            return Err(ToolError::invalid("line_spacing is a multiple of the line or { points }"));
        };
        format.line_spacing = Some(Some(spacing));
        any = true;
    }
    if let Some(v) = args.get("level") {
        format.level = Some(v.as_u64().filter(|l| *l <= 8).ok_or_else(|| ToolError::invalid("level is 0 to 8"))? as u8);
        any = true;
    }
    if let Some(v) = args.get("bullet") {
        format.list = Some(match v.as_str() {
            Some("none") => ListKind::None,
            Some("bullet") => ListKind::Bullet,
            Some("numbered") => ListKind::Numbered,
            _ => return Err(ToolError::invalid("bullet is none, bullet or numbered")),
        });
        any = true;
    }
    if !any {
        return Err(ToolError::invalid("no paragraph format is given"));
    }
    let open = service.open_document(id)?;
    let (slide, shape) = shape_id(open, number, args.usize_arg("shape")? as u64)?;
    let body = slide_at(presentation_of(open)?, number)?.shapes.iter().find(|s| s.id == shape).and_then(|s| s.text_body()).ok_or_else(|| ToolError::new("unsupported_operation", "the shape has no text"))?;
    // Characters count over the paragraphs joined by one line break each.
    let lengths: Vec<usize> = body.paragraphs.iter().map(|p| p.runs.iter().map(|r| r.text.chars().count()).sum()).collect();
    let chosen: Vec<usize> = match args.get("paragraphs") {
        None | Some(Value::Null) => (0..lengths.len()).collect(),
        Some(list) => list.as_array().and_then(|l| l.iter().map(|v| v.as_u64().map(|n| n as usize)).collect::<Option<Vec<_>>>()).ok_or_else(|| ToolError::invalid("paragraphs is a list of numbers"))?,
    };
    let mut commands = Vec::new();
    for p in chosen {
        let length = *lengths.get(p).ok_or_else(|| ToolError::new("out_of_range", format!("paragraph {p}; the shape has {}", lengths.len())))?;
        let start: usize = lengths[..p].iter().map(|l| l + 1).sum();
        commands.push(Command::SetParagraphFormat { slide, shape, start, end: start + length, format: format.clone() });
    }
    pptx_edit(service, id, commands, json!({}))
}

fn set_text_frame(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    use ibis_doc_model::text::{TextAutofit, VerticalAnchor};
    only_known(args, &["document_id", "slide", "shape", "vertical_alignment", "margins", "wrap", "autofit", "columns"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let open = service.open_document(id)?;
    let (slide, shape) = shape_id(open, number, args.usize_arg("shape")? as u64)?;
    let mut body = slide_at(presentation_of(open)?, number)?.shapes.iter().find(|s| s.id == shape).and_then(|s| s.text_body()).cloned().ok_or_else(|| ToolError::new("unsupported_operation", "the shape has no text"))?;
    let mut any = false;
    if let Some(v) = args.get("vertical_alignment") {
        body.anchor = match v.as_str() {
            Some("top") => VerticalAnchor::Top,
            Some("middle") => VerticalAnchor::Middle,
            Some("bottom") => VerticalAnchor::Bottom,
            _ => return Err(ToolError::invalid("vertical_alignment is top, middle or bottom")),
        };
        any = true;
    }
    if let Some(m) = args.get("margins") {
        for (k, slot) in [("left", &mut body.insets.left), ("top", &mut body.insets.top), ("right", &mut body.insets.right), ("bottom", &mut body.insets.bottom)] {
            if let Some(e) = points_arg(m, k)? {
                *slot = e.max(0);
            }
        }
        any = true;
    }
    if let Some(v) = args.get("wrap") {
        body.wrap = parse_bool("wrap", v)?;
        any = true;
    }
    if let Some(v) = args.get("autofit") {
        body.autofit = match v.as_str() {
            Some("none") => TextAutofit::None,
            Some("shrink") => TextAutofit::Shrink,
            Some("resize_shape") => TextAutofit::ResizeShape,
            _ => return Err(ToolError::invalid("autofit is none, shrink or resize_shape")),
        };
        any = true;
    }
    if let Some(v) = args.get("columns") {
        body.columns = v.as_u64().filter(|c| (1..=16).contains(c)).ok_or_else(|| ToolError::invalid("columns is 1 to 16"))? as u32;
        any = true;
    }
    if !any {
        return Err(ToolError::invalid("no text frame property is given"));
    }
    pptx_edit(service, id, vec![Command::SetTextBody { slide, shape, body }], json!({}))
}

fn dash_of(name: &str) -> Result<LineDash, ToolError> {
    Ok(match name {
        "solid" => LineDash::Solid,
        "dot" => LineDash::Dot,
        "dash" => LineDash::Dash,
        "long_dash" => LineDash::LgDash,
        "dash_dot" => LineDash::DashDot,
        "long_dash_dot" => LineDash::LgDashDot,
        "long_dash_dot_dot" => LineDash::LgDashDotDot,
        _ => return Err(ToolError::invalid("dash is solid, dot, dash, long_dash, dash_dot, long_dash_dot or long_dash_dot_dot")),
    })
}

fn end_of(name: &str) -> Result<LineEnd, ToolError> {
    let size = EndSize { width: 1, length: 1 };
    Ok(match name {
        "none" => LineEnd::None,
        "triangle" => LineEnd::Triangle(size),
        "stealth" => LineEnd::Stealth(size),
        "arrow" => LineEnd::Arrow(size),
        "diamond" => LineEnd::Diamond(size),
        "oval" => LineEnd::Oval(size),
        _ => return Err(ToolError::invalid("an arrow is none, triangle, stealth, arrow, diamond or oval")),
    })
}

/// The shapes list of a call.
fn shape_list(args: &Value) -> Result<Vec<u64>, ToolError> {
    args.get("shapes")
        .and_then(Value::as_array)
        .filter(|l| !l.is_empty())
        .and_then(|l| l.iter().map(Value::as_u64).collect::<Option<Vec<_>>>())
        .ok_or_else(|| ToolError::invalid("shapes is a list of shape_id"))
}

fn set_line(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "shapes", "color", "width", "dash", "arrow_head", "arrow_tail"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let list = shape_list(args)?;
    let color = args.get("color").map(|v| parse_color("color", v)).transpose()?;
    let width = match args.get("width") {
        None => None,
        Some(v) => Some(v.as_f64().filter(|w| w.is_finite() && (0.0..=1584.0).contains(w)).ok_or_else(|| ToolError::invalid("width is a number of points, 0 to 1584"))?),
    };
    let dash = args.get("dash").map(|v| v.as_str().ok_or_else(|| ToolError::invalid("dash is a name")).and_then(dash_of)).transpose()?;
    let head = args.get("arrow_head").map(|v| v.as_str().ok_or_else(|| ToolError::invalid("arrow_head is a name")).and_then(end_of)).transpose()?;
    let tail = args.get("arrow_tail").map(|v| v.as_str().ok_or_else(|| ToolError::invalid("arrow_tail is a name")).and_then(end_of)).transpose()?;
    if color.is_none() && width.is_none() && dash.is_none() && head.is_none() && tail.is_none() {
        return Err(ToolError::invalid("no line format is given (color, width, dash, arrow_head or arrow_tail)"));
    }
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let mut commands = Vec::new();
    for item in list {
        let (_, shape) = shape_id(open, number, item)?;
        let current = slide_at(presentation_of(open)?, number)?.shapes.iter().find(|s| s.id == shape).and_then(|s| match &s.kind {
            ShapeKind::Auto(a) => Some(a.stroke.clone()),
            ShapeKind::Line(l) => Some(Some(l.clone())),
            _ => None,
        });
        let Some(current) = current else { return Err(ToolError::new("unsupported_operation", format!("shape {item} has no outline"))) };
        // `color: null` is no line at all.
        if matches!(color, Some(ColorArg::Inherit)) {
            commands.push(Command::SetStroke { slide, shape, stroke: None });
            continue;
        }
        let mut stroke = current.unwrap_or_else(|| Stroke::solid(Color::BLACK, Stroke::DEFAULT_WIDTH));
        match &color {
            Some(ColorArg::Rgb(rgb)) => stroke.color = Color::opaque(*rgb),
            Some(ColorArg::Theme { name, brightness }) => stroke.color = crate::tools_pptx::theme_colour(open, name, *brightness)?,
            _ => {}
        }
        if let Some(w) = width {
            stroke.width = (w * 12700.0).round() as i64;
        }
        if let Some(d) = dash {
            stroke = stroke.with_dash(d);
        }
        if let Some(h) = head {
            stroke.head = h;
        }
        if let Some(t) = tail {
            stroke.tail = t;
        }
        commands.push(Command::SetStroke { slide, shape, stroke: Some(stroke) });
    }
    pptx_edit(service, id, commands, json!({}))
}

fn arrange_shapes(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "shapes", "order"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let list = shape_list(args)?;
    let order = args.str_arg("order")?;
    if !["front", "back", "forward", "backward"].contains(&order) {
        return Err(ToolError::invalid("order is front, back, forward or backward"));
    }
    let open = service.open_document(id)?;
    let slide = slide_id(open, number)?;
    let mut stack: Vec<ShapeId> = slide_at(presentation_of(open)?, number)?.shapes.iter().map(|s| s.id).collect();
    let mut targets = Vec::new();
    for item in list {
        targets.push(shape_id(open, number, item)?.1);
    }
    // Backward and forward keep the chosen shapes' own order: the one nearest the end moves first.
    if matches!(order, "front" | "forward") {
        targets.sort_by_key(|t| stack.iter().position(|s| s == t));
        targets.reverse();
    } else {
        targets.sort_by_key(|t| stack.iter().position(|s| s == t));
    }
    let mut commands = Vec::new();
    for shape in targets {
        let at = stack.iter().position(|s| *s == shape).ok_or_else(|| ToolError::new("no_such_shape", "shape"))?;
        let to = match order {
            "front" => stack.len() - 1,
            "back" => 0,
            "forward" => (at + 1).min(stack.len() - 1),
            _ => at.saturating_sub(1),
        };
        if to != at {
            stack.remove(at);
            stack.insert(to, shape);
            commands.push(Command::MoveShape { slide, shape, index: to });
        }
    }
    if commands.is_empty() {
        return Ok(crate::edit::edited(id, open, json!({}), vec![json!({ "code": "no_change", "message": "the shapes are already in that place" })]));
    }
    pptx_edit(service, id, commands, json!({}))
}

fn set_shape_properties(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    only_known(args, &["document_id", "slide", "shape", "name", "alt_text", "decorative", "hidden", "rotation", "flip", "aspect_locked", "hyperlink"])?;
    let id = args.str_arg("document_id")?;
    let number = args.usize_arg("slide")?;
    let open = service.open_document(id)?;
    let (slide, shape) = shape_id(open, number, args.usize_arg("shape")? as u64)?;
    let (rotation, flip_h, flip_v, alt_text) = {
        let sl = slide_at(presentation_of(open)?, number)?;
        let s = sl.shapes.iter().find(|s| s.id == shape).ok_or_else(|| ToolError::new("no_such_shape", "shape"))?;
        (s.rotation, s.flip_h, s.flip_v, sl.shape_alt_texts.get(&shape).cloned().unwrap_or_default())
    };
    let mut commands = Vec::new();
    if let Some(v) = args.get("name") {
        commands.push(Command::SetShapeName { slide, shape, name: v.as_str().ok_or_else(|| ToolError::invalid("name is a string"))?.to_string() });
    }
    if let Some(v) = args.get("alt_text") {
        let description = v.as_str().ok_or_else(|| ToolError::invalid("alt_text is a string"))?.to_string();
        commands.push(Command::SetShapeAltText { slide, shape, alt_text: ibis_doc_model::model::AltText { description, ..alt_text } });
    }
    if let Some(v) = args.get("decorative") {
        commands.push(Command::SetShapeDecorative { slide, shape, decorative: parse_bool("decorative", v)? });
    }
    if let Some(v) = args.get("hidden") {
        commands.push(Command::SetShapeHidden { slide, shape, hidden: parse_bool("hidden", v)? });
    }
    if let Some(v) = args.get("aspect_locked") {
        commands.push(Command::SetAspectLocked { slide, shape, locked: parse_bool("aspect_locked", v)? });
    }
    if let Some(v) = args.get("hyperlink") {
        use ibis_doc_model::text::SlideLink;
        let link = match v {
            Value::Null => None,
            Value::String(url) if !url.trim().is_empty() => Some(SlideLink::Url(url.trim().to_string())),
            Value::Object(o) => {
                let n = o.get("slide").and_then(Value::as_u64).ok_or_else(|| ToolError::invalid("hyperlink is a web address, { slide } or null"))? as usize;
                Some(SlideLink::Slide(slide_id(open, n)?))
            }
            _ => return Err(ToolError::invalid("hyperlink is a web address, { slide } or null")),
        };
        commands.push(Command::SetShapeLink { slide, shape, link });
    }
    if args.get("rotation").is_some() || args.get("flip").is_some() {
        let mut rotation = rotation;
        if let Some(v) = args.get("rotation") {
            let degrees = v.as_f64().filter(|d| d.is_finite()).ok_or_else(|| ToolError::invalid("rotation is a number of degrees"))?;
            rotation = ((degrees.rem_euclid(360.0)) * 60_000.0).round() as i32 % 21_600_000;
        }
        let (mut h, mut vflip) = (flip_h, flip_v);
        if let Some(flip) = args.get("flip") {
            let get = |k: &str, old: bool| flip.get(k).map(|v| parse_bool(k, v)).transpose().map(|o| o.unwrap_or(old));
            h = get("horizontal", h)?;
            vflip = get("vertical", vflip)?;
        }
        commands.push(Command::SetTransform { slide, shape, rotation, flip_h: h, flip_v: vflip });
    }
    if commands.is_empty() {
        return Err(ToolError::invalid("no property is given"));
    }
    pptx_edit(service, id, commands, json!({}))
}

/// The outline of a shape for a read: `null` for none, else colour, width (pt), dash and arrows.
pub(crate) fn line_json(stroke: Option<&Stroke>) -> Value {
    let Some(s) = stroke else { return Value::Null };
    let dash = match s.dash {
        LineDash::Solid => "solid",
        LineDash::Dot | LineDash::SysDot => "dot",
        LineDash::Dash | LineDash::SysDash => "dash",
        LineDash::LgDash => "long_dash",
        LineDash::DashDot | LineDash::SysDashDot => "dash_dot",
        LineDash::LgDashDot => "long_dash_dot",
        LineDash::LgDashDotDot | LineDash::SysDashDotDot => "long_dash_dot_dot",
    };
    let end = |e: LineEnd| match e {
        LineEnd::None => "none",
        LineEnd::Triangle(_) => "triangle",
        LineEnd::Stealth(_) => "stealth",
        LineEnd::Arrow(_) => "arrow",
        LineEnd::Diamond(_) => "diamond",
        LineEnd::Oval(_) => "oval",
    };
    json!({ "color": colour_json(s.color.rgb, s.color.theme_ref()), "width": s.width as f64 / 12700.0, "dash": dash, "arrow_head": end(s.head), "arrow_tail": end(s.tail) })
}
