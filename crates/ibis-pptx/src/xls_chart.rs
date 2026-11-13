// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The charts of an old workbook (`.xls`), from the public specification (MS-XLS): the records of a
//! chart's own stream (`BOF` of type chart … `EOF`) read into a plain description, and written as the
//! `c:chartSpace` of an `.xlsx` (with the values and texts of the cells the series name, as its cache:
//! the program draws a chart from them).
//!
//! What is read: the chart groups (bars and columns, lines, areas, pies and doughnuts, scatters and
//! radars; a group's grouping, gap and overlap), the series (name, values, categories or x values, the
//! colour of the series and of single points, a line's marker and smoothing), the titles (chart, category
//! axis, value axis), the legend and its place, and the axes' scale (minimum, maximum, major unit,
//! logarithmic, reversed) and gridlines. Left out: a secondary axis (its groups are drawn on the
//! first), 3-D views, trendlines, error bars and the text's fonts. A series' data labels are read for what
//! they show (value, percentage, category name, series name), not their place or their own text.

use crate::xls::{u16_at, Record};

/// A range of cells a series reads (an `AI` record's formula: a 3-D area or reference).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ref {
    /// The index of the `EXTERNSHEET` entry (which sheet).
    pub ixti: u16,
    pub rows: (u16, u16),
    pub columns: (u16, u16),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub(crate) enum Source {
    #[default]
    None,
    Ref(Ref),
    Text(String),
    /// Values the chart holds itself (a document's chart with its own data table).
    Lit(Vec<Cell>),
    /// The union of areas (`(Sheet!$C$6,Sheet!$E$6,…)`): their cells one after another.
    Refs(Vec<Ref>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Bar,
    Line,
    Area,
    Pie,
    Scatter,
    Radar,
}

#[derive(Clone, Debug)]
pub(crate) struct Group {
    pub kind: Kind,
    /// Bars: horizontal. Bars, lines, areas: stacked, or stacked to 100 %.
    pub horizontal: bool,
    pub stacked: bool,
    pub percent: bool,
    pub overlap: i16,
    pub gap: u16,
    /// Pies: the hole (percent; 0 is a pie) and the first slice's angle.
    pub hole: u16,
    pub angle: u16,
    /// Radars: filled.
    pub filled: bool,
}

/// Where the plot area is (`CrtLayout12A`, MS-XLS 2.4.66, as DrawingML's `c:manualLayout`): its left and top edges and its
/// width and height, fractions of the chart, of the inside of the axes (`inner`) or of the whole with the labels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlotLayout {
    pub inner: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Series {
    pub name: Source,
    pub values: Source,
    /// Categories; a scatter's x values.
    pub categories: Source,
    pub group: usize,
    pub fill: Option<u32>,
    pub line: Option<u32>,
    pub line_none: bool,
    pub marker: Option<u16>,
    pub marker_color: Option<u32>,
    /// The marker's border: `Some(None)` not shown (fNotShowBrd), `Some(Some(colour))` its colour, `None` automatic.
    pub marker_border: Option<Option<u32>>,
    /// The DrawingML of the series' look (line and fill) and of its marker, as Excel 2007 and later write it
    /// (`ShapePropsStream`): the content of an `a:spPr`, over what the records above give.
    pub sp_pr: Option<String>,
    pub marker_sp_pr: Option<String>,
    /// The same of single points (pie slices, bars).
    pub point_sp_pr: Vec<(u16, String)>,
    pub smooth: bool,
    /// What the series' data labels show (`ATTACHEDLABEL`'s flags, MS-XLS 2.4.19), when it has them.
    pub labels: Option<u16>,
    /// The data labels of their own (a TEXT of an OBJECTLINK to a data label, MS-XLS 2.4.181): the point (0xFFFF: every
    /// point) and its text, `None` for a label that shows what the flags say. Excel writes a label of fields (Excel 2013's
    /// `[分類名] [値]`) as that text, and prints it so from the .xls.
    pub label_texts: Vec<(u16, Option<String>)>,
    /// What stands between the parts of a data label (category, value, percentage; `DataLabExtContents`' separator).
    pub label_separator: Option<String>,
    /// A data label moved by hand (CrtLayout12, MS-XLS 2.4.52, in the label's text): the point and its x and y as DrawingML's
    /// manualLayout writes them (the move from the automatic place, in the fractions of the chart's width and height). The
    /// text's own x and y (the old way) are the same move in 1/4000 of the chart, plus the place of the automatic position
    /// that Excel computed; the layout gives the move alone.
    pub label_layouts: Vec<(u16, f64, f64)>,
    /// The colours of single points (a pie's slices).
    pub points: Vec<(u16, u32)>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Axis {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub major: Option<f64>,
    pub log: bool,
    pub reversed: bool,
    pub grid: bool,
    pub title: Option<String>,
    /// The tick labels' number format (`IFMTRECORD`, an index of the workbook's formats).
    pub format: Option<u16>,
    /// The display units (`YMult`): the labels show the values divided by this, as `c:dispUnits`.
    pub display_unit: Option<DisplayUnit>,
}

/// A value axis's display units ([MS-XLS] 2.4.343 YMult): a built-in one by its `axmid`, or a custom divisor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisplayUnit {
    BuiltIn(&'static str),
    Custom(f64),
}

impl DisplayUnit {
    /// From YMult's `axmid` (-1 custom, 0 none, 1 hundreds … 9 trillions, as ST_BuiltInUnit) and `numLabelMultiplier`.
    fn from_ymult(axmid: i16, multiplier: f64) -> Option<Self> {
        const UNITS: [&str; 9] = ["hundreds", "thousands", "tenThousands", "hundredThousands", "millions", "tenMillions", "hundredMillions", "billions", "trillions"];
        match axmid {
            -1 => (multiplier.is_finite() && multiplier > 0.0).then_some(DisplayUnit::Custom(multiplier)),
            1..=9 => Some(DisplayUnit::BuiltIn(UNITS[axmid as usize - 1])),
            _ => None,
        }
    }

    fn xml(self) -> String {
        match self {
            DisplayUnit::BuiltIn(unit) => format!("<c:dispUnits><c:builtInUnit val=\"{unit}\"/></c:dispUnits>"),
            DisplayUnit::Custom(value) => format!("<c:dispUnits><c:custUnit val=\"{}\"/></c:dispUnits>", ibis_calc::number_to_xml(value)),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Chart {
    pub title: Option<String>,
    pub groups: Vec<Group>,
    pub series: Vec<Series>,
    /// The legend's place (`b`, `tr`, `t`, `r`, `l`), if there is one.
    pub legend: Option<&'static str>,
    /// The category (x) axis and the value (y) axis.
    pub category: Axis,
    pub value: Axis,
    /// The secondary axes (the second `AXISPARENT`), and for each chart group whether it is drawn on them.
    pub category2: Axis,
    pub value2: Axis,
    pub group_secondary: Vec<bool>,
    /// The chart area's outline and fill: `Some(false)` when set to none (`FRAME`'s LINEFORMAT and AREAFORMAT), `None`
    /// when automatic.
    pub area_line: Option<bool>,
    pub area_fill: Option<bool>,
    /// The chart area's and the plot area's DrawingML (`ShapePropsStream` of their FRAMEs).
    pub area_sp_pr: Option<String>,
    pub plot_sp_pr: Option<String>,
    /// The plot area's place, when Excel 2007 or later placed it by hand.
    pub plot_layout: Option<PlotLayout>,
}

fn rgb(d: &[u8], at: usize) -> Option<u32> {
    Some(((*d.get(at)? as u32) << 16) | ((*d.get(at + 1)? as u32) << 8) | *d.get(at + 2)? as u32)
}

fn f64_at(d: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_le_bytes(d.get(at..at + 8)?.try_into().ok()?))
}

/// The reference of an `AI` record's formula: an area, or a union of them (a ptgMemFunc around areas and cells with
/// ptgUnion between them: corpus 061's line of C6, E6, G6, I6 and K6).
fn first_ref(rgce: &[u8]) -> Source {
    let mut refs = Vec::new();
    let mut at = 0;
    while let Some(&ptg) = rgce.get(at) {
        match ptg {
            // ptgMemFunc: the size of the expression it holds, then the expression.
            0x29 | 0x49 | 0x69 => at += 3,
            // ptgUnion, ptgParen.
            0x10 | 0x15 => at += 1,
            0x3A | 0x5A | 0x7A => {
                let (Some(ixti), Some(row), Some(col)) = (u16_at(rgce, at + 1), u16_at(rgce, at + 3), u16_at(rgce, at + 5)) else { return Source::None };
                let col = col & 0x3FFF;
                refs.push(Ref { ixti, rows: (row, row), columns: (col, col) });
                at += 7;
            }
            0x3B | 0x5B | 0x7B => {
                let (Some(ixti), Some(r1), Some(r2), Some(c1), Some(c2)) = (u16_at(rgce, at + 1), u16_at(rgce, at + 3), u16_at(rgce, at + 5), u16_at(rgce, at + 7), u16_at(rgce, at + 9)) else { return Source::None };
                refs.push(Ref { ixti, rows: (r1, r2), columns: (c1 & 0x3FFF, c2 & 0x3FFF) });
                at += 11;
            }
            _ => break,
        }
    }
    match refs.len() {
        0 => Source::None,
        1 => Source::Ref(refs.remove(0)),
        _ => Source::Refs(refs),
    }
}

/// A short unicode string (`SeriesText`: an id, then a length byte, flags and the characters).
fn series_text(d: &[u8]) -> Option<String> {
    let n = *d.get(2)? as usize;
    let wide = d.get(3)? & 1 != 0;
    let body = d.get(4..)?;
    Some(if wide {
        char::decode_utf16(body.chunks_exact(2).take(n).map(|c| u16::from_le_bytes([c[0], c[1]]))).map(|c| c.unwrap_or('\u{FFFD}')).collect()
    } else {
        body.iter().take(n).map(|&b| b as char).collect()
    })
}

/// What is being read inside a `BEGIN`…`END` pair.
#[derive(Clone, Copy, PartialEq)]
enum Block {
    Other,
    Series,
    Text,
    Axis(u16),
    DataFormat(u16, usize),
    /// A `FRAME`'s formats: the first is the chart area's.
    Frame(usize),
    /// An `AXISPARENT`: it holds the plot area's `POSITION`.
    AxisParent,
}

/// An axis of the chart by its `AXIS` kind (0 category, else value), the secondary ones marked with 0x100.
fn axis_of(chart: &mut Chart, kind: u16) -> &mut Axis {
    match (kind & 0xFF == 0, kind & 0x100 != 0) {
        (true, false) => &mut chart.category,
        (false, false) => &mut chart.value,
        (true, true) => &mut chart.category2,
        (false, true) => &mut chart.value2,
    }
}

/// The content of a ShapePropsStream's `a:spPr` (its children), without the XML declaration and the root.
fn sp_pr_inner(xml: &str) -> Option<String> {
    let start = xml.find("<a:spPr")?;
    let open_end = start + xml[start..].find('>')?;
    if xml[..open_end].ends_with('/') {
        return Some(String::new());
    }
    let close = xml.rfind("</a:spPr>")?;
    (close > open_end).then(|| xml[open_end + 1..close].to_string())
}

/// The colour of a GELFRAME's solid fill (its OfficeArtFOPT's fillType 0 and fillColor, an `OfficeArtCOLORREF` of RGB),
/// as RGB; `None` for another fill or no colour.
fn gel_frame_fill(d: &[u8]) -> Option<u32> {
    let mut props = crate::ppt::Props::default();
    for r in crate::ppt::records(d).into_iter().filter(|r| matches!(r.ty, 0xF00B | 0xF122)) {
        crate::ppt::Props::parse(&r, &mut props);
    }
    if props.get(0x0180).unwrap_or(0) != 0 || props.flag(0x01BF, 4) == Some(false) {
        return None;
    }
    let [r, g, b, flags] = props.get(0x0181)?.to_le_bytes();
    // Only an RGB (not an index of a scheme or palette, or a system colour).
    (flags & 0x1D == 0).then_some(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
}

/// Reads a chart's records (those between its `BOF` and `EOF`).
pub(crate) fn read_chart(records: &[&Record<'_>]) -> Option<Chart> {
    let mut chart = Chart::default();
    let mut stack: Vec<Block> = Vec::new();
    // The block a `BEGIN` will open, set by the record before it.
    let mut next = Block::Other;
    let mut text: Option<String> = None;
    let mut link: Option<u16> = None;
    // The separator of the data label being read (`DataLabExtContents`, in its TEXT).
    let mut label_separator: Option<String> = None;
    let mut label_layout: Option<(f64, f64)> = None;
    // The series and the point of a data label's object link.
    let mut link_point: (u16, u16) = (0, 0);
    let mut axis_kind: Option<u16> = None;
    // The axis group (`AXISPARENT`'s iax: 0 primary, 1 secondary) being read.
    let mut parent: u16 = 0;
    let mut grid_for: Option<u16> = None;
    let mut line_format_for_axis = false;
    let mut frames = 0usize;
    // The plot area's place as Excel 97-2003 writes it (`POSITION` in `AXISPARENT`), used when there is no `CrtLayout12A`.
    let mut plot_position: Option<PlotLayout> = None;
    // The chart's own values (MS-XLS 2.4.237 SIIndex): after an SIINDEX, the NUMBER and LABEL records hold the values (1),
    // the categories (2) or the bubble sizes (3) a series' BRAI says the chart holds itself (`rt` 1), one record a point:
    // its row is the point, its column the series (in the order of the SERIES records).
    let mut cache_of: u16 = 0;
    let mut cached: std::collections::BTreeMap<(u16, u16, u16), Cell> = std::collections::BTreeMap::new();
    for r in records {
        while chart.group_secondary.len() < chart.groups.len() {
            chart.group_secondary.push(parent == 1);
        }
        let d = r.data;
        let top = stack.last().copied().unwrap_or(Block::Other);
        match r.id {
            0x1065 => cache_of = u16_at(d, 0).unwrap_or(0),
            // CRTLAYOUT12A: the plot area's place (the first, of the primary axes): x and y as edges (mode 2), the width and
            // the height as sizes (mode 1), as DrawingML's xMode/yMode edge and wMode/hMode factor; other modes are automatic.
            0x1041 => {
                parent = u16_at(d, 0).unwrap_or(0);
                next = Block::AxisParent;
            }
            // POSITION of the axis group (MS-XLS 2.4.205): how the left top and the right bottom are given (2: relative to the
            // chart, in 1/4000 of its width and height; the second point as a size), then the point and the size.
            0x104F if top == Block::AxisParent && plot_position.is_none() => {
                if let (Some(2), Some(2), Some(x), Some(y), Some(w), Some(h)) = (u16_at(d, 0), u16_at(d, 2), crate::xls::u32_at(d, 4), crate::xls::u32_at(d, 8), crate::xls::u32_at(d, 12), crate::xls::u32_at(d, 16))
                    && w > 0
                    && h > 0
                {
                    let unit = |v: u32| f64::from(v as i32) / 4000.0;
                    plot_position = Some(PlotLayout { inner: true, x: unit(x), y: unit(y), width: unit(w), height: unit(h) });
                }
            }
            0x08A7 if chart.plot_layout.is_none() => {
                let modes: Vec<u16> = (0..4).filter_map(|k| u16_at(d, 26 + k * 2)).collect();
                if modes == [2, 2, 1, 1]
                    && let (Some(x), Some(y), Some(width), Some(height)) = (f64_at(d, 34), f64_at(d, 42), f64_at(d, 50), f64_at(d, 58))
                {
                    chart.plot_layout = Some(PlotLayout { inner: u16_at(d, 16).is_some_and(|f| f & 1 != 0), x, y, width, height });
                }
            }
            0x0203 | 0x0204 if cache_of != 0 => {
                let (Some(row), Some(column)) = (u16_at(d, 0), u16_at(d, 2)) else { continue };
                let cell = if r.id == 0x0203 {
                    d.get(6..14).map_or(Cell::Empty, |b| Cell::Number(f64::from_le_bytes(b.try_into().unwrap_or([0; 8]))))
                } else {
                    crate::xls::unicode_string(d, 6, true).map_or(Cell::Empty, |(t, _)| Cell::Text(t))
                };
                cached.insert((cache_of, column, row), cell);
            }
            0x1033 => stack.push(std::mem::replace(&mut next, Block::Other)),
            0x1034 => {
                if let Some(Block::Text) = stack.pop() {
                    // A data label's text, or its having none of its own.
                    if link == Some(4)
                        && let Some(series) = chart.series.get_mut(link_point.0 as usize)
                    {
                        series.label_texts.push((link_point.1, text.take()));
                        if let Some((x, y)) = label_layout {
                            series.label_layouts.push((link_point.1, x, y));
                        }
                        // The series' labels' separator (one of a single point's is not kept).
                        if link_point.1 == 0xFFFF && label_separator.is_some() {
                            series.label_separator = label_separator.clone();
                        }
                    }
                    label_separator = None;
                    label_layout = None;
                    // A title's text is known now: where it belongs is its object link.
                    // An axis title in the second AXISPARENT is a secondary axis's.
                    let secondary = parent == 1 && stack.contains(&Block::AxisParent);
                    if let (Some(t), Some(l)) = (text.take(), link.take()) {
                        match (l, secondary) {
                            (1, _) => chart.title = Some(t),
                            (2, false) => chart.value.title = Some(t),
                            (3, false) => chart.category.title = Some(t),
                            (2, true) => chart.value2.title = Some(t),
                            (3, true) => chart.category2.title = Some(t),
                            _ => {}
                        }
                    }
                    link = None;
                }
            }
            0x1003 => {
                chart.series.push(Series { group: 0, ..Series::default() });
                next = Block::Series;
            }
            0x1045 if matches!(top, Block::Series) => {
                if let (Some(s), Some(g)) = (chart.series.last_mut(), u16_at(d, 0)) {
                    s.group = g as usize;
                }
            }
            0x1051 if matches!(top, Block::Series) => {
                // AI: id (0 name, 1 values, 2 categories), the kind of link (2: a formula), …, the formula.
                let (Some(&id), Some(&rt)) = (d.first(), d.get(1)) else { continue };
                let cce = u16_at(d, 6).unwrap_or(0) as usize;
                let source = match rt {
                    2 => first_ref(d.get(8..8 + cce).unwrap_or(&[])),
                    // Held by the chart (its cache, read at the end), for the values and the categories.
                    1 if id == 1 || id == 2 => Source::Lit(Vec::new()),
                    _ => Source::None,
                };
                if let Some(s) = chart.series.last_mut() {
                    match id {
                        0 => s.name = source,
                        1 => s.values = source,
                        2 => s.categories = source,
                        _ => {}
                    }
                }
            }
            0x100D => {
                let t = series_text(d);
                if matches!(top, Block::Text) {
                    text = t;
                } else if matches!(top, Block::Series)
                    && let (Some(s), Some(t)) = (chart.series.last_mut(), t)
                    && s.name == Source::None
                {
                    s.name = Source::Text(t);
                }
            }
            0x1006 => {
                // DATAFORMAT: the point (0xFFFF: the series) and the series it formats.
                let (point, series) = (u16_at(d, 0).unwrap_or(0xFFFF), u16_at(d, 2).unwrap_or(0) as usize);
                next = Block::DataFormat(point, series);
            }
            0x1032 => {
                // FRAME: the chart area's (the first) or the plot area's.
                next = Block::Frame(frames);
                frames += 1;
            }
            0x100A if top == Block::Frame(0) => {
                // The chart area's fill: none when not automatic and of no pattern.
                if u16_at(d, 10).unwrap_or(1) & 1 == 0 {
                    chart.area_fill = Some(u16_at(d, 8).unwrap_or(1) != 0);
                }
            }
            0x1007 if top == Block::Frame(0) => {
                // The chart area's outline: none for the style 5.
                if u16_at(d, 8).unwrap_or(1) & 1 == 0 {
                    chart.area_line = Some(u16_at(d, 4) != Some(5));
                }
            }
            0x100A => {
                // AREAFORMAT: foreground colour, pattern, automatic.
                if let Block::DataFormat(point, series) = top {
                    let auto = u16_at(d, 10).unwrap_or(1) & 1 != 0;
                    let pattern = u16_at(d, 8).unwrap_or(0);
                    if !auto
                        && pattern != 0
                        && let Some(c) = rgb(d, 0)
                        && let Some(s) = chart.series.get_mut(series)
                    {
                        if point == 0xFFFF {
                            s.fill = Some(c);
                        } else {
                            s.points.push((point, c));
                        }
                    }
                }
            }
            0x08A4 => {
                // ShapePropsStream (MS-XLS 2.4.250): the element's look as DrawingML (Excel 2007 and later), after its
                // FrtHeader (12 bytes), the context (0: the element; 1: a series' marker), a checksum and the XML's length.
                // A series': its line and fill; a FRAME's: the chart area (the first) or the plot area.
                let (Some(context), Some(cb)) = (u16_at(d, 12), d.get(20..24).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)) else { continue };
                let Some(inner) = d.get(24..24 + cb).and_then(|x| sp_pr_inner(&String::from_utf8_lossy(x))) else { continue };
                match top {
                    Block::DataFormat(point, series) => {
                        if let Some(s) = chart.series.get_mut(series) {
                            match (context, point) {
                                (0, 0xFFFF) => s.sp_pr = Some(inner),
                                (0, p) => s.point_sp_pr.push((p, inner)),
                                (1, 0xFFFF) => s.marker_sp_pr = Some(inner),
                                _ => {}
                            }
                        }
                    }
                    Block::Frame(0) if context == 0 => chart.area_sp_pr = Some(inner),
                    Block::Frame(1) if context == 0 => chart.plot_sp_pr = Some(inner),
                    _ => {}
                }
            }
            0x1066 => {
                // GELFRAME (MS-XLS 2.4.128): the fill as OfficeArt properties, over AREAFORMAT's (Excel 2007 and later write
                // the area's colour here and black in AREAFORMAT: corpus 145's bars). A solid fill's colour is fillColor.
                if let Block::DataFormat(point, series) = top
                    && let Some(c) = gel_frame_fill(d)
                    && let Some(s) = chart.series.get_mut(series)
                {
                    if point == 0xFFFF {
                        s.fill = Some(c);
                    } else {
                        s.points.retain(|(p, _)| *p != point);
                        s.points.push((point, c));
                    }
                }
            }
            0x1007 => {
                // LINEFORMAT: colour, style (5: none), weight, automatic.
                if let Block::DataFormat(point, series) = top
                    && point == 0xFFFF
                    && let Some(s) = chart.series.get_mut(series)
                {
                    let auto = u16_at(d, 8).unwrap_or(1) & 1 != 0;
                    if u16_at(d, 4) == Some(5) {
                        s.line_none = true;
                    } else if !auto {
                        s.line = rgb(d, 0);
                    }
                }
            }
            0x1009 => {
                // MARKERFORMAT: the kind of marker (0: none).
                if let Block::DataFormat(point, series) = top
                    && point == 0xFFFF
                    && let Some(s) = chart.series.get_mut(series)
                {
                    s.marker = u16_at(d, 8);
                    // MarkerFormat (MS-XLS 2.4.173): rgbFore is the marker's border, rgbBack its inside; the flags after imk are
                    // fAuto (1), fNotShowInt (0x10), fNotShowBrd (0x20). The marker is filled with the inside's colour (corpus 145's
                    // dots: border black and not shown, inside green, blue, ...), or the border's when the inside is not shown.
                    let flags = u16_at(d, 10).unwrap_or(1);
                    s.marker_color = if flags & 1 != 0 { None } else if flags & 0x10 == 0 { rgb(d, 4) } else { rgb(d, 0) };
                    s.marker_border = if flags & 1 != 0 { None } else if flags & 0x20 != 0 { Some(None) } else { Some(rgb(d, 0)) };
                }
            }
            0x100C => {
                // ATTACHEDLABEL: the data labels of a series (its DATAFORMAT of every point).
                if let Block::DataFormat(0xFFFF, series) = top
                    && let (Some(s), Some(flags)) = (chart.series.get_mut(series), u16_at(d, 0))
                    && flags & 0x77 != 0
                {
                    s.labels = Some(flags);
                }
            }
            0x105D => {
                // SERFMT: smoothed line.
                if let Some(s) = chart.series.last_mut() {
                    s.smooth = d.first().is_some_and(|b| b & 1 != 0);
                }
            }
            0x1014 => {} // CHARTFORMAT: its first record of a kind starts the group below.
            0x1017 => chart.groups.push(Group {
                kind: Kind::Bar,
                horizontal: d.get(4).is_some_and(|b| b & 1 != 0),
                stacked: d.get(4).is_some_and(|b| b & 2 != 0),
                percent: d.get(4).is_some_and(|b| b & 4 != 0),
                overlap: u16_at(d, 0).unwrap_or(0) as i16,
                gap: u16_at(d, 2).unwrap_or(150),
                hole: 0,
                angle: 0,
                filled: false,
            }),
            0x1018 | 0x101A => chart.groups.push(Group {
                kind: if r.id == 0x1018 { Kind::Line } else { Kind::Area },
                horizontal: false,
                stacked: d.first().is_some_and(|b| b & 1 != 0),
                percent: d.first().is_some_and(|b| b & 2 != 0),
                overlap: 0,
                gap: 0,
                hole: 0,
                angle: 0,
                filled: false,
            }),
            0x1019 => chart.groups.push(Group { kind: Kind::Pie, horizontal: false, stacked: false, percent: false, overlap: 0, gap: 0, hole: u16_at(d, 2).unwrap_or(0), angle: u16_at(d, 0).unwrap_or(0), filled: false }),
            0x101B => chart.groups.push(Group { kind: Kind::Scatter, horizontal: false, stacked: false, percent: false, overlap: 0, gap: 0, hole: 0, angle: 0, filled: false }),
            0x103E | 0x1040 => chart.groups.push(Group { kind: Kind::Radar, horizontal: false, stacked: false, percent: false, overlap: 0, gap: 0, hole: 0, angle: 0, filled: r.id == 0x1040 }),
            0x1015 => {
                // LEGEND: where it is docked.
                chart.legend = Some(match d.get(16) {
                    Some(0) => "b",
                    Some(1) => "tr",
                    Some(2) => "t",
                    Some(4) => "l",
                    _ => "r",
                });
            }
            0x1025 => {
                next = Block::Text;
                text = None;
                link = None;
            }
            // DataLabExtContents (MS-XLS 2.4.60): after its FrtHeader (12 bytes) the flags of what is shown (2), then the
            // separator as a length (2), a flag byte (the characters are 16 bits when 1) and the characters.
            0x086B if matches!(top, Block::Text) => {
                if let (Some(n), Some(&wide)) = (u16_at(d, 14), d.get(16)) {
                    let n = n as usize;
                    let body = d.get(17..).unwrap_or(&[]);
                    label_separator = Some(if wide & 1 != 0 {
                        char::decode_utf16(body.chunks_exact(2).take(n).map(|c| u16::from_le_bytes([c[0], c[1]]))).map(|c| c.unwrap_or('\u{FFFD}')).collect()
                    } else {
                        body.iter().take(n).map(|&b| b as char).collect()
                    });
                }
            }
            // CrtLayout12 (MS-XLS 2.4.52) of a data label: after its FrtHeader (12 bytes), four modes of two bytes are
            // followed by x, y, width and height as doubles; here x and y at 26 and 34 are the move (measured: corpus 104's
            // are the values of the xlsx's c:manualLayout, and a probe of five pies by Excel gives the same).
            0x089D if matches!(top, Block::Text) => {
                if let (Some(x), Some(y)) = (f64_at(d, 26), f64_at(d, 34)) {
                    label_layout = Some((x, y));
                }
            }
            0x1027 if matches!(top, Block::Text) => {
                link = u16_at(d, 0);
                link_point = (u16_at(d, 2).unwrap_or(0), u16_at(d, 4).unwrap_or(0));
            }
            0x101D => {
                // AXIS: 0 category, 1 value.
                axis_kind = u16_at(d, 0);
                // (The secondary axes' kinds are marked with 0x100.)
                next = Block::Axis(axis_kind.unwrap_or(9) | if parent == 1 { 0x100 } else { 0 });
            }
            0x101F => {
                // VALUERANGE: minimum, maximum, major unit, minor unit, crossing, flags.
                if let Block::Axis(kind) = top {
                    let flags = u16_at(d, 40).unwrap_or(0xFF);
                    let axis = axis_of(&mut chart, kind);
                    if flags & 1 == 0 {
                        axis.min = f64_at(d, 0);
                    }
                    if flags & 2 == 0 {
                        axis.max = f64_at(d, 8);
                    }
                    if flags & 4 == 0 {
                        axis.major = f64_at(d, 16).filter(|m| *m > 0.0);
                    }
                    axis.log = flags & 0x20 != 0;
                    axis.reversed = flags & 0x40 != 0;
                }
            }
            // YMULT ([MS-XLS] 2.4.343) in a value axis: FrtHeaderOld (4 bytes), axmid, numLabelMultiplier, flags. Its display
            // units label (the AXM's ATTACHEDLABEL) is not drawn.
            0x0857 => {
                if let Block::Axis(kind) = top
                    && kind & 0xFF != 0
                    && let (Some(axmid), Some(multiplier)) = (u16_at(d, 4), f64_at(d, 6))
                {
                    axis_of(&mut chart, kind).display_unit = DisplayUnit::from_ymult(axmid as i16, multiplier);
                }
            }
            0x104E => {
                // IFMTRECORD in an axis: its tick labels' number format.
                if let Block::Axis(kind) = top {
                    axis_of(&mut chart, kind).format = u16_at(d, 0);
                }
            }
            0x1021 => {
                // AXISLINEFORMAT: 0 the axis line, 1 major gridlines.
                grid_for = if let Block::Axis(kind) = top { u16_at(d, 0).filter(|&i| i == 1).map(|_| kind) } else { None };
                line_format_for_axis = true;
                if let Some(kind) = grid_for {
                    axis_of(&mut chart, kind).grid = true;
                }
            }
            _ => {}
        }
    }
    let _ = (axis_kind, grid_for, line_format_for_axis);
    while chart.group_secondary.len() < chart.groups.len() {
        chart.group_secondary.push(parent == 1);
    }
    for (k, s) in chart.series.iter_mut().enumerate() {
        for (index, source) in [(1u16, &mut s.values), (2, &mut s.categories)] {
            if let Source::Lit(values) = source {
                let points: Vec<(u16, &Cell)> = cached.range((index, k as u16, 0)..=(index, k as u16, u16::MAX)).map(|(&(_, _, row), c)| (row, c)).collect();
                let count = points.last().map_or(0, |&(row, _)| row as usize + 1);
                *values = vec![Cell::Empty; count];
                for (row, c) in points {
                    values[row as usize] = c.clone();
                }
                if values.is_empty() {
                    *source = Source::None;
                }
            }
        }
    }
    if chart.plot_layout.is_none() {
        chart.plot_layout = plot_position;
    }
    // A series with no values is left out; a chart none of whose series has values (no points: SERIES' count of values 0)
    // is kept as an empty chart, whose frame, axes and gridlines Excel draws (corpus 145's second chart of a sheet).
    if chart.series.iter().any(|s| s.values != Source::None) {
        chart.series.retain(|s| s.values != Source::None);
    } else {
        for s in &mut chart.series {
            s.values = Source::Lit(Vec::new());
        }
    }
    if chart.groups.is_empty() || chart.series.is_empty() {
        return None;
    }
    Some(chart)
}

/// The cells a [`Ref`] covers, as the caller reads them.
pub(crate) trait Cells {
    /// The sheet's name and the values of the range: row by row, a number, a text or nothing.
    fn range(&self, r: &Ref) -> Option<(String, Vec<Cell>)>;
    /// A colour of the workbook's palette (the old charts' automatic colours are some of them: 16 and on
    /// for bars, areas and pie slices, 24 and on for lines).
    fn color(&self, index: usize) -> u32;
    /// The code of the workbook's number format `index` (a built-in one's, or the file's own).
    fn format_code(&self, _index: u16) -> Option<String> {
        None
    }
    /// The number format of a range's first cell (what a chart linked to its source shows the values in), `None` for General.
    fn range_format(&self, _r: &Ref) -> Option<String> {
        None
    }
    /// The text of a single cell reference.
    fn text(&self, r: &Ref) -> Option<String> {
        match self.range(r)?.1.into_iter().next()? {
            Cell::Text(t) => Some(t),
            Cell::Number(n) => Some(ibis_calc::number_to_xml(n)),
            Cell::Empty => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Cell {
    Number(f64),
    Text(String),
    Empty,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn column_name(mut c: u32) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (c % 26) as u8) as char);
        if c < 26 {
            break;
        }
        c = c / 26 - 1;
    }
    s
}

fn formula_of(name: &str, r: &Ref) -> String {
    let sheet = if name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') && !name.is_empty() { name.to_string() } else { format!("'{}'", name.replace('\'', "''")) };
    let (a, b) = (format!("${}${}", column_name(r.columns.0 as u32), r.rows.0 as u32 + 1), format!("${}${}", column_name(r.columns.1 as u32), r.rows.1 as u32 + 1));
    if a == b { format!("{sheet}!{a}") } else { format!("{sheet}!{a}:{b}") }
}

fn rich(text: &str) -> String {
    format!("<c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/>", esc(text))
}

fn solid(color: u32) -> String {
    format!("<a:solidFill><a:srgbClr val=\"{color:06X}\"/></a:solidFill>")
}

fn num_cache(cells: &[Cell]) -> String {
    num_cache_in(cells, None)
}

/// The cache of numbers, in the number format of the cells they come from.
fn num_cache_in(cells: &[Cell], format: Option<&str>) -> String {
    let mut pts = String::new();
    for (i, c) in cells.iter().enumerate() {
        match c {
            Cell::Number(n) => pts += &format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", ibis_calc::number_to_xml(*n)),
            Cell::Text(t) => {
                if let Ok(n) = t.trim().parse::<f64>() {
                    pts += &format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", ibis_calc::number_to_xml(n));
                }
            }
            Cell::Empty => {}
        }
    }
    format!("<c:numCache><c:formatCode>{}</c:formatCode><c:ptCount val=\"{}\"/>{pts}</c:numCache>", esc(format.unwrap_or("General")), cells.len())
}

fn str_cache(cells: &[Cell]) -> String {
    let mut pts = String::new();
    for (i, c) in cells.iter().enumerate() {
        let t = match c {
            Cell::Number(n) => ibis_calc::number_to_xml(*n),
            Cell::Text(t) => t.clone(),
            Cell::Empty => continue,
        };
        pts += &format!("<c:pt idx=\"{i}\"><c:v>{}</c:v></c:pt>", esc(&t));
    }
    format!("<c:strCache><c:ptCount val=\"{}\"/>{pts}</c:strCache>", cells.len())
}

fn axis_scaling(a: &Axis, value: bool) -> String {
    let mut s = String::from("<c:scaling>");
    if a.log {
        s += "<c:logBase val=\"10\"/>";
    }
    s += &format!("<c:orientation val=\"{}\"/>", if a.reversed { "maxMin" } else { "minMax" });
    if value {
        if let Some(m) = a.max {
            s += &format!("<c:max val=\"{}\"/>", ibis_calc::number_to_xml(m));
        }
        if let Some(m) = a.min {
            s += &format!("<c:min val=\"{}\"/>", ibis_calc::number_to_xml(m));
        }
    }
    s + "</c:scaling>"
}

fn marker_symbol(imk: u16) -> &'static str {
    match imk {
        0 => "none",
        1 => "square",
        2 => "diamond",
        3 => "triangle",
        4 => "x",
        5 => "star",
        6 => "dot",
        7 => "dash",
        8 => "circle",
        9 => "plus",
        _ => "auto",
    }
}

/// The `c:chartSpace` of a chart; `cells` reads the cells its series name, `default_name` gives the name of
/// a series with none (its number from 1).
pub(crate) fn chart_xml(chart: &Chart, cells: &dyn Cells, default_name: &dyn Fn(usize) -> String) -> String {
    let scatter = chart.groups.iter().any(|g| g.kind == Kind::Scatter);
    let pie = chart.groups.iter().all(|g| g.kind == Kind::Pie);
    let mut plot = String::new();
    for (gi, g) in chart.groups.iter().enumerate() {
        let mut sers = String::new();
        for (si, s) in chart.series.iter().enumerate().filter(|(_, s)| s.group.min(chart.groups.len() - 1) == gi) {
            let mut x = format!("<c:ser><c:idx val=\"{si}\"/><c:order val=\"{si}\"/>");
            // The name.
            let name_xml = match &s.name {
                Source::Ref(r) => match (cells.range(r), cells.text(r)) {
                    (Some((sheet, _)), Some(t)) => format!("<c:tx><c:strRef><c:f>{}</c:f>{}</c:strRef></c:tx>", esc(&formula_of(&sheet, r)), str_cache(&[Cell::Text(t)])),
                    _ => String::new(),
                },
                Source::Text(t) => format!("<c:tx><c:v>{}</c:v></c:tx>", esc(t)),
                Source::Lit(v) => format!("<c:tx><c:v>{}</c:v></c:tx>", esc(&v.iter().find_map(|c| match c { Cell::Text(t) => Some(t.clone()), Cell::Number(n) => Some(ibis_calc::number_to_xml(*n)), Cell::Empty => None }).unwrap_or_default())),
                Source::None | Source::Refs(_) => format!("<c:tx><c:v>{}</c:v></c:tx>", esc(&default_name(si + 1))),
            };
            x += &name_xml;
            // The look: the colours the file names, else the automatic ones of Excel 97-2003.
            let mut sp = String::new();
            let line_like = matches!(g.kind, Kind::Line | Kind::Scatter);
            let area_color = s.fill.unwrap_or_else(|| cells.color(16 + si % 16));
            let line_color = s.line.unwrap_or_else(|| cells.color(24 + si % 16));
            if line_like {
                if s.line_none {
                    sp += "<a:ln><a:noFill/></a:ln>";
                } else {
                    sp += &format!("<a:ln w=\"28575\" cap=\"rnd\">{}<a:round/></a:ln>", solid(line_color));
                }
            } else if g.kind == Kind::Radar && !g.filled {
                sp += &format!("<a:ln w=\"28575\">{}</a:ln>", solid(line_color));
            } else if g.kind != Kind::Pie {
                sp += &solid(area_color);
            }
            if let Some(own) = &s.sp_pr {
                sp = own.clone();
            }
            if !sp.is_empty() {
                x += &format!("<c:spPr>{sp}</c:spPr>");
            }
            if line_like || g.kind == Kind::Radar && !g.filled {
                let symbol = s.marker.map(marker_symbol).unwrap_or(if g.kind == Kind::Scatter { "auto" } else { "none" });
                let marker_color = s.marker_color.or(s.line).unwrap_or_else(|| cells.color(24 + si % 16));
                if symbol == "none" {
                    x += "<c:marker><c:symbol val=\"none\"/></c:marker>";
                } else {
                    let border = match s.marker_border {
                        Some(None) => "<a:ln><a:noFill/></a:ln>".to_string(),
                        Some(Some(c)) => format!("<a:ln w=\"9525\">{}</a:ln>", solid(c)),
                        None => String::new(),
                    };
                    let look = s.marker_sp_pr.clone().unwrap_or_else(|| format!("{}{border}", solid(marker_color)));
                    x += &format!("<c:marker><c:symbol val=\"{symbol}\"/><c:size val=\"7\"/><c:spPr>{look}</c:spPr></c:marker>");
                }
            }
            if g.kind == Kind::Pie {
                let count = match &s.values {
                    Source::Ref(r) => cells.range(r).map_or(0, |(_, v)| v.len()),
                    Source::Refs(refs) => refs.iter().map(|r| cells.range(r).map_or(0, |(_, v)| v.len())).sum(),
                    _ => 0,
                };
                let count = if let Source::Lit(v) = &s.values { v.len() } else { count };
                for p in 0..count {
                    let c = s.points.iter().find(|(i, _)| *i as usize == p).map_or_else(|| cells.color(16 + p % 16), |(_, c)| *c);
                    let look = s.point_sp_pr.iter().find(|(i, _)| *i as usize == p).map_or_else(|| solid(c), |(_, x)| x.clone());
                    x += &format!("<c:dPt><c:idx val=\"{p}\"/><c:bubble3D val=\"0\"/><c:spPr>{look}</c:spPr></c:dPt>");
                }
            }
            // The data labels: fShowValue (1), fShowPercent (2), fShowLabelAndPerc (4: the category and the
            // percentage), fShowLabel (0x10: the category), fShowBubbleSizes (0x20), fShowSeriesName (0x40).
            if let Some(f) = s.labels {
                let on = |bits: u16| if f & bits != 0 { 1 } else { 0 };
                let shows = format!(
                    "<c:showLegendKey val=\"0\"/><c:showVal val=\"{}\"/><c:showCatName val=\"{}\"/><c:showSerName val=\"{}\"/><c:showPercent val=\"{}\"/><c:showBubbleSize val=\"{}\"/>",
                    on(0x01),
                    on(0x14),
                    on(0x40),
                    on(0x06),
                    on(0x20)
                );
                // The labels of their own texts: a point's, or every point's where the point has no label of its own.
                let points = match &s.values {
                    Source::Ref(r) => cells.range(r).map_or(0, |(_, v)| v.len()),
                    Source::Refs(refs) => refs.iter().map(|r| cells.range(r).map_or(0, |(_, v)| v.len())).sum(),
                    Source::Lit(v) => v.len(),
                    _ => 0,
                };
                let every = s.label_texts.iter().find(|(p, _)| *p == 0xFFFF).and_then(|(_, t)| t.as_ref());
                let mut own = String::new();
                for p in 0..points as u16 {
                    let text = match s.label_texts.iter().find(|(q, _)| *q == p) {
                        Some((_, t)) => t.as_ref(),
                        None => every,
                    };
                    let layout = s
                        .label_layouts
                        .iter()
                        .find(|(q, _, _)| *q == p)
                        .map(|(_, x, y)| format!("<c:layout><c:manualLayout><c:x val=\"{x}\"/><c:y val=\"{y}\"/></c:manualLayout></c:layout>"))
                        .unwrap_or_default();
                    if text.is_none() && !layout.is_empty() {
                        own += &format!("<c:dLbl><c:idx val=\"{p}\"/>{layout}{shows}</c:dLbl>");
                    }
                    if let Some(t) = text {
                        let paragraphs: String = t.split('\n').map(|line| format!("<a:p><a:r><a:t>{}</a:t></a:r></a:p>", esc(line.trim_end_matches('\r')))).collect();
                        own += &format!("<c:dLbl><c:idx val=\"{p}\"/>{layout}<c:tx><c:rich><a:bodyPr/><a:lstStyle/>{paragraphs}</c:rich></c:tx>{shows}</c:dLbl>");
                    }
                }
                let separator = s.label_separator.as_ref().map(|t| format!("<c:separator>{}</c:separator>", esc(t))).unwrap_or_default();
                x += &format!("<c:dLbls>{own}{shows}{separator}</c:dLbls>");
            }
            // The categories (or a scatter's x values) and the values.
            let numbers = |r: &Ref| -> String {
                match cells.range(r) {
                    Some((sheet, v)) => format!("<c:numRef><c:f>{}</c:f>{}</c:numRef>", esc(&formula_of(&sheet, r)), num_cache_in(&v, cells.range_format(r).as_deref())),
                    None => String::new(),
                }
            };
            let strings = |r: &Ref| -> String {
                match cells.range(r) {
                    Some((sheet, v)) => format!("<c:strRef><c:f>{}</c:f>{}</c:strRef>", esc(&formula_of(&sheet, r)), str_cache(&v)),
                    None => String::new(),
                }
            };
            let num_lit = |v: &[Cell]| format!("<c:numLit>{}</c:numLit>", num_cache(v).trim_start_matches("<c:numCache>").trim_end_matches("</c:numCache>"));
            let str_lit = |v: &[Cell]| format!("<c:strLit>{}</c:strLit>", str_cache(v).trim_start_matches("<c:strCache>").trim_end_matches("</c:strCache>"));
            if let Source::Lit(values) = &s.values {
                if g.kind == Kind::Scatter {
                    if let Source::Lit(cat) = &s.categories {
                        x += &format!("<c:xVal>{}</c:xVal>", num_lit(cat));
                    }
                    x += &format!("<c:yVal>{}</c:yVal>", num_lit(values));
                } else {
                    if let Source::Lit(cat) = &s.categories {
                        x += &format!("<c:cat>{}</c:cat>", str_lit(cat));
                    }
                    x += &format!("<c:val>{}</c:val>", num_lit(values));
                }
                x += "</c:ser>";
                sers += &x;
                continue;
            }
            // A union: the formula of its areas, and their cells one after another.
            let union = |refs: &[Ref], tag: &str, cache: &dyn Fn(&[Cell]) -> String| -> String {
                let mut parts = Vec::new();
                let mut all = Vec::new();
                for r in refs {
                    let Some((sheet, v)) = cells.range(r) else { return String::new() };
                    parts.push(formula_of(&sheet, r));
                    all.extend(v);
                }
                format!("<c:{tag}><c:f>({})</c:f>{}</c:{tag}>", esc(&parts.join(",")), cache(&all))
            };
            if let Source::Refs(values) = &s.values {
                let cat = match &s.categories {
                    Source::Ref(r) => Some(if g.kind == Kind::Scatter { numbers(r) } else { strings(r) }),
                    Source::Refs(r) => Some(if g.kind == Kind::Scatter { union(r, "numRef", &num_cache) } else { union(r, "strRef", &str_cache) }),
                    _ => None,
                };
                let (cat_tag, val_tag) = if g.kind == Kind::Scatter { ("xVal", "yVal") } else { ("cat", "val") };
                if let Some(cat) = cat {
                    x += &format!("<c:{cat_tag}>{cat}</c:{cat_tag}>");
                }
                x += &format!("<c:{val_tag}>{}</c:{val_tag}>", union(values, "numRef", &num_cache));
                x += "</c:ser>";
                sers += &x;
                continue;
            }
            let Source::Ref(values) = &s.values else { continue };
            if g.kind == Kind::Scatter {
                if let Source::Ref(cat) = &s.categories {
                    x += &format!("<c:xVal>{}</c:xVal>", numbers(cat));
                }
                x += &format!("<c:yVal>{}</c:yVal>", numbers(values));
                if s.smooth {
                    x += "<c:smooth val=\"1\"/>";
                }
            } else {
                if let Source::Ref(cat) = &s.categories {
                    x += &format!("<c:cat>{}</c:cat>", strings(cat));
                }
                x += &format!("<c:val>{}</c:val>", numbers(values));
                if g.kind == Kind::Line && s.smooth {
                    x += "<c:smooth val=\"1\"/>";
                }
            }
            x += "</c:ser>";
            sers += &x;
        }
        let grouping = if g.percent { "percentStacked" } else if g.stacked { "stacked" } else { "standard" };
        let secondary = chart.group_secondary.get(gi).copied().unwrap_or(false) && chart.group_secondary.iter().any(|s| !s);
        let axes = if secondary { "<c:axId val=\"333\"/><c:axId val=\"444\"/>" } else { "<c:axId val=\"111\"/><c:axId val=\"222\"/>" };
        plot += &match g.kind {
            Kind::Bar => {
                let overlap = if g.stacked || g.percent { 100 } else { g.overlap.clamp(-100, 100) };
                format!("<c:barChart><c:barDir val=\"{}\"/><c:grouping val=\"{}\"/><c:varyColors val=\"0\"/>{sers}<c:gapWidth val=\"{}\"/><c:overlap val=\"{overlap}\"/>{axes}</c:barChart>", if g.horizontal { "bar" } else { "col" }, if g.percent { "percentStacked" } else if g.stacked { "stacked" } else { "clustered" }, g.gap.min(500))
            }
            Kind::Line => format!("<c:lineChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{sers}<c:marker val=\"1\"/>{axes}</c:lineChart>"),
            Kind::Area => format!("<c:areaChart><c:grouping val=\"{grouping}\"/><c:varyColors val=\"0\"/>{sers}{axes}</c:areaChart>"),
            Kind::Pie if g.hole > 0 => format!("<c:doughnutChart><c:varyColors val=\"1\"/>{sers}<c:firstSliceAng val=\"{}\"/><c:holeSize val=\"{}\"/></c:doughnutChart>", g.angle % 360, g.hole.clamp(10, 90)),
            Kind::Pie => format!("<c:pieChart><c:varyColors val=\"1\"/>{sers}<c:firstSliceAng val=\"{}\"/></c:pieChart>", g.angle % 360),
            Kind::Scatter => format!("<c:scatterChart><c:scatterStyle val=\"lineMarker\"/><c:varyColors val=\"0\"/>{sers}{axes}</c:scatterChart>"),
            Kind::Radar => format!("<c:radarChart><c:radarStyle val=\"{}\"/><c:varyColors val=\"0\"/>{sers}{axes}</c:radarChart>", if g.filled { "filled" } else { "marker" }),
        };
    }
    let mut axes = String::new();
    if !pie {
        let horizontal = chart.groups.iter().any(|g| g.kind == Kind::Bar && g.horizontal);
        let title = |a: &Axis| a.title.as_ref().map(|t| format!("<c:title>{}</c:title>", rich(t))).unwrap_or_default();
        let grid = |a: &Axis| if a.grid { "<c:majorGridlines/>" } else { "" };
        // The tick labels' format (IFMTRECORD: corpus 145's 0%), else the source's.
        let num_fmt = |a: &Axis| match a.format.and_then(|i| cells.format_code(i)) {
            Some(code) => format!("<c:numFmt formatCode=\"{}\" sourceLinked=\"0\"/>", esc(&code)),
            None => "<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>".to_string(),
        };
        let (cat_pos, val_pos) = if horizontal { ("l", "b") } else { ("b", "l") };
        if scatter {
            axes += &format!("<c:valAx><c:axId val=\"111\"/>{}<c:delete val=\"0\"/><c:axPos val=\"{cat_pos}\"/>{}{}{}<c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"222\"/><c:crosses val=\"autoZero\"/></c:valAx>", axis_scaling(&chart.category, true), grid(&chart.category), title(&chart.category), num_fmt(&chart.category));
        } else {
            axes += &format!("<c:catAx><c:axId val=\"111\"/>{}<c:delete val=\"0\"/><c:axPos val=\"{cat_pos}\"/>{}{}<c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"222\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/></c:catAx>", axis_scaling(&chart.category, false), grid(&chart.category), title(&chart.category));
        }
        let major = chart.value.major.map(|m| format!("<c:majorUnit val=\"{}\"/>", ibis_calc::number_to_xml(m))).unwrap_or_default();
        axes += &format!("<c:valAx><c:axId val=\"222\"/>{}<c:delete val=\"0\"/><c:axPos val=\"{val_pos}\"/>{}{}{}<c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"111\"/><c:crosses val=\"autoZero\"/>{major}{}</c:valAx>", axis_scaling(&chart.value, true), grid(&chart.value), title(&chart.value), num_fmt(&chart.value), chart.value.display_unit.map(DisplayUnit::xml).unwrap_or_default());
    }
    let title = chart.title.as_ref().map(|t| format!("<c:title>{}</c:title>", rich(t))).unwrap_or_default();
    // The secondary axes: a hidden category axis and a value axis on the other side, crossing at the end.
    if !pie && !scatter && chart.group_secondary.iter().any(|s| *s) && chart.group_secondary.iter().any(|s| !s) {
        let horizontal = chart.groups.iter().any(|g| g.kind == Kind::Bar && g.horizontal);
        let (cat_pos, val_pos) = if horizontal { ("l", "t") } else { ("b", "r") };
        let code = match chart.value2.format.and_then(|i| cells.format_code(i)) {
            Some(code) => format!("<c:numFmt formatCode=\"{}\" sourceLinked=\"0\"/>", esc(&code)),
            None => "<c:numFmt formatCode=\"General\" sourceLinked=\"1\"/>".to_string(),
        };
        let major = chart.value2.major.map(|m| format!("<c:majorUnit val=\"{}\"/>", ibis_calc::number_to_xml(m))).unwrap_or_default();
        axes += &format!("<c:valAx><c:axId val=\"444\"/>{}<c:delete val=\"0\"/><c:axPos val=\"{val_pos}\"/>{}{code}<c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"333\"/><c:crosses val=\"max\"/>{major}{}</c:valAx>", axis_scaling(&chart.value2, true), chart.value2.title.as_ref().map(|t| format!("<c:title>{}</c:title>", rich(t))).unwrap_or_default(), chart.value2.display_unit.map(DisplayUnit::xml).unwrap_or_default());
        axes += &format!("<c:catAx><c:axId val=\"333\"/>{}<c:delete val=\"1\"/><c:axPos val=\"{cat_pos}\"/><c:numFmt formatCode=\"General\" sourceLinked=\"0\"/><c:tickLblPos val=\"nextTo\"/><c:crossAx val=\"444\"/><c:crosses val=\"autoZero\"/><c:auto val=\"1\"/><c:lblAlgn val=\"ctr\"/><c:lblOffset val=\"100\"/></c:catAx>", axis_scaling(&chart.category2, false));
    }
    let legend = chart.legend.map(|p| format!("<c:legend><c:legendPos val=\"{p}\"/><c:overlay val=\"0\"/></c:legend>")).unwrap_or_default();
    let plot_sp_pr = chart.plot_sp_pr.as_ref().map(|x| format!("<c:spPr>{x}</c:spPr>")).unwrap_or_default();
    // The chart area set to no fill or no outline (an automatic one is the reader's default).
    let area = match (chart.area_fill == Some(false), chart.area_line == Some(false)) {
        _ if chart.area_sp_pr.is_some() => format!("<c:spPr>{}</c:spPr>", chart.area_sp_pr.as_deref().unwrap_or_default()),
        (false, false) => String::new(),
        (fill, line) => format!("<c:spPr>{}{}</c:spPr>", if fill { "<a:noFill/>" } else { "" }, if line { "<a:ln><a:noFill/></a:ln>" } else { "" }),
    };
    let plot_layout = match chart.plot_layout {
        Some(l) => format!(
            "<c:layout><c:manualLayout><c:layoutTarget val=\"{}\"/><c:xMode val=\"edge\"/><c:yMode val=\"edge\"/><c:x val=\"{}\"/><c:y val=\"{}\"/><c:w val=\"{}\"/><c:h val=\"{}\"/></c:manualLayout></c:layout>",
            if l.inner { "inner" } else { "outer" },
            ibis_calc::number_to_xml(l.x),
            ibis_calc::number_to_xml(l.y),
            ibis_calc::number_to_xml(l.width),
            ibis_calc::number_to_xml(l.height)
        ),
        None => "<c:layout/>".to_string(),
    };
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><c:chart>{title}<c:autoTitleDeleted val=\"{}\"/><c:plotArea>{plot_layout}{plot}{axes}{plot_sp_pr}</c:plotArea>{legend}<c:plotVisOnly val=\"1\"/><c:dispBlanksAs val=\"gap\"/></c:chart>{area}</c:chartSpace>",
        if chart.title.is_some() { 0 } else { 1 }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_series_data_labels_are_read() {
        // SERIES, then its DATAFORMAT of every point with an ATTACHEDLABEL of value, percentage, category and
        // percentage, and category (0x17: corpus 104's pie), and its group, a pie.
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let data_format = [0xFF, 0xFF, 0, 0, 0, 0, 0, 0];
        let label = [0x17, 0];
        // AI of the values: a formula of an area of a sheet (B1:B6).
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1006, data: &data_format },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x100C, data: &label },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1019, data: &[0, 0, 0, 0, 0, 0] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!(chart.series[0].labels, Some(0x17));
    }

    #[test]
    fn a_series_fill_in_a_gel_frame_is_over_the_area_formats() {
        // Corpus 145: AREAFORMAT black, not automatic, then a GELFRAME of fillType 0 (solid) and fillColor 0x7F7F7F
        // (an RGB of fPaletteRGB): the bars are grey.
        let area = [0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0x4D, 0, 0x4D, 0];
        let gel = [0x23, 0x00, 0x0B, 0xF0, 12, 0, 0, 0, 0x80, 0x01, 0, 0, 0, 0, 0x81, 0x01, 0x7F, 0x7F, 0x7F, 0x02];
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let data_format = [0xFF, 0xFF, 0, 0, 0, 0, 0, 0];
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1006, data: &data_format },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x100A, data: &area },
            Record { id: 0x1066, data: &gel },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1017, data: &[0, 0, 0x96, 0, 0, 0] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        assert_eq!(read_chart(&refs).unwrap().series[0].fill, Some(0x7F7F7F));
    }

    #[test]
    fn a_markers_colour_is_its_inside() {
        // MARKERFORMAT of corpus 145: border 000000 (not shown), inside 339966, circle, not automatic.
        let marker = [0, 0, 0, 0, 0x33, 0x99, 0x66, 0, 8, 0, 0x20, 0, 0x4D, 0, 0x39, 0, 0x78, 0, 0, 0];
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let data_format = [0xFF, 0xFF, 0, 0, 0, 0, 0, 0];
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1006, data: &data_format },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1009, data: &marker },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x101B, data: &[0, 0, 0, 0, 0, 0] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!((chart.series[0].marker_color, chart.series[0].marker_border), (Some(0x339966), Some(None)));
    }

    #[test]
    fn a_series_look_in_drawingml_is_kept() {
        // ShapePropsStream after a series' formats (corpus 145): its DrawingML is the series' spPr.
        let xml = br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><a:spPr xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:solidFill><a:schemeClr val="bg2"><a:alpha val="15000"/></a:schemeClr></a:solidFill><a:ln><a:noFill/></a:ln></a:spPr>"#;
        let mut props = vec![0xA4, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        props.extend((xml.len() as u32).to_le_bytes());
        props.extend_from_slice(xml);
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let data_format = [0xFF, 0xFF, 0, 0, 0, 0, 0, 0];
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1006, data: &data_format },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x08A4, data: &props },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1017, data: &[0, 0, 0x96, 0, 0, 0] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!(chart.series[0].sp_pr.as_deref(), Some(r#"<a:solidFill><a:schemeClr val="bg2"><a:alpha val="15000"/></a:schemeClr></a:solidFill><a:ln><a:noFill/></a:ln>"#));
    }

    #[test]
    fn an_axis_number_format_is_read() {
        // The value axis of corpus 145: AXIS of type 1, BEGIN, VALUERANGE, IFMTRECORD 9 (0%), END.
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let axis = [1u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1017, data: &[0, 0, 0x96, 0, 0, 0] },
            Record { id: 0x101D, data: &axis },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x104E, data: &[9, 0] },
            Record { id: 0x1034, data: &[] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        assert_eq!(read_chart(&refs).unwrap().value.format, Some(9));
    }

    #[test]
    fn a_chart_area_without_outline_or_fill_is_kept() {
        // FRAME, BEGIN, LINEFORMAT of style 5 (none, not automatic), AREAFORMAT of pattern 0 (not automatic), END,
        // then a pie's series: corpus 104's charts have no border.
        let line = [0, 0, 0, 0, 5, 0, 0xFF, 0xFF, 0, 0];
        let area = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let recs = [
            Record { id: 0x1032, data: &[0, 0, 0, 0] },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1007, data: &line },
            Record { id: 0x100A, data: &area },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1019, data: &[0, 0, 0, 0, 0, 0] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!((chart.area_line, chart.area_fill), (Some(false), Some(false)));
    }

    #[test]
    fn values_the_chart_holds_itself_are_read() {
        // Corpus 024 and 114: the series' BRAIs of values and categories are literal (rt 1), and the chart's cache after an
        // SIINDEX holds them: a NUMBER or a LABEL a point (its row), of a series (its column).
        let series = [0x03u8, 0, 1, 0, 2, 0, 2, 0, 1, 0, 0, 0];
        let values = [1u8, 1, 0, 0, 0, 0, 0, 0];
        let categories = [2u8, 1, 0, 0, 0, 0, 0, 0];
        let number = |row: u8, v: f64| {
            let mut d = vec![row, 0, 0, 0, 0, 0];
            d.extend_from_slice(&v.to_le_bytes());
            d
        };
        let label = |row: u8, t: &str| {
            let mut d = vec![row, 0, 0, 0, 0, 0, t.len() as u8, 0, 0];
            d.extend_from_slice(t.as_bytes());
            d
        };
        let (n0, n1, l0, l1) = (number(0, 96995.0), number(1, 3.5), label(0, "2019"), label(1, "2020"));
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1051, data: &categories },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1017, data: &[0, 0, 150, 0, 0, 0] },
            Record { id: 0x1065, data: &[2, 0] },
            Record { id: 0x0204, data: &l0 },
            Record { id: 0x0204, data: &l1 },
            Record { id: 0x1065, data: &[1, 0] },
            Record { id: 0x0203, data: &n0 },
            Record { id: 0x0203, data: &n1 },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!(chart.series[0].values, Source::Lit(vec![Cell::Number(96995.0), Cell::Number(3.5)]));
        assert_eq!(chart.series[0].categories, Source::Lit(vec![Cell::Text("2019".into()), Cell::Text("2020".into())]));
    }

    #[test]
    fn a_union_of_cells_is_read_whole() {
        // Corpus 061's values: ptgMemFunc around C6, E6 and G6 (ptgRef3d of sheet 1) with ptgUnion between, then ptgParen.
        let rgce = [0x29u8, 25, 0, 0x3A, 1, 0, 5, 0, 2, 0, 0x3A, 1, 0, 5, 0, 4, 0, 0x10, 0x3A, 1, 0, 5, 0, 6, 0, 0x10, 0x15];
        let cell = |col: u16| Ref { ixti: 1, rows: (5, 5), columns: (col, col) };
        assert_eq!(first_ref(&rgce), Source::Refs(vec![cell(2), cell(4), cell(6)]));
        // One area alone is a reference.
        assert_eq!(first_ref(&[0x3B, 1, 0, 0, 0, 4, 0, 1, 0, 1, 0]), Source::Ref(Ref { ixti: 1, rows: (0, 4), columns: (1, 1) }));
    }

    #[test]
    fn a_plot_area_placed_by_hand_is_kept() {
        // CRTLAYOUT12A of corpus 061: the inner plot area, x and y edges, width and height sizes (0.114, 0.183, 0.87, 0.582).
        let mut layout = vec![0xA7u8, 0x08, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 1, 0, 1, 0];
        for v in [0.114f64, 0.183, 0.87, 0.582] {
            layout.extend_from_slice(&v.to_le_bytes());
        }
        layout.extend_from_slice(&[0, 0]);
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1017, data: &[0, 0, 150, 0, 0, 0] },
            Record { id: 0x08A7, data: &layout },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!(chart.plot_layout, Some(PlotLayout { inner: true, x: 0.114, y: 0.183, width: 0.87, height: 0.582 }));
    }

    #[test]
    fn a_chart_of_series_with_no_values_is_kept_empty() {
        // Corpus 145's second chart of a sheet: SERIES with no points, its AIs of the values and categories of no link (rt 0).
        // Excel draws its frame and axes, so the chart is kept, with its series of no values.
        let series = [0x03u8, 0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0];
        let ai = |id: u8| [id, 0, 0, 0, 0, 0, 0, 0];
        let (name, values, categories) = (ai(0), ai(1), ai(2));
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &name },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1051, data: &categories },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1017, data: &[0, 0, 150, 0, 0, 0] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!(chart.series.len(), 1);
        assert_eq!(chart.series[0].values, Source::Lit(Vec::new()));
        // With one series that has values, the empty one is still left out.
        let with_values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &with_values },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1017, data: &[0, 0, 150, 0, 0, 0] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        assert_eq!(read_chart(&refs).unwrap().series.len(), 1);
    }

    #[test]
    fn a_plot_area_of_an_old_chart_is_the_axis_groups_position() {
        // Corpus 104's pie: AXISPARENT, then its POSITION (relative to the chart, left 985, top 366, width 2015, height 3244, in 1/4000).
        let mut position = vec![2u8, 0, 2, 0];
        for v in [985u32, 366, 2015, 3244] {
            position.extend_from_slice(&v.to_le_bytes());
        }
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1041, data: &[0; 18] },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x104F, data: &position },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1019, data: &[0, 0, 0, 0, 2, 0] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!(chart.plot_layout, Some(PlotLayout { inner: true, x: 985.0 / 4000.0, y: 366.0 / 4000.0, width: 2015.0 / 4000.0, height: 3244.0 / 4000.0 }));
    }

    #[test]
    fn a_series_labels_separator_is_read() {
        // Corpus 104's pie: the TEXT of the series' labels (an OBJECTLINK to every point of series 0) holds a DataLabExtContents
        // of category, value and percentage whose separator is a line break (16-bit characters, one of them).
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let mut ext = vec![0u8; 12];
        ext.extend_from_slice(&[0x0E, 0, 1, 0, 1, 0x0A, 0]);
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1025, data: &[0; 32] },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1027, data: &[4, 0, 0, 0, 0xFF, 0xFF] },
            Record { id: 0x086B, data: &ext },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1019, data: &[0, 0, 0, 0, 2, 0] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!(chart.series[0].label_separator.as_deref(), Some("\n"));
    }

    #[test]
    fn a_group_in_the_second_axis_parent_is_on_the_secondary_axes() {
        // Corpus 106's bars and line: the bars' chart group in the first AXISPARENT, the line's in the second, whose value axis
        // (AXIS 1) has its own scale (0 to 800).
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let mut range = vec![0u8; 42];
        range[8..16].copy_from_slice(&800f64.to_le_bytes());
        let (first, second) = ([0u8, 0], [1u8, 0]);
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1045, data: &first },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1045, data: &second },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1041, data: &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1017, data: &[0, 0, 150, 0, 0, 0] },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1041, data: &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x101D, data: &[1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0] },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x101F, data: &range },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1018, data: &[0, 0, 0, 0, 0, 0] },
            Record { id: 0x1034, data: &[] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!(chart.group_secondary, vec![false, true]);
        assert_eq!(chart.value2.max, Some(800.0));
        assert_eq!(chart.value.max, None);
    }

    #[test]
    fn a_data_label_moved_by_hand_has_its_layout_read() {
        // Corpus 104's first point: the TEXT of the point's label holds a CrtLayout12 whose x and y (doubles) are the xlsx's manualLayout.
        let series = [0x03u8, 0, 1, 0, 6, 0, 6, 0, 1, 0, 0, 0];
        let values = [1u8, 2, 0, 0, 0, 0, 11, 0, 0x3B, 0, 0, 0, 0, 5, 0, 1, 0, 1, 0];
        let mut layout = vec![0u8; 12];
        layout.extend_from_slice(&[1, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0]);
        layout.extend_from_slice(&(-0.25f64).to_le_bytes());
        layout.extend_from_slice(&0.125f64.to_le_bytes());
        layout.extend_from_slice(&[0; 18]);
        let recs = [
            Record { id: 0x1003, data: &series },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1051, data: &values },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1025, data: &[0; 32] },
            Record { id: 0x1033, data: &[] },
            Record { id: 0x1027, data: &[4, 0, 0, 0, 0, 0] },
            Record { id: 0x089D, data: &layout },
            Record { id: 0x1034, data: &[] },
            Record { id: 0x1019, data: &[0, 0, 0, 0, 2, 0] },
        ];
        let refs: Vec<&Record> = recs.iter().collect();
        let chart = read_chart(&refs).unwrap();
        assert_eq!(chart.series[0].label_layouts, vec![(0, -0.25, 0.125)]);
    }
}
