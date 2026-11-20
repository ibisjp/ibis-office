// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Office Chart Template (`.crtx`) reading, writing, and application.
//!
//! A `.crtx` file is an Open Packaging Conventions (OPC) package storing custom
//! chart formatting, layouts, styles, axes, legends, and series formatting in
//! `chart/chart.xml` (`<c:chartSpace>`). When applied to an existing chart, the
//! template's layout and styling are transferred while preserving the target chart's
//! data ranges and series values.

use ibis_doc_model::chart::{
    AxisKind, Chart, ChartType, DispBlanksAs, Grouping, LabelPosition, LegendPosition,
    MarkerSymbol, Number, OfPieKind, OfPieSplit, RadarStyle, Side,
};
use ibis_doc_model::{Color, Fill, Stroke};
use ibis_ooxml::zip::{Archive, EntryMeta, Writer};
use std::sync::Arc;

use crate::Error;

const CONTENT_TYPES_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/chart/chart.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/>
</Types>"#;

const RELS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart" Target="chart/chart.xml"/>
</Relationships>"#;

/// Escapes special XML characters.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn write_color_xml(color: Color) -> String {
    format!("<a:srgbClr val=\"{:06X}\"/>", color.rgb.0 & 0xFFFFFF)
}

fn write_fill_xml(fill: &Fill) -> String {
    match fill {
        Fill::None => "<a:noFill/>".to_string(),
        Fill::Solid(color) => format!("<a:solidFill>{}</a:solidFill>", write_color_xml(*color)),
        _ => String::new(),
    }
}

fn write_stroke_xml(stroke: Option<&Stroke>) -> String {
    match stroke {
        None => "<a:ln><a:noFill/></a:ln>".to_string(),
        Some(s) => {
            let fill = write_fill_xml(&Fill::Solid(s.color));
            format!("<a:ln w=\"{}\">{}</a:ln>", s.width, fill)
        }
    }
}

fn marker_symbol_str(sym: MarkerSymbol) -> &'static str {
    match sym {
        MarkerSymbol::None => "none",
        MarkerSymbol::Circle => "circle",
        MarkerSymbol::Square => "square",
        MarkerSymbol::Diamond => "diamond",
        MarkerSymbol::Triangle => "triangle",
    }
}

fn label_pos_str(pos: LabelPosition) -> &'static str {
    match pos {
        LabelPosition::Center => "ctr",
        LabelPosition::InsideEnd => "inEnd",
        LabelPosition::InsideBase => "inBase",
        LabelPosition::OutsideEnd => "outEnd",
        LabelPosition::Above => "t",
        LabelPosition::Below => "b",
        LabelPosition::Left => "l",
        LabelPosition::Right => "r",
        LabelPosition::BestFit => "bestFit",
    }
}

fn safe_label_pos_str(pos: LabelPosition, chart_tag: &str) -> &'static str {
    match chart_tag {
        "lineChart" | "line3DChart" | "areaChart" | "area3DChart" | "scatterChart" | "bubbleChart" | "radarChart" | "stockChart" | "surfaceChart" | "surface3DChart" => {
            match pos {
                LabelPosition::OutsideEnd | LabelPosition::InsideEnd => "t",
                LabelPosition::InsideBase => "b",
                LabelPosition::Center => "ctr",
                LabelPosition::Above => "t",
                LabelPosition::Below => "b",
                LabelPosition::Left => "l",
                LabelPosition::Right => "r",
                LabelPosition::BestFit => "bestFit",
            }
        }
        _ => label_pos_str(pos),
    }
}

fn legend_pos_str(pos: Option<LegendPosition>, side: Side) -> &'static str {
    if let Some(p) = pos {
        match p {
            LegendPosition::Bottom => "b",
            LegendPosition::Left => "l",
            LegendPosition::Right => "r",
            LegendPosition::Top => "t",
            LegendPosition::TopRight => "tr",
        }
    } else {
        match side {
            Side::Bottom => "b",
            Side::Left => "l",
            Side::Right => "r",
            Side::Top => "t",
        }
    }
}

fn axis_side_str(side: Side) -> &'static str {
    match side {
        Side::Bottom => "b",
        Side::Left => "l",
        Side::Right => "r",
        Side::Top => "t",
    }
}

/// Serializes a `Chart` model into a valid ECMA-376 DrawingML `<c:chartSpace>` XML string.
pub fn chart_to_xml(chart: &Chart) -> Result<String, Error> {
    let mut out = String::with_capacity(4096);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n");
    out.push_str("<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\r\n");
    out.push_str("<c:date1904 val=\"0\"/><c:lang val=\"ja-JP\"/><c:roundedCorners val=\"0\"/>\r\n");

    if let Some(style_id) = chart.style {
        out.push_str(&format!("<c:style val=\"{style_id}\"/>\r\n"));
    }

    if let Some(pivot_name) = &chart.pivot_source {
        let escaped = xml_escape(pivot_name);
        out.push_str(&format!("<c:pivotSource><c:name>{escaped}</c:name><c:fmtId val=\"0\"/></c:pivotSource>\r\n"));
    }

    out.push_str("<c:chart>\r\n");

    // Title
    if let Some(t) = &chart.title {
        let escaped = xml_escape(&t.text);
        out.push_str(&format!(
            "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr/><a:t>{escaped}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val=\"0\"/></c:title>\r\n"
        ));
    } else {
        out.push_str("<c:autoTitleDeleted val=\"1\"/>\r\n");
    }

    // PlotArea
    out.push_str("<c:plotArea><c:layout/>\r\n");

    // Chart Groups
    for group in &chart.groups {
        let (tag, extra_attrs) = match &group.kind {
            ChartType::Bar { horizontal, gap_width, overlap } => {
                let dir = if *horizontal { "bar" } else { "col" };
                let grp = match group.grouping {
                    Grouping::Standard => "clustered",
                    Grouping::Stacked => "stacked",
                    Grouping::PercentStacked => "percentStacked",
                };
                (
                    "barChart",
                    format!(
                        "<c:barDir val=\"{dir}\"/><c:grouping val=\"{grp}\"/><c:varyColors val=\"0\"/><c:gapWidth val=\"{gap_width}\"/><c:overlap val=\"{overlap}\"/>"
                    ),
                )
            }
            ChartType::Line => {
                let grp = match group.grouping {
                    Grouping::Standard => "standard",
                    Grouping::Stacked => "stacked",
                    Grouping::PercentStacked => "percentStacked",
                };
                ("lineChart", format!("<c:grouping val=\"{grp}\"/><c:varyColors val=\"0\"/><c:marker val=\"1\"/>"))
            }
            ChartType::Area => {
                let grp = match group.grouping {
                    Grouping::Standard => "standard",
                    Grouping::Stacked => "stacked",
                    Grouping::PercentStacked => "percentStacked",
                };
                ("areaChart", format!("<c:grouping val=\"{grp}\"/><c:varyColors val=\"0\"/>"))
            }
            ChartType::Pie { first_angle } => {
                ("pieChart", format!("<c:varyColors val=\"1\"/><c:firstSliceAng val=\"{first_angle}\"/>"))
            }
            ChartType::Doughnut { first_angle, hole } => {
                ("doughnutChart", format!("<c:varyColors val=\"1\"/><c:firstSliceAng val=\"{first_angle}\"/><c:holeSize val=\"{hole}\"/>"))
            }
            ChartType::Scatter { smooth } => {
                let style = if *smooth { "smoothMarker" } else { "marker" };
                ("scatterChart", format!("<c:scatterStyle val=\"{style}\"/><c:varyColors val=\"0\"/>"))
            }
            ChartType::Bubble { scale } => {
                ("bubbleChart", format!("<c:varyColors val=\"0\"/><c:bubbleScale val=\"{scale}\"/><c:showNegBubbles val=\"0\"/><c:sizeRepresents val=\"area\"/>"))
            }
            ChartType::Radar { style } => {
                let st = match style {
                    Some(RadarStyle::Filled) => "filled",
                    Some(RadarStyle::Marker) => "marker",
                    _ => "standard",
                };
                ("radarChart", format!("<c:radarStyle val=\"{st}\"/><c:varyColors val=\"0\"/>"))
            }
            ChartType::OfPie { kind } => {
                let k = match kind {
                    Some(OfPieKind::Bar) => "bar",
                    _ => "pie",
                };
                let split_type = group.of_pie.as_ref().and_then(|o| o.split).map(|s| match s {
                    OfPieSplit::Percent => "percent",
                    OfPieSplit::Value => "val",
                    OfPieSplit::Custom => "cust",
                    OfPieSplit::Position => "pos",
                }).unwrap_or("pos");
                let split_pos = group.of_pie.as_ref().and_then(|o| o.split_position).map(|Number(v)| v as u32).unwrap_or(2);
                let sec_size = group.of_pie.as_ref().and_then(|o| o.second_size).unwrap_or(75);
                (
                    "ofPieChart",
                    format!("<c:ofPieType val=\"{k}\"/><c:varyColors val=\"1\"/><c:splitType val=\"{split_type}\"/><c:splitPos val=\"{split_pos}\"/><c:secondPieSize val=\"{sec_size}\"/><c:serLines/>"),
                )
            }
            ChartType::Stock { up_down_bars, hi_low_lines, drop_lines } => {
                let mut extras = String::new();
                if *hi_low_lines { extras.push_str("<c:hiLowLines/>"); }
                if *up_down_bars { extras.push_str("<c:upDownBars><c:gapWidth val=\"150\"/><c:upBars/><c:downBars/></c:upDownBars>"); }
                if *drop_lines { extras.push_str("<c:dropLines/>"); }
                ("stockChart", extras)
            }
            ChartType::Surface { is_3d, wireframe } => {
                let tag = if *is_3d { "surface3DChart" } else { "surfaceChart" };
                let wf_val = if *wireframe { "1" } else { "0" };
                (tag, format!("<c:wireframe val=\"{wf_val}\"/>"))
            }
        };

        out.push_str(&format!("<c:{tag}>{extra_attrs}"));

        // Series
        for (s_idx, ser) in group.series.iter().enumerate() {
            out.push_str(&format!("<c:ser><c:idx val=\"{s_idx}\"/><c:order val=\"{s_idx}\"/>"));
            if !ser.name.is_empty() {
                let esc = xml_escape(&ser.name);
                out.push_str(&format!("<c:tx><c:v>{esc}</c:v></c:tx>"));
            }

            // spPr (Fill & Stroke)
            let fill_xml = write_fill_xml(&ser.fill);
            let stroke_xml = write_stroke_xml(ser.stroke.as_ref());
            if !fill_xml.is_empty() || !stroke_xml.is_empty() {
                out.push_str(&format!("<c:spPr>{fill_xml}{stroke_xml}</c:spPr>"));
            }

            // Marker
            if let Some(m) = &ser.marker {
                let sym = marker_symbol_str(m.symbol);
                let m_fill = write_fill_xml(&m.fill);
                let sp_pr = if !m_fill.is_empty() {
                    format!("<c:spPr>{m_fill}</c:spPr>")
                } else {
                    String::new()
                };
                out.push_str(&format!(
                    "<c:marker><c:symbol val=\"{sym}\"/><c:size val=\"{}\"/>{sp_pr}</c:marker>",
                    m.size
                ));
            }

            // Labels
            if let Some(l) = &ser.labels {
                let pos = l.position.map(|p| safe_label_pos_str(p, tag)).map(|p| format!("<c:dLblPos val=\"{p}\"/>")).unwrap_or_default();
                out.push_str(&format!(
                    "<c:dLbls><c:showVal val=\"{}\"/><c:showCatName val=\"{}\"/><c:showSerName val=\"{}\"/><c:showPercent val=\"{}\"/>{pos}</c:dLbls>",
                    if l.show_value { 1 } else { 0 },
                    if l.show_category { 1 } else { 0 },
                    if l.show_series_name { 1 } else { 0 },
                    if l.show_percent { 1 } else { 0 }
                ));
            }

            // Trendline
            if let Some(tl) = &ser.trendline {
                out.push_str(&format!(
                    "<c:trendline><c:trendlineType val=\"{}\"/><c:dispEq val=\"{}\"/><c:dispRSqr val=\"{}\"/></c:trendline>",
                    tl.kind.as_ooxml_str(),
                    if tl.disp_eq { 1 } else { 0 },
                    if tl.disp_r_sqr { 1 } else { 0 }
                ));
            }

            // ErrorBars
            if let Some(eb) = &ser.error_bars {
                out.push_str(&format!(
                    "<c:errBars><c:errDir val=\"{}\"/><c:errBarType val=\"{}\"/><c:errValType val=\"{}\"/></c:errBars>",
                    eb.direction.as_ooxml_str(),
                    eb.bar_type.as_ooxml_str(),
                    eb.val_type.as_ooxml_str()
                ));
            }

            // Placeholder Category/Value caches (standard sample data for template preview)
            let cat_count = ser.categories.len().max(3);
            let val_count = ser.values.len().max(3);
            out.push_str(&format!("<c:cat><c:strLit><c:ptCount val=\"{cat_count}\"/>"));
            for ci in 0..cat_count {
                let val_str = ser.categories.get(ci).cloned().unwrap_or_else(|| format!("項目 {}", ci + 1));
                out.push_str(&format!("<c:pt idx=\"{ci}\"><c:v>{}</c:v></c:pt>", xml_escape(&val_str)));
            }
            out.push_str("</c:strLit></c:cat>");

            out.push_str(&format!("<c:val><c:numLit><c:formatCode>General</c:formatCode><c:ptCount val=\"{val_count}\"/>"));
            for vi in 0..val_count {
                let num_val = ser.values.get(vi).copied().flatten().map(|Number(n)| n).unwrap_or((vi as f64 + 1.0) * 10.0);
                out.push_str(&format!("<c:pt idx=\"{vi}\"><c:v>{num_val}</c:v></c:pt>"));
            }
            out.push_str("</c:numLit></c:val>");

            out.push_str("</c:ser>");
        }

        for ax_id in &group.axes {
            out.push_str(&format!("<c:axId val=\"{ax_id}\"/>"));
        }

        out.push_str(&format!("</c:{tag}>\r\n"));
    }

    // Axes
    for axis in &chart.axes {
        let tag = match axis.kind {
            AxisKind::Category => if axis.date { "dateAx" } else { "catAx" },
            AxisKind::Value => "valAx",
            AxisKind::Series => "serAx",
        };
        let orient = if axis.reversed { "maxMin" } else { "minMax" };
        let del = if axis.deleted { 1 } else { 0 };
        let pos = axis_side_str(axis.side);

        out.push_str(&format!("<c:{tag}><c:axId val=\"{}\"/><c:scaling><c:orientation val=\"{orient}\"/>", axis.id));
        if let Some(Number(min_val)) = axis.min {
            out.push_str(&format!("<c:min val=\"{min_val}\"/>"));
        }
        if let Some(Number(max_val)) = axis.max {
            out.push_str(&format!("<c:max val=\"{max_val}\"/>"));
        }
        out.push_str(&format!("</c:scaling><c:delete val=\"{del}\"/><c:axPos val=\"{pos}\"/>"));

        if let Some(grid) = &axis.gridlines {
            out.push_str(&format!("<c:majorGridlines>{}</c:majorGridlines>", write_stroke_xml(Some(grid))));
        }
        if let Some(m_grid) = &axis.minor_gridlines {
            out.push_str(&format!("<c:minorGridlines>{}</c:minorGridlines>", write_stroke_xml(Some(m_grid))));
        }

        if let Some(t) = &axis.title {
            let esc = xml_escape(&t.text);
            out.push_str(&format!(
                "<c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{esc}</a:t></a:r></a:p></c:rich></c:tx></c:title>"
            ));
        }

        out.push_str(&format!("<c:numFmt formatCode=\"{}\" sourceLinked=\"1\"/>", xml_escape(&axis.format)));
        if let Some(Number(unit)) = axis.major_unit {
            out.push_str(&format!("<c:majorUnit val=\"{unit}\"/>"));
        }
        if let Some(Number(m_unit)) = axis.minor_unit {
            out.push_str(&format!("<c:minorUnit val=\"{m_unit}\"/>"));
        }
        if let Some(Number(log)) = axis.log_base {
            out.push_str(&format!("<c:logBase val=\"{log}\"/>"));
        }
        if let Some(Number(disp)) = axis.display_unit {
            out.push_str(&format!("<c:dispUnits><c:custUnit val=\"{disp}\"/></c:dispUnits>"));
        }

        out.push_str(&format!("</c:{tag}>\r\n"));
    }

    // Data Table
    if let Some(dt) = &chart.data_table {
        out.push_str(&format!(
            "<c:dTable><c:showHorzBorder val=\"{}\"/><c:showVertBorder val=\"{}\"/><c:showOutline val=\"{}\"/><c:showKeys val=\"{}\"/></c:dTable>\r\n",
            if dt.show_horz_border { 1 } else { 0 },
            if dt.show_vert_border { 1 } else { 0 },
            if dt.show_outline { 1 } else { 0 },
            if dt.show_keys { 1 } else { 0 }
        ));
    }

    // Plot Background
    let plot_bg_xml = write_fill_xml(&chart.plot_background);
    if !plot_bg_xml.is_empty() {
        out.push_str(&format!("<c:spPr>{plot_bg_xml}</c:spPr>\r\n"));
    }

    out.push_str("</c:plotArea>\r\n");

    // Legend
    if let Some(leg) = &chart.legend {
        let lpos = legend_pos_str(leg.source_position, leg.side);
        let overlay = if leg.overlay == Some(true) { 1 } else { 0 };
        out.push_str(&format!("<c:legend><c:legendPos val=\"{lpos}\"/><c:overlay val=\"{overlay}\"/></c:legend>\r\n"));
    }

    out.push_str(&format!("<c:plotVisOnly val=\"{}\"/>\r\n", if chart.plot_vis_only { 1 } else { 0 }));
    let disp_b = match chart.disp_blanks_as.unwrap_or(DispBlanksAs::Gap) {
        DispBlanksAs::Gap => "gap",
        DispBlanksAs::Zero => "zero",
        DispBlanksAs::Span => "span",
    };
    out.push_str(&format!("<c:dispBlanksAs val=\"{disp_b}\"/>\r\n"));

    out.push_str("</c:chart>\r\n");

    // Frame background and border
    let bg_xml = write_fill_xml(&chart.background);
    let border_xml = write_stroke_xml(chart.border.as_ref());
    if !bg_xml.is_empty() || !border_xml.is_empty() {
        out.push_str(&format!("<c:spPr>{bg_xml}{border_xml}</c:spPr>\r\n"));
    }

    out.push_str("<c:txPr><a:bodyPr/><a:lstStyle/><a:p><a:pPr><a:defRPr sz=\"1197\"><a:solidFill><a:schemeClr val=\"tx1\"><a:lumMod val=\"65000\"/><a:lumOff val=\"35000\"/></a:schemeClr></a:solidFill></a:defRPr></a:pPr><a:endParaRPr lang=\"ja-JP\"/></a:p></c:txPr>\r\n");
    out.push_str("</c:chartSpace>\r\n");

    Ok(out)
}

/// Packages a `Chart` into an Office Chart Template (`.crtx`) OPC package byte buffer.
pub fn chart_to_crtx(chart: &Chart) -> Result<Vec<u8>, Error> {
    let xml = chart_to_xml(chart)?;
    let mut writer = Writer::new();
    writer.add("[Content_Types].xml", EntryMeta::new(), CONTENT_TYPES_XML.as_bytes())?;
    writer.add("_rels/.rels", EntryMeta::new(), RELS_XML.as_bytes())?;
    writer.add("chart/chart.xml", EntryMeta::new(), xml.as_bytes())?;
    writer.finish().map_err(|e| Error::new(e.to_string()))
}

/// Reads an Office Chart Template (`.crtx`) package and parses its `chart/chart.xml` into a `Chart` model.
pub fn read_crtx(data: &[u8]) -> Result<Chart, Error> {
    let archive = Archive::new(Arc::from(data))?;
    let chart_entry = archive
        .entries()
        .iter()
        .find(|e| {
            e.name == "chart/chart.xml"
                || e.name == "charts/chart.xml"
                || e.name.ends_with("/chart.xml")
                || e.name.ends_with("chart1.xml")
        })
        .ok_or_else(|| Error::new("チャート テンプレート内に chart.xml が見つかりません"))?;

    let xml_bytes = archive.read(chart_entry)?;
    let xml_str = std::str::from_utf8(&xml_bytes)
        .map_err(|e| Error::new(format!("chart.xml は有効な UTF-8 ではありません: {e}")))?;
    let doc = roxmltree::Document::parse(xml_str)
        .map_err(|e| Error::new(format!("chart.xml の解析に失敗しました: {e}")))?;
    let theme = crate::theme::Theme::default();
    let colors = crate::theme::Colors::new(&theme, None, &[]);
    crate::chart::read(doc.root_element(), &colors)
        .ok_or_else(|| Error::new("チャート テンプレートをパースできませんでした"))
}

/// Applies a template's formatting, chart kind, styling, axes, and legend onto `target`,
/// preserving `target`'s existing data series names, categories, and numerical values.
pub fn apply_chart_template(target: &mut Chart, template: &Chart) {
    if template.groups.is_empty() || target.groups.is_empty() {
        return;
    }

    // 1. Chart type, grouping, and specific type settings
    let tmpl_grp = &template.groups[0];
    let target_grp = &mut target.groups[0];
    target_grp.kind = tmpl_grp.kind;
    target_grp.grouping = tmpl_grp.grouping;
    target_grp.of_pie = tmpl_grp.of_pie.clone();

    // 2. Series formatting (fills, strokes, markers, labels, error bars, trendlines)
    let tmpl_series = &tmpl_grp.series;
    for (i, target_ser) in target_grp.series.iter_mut().enumerate() {
        if !tmpl_series.is_empty() {
            let tmpl_ser = &tmpl_series[i % tmpl_series.len()];
            target_ser.fill = tmpl_ser.fill.clone();
            target_ser.stroke = tmpl_ser.stroke.clone();
            target_ser.marker = tmpl_ser.marker.clone();
            target_ser.labels = tmpl_ser.labels.clone();
            target_ser.trendline = tmpl_ser.trendline.clone();
            target_ser.error_bars = tmpl_ser.error_bars.clone();
        }
    }

    // 3. Axes settings (matching by axis kind)
    for tmpl_ax in &template.axes {
        if let Some(target_ax) = target.axes.iter_mut().find(|a| a.kind == tmpl_ax.kind) {
            target_ax.gridlines = tmpl_ax.gridlines.clone();
            target_ax.minor_gridlines = tmpl_ax.minor_gridlines.clone();
            target_ax.reversed = tmpl_ax.reversed;
            target_ax.log_base = tmpl_ax.log_base;
            target_ax.display_unit = tmpl_ax.display_unit;
            target_ax.format = tmpl_ax.format.clone();
            target_ax.show_labels = tmpl_ax.show_labels;
            target_ax.min = tmpl_ax.min;
            target_ax.max = tmpl_ax.max;
            target_ax.major_unit = tmpl_ax.major_unit;
            target_ax.minor_unit = tmpl_ax.minor_unit;
            if tmpl_ax.title.is_some() {
                target_ax.title = tmpl_ax.title.clone();
            }
        }
    }

    // 4. Legend
    target.legend = template.legend.clone();

    // 5. Title styling (retaining target's title text if present)
    if let Some(t_tmpl) = &template.title {
        if let Some(t_target) = &mut target.title {
            t_target.style = t_tmpl.style.clone();
            t_target.at = t_tmpl.at;
        } else {
            target.title = Some(t_tmpl.clone());
        }
    }

    // 6. Style, backgrounds, data table, blanks
    target.style = template.style;
    target.background = template.background.clone();
    target.border = template.border.clone();
    target.plot_background = template.plot_background.clone();
    target.disp_blanks_as = template.disp_blanks_as;
    target.plot_vis_only = template.plot_vis_only;
    target.data_table = template.data_table;
}
