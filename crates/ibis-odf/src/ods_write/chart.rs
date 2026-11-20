// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A chart of a sheet as the embedded object of an `.ods` (ODF 1.3 chapter 11): `office:chart` with the chart's
//! own data table (`local-table`), which the series read. Bars and columns (clustered, stacked), lines, areas, pies,
//! rings, scatters and radars; the series' colours, titles, legend and the value axis' scale.

use ibis_doc_model::{AxisKind, Chart, ChartType, Fill, Grouping, Series, Side};

use super::esc;

fn hex(rgb: ibis_doc_model::Rgb) -> String {
    format!("#{:06x}", rgb.0)
}

fn fill_color(f: &Fill) -> Option<String> {
    match f {
        Fill::Solid(c) => Some(hex(c.rgb)),
        _ => None,
    }
}

fn class_of(kind: &ChartType) -> Option<&'static str> {
    Some(match kind {
        ChartType::Bar { .. } => "chart:bar",
        ChartType::Line => "chart:line",
        ChartType::Area => "chart:area",
        ChartType::Pie { .. } => "chart:circle",
        ChartType::Doughnut { .. } => "chart:ring",
        ChartType::Scatter { .. } => "chart:scatter",
        ChartType::Radar { .. } => "chart:radar",
        _ => return None,
    })
}

/// A reference of a workbook (`Sheet1!$B$2:$B$5`, `'月次 売上'!$A$2`) as an ODF cell range address (`Sheet1.B2:.B5`, `'月次 売上'.A2`).
fn ods_range(reference: &str) -> Option<String> {
    let (sheet, range) = ibis_formats::parse_ref(reference)?;
    let name = sheet.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')).map_or_else(|| sheet.clone(), |s| s.replace("''", "'"));
    let plain = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
    let sheet = if plain { name } else { format!("'{}'", name.replace('\'', "''")) };
    let cell = |c: u32, r: u32| format!("{}{}", super::column_letters(c), r + 1);
    let first = cell(range.first_column, range.first_row);
    Some(if range.first_row == range.last_row && range.first_column == range.last_column { format!("{sheet}.{first}") } else { format!("{sheet}.{first}:{sheet}.{}", cell(range.last_column, range.last_row)) })
}

fn column(n: usize) -> String {
    super::column_letters(n as u32)
}

/// The `content.xml` of the chart object, `width` and `height` in EMU; `None` for a kind of chart not written.
/// The chart object's XML and the cell ranges (ODF addresses) it reads, for the `draw:object` that holds it.
pub(super) fn content(chart: &Chart, width: i64, height: i64, refs: Option<&[ibis_formats::SeriesRefs]>) -> Option<(String, Vec<String>)> {
    let first = chart.groups.first()?;
    let chart_class = class_of(&first.kind)?;
    let scatter = matches!(first.kind, ChartType::Scatter { .. });
    // The series of the groups that are written, with the group they are in.
    let mut series: Vec<(&Series, &'static str, &ibis_doc_model::ChartGroup)> = Vec::new();
    // Each written series' references (the file's order of series, the groups' that are not written counted).
    let mut series_refs: Vec<Option<&ibis_formats::SeriesRefs>> = Vec::new();
    let mut linked: Vec<String> = Vec::new();
    let mut counted = 0;
    for g in &chart.groups {
        if let Some(class) = class_of(&g.kind) {
            series.extend(g.series.iter().map(|s| (s, class, g)));
            series_refs.extend((0..g.series.len()).map(|i| refs.and_then(|r| r.get(counted + i))));
        }
        counted += g.series.len();
    }
    if series.is_empty() {
        return None;
    }
    let rows = series.iter().map(|(s, ..)| s.values.len()).max().unwrap_or(0);
    let mut styles = String::new();
    let mut n = 0;
    let mut style = |xml: String, styles: &mut String| -> String {
        n += 1;
        let name = format!("ch{n}");
        styles.push_str(&format!("<style:style style:name=\"{name}\" style:family=\"chart\">{xml}</style:style>"));
        name
    };
    // The plot area: bars standing or lying, stacking.
    let mut plot_props = String::new();
    if let ChartType::Bar { horizontal: true, .. } = first.kind {
        plot_props.push_str(" chart:vertical=\"true\"");
    }
    if first.grouping != Grouping::Standard {
        plot_props.push_str(" chart:stacked=\"true\"");
    }
    if first.grouping == Grouping::PercentStacked {
        plot_props.push_str(" chart:percentage=\"true\"");
    }
    let plot_style = style(format!("<style:chart-properties{plot_props}/>"), &mut styles);
    // The axes.
    // A scatter's two axes are both value axes: the first one runs along the bottom.
    let value_axes: Vec<&ibis_doc_model::Axis> = chart.axes.iter().filter(|a| a.kind == AxisKind::Value && !a.deleted).collect();
    let x_axis = if scatter { value_axes.first().copied() } else { chart.axes.iter().find(|a| a.kind == AxisKind::Category && !a.deleted) };
    let y_axis = if scatter { value_axes.get(1).copied() } else { value_axes.first().copied() };
    let title_xml = |t: &ibis_doc_model::ChartTitle| format!("<chart:title><text:p>{}</text:p></chart:title>", esc(&t.text));
    let mut axes = String::new();
    let mut x_props = String::new();
    let mut y_props = String::new();
    if let ChartType::Bar { gap_width, overlap, .. } = first.kind {
        y_props.push_str(&format!(" chart:gap-width=\"{gap_width}\" chart:overlap=\"{overlap}\""));
    }
    if let Some(a) = y_axis {
        if let Some(m) = a.min {
            y_props.push_str(&format!(" chart:minimum=\"{}\"", ibis_calc::number_to_xml(m.0)));
        }
        if let Some(m) = a.max {
            y_props.push_str(&format!(" chart:maximum=\"{}\"", ibis_calc::number_to_xml(m.0)));
        }
        if let Some(m) = a.major_unit {
            y_props.push_str(&format!(" chart:interval-major=\"{}\"", ibis_calc::number_to_xml(m.0)));
        }
        if a.log_base.is_some() {
            y_props.push_str(" chart:logarithmic=\"true\"");
        }
        if a.reversed {
            y_props.push_str(" chart:reverse-direction=\"true\"");
        }
    }
    if let Some(a) = x_axis {
        if a.reversed {
            x_props.push_str(" chart:reverse-direction=\"true\"");
        }
    }
    let pie = matches!(first.kind, ChartType::Pie { .. } | ChartType::Doughnut { .. });
    if !pie {
        let x_style = style(format!("<style:chart-properties{x_props}/>"), &mut styles);
        let y_style = style(format!("<style:chart-properties{y_props}/>"), &mut styles);
        let x_title = x_axis.and_then(|a| a.title.as_ref()).map(title_xml).unwrap_or_default();
        let y_title = y_axis.and_then(|a| a.title.as_ref()).map(title_xml).unwrap_or_default();
        let grid = y_axis.is_some_and(|a| a.gridlines.is_some());
        let categories = if scatter {
            String::new()
        } else {
            let address = match series_refs.first().copied().flatten().and_then(|r| r.categories.as_deref()).and_then(ods_range) {
                Some(a) => {
                    linked.push(a.clone());
                    a
                }
                None => format!("local-table.A2:.A{}", rows + 1),
            };
            format!("<chart:categories table:cell-range-address=\"{address}\"/>")
        };
        axes.push_str(&format!("<chart:axis chart:dimension=\"x\" chart:name=\"primary-x\" chart:style-name=\"{x_style}\">{x_title}{categories}</chart:axis>"));
        axes.push_str(&format!("<chart:axis chart:dimension=\"y\" chart:name=\"primary-y\" chart:style-name=\"{y_style}\">{y_title}{}</chart:axis>", if grid { "<chart:grid chart:class=\"major\"/>" } else { "" }));
    }
    // The series.
    let mut series_xml = String::new();
    for (k, (s, class, g)) in series.iter().enumerate() {
        let col = column(k + 1);
        let mut props = String::new();
        let line_like = matches!(g.kind, ChartType::Line | ChartType::Scatter { .. } | ChartType::Radar { .. });
        match (line_like, fill_color(&s.fill), s.stroke.as_ref()) {
            (true, _, Some(st)) => props.push_str(&format!(" draw:stroke=\"solid\" svg:stroke-color=\"{}\"", hex(st.color.rgb))),
            (true, Some(c), None) => props.push_str(&format!(" draw:stroke=\"solid\" svg:stroke-color=\"{c}\"")),
            (_, Some(c), _) => props.push_str(&format!(" draw:fill=\"solid\" draw:fill-color=\"{c}\"")),
            _ => {}
        }
        let sstyle = style(format!("<style:graphic-properties{props}/>"), &mut styles);
        let mut points = String::new();
        if pie {
            for i in 0..rows {
                match s.point_fills.iter().find(|(p, _)| *p == i).and_then(|(_, f)| fill_color(f)) {
                    Some(c) => {
                        let pstyle = style(format!("<style:graphic-properties draw:fill=\"solid\" draw:fill-color=\"{c}\"/>"), &mut styles);
                        points.push_str(&format!("<chart:data-point chart:style-name=\"{pstyle}\"/>"));
                    }
                    None => points.push_str("<chart:data-point/>"),
                }
            }
        } else {
            points.push_str(&format!("<chart:data-point chart:repeated=\"{}\"/>", rows.max(1)));
        }
        let own = series_refs.get(k).copied().flatten();
        let domain_address = own.and_then(|r| r.categories.as_deref()).and_then(ods_range).unwrap_or_else(|| format!("local-table.A2:.A{}", rows + 1));
        let domain = if scatter { format!("<chart:domain table:cell-range-address=\"{domain_address}\"/>") } else { String::new() };
        let mut link = |address: Option<String>, own_address: String| match address {
            Some(a) => {
                linked.push(a.clone());
                a
            }
            None => own_address,
        };
        let values = link(own.and_then(|r| r.values.as_deref()).and_then(ods_range), format!("local-table.{col}2:.{col}{}", rows + 1));
        let label = link(own.and_then(|r| r.name.as_deref()).and_then(ods_range), format!("local-table.{col}1"));
        series_xml.push_str(&format!(
            "<chart:series chart:style-name=\"{sstyle}\" chart:values-cell-range-address=\"{values}\" chart:label-cell-address=\"{label}\" chart:class=\"{class}\">{domain}{points}</chart:series>"
        ));
    }
    // The chart's own data table: names across, categories (or x values) down.
    let cell_text = |t: &str| format!("<table:table-cell office:value-type=\"string\"><text:p>{}</text:p></table:table-cell>", esc(t));
    let cell_number = |v: Option<f64>| match v {
        Some(n) => format!("<table:table-cell office:value-type=\"float\" office:value=\"{0}\"><text:p>{0}</text:p></table:table-cell>", ibis_calc::number_to_xml(n)),
        None => "<table:table-cell/>".to_string(),
    };
    let mut table = format!("<table:table table:name=\"local-table\"><table:table-header-columns><table:table-column/></table:table-header-columns><table:table-columns><table:table-column table:number-columns-repeated=\"{}\"/></table:table-columns>", series.len());
    table.push_str("<table:table-header-rows><table:table-row><table:table-cell><text:p/></table:table-cell>");
    for (s, ..) in &series {
        table.push_str(&cell_text(&s.name));
    }
    table.push_str("</table:table-row></table:table-header-rows><table:table-rows>");
    let lead = &series[0].0;
    for i in 0..rows {
        table.push_str("<table:table-row>");
        if scatter {
            table.push_str(&cell_number(lead.x_values.get(i).copied().flatten().map(|n| n.0)));
        } else {
            table.push_str(&cell_text(lead.categories.get(i).map(String::as_str).unwrap_or("")));
        }
        for (s, ..) in &series {
            table.push_str(&cell_number(s.values.get(i).copied().flatten().map(|n| n.0)));
        }
        table.push_str("</table:table-row>");
    }
    table.push_str("</table:table-rows></table:table>");
    let title = chart.title.as_ref().map(title_xml).unwrap_or_default();
    let legend = chart.legend.as_ref().map(|l| {
        let pos = match l.side {
            Side::Left => "start",
            Side::Top => "top",
            Side::Bottom => "bottom",
            Side::Right => "end",
        };
        format!("<chart:legend chart:legend-position=\"{pos}\"/>")
    });
    let ns = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:fo=\"urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:chart=\"urn:oasis:names:tc:opendocument:xmlns:chart:1.0\" xmlns:number=\"urn:oasis:names:tc:opendocument:xmlns:datastyle:1.0\"";
    // Tied to the workbook's cells (as LibreOffice writes a chart it reads from them).
    let tie = if linked.is_empty() { "" } else { " xlink:href=\"..\" xlink:type=\"simple\"" };
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {ns} office:version=\"1.2\"><office:automatic-styles>{styles}</office:automatic-styles><office:body><office:chart><chart:chart svg:width=\"{}\" svg:height=\"{}\"{} chart:class=\"{chart_class}\">{title}{}<chart:plot-area chart:style-name=\"{plot_style}\" chart:data-source-has-labels=\"both\">{axes}{series_xml}</chart:plot-area>{table}</chart:chart></office:chart></office:body></office:document-content>",
        super::cm(width),
        super::cm(height),
        tie,
        legend.unwrap_or_default()
    );
    Some((xml, linked))
}
