// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Charts: their data and how it is drawn.
//!
//! The model follows DrawingML charts closely enough that a chart read from
//! a .pptx draws the way PowerPoint drew it: plot area and label positions
//! are kept as the fractions of the chart frame that the file states.

use crate::drawing::{Color, Fill, Stroke};

/// A value in a chart. Compared by bits, so charts are `Eq` like the rest of
/// the model (and a NaN equals itself).
#[derive(Clone, Copy, Debug)]
pub struct Number(pub f64);

impl PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}

impl Eq for Number {}

/// A rectangle as fractions of the chart frame, in 100 000ths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FractionRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// Formatting of chart text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartText {
    /// Hundredths of a point.
    pub size: u32,
    pub bold: bool,
    /// `None` hides the text (no fill).
    pub color: Option<Color>,
    /// The Latin typeface.
    pub font: Option<String>,
    /// The East Asian typeface.
    pub east_asian_font: Option<String>,
}

impl ChartText {
    /// The typeface for `text`: the East Asian one when it has East Asian
    /// characters, otherwise the Latin one.
    pub fn font_for(&self, text: &str) -> Option<String> {
        if text.chars().any(|c| c as u32 >= 0x2e80) {
            self.east_asian_font.clone().or_else(|| self.font.clone())
        } else {
            self.font.clone().or_else(|| self.east_asian_font.clone())
        }
    }
}

impl Default for ChartText {
    /// Office's default chart text: 10 pt, dark grey.
    fn default() -> Self {
        Self { size: 1000, bold: false, color: Some(Color::opaque(crate::Rgb(0x595959))), font: None, east_asian_font: None }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Grouping {
    /// Side by side (bars) or independent (lines, areas).
    #[default]
    Standard,
    Stacked,
    PercentStacked,
}

/// Source c:radarStyle values. Absence/unknown lexical values are not defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadarStyle {
    Standard,
    Marker,
    Filled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfPieKind { Pie, Bar }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfPieSplit { Custom, Percent, Position, Value }

/// Source OfPie settings; no split/default/layout interpretation is implied.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct OfPieSettings {
    pub split: Option<OfPieSplit>,
    pub split_position: Option<Number>,
    pub second_size: Option<u16>,
    pub gap_width: Option<u16>,
    pub vary_colors: Option<bool>,
    /// None is absent; Some preserves empty lists and invalid value slots.
    pub custom_points: Option<Vec<Option<u32>>>,
    /// None is absent serLines; inner None is a present unformatted/noFill line.
    /// The original XML preserves those lexical/formatting distinctions.
    pub connector: Option<Option<Stroke>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartType {
    /// `gap_width` is the gap between categories as a percentage of a bar's
    /// width; `overlap` (−100 to 100) how much bars of one category overlap.
    Bar { horizontal: bool, gap_width: u32, overlap: i32 },
    Line,
    Area,
    /// Retained only; common rendering currently uses the unsupported placeholder.
    Radar { style: Option<RadarStyle> },
    /// Source retention only; native split/geometry is not implemented.
    OfPie { kind: Option<OfPieKind> },
    /// `first_angle` in degrees, clockwise from 12 o'clock.
    Pie { first_angle: u32 },
    /// `hole` is the hole's diameter as a percentage of the doughnut's.
    Doughnut { first_angle: u32, hole: u32 },
    /// 散布図: each point at its x value (`Series::x_values`) and y value;
    /// `smooth` lines are curves.
    Scatter { smooth: bool },
    /// バブル: a scatter whose points are circles sized by
    /// `Series::bubble_sizes` (their area, `scale` percent of the default).
    Bubble { scale: u32 },
    /// 株価: high-low-close or open-high-low-close.
    Stock { up_down_bars: bool, hi_low_lines: bool, drop_lines: bool },
    /// 等高線・3D表面: 3D surface or 2D contour chart.
    Surface { is_3d: bool, wireframe: bool },
}

impl ChartType {
    /// ECMA-376 Part 4, OfficeOpenXML-XMLSchema-Transitional/dml-chart.xsd:
    /// CT_LineSer (also stockChart), CT_ScatterSer and CT_RadarSer allow c:marker;
    /// CT_BarSer, CT_AreaSer, CT_BubbleSer, CT_PieSer and CT_SurfaceSer do not.
    pub fn supports_markers(self) -> bool {
        matches!(self, Self::Line | Self::Scatter { .. } | Self::Radar { .. } | Self::Stock { .. })
    }
    /// The same schema's series types admitting c:trendline and c:errBars.
    pub fn supports_series_analysis(self) -> bool {
        matches!(self, Self::Bar { .. } | Self::Line | Self::Area | Self::Scatter { .. } | Self::Bubble { .. } | Self::Stock { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelPosition {
    Center,
    InsideEnd,
    InsideBase,
    OutsideEnd,
    Above,
    Below,
    Left,
    Right,
    BestFit,
}

/// The format of a chartEx chart that can be changed: its data labels, the range of its value axis, the fill of its series.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExFormat {
    /// The data labels of the (first) series; `None`: none.
    pub labels: Option<ExLabels>,
    /// The value axis' least, greatest and major unit; `None`: automatic.
    pub axis_min: Option<crate::Number>,
    pub axis_max: Option<crate::Number>,
    pub axis_major: Option<crate::Number>,
    /// The fill of the series (a waterfall's increase, decrease and total; a box's box; the first of the others); `None`: the
    /// chart's own colour.
    pub fills: Vec<Option<crate::Rgb>>,
}

/// What a chartEx chart's data labels show (`cx:dataLabels`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExLabels {
    pub value: bool,
    pub category: bool,
    pub series_name: bool,
    /// `pos`: outside end, inside end, centre, inside base; `None`: the kind's own.
    pub position: Option<LabelPosition>,
}

/// A piece of a data label's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelPiece {
    Text(String),
    Value,
    Percent,
    Category,
    SeriesName,
}

/// Settings of one data point's label that differ from its series'.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PointLabel {
    pub index: usize,
    pub deleted: bool,
    pub show_value: Option<bool>,
    pub show_percent: Option<bool>,
    pub show_category: Option<bool>,
    pub show_series_name: Option<bool>,
    /// Moved from its automatic position, in 100 000ths of the chart frame.
    pub offset: (i32, i32),
    /// Fixed text, with fields for the value and the like.
    pub text: Option<Vec<LabelPiece>>,
    pub style: Option<ChartText>,
    pub fill: Option<Fill>,
    pub border: Option<Option<Stroke>>,
    pub position: Option<LabelPosition>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataLabels {
    pub show_value: bool,
    pub show_percent: bool,
    pub show_category: bool,
    pub show_series_name: bool,
    /// The text between the parts of a label (`c:separator`); `None` is ", ".
    pub separator: Option<String>,
    pub position: Option<LabelPosition>,
    /// Number format code; `None` uses the series'.
    pub format: Option<String>,
    pub style: ChartText,
    pub fill: Fill,
    pub border: Option<Stroke>,
    pub points: Vec<PointLabel>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerSymbol {
    None,
    Circle,
    Square,
    Diamond,
    Triangle,
}

/// Exact source values only; unsupported geometry/defaults are not inferred.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerSourceSymbol {
    Auto, Circle, Dash, Diamond, Dot, None, Picture, Plus, Square, Star, Triangle, Cross,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkerSource {
    /// None retains absent/val-less/unknown source symbol in opaque XML.
    pub symbol: Option<MarkerSourceSymbol>,
    /// Original UInt32 size before the legacy drawing clamp; not native validity.
    pub size: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    /// None is a derived marker; Some is an explicit chart-namespace marker.
    pub source: Option<MarkerSource>,
    pub symbol: MarkerSymbol,
    /// Points.
    pub size: u32,
    pub fill: Fill,
}

/// Trendline type for a series (`c:trendlineType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrendlineType {
    #[default]
    Linear,
    Exponential,
    Logarithmic,
    Polynomial,
    Power,
    MovingAverage,
}

impl TrendlineType {
    pub fn as_ooxml_str(&self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Exponential => "exp",
            Self::Logarithmic => "log",
            Self::Polynomial => "poly",
            Self::Power => "power",
            Self::MovingAverage => "movingAvg",
        }
    }

    pub fn from_ooxml_str(s: &str) -> Option<Self> {
        match s {
            "linear" => Some(Self::Linear),
            "exp" => Some(Self::Exponential),
            "log" => Some(Self::Logarithmic),
            "poly" => Some(Self::Polynomial),
            "power" => Some(Self::Power),
            "movingAvg" => Some(Self::MovingAverage),
            _ => None,
        }
    }
}

/// A trendline on a series (`c:trendline`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trendline {
    pub kind: TrendlineType,
    /// Polynomial degree (2..6, default 2).
    pub order: u8,
    /// Moving average period (>= 2, default 2).
    pub period: u32,
    /// Display equation on chart (`c:dispEq`).
    pub disp_eq: bool,
    /// Display R-squared value on chart (`c:dispRSqr`).
    pub disp_r_sqr: bool,
    /// Custom line stroke (`c:spPr`).
    pub stroke: Option<Stroke>,
}

impl Default for Trendline {
    fn default() -> Self {
        Self {
            kind: TrendlineType::Linear,
            order: 2,
            period: 2,
            disp_eq: false,
            disp_r_sqr: false,
            stroke: None,
        }
    }
}

/// Direction of error bars (`c:errDir`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ErrorBarDirection {
    #[default]
    Y,
    X,
}

impl ErrorBarDirection {
    pub fn as_ooxml_str(&self) -> &'static str {
        match self {
            Self::Y => "y",
            Self::X => "x",
        }
    }

    pub fn from_ooxml_str(s: &str) -> Option<Self> {
        match s {
            "y" => Some(Self::Y),
            "x" => Some(Self::X),
            _ => None,
        }
    }
}

/// Type of error bar display: both, minus, or plus (`c:errBarType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ErrorBarType {
    #[default]
    Both,
    Minus,
    Plus,
}

impl ErrorBarType {
    pub fn as_ooxml_str(&self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Minus => "minus",
            Self::Plus => "plus",
        }
    }

    pub fn from_ooxml_str(s: &str) -> Option<Self> {
        match s {
            "both" => Some(Self::Both),
            "minus" => Some(Self::Minus),
            "plus" => Some(Self::Plus),
            _ => None,
        }
    }
}

/// Value calculation method for error bars (`c:errValType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ErrorBarValueType {
    #[default]
    FixedValue,
    Percentage,
    StandardDeviation,
    StandardError,
    Custom,
}

impl ErrorBarValueType {
    pub fn as_ooxml_str(&self) -> &'static str {
        match self {
            Self::FixedValue => "fixedVal",
            Self::Percentage => "percentage",
            Self::StandardDeviation => "stdDev",
            Self::StandardError => "stdErr",
            Self::Custom => "cust",
        }
    }

    pub fn from_ooxml_str(s: &str) -> Option<Self> {
        match s {
            "fixedVal" => Some(Self::FixedValue),
            "percentage" => Some(Self::Percentage),
            "stdDev" => Some(Self::StandardDeviation),
            "stdErr" => Some(Self::StandardError),
            "cust" => Some(Self::Custom),
            _ => None,
        }
    }
}

/// Error bars on a series (`c:errBars`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorBars {
    pub direction: ErrorBarDirection,
    pub bar_type: ErrorBarType,
    pub val_type: ErrorBarValueType,
    pub no_end_cap: bool,
    /// Value for fixed value, percentage, or stdDev (default 1.0 or 5.0).
    pub value: Option<Number>,
    /// Line style for the error bar (`c:spPr`).
    pub stroke: Option<Stroke>,
    /// Custom positive error values (`c:plus`) when `val_type == ErrorBarValueType::Custom`.
    pub plus: Vec<Option<Number>>,
    /// Custom negative error values (`c:minus`) when `val_type == ErrorBarValueType::Custom`.
    pub minus: Vec<Option<Number>>,
}

impl Default for ErrorBars {
    fn default() -> Self {
        Self {
            direction: ErrorBarDirection::Y,
            bar_type: ErrorBarType::Both,
            val_type: ErrorBarValueType::FixedValue,
            no_end_cap: false,
            value: Some(Number(1.0)),
            stroke: None,
            plus: Vec::new(),
            minus: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Series {
    pub name: String,
    pub categories: Vec<String>,
    pub values: Vec<Option<Number>>,
    /// Number format code of the values.
    pub format: String,
    pub fill: Fill,
    pub stroke: Option<Stroke>,
    /// Fills of single data points (pie slices, highlighted bars).
    pub point_fills: Vec<(usize, Fill)>,
    /// Outlines of single data points; `None` for no outline.
    pub point_strokes: Vec<(usize, Option<Stroke>)>,
    /// How far a pie's slices are pulled out from the centre (`c:explosion` of the series), as a percentage of the radius.
    pub explosion: u32,
    /// The same for single slices (`c:explosion` of a `c:dPt`), overriding the series'.
    pub point_explosions: Vec<(usize, u32)>,
    pub marker: Option<Marker>,
    pub labels: Option<DataLabels>,
    /// Trendline on this series (`c:trendline`).
    pub trendline: Option<Trendline>,
    /// Error bars on this series (`c:errBars`).
    pub error_bars: Option<ErrorBars>,
    /// A scatter or bubble series' x values (`c:xVal`; empty for the others,
    /// and for x values that are text: then 1, 2, 3, …).
    pub x_values: Vec<Option<Number>>,
    /// A bubble series' sizes (`c:bubbleSize`).
    pub bubble_sizes: Vec<Option<Number>>,
    /// A bubble series drawn as spheres (`c:bubble3D`).
    pub bubble_3d: bool,
}

/// Series of one chart type drawn against one pair of axes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartGroup {
    pub kind: ChartType,
    pub of_pie: Option<OfPieSettings>,
    pub grouping: Grouping,
    pub series: Vec<Series>,
    /// Ids of the category and value axes, in the file's order.
    pub axes: Vec<u32>,
    /// The 3-D view of a `bar3DChart`, `line3DChart`, `area3DChart` or `pie3DChart`
    /// (`c:view3D` of the chart); `None` for a flat group.
    pub three_d: Option<View3D>,
}

/// `c:view3D`: how a 3-D chart is turned. Angles are degrees; `depth_percent` and
/// `height_percent` are percentages (`c:depthPercent`, `c:hPercent`; 100 and automatic by default).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct View3D {
    pub rot_x: i32,
    pub rot_y: i32,
    /// Right-angle axes (`c:rAngAx`): an oblique projection, the axes stay upright.
    pub right_angle_axes: bool,
    pub depth_percent: u32,
    /// `c:hPercent`: the box's height as a percentage of its base (`None`: automatic).
    pub height_percent: Option<u32>,
    /// `c:grouping val="standard"` (3-D column): the series stand one behind the other; otherwise (clustered) they stand side by side in one row.
    pub series_in_depth: bool,
    /// `c:gapDepth`: the gap behind a bar as a percentage of its depth.
    pub gap_depth: u32,
}

impl Default for View3D {
    fn default() -> Self {
        Self { rot_x: 15, rot_y: 20, right_angle_axes: true, depth_percent: 100, height_percent: None, series_in_depth: true, gap_depth: 150 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisKind {
    Category,
    Value,
    /// Source retention only; filtered out of the current 2D renderer.
    Series,
}

/// Original axis element kind; Date retains the existing Category behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisSourceKind { Category, Value, Date, Series }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AxisCrosses { AutoZero, Maximum, Minimum }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Bottom,
    Left,
    Right,
    Top,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Axis {
    pub id: u32,
    /// Strict UInt32 source ID; absent/invalid is not a valid zero ID.
    /// The original XML keeps absence and invalid lexical details separate.
    pub source_id: Option<u32>,
    pub source_kind: AxisSourceKind,
    pub crosses: Option<AxisCrosses>,
    pub crosses_at: Option<Number>,
    pub cross_axis_id: Option<u32>,
    pub kind: AxisKind,
    pub side: Side,
    pub deleted: bool,
    /// Values run from maximum to minimum.
    pub reversed: bool,
    /// Base of a logarithmic scale (`c:logBase`, typically 10); unset for a linear scale.
    pub log_base: Option<Number>,
    pub min: Option<Number>,
    pub max: Option<Number>,
    pub major_unit: Option<Number>,
    /// Labels show values divided by this (`c:dispUnits`, such as 10 000).
    pub display_unit: Option<Number>,
    pub format: String,
    pub show_labels: bool,
    /// Labels sit at the far end of the crossing axis (`tickLblPos="high"`),
    /// such as on the right of a chart for a vertical value axis.
    pub labels_high: bool,
    /// Labels sit at the low end of the crossing axis (`tickLblPos="low"`),
    /// such as below the plot for a top value axis.
    pub labels_low: bool,
    /// Rotation of the tick labels, in 60 000ths of a degree clockwise
    /// (−5 400 000 reads bottom to top); 0 for horizontal.
    pub label_rotation: i32,
    /// Whether `label_rotation` was explicitly set on the axis (`a:bodyPr/@rot`).
    pub fixed_label_rotation: bool,
    /// Show every `label_skip`-th category label (1 shows all); 0 leaves it
    /// to the layout, which thins out labels that would overlap.
    pub label_skip: u32,
    /// A date axis (`c:dateAx`): its crowded labels turn upright.
    pub date: bool,
    /// A date axis's time scale: categories are placed by their dates.
    pub time: Option<TimeScale>,
    /// Major tick marks reach out from the axis (`majorTickMark` "out" or
    /// "cross"), between it and its labels.
    pub ticks_outside: bool,
    pub line: Option<Stroke>,
    pub gridlines: Option<Stroke>,
    /// Lines at the minor unit between the major gridlines (`c:minorGridlines`).
    pub minor_gridlines: Option<Stroke>,
    /// The minor unit (`c:minorUnit`); unset, a fifth of the major unit.
    pub minor_unit: Option<Number>,
    pub text: ChartText,
    pub title: Option<ChartTitle>,
    /// Crowded category labels stand in two rows, every other one lower (a
    /// histogram's and a Pareto chart's) instead of turning.
    pub stagger_labels: bool,
    /// The title's rotation (`a:bodyPr/@rot` of the title, in 60 000ths of a degree); `None` is the automatic one (a
    /// value axis' title stands upright along the axis).
    pub title_rotation: Option<i32>,
    /// The outer levels of a category axis of several levels (`c:multiLvlStrRef`), the nearest to the labels first: each
    /// a group of categories (the first of them, how many, its name), as a year over its months.
    pub category_levels: Vec<Vec<(usize, usize, String)>>,
}

/// The chart's title (`c:title`).
/// A unit of time on a date axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeUnit {
    Days,
    Months,
    Years,
}

/// How a date axis spaces its categories and ticks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimeScale {
    /// The step categories are counted in (`baseTimeUnit`).
    pub base: TimeUnit,
    /// Ticks and labels every `major` of `major_unit`; `None` is automatic.
    pub major: Option<(Number, TimeUnit)>,
    /// The categories' dates (spreadsheet serial numbers), as shown.
    pub dates: Vec<Option<Number>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartTitle {
    /// Lines of text.
    pub text: String,
    pub style: ChartText,
    /// Top-left corner, in 100 000ths of the chart frame; `None` centres it
    /// at the top.
    pub at: Option<(i32, i32)>,
}

/// Original legend position; native layout interpretation is separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegendPosition { Bottom, Left, Right, Top, TopRight }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Legend {
    /// None preserves absent/val-less/unknown position in original XML.
    pub source_position: Option<LegendPosition>,
    pub side: Side,
    /// Source c:legend/c:overlay. None means absent or unrecognized XML;
    /// the original XML retains lexical details. Rendering/editing are separate.
    pub overlay: Option<bool>,
    pub layout: Option<FractionRect>,
    pub text: ChartText,
    /// Entries (series, or points of a pie) that are not listed.
    pub hidden: Vec<usize>,
}

/// How blank cells are plotted on a chart (`c:dispBlanksAs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DispBlanksAs {
    /// Blank cells are not plotted (gap / leave empty).
    #[default]
    Gap,
    /// Blank cells are plotted as zero.
    Zero,
    /// Blank cells are plotted as spanned (connect data points with a line).
    Span,
}

/// A chart's data table (`c:dTable`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChartDataTable {
    pub show_horz_border: bool,
    pub show_vert_border: bool,
    pub show_outline: bool,
    pub show_keys: bool,
}

impl Default for ChartDataTable {
    fn default() -> Self {
        Self {
            show_horz_border: true,
            show_vert_border: true,
            show_outline: true,
            show_keys: false,
        }
    }
}

impl Chart {
    /// A chart with nothing in it: the placeholder a new chart is written with and read back from.
    pub fn empty() -> Self {
        Chart {
            groups: Vec::new(),
            axes: Vec::new(),
            legend: None,
            title: None,
            plot_area: None,
            background: crate::Fill::None,
            border: None,
            plot_background: crate::Fill::None,
            plot_border: None,
            disp_blanks_as: None,
            plot_vis_only: true,
            data_table: None,
            style: None,
            pivot_source: None,
            rounded_corners: false,
            title_overlay: false,
            ex: None,
            ex_format: None,
            excel_layout: false,
            word_layout: false,
            cell_refs: Vec::new(),
            ex_source: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chart {
    pub groups: Vec<ChartGroup>,
    pub axes: Vec<Axis>,
    pub legend: Option<Legend>,
    pub title: Option<ChartTitle>,
    /// The inner plot area (inside the axes); `None` is automatic.
    pub plot_area: Option<FractionRect>,
    pub background: Fill,
    pub border: Option<Stroke>,
    pub plot_background: Fill,
    /// The outline of the plot area (`c:plotArea/c:spPr/a:ln`); `None` draws none.
    pub plot_border: Option<Stroke>,
    /// How blank cells are plotted (`c:dispBlanksAs`).
    pub disp_blanks_as: Option<DispBlanksAs>,
    /// Plot visible cells only (`c:plotVisOnly`). Defaults to true.
    pub plot_vis_only: bool,
    /// A data table beneath the plot area (`c:dTable`).
    pub data_table: Option<ChartDataTable>,
    /// Visual style identifier (`c:style`). Typically 1..48.
    pub style: Option<u32>,
    /// Pivot chart source reference (`c:pivotSource`). Name of the linked PivotTable.
    pub pivot_source: Option<String>,
    /// The chart area has rounded corners (`c:roundedCorners`).
    pub rounded_corners: bool,
    /// The title lies over the plot area (`c:title/c:overlay`), which then
    /// reaches to the top of the chart.
    pub title_overlay: bool,
    /// What the new chart kinds (chartEx) draw that the groups do not: boxes
    /// over a chart's axes, or a whole treemap or sunburst.
    pub ex: Option<ChartEx>,
    /// What of a chartEx chart's format can be changed besides its title and legend (`cx:dataLabels`, the value axis' range, the
    /// colours of its series); `None` for a chart of the old kinds.
    pub ex_format: Option<ExFormat>,
    /// Drawn as Excel's engine lays a chart out (a chart of a worksheet or of
    /// Word, not of PowerPoint): the margins and gaps of the plot differ.
    pub excel_layout: bool,
    /// A chart in a Word document: laid out as Word does (its automatic axis intervals are not yet measured and follow the earlier rule).
    pub word_layout: bool,
    /// The cells each series takes its name, categories and values from (`c:f`), for a chart of a worksheet: the chart
    /// follows them as the cells change. Empty where the chart holds its data itself.
    pub cell_refs: Vec<SeriesCells>,
    /// For a new kind of chart (chartEx) of a worksheet: what it is made again from when its cells change, and the
    /// settings (totals, bins) written back into its part.
    pub ex_source: Option<ExSourceRef>,
}

/// The cells a chartEx chart reads: each cell's text, number and number format, by row.
pub type ExCells = Vec<Vec<(String, Option<f64>, String)>>;

/// What a chartEx chart is made from: its part's XML and the cells its data names. Implemented where that XML is read.
pub trait ChartExSource: Send + Sync {
    /// The chart made from the cells `cells` gives (a sheet's name in lower case and a range).
    fn rebuild(&self, cells: &dyn Fn(&str, crate::CellRange) -> Option<ExCells>) -> Option<Chart>;
    /// The settings the part holds.
    fn options(&self) -> ChartExOptions;
    /// The same source with `options` in its part.
    fn with_options(&self, options: &ChartExOptions) -> Option<ExSourceRef>;
    /// The part's XML.
    fn xml(&self) -> String;
    /// The same source with its formula references rewritten by `f`, or `None` if none changed.
    fn rewrite_references(&self, f: &dyn Fn(&str) -> String) -> Option<ExSourceRef> {
        let _ = f;
        None
    }
}

/// A chartEx chart's source, compared by the settings and references it holds; `follows`: the chart is made again as its cells change.
#[derive(Clone)]
pub struct ExSourceRef(pub std::sync::Arc<dyn ChartExSource>, pub bool);

impl PartialEq for ExSourceRef {
    fn eq(&self, other: &Self) -> bool {
        self.1 == other.1 && (std::sync::Arc::ptr_eq(&self.0, &other.0) || self.0.options() == other.0.options())
    }
}

impl Eq for ExSourceRef {}

impl std::fmt::Debug for ExSourceRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ExSourceRef")
    }
}

/// How a histogram's bins are made.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BinBy {
    #[default]
    Auto,
    /// A width (`binSize`).
    Width(Number),
    /// A number of bins (`binCount`).
    Count(u32),
}

/// The settings of a chartEx chart that its format pane changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartExOptions {
    /// A waterfall's points that are totals (`cx:subtotals`), by index.
    pub subtotals: Vec<usize>,
    /// A histogram's bins.
    pub bin_by: BinBy,
    /// The values at and below which (above which) one bin is made (`underflow`, `overflow`).
    pub underflow: Option<Number>,
    pub overflow: Option<Number>,
    /// Bins closed on the right (`intervalClosed="r"`, the default), else on the left.
    pub closed_right: bool,
    /// Box and Whisker: show mean marker (`meanMarker="1|0"`).
    pub box_mean_marker: bool,
    /// Box and Whisker: show mean line (`meanLine="1|0"`).
    pub box_mean_line: bool,
    /// Box and Whisker: show outlier points (`outliers="1|0"`).
    pub box_outliers: bool,
    /// Box and Whisker: use inclusive median for quartile calculation (`quartileMethod="inclusive"`).
    pub box_inclusive: bool,
    /// Treemap: show overlapping parent labels (`parentLabelLayout="overlapping"`).
    pub treemap_overlapping: bool,
    /// Gap width percentage (`<cx:catScaling gapWidth="..."/>`).
    pub gap_width: Option<u32>,
}

impl Default for ChartExOptions {
    fn default() -> Self {
        Self {
            subtotals: Vec::new(),
            bin_by: BinBy::Auto,
            underflow: None,
            overflow: None,
            closed_right: true,
            box_mean_marker: true,
            box_mean_line: false,
            box_outliers: true,
            box_inclusive: false,
            treemap_overlapping: true,
            gap_width: None,
        }
    }
}

/// The references (`Sheet1!$B$2:$B$5`) a series reads its data from; `None` where it holds that itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SeriesCells {
    pub name: Option<String>,
    pub categories: Option<String>,
    pub values: Option<String>,
    /// A bubble chart's sizes (`bubbleSize`).
    pub sizes: Option<String>,
}

/// One box of a box and whisker chart: its quartiles, whiskers, mean and
/// outliers, in the data's units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxStats {
    pub name: String,
    pub q1: Number,
    pub median: Number,
    pub q3: Number,
    pub whisker_low: Number,
    pub whisker_high: Number,
    pub mean: Number,
    pub outliers: Vec<Number>,
}

/// A node of a treemap or sunburst: its name, its value (the sum of its
/// children's for a parent) and the colour it is drawn in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeNode {
    pub name: String,
    pub value: Number,
    pub color: Color,
    pub children: Vec<TreeNode>,
}

/// The drawing of a new chart kind (chartEx) that is not a bar, line or the
/// like (those are read as groups).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChartEx {
    /// A chartEx kind drawn as columns or the like (waterfall, histogram, Pareto, funnel). `links`: a waterfall's
    /// connector lines, each from the right edge of one column to the left edge of another: (the first column's
    /// index, its level, the second's index, its level).
    Synthesized { links: Vec<(usize, Number, usize, Number)> },
    /// 箱ひげ図: over the axes of one invisible group (which gives the scale
    /// and the categories).
    Box { boxes: Vec<BoxStats>, fill: Fill, mean_marker: bool, mean_line: bool, outliers: bool },
    /// ツリーマップ. `overlapping`: a group's name is written over its first corner, on its members (no band of its
    /// own, as Excel's "overlapping" parent labels); `values`: the numbers are written after the names; `labels_at_end`:
    /// a member's name stands at the bottom of its rectangle, not the top.
    Treemap { nodes: Vec<TreeNode>, overlapping: bool, values: bool, labels_at_end: bool },
    /// サンバースト.
    Sunburst(Vec<TreeNode>),
}

impl Chart {
    pub fn axis(&self, id: u32) -> Option<&Axis> {
        self.axes.iter().find(|a| a.id == id)
    }

    /// Whether this chart is a PivotChart linked to a PivotTable.
    pub fn is_pivot_chart(&self) -> bool {
        self.pivot_source.is_some()
    }
}

/// Format `value` with an Excel number format code such as `#,##0`,
/// `0.0%` or `#,##0;△#,##0`.
pub fn format_number(value: f64, code: &str) -> String {
    let sections = split_sections(code);
    let (section, value, negative_sign) = match sections.as_slice() {
        [] => ("General", value, false),
        [only] => (*only, value.abs(), value < 0.0),
        [positive, negative, rest @ ..] => {
            if value < 0.0 {
                (*negative, value.abs(), false)
            } else if value == 0.0 && !rest.is_empty() {
                (rest[0], value, false)
            } else {
                (*positive, value, false)
            }
        }
    };
    let formatted = format_section(value, section);
    if negative_sign { format!("-{formatted}") } else { formatted }
}

fn split_sections(code: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut start, mut quoted, mut escaped) = (0, false, false);
    for (i, c) in code.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => quoted = !quoted,
            ';' if !quoted => {
                out.push(&code[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&code[start..]);
    out
}

fn format_general(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        return format!("{}", value as i64);
    }
    let s = format!("{:.10}", value);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn format_section(value: f64, section: &str) -> String {
    // What the section is, worked out once for each code (a sheet formats
    // hundreds of thousands of numbers with a few codes).
    let parsed = match PARSED_SECTIONS.with(|c| c.borrow().get(section).cloned()) {
        Some(parsed) => parsed,
        None => {
            let parsed = std::rc::Rc::new(parse_section(section));
            PARSED_SECTIONS.with(|c| {
                let mut c = c.borrow_mut();
                if c.len() > 256 {
                    c.clear();
                }
                c.insert(section.to_string(), parsed.clone());
            });
            parsed
        }
    };
    let p = match &parsed.kind {
        SectionKind::General => return format_general(value),
        SectionKind::Date => return format_date(value, section),
        // "General" with literal text around it, such as `General"件"`.
        SectionKind::GeneralWith(before, after) => return format!("{before}{}{after}", format_general(value)),
        // The text format shows the value in the text around it.
        SectionKind::Digits(_) if parsed.text_at => &parse_digits(section, Some(value)),
        SectionKind::Digits(p) => p,
    };
    if p.digits.is_empty() {
        return format!("{}{}", p.before, p.after);
    }
    let mut value = if p.percent { value * 100.0 } else { value };
    // Commas after the last placeholder scale by a thousand each.
    value /= 1000f64.powi(p.trailing_commas as i32);
    let (decimals, required_decimals) = (p.decimals, p.required_decimals);

    // Excel rounds halves away from zero; formatting alone rounds them to even.
    let factor = 10f64.powi(decimals as i32);
    let scaled = (value * factor).round();
    let rounded = if scaled.abs() < 1e15 && decimals <= 15 {
        // In whole numbers, quicker than formatting to a precision, and the
        // same digits.
        let n = scaled as i64;
        let unit = 10i64.pow(decimals as u32);
        let int = (n / unit).abs();
        let sign = if n < 0 || (n == 0 && scaled.is_sign_negative()) { "-" } else { "" };
        if decimals == 0 { format!("{sign}{int}") } else { format!("{sign}{int}.{:0width$}", (n % unit).abs(), width = decimals) }
    } else {
        format!("{:.*}", decimals, scaled / factor)
    };
    let (int_digits, frac_digits) = rounded.split_once('.').unwrap_or((&rounded, ""));
    // Optional decimals (#) drop trailing zeros.
    let mut frac_digits = frac_digits;
    while frac_digits.len() > required_decimals && frac_digits.ends_with('0') {
        frac_digits = &frac_digits[..frac_digits.len() - 1];
    }
    let int_digits = int_digits.trim_start_matches('0');
    let mut number = String::with_capacity(int_digits.len() + int_digits.len() / 3 + frac_digits.len() + p.before.len() + p.after.len() + 4);
    number.push_str(&p.before);
    // Zeros up to the placeholders' count, then the digits, grouped by three.
    let int_len = int_digits.len().max(p.min_int_digits);
    let padded = std::iter::repeat_n('0', int_len - int_digits.len()).chain(int_digits.chars());
    for (i, c) in padded.enumerate() {
        if p.grouping && i > 0 && (int_len - i) % 3 == 0 {
            number.push(p.group_char.unwrap_or(','));
        }
        number.push(c);
    }
    if !frac_digits.is_empty() {
        number.push('.');
        number.push_str(frac_digits);
    }
    number.push_str(&p.after);
    number
}

thread_local! {
    /// Number format sections split up, by the section.
    static PARSED_SECTIONS: std::cell::RefCell<std::collections::HashMap<String, std::rc::Rc<ParsedSection>>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// What a number format section is.
struct ParsedSection {
    kind: SectionKind,
    /// Has the text placeholder `@`, which shows the value itself.
    text_at: bool,
}

enum SectionKind {
    General,
    Date,
    /// `General` with literal text before and after it.
    GeneralWith(String, String),
    Digits(DigitSection),
}

/// A section of digit placeholders: the literal text before and after its
/// run of them, and what the run asks for.
struct DigitSection {
    before: String,
    after: String,
    digits: String,
    percent: bool,
    /// Commas after the last placeholder, each dividing by a thousand.
    trailing_commas: usize,
    decimals: usize,
    required_decimals: usize,
    grouping: bool,
    /// The character between the groups (`###\ ###`: a space); a comma if none is given.
    group_char: Option<char>,
    min_int_digits: usize,
}

fn parse_section(section: &str) -> ParsedSection {
    let text_at = section.contains('@');
    let kind = if section.trim().is_empty() {
        SectionKind::General
    } else if is_date_format(section) {
        SectionKind::Date
    } else if let Some(at) = section.to_ascii_lowercase().find("general") {
        SectionKind::GeneralWith(literal(&section[..at]), literal(&section[at + 7..]))
    } else {
        SectionKind::Digits(parse_digits(section, None))
    };
    ParsedSection { kind, text_at }
}

/// Split `section` into its text and digit placeholders; `@` shows `value`.
fn parse_digits(section: &str, value: Option<f64>) -> DigitSection {
    let mut before = String::new();
    let mut after = String::new();
    let mut digits = String::new();
    let mut percent = false;
    let mut group_char: Option<char> = None;
    let mut chars = section.chars().peekable();
    while let Some(c) = chars.next() {
        // Placeholders continue the digit run until literal text follows it.
        let in_digits = after.is_empty();
        let target = if digits.is_empty() { &mut before } else { &mut after };
        match c {
            '"' => {
                for q in chars.by_ref() {
                    if q == '"' {
                        break;
                    }
                    target.push(q);
                }
            }
            // A literal between two runs of placeholders (`###\ ###`) is the
            // character between the groups of thousands, not text after the number.
            '\\' if in_digits && !digits.is_empty() && {
                let mut ahead = chars.clone();
                ahead.next();
                matches!(ahead.next(), Some('#' | '0' | '?'))
            } =>
            {
                group_char = chars.next();
                digits.push(',');
            }
            '\\' => target.extend(chars.next()),
            '[' => {
                // Colours and locales, such as [Red] or [$-411].
                for q in chars.by_ref() {
                    if q == ']' {
                        break;
                    }
                }
            }
            '_' => {
                chars.next();
                target.push(' ');
            }
            '*' => {
                chars.next();
            }
            '%' => {
                percent = true;
                target.push('%');
            }
            // The text format shows the value as it is.
            '@' => {
                if let Some(value) = value {
                    target.push_str(&format_general(value));
                }
            }
            '0' | '#' | '?' | '.' | ',' if in_digits => digits.push(c),
            other => target.push(other),
        }
    }
    let trailing_commas = digits.len() - digits.trim_end_matches(',').len();
    let run = digits.trim_end_matches(',');
    let (int_part, frac_part) = run.split_once('.').unwrap_or((run, ""));
    DigitSection {
        decimals: frac_part.chars().filter(|c| matches!(c, '0' | '#' | '?')).count(),
        required_decimals: frac_part.chars().filter(|c| *c == '0').count(),
        grouping: int_part.contains(','),
        group_char,
        min_int_digits: int_part.chars().filter(|c| *c == '0').count(),
        trailing_commas,
        before,
        after,
        digits,
        percent,
    }
}

/// A section with date parts (`y`, `d`, or `m` outside quotes and brackets).
fn is_date_format(section: &str) -> bool {
    let mut quoted = false;
    let mut bracket = false;
    for c in section.chars() {
        match c {
            '"' => quoted = !quoted,
            '[' if !quoted => bracket = true,
            ']' if !quoted => bracket = false,
            'y' | 'Y' | 'd' | 'D' | 'm' | 'M' if !quoted && !bracket => return true,
            // An era (`gg`, `ggg`), but not the `g` of General.
            'g' | 'G' if !quoted && !bracket && !section.to_ascii_lowercase().contains("general") => return true,
            _ => {}
        }
    }
    false
}

/// The civil date of an Excel serial day number (1900 date system:
/// day 1 is 1900-01-01, with the historical 1900-02-29 at day 60).
pub(crate) fn serial_to_date(serial: f64) -> (i64, u32, u32) {
    let days = serial.floor() as i64;
    // Days since 1970-01-01; serials from day 61 on count from 1899-12-30.
    let unix = if days >= 61 { days - 25_569 } else { days - 25_568 };
    // Howard Hinnant's civil-from-days.
    let z = unix + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + if month <= 2 { 1 } else { 0 };
    (year, month, day)
}

/// Format a date serial with a code such as `yyyy/m`, `yyyy"年"m"月"`,
/// `mmm-yy` or `d`. Times of day are not shown.
fn format_date(value: f64, section: &str) -> String {
    // The cells' formatter knows the rest of the date codes (an era year `ggge`, the weekday `aaa`, a time of day).
    let formatted = ibis_calc::numfmt::format(&ibis_calc::CellValue::Number(value), section, false);
    if !formatted.overflow && !formatted.text.is_empty() {
        return formatted.text;
    }
    format_date_basic(value, section)
}

fn format_date_basic(value: f64, section: &str) -> String {
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let (year, month, day) = serial_to_date(value);
    let mut out = String::new();
    let chars: Vec<char> = section.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let run = chars[i..].iter().take_while(|&&x| x.eq_ignore_ascii_case(&c)).count();
        match c.to_ascii_lowercase() {
            '"' => {
                let end = chars[i + 1..].iter().position(|&x| x == '"').map(|p| i + 1 + p).unwrap_or(chars.len());
                out.extend(&chars[i + 1..end]);
                i = end + 1;
                continue;
            }
            '\\' => {
                if let Some(next) = chars.get(i + 1) {
                    out.push(*next);
                }
                i += 2;
                continue;
            }
            '[' => {
                i += chars[i..].iter().position(|&x| x == ']').map(|p| p + 1).unwrap_or(chars.len() - i);
                continue;
            }
            'y' => out.push_str(&if run <= 2 { format!("{:02}", year.rem_euclid(100)) } else { year.to_string() }),
            'm' => {
                let name = MONTHS[(month - 1) as usize];
                out.push_str(&match run {
                    1 => month.to_string(),
                    2 => format!("{month:02}"),
                    3 => name[..3].to_string(),
                    5 => name[..1].to_string(),
                    _ => name.to_string(),
                });
            }
            'd' => out.push_str(&match run {
                1 => day.to_string(),
                _ => format!("{day:02}"),
            }),
            '_' | '*' => {
                i += 2;
                continue;
            }
            _ => out.extend(std::iter::repeat_n(c, run)),
        }
        i += run;
    }
    out
}

/// The text of a format section without digit placeholders.
fn literal(section: &str) -> String {
    let mut out = String::new();
    let mut chars = section.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => out.extend(chars.by_ref().take_while(|q| *q != '"')),
            '\\' => out.extend(chars.next()),
            '[' => {
                chars.by_ref().take_while(|q| *q != ']').for_each(drop);
            }
            '_' => {
                chars.next();
                out.push(' ');
            }
            '*' => {
                chars.next();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formats() {
        assert_eq!(format_number(1694.0, "#,##0"), "1,694");
        assert_eq!(format_number(1234.0, "@"), "1234");
        assert_eq!(format_number(0.65123, "0.0%"), "65.1%");
        assert_eq!(format_number(-12.5, "#,##0.0;△#,##0.0"), "△12.5");
        assert_eq!(format_number(3.0, r#"#,##0"億円""#), "3億円");
        assert_eq!(format_number(0.097, "General"), "0.097");
        assert_eq!(format_number(1234567.0, "#,##0,"), "1,235");
        assert_eq!(format_number(5.0, "0.0#"), "5.0");
        assert_eq!(format_number(-3.0, "0"), "-3");
        assert_eq!(format_number(0.5, "0"), "1"); // rounds half away from zero like Excel
        assert_eq!(format_number(1234.5678, "#,##0.00"), "1,234.57");
        assert_eq!(format_number(5.0, "0,000"), "0,005");
        assert_eq!(format_number(0.4, "#,##0"), "0");
        assert_eq!(format_number(0.4, "#,###"), "");
        assert_eq!(format_number(7.0, r#"@"件""#), "7件");
        assert_eq!(format_number(0.05, "0.0"), "0.1");
        assert_eq!(format_number(12.0, "0.000"), "12.000");
        assert_eq!(format_number(1e17, "#,##0"), "100,000,000,000,000,000");
        // The quick whole-number formatting gives the digits formatting to a
        // precision gives.
        let mut seed: u64 = 1;
        for _ in 0..200_000 {
            seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            let value = (seed >> 11) as f64 / (1u64 << 53) as f64 * 10f64.powi((seed % 13) as i32);
            for decimals in 0..4 {
                let code = if decimals == 0 { "0".to_string() } else { format!("0.{}", "0".repeat(decimals)) };
                let factor = 10f64.powi(decimals as i32);
                assert_eq!(format_number(value, &code), format!("{:.*}", decimals, (value * factor).round() / factor), "{value} {code}");
            }
        }
        assert_eq!(format_number(12.0, r#"General"件""#), "12件");
    }

    #[test]
    fn date_formats() {
        // 45078 is 2023-06-01.
        assert_eq!(format_number(45078.0, "yyyy/m"), "2023/6");
        assert_eq!(format_number(45078.0, r#"yyyy"年"m"月""#), "2023年6月");
        assert_eq!(format_number(45078.0, "mmm-yy"), "Jun-23");
        // An era year (the cells' formatter shows it).
        assert_eq!(format_number(42856.0, r#"ggge"年""#), "平成29年");
        assert_eq!(format_number(45778.0, r#"ggge"年""#), "令和7年");
        assert_eq!(format_number(45078.0, "yyyy/mm/dd"), "2023/06/01");
        assert_eq!(format_number(1.0, "yyyy/m/d"), "1900/1/1");
        assert_eq!(format_number(61.0, "yyyy/m/d"), "1900/3/1");
    }
}
