// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What drawing shapes and pictures needs that is not a method of the window: the preset shape lists, the shapes new ones are made as,
//! the pictures' sizes (read from their headers), the icon categories, the order and alignment kinds. The three formats put shapes on their
//! pages and read pictures alike, so these are here and not in `pptx/editing.rs` (which keeps the old names as re-exports).

use crate::chrome::{MenuEntry, MenuItem};
use ibis_doc_model::{
    AutoShape, Color, EmuRect, Fill, Geometry, Rgb, Shape, ShapeId, ShapeKind, Stroke, TextAlign,
    TextBody, TextStyle, VerticalAnchor,
};


/// The preset shapes the toolbar and 挿入 > 図形 offer: (preset, label,
/// icon, menu item id).
pub const SHAPES: &[(&str, &str, &str)] = &[
    ("rect", "rectangle", "shape.rect"),
    ("roundRect", "rounded_corner", "shape.roundRect"),
    ("ellipse", "circle", "shape.ellipse"),
    ("triangle", "change_history", "shape.triangle"),
    ("rtTriangle", "signal_cellular_4_bar", "shape.rtTriangle"),
    ("diamond", "diamond", "shape.diamond"),
    ("pentagon", "pentagon", "shape.pentagon"),
    ("hexagon", "hexagon", "shape.hexagon"),
    ("rightArrow", "arrow_forward", "shape.rightArrow"),
    ("leftArrow", "arrow_back", "shape.leftArrow"),
    ("upArrow", "arrow_upward", "shape.upArrow"),
    ("downArrow", "arrow_downward", "shape.downArrow"),
    ("star5", "star", "shape.star5"),
    ("wedgeRectCallout", "chat_bubble", "shape.wedgeRectCallout"),
    ("cloud", "cloud", "shape.cloud"),
    ("heart", "favorite", "shape.heart"),
];

/// A pasted copy on the same slide is moved this much down and right.
pub const PASTE_OFFSET: i64 = 182_880;


/// ワードアートのプリセットスタイル (WordArt styles)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WordArtPreset {
    pub id: &'static str,
    pub insert_id: &'static str,
    pub apply_id: &'static str,
    pub(crate) name: &'static str,
    pub color: Rgb,
    pub bold: bool,
    pub outline: Option<(Rgb, i64)>,
    pub shadow: Option<(i32, i64, i64)>,
}

pub const WORDART_PRESETS: &[WordArtPreset] = &[
    WordArtPreset {
        id: "pptx.wordart.style1",
        insert_id: "pptx.insert.wordart.style1",
        apply_id: "pptx.text.wordart.style1",
        name: "",
        color: Rgb(0x000000),
        bold: true,
        outline: None,
        shadow: None,
    },
    WordArtPreset {
        id: "pptx.wordart.style2",
        insert_id: "pptx.insert.wordart.style2",
        apply_id: "pptx.text.wordart.style2",
        name: "",
        color: Rgb(0x4472C4),
        bold: true,
        outline: None,
        shadow: Some((2_700_000, 19_050, 25_400)),
    },
    WordArtPreset {
        id: "pptx.wordart.style3",
        insert_id: "pptx.insert.wordart.style3",
        apply_id: "pptx.text.wordart.style3",
        name: "",
        color: Rgb(0xED7D31),
        bold: true,
        outline: Some((Rgb(0xC55A11), 12_700)),
        shadow: None,
    },
    WordArtPreset {
        id: "pptx.wordart.style4",
        insert_id: "pptx.insert.wordart.style4",
        apply_id: "pptx.text.wordart.style4",
        name: "",
        color: Rgb(0xFFFFFF),
        bold: true,
        outline: Some((Rgb(0x000000), 19_050)),
        shadow: None,
    },
    WordArtPreset {
        id: "pptx.wordart.style5",
        insert_id: "pptx.insert.wordart.style5",
        apply_id: "pptx.text.wordart.style5",
        name: "",
        color: Rgb(0xFFC000),
        bold: true,
        outline: None,
        shadow: Some((5_400_000, 19_050, 25_400)),
    },
    WordArtPreset {
        id: "pptx.wordart.style6",
        insert_id: "pptx.insert.wordart.style6",
        apply_id: "pptx.text.wordart.style6",
        name: "",
        color: Rgb(0x4472C4),
        bold: true,
        outline: Some((Rgb(0xFFFFFF), 12_700)),
        shadow: Some((2_700_000, 19_050, 25_400)),
    },
];

pub fn new_shape(id: ShapeId, preset: &str, frame: EmuRect) -> Shape {
    let geometry = ibis_pptx::preset_geometry(preset).unwrap_or(Geometry::Rect);
    let mut auto = AutoShape::new(geometry, Fill::Solid(Color::opaque(Rgb(0x4472c4))), Some(Stroke::solid(Color::opaque(Rgb(0x2f528f)), 12_700)));
    let style = TextStyle { color: Rgb::WHITE, align: TextAlign::Center, ..TextStyle::default() };
    auto.text = Some(TextBody { anchor: VerticalAnchor::Middle, ..TextBody::plain("", style) });
    Shape::new(id, frame, ShapeKind::Auto(auto))
}

/// An empty text body as PowerPoint gives a shape: centred, in the middle.
pub(crate) fn centred_body() -> TextBody {
    let style = TextStyle { color: Rgb::WHITE, align: TextAlign::Center, ..TextStyle::default() };
    TextBody { anchor: VerticalAnchor::Middle, ..TextBody::plain("", style) }
}

/// Returns true if the geometry is a connector/line preset (straight, bent, or curved).
pub fn is_connector_geometry(geom: &Geometry) -> bool {
    match geom {
        Geometry::Formula(f) => matches!(
            f.preset.as_deref(),
            Some(
                "line"
                    | "lineInv"
                    | "straightConnector1"
                    | "bentConnector2"
                    | "bentConnector3"
                    | "bentConnector4"
                    | "bentConnector5"
                    | "curvedConnector2"
                    | "curvedConnector3"
                    | "curvedConnector4"
                    | "curvedConnector5"
            )
        ),
        _ => false,
    }
}

/// A new line, from its frame and flips.
pub fn new_line(id: ShapeId, frame: EmuRect, flip_h: bool, flip_v: bool, arrow: bool) -> Shape {
    new_connector(id, frame, flip_h, flip_v, LineKind::Straight, if arrow { ArrowKind::Single } else { ArrowKind::None })
}

/// A new line or connector (straight, bent, curved) with optional single or double arrowheads.
pub fn new_connector(
    id: ShapeId,
    frame: EmuRect,
    flip_h: bool,
    flip_v: bool,
    kind: LineKind,
    arrow: ArrowKind,
) -> Shape {
    let mut stroke = Stroke::solid(Color::opaque(Rgb(0x4472c4)), 12_700);
    match arrow {
        ArrowKind::None => {}
        ArrowKind::Single => {
            stroke.tail = ibis_doc_model::LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
        }
        ArrowKind::Double => {
            stroke.head = ibis_doc_model::LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
            stroke.tail = ibis_doc_model::LineEnd::Triangle(ibis_doc_model::EndSize { width: 1, length: 1 });
        }
    }
    match kind {
        LineKind::Straight => {
            let mut shape = Shape::new(id, frame, ShapeKind::Line(stroke));
            shape.flip_h = flip_h;
            shape.flip_v = flip_v;
            shape
        }
        LineKind::Bent => {
            let geom = ibis_pptx::preset_geometry("bentConnector3").unwrap_or(Geometry::Rect);
            let auto = AutoShape::new(geom, Fill::None, Some(stroke));
            let mut shape = Shape::new(id, frame, ShapeKind::Auto(auto));
            shape.flip_h = flip_h;
            shape.flip_v = flip_v;
            shape
        }
        LineKind::Curved => {
            let geom = ibis_pptx::preset_geometry("curvedConnector3").unwrap_or(Geometry::Rect);
            let auto = AutoShape::new(geom, Fill::None, Some(stroke));
            let mut shape = Shape::new(id, frame, ShapeKind::Auto(auto));
            shape.flip_h = flip_h;
            shape.flip_v = flip_v;
            shape
        }
        LineKind::Freehand => {
            let pts = vec![(0, 0), (frame.width, frame.height)];
            new_freehand_shape(id, frame, &pts, Some(stroke))
        }
    }
}

/// A new freehand scribble shape from its frame, local EMU coordinates, and optional stroke.
pub fn new_freehand_shape(
    id: ShapeId,
    frame: EmuRect,
    points: &[(i64, i64)],
    stroke: Option<Stroke>,
) -> Shape {
    use ibis_doc_model::{FormulaGeometry, GeometryDef, GeometryPath, PathFill, PathOp};
    let stroke = stroke.unwrap_or_else(|| Stroke::solid(Color::opaque(Rgb(0x4472c4)), 12_700));
    let mut ops = Vec::with_capacity(points.len().max(2));
    if let Some(&(x, y)) = points.first() {
        ops.push(PathOp::MoveTo(x.to_string(), y.to_string()));
    }
    for &(x, y) in points.iter().skip(1) {
        ops.push(PathOp::LineTo(x.to_string(), y.to_string()));
    }
    let path = GeometryPath {
        width: frame.width,
        height: frame.height,
        fill: PathFill::None,
        stroke: true,
        ops,
    };
    let def = GeometryDef {
        adjust: Vec::new(),
        guides: Vec::new(),
        paths: vec![path],
        text_rect: None,
    };
    let geom = FormulaGeometry {
        preset: None,
        adjust: Vec::new(),
        def: std::sync::Arc::new(def),
    };
    let auto = AutoShape::new(Geometry::Formula(geom), Fill::None, Some(stroke));
    Shape::new(id, frame, ShapeKind::Auto(auto))
}

/// Parse width and height from SVG data (attributes `width`, `height`, or `viewBox`).
pub(crate) fn svg_pixels(data: &[u8]) -> Option<(u32, u32)> {
    let text = std::str::from_utf8(data).ok()?;
    let svg_start = text.find("<svg")?;
    let svg_tag = &text[svg_start..];
    let tag_end = svg_tag.find('>')?;
    let header = &svg_tag[..tag_end];

    let parse_dim = |attr_name: &str| -> Option<f64> {
        let pattern_dq = format!("{attr_name}=\"");
        let pattern_sq = format!("{attr_name}='");
        let (val_str, quote) = if let Some(pos) = header.find(&pattern_dq) {
            (&header[pos + pattern_dq.len()..], '"')
        } else if let Some(pos) = header.find(&pattern_sq) {
            (&header[pos + pattern_sq.len()..], '\'')
        } else {
            return None;
        };
        let end = val_str.find(quote)?;
        let raw = val_str[..end].trim();
        if raw.ends_with('%') {
            return None;
        }
        let num_str = raw.trim_end_matches(|c: char| c.is_alphabetic());
        let val = num_str.parse::<f64>().ok()?;
        if raw.ends_with("pt") {
            Some(val * 96.0 / 72.0)
        } else if raw.ends_with("in") {
            Some(val * 96.0)
        } else if raw.ends_with("mm") {
            Some(val * 96.0 / 25.4)
        } else if raw.ends_with("cm") {
            Some(val * 96.0 / 2.54)
        } else {
            Some(val)
        }
    };

    let w = parse_dim("width");
    let h = parse_dim("height");
    if let (Some(w), Some(h)) = (w, h) {
        if w > 0.0 && h > 0.0 {
            return Some((w.round() as u32, h.round() as u32));
        }
    }

    // Fallback to viewBox: "min-x min-y width height"
    let pattern_dq = "viewBox=\"";
    let pattern_sq = "viewBox='";
    let (val_str, quote) = if let Some(pos) = header.find(pattern_dq) {
        (&header[pos + pattern_dq.len()..], '"')
    } else if let Some(pos) = header.find(pattern_sq) {
        (&header[pos + pattern_sq.len()..], '\'')
    } else {
        return None;
    };
    let end = val_str.find(quote)?;
    let raw = val_str[..end].trim();
    let parts: Vec<f64> = raw
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<f64>().ok())
        .collect();
    if parts.len() == 4 && parts[2] > 0.0 && parts[3] > 0.0 {
        Some((parts[2].round() as u32, parts[3].round() as u32))
    } else {
        None
    }
}

fn is_svg_header(data: &[u8]) -> bool {
    data.starts_with(b"<svg")
        || data.starts_with(b"<?xml")
        || (data.len() > 4 && data[..1024.min(data.len())].windows(4).any(|w| w == b"<svg"))
}

/// The pixel size of a PNG, JPEG, GIF, BMP or SVG picture, from its header.
pub fn validated_picture_size(format: ibis_doc_model::MediaFormat, data: &[u8]) -> Option<(u32, u32)> {
    use ibis_doc_model::MediaFormat;
    // A metafile is as large as the picture it draws (an EMF or WMF of a freeform pasted from Office).
    if matches!(format, MediaFormat::Emf | MediaFormat::Wmf) {
        return metafile_picture_size(format, data);
    }
    let signature = match format {
        MediaFormat::Png => data.starts_with(b"\x89PNG\r\n\x1a\n"),
        MediaFormat::Jpeg => data.starts_with(&[0xff, 0xd8, 0xff]),
        MediaFormat::Gif => data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a"),
        MediaFormat::Bmp => data.starts_with(b"BM"),
        MediaFormat::Svg => is_svg_header(data),
        _ => false,
    };
    if !signature { return None; }
    let (width, height) = picture_size(data)?;
    if width == 0 || height == 0 { return None; }
    if format == MediaFormat::Png {
        // Reject truncated containers on every platform before storing media.
        let mut at = 8usize;
        let mut have_data = false;
        let mut have_end = false;
        while at < data.len() {
            let length = u32::from_be_bytes(data.get(at..at.checked_add(4)?)?.try_into().ok()?) as usize;
            let kind = data.get(at + 4..at + 8)?;
            let end = at.checked_add(12)?.checked_add(length)?;
            if end > data.len() { return None; }
            if at == 8 && (kind != b"IHDR" || length != 13) { return None; }
            have_data |= kind == b"IDAT";
            if kind == b"IEND" { have_end = length == 0 && end == data.len(); break; }
            at = end;
        }
        if !have_data || !have_end { return None; }
    }
    #[cfg(all(target_os = "macos", feature = "export"))]
    {
        if format == MediaFormat::Svg {
            return Some((width, height));
        }
        // All four codecs are already enabled by the native dependency graph.
        let decoder = match format { MediaFormat::Png => image::ImageFormat::Png, MediaFormat::Jpeg => image::ImageFormat::Jpeg, MediaFormat::Gif => image::ImageFormat::Gif, MediaFormat::Bmp => image::ImageFormat::Bmp, _ => return None };
        let decoded = image::load_from_memory_with_format(data, decoder).ok()?;
        Some((decoded.width(), decoded.height()))
    }
    #[cfg(not(all(target_os = "macos", feature = "export")))]
    Some((width, height))
}

/// The pixel size (96 dpi) of what an EMF or WMF draws: of its SVG, or of the one bitmap it holds.
fn metafile_picture_size(format: ibis_doc_model::MediaFormat, data: &[u8]) -> Option<(u32, u32)> {
    let size = match format {
        ibis_doc_model::MediaFormat::Emf => match ibis_emf::emf_picture(data)? {
            ibis_emf::Picture::Svg(svg) => picture_size(svg.as_bytes()),
            ibis_emf::Picture::Image(_, bytes) => picture_size(&bytes),
        },
        _ => picture_size(ibis_emf::wmf_to_svg(data)?.as_bytes()),
    }?;
    (size.0 > 0 && size.1 > 0).then_some(size)
}

pub fn picture_size(data: &[u8]) -> Option<(u32, u32)> {
    if is_svg_header(data) {
        if let Some((w, h)) = svg_pixels(data) {
            return Some((w, h));
        }
    }
    let be32 = |i: usize| Some(u32::from_be_bytes(data.get(i..i + 4)?.try_into().ok()?));
    let le = |i: usize, n: usize| -> Option<u32> { Some(data.get(i..i + n)?.iter().rev().fold(0u32, |a, &b| (a << 8) | b as u32)) };
    if data.starts_with(b"\x89PNG") {
        return Some((be32(16)?, be32(20)?));
    }
    if data.starts_with(b"GIF8") {
        return Some((le(6, 2)?, le(8, 2)?));
    }
    if data.starts_with(b"BM") {
        // The height of a top-down bitmap is negative.
        return Some((le(18, 4)?, (le(22, 4)? as i32).unsigned_abs()));
    }
    if data.starts_with(&[0xff, 0xd8]) {
        // The first start-of-frame marker.
        let mut i = 2;
        while i + 9 < data.len() {
            if data[i] != 0xff {
                i += 1;
                continue;
            }
            let marker = data[i + 1];
            let len = ((data[i + 2] as usize) << 8) | data[i + 3] as usize;
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                let h = ((data[i + 5] as u32) << 8) | data[i + 6] as u32;
                let w = ((data[i + 7] as u32) << 8) | data[i + 8] as u32;
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    Front,
    Forward,
    Backward,
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

/// A category grouping of bundled icons for the Insert Icon menu.
pub struct IconCategory {
    pub label_key: &'static str,
    pub icon: &'static str,
    pub items: &'static [(&'static str, &'static str)],
}

pub const ICON_CATEGORIES: &[IconCategory] = &[
    IconCategory {
        label_key: "pptx.icon-category.arrows",
        icon: "arrow_forward",
        items: &[
            ("arrow_forward", "pptx.icon.arrow-right"),
            ("arrow_left", "pptx.icon.arrow-left"),
            ("arrow_upward", "pptx.icon.arrow-up"),
            ("arrow_downward", "pptx.icon.arrow-down"),
            ("arrow_right_alt", "pptx.icon.arrow-right-alt"),
            ("turn_right", "pptx.icon.turn-right"),
            ("swap_horiz", "pptx.icon.swap-horiz"),
        ],
    },
    IconCategory {
        label_key: "pptx.icon-category.business",
        icon: "description",
        items: &[
            ("description", "pptx.icon.document"),
            ("folder_open", "pptx.icon.folder"),
            ("insert_chart", "pptx.icon.chart"),
            ("table_chart", "pptx.icon.table"),
            ("bar_chart", "pptx.icon.bar-chart"),
            ("schedule", "pptx.icon.schedule"),
            ("edit", "pptx.icon.edit"),
            ("save", "pptx.icon.save"),
            ("print", "pptx.icon.print"),
            ("search", "pptx.icon.search"),
        ],
    },
    IconCategory {
        label_key: "pptx.icon-category.media",
        icon: "image",
        items: &[
            ("image", "pptx.icon.image"),
            ("movie", "pptx.icon.movie"),
            ("music_note", "pptx.icon.music"),
            ("photo_camera", "pptx.icon.camera"),
            ("play_circle", "pptx.icon.play"),
            ("link", "pptx.icon.link"),
            ("language", "pptx.icon.globe"),
            ("chat_bubble", "pptx.icon.chat"),
            ("co_present", "pptx.icon.presentation"),
        ],
    },
    IconCategory {
        label_key: "pptx.icon-category.symbols",
        icon: "auto_awesome",
        items: &[
            ("check", "pptx.icon.check"),
            ("close", "pptx.icon.close"),
            ("add", "pptx.icon.add"),
            ("info", "pptx.icon.info"),
            ("help", "pptx.icon.help"),
            ("settings", "pptx.icon.settings"),
            ("palette", "pptx.icon.palette"),
            ("lock", "pptx.icon.lock"),
            ("auto_awesome", "pptx.icon.sparkle"),
            ("new_releases", "pptx.icon.badge"),
        ],
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Straight,
    Bent,
    Curved,
    Freehand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowKind {
    None,
    Single,
    Double,
}

pub fn smartart_insert_items() -> Vec<MenuEntry> {
    vec![
        MenuItem::tr("pptx.insert.smartart.list").icon("view_list").on(|e, _, cx| e.insert_smartart(crate::smartart::SmartArtKind::BasicBlockList, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.vlist").icon("format_list_bulleted").on(|e, _, cx| e.insert_smartart(crate::smartart::SmartArtKind::VerticalList, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.insert.smartart.process").icon("arrow_forward").on(|e, _, cx| e.insert_smartart(crate::smartart::SmartArtKind::BasicProcess, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.chevron").icon("double_arrow").on(|e, _, cx| e.insert_smartart(crate::smartart::SmartArtKind::ContinuousChevronProcess, cx)).into(),
        MenuEntry::Separator,
        MenuItem::tr("pptx.insert.smartart.cycle").icon("autorenew").on(|e, _, cx| e.insert_smartart(crate::smartart::SmartArtKind::BasicCycle, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.hierarchy").icon("account_tree").on(|e, _, cx| e.insert_smartart(crate::smartart::SmartArtKind::OrganizationChart, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.relationship").icon("sync_alt").on(|e, _, cx| e.insert_smartart(crate::smartart::SmartArtKind::BasicRelationship, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.matrix").icon("grid_view").on(|e, _, cx| e.insert_smartart(crate::smartart::SmartArtKind::BasicMatrix, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.pyramid").icon("change_history").on(|e, _, cx| e.insert_smartart(crate::smartart::SmartArtKind::BasicPyramid, cx)).into(),
        MenuItem::tr("pptx.insert.smartart.picture").icon("insert_photo").on(|e, _, cx| e.insert_smartart(crate::smartart::SmartArtKind::PictureList, cx)).into(),
    ]
}


/// The kinds of picture a shape gallery draws (`shape_thumb`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeThumb {
    /// A closed preset shape (`rect`, `star5`…) as a new one is made: filled, with an outline.
    Preset(&'static str),
    /// A line or connector preset (`line`, `bentConnector3`, `curvedConnector3`…): an outline only, from the bottom left to the top
    /// right, with an arrow head at its end or not.
    Line { preset: &'static str, arrow: bool },
}

/// The picture of a shape in a gallery of the menus (挿入 > 図形 of each screen), `width` pixels wide and 4:3: the shape drawn by the
/// shapes' own drawing (the preset definitions of ECMA-376, `ibis_pptx::preset_geometry`), as it is when inserted (the fill and
/// outline of a new shape), inside a margin. The three screens' galleries are made of this one part.
pub fn shape_thumb(editor: &crate::editor::SlideEditor, thumb: ShapeThumb, width: f32) -> gpui_kit::AnyElement {
    use gpui_kit::{div, IntoElement as _, ParentElement as _, Styled as _};
    /// The picture's frame in EMU, 4:3 (the same unit the shapes are laid out in).
    const WIDTH_EMU: i64 = 4_572_000;
    let height_emu = WIDTH_EMU * 3 / 4;
    let scale = width / WIDTH_EMU as f32;
    // The margin: a tenth of the width at the sides and of the height above and below.
    let (mx, my) = (WIDTH_EMU / 10, height_emu / 10);
    let frame = EmuRect::new(mx, my, WIDTH_EMU - 2 * mx, height_emu - 2 * my);
    let fill_color = Color::opaque(Rgb(0x4472C4));
    let line_color = Color::opaque(Rgb(0x2F528F));
    let kind = match thumb {
        ShapeThumb::Preset(preset) => {
            let geometry = ibis_pptx::preset_geometry(preset).unwrap_or(Geometry::Rect);
            ShapeKind::Auto(AutoShape::new(geometry, Fill::Solid(fill_color), Some(Stroke::solid(line_color, 12_700))))
        }
        ShapeThumb::Line { preset, arrow } => {
            let mut stroke = Stroke::solid(line_color, 19_050);
            if arrow {
                stroke.tail = ibis_doc_model::LineEnd::parse(Some("triangle"), None, None);
            }
            match ibis_pptx::preset_geometry(preset).filter(|_| preset != "line") {
                Some(geometry) => ShapeKind::Auto(AutoShape::new(geometry, Fill::None, Some(stroke))),
                None => ShapeKind::Line(stroke),
            }
        }
    };
    let mut shape = Shape::new(ShapeId(u64::MAX), frame, kind);
    // A line runs from the bottom left to the top right, as Word's gallery draws them.
    if matches!(thumb, ShapeThumb::Line { .. }) {
        shape.flip_v = true;
    }
    div().size_full().child(ibis_office_render::shape_view::shape_element(&shape, frame, scale, &editor.resources, None)).into_any_element()
}
