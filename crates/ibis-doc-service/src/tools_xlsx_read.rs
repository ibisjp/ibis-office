// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The `xlsx_*` reads of stage B (design `2026-11-24-159-mcp-stage-b.md` 5 and 8, step 1): what the edit tools of stages 2 to 4 change can be read
//! first. The shapes are the ones the edit tools take (a colour is `"#RRGGBB"` or a theme reference, sizes are points, ranges are A1 text).

use ibis_doc_model::sheet::{Border, BorderLine, CellAlign, CellRange, CellStyle, CellUnderline, CellVerticalAlign, CfKind, CfStyle, CfValue, Sheet, ThemeColor};
use ibis_doc_model::{EMU_PER_POINT, Rgb, ShapeKind};
use serde_json::{Value, json};

use crate::a1::{column_name, range_text};
use crate::catalog::{Access, Tool, document_id, object, string};
use crate::error::ToolError;
use crate::service::{Args, Open, Service};
use crate::tools_xlsx::sheets_of;

pub(crate) fn tools() -> Vec<Tool> {
    vec![
        Tool { name: "xlsx_get_sheet_view", description: "How a sheet is shown: gridlines, headings, zoom (percent), tab colour, show_formulas, show_zeros, and the freeze_panes cell (the first cell that scrolls, or null).", access: Access::Read, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("The sheet's name."))], &["document_id", "sheet"]), run: get_sheet_view },
        Tool { name: "xlsx_list_names", description: "The defined names of the workbook: name, refers_to (a formula with =) and scope (the sheet's name, or workbook). Print area and print titles are names too (_xlnm.Print_Area).", access: Access::Read, returns_content: false, schema: &|| object(&[document_id()], &["document_id"]), run: list_names },
        Tool { name: "xlsx_list_cell_styles", description: "The names of the cell styles that xlsx_set_format can apply (style).", access: Access::Read, returns_content: false, schema: &|| object(&[document_id()], &["document_id"]), run: list_cell_styles },
        Tool { name: "xlsx_list_validations", description: "The data validation rules of a sheet: index, ranges, kind (list, whole, decimal, date, time, text_length, custom, any), operator, formula1, formula2, ignore_blank, dropdown, message and error.", access: Access::Read, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("The sheet's name."))], &["document_id", "sheet"]), run: list_validations },
        Tool { name: "xlsx_list_conditional_formats", description: "The conditional formats of a sheet: index, ranges and rules (type, priority, stop, the rule's own settings, and the format it gives).", access: Access::Read, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("The sheet's name."))], &["document_id", "sheet"]), run: list_conditional_formats },
        Tool { name: "xlsx_list_tables", description: "The tables of a sheet (name, range, header and totals rows, columns, style) and the sheet's auto filter (range and the values each filtered column keeps). Without sheet: every sheet.", access: Access::Read, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("Only this sheet."))], &["document_id"]), run: list_tables },
        Tool { name: "xlsx_list_comments", description: "The notes and the threaded comments of a sheet (cell, author, text). Without sheet: every sheet.", access: Access::Read, returns_content: true, schema: &|| object(&[document_id(), ("sheet", string("Only this sheet."))], &["document_id"]), run: list_comments },
        Tool { name: "xlsx_list_hyperlinks", description: "The hyperlinks of a sheet (range, url or location, tooltip). Without sheet: every sheet.", access: Access::Read, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("Only this sheet."))], &["document_id"]), run: list_hyperlinks },
        Tool { name: "xlsx_get_page_setup", description: "How a sheet is printed: paper (points), orientation, margins (points), scale or fit_to, print_area, print_titles, centering, grid lines, headings and the header and footer texts.", access: Access::Read, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("The sheet's name."))], &["document_id", "sheet"]), run: get_page_setup },
        Tool { name: "xlsx_list_charts", description: "The charts of a sheet: shape id, kind, frame (points), title, legend and the cells of each series. Without sheet: every sheet.", access: Access::Read, returns_content: false, schema: &|| object(&[document_id(), ("sheet", string("Only this sheet."))], &["document_id"]), run: list_charts },
    ]
}

/// The theme names of 6.6, by SpreadsheetML's order of theme colours.
pub(crate) fn theme_name(index: u8) -> &'static str {
    ["background1", "text1", "background2", "text2", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hyperlink", "followed_hyperlink"].get(index as usize).copied().unwrap_or("text1")
}

pub(crate) fn rgb_text(rgb: Rgb) -> String {
    format!("#{:06X}", rgb.0 & 0xFF_FFFF)
}

/// A colour as 6.6 has it: `"#RRGGBB"`, or the theme colour as a reference with its resolved colour.
pub(crate) fn color_json(rgb: Rgb, theme: Option<ThemeColor>) -> Value {
    match theme {
        Some(t) => json!({ "theme": theme_name(t.index), "brightness": (f64::from(t.tint) / 1_000.0).round() / 1_000.0, "resolved": rgb_text(rgb) }),
        None => Value::String(rgb_text(rgb)),
    }
}

/// The name of a border line, as the edit tool takes it.
pub(crate) fn border_name(line: BorderLine) -> &'static str {
    match line {
        BorderLine::Thin => "thin",
        BorderLine::Hair => "hair",
        BorderLine::Medium => "medium",
        BorderLine::Thick => "thick",
        BorderLine::Double => "double",
        BorderLine::Dotted => "dotted",
        BorderLine::Dashed => "dashed",
        BorderLine::DashDot => "dash_dot",
        BorderLine::DashDotDot => "dash_dot_dot",
        BorderLine::MediumDashed => "medium_dashed",
        BorderLine::MediumDashDot => "medium_dash_dot",
        BorderLine::MediumDashDotDot => "medium_dash_dot_dot",
        BorderLine::SlantDashDot => "slant_dash_dot",
    }
}

pub(crate) fn border_from_name(name: &str) -> Option<Option<BorderLine>> {
    Some(match name {
        "none" => None,
        _ => Some(BorderLine::ALL.into_iter().find(|l| border_name(*l) == name)?),
    })
}

fn border_json(border: &Option<Border>) -> Value {
    match border {
        Some(b) => json!({ "style": border_name(b.line), "color": rgb_text(b.color) }),
        None => Value::Null,
    }
}

fn align_name(a: CellAlign) -> &'static str {
    match a {
        CellAlign::General => "general",
        CellAlign::Left => "left",
        CellAlign::Center => "center",
        CellAlign::Right => "right",
        CellAlign::Fill => "fill",
        CellAlign::Justify => "justify",
        CellAlign::CenterAcross => "center_across",
        CellAlign::Distributed => "distributed",
    }
}

fn vertical_name(a: CellVerticalAlign) -> &'static str {
    match a {
        CellVerticalAlign::Top => "top",
        CellVerticalAlign::Center => "center",
        CellVerticalAlign::Bottom => "bottom",
        CellVerticalAlign::Justify => "justify",
        CellVerticalAlign::Distributed => "distributed",
    }
}

/// The format of a cell (`xlsx_get_cells`, kind `format`).
pub(crate) fn format_json(style: &CellStyle) -> Value {
    json!({
        "bold": style.bold,
        "italic": style.italic,
        "underline": style.underline != CellUnderline::None,
        "strike": style.strike,
        "size": f64::from(style.font_size) / 100.0,
        "font": style.font,
        "color": color_json(style.color, style.color_theme),
        "fill": style.fill.map(|f| color_json(f, style.fill_theme)),
        "number_format": style.number_format,
        "horizontal_alignment": align_name(style.align),
        "vertical_alignment": vertical_name(style.vertical),
        "wrap_text": style.wrap,
        "indent": style.indent,
        "text_rotation": match style.rotation { 255 => json!("vertical"), n if n <= 90 => json!(n), n => json!(90 - i32::from(n)) },
        "shrink_to_fit": style.shrink,
        "locked": !style.unlocked,
        "hidden": style.formula_hidden,
        "borders": {
            "top": border_json(&style.top),
            "bottom": border_json(&style.bottom),
            "left": border_json(&style.left),
            "right": border_json(&style.right),
            "diagonal_up": border_json(&style.diagonal_up),
            "diagonal_down": border_json(&style.diagonal_down),
        },
    })
}

/// The sheet named `name` of the open workbook (visible sheets; the name as Excel matches it).
pub(crate) fn sheet_named<'a>(open: &'a Open, name: &str) -> Result<(String, &'a Sheet), ToolError> {
    let sheets = sheets_of(open)?;
    let at = sheets.iter().position(|(_, n, _)| n == name).or_else(|| sheets.iter().position(|(_, n, _)| n.eq_ignore_ascii_case(name))).ok_or_else(|| ToolError::new("no_such_sheet", format!("no sheet named {name}")))?;
    let (_, found, sheet) = sheets.into_iter().nth(at).expect("position is in range");
    Ok((found, sheet))
}

/// The sheets a tool with an optional `sheet` argument is about: `(name, sheet)`.
fn chosen<'a>(open: &'a Open, args: &Value) -> Result<Vec<(String, &'a Sheet)>, ToolError> {
    match args.opt_str("sheet")? {
        Some(name) => Ok(vec![sheet_named(open, name)?]),
        None => Ok(sheets_of(open)?.into_iter().map(|(_, n, s)| (n, s)).collect()),
    }
}

fn ranges_json(ranges: &[CellRange]) -> Vec<String> {
    ranges.iter().map(range_text).collect()
}

fn get_sheet_view(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let (name, sheet) = sheet_named(open, args.str_arg("sheet")?)?;
    let cell = |(columns, rows): (u32, u32)| format!("{}{}", column_name(columns), rows + 1);
    Ok(json!({
        "sheet": name,
        "gridlines": sheet.grid_lines,
        "headings": sheet.headings,
        "zoom": sheet.zoom.unwrap_or(100),
        "tab_color": sheet.tab_color.map(rgb_text),
        "show_formulas": sheet.show_formulas,
        "show_zeros": sheet.show_zeros,
        "freeze_panes": sheet.frozen.map(cell),
        "split": sheet.split.map(cell),
    }))
}

fn list_names(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    sheets_of(open)?;
    let workbook = open.document.presentation().workbook.as_ref();
    let names: Vec<Value> = workbook
        .map(|w| {
            w.names
                .iter()
                .map(|n| json!({ "name": n.name, "refers_to": format!("={}", n.formula), "scope": n.sheet.and_then(|i| w.sheets.get(i)).map_or("workbook", |s| s.name.as_str()), "hidden": n.hidden }))
                .collect()
        })
        .unwrap_or_default();
    Ok(json!({ "names": names }))
}

fn list_cell_styles(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    sheets_of(open)?;
    let styles: Vec<&str> = open.document.presentation().workbook.as_ref().map(|w| w.custom_cell_styles.iter().map(|s| s.name.as_str()).collect()).unwrap_or_default();
    Ok(json!({ "styles": styles }))
}

pub(crate) fn validation_kind(kind: &str) -> &str {
    if kind == "textLength" { "text_length" } else if kind.is_empty() || kind == "none" { "any" } else { kind }
}

pub(crate) fn operator_name(op: &str) -> String {
    let mut out = String::new();
    for c in op.chars() {
        if c.is_ascii_uppercase() {
            out.push('_');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn list_validations(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let (name, sheet) = sheet_named(open, args.str_arg("sheet")?)?;
    let rules: Vec<Value> = sheet
        .validations
        .iter()
        .enumerate()
        .map(|(i, v)| {
            json!({
                "index": i,
                "ranges": ranges_json(&v.ranges),
                "kind": validation_kind(&v.kind),
                "operator": (!v.operator.is_empty()).then(|| operator_name(&v.operator)),
                "formula1": v.formula1,
                "formula2": v.formula2,
                "ignore_blank": v.allow_blank,
                "dropdown": v.dropdown,
                "message": v.prompt.as_ref().map(|(t, m)| json!({ "title": t, "text": m })),
                "error": v.error.as_ref().map(|(s, t, m)| json!({ "style": s, "title": t, "text": m })),
            })
        })
        .collect();
    Ok(json!({ "sheet": name, "validations": rules }))
}

fn cf_value_json(v: &CfValue) -> Value {
    json!({ "type": v.kind, "value": v.value })
}

fn cf_style_json(style: &CfStyle) -> Value {
    let borders: Vec<Value> = style.borders.iter().map(border_json).collect();
    json!({
        "color": style.color.map(rgb_text),
        "bold": style.bold,
        "italic": style.italic,
        "underline": style.underline.map(|u| u != CellUnderline::None),
        "strike": style.strike,
        "fill": style.fill.map(rgb_text),
        "number_format": style.number_format,
        "borders": { "left": borders[0], "right": borders[1], "top": borders[2], "bottom": borders[3] },
    })
}

pub(crate) fn cf_rule_json(rule: &ibis_doc_model::sheet::CfRule) -> Value {
    let mut out = match &rule.kind {
        CfKind::CellIs { operator, formulas } => json!({ "type": "cell_value", "operator": operator_name(operator), "formulas": formulas }),
        CfKind::Expression(f) => json!({ "type": "expression", "formula": f }),
        CfKind::TimePeriod { period, formula } => json!({ "type": "time_period", "period": period, "formula": formula }),
        CfKind::Top10 { rank, percent, bottom } => json!({ "type": "top_bottom", "rank": rank, "percent": percent, "bottom": bottom }),
        CfKind::AboveAverage { above, equal, std_dev } => json!({ "type": "above_average", "above": above, "equal": equal, "std_dev": std_dev }),
        CfKind::Unique => json!({ "type": "unique" }),
        CfKind::Duplicate => json!({ "type": "duplicate" }),
        CfKind::ColorScale(points) => json!({ "type": "color_scale", "points": points.iter().map(|(v, c)| { let mut p = cf_value_json(v); p["color"] = json!(rgb_text(*c)); p }).collect::<Vec<_>>() }),
        CfKind::DataBar { min, max, color, gradient, negative_color, .. } => json!({ "type": "data_bar", "min": cf_value_json(min), "max": cf_value_json(max), "color": rgb_text(*color), "gradient": gradient, "negative_color": negative_color.map(rgb_text) }),
        CfKind::IconSet { set, values, reverse, show_value, .. } => json!({ "type": "icon_set", "icon_set": set, "thresholds": values.iter().map(cf_value_json).collect::<Vec<_>>(), "reverse": reverse, "show_value": show_value }),
        CfKind::Other => json!({ "type": "other" }),
    };
    out["priority"] = json!(rule.priority);
    out["stop"] = json!(rule.stop);
    out["format"] = rule.style.as_ref().map_or(Value::Null, cf_style_json);
    out
}

fn list_conditional_formats(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let (name, sheet) = sheet_named(open, args.str_arg("sheet")?)?;
    let formats: Vec<Value> = sheet.conditional.iter().enumerate().map(|(i, f)| json!({ "index": i, "ranges": ranges_json(&f.ranges), "rules": f.rules.iter().map(cf_rule_json).collect::<Vec<_>>() })).collect();
    Ok(json!({ "sheet": name, "conditional_formats": formats }))
}

fn filter_json(filter: &ibis_doc_model::sheet::AutoFilter) -> Value {
    use ibis_doc_model::sheet::ColumnFilter as F;
    let columns: Vec<Value> = filter
        .columns
        .iter()
        .map(|(column, f)| {
            let mut j = match f {
                F::Values(values) => json!({ "type": "values", "values": values }),
                F::Dates { values, .. } => json!({ "type": "dates", "values": values }),
                F::AboveAverage(above) => json!({ "type": "above_average", "above": above }),
                F::Custom { and, conditions } => json!({ "type": "custom", "and": and, "conditions": conditions.iter().map(|(o, v)| json!({ "operator": operator_name(o), "value": v })).collect::<Vec<_>>() }),
                F::Top { top, percent, value } => json!({ "type": "top", "top": top, "percent": percent, "count": value }),
                F::Period(p) => json!({ "type": "period", "period": p }),
                F::Color { cell, color } => json!({ "type": "color", "cell": cell, "color": color.map(rgb_text) }),
                F::Icon { icon_set, icon_id } => json!({ "type": "icon", "icon_set": icon_set, "icon": icon_id }),
            };
            j["column"] = json!(column_name(*column));
            j
        })
        .collect();
    json!({ "range": range_text(&filter.range), "columns": columns })
}

fn list_tables(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let mut tables = Vec::new();
    let mut filters = Vec::new();
    for (name, sheet) in chosen(open, args)? {
        for (i, t) in sheet.tables.iter().enumerate() {
            tables.push(json!({
                "sheet": name,
                "index": i,
                "name": t.name,
                "range": range_text(&t.range),
                "header_rows": t.header_rows,
                "totals_rows": t.totals_rows,
                "columns": t.columns,
                "style": t.style.as_ref().map(|s| json!({ "name": s.name, "first_column": s.first_column, "last_column": s.last_column, "row_stripes": s.row_stripes, "column_stripes": s.column_stripes })),
                "filter": t.filter.as_ref().map(filter_json),
            }));
        }
        if let Some(f) = &sheet.auto_filter {
            let mut j = filter_json(f);
            j["sheet"] = json!(name);
            filters.push(j);
        }
    }
    Ok(json!({ "tables": tables, "auto_filters": filters }))
}

fn list_comments(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let mut notes = Vec::new();
    let mut threads = Vec::new();
    for (name, sheet) in chosen(open, args)? {
        for c in &sheet.comments {
            notes.push(json!({ "sheet": name, "cell": format!("{}{}", column_name(c.column), c.row + 1), "author": c.author, "text": c.text }));
        }
        for t in &sheet.threads {
            threads.push(json!({ "sheet": name, "cell": format!("{}{}", column_name(t.column), t.row + 1), "resolved": t.done, "posts": t.posts.iter().map(|p| json!({ "author": p.author, "date": p.date, "text": p.text })).collect::<Vec<_>>() }));
        }
    }
    Ok(json!({ "comments": notes, "threads": threads }))
}

fn list_hyperlinks(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let mut links = Vec::new();
    for (name, sheet) in chosen(open, args)? {
        for h in &sheet.hyperlinks {
            links.push(json!({ "sheet": name, "range": range_text(&h.range), "url": h.target, "location": h.location, "tooltip": h.tooltip }));
        }
    }
    Ok(json!({ "hyperlinks": links }))
}

fn points(emu: i64) -> f64 {
    (emu as f64 / EMU_PER_POINT as f64 * 100.0).round() / 100.0
}

/// A paper's name when its size is one of the common ones (millimetres, to 1 mm).
pub(crate) fn paper_name(width: i64, height: i64) -> Option<&'static str> {
    let mm = |e: i64| (e as f64 / 36_000.0).round() as i64;
    Some(match (mm(width), mm(height)) {
        (210, 297) => "a4",
        (297, 420) => "a3",
        (148, 210) => "a5",
        (182, 257) => "b5",
        (257, 364) => "b4",
        (216, 279) => "letter",
        (216, 356) => "legal",
        (100, 148) => "hagaki",
        _ => return None,
    })
}

fn get_page_setup(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let (name, sheet) = sheet_named(open, args.str_arg("sheet")?)?;
    let print = &sheet.print;
    let workbook = open.document.presentation().workbook.as_ref();
    let index = workbook.and_then(|w| w.sheets.iter().position(|s| s.name == name));
    let titles = workbook.and_then(|w| w.names.iter().find(|n| n.name.eq_ignore_ascii_case("_xlnm.Print_Titles") && n.sheet == index && index.is_some())).map(|n| format!("={}", n.formula));
    let hf = &print.header_footer;
    Ok(json!({
        "sheet": name,
        "paper": { "name": paper_name(print.paper.0, print.paper.1), "width": points(print.paper.0), "height": points(print.paper.1) },
        "orientation": if print.landscape { "landscape" } else { "portrait" },
        "margins": { "left": points(print.margins[0]), "right": points(print.margins[1]), "top": points(print.margins[2]), "bottom": points(print.margins[3]), "header": points(print.header_margin), "footer": points(print.footer_margin) },
        "scale": print.scale,
        "fit_to": print.fit_to.map(|(w, h)| json!({ "pages_wide": w, "pages_tall": h })),
        "print_area": sheet.print_area.as_ref().map(range_text),
        "print_titles": titles,
        "center_horizontally": print.centered.0,
        "center_vertically": print.centered.1,
        "print_gridlines": print.grid_lines,
        "print_headings": print.headings,
        "black_and_white": print.black_and_white,
        "page_order": if print.over_then_down { "over_then_down" } else { "down_then_over" },
        "header_footer": { "header": hf.odd_header, "footer": hf.odd_footer, "first_page_header": hf.first_header, "first_page_footer": hf.first_footer, "even_page_header": hf.even_header, "even_page_footer": hf.even_footer, "different_first": hf.different_first, "different_odd_even": hf.different_odd_even },
        "row_breaks": print.row_breaks.iter().map(|r| r + 1).collect::<Vec<_>>(),
        "column_breaks": print.column_breaks.iter().map(|c| column_name(*c)).collect::<Vec<_>>(),
    }))
}

pub(crate) fn chart_kind_name(chart: &ibis_doc_model::chart::Chart) -> String {
    use ibis_doc_model::chart::{ChartType, Grouping};
    let Some(group) = chart.groups.first() else { return "empty".into() };
    let stacked = match group.grouping {
        Grouping::Standard => "",
        Grouping::Stacked => "stacked_",
        Grouping::PercentStacked => "percent_stacked_",
    };
    match group.kind {
        ChartType::Bar { horizontal: true, .. } => format!("{stacked}bar"),
        ChartType::Bar { horizontal: false, .. } => format!("{stacked}column"),
        ChartType::Line => format!("{stacked}line"),
        ChartType::Area => format!("{stacked}area"),
        ChartType::Radar { .. } => "radar".into(),
        ChartType::OfPie { .. } => "of_pie".into(),
        ChartType::Pie { .. } => "pie".into(),
        ChartType::Doughnut { .. } => "doughnut".into(),
        ChartType::Scatter { .. } => "scatter".into(),
        ChartType::Bubble { .. } => "bubble".into(),
        ChartType::Stock { .. } => "stock".into(),
        ChartType::Surface { .. } => "surface".into(),
    }
}

fn list_charts(service: &mut Service, args: &Value) -> Result<Value, ToolError> {
    let open = service.open_document(args.str_arg("document_id")?)?;
    let sheets = sheets_of(open)?;
    let only = args.opt_str("sheet")?.map(|n| crate::tools_xlsx::find_sheet(&sheets, n).map(|(i, name, _)| (*i, name.clone()))).transpose()?;
    let mut charts = Vec::new();
    for (index, name, _) in &sheets {
        if only.as_ref().is_some_and(|(i, _)| i != index) {
            continue;
        }
        for shape in &open.document.presentation().slides[*index].shapes {
            let ShapeKind::Chart(chart) = &shape.kind else { continue };
            let legend = chart.legend.as_ref().map(|l| format!("{:?}", l.side).to_lowercase());
            let series: Vec<Value> = chart
                .groups
                .iter()
                .flat_map(|g| g.series.iter())
                .enumerate()
                .map(|(i, s)| {
                    let cells = chart.cell_refs.get(i);
                    json!({ "name": s.name, "name_cell": cells.and_then(|c| c.name.clone()), "categories": cells.and_then(|c| c.categories.clone()), "values": cells.and_then(|c| c.values.clone()) })
                })
                .collect();
            let f = &shape.frame;
            charts.push(json!({
                "sheet": name,
                "shape": shape.id.0,
                "kind": chart_kind_name(chart),
                "frame": { "x": points(f.x), "y": points(f.y), "width": points(f.width), "height": points(f.height) },
                "title": chart.title.as_ref().map(|t| t.text.clone()),
                "legend": legend,
                "series": series,
            }));
        }
    }
    Ok(json!({ "charts": charts }))
}

/// The document properties as the `*_set_document_properties` tools take them.
pub(crate) fn properties_json(p: &ibis_doc_model::flow::DocProperties) -> Value {
    json!({ "title": p.title, "subject": p.subject, "author": p.creator, "keywords": p.keywords, "description": p.description, "category": p.category, "company": p.company, "manager": p.manager })
}

/// The colour a theme reference has in the workbook now.
pub(crate) fn theme_color_resolution(open: &Open, name: &str, brightness: f64) -> Result<Rgb, ToolError> {
    let theme = ThemeColor { index: crate::tools_xlsx::theme_of(name)?, tint: (brightness * 1_000_000.0).round() as i32 };
    Ok(theme.resolve(&open.document.presentation().theme_colors).unwrap_or(Rgb::BLACK))
}
