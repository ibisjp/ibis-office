// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Picture and shape formatting tools for Word (docx):
//! - Context: Picture Tools > Picture > Artistic Effects (`docx.picture.artistic-effects`)
//! - Context: Picture Tools > Picture > Picture Styles (`docx.picture.styles`)
//! - Context: Picture Tools > Picture > Picture Effects (`docx.picture.effects`)
//! - Context: Picture Tools > Picture > Remove Background (`docx.picture.remove-background`)
//! - Context: Shape Tools > Shape / Text Box > Edit Shape (`docx.shape.edit-shape`)
//! - Context: Shape Tools > Shape / Text Box > WordArt Styles (`docx.shape.wordart-styles`)
//! - Context: Shape Tools > Shape / Text Box > Text Fill / Outline / Effects (`docx.shape.text-effects`)

use gpui_kit::*;
use ibis_doc_model::{
    Block, Color, Fill, FlowCommand, FlowRunKind, Geometry, Rgb,
    ShapeKind, Stroke, AutoShape,
};

use crate::shell::chrome::{MenuEntry, MenuId, MenuItem, Menus, Submenu};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

// ---------------------------------------------------------------------------
// 1. Picture Artistic Effects (docx.picture.artistic-effects)
// ---------------------------------------------------------------------------

pub(crate) const ARTISTIC_EFFECT_PRESETS: &[(&str, &str)] = &[
    ("docx.picture.artistic-effects.none", "none"),
    ("docx.picture.artistic-effects.marker", "marker"),
    ("docx.picture.artistic-effects.pencil-grayscale", "pencil-grayscale"),
    ("docx.picture.artistic-effects.chalk-sketch", "chalk-sketch"),
    ("docx.picture.artistic-effects.blur", "blur"),
    ("docx.picture.artistic-effects.glass", "glass"),
    ("docx.picture.artistic-effects.paint-strokes", "paint-strokes"),
    ("docx.picture.artistic-effects.cutout", "cutout"),
    ("docx.picture.artistic-effects.mosaic-bubbles", "mosaic-bubbles"),
    ("docx.picture.artistic-effects.glow-edges", "glow-edges"),
];

// ---------------------------------------------------------------------------
// 2. Picture Styles (docx.picture.styles)
// ---------------------------------------------------------------------------

pub(crate) const PICTURE_STYLE_PRESETS: &[(&str, &str)] = &[
    ("docx.picture.styles.simple-white", "simple-white"),
    ("docx.picture.styles.simple-black", "simple-black"),
    ("docx.picture.styles.thick-black", "thick-black"),
    ("docx.picture.styles.round-rect", "round-rect"),
    ("docx.picture.styles.ellipse", "ellipse"),
    ("docx.picture.styles.soft-edge", "soft-edge"),
    ("docx.picture.styles.drop-shadow", "drop-shadow"),
    ("docx.picture.styles.reflection", "reflection"),
];

// ---------------------------------------------------------------------------
// 3. Picture Effects (docx.picture.effects)
// ---------------------------------------------------------------------------

pub(crate) const PICTURE_EFFECT_SHADOWS: &[(&str, Option<&str>)] = &[
    ("docx.picture.effects.shadow.none", None),
    ("docx.picture.effects.shadow.outer", Some("outer")),
    ("docx.picture.effects.shadow.inner", Some("inner")),
];

pub(crate) const PICTURE_EFFECT_REFLECTIONS: &[(&str, Option<&str>)] = &[
    ("docx.picture.effects.reflection.none", None),
    ("docx.picture.effects.reflection.tight", Some("tight")),
    ("docx.picture.effects.reflection.half", Some("half")),
];

pub(crate) const PICTURE_EFFECT_GLOWS: &[(&str, Option<u32>)] = &[
    ("docx.picture.effects.glow.none", None),
    ("docx.picture.effects.glow.blue", Some(0x4472c4)),
    ("docx.picture.effects.glow.orange", Some(0xed7d31)),
    ("docx.picture.effects.glow.yellow", Some(0xffc000)),
];

pub(crate) const PICTURE_EFFECT_SOFT_EDGES: &[(&str, Option<i64>)] = &[
    ("docx.picture.effects.soft-edge.none", None),
    ("docx.picture.effects.soft-edge.1pt", Some(12_700)),
    ("docx.picture.effects.soft-edge.2_5pt", Some(31_750)),
    ("docx.picture.effects.soft-edge.5pt", Some(63_500)),
    ("docx.picture.effects.soft-edge.10pt", Some(127_000)),
    ("docx.picture.effects.soft-edge.25pt", Some(317_500)),
    ("docx.picture.effects.soft-edge.50pt", Some(635_000)),
];

// ---------------------------------------------------------------------------
// 4. Shape Editing: Change Shape (docx.shape.edit-shape)
// ---------------------------------------------------------------------------

pub(crate) const SHAPE_GEOMETRY_PRESETS: &[(&str, &str)] = &[
    ("docx.shape.edit-shape.rect", "rect"),
    ("docx.shape.edit-shape.round-rect", "roundRect"),
    ("docx.shape.edit-shape.ellipse", "ellipse"),
    ("docx.shape.edit-shape.triangle", "triangle"),
    ("docx.shape.edit-shape.diamond", "diamond"),
    ("docx.shape.edit-shape.right-arrow", "rightArrow"),
    ("docx.shape.edit-shape.heart", "heart"),
    ("docx.shape.edit-shape.star5", "star5"),
    ("docx.shape.edit-shape.callout", "wedgeRoundRectCallout"),
];

// ---------------------------------------------------------------------------
// 5. WordArt Styles (docx.shape.wordart-styles)
// ---------------------------------------------------------------------------

pub(crate) const WORDART_STYLE_PRESETS: &[(&str, usize)] = &[
    ("docx.shape.wordart-styles.style1", 0), // Fill: Black, Plain
    ("docx.shape.wordart-styles.style2", 1), // Fill: Blue, Accent 1, Shadow
    ("docx.shape.wordart-styles.style3", 2), // Fill: Orange, Accent 2, Outline
    ("docx.shape.wordart-styles.style4", 3), // Fill: Purple, Gradient, Reflection
    ("docx.shape.wordart-styles.style5", 4), // Fill: Gold, Accent 4, Bold
];

// ---------------------------------------------------------------------------
// 6. Text Effects (docx.shape.text-effects)
// ---------------------------------------------------------------------------

pub(crate) const TEXT_FILL_COLORS: &[(&str, Option<u32>)] = &[
    ("docx.shape.text-effects.fill.black", Some(0x000000)),
    ("docx.shape.text-effects.fill.blue", Some(0x4472c4)),
    ("docx.shape.text-effects.fill.orange", Some(0xed7d31)),
    ("docx.shape.text-effects.fill.red", Some(0xd32f2f)),
    ("docx.shape.text-effects.fill.green", Some(0x2e7d32)),
    ("docx.shape.text-effects.fill.white", Some(0xffffff)),
];

pub(crate) const TEXT_OUTLINE_COLORS: &[(&str, Option<u32>)] = &[
    ("docx.shape.text-effects.outline.none", None),
    ("docx.shape.text-effects.outline.black", Some(0x000000)),
    ("docx.shape.text-effects.outline.blue", Some(0x4472c4)),
    ("docx.shape.text-effects.outline.orange", Some(0xed7d31)),
];

pub(crate) const TEXT_EFFECT_SHADOWS: &[(&str, Option<&str>)] = &[
    ("docx.shape.text-effects.shadow.none", None),
    ("docx.shape.text-effects.shadow.outer", Some("outer")),
    ("docx.shape.text-effects.shadow.inner", Some("inner")),
];

// ---------------------------------------------------------------------------
// Extension methods on SlideEditor
// ---------------------------------------------------------------------------

#[ibis_office_macros::editor_ext(ShapePictureTools)]
impl SlideEditor {
    // --- 1. Artistic Effects ---

    pub(crate) fn set_docx_picture_artistic_effect(
        &mut self,
        effect_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.change_picture_look(|l| {
            match effect_id {
                None | Some("none") => {
                    l.effects.grayscale = false;
                    l.effects.bilevel = None;
                    l.effects.saturation = None;
                    l.effects.opacity = None;
                    l.soft_edge = None;
                    l.duotone = None;
                    l.luminance = None;
                }
                Some("marker") => {
                    l.effects.saturation = Some(150_000);
                }
                Some("pencil-grayscale") => {
                    l.effects.grayscale = true;
                    l.effects.bilevel = None;
                }
                Some("chalk-sketch") => {
                    l.effects.grayscale = true;
                    l.effects.bilevel = Some(50_000);
                }
                Some("blur") => {
                    l.soft_edge = Some(127_000); // 10 pt blur
                }
                Some("glass") => {
                    l.duotone = Some((Rgb(0x3F2300), Rgb(0xFFF3D6)));
                }
                Some("paint-strokes") => {
                    l.luminance = Some((20_000, 30_000));
                    l.effects.saturation = Some(130_000);
                }
                Some("cutout") => {
                    l.effects.bilevel = Some(60_000);
                }
                Some("mosaic-bubbles") => {
                    l.effects.saturation = Some(180_000);
                    l.luminance = Some((10_000, 20_000));
                }
                Some("glow-edges") => {
                    l.effects.bilevel = Some(40_000);
                    l.duotone = Some((Rgb(0x000000), Rgb(0x00FFCC)));
                }
                _ => {}
            }
        }, cx);
        self.status = Some(crate::shell::i18n::text("docx.picture.artistic-effects.applied"));
        cx.notify();
    }

    // --- 2. Picture Styles ---

    pub(crate) fn apply_docx_picture_style(
        &mut self,
        style_id: &str,
        cx: &mut Context<Self>,
    ) {
        self.change_picture_look(|l| {
            match style_id {
                "simple-white" => {
                    l.stroke = Some(Stroke::solid(Color::opaque(Rgb(0xFFFFFF)), 19_050)); // 1.5 pt
                    l.geometry = None;
                    l.soft_edge = None;
                }
                "simple-black" => {
                    l.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x000000)), 12_700)); // 1.0 pt
                    l.geometry = None;
                    l.soft_edge = None;
                }
                "thick-black" => {
                    l.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x000000)), 38_100)); // 3.0 pt
                    l.geometry = None;
                    l.soft_edge = None;
                }
                "round-rect" => {
                    l.geometry = Some(Geometry::RoundRect { radius: 15_000 });
                    l.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x4472C4)), 12_700));
                    l.soft_edge = None;
                }
                "ellipse" => {
                    l.geometry = Some(Geometry::Ellipse);
                    l.stroke = None;
                    l.soft_edge = None;
                }
                "soft-edge" => {
                    l.geometry = None;
                    l.stroke = None;
                    l.soft_edge = Some(127_000); // 10 pt
                }
                "drop-shadow" => {
                    l.geometry = None;
                    l.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x757575)), 12_700));
                    l.soft_edge = Some(31_750);
                }
                "reflection" => {
                    l.geometry = None;
                    l.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x4472C4)), 12_700));
                    l.effects.opacity = Some(85_000);
                }
                _ => {}
            }
        }, cx);
        self.status = Some(crate::shell::i18n::text("docx.picture.styles.applied"));
        cx.notify();
    }

    // --- 3. Picture Effects ---

    pub(crate) fn set_docx_picture_effect(
        &mut self,
        kind: &str,
        val: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        self.change_picture_look(|l| {
            match kind {
                "shadow" => {
                    if val.is_some() {
                        l.stroke = Some(Stroke::solid(Color::opaque(Rgb(0x404040)), 12_700));
                        l.soft_edge = Some(25_400); // 2 pt shadow softness
                    } else {
                        l.soft_edge = None;
                    }
                }
                "reflection" => {
                    if let Some(r) = val {
                        match r {
                            "tight" => l.effects.opacity = Some(90_000),
                            "half" => l.effects.opacity = Some(70_000),
                            _ => l.effects.opacity = None,
                        }
                    } else {
                        l.effects.opacity = None;
                    }
                }
                "glow" => {
                    if let Some(hex_str) = val {
                        if let Ok(c) = u32::from_str_radix(hex_str.trim_start_matches("0x"), 16) {
                            l.stroke = Some(Stroke::solid(Color::opaque(Rgb(c)), 25_400));
                        }
                    } else {
                        l.stroke = None;
                    }
                }
                _ => {}
            }
        }, cx);
        self.status = Some(crate::shell::i18n::text("docx.picture.effects.applied"));
        cx.notify();
    }

    // --- 4. Remove Background ---

    pub(crate) fn set_docx_picture_remove_background(&mut self, cx: &mut Context<Self>) {
        let is_active = self.is_docx_picture_remove_background_active();
        self.change_picture_look(|l| {
            if is_active {
                // Reset transparency / bilevel
                l.effects.bilevel = None;
                l.effects.opacity = None;
            } else {
                // Apply background removal (transparent keying / washout threshold)
                l.effects.bilevel = Some(95_000);
            }
        }, cx);
        let msg = if is_active {
            "docx.picture.remove-background.reset"
        } else {
            "docx.picture.remove-background.applied"
        };
        self.status = Some(crate::shell::i18n::text(msg));
        cx.notify();
    }

    pub(crate) fn is_docx_picture_remove_background_active(&self) -> bool {
        self.picture_look_at_caret().is_some_and(|l| l.effects.bilevel == Some(95_000))
    }

    // --- 5. Edit Shape: Change Geometry ---

    pub(crate) fn change_docx_shape_geometry(&mut self, geom_name: &str, cx: &mut Context<Self>) {
        let Some(info) = self.text_box_at_caret() else { return };
        let doc = match self.flow() {
            Some(d) => d,
            None => return,
        };
        let p = match self.paragraph_of(info.at.para) {
            Some(p) => p,
            None => return,
        };
        let mut offset = 0;
        let mut target_idx = None;
        for (idx, run) in p.runs.iter().enumerate() {
            if offset == info.at.offset {
                target_idx = Some(idx);
                break;
            }
            offset += run.len();
        }
        let Some(run_idx) = target_idx else { return };
        let mut new_p = p.clone();
        let run = &mut new_p.runs[run_idx];
        let FlowRunKind::Shape(shape) = &mut run.kind else { return };

        let new_geom = match geom_name {
            "rect" => Geometry::Rect,
            "roundRect" => Geometry::RoundRect { radius: 15_000 },
            "ellipse" => Geometry::Ellipse,
            other => ibis_pptx::geometry::preset(other).unwrap_or(Geometry::Rect),
        };

        match &mut shape.kind {
            ShapeKind::Auto(a) => {
                a.geometry = new_geom;
            }
            ShapeKind::TextBox(body) => {
                let text = std::mem::take(body);
                shape.kind = ShapeKind::Auto(AutoShape {
                    geometry: new_geom,
                    fill: Fill::Solid(Color::opaque(Rgb(0xffffff))),
                    stroke: Some(Stroke::solid(Color::opaque(Rgb(0x4472c4)), 12_700)),
                    text: Some(text),
                    tilt: None,
                    shadow: None,
                    glow: None,
                    inner_shadow: None,
                    soft_edge: None,
                    reflection: None,
                    bevel: None,
                });
            }
            _ => return,
        }

        // Find paragraph container and index
        let mut found = None;
        for (b_idx, block) in doc.body.iter().enumerate() {
            if let Block::Paragraph(bp) = block {
                if bp.id == p.id {
                    found = Some(b_idx);
                    break;
                }
            }
        }
        let Some(block_idx) = found else { return };

        let cmd = FlowCommand::ReplaceBlocks {
            container: ibis_doc_model::Container::Body,
            index: block_idx,
            remove: 1,
            blocks: vec![Block::Paragraph(new_p)],
        };
        self.run_flow(vec![cmd], cx);
        self.status = Some(crate::shell::i18n::text("docx.shape.edit-shape.applied"));
        cx.notify();
    }

    pub(crate) fn toggle_docx_shape_edit_points(&mut self, cx: &mut Context<Self>) {
        let is_on = self.is_docx_shape_edit_points_active();
        // Toggle status notification
        let msg = if is_on {
            "docx.shape.edit-shape.points-off"
        } else {
            "docx.shape.edit-shape.points-on"
        };
        self.status = Some(crate::shell::i18n::text(msg));
        cx.notify();
    }

    pub(crate) fn is_docx_shape_edit_points_active(&self) -> bool {
        self.status.as_deref() == Some(crate::shell::i18n::text("docx.shape.edit-shape.points-on").as_ref())
    }

    // --- 6. WordArt Styles ---

    pub(crate) fn apply_docx_wordart_style(&mut self, style_index: usize, cx: &mut Context<Self>) {
        let Some(info) = self.text_box_at_caret() else { return };
        let (bold, italic, size, color, fill, shadow, reflection) = match style_index {
            0 => (true, false, Some(2400), Some(Rgb(0x000000)), None, None, None),
            1 => (true, false, Some(2600), Some(Rgb(0x4472c4)), Some(Fill::Solid(Color::opaque(Rgb(0xd9e1f2)))), Some(Some(ibis_doc_model::Shadow::default())), None),
            2 => (true, true, Some(2400), Some(Rgb(0xed7d31)), Some(Fill::Solid(Color::opaque(Rgb(0xfce4d6)))), None, None),
            3 => (true, false, Some(2800), Some(Rgb(0x7030a0)), Some(Fill::Solid(Color::opaque(Rgb(0xf2e6ff)))), None, Some(Some(ibis_doc_model::Reflection::standard(63_500, 100_000)))),
            _ => (true, false, Some(2600), Some(Rgb(0xffc000)), Some(Fill::Solid(Color::opaque(Rgb(0xfff2cc)))), Some(Some(ibis_doc_model::Shadow::default())), None),
        };

        let mut commands = Vec::new();
        // Format text in shape
        commands.push(FlowCommand::SetShapeRangeFormat {
            at: info.at,
            from: 0,
            to: info.text.chars().count().max(1),
            bold: Some(bold),
            italic: Some(italic),
            size,
            color,
        });

        // Set shape effects
        if fill.is_some() || shadow.is_some() || reflection.is_some() {
            commands.push(FlowCommand::SetShapeEffects {
                at: info.at,
                fill,
                shadow,
                glow: None,
                soft_edge: None,
                reflection,
            });
        }

        self.run_flow(commands, cx);
        self.status = Some(crate::shell::i18n::text("docx.shape.wordart-styles.applied"));
        cx.notify();
    }

    // --- 7. Text Fill / Outline / Effects ---

    pub(crate) fn apply_docx_text_effect(&mut self, kind: &str, val: &str, cx: &mut Context<Self>) {
        let Some(info) = self.text_box_at_caret() else { return };
        let char_count = info.text.chars().count().max(1);
        let mut commands = Vec::new();

        match kind {
            "fill" => {
                let color = match val {
                    "black" => Some(Rgb(0x000000)),
                    "blue" => Some(Rgb(0x4472c4)),
                    "orange" => Some(Rgb(0xed7d31)),
                    "red" => Some(Rgb(0xd32f2f)),
                    "green" => Some(Rgb(0x2e7d32)),
                    "white" => Some(Rgb(0xffffff)),
                    _ => None,
                };
                commands.push(FlowCommand::SetShapeRangeFormat {
                    at: info.at,
                    from: 0,
                    to: char_count,
                    bold: None,
                    italic: None,
                    size: None,
                    color,
                });
            }
            "outline" => {
                let line = match val {
                    "black" => Some((Rgb(0x000000), 12_700)),
                    "blue" => Some((Rgb(0x4472c4), 12_700)),
                    "orange" => Some((Rgb(0xed7d31), 12_700)),
                    _ => None,
                };
                commands.push(FlowCommand::SetShapeStyle {
                    at: info.at,
                    fill: None,
                    line: Some(line),
                });
            }
            "shadow" => {
                let shadow = match val {
                    "outer" | "inner" => Some(Some(ibis_doc_model::Shadow::default())),
                    _ => Some(None),
                };
                commands.push(FlowCommand::SetShapeEffects {
                    at: info.at,
                    fill: None,
                    shadow,
                    glow: None,
                    soft_edge: None,
                    reflection: None,
                });
            }
            "reflection" => {
                let reflection = match val {
                    "tight" | "half" => Some(Some(ibis_doc_model::Reflection::standard(63_500, 100_000))),
                    _ => Some(None),
                };
                commands.push(FlowCommand::SetShapeEffects {
                    at: info.at,
                    fill: None,
                    shadow: None,
                    glow: None,
                    soft_edge: None,
                    reflection,
                });
            }
            "glow" => {
                let glow = match val {
                    "blue" => Some(Some((Color::opaque(Rgb(0x4472c4)), 63_500))),
                    "orange" => Some(Some((Color::opaque(Rgb(0xed7d31)), 63_500))),
                    "yellow" => Some(Some((Color::opaque(Rgb(0xffc000)), 63_500))),
                    _ => Some(None),
                };
                commands.push(FlowCommand::SetShapeEffects {
                    at: info.at,
                    fill: None,
                    shadow: None,
                    glow,
                    soft_edge: None,
                    reflection: None,
                });
            }
            _ => {}
        }

        if !commands.is_empty() {
            self.run_flow(commands, cx);
        }
        self.status = Some(crate::shell::i18n::text("docx.shape.text-effects.applied"));
        cx.notify();
    }

    // --- 8. Shape Grouping & Ungrouping (DOCX-FLOAT-016: wpg:wgp) ---

    /// Returns true if the current paragraph (or document body) has at least 2 shapes that can be grouped.
    pub(crate) fn can_group_docx_shapes(&self) -> bool {
        let Some(doc) = self.flow() else { return false };
        if let Some((sel, _)) = self.ordered_selection()
            && let Some(p) = self.paragraph_of(sel.para)
        {
            let count = p.runs.iter().filter(|r| matches!(r.kind, FlowRunKind::Shape(_))).count();
            if count >= 2 {
                return true;
            }
        }
        let total: usize = doc
            .body
            .iter()
            .filter_map(|b| match b {
                Block::Paragraph(p) => Some(p.runs.iter().filter(|r| matches!(r.kind, FlowRunKind::Shape(_))).count()),
                _ => None,
            })
            .sum();
        total >= 2
    }

    /// Returns true if a shape at/near the caret or in the caret's paragraph belongs to a `ShapeGroup`.
    pub(crate) fn is_docx_shape_grouped(&self) -> bool {
        let Some(doc) = self.flow() else { return false };
        if doc.shape_groups.is_empty() {
            return false;
        }
        if let Some((sel, _)) = self.ordered_selection()
            && let Some(p) = self.paragraph_of(sel.para)
        {
            for run in &p.runs {
                if let FlowRunKind::Shape(s) = &run.kind
                    && doc.shape_groups.iter().any(|g| g.contains_shape(s.id))
                {
                    return true;
                }
            }
        }
        !doc.shape_groups.is_empty()
    }

    /// Groups the specified shapes into a single `ShapeGroup` (`<wpg:wgp>`) in 1 Undo/Redo step.
    pub(crate) fn group_docx_shapes(
        &mut self,
        shape_ids: &[ibis_doc_model::ShapeId],
        cx: &mut Context<Self>,
    ) -> bool {
        if shape_ids.len() < 2 {
            return false;
        }
        let Some(doc) = self.flow() else { return false };
        let mut groups = doc.shape_groups.clone();
        for g in &mut groups {
            for &id in shape_ids {
                g.remove_shape(id);
            }
        }
        groups.retain(|g| g.leaf_shape_ids().len() >= 2 || g.has_unmodelled_children);
        let next_idx = groups.len() + 1;
        let native_id = (9000 + next_idx).to_string();
        let name = format!("Group {next_idx}");
        groups.push(ibis_doc_model::ShapeGroup::from_shape_ids(native_id, name, shape_ids));
        self.run_flow(vec![FlowCommand::SetShapeGroups(groups)], cx);
        self.status = Some(crate::shell::i18n::text("docx.form.grouped"));
        cx.notify();
        true
    }

    /// Groups all shapes in the caret's paragraph (or in the document body if the caret paragraph has < 2 shapes).
    pub(crate) fn group_docx_shapes_in_paragraph(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        let mut ids = Vec::new();
        if let Some((sel, _)) = self.ordered_selection()
            && let Some(p) = self.paragraph_of(sel.para)
        {
            for run in &p.runs {
                if let FlowRunKind::Shape(s) = &run.kind {
                    ids.push(s.id);
                }
            }
        }
        if ids.len() < 2 {
            ids.clear();
            for block in &doc.body {
                if let Block::Paragraph(p) = block {
                    for run in &p.runs {
                        if let FlowRunKind::Shape(s) = &run.kind {
                            ids.push(s.id);
                        }
                    }
                }
            }
        }
        self.group_docx_shapes(&ids, cx)
    }

    /// Ungroups the `ShapeGroup` containing the shape at/near the caret (or the first active group in the caret's paragraph).
    pub(crate) fn ungroup_docx_shape(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(doc) = self.flow() else { return false };
        if doc.shape_groups.is_empty() {
            return false;
        }
        let mut target_group_idx = None;
        if let Some((sel, _)) = self.ordered_selection()
            && let Some(p) = self.paragraph_of(sel.para)
        {
            let mut offset = 0usize;
            for run in &p.runs {
                let len = run.len();
                if let FlowRunKind::Shape(s) = &run.kind
                    && let Some(idx) = doc.shape_groups.iter().position(|g| g.contains_shape(s.id))
                {
                    if offset == sel.offset || target_group_idx.is_none() {
                        target_group_idx = Some(idx);
                    }
                }
                offset += len;
            }
        }
        let group_idx = target_group_idx.unwrap_or(doc.shape_groups.len() - 1);
        let mut groups = doc.shape_groups.clone();
        groups.remove(group_idx);
        self.run_flow(vec![FlowCommand::SetShapeGroups(groups)], cx);
        self.status = Some(crate::shell::i18n::text("docx.form.ungrouped"));
        cx.notify();
        true
    }

    /// Moves a shape (and all other member shapes in its `ShapeGroup`, if grouped) by `(dx, dy)` in EMUs in 1 Undo/Redo step.
    pub(crate) fn move_docx_shape_group(
        &mut self,
        shape_id: ibis_doc_model::ShapeId,
        dx: ibis_doc_model::Emu,
        dy: ibis_doc_model::Emu,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(doc) = self.flow() else { return false };
        let member_ids: Vec<ibis_doc_model::ShapeId> = doc
            .shape_groups
            .iter()
            .find(|g| g.contains_shape(shape_id))
            .map(|g| g.leaf_shape_ids())
            .unwrap_or_else(|| vec![shape_id]);

        let mut commands = Vec::new();
        for (b_idx, block) in doc.body.iter().enumerate() {
            if let Block::Paragraph(p) = block {
                let mut changed = false;
                let mut new_p = p.clone();
                for run in &mut new_p.runs {
                    if let FlowRunKind::Shape(s) = &mut run.kind
                        && member_ids.contains(&s.id)
                    {
                        s.frame.x += dx;
                        s.frame.y += dy;
                        if let Some(anchor) = &mut s.floating {
                            anchor.offset_x += dx;
                            anchor.offset_y += dy;
                        }
                        changed = true;
                    }
                }
                if changed {
                    commands.push(FlowCommand::ReplaceBlocks {
                        container: ibis_doc_model::Container::Body,
                        index: b_idx,
                        remove: 1,
                        blocks: vec![Block::Paragraph(new_p)],
                    });
                }
            }
        }
        if commands.is_empty() {
            return false;
        }
        self.run_flow(commands, cx);
        cx.notify();
        true
    }
}

// ---------------------------------------------------------------------------
// Menu Registration
// ---------------------------------------------------------------------------

pub(crate) fn register_menus(menus: &mut Menus, editor: &SlideEditor) {
    let has_pic = editor.picture_at_caret().is_some();
    let has_shape = editor.text_box_at_caret().is_some();

    // 1. Picture: Artistic Effects (docx.picture.artistic-effects)
    let artistic_items: Vec<MenuEntry> = ARTISTIC_EFFECT_PRESETS
        .iter()
        .map(|&(id, key)| {
            MenuItem::tr(id)
                .disabled(!has_pic)
                .on(move |this, _, cx| this.set_docx_picture_artistic_effect(Some(key), cx))
                .into()
        })
        .collect();
    menus.items_mut(MenuId::Format).push(
        Submenu::tr("docx.picture.artistic-effects", "auto_fix_high", artistic_items).into(),
    );

    // 2. Picture: Styles (docx.picture.styles)
    let style_items: Vec<MenuEntry> = PICTURE_STYLE_PRESETS
        .iter()
        .map(|&(id, key)| {
            MenuItem::tr(id)
                .disabled(!has_pic)
                .on(move |this, _, cx| this.apply_docx_picture_style(key, cx))
                .into()
        })
        .collect();
    menus.items_mut(MenuId::Format).push(
        Submenu::tr("docx.picture.styles", "style", style_items).into(),
    );

    // 3. Picture: Effects (docx.picture.effects)
    let mut effect_items: Vec<MenuEntry> = Vec::new();
    // Shadow sub
    let shadow_items: Vec<MenuEntry> = PICTURE_EFFECT_SHADOWS
        .iter()
        .map(|&(id, val)| {
            MenuItem::tr(id)
                .disabled(!has_pic)
                .on(move |this, _, cx| this.set_docx_picture_effect("shadow", val, cx))
                .into()
        })
        .collect();
    effect_items.push(Submenu::tr("docx.picture.effects.shadow", "wb_shade", shadow_items).into());

    // Reflection sub
    let reflection_items: Vec<MenuEntry> = PICTURE_EFFECT_REFLECTIONS
        .iter()
        .map(|&(id, val)| {
            MenuItem::tr(id)
                .disabled(!has_pic)
                .on(move |this, _, cx| this.set_docx_picture_effect("reflection", val, cx))
                .into()
        })
        .collect();
    effect_items.push(Submenu::tr("docx.picture.effects.reflection", "flip", reflection_items).into());

    // Glow sub
    let glow_items: Vec<MenuEntry> = PICTURE_EFFECT_GLOWS
        .iter()
        .map(|&(id, val)| {
            let hex = val.map(|v| format!("0x{:06x}", v));
            MenuItem::tr(id)
                .disabled(!has_pic)
                .on(move |this, _, cx| this.set_docx_picture_effect("glow", hex.as_deref(), cx))
                .into()
        })
        .collect();
    effect_items.push(Submenu::tr("docx.picture.effects.glow", "flare", glow_items).into());

    // Soft Edge sub
    let soft_items: Vec<MenuEntry> = PICTURE_EFFECT_SOFT_EDGES
        .iter()
        .map(|&(id, val)| {
            MenuItem::tr(id)
                .disabled(!has_pic)
                .on(move |this, _, cx| {
                    this.set_picture_soft_edge(val, cx);
                    this.status = Some(crate::shell::i18n::text("docx.picture.effects.applied"));
                    cx.notify();
                })
                .into()
        })
        .collect();
    effect_items.push(Submenu::tr("docx.picture.effects.soft-edge", "blur_on", soft_items).into());

    menus.items_mut(MenuId::Format).push(
        Submenu::tr("docx.picture.effects", "auto_awesome", effect_items).into(),
    );

    // 4. Picture: Remove Background (docx.picture.remove-background)
    menus.items_mut(MenuId::Format).push(
        MenuItem::tr("docx.picture.remove-background")
            .icon("auto_fix_high")
            .disabled(!has_pic)
            .checked(has_pic && editor.is_docx_picture_remove_background_active())
            .on(|this, _, cx| this.set_docx_picture_remove_background(cx))
            .into(),
    );

    // 5. Shape: Edit Shape (docx.shape.edit-shape)
    let mut edit_shape_items: Vec<MenuEntry> = Vec::new();
    let change_shape_items: Vec<MenuEntry> = SHAPE_GEOMETRY_PRESETS
        .iter()
        .map(|&(id, key)| {
            MenuItem::tr(id)
                .disabled(!has_shape)
                .on(move |this, _, cx| this.change_docx_shape_geometry(key, cx))
                .into()
        })
        .collect();
    edit_shape_items.push(
        Submenu::tr("docx.shape.edit-shape.change-shape", "category", change_shape_items).into(),
    );
    edit_shape_items.push(
        MenuItem::tr("docx.shape.edit-shape.edit-points")
            .icon("edit_attributes")
            .disabled(!has_shape)
            .checked(has_shape && editor.is_docx_shape_edit_points_active())
            .on(|this, _, cx| this.toggle_docx_shape_edit_points(cx))
            .into(),
    );
    menus.items_mut(MenuId::Format).push(
        Submenu::tr("docx.shape.edit-shape", "edit_attributes", edit_shape_items).into(),
    );

    // 6. Shape: WordArt Styles (docx.shape.wordart-styles)
    let wordart_items: Vec<MenuEntry> = WORDART_STYLE_PRESETS
        .iter()
        .map(|&(id, idx)| {
            MenuItem::tr(id)
                .disabled(!has_shape)
                .on(move |this, _, cx| this.apply_docx_wordart_style(idx, cx))
                .into()
        })
        .collect();
    menus.items_mut(MenuId::Format).push(
        Submenu::tr("docx.shape.wordart-styles", "text_format", wordart_items).into(),
    );

    // 7. Shape: Text Fill / Outline / Effects (docx.shape.text-effects)
    let mut text_effect_items: Vec<MenuEntry> = Vec::new();
    // Fill sub
    let fill_items: Vec<MenuEntry> = TEXT_FILL_COLORS
        .iter()
        .map(|&(id, _)| {
            let key = id.rsplit('.').next().unwrap_or("black");
            MenuItem::tr(id)
                .disabled(!has_shape)
                .on(move |this, _, cx| this.apply_docx_text_effect("fill", key, cx))
                .into()
        })
        .collect();
    text_effect_items.push(
        Submenu::tr("docx.shape.text-effects.fill", "format_color_fill", fill_items).into(),
    );

    // Outline sub
    let outline_items: Vec<MenuEntry> = TEXT_OUTLINE_COLORS
        .iter()
        .map(|&(id, _)| {
            let key = id.rsplit('.').next().unwrap_or("none");
            MenuItem::tr(id)
                .disabled(!has_shape)
                .on(move |this, _, cx| this.apply_docx_text_effect("outline", key, cx))
                .into()
        })
        .collect();
    text_effect_items.push(
        Submenu::tr("docx.shape.text-effects.outline", "border_color", outline_items).into(),
    );

    // Shadow sub
    let shadow_sub: Vec<MenuEntry> = TEXT_EFFECT_SHADOWS
        .iter()
        .map(|&(id, val)| {
            let key = val.unwrap_or("none");
            MenuItem::tr(id)
                .disabled(!has_shape)
                .on(move |this, _, cx| this.apply_docx_text_effect("shadow", key, cx))
                .into()
        })
        .collect();
    text_effect_items.push(
        Submenu::tr("docx.shape.text-effects.shadow", "wb_shade", shadow_sub).into(),
    );

    menus.items_mut(MenuId::Format).push(
        Submenu::tr("docx.shape.text-effects", "format_color_text", text_effect_items).into(),
    );

    // 8. Shape / Picture: Group & Ungroup (docx.shape.group / docx.shape.ungroup, DOCX-FLOAT-016)
    let can_group = editor.can_group_docx_shapes();
    let is_grouped = editor.is_docx_shape_grouped();
    menus.items_mut(MenuId::Format).push(
        MenuItem::new("docx.shape.group", crate::shell::i18n::text("pptx.group"))
            .icon("group_work")
            .disabled(!can_group)
            .on(|this, _, cx| {
                this.group_docx_shapes_in_paragraph(cx);
            })
            .into(),
    );
    menus.items_mut(MenuId::Format).push(
        MenuItem::new("docx.shape.ungroup", crate::shell::i18n::text("pptx.ungroup"))
            .icon("workspaces")
            .disabled(!is_grouped)
            .on(|this, _, cx| {
                this.ungroup_docx_shape(cx);
            })
            .into(),
    );

    // Connect top-level actions
    menus.connect("docx.picture.remove-background", |this, _, cx| {
        this.set_docx_picture_remove_background(cx);
    });
    menus.connect("docx.shape.group", |this, _, cx| {
        this.group_docx_shapes_in_paragraph(cx);
    });
    menus.connect("docx.shape.ungroup", |this, _, cx| {
        this.ungroup_docx_shape(cx);
    });
    menus.connect("docx.picture.group", |this, _, cx| {
        this.group_docx_shapes_in_paragraph(cx);
    });
    menus.connect("docx.picture.ungroup", |this, _, cx| {
        this.ungroup_docx_shape(cx);
    });
}
