// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust Mermaid diagram parser and SVG / native-chart renderer.
//!
//! Zero external dependencies: converts Mermaid code blocks into either:
//! - Native Office charts (`pie`, `xychart-beta`) via [`MermaidChartSpec`], or
//! - Clean, self-contained SVG vector graphics (`sequenceDiagram`, `flowchart` / `graph`)
//!   embedded as `MediaFormat::Svg` with the original Mermaid source preserved in `PictureLook::alt_text`
//!   for lossless `.md` round-tripping.
//!
//! This file detects the diagram type and dispatches; each diagram type has its own module
//! (`flowchart`, `sequence`, `class`, `er`, `state`, `gantt`, `mindmap`, `gitgraph`, `journey`,
//! `timeline`, `quadrant`, `chart`), with shared code in `config` (frontmatter and themes),
//! `layout` (layered layout, edge routing, grouped frames) and `text` (labels and wrapping).
//! The modules share one namespace: each starts with `use super::*;`.

use std::collections::HashMap;

use ibis_doc_model::{Number, Side};

use crate::save::pptx_new_chart::NewChartKind;

mod config;
mod chart;
mod sequence;
mod flowchart;
mod layout;
mod state;
mod class;
mod er;
mod gantt;
mod mindmap;
mod gitgraph;
mod journey;
mod timeline;
mod quadrant;
mod requirement;
mod kanban;
mod packet;
mod sankey;
mod block;
mod architecture;
mod c4;
mod treemap;
mod venn;
mod radar;
mod usecase;
mod wardley;
mod treeview;
mod swimlanes;
mod eventmodeling;
mod agentflow;
mod ishikawa;
mod cynefin;
mod zenuml;
mod railroad;
pub mod plantuml;
mod plantuml_uml;
mod plantuml_extra;
mod plantuml_timing;
mod plantuml_non_uml;
mod plantuml_style;
mod text;
#[cfg(test)]
mod test_support;

use config::*;
use chart::*;
use sequence::*;
use flowchart::*;
use layout::*;
use state::*;
use class::*;
use er::*;
use gantt::*;
use mindmap::*;
use gitgraph::*;
use journey::*;
use timeline::*;
use quadrant::*;
use requirement::*;
use kanban::*;
use packet::*;
use block::*;
use architecture::*;
use c4::*;
use treemap::*;
use venn::*;
use radar::*;
use usecase::*;
use wardley::*;
use treeview::*;
use swimlanes::*;
use eventmodeling::*;
use agentflow::*;
use ishikawa::*;
use cynefin::*;
use zenuml::*;
use railroad::*;
use sankey::*;
use text::*;

pub use chart::render_chart_spec_svg;
pub use plantuml::{
    PlantUmlConversion, PlantUmlNativeRender, PlantUmlUnsupported, convert_plantuml_sequence,
    render_plantuml_sequence_native,
};
pub use plantuml_uml::render_plantuml;

/// Convert a PlantUML basic sequence diagram to Mermaid and render it with the existing
/// Mermaid sequence renderer when conversion produced a sequence diagram.
///
/// Always returns the [`PlantUmlConversion`] (preserving `original` and line-numbered
/// `unsupported` diagnostics) alongside the rendered [`MermaidOutput`] if available.
pub fn render_plantuml_sequence(source: &str) -> (PlantUmlConversion, Option<MermaidOutput>) {
    let conversion = convert_plantuml_sequence(source);
    let rendered = conversion.mermaid.as_deref().and_then(render_mermaid);
    (conversion, rendered)
}

/// Parsed native chart specification from Mermaid `pie` or `xychart-beta`.
#[derive(Debug, Clone, PartialEq)]
pub struct MermaidChartSpec {
    pub kind: NewChartKind,
    pub horizontal: bool,
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<(String, Vec<Option<Number>>)>,
    /// One drawing kind per series. Empty means every series uses [`Self::kind`].
    pub series_kinds: Vec<NewChartKind>,
    /// `pie showData`: the slices show their values.
    pub show_data: bool,
    /// `xychart` axis titles (`x-axis "Month" [...]`, `y-axis "Revenue" 0 --> 100`).
    pub x_axis_title: Option<String>,
    /// Numeric x-axis bounds. Samples in each series are spaced evenly across this range.
    pub x_range: Option<(f64, f64)>,
    pub y_axis_title: Option<String>,
    /// `xychart` value-axis range (`y-axis 4000 --> 11000`); `None` is automatic.
    pub y_range: Option<(f64, f64)>,
    /// `xychart` point labels (`line [540 "PaLM", 65]`): per series, per point. Empty: none.
    pub point_labels: Vec<Vec<Option<String>>>,
    /// Drawing options from the frontmatter `config`.
    pub style: MermaidChartStyle,
}

/// Pie legend placement. OOXML supports the four sides; Mermaid also allows a centered overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieLegendPosition {
    Side(Side),
    Center,
}

/// The frontmatter `config` options of a `pie` / `xychart` that change how it is drawn.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MermaidChartStyle {
    /// Series (xychart, `themeVariables.xyChart.plotColorPalette`) or slice (pie, `pie1`..`pie12`)
    /// colours as `#rrggbb`, in order. Empty: the built-in palette.
    pub palette: Vec<String>,
    /// `xyChart.showDataLabel`: each bar / point shows its value.
    pub data_labels: bool,
    /// `xyChart.showDataLabelOutsideBar`: bar values sit above the bar instead of inside it.
    pub data_labels_outside: bool,
    /// `xyChart.width` / `xyChart.height`, in px.
    pub size_px: Option<(u32, u32)>,
    /// `themeVariables.xyChart.titleColor` as `#rrggbb`.
    pub title_color: Option<String>,
    /// `pie.textPosition`: where a slice's label sits, 0 (centre) to 1 (rim); Mermaid's default is 0.75.
    pub text_position: Option<f64>,
    /// `pie.donutHole`: the hole as a fraction of the radius (0 < hole <= 0.9).
    pub donut_hole: Option<f64>,
    /// `pie.legendPosition`: placement relative to the plot.
    pub pie_legend: Option<PieLegendPosition>,
    /// `pie.highlightSlice`: the label of the slice that is pulled out.
    pub highlight: Option<String>,
    /// `themeVariables.pieOuterStrokeWidth`, in px.
    pub outer_stroke_width: Option<f64>,
}

/// Result of converting a Mermaid block.
#[derive(Debug, Clone, PartialEq)]
pub enum MermaidOutput {
    /// Native Office chart (`pie`, `xychart-beta`).
    Chart(MermaidChartSpec),
    /// Self-contained SVG vector graphic (`svg_bytes`, `width_px`, `height_px`).
    Svg {
        bytes: Vec<u8>,
        width_px: u32,
        height_px: u32,
    },
}

/// Diagram families this renderer understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiagramKind {
    Pie,
    XyChart,
    Sequence,
    Flowchart,
    State,
    Class,
    Er,
    Gantt,
    Mindmap,
    GitGraph,
    Journey,
    Timeline,
    Quadrant,
    Requirement,
    Kanban,
    Packet,
    Sankey,
    Block,
    Architecture,
    C4,
    Treemap,
    Venn,
    Radar,
    Usecase,
    Wardley,
    TreeView,
    Swimlanes,
    EventModeling,
    Agentflow,
    Ishikawa,
    Cynefin,
    ZenUml,
    Railroad,
}

/// Header keywords, including the aliases current mermaid still accepts
/// (`graph`, `flowchart-elk`, `classDiagram-v2`, `stateDiagram-v2`, `xychart-beta`).
/// Matching is on the first word, so `gitGraph:`, `graph LR;` and
/// `xychart horizontal` are recognised too.
const DIAGRAM_HEADERS: &[(&str, DiagramKind)] = &[
    ("pie", DiagramKind::Pie),
    ("xychart", DiagramKind::XyChart),
    ("xychart-beta", DiagramKind::XyChart),
    ("sequenceDiagram", DiagramKind::Sequence),
    ("flowchart", DiagramKind::Flowchart),
    ("flowchart-v2", DiagramKind::Flowchart),
    ("flowchart-elk", DiagramKind::Flowchart),
    ("graph", DiagramKind::Flowchart),
    ("stateDiagram", DiagramKind::State),
    ("stateDiagram-v2", DiagramKind::State),
    ("classDiagram", DiagramKind::Class),
    ("classDiagram-v2", DiagramKind::Class),
    ("erDiagram", DiagramKind::Er),
    ("gantt", DiagramKind::Gantt),
    ("mindmap", DiagramKind::Mindmap),
    ("gitGraph", DiagramKind::GitGraph),
    ("journey", DiagramKind::Journey),
    ("timeline", DiagramKind::Timeline),
    ("quadrantChart", DiagramKind::Quadrant),
    ("requirementDiagram", DiagramKind::Requirement),
    ("kanban", DiagramKind::Kanban),
    ("packet", DiagramKind::Packet),
    ("packet-beta", DiagramKind::Packet),
    ("sankey", DiagramKind::Sankey),
    ("sankey-beta", DiagramKind::Sankey),
    ("block", DiagramKind::Block),
    ("block-beta", DiagramKind::Block),
    ("architecture-beta", DiagramKind::Architecture),
    ("architecture", DiagramKind::Architecture),
    ("C4Context", DiagramKind::C4),
    ("C4Container", DiagramKind::C4),
    ("C4Component", DiagramKind::C4),
    ("C4Dynamic", DiagramKind::C4),
    ("C4Deployment", DiagramKind::C4),
    ("treemap", DiagramKind::Treemap),
    ("treemap-beta", DiagramKind::Treemap),
    ("venn", DiagramKind::Venn),
    ("venn-beta", DiagramKind::Venn),
    ("radar", DiagramKind::Radar),
    ("radar-beta", DiagramKind::Radar),
    ("usecase", DiagramKind::Usecase),
    ("usecase-beta", DiagramKind::Usecase),
    ("wardley", DiagramKind::Wardley),
    ("wardley-beta", DiagramKind::Wardley),
    ("treeView-beta", DiagramKind::TreeView),
    ("treeView", DiagramKind::TreeView),
    ("swimlane-beta", DiagramKind::Swimlanes),
    ("swimlane", DiagramKind::Swimlanes),
    ("eventmodeling", DiagramKind::EventModeling),
    ("agentflow-beta", DiagramKind::Agentflow),
    ("agentflow", DiagramKind::Agentflow),
    ("ishikawa-beta", DiagramKind::Ishikawa),
    ("cynefin-beta", DiagramKind::Cynefin),
    ("zenuml", DiagramKind::ZenUml),
    ("railroad-beta", DiagramKind::Railroad),
    ("railroad-ebnf-beta", DiagramKind::Railroad),
    ("railroad-abnf-beta", DiagramKind::Railroad),
    ("railroad-peg-beta", DiagramKind::Railroad),
];

fn diagram_kind(header: &str) -> Option<DiagramKind> {
    let header = header.trim();
    let word_end = header
        .find(|c: char| c.is_whitespace() || c == ':' || c == ';')
        .unwrap_or(header.len());
    let word = &header[..word_end];
    DIAGRAM_HEADERS.iter().find(|(name, _)| *name == word).map(|(_, kind)| *kind)
}

/// Appends SVG text elements for font prefetch. Joining descendant text keeps
/// combining characters and variation selectors in the same shaping sequence.
pub fn append_svg_text(bytes: &[u8], output: &mut String) {
    let Ok(svg) = std::str::from_utf8(bytes) else { return };
    let Ok(document) = roxmltree::Document::parse(svg) else { return };
    for node in document
        .descendants()
        .filter(|node| node.has_tag_name(("http://www.w3.org/2000/svg", "text")))
    {
        output.push('\n');
        for child in node.descendants().filter(|child| child.is_text()) {
            output.push_str(child.text().unwrap_or_default());
        }
    }
}

#[cfg(test)]
#[test]
fn svg_font_prefetch_collects_text_runs_without_markup() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg"><title>metadata</title>
      <text>𠮷<tspan>&#xE0100;</tspan> &amp; ❤</text><text>確認</text>
    </svg>"#;
    let mut text = "本文".to_owned();
    append_svg_text(svg.as_bytes(), &mut text);
    assert_eq!(text, format!("本文\n𠮷{} & ❤\n確認", char::from_u32(0xE0100).unwrap()));
}

/// Parse and render a Mermaid diagram into either a native chart or an SVG.
/// Returns `None` if the diagram type is unsupported (caller falls back to a fenced code block).
pub fn render_mermaid(source: &str) -> Option<MermaidOutput> {
    let trimmed = strip_mermaid_frontmatter(source);
    if trimmed.is_empty() {
        return None;
    }
    let first_line = trimmed.lines().next()?.trim();
    let kind = diagram_kind(first_line)?;
    // A `title` statement in the body wins over the frontmatter title, so the title is drawn once.
    let body_title = trimmed
        .lines()
        .skip(1)
        .any(|line| line.trim_start().starts_with("title ") || line.trim() == "title");
    let front_title = mermaid_frontmatter_title(source).filter(|_| !body_title);
    let config = mermaid_frontmatter_config(source);
    // A Markdown string may run over several lines; the parsers read one statement per line.
    let joined = join_markdown_strings(trimmed);
    let trimmed = joined.as_ref();

    let svg = match kind {
        DiagramKind::Pie => {
            let mut spec = parse_pie_chart(trimmed, &config)?;
            spec.title = spec.title.or(front_title);
            if spec.style.pie_legend != Some(PieLegendPosition::Center) {
                return Some(MermaidOutput::Chart(spec));
            }
            let (bytes, width_px, height_px) = render_chart_spec_svg(&spec)?;
            return Some(MermaidOutput::Svg {
                bytes: inject_acc_meta(apply_mermaid_theme(bytes, &config), trimmed),
                width_px,
                height_px,
            });
        }
        DiagramKind::XyChart => {
            let mut spec = parse_xy_chart(trimmed, &config)?;
            spec.title = spec.title.or(front_title);
            if spec.x_range.is_none() && (!spec.horizontal || !spec.series_kinds.contains(&NewChartKind::Line)) {
                return Some(MermaidOutput::Chart(spec));
            }
            let (bytes, width_px, height_px) = render_chart_spec_svg(&spec)?;
            return Some(MermaidOutput::Svg {
                bytes: inject_acc_meta(apply_mermaid_theme(bytes, &config), trimmed),
                width_px,
                height_px,
            });
        }
        DiagramKind::Sequence => render_sequence_svg(trimmed),
        DiagramKind::Flowchart => render_flowchart_svg(trimmed),
        DiagramKind::State => render_state_diagram_svg(trimmed),
        DiagramKind::Class => render_class_diagram_svg(trimmed, &config),
        DiagramKind::Er => render_er_diagram_svg(trimmed),
        DiagramKind::Gantt => render_gantt_svg(trimmed, &config),
        DiagramKind::Mindmap => render_mindmap_svg(trimmed, &config),
        DiagramKind::GitGraph => render_git_graph_svg(trimmed, &config),
        DiagramKind::Journey => render_journey_svg(trimmed),
        DiagramKind::Timeline => render_timeline_svg(trimmed, &config),
        DiagramKind::Quadrant => render_quadrant_chart_svg(trimmed, &config),
        DiagramKind::Requirement => render_requirement_svg(trimmed),
        DiagramKind::Kanban => render_kanban_svg(trimmed, &config),
        DiagramKind::Packet => render_packet_svg(trimmed, &config),
        DiagramKind::Sankey => render_sankey_svg(trimmed, &config),
        DiagramKind::Block => render_block_svg(trimmed),
        DiagramKind::Architecture => render_architecture_svg(trimmed),
        DiagramKind::C4 => render_c4_svg(trimmed),
        DiagramKind::Treemap => render_treemap_svg(trimmed, &config),
        DiagramKind::Venn => render_venn_svg(trimmed),
        DiagramKind::Radar => render_radar_svg(trimmed, &config),
        DiagramKind::Usecase => render_usecase_svg(trimmed, &config),
        DiagramKind::Wardley => render_wardley_svg(trimmed),
        DiagramKind::TreeView => render_treeview_svg(trimmed, &config),
        DiagramKind::Swimlanes => render_swimlanes_svg(trimmed),
        DiagramKind::EventModeling => render_eventmodeling_svg(trimmed),
        DiagramKind::Agentflow => render_agentflow_svg(trimmed, &config),
        DiagramKind::Ishikawa => render_ishikawa_svg(trimmed),
        DiagramKind::Cynefin => render_cynefin_svg(trimmed, &config),
        DiagramKind::ZenUml => render_zenuml_svg(trimmed),
        DiagramKind::Railroad => render_railroad_svg(trimmed),
    };
    let (mut bytes, mut width_px, mut height_px) = svg?;
    if let Some(title) = front_title {
        (bytes, width_px, height_px) = add_svg_title(bytes, width_px, height_px, &title);
    }
    let bytes = apply_mermaid_theme(bytes, &config);
    let bytes = inject_acc_meta(bytes, trimmed);
    Some(MermaidOutput::Svg {
        bytes,
        width_px,
        height_px,
    })
}

/// Put mermaid `accTitle` / `accDescr` on the root SVG for accessibility (and mermaid.js parity).
fn inject_acc_meta(bytes: Vec<u8>, source: &str) -> Vec<u8> {
    let (title, descr) = parse_acc_meta(source);
    if title.is_none() && descr.is_none() {
        return bytes;
    }
    let Ok(svg) = std::str::from_utf8(&bytes) else {
        return bytes;
    };
    let Some(open_end) = svg.find('>') else {
        return bytes;
    };
    if !svg.starts_with("<svg ") || open_end == 0 {
        return bytes;
    }
    let mut insert = String::new();
    if let Some(t) = &title {
        insert.push_str(&format!("<title>{}</title>", escape_xml(t)));
    }
    if let Some(d) = &descr {
        insert.push_str(&format!("<desc>{}</desc>", escape_xml(d)));
    }
    let mut out = String::with_capacity(svg.len() + insert.len());
    out.push_str(&svg[..=open_end]);
    out.push_str(&insert);
    out.push_str(&svg[open_end + 1..]);
    out.into_bytes()
}

/// Put a title band above one of this module's SVGs: the drawing moves down
/// (and is centred when the title is wider than it).
fn add_svg_title(bytes: Vec<u8>, width: u32, height: u32, title: &str) -> (Vec<u8>, u32, u32) {
    let Ok(svg) = std::str::from_utf8(&bytes) else {
        return (bytes, width, height);
    };
    let (Some(open_end), Some(close)) = (svg.find('>'), svg.rfind("</svg>")) else {
        return (bytes, width, height);
    };
    if !svg.starts_with("<svg ") || close <= open_end {
        return (bytes, width, height);
    }
    let band = 40u32;
    let title_w = (estimate_text_width(title, 16.0) + 32.0).ceil() as u32;
    let new_w = width.max(title_w);
    let new_h = height + band;
    let dx = (new_w - width) as f32 / 2.0;
    let mut out = String::with_capacity(svg.len() + 400);
    out.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{new_w}" height="{new_h}" viewBox="0 0 {new_w} {new_h}" font-family="sans-serif">"##
    ));
    out.push_str(&format!(r##"<rect width="{new_w}" height="{new_h}" fill="#ffffff"/>"##));
    out.push_str(&format!(
        r##"<text x="{:.1}" y="27" text-anchor="middle" font-size="16" font-weight="bold" fill="#0f172a" data-role="diagram-title">{}</text>"##,
        new_w as f32 / 2.0,
        escape_xml(title)
    ));
    out.push_str(&format!(r##"<g transform="translate({dx:.1},{band})">"##));
    out.push_str(&svg[open_end + 1..close]);
    out.push_str("</g></svg>");
    (out.into_bytes(), new_w, new_h)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn unsupported_diagram_returns_none() {
        assert!(render_mermaid("").is_none());
    }

    #[test]
    fn renders_mindmap_and_gitgraph_and_chart_svg_fallback() {
        let mm = "\
mindmap
  root((ibis-office))
    Markdown
      Mermaid
      Math
    Formats
      DOCX
      PPTX
";
        let Some(MermaidOutput::Svg { bytes, width_px, height_px }) = render_mermaid(mm) else {
            panic!("expected svg for mindmap");
        };
        assert!(width_px > 200 && height_px > 120);
        let mm_svg = std::str::from_utf8(&bytes).unwrap();
        assert!(mm_svg.contains("ibis-office"));
        assert!(mm_svg.contains("Mermaid"));

        let git = "\
gitGraph
  commit id: \"init\"
  branch feat
  checkout feat
  commit id: \"wip\"
  checkout main
  merge feat tag: \"v1.0\"
";
        let Some(MermaidOutput::Svg { bytes: gbytes, width_px: gw, height_px: gh }) = render_mermaid(git) else {
            panic!("expected svg for gitGraph");
        };
        assert!(gw > 250 && gh > 80);
        let git_svg = std::str::from_utf8(&gbytes).unwrap();
        assert!(git_svg.contains("main"));
        assert!(git_svg.contains("feat"));
        assert!(git_svg.contains("v1.0"));

        let pie = "pie\n  title Share\n  \"A\" : 60\n  \"B\" : 40\n";
        let Some(MermaidOutput::Chart(spec)) = render_mermaid(pie) else {
            panic!("expected chart spec");
        };
        let (svg_bytes, pw, ph) = render_chart_spec_svg(&spec).expect("svg fallback");
        assert!(pw > 300 && ph > 200);
        let pie_svg = std::str::from_utf8(&svg_bytes).unwrap();
        assert!(pie_svg.contains("Share"));
        assert!(pie_svg.contains("60.0%"));
    }

    #[test]
    fn renders_journey_timeline_quadrant_and_frontmatter() {
        let journey = "\
---
title: User Onboarding
---
journey
  title My Day
  section Morning
    Coffee: 5: Me
    Commute: 2: Me, Train
";
        let Some(MermaidOutput::Svg { bytes: jbytes, width_px: jw, height_px: jh }) = render_mermaid(journey) else {
            panic!("expected svg for journey with YAML frontmatter");
        };
        assert!(jw > 200 && jh > 180);
        let jsvg = std::str::from_utf8(&jbytes).unwrap();
        assert!(jsvg.contains("Coffee"));
        assert!(jsvg.contains("Commute"));

        let tl = "\
timeline
  title History
  2024 : Alpha
  2025 : Beta : GA
";
        let Some(MermaidOutput::Svg { bytes: tbytes, .. }) = render_mermaid(tl) else {
            panic!("expected svg for timeline");
        };
        let tsvg = std::str::from_utf8(&tbytes).unwrap();
        assert!(tsvg.contains("2024"));
        assert!(tsvg.contains("Alpha"));
        assert!(tsvg.contains("GA"));

        let qc = "\
quadrantChart
  title Priority Matrix
  x-axis Low Effort --> High Effort
  y-axis Low Impact --> High Impact
  quadrant-1 Strategic
  quadrant-2 Quick Wins
  Task A: [0.2, 0.8]
  Task B: [0.75, 0.3]
";
        let Some(MermaidOutput::Svg { bytes: qbytes, .. }) = render_mermaid(qc) else {
            panic!("expected svg for quadrantChart");
        };
        let qsvg = std::str::from_utf8(&qbytes).unwrap();
        assert!(qsvg.contains("Priority Matrix"));
        assert!(qsvg.contains("Quick Wins"));
        assert!(qsvg.contains("Task A"));
    }

    #[test]
    fn header_aliases_reach_the_same_renderers() {
        for (alias, canonical) in [
            ("classDiagram-v2\n  class Animal\n  Animal <|-- Duck\n", "classDiagram\n  class Animal\n  Animal <|-- Duck\n"),
            ("stateDiagram-v2\n  [*] --> Still\n", "stateDiagram\n  [*] --> Still\n"),
            ("graph LR\n  A --> B\n", "flowchart LR\n  A --> B\n"),
            ("flowchart-elk LR\n  A --> B\n", "flowchart LR\n  A --> B\n"),
            ("flowchart-v2 LR\n  A --> B\n", "flowchart LR\n  A --> B\n"),
            ("gitGraph:\n  commit\n  branch dev\n  commit\n", "gitGraph\n  commit\n  branch dev\n  commit\n"),
        ] {
            assert_eq!(render_mermaid(alias), render_mermaid(canonical), "{alias}");
            assert!(render_mermaid(alias).is_some(), "{alias}");
        }
        // `graph LR;` keeps its direction: the two nodes sit side by side.
        let svg = svg_of("graph LR;\n  A-->B;\n");
        assert!((text_y(&svg, "A") - text_y(&svg, "B")).abs() < 1.0, "{svg}");
        assert!(text_x(&svg, "B") > text_x(&svg, "A") + 40.0);
        // Only whole words count as headers.
        assert!(render_mermaid("graphql\n  A --> B\n").is_none());
        assert!(render_mermaid("classDiagramX\n  class A\n").is_none());
        assert!(render_mermaid("sankeyX\n  A,B,10\n").is_none());
        assert_eq!(render_mermaid("sankey-beta\n  A,B,10\n"), render_mermaid("sankey\n  A,B,10\n"));
        assert_eq!(render_mermaid("packet-beta\n  0-7: \"A\"\n"), render_mermaid("packet\n  0-7: \"A\"\n"));
        assert_eq!(diagram_kind("xychart horizontal"), Some(DiagramKind::XyChart));
        assert_eq!(diagram_kind("sequenceDiagram"), Some(DiagramKind::Sequence));
    }

    #[test]
    fn frontmatter_title_is_drawn_above_svg_diagrams() {
        let plain = svg_of("flowchart LR\n  A --> B\n");
        let titled_src = "---\ntitle: \"受注の流れ\"\nconfig:\n  title: nested is ignored\n---\nflowchart LR\n  A --> B\n";
        let Some(MermaidOutput::Svg { bytes, width_px, height_px }) = render_mermaid(titled_src) else {
            panic!("svg");
        };
        let titled = String::from_utf8(bytes).unwrap();
        assert!(titled.contains(r#"data-role="diagram-title">受注の流れ</text>"#), "{titled}");
        assert!(!titled.contains("nested is ignored"));
        let (pw, ph) = view_box(&plain);
        assert_eq!(width_px as f32, pw.max(width_px as f32));
        assert_eq!(height_px as f32, ph + 40.0);
        assert!(titled.contains(r#"<g transform="translate("#));
        assert!(roxmltree::Document::parse(&titled).is_ok());
        // A long title widens the canvas and centres the drawing.
        let long = svg_of("---\ntitle: A very long diagram title that is wider than the single node below\n---\nflowchart LR\n  A\n");
        let (lw, _) = view_box(&long);
        assert!(lw > view_box(&svg_of("flowchart LR\n  A\n")).0);
        // Charts take it as their title unless the body has one.
        let Some(MermaidOutput::Chart(pie)) = render_mermaid("---\ntitle: Pets\n---\npie\n  \"Dogs\" : 3\n  \"Cats\" : 2\n") else {
            panic!("pie");
        };
        assert_eq!(pie.title.as_deref(), Some("Pets"));
        let Some(MermaidOutput::Chart(pie)) = render_mermaid("---\ntitle: Pets\n---\npie title Body\n  title Body\n  \"Dogs\" : 3\n") else {
            panic!("pie");
        };
        assert_eq!(pie.title.as_deref(), Some("Body"));
        // A diagram with its own title statement is not titled twice.
        let seq = svg_of("---\ntitle: Front\n---\nsequenceDiagram\n  title Body title\n  A->>B: hi\n");
        assert!(seq.contains(">Body title<"));
        assert!(!seq.contains(">Front<"));
        assert_eq!(mermaid_frontmatter_title("---\ntitle: 'Quoted'\n---\ngraph\n"), Some("Quoted".to_string()));
        assert_eq!(mermaid_frontmatter_title("graph\n  A\n"), None);
    }
}
