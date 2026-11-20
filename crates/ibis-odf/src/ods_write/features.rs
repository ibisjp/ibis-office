// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The sheet parts beyond the cells in an `.ods`: conditional formats (as `calcext:conditional-formats`, the
//! extension LibreOffice Calc reads and writes), data validations, notes. Written from the OpenDocument specification.

use ibis_doc_model::{CellUnderline, CfKind, CfStyle, CfValue, DataBarAxisPosition, Sheet};

use super::{esc, formula_to_odf};

/// A sheet name in an address (`Data`, `'My Sheet'`).
fn quoted(name: &str) -> String {
    if name.chars().all(|c| c.is_alphanumeric() || c == '_') { name.to_string() } else { format!("'{}'", name.replace('\'', "''")) }
}

fn address(name: &str, row: u32, column: u32) -> String {
    format!("{}.{}{}", quoted(name), super::column_letters(column), row + 1)
}

/// A formula of Excel as the text in an ODF condition (no `of:=`).
fn odf(formula: &str) -> String {
    formula_to_odf(formula).strip_prefix("of:=").unwrap_or(formula).to_string()
}

fn hex(rgb: ibis_doc_model::Rgb) -> String {
    format!("#{:06x}", rgb.0)
}

/// The style a conditional format gives: the XML of a named cell style.
fn style_xml(name: &str, s: &CfStyle) -> String {
    let mut cell = String::new();
    if let Some(f) = s.fill {
        cell.push_str(&format!(" fo:background-color=\"{}\"", hex(f)));
    }
    let mut text = String::new();
    if let Some(c) = s.color {
        text.push_str(&format!(" fo:color=\"{}\"", hex(c)));
    }
    if s.bold == Some(true) {
        text.push_str(" fo:font-weight=\"bold\"");
    }
    if s.italic == Some(true) {
        text.push_str(" fo:font-style=\"italic\"");
    }
    match s.underline {
        Some(CellUnderline::Double | CellUnderline::DoubleAccounting) => text.push_str(" style:text-underline-style=\"solid\" style:text-underline-type=\"double\""),
        Some(CellUnderline::None) | None => {}
        Some(_) => text.push_str(" style:text-underline-style=\"solid\" style:text-underline-type=\"single\""),
    }
    if s.strike == Some(true) {
        text.push_str(" style:text-line-through-style=\"solid\"");
    }
    format!("<style:style style:name=\"{name}\" style:family=\"table-cell\" style:parent-style-name=\"Default\"><style:table-cell-properties{cell}/><style:text-properties{text}/></style:style>")
}

fn entry_type(kind: &str) -> &'static str {
    match kind {
        "min" => "minimum",
        "max" => "maximum",
        "percent" => "percent",
        "percentile" => "percentile",
        "formula" => "formula",
        "autoMin" => "auto-minimum",
        "autoMax" => "auto-maximum",
        _ => "number",
    }
}

fn entry(tag: &str, v: &CfValue, color: Option<ibis_doc_model::Rgb>) -> String {
    let value = if v.kind == "formula" { odf(v.value.as_deref().unwrap_or("0")) } else { v.value.clone().unwrap_or_else(|| "0".into()) };
    let color = color.map(|c| format!(" calcext:color=\"{}\"", hex(c))).unwrap_or_default();
    format!("<calcext:{tag} calcext:value=\"{}\" calcext:type=\"{}\"{color}/>", esc(&value), entry_type(&v.kind))
}

/// `calcext:conditional-formats` of the sheet named `name`, and the named styles the rules use (appended to `styles`,
/// numbered from `next_style`).
pub(super) fn conditional(sheet: &Sheet, name: &str, next_style: &mut u32, styles: &mut String) -> String {
    let mut out = String::new();
    for format in &sheet.conditional {
        let Some(first) = format.ranges.first() else { continue };
        let target: Vec<String> = format.ranges.iter().map(|r| format!("{}:{}", address(name, r.first_row, r.first_column), address(name, r.last_row, r.last_column))).collect();
        let base = address(name, first.first_row, first.first_column);
        let mut rules = String::new();
        let mut rules_in_order: Vec<_> = format.rules.iter().collect();
        rules_in_order.sort_by_key(|r| r.priority);
        for rule in rules_in_order {
            let condition = |value: String, rules: &mut String, styles: &mut String, next: &mut u32| {
                let style = rule.style.as_ref().map(|s| {
                    *next += 1;
                    let n = format!("ConditionalStyle_{}", *next);
                    styles.push_str(&style_xml(&n, s));
                    format!(" calcext:apply-style-name=\"{n}\"")
                });
                rules.push_str(&format!("<calcext:condition{} calcext:value=\"{}\" calcext:base-cell-address=\"{base}\"/>", style.unwrap_or_default(), esc(&value)));
            };
            match &rule.kind {
                CfKind::CellIs { operator, formulas } => {
                    let a = formulas.first().map(|f| odf(f)).unwrap_or_default();
                    let b = formulas.get(1).map(|f| odf(f)).unwrap_or_default();
                    let value = match operator.as_str() {
                        "greaterThan" => format!(">{a}"),
                        "lessThan" => format!("<{a}"),
                        "greaterThanOrEqual" => format!(">={a}"),
                        "lessThanOrEqual" => format!("<={a}"),
                        "equal" => format!("={a}"),
                        "notEqual" => format!("!={a}"),
                        "between" => format!("between({a},{b})"),
                        "notBetween" => format!("not-between({a},{b})"),
                        _ => continue,
                    };
                    condition(value, &mut rules, styles, next_style);
                }
                CfKind::Expression(f) => condition(format!("formula-is({})", odf(f)), &mut rules, styles, next_style),
                CfKind::Top10 { rank, percent, bottom } => condition(format!("{}-{}({rank})", if *bottom { "bottom" } else { "top" }, if *percent { "percent" } else { "elements" }), &mut rules, styles, next_style),
                CfKind::AboveAverage { above, equal, std_dev: 0 } => condition(format!("{}-{}average", if *above { "above" } else { "below" }, if *equal { "equal-" } else { "" }), &mut rules, styles, next_style),
                CfKind::Unique => condition("unique".into(), &mut rules, styles, next_style),
                CfKind::Duplicate => condition("duplicate".into(), &mut rules, styles, next_style),
                CfKind::ColorScale(points) if points.len() >= 2 => {
                    rules.push_str("<calcext:color-scale>");
                    for (v, c) in points {
                        rules.push_str(&entry("color-scale-entry", v, Some(*c)));
                    }
                    rules.push_str("</calcext:color-scale>");
                }
                CfKind::DataBar { min, max, color, negative_color, axis_color, axis_position, min_length, max_length, gradient, .. } => {
                    let axis = match axis_position {
                        Some(DataBarAxisPosition::Midpoint) => "middle",
                        Some(DataBarAxisPosition::None) => "none",
                        _ => "automatic",
                    };
                    rules.push_str(&format!(
                        "<calcext:data-bar calcext:min-length=\"{}\" calcext:max-length=\"{}\" calcext:negative-color=\"{}\" calcext:axis-position=\"{axis}\" calcext:positive-color=\"{}\" calcext:axis-color=\"{}\"{}>{}{}</calcext:data-bar>",
                        min_length.unwrap_or(10),
                        max_length.unwrap_or(90),
                        hex(negative_color.unwrap_or(ibis_doc_model::Rgb(0xFF0000))),
                        hex(*color),
                        hex(axis_color.unwrap_or(ibis_doc_model::Rgb(0))),
                        if *gradient { "" } else { " calcext:gradient=\"false\"" },
                        entry("formatting-entry", min, None),
                        entry("formatting-entry", max, None)
                    ));
                }
                CfKind::IconSet { set, values, show_value, .. } if !values.is_empty() => {
                    rules.push_str(&format!("<calcext:icon-set calcext:icon-set-type=\"{}\"{}>", esc(set), if *show_value { "" } else { " calcext:show-value=\"false\"" }));
                    for v in values {
                        rules.push_str(&entry("formatting-entry", v, None));
                    }
                    rules.push_str("</calcext:icon-set>");
                }
                _ => {}
            }
        }
        if !rules.is_empty() {
            out.push_str(&format!("<calcext:conditional-format calcext:target-range-address=\"{}\">{rules}</calcext:conditional-format>", target.join(" ")));
        }
    }
    if out.is_empty() { out } else { format!("<calcext:conditional-formats>{out}</calcext:conditional-formats>") }
}

// ---------------------------------------------------------------- validations

/// The condition text of a validation (`of:cell-content-is-whole-number() and cell-content-is-between(1,10)`).
fn validation_condition(v: &ibis_doc_model::Validation) -> Option<String> {
    let f1 = v.formula1.as_deref().map(odf);
    let f2 = v.formula2.as_deref().map(odf);
    let a = f1.clone().unwrap_or_default();
    let b = f2.clone().unwrap_or_default();
    let compare = |length: bool| -> Option<String> {
        let subject = if length { "cell-content-text-length()" } else { "cell-content()" };
        Some(match v.operator.as_str() {
            "between" | "" => format!("cell-content{}-is-between({a},{b})", if length { "-text-length" } else { "" }),
            "notBetween" => format!("cell-content{}-is-not-between({a},{b})", if length { "-text-length" } else { "" }),
            "equal" => format!("{subject}={a}"),
            "notEqual" => format!("{subject}!={a}"),
            "greaterThan" => format!("{subject}>{a}"),
            "lessThan" => format!("{subject}<{a}"),
            "greaterThanOrEqual" => format!("{subject}>={a}"),
            "lessThanOrEqual" => format!("{subject}<={a}"),
            _ => return None,
        })
    };
    let body = match v.kind.as_str() {
        "list" => {
            let list = v.formula1.as_deref()?;
            match list.strip_prefix('"').and_then(|l| l.strip_suffix('"')) {
                Some(items) => format!("cell-content-is-in-list({})", items.split(',').map(|i| format!("\"{}\"", i.replace('"', "\"\""))).collect::<Vec<_>>().join(";")),
                None => format!("cell-content-is-in-list({a})"),
            }
        }
        "whole" => format!("cell-content-is-whole-number() and {}", compare(false)?),
        "decimal" => format!("cell-content-is-decimal-number() and {}", compare(false)?),
        "date" => format!("cell-content-is-date() and {}", compare(false)?),
        "time" => format!("cell-content-is-time() and {}", compare(false)?),
        "textLength" => compare(true)?,
        "custom" => format!("is-true-formula({a})"),
        _ => return None,
    };
    Some(format!("of:{body}"))
}

/// The `table:content-validation` elements (to go in `table:content-validations`), and for each cell the name of the validation it has (the first rule over it).
pub(super) fn validations(sheet: &Sheet, name: &str, prefix: &str, cells: &mut std::collections::HashMap<(u32, u32), String>) -> String {
    let mut out = String::new();
    for (n, v) in sheet.validations.iter().enumerate() {
        let Some(condition) = validation_condition(v) else { continue };
        let Some(first) = v.ranges.first() else { continue };
        let id = format!("{prefix}{}", n + 1);
        let mut x = format!("<table:content-validation table:name=\"{id}\" table:condition=\"{}\" table:allow-empty-cell=\"{}\"", esc(&condition), v.allow_blank);
        if v.kind == "list" {
            x.push_str(if v.dropdown { " table:display-list=\"unsorted\"" } else { " table:display-list=\"none\"" });
        }
        x.push_str(&format!(" table:base-cell-address=\"{}\">", address(name, first.first_row, first.first_column)));
        if let Some((title, text)) = &v.prompt {
            x.push_str(&format!("<table:help-message table:title=\"{}\" table:display=\"true\">{}</table:help-message>", esc(title), paragraphs(text)));
        }
        if let Some((style, title, text)) = &v.error {
            let kind = match style.as_str() {
                "warning" => "warning",
                "information" => "information",
                _ => "stop",
            };
            x.push_str(&format!("<table:error-message table:message-type=\"{kind}\" table:title=\"{}\" table:display=\"true\">{}</table:error-message>", esc(title), paragraphs(text)));
        }
        x.push_str("</table:content-validation>");
        out.push_str(&x);
        for r in &v.ranges {
            // (A rule over a whole column or row marks only the cells of the first rows and columns.)
            for row in r.first_row..=r.last_row.min(r.first_row + 2047) {
                for col in r.first_column..=r.last_column.min(r.first_column + 255) {
                    cells.entry((row, col)).or_insert_with(|| id.clone());
                }
            }
        }
    }
    out
}

fn paragraphs(text: &str) -> String {
    text.split('\n').map(|l| format!("<text:p>{}</text:p>", esc(l))).collect()
}
