// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The parts of a sheet of an `.ods` beyond its cells that become parts of the sheet of the `.xlsx`: conditional
//! formats, data validations, notes. Written from the OpenDocument specification (ODF 1.3: `table:content-validations`,
//! `office:annotation`; the conditional formats as `calcext:conditional-formats`, the extension that
//! LibreOffice Calc writes, whose names are published in its file format description).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use roxmltree::Node;

use super::{CALCEXT, Format, Styles, attr, children, esc, formula_to_excel, split_reference};

/// The text between the first `(` and the last `)` of `text`, split at the commas (or semicolons) that are not in
/// brackets, parentheses or quotes.
pub(super) fn arguments(text: &str) -> Vec<String> {
    let inner = match (text.find('('), text.rfind(')')) {
        (Some(a), Some(b)) if b > a => &text[a + 1..b],
        _ => return Vec::new(),
    };
    split_top(inner)
}

fn split_top(inner: &str) -> Vec<String> {
    let (mut out, mut cur, mut depth, mut quote) = (Vec::new(), String::new(), 0i32, false);
    for ch in inner.chars() {
        match ch {
            '"' => {
                quote = !quote;
                cur.push(ch);
            }
            '(' | '[' | '{' if !quote => {
                depth += 1;
                cur.push(ch);
            }
            ')' | ']' | '}' if !quote => {
                depth -= 1;
                cur.push(ch);
            }
            ',' | ';' if !quote && depth == 0 => out.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() || !out.is_empty() {
        out.push(cur);
    }
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

/// A formula of a document (`[.A1]*2`) as an Excel formula.
fn excel(formula: &str) -> String {
    formula_to_excel(&format!("of:={formula}"))
}

/// The `sqref` text of range addresses (`Data.A1:Data.B3 Data.D1`).
fn sqref(addresses: &str) -> String {
    addresses
        .split_whitespace()
        .map(|range| range.split(':').map(|p| split_reference(p).1.replace('$', "")).collect::<Vec<_>>().join(":"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn dxf_of(format: &Format) -> String {
    let mut font = String::new();
    if format.bold == Some(true) {
        font.push_str("<b/>");
    }
    if format.italic == Some(true) {
        font.push_str("<i/>");
    }
    if format.strike == Some(true) {
        font.push_str("<strike/>");
    }
    if format.underline == Some(true) {
        font.push_str(if format.double_underline { "<u val=\"double\"/>" } else { "<u/>" });
    }
    if let Some(c) = &format.color {
        font.push_str(&format!("<color rgb=\"FF{c}\"/>"));
    }
    let mut x = String::from("<dxf>");
    if !font.is_empty() {
        x.push_str(&format!("<font>{font}</font>"));
    }
    if let Some(c) = &format.fill {
        x.push_str(&format!("<fill><patternFill><bgColor rgb=\"FF{c}\"/></patternFill></fill>"));
    }
    x.push_str("</dxf>");
    x
}

fn cfvo(kind: &str, value: Option<&str>) -> String {
    let t = match kind {
        "minimum" | "auto-minimum" => "min",
        "maximum" | "auto-maximum" => "max",
        "percentile" => "percentile",
        "percent" => "percent",
        "formula" => "formula",
        _ => "num",
    };
    match (t, value) {
        ("min" | "max", _) | (_, None) => format!("<cfvo type=\"{t}\"/>"),
        (_, Some(v)) => format!("<cfvo type=\"{t}\" val=\"{}\"/>", esc(v)),
    }
}

/// One rule of the condition of a `calcext:condition` (`value`) as a `cfRule`; `base` is the top left cell of the range.
fn condition_rule(value: &str, base: &str, dxf: Option<usize>, priority: u32) -> Option<String> {
    let value = value.trim();
    let dxf_attr = dxf.map(|d| format!(" dxfId=\"{d}\"")).unwrap_or_default();
    let head = format!("priority=\"{priority}\"");
    for (op, name) in [(">=", "greaterThanOrEqual"), ("<=", "lessThanOrEqual"), ("!=", "notEqual"), (">", "greaterThan"), ("<", "lessThan"), ("=", "equal")] {
        if let Some(rest) = value.strip_prefix(op) {
            return Some(format!("<cfRule type=\"cellIs\"{dxf_attr} {head} operator=\"{name}\"><formula>{}</formula></cfRule>", esc(&excel(rest.trim()))));
        }
    }
    let name = value.split('(').next().unwrap_or("").trim();
    let args = arguments(value);
    let text_arg = |i: usize| args.get(i).map(|a| a.trim_matches('"').replace("\"\"", "\"")).unwrap_or_default();
    let number = |i: usize| args.get(i).and_then(|a| a.parse::<u32>().ok()).unwrap_or(10);
    let cell = base;
    Some(match name {
        "between" | "not-between" if args.len() == 2 => format!("<cfRule type=\"cellIs\"{dxf_attr} {head} operator=\"{}\"><formula>{}</formula><formula>{}</formula></cfRule>", if name == "between" { "between" } else { "notBetween" }, esc(&excel(&args[0])), esc(&excel(&args[1]))),
        "formula-is" => format!("<cfRule type=\"expression\"{dxf_attr} {head}><formula>{}</formula></cfRule>", esc(&excel(&value["formula-is(".len()..value.len() - 1]))),
        "duplicate" => format!("<cfRule type=\"duplicateValues\"{dxf_attr} {head}/>"),
        "unique" => format!("<cfRule type=\"uniqueValues\"{dxf_attr} {head}/>"),
        "is-error" => format!("<cfRule type=\"containsErrors\"{dxf_attr} {head}><formula>ISERROR({cell})</formula></cfRule>"),
        "is-no-error" => format!("<cfRule type=\"notContainsErrors\"{dxf_attr} {head}><formula>NOT(ISERROR({cell}))</formula></cfRule>"),
        "above-average" => format!("<cfRule type=\"aboveAverage\"{dxf_attr} {head}/>"),
        "below-average" => format!("<cfRule type=\"aboveAverage\"{dxf_attr} {head} aboveAverage=\"0\"/>"),
        "above-equal-average" => format!("<cfRule type=\"aboveAverage\"{dxf_attr} {head} equalAverage=\"1\"/>"),
        "below-equal-average" => format!("<cfRule type=\"aboveAverage\"{dxf_attr} {head} aboveAverage=\"0\" equalAverage=\"1\"/>"),
        "top-elements" => format!("<cfRule type=\"top10\"{dxf_attr} {head} rank=\"{}\"/>", number(0)),
        "bottom-elements" => format!("<cfRule type=\"top10\"{dxf_attr} {head} bottom=\"1\" rank=\"{}\"/>", number(0)),
        "top-percent" => format!("<cfRule type=\"top10\"{dxf_attr} {head} percent=\"1\" rank=\"{}\"/>", number(0)),
        "bottom-percent" => format!("<cfRule type=\"top10\"{dxf_attr} {head} percent=\"1\" bottom=\"1\" rank=\"{}\"/>", number(0)),
        "contains-text" | "not-contains-text" | "begins-with" | "ends-with" => {
            let t = text_arg(0);
            let q = format!("\"{}\"", t.replace('"', "\"\""));
            let (kind, operator, formula) = match name {
                "contains-text" => ("containsText", "containsText", format!("NOT(ISERROR(SEARCH({q},{cell})))")),
                "not-contains-text" => ("notContainsText", "notContains", format!("ISERROR(SEARCH({q},{cell}))")),
                "begins-with" => ("beginsWith", "beginsWith", format!("LEFT({cell},LEN({q}))={q}")),
                _ => ("endsWith", "endsWith", format!("RIGHT({cell},LEN({q}))={q}")),
            };
            format!("<cfRule type=\"{kind}\"{dxf_attr} {head} operator=\"{operator}\" text=\"{}\"><formula>{}</formula></cfRule>", esc(&t), esc(&formula))
        }
        _ => return None,
    })
}

/// The `conditionalFormatting` elements of a table (`calcext:conditional-formats`). `own` gives what the style of a name says.
pub(super) fn conditional_xml(table: Node, own: &dyn Fn(&str) -> Option<Format>, styles: &mut Styles) -> String {
    let mut out = String::new();
    let mut priority = 1u32;
    for formats in children(table, CALCEXT, "conditional-formats") {
        for format in children(formats, CALCEXT, "conditional-format") {
            let Some(target) = attr(format, CALCEXT, "target-range-address") else { continue };
            let sq = sqref(target);
            if sq.is_empty() {
                continue;
            }
            let base = sq.split([' ', ':']).next().unwrap_or("A1").to_string();
            let mut rules = String::new();
            for item in format.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(CALCEXT)) {
                match item.tag_name().name() {
                    "condition" => {
                        let (Some(value), style) = (attr(item, CALCEXT, "value"), attr(item, CALCEXT, "apply-style-name")) else { continue };
                        let dxf = style.and_then(own).map(|f| styles.dxf(&dxf_of(&f)));
                        if let Some(rule) = condition_rule(value, &base, dxf, priority) {
                            rules.push_str(&rule);
                            priority += 1;
                        }
                    }
                    "color-scale" => {
                        let entries: Vec<Node> = children(item, CALCEXT, "color-scale-entry").collect();
                        if entries.len() < 2 {
                            continue;
                        }
                        let values: String = entries.iter().map(|e| cfvo(attr(*e, CALCEXT, "type").unwrap_or("minimum"), attr(*e, CALCEXT, "value"))).collect();
                        let colors: String = entries.iter().map(|e| format!("<color rgb=\"FF{}\"/>", attr(*e, CALCEXT, "color").map(|c| c.trim_start_matches('#').to_ascii_uppercase()).unwrap_or_else(|| "FFFFFF".into()))).collect();
                        rules.push_str(&format!("<cfRule type=\"colorScale\" priority=\"{priority}\"><colorScale>{values}{colors}</colorScale></cfRule>"));
                        priority += 1;
                    }
                    "data-bar" => {
                        let entries: Vec<Node> = children(item, CALCEXT, "formatting-entry").collect();
                        let (lo, hi) = match entries.as_slice() {
                            [a, b, ..] => (cfvo(attr(*a, CALCEXT, "type").unwrap_or("minimum"), attr(*a, CALCEXT, "value")), cfvo(attr(*b, CALCEXT, "type").unwrap_or("maximum"), attr(*b, CALCEXT, "value"))),
                            _ => ("<cfvo type=\"min\"/>".to_string(), "<cfvo type=\"max\"/>".to_string()),
                        };
                        let color = attr(item, CALCEXT, "positive-color").map(|c| c.trim_start_matches('#').to_ascii_uppercase()).unwrap_or_else(|| "638EC6".into());
                        let lengths = match (attr(item, CALCEXT, "min-length"), attr(item, CALCEXT, "max-length")) {
                            (Some(a), Some(b)) if (a, b) != ("10", "90") => format!(" minLength=\"{}\" maxLength=\"{}\"", esc(a), esc(b)),
                            _ => String::new(),
                        };
                        rules.push_str(&format!("<cfRule type=\"dataBar\" priority=\"{priority}\"><dataBar{lengths}>{lo}{hi}<color rgb=\"FF{color}\"/></dataBar></cfRule>"));
                        priority += 1;
                    }
                    "icon-set" => {
                        let entries: Vec<Node> = children(item, CALCEXT, "formatting-entry").collect();
                        if entries.is_empty() {
                            continue;
                        }
                        let set = attr(item, CALCEXT, "icon-set-type").unwrap_or("3TrafficLights1");
                        let values: String = entries.iter().map(|e| cfvo(attr(*e, CALCEXT, "type").unwrap_or("percent"), attr(*e, CALCEXT, "value"))).collect();
                        let show = if attr(item, CALCEXT, "show-value") == Some("false") { " showValue=\"0\"" } else { "" };
                        rules.push_str(&format!("<cfRule type=\"iconSet\" priority=\"{priority}\"><iconSet iconSet=\"{}\"{show}>{values}</iconSet></cfRule>", esc(set)));
                        priority += 1;
                    }
                    _ => {}
                }
            }
            if !rules.is_empty() {
                out.push_str(&format!("<conditionalFormatting sqref=\"{}\">{rules}</conditionalFormatting>", esc(&sq)));
            }
        }
    }
    out
}

/// The conditional formats Excel writes as `style:map` elements of a cell style (the cells with the style, the conditions with the
/// style to apply: `of:is-true-formula(MOD(ROW(); 2) = 0)` for a banded row, `cell-content()>5`, `cell-content-is-between(1; 5)`), as
/// `conditionalFormatting` elements: one per style over the cells that have it. A formula's relative references are not moved to the first
/// cell of the range.
pub(super) fn style_map_xml(cells: &BTreeMap<String, BTreeSet<(u32, u32)>>, maps: &HashMap<String, Vec<(String, String)>>, own: &dyn Fn(&str) -> Option<Format>, styles: &mut Styles, first_priority: u32) -> String {
    let mut out = String::new();
    let mut priority = first_priority;
    for (style, set) in cells {
        let Some(list) = maps.get(style) else { continue };
        let rects = rectangles(set);
        let Some(first) = rects.first() else { continue };
        let base = format!("{}{}", super::column_letters(first.1), first.0 + 1);
        let sq: String = rects.iter().map(|(r0, c0, r1, c1)| if (r0, c0) == (r1, c1) { format!("{}{}", super::column_letters(*c0), r0 + 1) } else { format!("{}{}:{}{}", super::column_letters(*c0), r0 + 1, super::column_letters(*c1), r1 + 1) }).collect::<Vec<_>>().join(" ");
        let mut rules = String::new();
        for (condition, apply) in list {
            let c = condition.trim();
            let c = c.strip_prefix("of:").unwrap_or(c);
            let value = if let Some(rest) = c.strip_prefix("is-true-formula(") {
                format!("formula-is({rest}")
            } else if let Some(rest) = c.strip_prefix("cell-content-is-between(") {
                format!("between({rest}")
            } else if let Some(rest) = c.strip_prefix("cell-content-is-not-between(") {
                format!("not-between({rest}")
            } else if let Some(rest) = c.strip_prefix("cell-content()") {
                rest.to_string()
            } else {
                continue;
            };
            let dxf = own(apply).map(|f| styles.dxf(&dxf_of(&f)));
            if let Some(rule) = condition_rule(&value, &base, dxf, priority) {
                rules.push_str(&rule);
                priority += 1;
            }
        }
        if !rules.is_empty() {
            out.push_str(&format!("<conditionalFormatting sqref=\"{}\">{rules}</conditionalFormatting>", esc(&sq)));
        }
    }
    out
}

/// The cells of `cells` (row, column) as the rectangles they make: runs along each row, runs of the same columns on
/// the next row joined.
pub(super) fn rectangles(cells: &BTreeSet<(u32, u32)>) -> Vec<(u32, u32, u32, u32)> {
    let mut runs: Vec<(u32, u32, u32)> = Vec::new(); // (row, first column, last column)
    for &(r, c) in cells {
        match runs.last_mut() {
            Some((lr, _, lc)) if *lr == r && *lc + 1 == c => *lc = c,
            _ => runs.push((r, c, c)),
        }
    }
    let mut rects: Vec<(u32, u32, u32, u32)> = Vec::new(); // (first row, first column, last row, last column)
    for (r, c0, c1) in runs {
        match rects.iter_mut().find(|(_, a, lr, b)| *a == c0 && *b == c1 && *lr + 1 == r) {
            Some(rect) => rect.2 = r,
            None => rects.push((r, c0, r, c1)),
        }
    }
    rects
}

// ---------------------------------------------------------------- validations

/// The text of the `text:p`s of a message, with newlines between.
fn message_text(n: Node) -> String {
    children(n, super::TEXT, "p").map(|p| p.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect::<String>()).collect::<Vec<_>>().join("\n")
}

/// `type`, `operator`, formulas of a condition (`of:cell-content-is-whole-number() and cell-content-is-between(1,10)`).
fn validation_condition(condition: &str) -> Option<(String, Option<String>, Option<String>, Option<String>)> {
    let text = condition.trim();
    let text = text.strip_prefix("of:").or_else(|| text.strip_prefix("oooc:")).unwrap_or(text);
    // The parts joined with ` and ` (outside quotes).
    let mut parts: Vec<String> = Vec::new();
    let (mut cur, mut quote) = (String::new(), false);
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '"' {
            quote = !quote;
        }
        if !quote && chars[i..].iter().take(5).collect::<String>() == " and " {
            parts.push(std::mem::take(&mut cur));
            i += 5;
            continue;
        }
        cur.push(chars[i]);
        i += 1;
    }
    parts.push(cur);
    let (mut kind, mut operator, mut f1, mut f2) = (None::<String>, None::<String>, None::<String>, None::<String>);
    for part in parts.iter().map(|p| p.trim()) {
        let name = part.split('(').next().unwrap_or("");
        let args = arguments(part);
        match name {
            "cell-content-is-whole-number" => kind = Some("whole".into()),
            "cell-content-is-decimal-number" => kind = Some("decimal".into()),
            "cell-content-is-date" => kind = Some("date".into()),
            "cell-content-is-time" => kind = Some("time".into()),
            "cell-content-is-in-list" => {
                kind = Some("list".into());
                let quoted = !args.is_empty() && args.iter().all(|a| a.starts_with('"') && a.ends_with('"') && a.len() >= 2);
                f1 = Some(if quoted { format!("\"{}\"", args.iter().map(|a| a[1..a.len() - 1].replace("\"\"", "\"")).collect::<Vec<_>>().join(",")) } else { excel(args.first().map(String::as_str).unwrap_or("")) });
            }
            "is-true-formula" => {
                kind = Some("custom".into());
                f1 = Some(excel(&part["is-true-formula(".len()..part.len() - 1]));
            }
            "cell-content-is-between" | "cell-content-is-not-between" | "cell-content-text-length-is-between" | "cell-content-text-length-is-not-between" if args.len() == 2 => {
                if name.starts_with("cell-content-text-length") {
                    kind = Some("textLength".into());
                }
                operator = Some(if name.contains("not-between") { "notBetween" } else { "between" }.into());
                f1 = Some(excel(&args[0]));
                f2 = Some(excel(&args[1]));
            }
            _ => {
                let length = part.starts_with("cell-content-text-length()");
                let rest = part.strip_prefix("cell-content-text-length()").or_else(|| part.strip_prefix("cell-content()"));
                if let Some(rest) = rest {
                    for (op, name) in [(">=", "greaterThanOrEqual"), ("<=", "lessThanOrEqual"), ("!=", "notEqual"), (">", "greaterThan"), ("<", "lessThan"), ("=", "equal")] {
                        if let Some(v) = rest.strip_prefix(op) {
                            operator = Some(name.into());
                            f1 = Some(excel(v.trim()));
                            break;
                        }
                    }
                    if length {
                        kind = Some("textLength".into());
                    }
                }
            }
        }
    }
    let kind = kind.or_else(|| operator.as_ref().map(|_| "decimal".to_string()))?;
    Some((kind, operator, f1, f2))
}

/// The `dataValidations` element: `definitions` are the `table:content-validation` elements by name, `cells` the
/// cells that use each (by name).
pub(super) fn validations_xml(definitions: &std::collections::HashMap<String, Node>, cells: &std::collections::BTreeMap<String, BTreeSet<(u32, u32)>>) -> String {
    let mut out = String::new();
    let mut count = 0;
    for (name, set) in cells {
        let Some(def) = definitions.get(name) else { continue };
        let Some((kind, operator, f1, f2)) = attr(*def, super::TABLE, "condition").and_then(validation_condition) else { continue };
        let sq: Vec<String> = rectangles(set)
            .into_iter()
            .map(|(r0, c0, r1, c1)| {
                let a = format!("{}{}", super::column_letters(c0), r0 + 1);
                if (r0, c0) == (r1, c1) { a } else { format!("{a}:{}{}", super::column_letters(c1), r1 + 1) }
            })
            .collect();
        if sq.is_empty() {
            continue;
        }
        let mut x = format!("<dataValidation type=\"{kind}\"");
        if let Some(o) = &operator {
            if kind != "list" && kind != "custom" && o != "between" {
                x.push_str(&format!(" operator=\"{o}\""));
            }
        }
        if attr(*def, super::TABLE, "allow-empty-cell") != Some("false") {
            x.push_str(" allowBlank=\"1\"");
        }
        if kind == "list" && attr(*def, super::TABLE, "display-list") == Some("none") {
            x.push_str(" showDropDown=\"1\"");
        }
        if let Some(help) = children(*def, super::TABLE, "help-message").next() {
            if attr(help, super::TABLE, "display") != Some("false") {
                x.push_str(" showInputMessage=\"1\"");
            }
            if let Some(t) = attr(help, super::TABLE, "title").filter(|t| !t.is_empty()) {
                x.push_str(&format!(" promptTitle=\"{}\"", esc(t)));
            }
            let m = message_text(help);
            if !m.is_empty() {
                x.push_str(&format!(" prompt=\"{}\"", esc(&m)));
            }
        }
        if let Some(error) = children(*def, super::TABLE, "error-message").next() {
            if attr(error, super::TABLE, "display") != Some("false") {
                x.push_str(" showErrorMessage=\"1\"");
            }
            match attr(error, super::TABLE, "message-type") {
                Some("warning") => x.push_str(" errorStyle=\"warning\""),
                Some("information") => x.push_str(" errorStyle=\"information\""),
                _ => {}
            }
            if let Some(t) = attr(error, super::TABLE, "title").filter(|t| !t.is_empty()) {
                x.push_str(&format!(" errorTitle=\"{}\"", esc(t)));
            }
            let m = message_text(error);
            if !m.is_empty() {
                x.push_str(&format!(" error=\"{}\"", esc(&m)));
            }
        }
        x.push_str(&format!(" sqref=\"{}\">", sq.join(" ")));
        if let Some(f) = f1 {
            x.push_str(&format!("<formula1>{}</formula1>", esc(&f)));
        }
        if let Some(f) = f2 {
            x.push_str(&format!("<formula2>{}</formula2>", esc(&f)));
        }
        x.push_str("</dataValidation>");
        out.push_str(&x);
        count += 1;
    }
    if count == 0 { out } else { format!("<dataValidations count=\"{count}\">{out}</dataValidations>") }
}
