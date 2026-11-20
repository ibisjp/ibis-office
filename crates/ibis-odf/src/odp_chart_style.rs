// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The formatting of an embedded chart of an `.odp` (ODF 1.3 chapter 11: the chart's automatic styles) written into the `c:chartSpace` made
//! by the `.ods` chart writer, as PowerPoint writes it when it saves the `.odp` as `.pptx` (measured with topptx on 007: the title's
//! `a:defRPr sz` is the title paragraph style's font size in hundredths of a point (0.25861in = 1862), the axis text 0.13889in = 1000, the
//! data labels 0.16625in = 1197, a grid line's `a:ln w` is `svg:stroke-width` in EMU (0.01042in = 9528) with its `svg:stroke-color`, the
//! axis line, the tick marks, the legend's text, the chart area's and the plot area's fill and outline come from their styles).

use std::collections::HashMap;
use std::fmt::Write;

use roxmltree::{Document, Node};

const CHART: &str = "urn:oasis:names:tc:opendocument:xmlns:chart:1.0";
const OFFICE: &str = "urn:oasis:names:tc:opendocument:xmlns:office:1.0";
const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const DR3D: &str = "urn:oasis:names:tc:opendocument:xmlns:dr3d:1.0";
const SVG: &str = "urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0";

type Props = HashMap<String, String>;

/// The properties of every automatic style by name (the attributes of its property elements by local name).
fn styles_of(root: Node) -> HashMap<String, Props> {
    let mut styles: HashMap<String, Props> = HashMap::new();
    for holder in root.children().filter(|c| c.is_element() && c.tag_name().name() == "automatic-styles") {
        for st in holder.children().filter(|c| c.is_element() && c.tag_name().name() == "style") {
            let Some(name) = st.attribute((STYLE, "name")) else { continue };
            let entry = styles.entry(name.to_string()).or_default();
            for props in st.children().filter(|c| c.is_element()) {
                for a in props.attributes() {
                    entry.insert(a.name().to_string(), a.value().to_string());
                }
            }
        }
    }
    styles
}

/// A length in points ("0.13889in", "10pt", "0.5cm").
fn points(v: &str) -> Option<f64> {
    let v = v.trim();
    let (n, k) = if let Some(n) = v.strip_suffix("in") {
        (n, 72.0)
    } else if let Some(n) = v.strip_suffix("pt") {
        (n, 1.0)
    } else if let Some(n) = v.strip_suffix("cm") {
        (n, 72.0 / 2.54)
    } else if let Some(n) = v.strip_suffix("mm") {
        (n, 72.0 / 25.4)
    } else {
        return None;
    };
    Some(n.trim().parse::<f64>().ok()? * k)
}

fn rgb(v: &str) -> Option<String> {
    let v = v.trim().strip_prefix('#')?;
    (v.len() == 6).then(|| v.to_ascii_uppercase())
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// `a:defRPr` (or `a:rPr`) of the text properties of a style.
fn text_props(m: &Props, tag: &str, lang: bool) -> String {
    let mut attrs = String::new();
    if lang {
        attrs.push_str(r#" lang="ja-JP""#);
    }
    if let Some(sz) = m.get("font-size").and_then(|v| points(v)) {
        let _ = write!(attrs, r#" sz="{}""#, (sz * 100.0).round() as i64);
    }
    let _ = write!(attrs, r#" b="{}""#, u8::from(m.get("font-weight").is_some_and(|w| w == "bold")));
    let _ = write!(attrs, r#" i="{}""#, u8::from(m.get("font-style").is_some_and(|w| w == "italic")));
    let mut inner = String::new();
    if let Some(c) = m.get("color").and_then(|c| rgb(c)) {
        let _ = write!(inner, r#"<a:solidFill><a:srgbClr val="{c}"/></a:solidFill>"#);
    }
    if let Some(f) = m.get("font-family") {
        let _ = write!(inner, r#"<a:latin typeface="{}"/>"#, esc(f.trim_matches('\'')));
    }
    if let Some(f) = m.get("font-family-asian") {
        let _ = write!(inner, r#"<a:ea typeface="{}"/>"#, esc(f.trim_matches('\'')));
    }
    format!("<a:{tag}{attrs}>{inner}</a:{tag}>")
}

/// `c:txPr` of the text properties of a style.
fn tx_pr(m: &Props) -> String {
    format!(
        r#"<c:txPr><a:bodyPr lIns="0" tIns="0" rIns="0" bIns="0"/><a:lstStyle/><a:p><a:pPr>{}</a:pPr><a:endParaRPr lang="ja-JP"/></a:p></c:txPr>"#,
        text_props(m, "defRPr", false)
    )
}

/// `a:ln` of the graphic properties of a style.
fn line(m: &Props) -> String {
    match m.get("stroke").map(String::as_str) {
        Some("solid") => {
            let w = m.get("stroke-width").and_then(|v| points(v)).map(|p| format!(r#" w="{}""#, (p * 12_700.0).round() as i64)).unwrap_or_default();
            match m.get("stroke-color").and_then(|c| rgb(c)) {
                Some(c) => format!(r#"<a:ln{w}><a:solidFill><a:srgbClr val="{c}"/></a:solidFill></a:ln>"#),
                None => format!("<a:ln{w}/>"),
            }
        }
        Some("none") => "<a:ln><a:noFill/></a:ln>".to_string(),
        _ => String::new(),
    }
}

/// `c:spPr` of the graphic properties of a style: its fill and its outline.
fn sp_pr(m: &Props) -> String {
    let fill = match m.get("fill").map(String::as_str) {
        Some("none") => "<a:noFill/>".to_string(),
        Some("solid") => m.get("fill-color").and_then(|c| rgb(c)).map(|c| format!(r#"<a:solidFill><a:srgbClr val="{c}"/></a:solidFill>"#)).unwrap_or_default(),
        _ => String::new(),
    };
    format!("<c:spPr>{fill}{}</c:spPr>", line(m))
}

fn tick(m: &Props, which: &str) -> &'static str {
    let on = |k: &str| m.get(k).is_some_and(|v| v == "true");
    match (on(&format!("tick-marks-{which}-inner")), on(&format!("tick-marks-{which}-outer"))) {
        (true, true) => "cross",
        (true, false) => "in",
        (false, true) => "out",
        _ => "none",
    }
}

/// The part of `xml` from the first `open` to the matching `close` (the first one after it), as byte positions.
fn span(xml: &str, from: usize, open: &str, close: &str) -> Option<(usize, usize)> {
    let a = xml[from..].find(open)? + from;
    let b = xml[a..].find(close)? + a + close.len();
    Some((a, b))
}

/// `xml` (a chart part made by the `.ods` chart writer) with the formatting of the chart whose `content.xml` is `content`.
pub(crate) fn style_chart(xml: String, content: &str, reversed: bool) -> String {
    let Ok(doc) = Document::parse_with_options(content, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }) else { return xml };
    let root = doc.root_element();
    let styles = styles_of(root);
    let empty = Props::new();
    let style = |n: Node| -> &Props { n.attribute((CHART, "style-name")).and_then(|s| styles.get(s)).unwrap_or(&empty) };
    let Some(chart_el) = root.descendants().find(|n| n.is_element() && n.tag_name().namespace() == Some(CHART) && n.tag_name().name() == "chart") else { return xml };
    let Some(plot) = chart_el.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "plot-area") else { return xml };
    let mut xml = xml;

    // The title: the title paragraph's style, else the title's own.
    if let Some(title) = chart_el.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "title") {
        let p_style = title.descendants().find(|n| n.is_element() && n.tag_name().namespace() == Some(TEXT) && n.tag_name().name() == "p").and_then(|p| p.attribute((TEXT, "style-name")));
        let m = p_style.and_then(|s| styles.get(s)).unwrap_or_else(|| style(title));
        if let Some((a, b)) = span(&xml, 0, "<c:title>", "</c:title>") {
            let part = &xml[a..b];
            if part.contains("<a:p><a:r><a:t>") && !part.contains("<a:defRPr") {
                let algn = if m.get("text-align").is_none_or(|t| t == "center") { r#" algn="ctr""# } else { "" };
                let new = part
                    .replacen("<a:bodyPr/>", r#"<a:bodyPr lIns="0" tIns="0" rIns="0" bIns="0"/>"#, 1)
                    .replacen("<a:p><a:r><a:t>", &format!("<a:p><a:pPr{algn}>{}</a:pPr><a:r>{}<a:t>", text_props(m, "defRPr", false), text_props(m, "rPr", true)), 1)
                    .replacen(r#"<c:overlay val="0"/>"#, r#"<c:overlay val="0"/><c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>"#, 1);
                xml.replace_range(a..b, &new);
            }
        }
    }

    // The axes: x then y of the primary pair, then the secondary pair.
    let axes: Vec<Node> = plot.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "axis").collect();
    let axis_of = |dimension: &str, nth: usize| axes.iter().filter(|a| a.attribute((CHART, "dimension")) == Some(dimension)).nth(nth).copied();
    let mut seen = [0usize; 2];
    let mut at = 0usize;
    loop {
        let next_cat = xml[at..].find("<c:catAx>").map(|i| i + at);
        let next_val = xml[at..].find("<c:valAx>").map(|i| i + at);
        let (start, is_cat) = match (next_cat, next_val) {
            (Some(c), Some(v)) if c < v => (c, true),
            (Some(c), None) => (c, true),
            (_, Some(v)) => (v, false),
            _ => break,
        };
        let close = if is_cat { "</c:catAx>" } else { "</c:valAx>" };
        let Some((a, b)) = span(&xml, start, if is_cat { "<c:catAx>" } else { "<c:valAx>" }, close) else { break };
        // A chart with no category axis (a scatter) has its x axis a value axis.
        let dimension = if is_cat || (!xml.contains("<c:catAx>") && seen[1] == 0 && !xml[..a].contains("<c:valAx>")) { "x" } else { "y" };
        let idx = usize::from(dimension == "y");
        let odf = axis_of(dimension, seen[idx]);
        seen[idx] += 1;
        let mut new = xml[a..b].to_string();
        if let Some(odf) = odf {
            let m = style(odf);
            if !new.contains("<c:majorTickMark") {
                new = new.replacen("<c:tickLblPos", &format!(r#"<c:majorTickMark val="{}"/><c:minorTickMark val="{}"/><c:tickLblPos"#, tick(m, "major"), tick(m, "minor")), 1);
            }
            if !new.contains("<c:txPr>") {
                new = new.replacen("<c:crossAx", &format!("{}{}<c:crossAx", sp_pr(m), tx_pr(m)), 1);
            }
            // Grid lines: the style of the grid the axis names.
            for (class, tag) in [("major", "majorGridlines"), ("minor", "minorGridlines")] {
                let grid = odf.children().find(|g| g.is_element() && g.tag_name().namespace() == Some(CHART) && g.tag_name().name() == "grid" && g.attribute((CHART, "class")) == Some(class));
                if let Some(g) = grid {
                    new = new.replacen(&format!("<c:{tag}/>"), &format!("<c:{tag}><c:spPr>{}</c:spPr></c:{tag}>", line(style(g))), 1);
                }
            }
        }
        xml.replace_range(a..b, &new);
        at = a + new.len();
    }

    // The legend.
    if let Some(legend) = chart_el.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "legend") {
        if let Some((a, b)) = span(&xml, 0, "<c:legend>", "</c:legend>") {
            if !xml[a..b].contains("<c:txPr>") {
                let m = style(legend);
                let new = xml[a..b].replacen("</c:legend>", &format!("{}{}</c:legend>", sp_pr(m), tx_pr(m)), 1);
                xml.replace_range(a..b, &new);
            }
        }
    }

    // The data labels of each series (the series the writer kept, in order).
    let mut series: Vec<Node> = plot.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "series").collect();
    if reversed {
        series.reverse();
    }
    let mut at = 0usize;
    let mut kept = series.iter();
    while let Some((a, b)) = span(&xml, at, "<c:ser>", "</c:ser>") {
        let Some(s) = kept.next() else { break };
        let sm = style(*s);
        let number = sm.get("data-label-number").map(String::as_str).unwrap_or("none");
        let mut new = xml[a..b].to_string();
        // The outline of a filled series (bars, areas): PowerPoint writes `a:ln` from the series' stroke, `<a:ln><a:noFill/></a:ln>` for
        // `draw:stroke="none"` (measured with topptx on 007: 0.01042in solid #4f7ae9 is `a:ln w="9528"` with that colour).
        if let (Some(i), Some(j)) = (new.find("<c:spPr>"), new.find("</c:spPr>")) {
            if i < j && !new[i..j].contains("<a:ln") {
                let ln = line(sm);
                new.insert_str(j, &ln);
            }
        }
        // The marker of a line or scatter series: `chart:symbol-type="named-symbol"` with `chart:symbol-name="circle"` is
        // `<c:marker><c:symbol val="circle"/><c:size val="N"/></c:marker>` with no fill of its own, N the symbol's width in points (0.06944in
        // = 5; measured with topptx on 007's line and scatter charts, whatever the series' fill and stroke). Other symbols are not measured.
        if sm.get("symbol-type").map(String::as_str) == Some("named-symbol") && sm.get("symbol-name").map(String::as_str) == Some("circle") {
            if let (Some(i), Some(j)) = (new.find("<c:marker>"), new.find("</c:marker>")) {
                if let Some(w) = sm.get("symbol-width").and_then(|v| points(v)) {
                    let marker = format!(r#"<c:marker><c:symbol val="circle"/><c:size val="{}"/>"#, w.round() as i64);
                    new.replace_range(i..j, &marker);
                }
            }
        }
        if number != "none" && !new.contains("<c:dLbls>") {
            let label = s.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "data-label");
            let lm = label.map(|l| style(l)).unwrap_or(&empty);
            let val = u8::from(number == "value" || number == "value-and-percentage");
            let pct = u8::from(number == "percentage" || number == "value-and-percentage");
            let points = point_labels(*s, chart_el, &styles);
            let d = format!(
                r#"<c:dLbls>{points}<c:spPr><a:noFill/><a:ln><a:noFill/></a:ln></c:spPr>{}<c:showLegendKey val="0"/><c:showVal val="{val}"/><c:showCatName val="0"/><c:showSerName val="0"/><c:showPercent val="{pct}"/><c:showBubbleSize val="0"/></c:dLbls>"#,
                tx_pr(lm)
            );
            for marker in ["<c:cat>", "<c:xVal>", "<c:val>"] {
                if new.contains(marker) {
                    new = new.replacen(marker, &format!("{d}{marker}"), 1);
                    break;
                }
            }
        }
        xml.replace_range(a..b, &new);
        at = a + new.len();
    }

    // The plot area (its wall) and the chart area.
    let wall = plot.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "wall");
    let is_3d = style(plot).get("three-dimensional").is_some_and(|v| v == "true") && xml.contains("<c:barChart>");
    // A 3-D chart's walls are `c:sideWall` and `c:backWall` (below); its plot area keeps the plot area's own style.
    let wall_style = wall.map(|w| style(w)).filter(|m| !m.is_empty() && !is_3d).unwrap_or_else(|| style(plot));
    if !wall_style.is_empty() && xml.contains("</c:plotArea>") {
        let tail = xml.rfind("</c:plotArea>").unwrap_or(0);
        let before = &xml[..tail];
        if !before.ends_with("</c:spPr>") {
            xml.insert_str(tail, &sp_pr(wall_style));
        }
    }
    let area = style(chart_el);
    if !area.is_empty() && !xml.contains("</c:chart><c:spPr>") {
        xml = xml.replacen("</c:chart>", &format!("</c:chart>{}", sp_pr(area)), 1);
    }
    if is_3d {
        xml = three_d(xml, plot, &styles);
    }
    let _ = OFFICE;
    xml
}

/// The labels a series moved by hand: for each `chart:data-point` whose `chart:data-label` has `svg:x` and `svg:y` (a position in points from
/// the chart's top left corner) and the style `chart:auto-position="false"`, a `c:dLbl` whose `c:manualLayout` is `xMode`/`yMode` edge with
/// `x = svg:x / chart width` and `y = svg:y / chart height` (measured with topptx on 153's line chart: 388.1277pt wide and 160.7646pt high,
/// the label at (39.90pt, 55.84pt) is x 0.1028, y 0.3473, and the three other points agree to 4 digits). The point's own style gives what it shows.
fn point_labels(series: Node, chart_el: Node, styles: &HashMap<String, Props>) -> String {
    let len = |n: Node, a: &str| n.attribute((SVG, a)).and_then(points);
    let (Some(w), Some(h)) = (len(chart_el, "width"), len(chart_el, "height")) else { return String::new() };
    let mut out = String::new();
    let mut idx = 0usize;
    for dp in series.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "data-point") {
        let count = dp.attribute((CHART, "repeated")).and_then(|r| r.parse::<usize>().ok()).unwrap_or(1);
        let label = dp.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "data-label");
        if let (Some(l), true) = (label, count == 1) {
            let manual = l.attribute((CHART, "style-name")).and_then(|n| styles.get(n)).is_some_and(|m| m.get("auto-position").is_some_and(|v| v == "false"));
            if let (true, Some(x), Some(y)) = (manual, len(l, "x"), len(l, "y")) {
                let m = dp.attribute((CHART, "style-name")).and_then(|n| styles.get(n));
                let number = m.and_then(|m| m.get("data-label-number")).map(String::as_str).unwrap_or("none");
                let val = u8::from(number == "value" || number == "value-and-percentage");
                let pct = u8::from(number == "percentage" || number == "value-and-percentage");
                let _ = write!(
                    out,
                    r#"<c:dLbl><c:idx val="{idx}"/><c:layout><c:manualLayout><c:xMode val="edge"/><c:yMode val="edge"/><c:x val="{}"/><c:y val="{}"/></c:manualLayout></c:layout><c:showLegendKey val="0"/><c:showVal val="{val}"/><c:showCatName val="0"/><c:showSerName val="0"/><c:showPercent val="{pct}"/><c:showBubbleSize val="0"/></c:dLbl>"#,
                    x / w,
                    y / h
                );
            }
        }
        idx += count;
    }
    out
}

/// `a:ln` of a chart floor: PowerPoint writes `cap="flat"` and, for a solid stroke, `a:prstDash` and `a:round` (measured with topptx: a
/// floor with the 0.01042in #8f8f8f stroke is `a:ln w="9528" cap="flat"` + solidFill 8F8F8F + prstDash solid + round; no stroke is
/// `<a:ln cap="flat"><a:noFill/></a:ln>`).
fn floor_line(m: &Props) -> String {
    match m.get("stroke").map(String::as_str) {
        Some("solid") => {
            let w = m.get("stroke-width").and_then(|v| points(v)).map(|p| format!(r#" w="{}""#, (p * 12_700.0).round() as i64)).unwrap_or_default();
            match m.get("stroke-color").and_then(|c| rgb(c)) {
                Some(c) => format!(r#"<a:ln{w} cap="flat"><a:solidFill><a:srgbClr val="{c}"/></a:solidFill><a:prstDash val="solid"/><a:round/></a:ln>"#),
                None => format!(r#"<a:ln{w} cap="flat"/>"#),
            }
        }
        Some("none") => r#"<a:ln cap="flat"><a:noFill/></a:ln>"#.to_string(),
        _ => String::new(),
    }
}

fn mul3(a: [[f64; 3]; 3], b: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut c = [[0.0; 3]; 3];
    for (i, row) in c.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            *v = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    c
}

/// `c:rotX` and `c:rotY` (whole degrees) PowerPoint writes for the scene `dr3d:transform` of a chart (`matrix (m00 m01 m02 m10 .. m22 tx ty tz)`).
///
/// Measured with topptx on 38 charts whose transform was a rotation about x by `a` and about y by `b` (a from -100 to 60 degrees, b from -200
/// to 300): PowerPoint does not read `dr3d:vpn`, `dr3d:vrp`, `dr3d:vup`, `dr3d:distance` or `dr3d:focal-length` (13 variants of them gave
/// the same angles), only the matrix. The identity matrix is rotX 10, rotY 25, so the scene is `T * B` with `B = Ry(25 deg) * Rx(-10 deg)`
/// (`Rx`, `Ry` the right-handed rotations, rows as written in the attribute); then `rotX = -atan2(-s12, hypot(s02, s22))` and
/// `rotY = atan2(s02, s22)` of `s = T * B`, rotY taken modulo 360, and both truncated toward zero (the values 14.99995 and 19.99986 of a
/// real chart's matrix are written 14 and 19; -8.17 is -8, 54.86 is 54). All 38 agree.
fn view_angles(transform: &str) -> (i32, i32) {
    let n: Vec<f64> = transform
        .trim()
        .trim_start_matches("matrix")
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split_whitespace()
        .filter_map(|t| t.trim_end_matches("cm").parse::<f64>().ok())
        .collect();
    let t = if n.len() >= 9 { [[n[0], n[1], n[2]], [n[3], n[4], n[5]], [n[6], n[7], n[8]]] } else { [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] };
    let (sx, cx) = (-10f64).to_radians().sin_cos();
    let (sy, cy) = 25f64.to_radians().sin_cos();
    let b = mul3([[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]], [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]]);
    let s = mul3(t, b);
    let rot_x = -(-s[1][2]).atan2(s[0][2].hypot(s[2][2])).to_degrees();
    let rot_y = s[0][2].atan2(s[2][2]).to_degrees().rem_euclid(360.0);
    // 1e-9: floating point noise only (the identity must stay 10 and 25).
    let trunc = |v: f64| (v + 1e-9f64.copysign(v)).trunc() as i32;
    (trunc(rot_x), trunc(rot_y))
}

/// A chart with `chart:three-dimensional="true"` (bars) as PowerPoint writes it: `c:bar3DChart` with `c:view3D`, `c:floor`, `c:sideWall`,
/// `c:backWall`, `c:shape` (measured with topptx on 125 and 153 and on 40 variants of 125's chart):
/// - `chart:right-angled-axes` is `c:rAngAx` (absent: 1, as 153 has none and gets 1); with it false `c:perspective` is 46 for
///   `dr3d:projection="perspective"` whatever `dr3d:distance` and `dr3d:focal-length` (9 pairs) and 0 for `parallel`; with it true no
///   `c:perspective` is written;
/// - `chart:deep` (series in depth) with no stacking is `c:grouping` standard, otherwise (false) clustered; stacked ones keep their grouping;
///   the series axis `c:serAx` (deleted) exists unless the grouping is clustered, where the third `c:axId` is 0;
/// - the wall style is `c:sideWall` and `c:backWall` (`c:thickness` 0), the floor style is `c:floor`;
/// - the chart-level `c:shape` is `box`; a series' `chart:solid-type` cylinder, cone or pyramid is its `c:shape` (cuboid writes none);
/// - `c:overlap` does not exist in a 3-D bar chart.
/// A 3-D bar chart of an `.ods` (the same chart part as an `.odp`'s, written by `chart_xml`): only the 3-D view, walls, floor and the bar group
/// become `c:view3D`, `c:bar3DChart` and so on. Excel reads `dr3d:transform` as PowerPoint does (measured with `toxlsx` on a clustered
/// 3-D column chart: the matrix gives rotX 0.906 and rotY 18.997 before the base of 10 and 25 degrees, and Excel wrote rotX 10, rotY 43,
/// rAngAx 1, shape box, clustered, the same as `view_angles` and `three_d`).
pub(crate) fn three_d_chart(xml: String, content: &str) -> String {
    let Ok(doc) = Document::parse_with_options(content, roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() }) else { return xml };
    let root = doc.root_element();
    let styles = styles_of(root);
    let Some(chart_el) = root.descendants().find(|n| n.is_element() && n.tag_name().namespace() == Some(CHART) && n.tag_name().name() == "chart") else { return xml };
    let Some(plot) = chart_el.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "plot-area") else { return xml };
    let is_3d = plot.attribute((CHART, "style-name")).and_then(|s| styles.get(s)).and_then(|m| m.get("three-dimensional")).is_some_and(|v| v == "true") && xml.contains("<c:barChart>");
    if is_3d { three_d(xml, plot, &styles) } else { xml }
}

fn three_d(mut xml: String, plot: Node, styles: &HashMap<String, Props>) -> String {
    let empty = Props::new();
    let style = |n: Node| -> &Props { n.attribute((CHART, "style-name")).and_then(|s| styles.get(s)).unwrap_or(&empty) };
    let ps = style(plot);
    let child = |name: &str| plot.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == name);
    let (rot_x, rot_y) = view_angles(plot.attribute((DR3D, "transform")).unwrap_or(""));
    let right_angled = ps.get("right-angled-axes").is_none_or(|v| v == "true");
    let mut view = format!(r#"<c:view3D><c:rotX val="{rot_x}"/><c:rotY val="{rot_y}"/><c:rAngAx val="{}"/>"#, u8::from(right_angled));
    if !right_angled {
        let persp = if plot.attribute((DR3D, "projection")) == Some("parallel") { 0 } else { 46 };
        let _ = write!(view, r#"<c:perspective val="{persp}"/>"#);
    }
    view.push_str("</c:view3D>");
    let wall = child("wall").map(|w| style(w)).unwrap_or(&empty);
    let floor = child("floor").map(|f| style(f)).unwrap_or(&empty);
    let wall_pr = sp_pr(wall);
    let fill = match floor.get("fill").map(String::as_str) {
        Some("none") => "<a:noFill/>".to_string(),
        Some("solid") => floor.get("fill-color").and_then(|c| rgb(c)).map(|c| format!(r#"<a:solidFill><a:srgbClr val="{c}"/></a:solidFill>"#)).unwrap_or_default(),
        _ => String::new(),
    };
    let _ = write!(
        view,
        r#"<c:floor><c:thickness val="0"/><c:spPr>{fill}{}</c:spPr></c:floor><c:sideWall><c:thickness val="0"/>{wall_pr}</c:sideWall><c:backWall><c:thickness val="0"/>{wall_pr}</c:backWall>"#,
        floor_line(floor)
    );
    if let Some(i) = xml.find("<c:plotArea>") {
        xml.insert_str(i, &view);
    }
    // The chart group: series in depth is the standard grouping, otherwise clustered.
    let deep = ps.get("deep").is_some_and(|v| v == "true");
    let plain = xml.contains(r#"<c:grouping val="standard"/>"#) || xml.contains(r#"<c:grouping val="clustered"/>"#);
    if plain {
        let grouping = if deep { "standard" } else { "clustered" };
        xml = xml.replacen(r#"<c:grouping val="standard"/>"#, &format!(r#"<c:grouping val="{grouping}"/>"#), 1).replacen(r#"<c:grouping val="clustered"/>"#, &format!(r#"<c:grouping val="{grouping}"/>"#), 1);
    }
    let clustered = plain && !deep;
    xml = xml.replacen("<c:barChart>", "<c:bar3DChart>", 1).replacen("</c:barChart>", "</c:bar3DChart>", 1);
    let (Some(a), Some(b)) = (xml.find("<c:bar3DChart>"), xml.find("</c:bar3DChart>")) else { return xml };
    let mut group = xml[a..b].to_string();
    if let Some(i) = group.find("<c:overlap ") {
        if let Some(j) = group[i..].find("/>") {
            group.replace_range(i..i + j + 2, "");
        }
    }
    // A series' shape goes at the end of the series.
    let series: Vec<Node> = plot.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(CHART) && c.tag_name().name() == "series").collect();
    let mut at = 0usize;
    let mut kept = series.iter();
    while let Some((x, y)) = span(&group, at, "<c:ser>", "</c:ser>") {
        let Some(s) = kept.next() else { break };
        let mut new = group[x..y].to_string();
        if let Some(shape @ ("cylinder" | "cone" | "pyramid")) = style(*s).get("solid-type").map(String::as_str) {
            let end = new.rfind("<c:extLst>").unwrap_or(new.len() - "</c:ser>".len());
            new.insert_str(end, &format!(r#"<c:shape val="{shape}"/>"#));
        }
        group.replace_range(x..y, &new);
        at = x + new.len();
    }
    let ids: Vec<u64> = group.match_indices("<c:axId val=\"").filter_map(|(i, _)| group[i + 13..].split('"').next()?.parse().ok()).collect();
    let third = if clustered { 0 } else { ids.iter().copied().max().unwrap_or(0) + 1 };
    if let Some(i) = group.find("<c:axId ") {
        group.insert_str(i, r#"<c:shape val="box"/>"#);
    }
    let _ = write!(group, r#"<c:axId val="{third}"/>"#);
    xml.replace_range(a..b, &group);
    if !clustered {
        let val_id = ids.get(1).copied().unwrap_or(0);
        let ser_ax = format!(
            r#"<c:serAx><c:axId val="{third}"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="1"/><c:axPos val="b"/><c:numFmt formatCode="General" sourceLinked="0"/><c:majorTickMark val="out"/><c:minorTickMark val="none"/><c:tickLblPos val="nextTo"/><c:crossAx val="{val_id}"/><c:crosses val="autoZero"/></c:serAx>"#
        );
        let end = ["</c:valAx>", "</c:catAx>"].iter().filter_map(|t| xml.rfind(t).map(|i| i + t.len())).max();
        if let Some(i) = end {
            xml.insert_str(i, &ser_ax);
        }
    }
    xml
}

#[cfg(test)]
mod tests_3d {
    use super::view_angles;

    fn rot(rx: f64, ry: f64) -> String {
        let (sx, cx) = rx.to_radians().sin_cos();
        let (sy, cy) = ry.to_radians().sin_cos();
        let m = [cy, sy * sx, sy * cx, 0.0, cx, -sx, -sy, cy * sx, cy * cx];
        format!("matrix ({} 0cm 0cm 0cm)", m.iter().map(|v| format!("{v:.12}")).collect::<Vec<_>>().join(" "))
    }

    /// Measured with topptx (the angles PowerPoint wrote for each `dr3d:transform`).
    #[test]
    fn angles_match_powerpoint() {
        let real = "matrix (0.99564446382742067 -2.9808720977666348E-2 -8.833765790127851E-2 3.6833428844505495E-2 0.99619477459360894 7.8989047291030706E-2 8.5646950730101257E-2 -8.1898786474914415E-2 0.99295376962100956 0cm 0cm 0cm)";
        assert_eq!(view_angles(real), (14, 19));
        assert_eq!(view_angles("matrix (1 0 0 0 1 0 0 0 1 0cm 0cm 0cm)"), (10, 25));
        for (rx, ry, want) in [
            (10.0, 0.0, (0, 24)),
            (0.0, 25.0, (10, 50)),
            (20.0, 30.0, (-8, 54)),
            (-20.0, 0.0, (27, 28)),
            (30.0, 0.0, (-17, 25)),
            (5.0, 0.0, (5, 24)),
            (10.0, 10.0, (0, 34)),
            (0.0, -40.0, (10, 345)),
            (0.0, -200.0, (10, 185)),
            (0.0, 300.0, (10, 325)),
            (-70.0, 0.0, (63, 71)),
            (60.0, 0.0, (-43, 34)),
            (-100.0, 0.0, (58, 128)),
        ] {
            assert_eq!(view_angles(&rot(rx, ry)), want, "rx {rx} ry {ry}");
        }
    }
}
