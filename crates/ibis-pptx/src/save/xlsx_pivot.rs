// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Saving pivot tables: one built again keeps its parts and asks Excel to
//! build its cache again on opening; one made in the app, or laid out
//! anew, gets its definition and its cache's definition written from the
//! model (ECMA-376 Part 1 §18.10), with no records and `refreshOnLoad`, so
//! Excel builds them from the source when it opens the file.

use ibis_doc_model::{CellValue, PivotAxis, PivotTable, PivotSort};
use ibis_ooxml::Package;

use super::super::unsupported;
use crate::Error;

const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_PIVOT_TABLE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable";
const REL_CACHE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition";
const REL_RECORDS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheRecords";
const CT_PIVOT_TABLE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotTable+xml";
const CT_CACHE: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheDefinition+xml";
const CT_RECORDS: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.pivotCacheRecords+xml";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn range(r: ibis_doc_model::CellRange) -> String {
    format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row))
}

/// The same table but for what building it again changes.
fn layout(p: &PivotTable) -> PivotTable {
    PivotTable {
        location: ibis_doc_model::CellRange { last_row: p.location.first_row, last_column: p.location.first_column, ..p.location },
        first_header_row: 0,
        first_data_row: 0,
        first_data_col: 0,
        row_items: Vec::new(),
        col_items: Vec::new(),
        refreshed: false,
        refresh_on_load: false,
        buttons: Vec::new(),
        corner: None,
        ..p.clone()
    }
}

/// A number as the file writes it.
fn number(n: f64) -> String {
    ibis_calc::number_to_xml(n)
}

fn serial_to_iso_date(serial: f64) -> String {
    let day = serial.trunc() as i64;
    let (y, m, d) = ibis_calc::eval::convert::date_from_serial(day, false);
    let frac = (serial.fract().abs() * 86400.0).round() as u32;
    let sec = frac % 60;
    let min = (frac / 60) % 60;
    let hour = (frac / 3600) % 24;
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}")
}

fn manual_group_items(f: &ibis_doc_model::PivotField, groups: &std::collections::HashMap<String, String>) -> (Vec<String>, Vec<usize>) {
    let mut group_items: Vec<String> = Vec::new();
    let mut item_to_group_idx: Vec<usize> = Vec::new();

    for item_val in &f.shared {
        let name = match item_val {
            CellValue::Text(t) => t.clone(),
            CellValue::Number(n) => number(*n),
            CellValue::Bool(b) => if *b { "TRUE".into() } else { "FALSE".into() },
            CellValue::Error(e) => e.as_str().to_string(),
            CellValue::Empty => String::new(),
        };
        let target_name = groups.get(&name).cloned().unwrap_or(name);
        let idx = if let Some(pos) = group_items.iter().position(|g| g == &target_name) {
            pos
        } else {
            group_items.push(target_name);
            group_items.len() - 1
        };
        item_to_group_idx.push(idx);
    }
    (group_items, item_to_group_idx)
}

fn range_group_items(f: &ibis_doc_model::PivotField, g: &ibis_doc_model::PivotFieldGrouping) -> Vec<String> {
    match g {
        ibis_doc_model::PivotFieldGrouping::Manual { groups } => manual_group_items(f, groups).0,
        ibis_doc_model::PivotFieldGrouping::Number { start, end, interval, .. } => {
            let mut items = Vec::new();
            items.push(format!("<{}", number(*start)));
            let step = if *interval > 0.0 { *interval } else { 1.0 };
            if *end <= *start {
                items.push(format!("{}-{}", number(*start), number(*end)));
            } else {
                let mut lo = *start;
                while lo <= *end && items.len() < 1000 {
                    let hi = lo + step;
                    let hi_incl = if step.fract() == 0.0 && lo.fract() == 0.0 {
                        (hi - 1.0).min(*end)
                    } else {
                        hi.min(*end)
                    };
                    if hi_incl <= lo {
                        items.push(number(lo));
                    } else {
                        items.push(format!("{}-{}", number(lo), number(hi_incl)));
                    }
                    lo = hi;
                }
            }
            items.push(format!(">{}", number(*end)));
            items
        }
        ibis_doc_model::PivotFieldGrouping::Date { by, start, end, .. } => {
            let numbers: Vec<f64> = f.shared.iter().filter_map(|v| if let CellValue::Number(n) = v { Some(*n) } else { None }).collect();
            let (min_s, max_s) = if numbers.is_empty() {
                (46023.0, 46387.0)
            } else {
                numbers.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &n| (a.min(n), b.max(n)))
            };
            let s_val = start.unwrap_or(min_s);
            let e_val = end.unwrap_or(max_s);
            let (sy, sm, sd) = ibis_calc::eval::convert::date_from_serial(s_val.trunc() as i64, false);
            let (ey, em, ed) = ibis_calc::eval::convert::date_from_serial(e_val.trunc() as i64, false);
            let mut items = Vec::new();
            items.push(format!("<{sy:04}/{sm:02}/{sd:02}"));
            match by {
                ibis_doc_model::DateGroupBy::Months => {
                    for m in 1..=12 {
                        items.push(format!("{m}月"));
                    }
                }
                ibis_doc_model::DateGroupBy::Quarters => {
                    for q in 1..=4 {
                        items.push(format!("第{q}四半期"));
                    }
                }
                ibis_doc_model::DateGroupBy::Years => {
                    let y0 = sy.min(ey);
                    let y1 = sy.max(ey).min(y0 + 100);
                    for y in y0..=y1 {
                        items.push(format!("{y}年"));
                    }
                }
                ibis_doc_model::DateGroupBy::Days => {
                    let month_days = [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
                    for (m_idx, &days) in month_days.iter().enumerate() {
                        for d in 1..=days {
                            items.push(format!("{}月{}日", m_idx + 1, d));
                        }
                    }
                }
            }
            items.push(format!(">{ey:04}/{em:02}/{ed:02}"));
            items
        }
    }
}

/// The cache's definition: its source and each field's items (the ones
/// the table's items point at), no records, built again on opening.
fn cache_xml(p: &PivotTable, records_rid: &str) -> String {
    let mut out = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><pivotCacheDefinition xmlns="{MAIN_NS}" xmlns:r="{REL_NS}" r:id="{records_rid}" refreshOnLoad="1" createdVersion="6" refreshedVersion="6" minRefreshableVersion="3" recordCount="0"><cacheSource type="worksheet">"#);
    out += "<worksheetSource";
    if let Some(n) = &p.source.name {
        out += &format!(r#" name="{}""#, esc(n));
    } else {
        if let Some(r) = p.source.range {
            out += &format!(r#" ref="{}""#, range(r));
        }
        if let Some(s) = &p.source.sheet {
            out += &format!(r#" sheet="{}""#, esc(s));
        }
    }
    out += &format!(r#"/></cacheSource><cacheFields count="{}">"#, p.fields.len());
    for (i, f) in p.fields.iter().enumerate() {
        let calc_items: Vec<&ibis_doc_model::pivot::PivotCalculatedItem> = p.calculated_items.iter().filter(|ci| ci.field as usize == i).collect();
        let formula_attr = f.formula.as_ref().map(|fml| format!(r#" formula="{}""#, esc(fml.trim_start_matches('=').trim()))).unwrap_or_default();
        out += &format!(r#"<cacheField name="{}"{formula_attr} numFmtId="0">"#, esc(&f.name));
        let is_date = matches!(f.grouping, Some(ibis_doc_model::PivotFieldGrouping::Date { .. }));
        let numbers: Vec<f64> = f.shared.iter().filter_map(|v| if let CellValue::Number(n) = v { Some(*n) } else { None }).collect();
        let texts = !calc_items.is_empty() || f.shared.iter().any(|v| matches!(v, CellValue::Text(_) | CellValue::Bool(_) | CellValue::Error(_)));
        let blank = f.shared.iter().any(|v| matches!(v, CellValue::Empty));
        let mut attrs = String::new();
        if is_date && !numbers.is_empty() {
            attrs += r#" containsSemiMixedTypes="0" containsNonDate="0" containsDate="1" containsString="0""#;
            let (lo, hi) = numbers.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &n| (a.min(n), b.max(n)));
            attrs += &format!(r#" minDate="{}" maxDate="{}""#, serial_to_iso_date(lo), serial_to_iso_date(hi));
        } else if !numbers.is_empty() {
            if !texts {
                attrs += r#" containsSemiMixedTypes="0" containsString="0""#;
            } else {
                attrs += r#" containsMixedTypes="1""#;
            }
            let all_int = numbers.iter().all(|n| n.fract() == 0.0);
            attrs += r#" containsNumber="1""#;
            if all_int {
                attrs += r#" containsInteger="1""#;
            }
            let (lo, hi) = numbers.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &n| (a.min(n), b.max(n)));
            attrs += &format!(r#" minValue="{}" maxValue="{}""#, number(lo), number(hi));
        }
        if blank {
            attrs += r#" containsBlank="1""#;
        }
        out += &format!(r#"<sharedItems{attrs} count="{}">"#, f.shared.len() + calc_items.len());
        for v in &f.shared {
            out += &match v {
                CellValue::Text(t) => format!(r#"<s v="{}"/>"#, esc(t)),
                CellValue::Number(n) if is_date => format!(r#"<d v="{}"/>"#, serial_to_iso_date(*n)),
                CellValue::Number(n) => format!(r#"<n v="{}"/>"#, number(*n)),
                CellValue::Bool(b) => format!(r#"<b v="{}"/>"#, u8::from(*b)),
                CellValue::Error(e) => format!(r#"<e v="{}"/>"#, esc(e.as_str())),
                CellValue::Empty => "<m/>".to_string(),
            };
        }
        for ci in &calc_items {
            out += &format!(r#"<s v="{}" f="1"/>"#, esc(&ci.name));
        }
        out += "</sharedItems>";
        if let Some(g) = &f.grouping {
            out += &format!(r#"<fieldGroup base="{i}">"#);
            match g {
                ibis_doc_model::PivotFieldGrouping::Number { start, end, interval, auto_start, auto_end } => {
                    let mut rp = format!(r#"<rangePr groupBy="range" startNum="{}" endNum="{}" groupInterval="{}""#, number(*start), number(*end), number(*interval));
                    if !auto_start {
                        rp += r#" autoStart="0""#;
                    }
                    if !auto_end {
                        rp += r#" autoEnd="0""#;
                    }
                    rp += "/>";
                    out += &rp;
                    let group_items = range_group_items(f, g);
                    out += &format!(r#"<groupItems count="{}">"#, group_items.len());
                    for grp in &group_items {
                        out += &format!(r#"<s v="{}"/>"#, esc(grp));
                    }
                    out += "</groupItems>";
                }
                ibis_doc_model::PivotFieldGrouping::Date { by, start, end, auto_start, auto_end } => {
                    let (min_s, max_s) = if numbers.is_empty() {
                        (46023.0, 46387.0)
                    } else {
                        numbers.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &n| (a.min(n), b.max(n)))
                    };
                    let s_val = start.unwrap_or(min_s);
                    let e_val = end.unwrap_or(max_s);
                    let mut rp = format!(r#"<rangePr groupBy="{}" startDate="{}" endDate="{}""#, by.as_str(), serial_to_iso_date(s_val), serial_to_iso_date(e_val));
                    if !auto_start {
                        rp += r#" autoStart="0""#;
                    }
                    if !auto_end {
                        rp += r#" autoEnd="0""#;
                    }
                    rp += "/>";
                    out += &rp;
                    let group_items = range_group_items(f, g);
                    out += &format!(r#"<groupItems count="{}">"#, group_items.len());
                    for grp in &group_items {
                        out += &format!(r#"<s v="{}"/>"#, esc(grp));
                    }
                    out += "</groupItems>";
                }
                ibis_doc_model::PivotFieldGrouping::Manual { groups } => {
                    let (group_items, item_to_group_idx) = manual_group_items(f, groups);
                    out += &format!(r#"<discretePr count="{}">"#, item_to_group_idx.len());
                    for idx in item_to_group_idx {
                        out += &format!(r#"<x v="{idx}"/>"#);
                    }
                    out += "</discretePr>";
                    out += &format!(r#"<groupItems count="{}">"#, group_items.len());
                    for grp in &group_items {
                        out += &format!(r#"<s v="{}"/>"#, esc(grp));
                    }
                    out += "</groupItems>";
                }
            }
            out += "</fieldGroup>";
        }
        out += "</cacheField>";
    }
    out += "</cacheFields>";
    if !p.calculated_items.is_empty() {
        out += &format!(r#"<calculatedItems count="{}">"#, p.calculated_items.len());
        let mut field_calc_offset: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
        for ci in &p.calculated_items {
            let base_len = p.fields.get(ci.field as usize).map_or(0, |f| f.shared.len());
            let offset = field_calc_offset.entry(ci.field).or_insert(0);
            let item_x = base_len + *offset;
            *offset += 1;
            let clean = ci.formula.trim().trim_start_matches('=').trim();
            out += &format!(
                r#"<calculatedItem field="{f}" formula="{fml}"><pivotArea field="{f}" dataOnly="0" outline="0"><references count="1"><reference field="{f}" count="1"><x v="{item_x}"/></reference></references></pivotArea></calculatedItem>"#,
                f = ci.field,
                fml = esc(clean),
            );
        }
        out += "</calculatedItems>";
    }
    out += "</pivotCacheDefinition>";
    out
}

const RECORDS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><pivotCacheRecords xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" count="0"/>"#;

/// The table's definition, without its lines (`rowItems`, `colItems`),
/// which Excel builds on opening.
fn table_xml(p: &PivotTable, cache_id: u32) -> String {
    let auto_fit = if p.auto_fit_columns { "1" } else { "0" };
    let use_auto_fmt = if p.preserve_formatting { "1" } else { "0" };
    let merge_item_attr = if p.merge_item { r#" mergeItem="1""# } else { "" };
    let multiple_filters = if p.multiple_field_filters { "1" } else { "0" };
    let mut out = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><pivotTableDefinition xmlns="{MAIN_NS}" name="{}" cacheId="{cache_id}" applyNumberFormats="0" applyBorderFormats="0" applyFontFormats="0" applyPatternFormats="0" applyAlignmentFormats="0" applyWidthHeightFormats="{auto_fit}" dataCaption="{}" updatedVersion="6" minRefreshableVersion="3" useAutoFormatting="{use_auto_fmt}" itemPrintTitles="1" createdVersion="6" indent="0"{merge_item_attr}"#,
        esc(&p.name),
        esc(p.data_caption.as_deref().unwrap_or("値")),
    );
    if !p.row_grand_totals {
        out += r#" rowGrandTotals="0""#;
    }
    if !p.col_grand_totals {
        out += r#" colGrandTotals="0""#;
    }
    if !p.compact {
        out += r#" compact="0" compactData="0""#;
    }
    if p.outline {
        out += r#" outline="1" outlineData="1""#;
    }
    if !p.show_drill {
        out += r#" showDrill="0""#;
    }
    for (name, v) in [("rowHeaderCaption", &p.row_header_caption), ("colHeaderCaption", &p.col_header_caption), ("grandTotalCaption", &p.grand_total_caption)] {
        if let Some(v) = v {
            out += &format!(r#" {name}="{}""#, esc(v));
        }
    }
    if let Some(c) = &p.missing_caption {
        out += &format!(r#" showMissing="1" missingCaption="{}""#, esc(c));
    } else if p.missing_zero {
        out += r#" showMissing="0""#;
    }
    if let Some(c) = &p.error_caption {
        out += &format!(r#" showError="1" errorCaption="{}""#, esc(c));
    }
    out += &format!(r#" multipleFieldFilters="{multiple_filters}">"#);
    let pages = p.page_fields.len();
    out += &format!(r#"<location ref="{}" firstHeaderRow="1" firstDataRow="{}" firstDataCol="{}""#, range(p.location), p.first_data_row.max(1), p.first_data_col.max(1));
    if pages > 0 {
        out += &format!(r#" rowPageCount="{pages}" colPageCount="1""#);
    }
    out += "/>";
    out += &format!(r#"<pivotFields count="{}">"#, p.fields.len());
    for (i, f) in p.fields.iter().enumerate() {
        out += "<pivotField";
        if let Some(n) = p.aliases.field(i) {
            out += &format!(r#" name="{}""#, esc(n));
        }
        let axis = match p.place_of(i).map(|(a, _)| a).filter(|a| *a != PivotAxis::Values) {
            Some(PivotAxis::Row) => Some("axisRow"),
            Some(PivotAxis::Column) => Some("axisCol"),
            Some(PivotAxis::Page) => Some("axisPage"),
            _ => None,
        };
        if let Some(a) = axis {
            out += &format!(r#" axis="{a}""#);
        }
        if p.data_fields.iter().any(|d| d.field == i as u32) {
            out += r#" dataField="1""#;
        }
        if !f.compact {
            out += r#" compact="0""#;
        }
        if !f.outline {
            out += r#" outline="0""#;
        }
        out += r#" showAll="0""#;
        if !f.subtotal_top {
            out += r#" subtotalTop="0""#;
        }
        if f.insert_blank_row {
            out += r#" insertBlankRow="1""#;
        }
        match f.sort {
            PivotSort::Ascending => {
                out += r#" sortType="ascending""#;
                if let Some(df) = f.sort_data_field {
                    out += &format!(r#" dataField="{df}""#);
                }
            }
            PivotSort::Descending => {
                out += r#" sortType="descending""#;
                if let Some(df) = f.sort_data_field {
                    out += &format!(r#" dataField="{df}""#);
                }
            }
            PivotSort::Manual => {}
        }
        if !f.default_subtotal {
            out += r#" defaultSubtotal="0""#;
        }
        let fill_down_ext = match f.fill_down_labels {
            Some(true) => Some(FILL_DOWN_FIELD),
            Some(false) => Some(FILL_DOWN_FIELD_OFF),
            None if axis == Some("axisRow") && p.fill_down_labels => Some(FILL_DOWN_FIELD),
            None => None,
        };
        let calc_count = p.calculated_items.iter().filter(|ci| ci.field as usize == i).count();
        if axis.is_some() && (!f.items.is_empty() || calc_count > 0) {
            if let Some(g) = &f.grouping {
                let group_items = range_group_items(f, g);
                let default_subtotal = f.default_subtotal;
                let count = group_items.len() + if default_subtotal { 1 } else { 0 };
                out += &format!(r#"><items count="{count}">"#);
                for idx in 0..group_items.len() {
                    let sd = if f.items.get(idx).is_some_and(|it| it.collapsed) { r#" sd="0""# } else { "" };
                    out += &format!(r#"<item{sd} x="{idx}"/>"#);
                }
                if default_subtotal {
                    out += r#"<item t="default"/>"#;
                }
                out += "</items>";
            } else {
                let data_items: Vec<_> = f.items.iter().filter(|it| it.kind.is_none()).collect();
                let subtotal_items: Vec<_> = if f.default_subtotal {
                    f.items.iter().filter(|it| it.kind.is_some()).collect()
                } else {
                    Vec::new()
                };
                let count = data_items.len() + calc_count + subtotal_items.len();
                out += &format!(r#"><items count="{count}">"#);
                for it in data_items {
                    if let Some(x) = it.x {
                        let alias = f.shared.get(x as usize).and_then(|v| p.aliases.item(i, &ibis_doc_model::pivot_item_key(v)));
                        let n_attr = alias.map(|n| format!(r#" n="{}""#, esc(n))).unwrap_or_default();
                        out += &format!(r#"<item{n_attr}{} x="{x}"{}/>"#, if it.collapsed { r#" sd="0""# } else { "" }, if it.hidden { r#" h="1""# } else { "" });
                    }
                }
                for k in 0..calc_count {
                    let x = f.shared.len() + k;
                    out += &format!(r#"<item x="{x}"/>"#);
                }
                for it in subtotal_items {
                    if let Some(k) = &it.kind {
                        out += &format!(r#"<item t="{}"/>"#, esc(k));
                    }
                }
                out += "</items>";
            }
            if let Some(ext) = fill_down_ext {
                out += ext;
            }
            out += "</pivotField>";
        } else if let Some(ext) = fill_down_ext {
            out += ">";
            out += ext;
            out += "</pivotField>";
        } else {
            out += "/>";
        }
    }
    out += "</pivotFields>";
    let fields = |name: &str, list: &[i32]| if list.is_empty() { String::new() } else { format!(r#"<{name} count="{}">{}</{name}>"#, list.len(), list.iter().map(|x| format!(r#"<field x="{x}"/>"#)).collect::<String>()) };
    out += &fields("rowFields", &p.row_fields);
    out += &fields("colFields", &p.col_fields);
    if pages > 0 {
        out += &format!(r#"<pageFields count="{pages}">"#);
        for pf in &p.page_fields {
            out += &format!(r#"<pageField fld="{}"{} hier="-1"/>"#, pf.field, pf.item.map(|i| format!(r#" item="{i}""#)).unwrap_or_default());
        }
        out += "</pageFields>";
    }
    if !p.data_fields.is_empty() {
        out += &format!(r#"<dataFields count="{}">"#, p.data_fields.len());
        for d in &p.data_fields {
            out += &format!(r#"<dataField name="{}" fld="{}""#, esc(&d.name), d.field);
            if d.function != "sum" {
                out += &format!(r#" subtotal="{}""#, esc(&d.function));
            }
            out += r#" baseField="0" baseItem="0""#;
            if let Some(show_as) = &d.show_as {
                out += &format!(r#" showDataAs="{}""#, esc(show_as));
            }
            let num_fmt_id = d.number_format.as_deref().and_then(crate::xlsx::builtin_format_id)
                .or_else(|| if d.show_as.as_deref().is_some_and(|s| s.starts_with("percent")) { Some(10) } else { None });
            if let Some(id) = num_fmt_id {
                out += &format!(r#" numFmtId="{id}""#);
            }
            out += "/>";
        }
        out += "</dataFields>";
    }
    if let Some(s) = &p.style {
        let b = |v: bool| u8::from(v);
        out += &format!(r#"<pivotTableStyleInfo name="{}" showRowHeaders="{}" showColHeaders="{}" showRowStripes="{}" showColStripes="{}" showLastColumn="{}"/>"#, esc(&s.name), b(s.row_headers), b(s.col_headers), b(s.row_stripes), b(s.col_stripes), b(s.last_column));
    }
    if !p.filters.is_empty() {
        out += &format!(r#"<filters count="{}">"#, p.filters.len());
        for (i, f) in p.filters.iter().enumerate() {
            out += &format!(r#"<filter fld="{}" type="{}" evalOrder="-1" id="{}""#, f.field, esc(&f.kind), i + 1);
            if !f.is_label() {
                out += &format!(r#" iMeasureFld="{}""#, f.measure);
            }
            out += r#"><autoFilter ref="A1"><filterColumn colId="0">"#;
            let custom = |op: Option<&str>, val: &str| format!(r#"<customFilter{} val="{}"/>"#, op.map(|o| format!(r#" operator="{o}""#)).unwrap_or_default(), esc(val));
            let v = f.value.as_str();
            let wild = |before: &str, after: &str| format!("{before}{v}{after}");
            let two = f.value2.as_deref().unwrap_or("");
            let body = match f.kind.as_str() {
                "captionEqual" | "valueEqual" => format!("<customFilters>{}</customFilters>", custom(None, v)),
                "captionNotEqual" | "valueNotEqual" => format!("<customFilters>{}</customFilters>", custom(Some("notEqual"), v)),
                "captionBeginsWith" => format!("<customFilters>{}</customFilters>", custom(None, &wild("", "*"))),
                "captionNotBeginsWith" => format!("<customFilters>{}</customFilters>", custom(Some("notEqual"), &wild("", "*"))),
                "captionEndsWith" => format!("<customFilters>{}</customFilters>", custom(None, &wild("*", ""))),
                "captionNotEndsWith" => format!("<customFilters>{}</customFilters>", custom(Some("notEqual"), &wild("*", ""))),
                "captionContains" => format!("<customFilters>{}</customFilters>", custom(None, &wild("*", "*"))),
                "captionNotContains" => format!("<customFilters>{}</customFilters>", custom(Some("notEqual"), &wild("*", "*"))),
                "captionGreaterThan" | "valueGreaterThan" => format!("<customFilters>{}</customFilters>", custom(Some("greaterThan"), v)),
                "captionGreaterThanOrEqual" | "valueGreaterThanOrEqual" => format!("<customFilters>{}</customFilters>", custom(Some("greaterThanOrEqual"), v)),
                "captionLessThan" | "valueLessThan" => format!("<customFilters>{}</customFilters>", custom(Some("lessThan"), v)),
                "captionLessThanOrEqual" | "valueLessThanOrEqual" => format!("<customFilters>{}</customFilters>", custom(Some("lessThanOrEqual"), v)),
                "captionBetween" | "valueBetween" => format!(r#"<customFilters and="1">{}{}</customFilters>"#, custom(Some("greaterThanOrEqual"), v), custom(Some("lessThanOrEqual"), two)),
                "captionNotBetween" | "valueNotBetween" => format!("<customFilters>{}{}</customFilters>", custom(Some("lessThan"), v), custom(Some("greaterThan"), two)),
                "count" | "percent" | "sum" => format!(r#"<top10{}{} val="{}" filterVal="{}"/>"#, if f.top { "" } else { r#" top="0""# }, if f.kind == "percent" { r#" percent="1""# } else { "" }, esc(v), esc(v)),
                _ => String::new(),
            };
            out += &body;
            out += "</filterColumn></autoFilter></filter>";
        }
        out += "</filters>";
    }
    if p.fill_down_labels {
        out += r#"<extLst><ext uri="{962EF5D1-5CA2-4c93-8EF4-DBF39C750712}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:pivotTableDefinition fillDownLabelsDefault="1"/></ext></extLst>"#;
    }
    out += "</pivotTableDefinition>";
    out
}

/// A row field's own 項目ラベルを繰り返す.
const FILL_DOWN_FIELD: &str = r#"<extLst><ext uri="{2946ED86-A175-432a-8AC1-64E0C546D7DE}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:pivotField fillDownLabels="1"/></ext></extLst>"#;
const FILL_DOWN_FIELD_OFF: &str = r#"<extLst><ext uri="{2946ED86-A175-432a-8AC1-64E0C546D7DE}" xmlns:x14="http://schemas.microsoft.com/office/spreadsheetml/2009/9/main"><x14:pivotField fillDownLabels="0"/></ext></extLst>"#;

/// The path from `from`'s folder to `to` (both part names).
fn relative(from: &str, to: &str) -> String {
    let from_dir: Vec<&str> = from.rsplit_once('/').map_or(Vec::new(), |(d, _)| d.split('/').collect());
    let to_parts: Vec<&str> = to.split('/').collect();
    let common = from_dir.iter().zip(&to_parts).take_while(|(a, b)| a == b).count();
    let mut out: Vec<&str> = vec![".."; from_dir.len() - common];
    out.extend(&to_parts[common..]);
    out.join("/")
}

fn free_name(package: &Package, pattern: &str) -> (String, u32) {
    let mut n = 1;
    while package.contains(&pattern.replace("{}", &n.to_string())) {
        n += 1;
    }
    (pattern.replace("{}", &n.to_string()), n)
}

/// The workbook's cache ids, and a new one.
fn new_cache_id(package: &mut Package, workbook: &str) -> Result<u32, Error> {
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    let max = doc
        .child(root, "pivotCaches")
        .map(|c| doc.children_named(c, "pivotCache").filter_map(|e| doc.attr(e, "cacheId").and_then(|v| v.parse::<u32>().ok())).max().unwrap_or(0))
        .unwrap_or(0);
    Ok(max + 1)
}

/// The workbook's `pivotCache` for the cache `rid`, in its place.
fn add_workbook_cache(package: &mut Package, workbook: &str, cache_id: u32, rid: &str) -> Result<(), Error> {
    let doc = package.xml_mut(workbook)?;
    let root = doc.root_element().ok_or_else(|| Error::new("ブックの XML が空です"))?;
    let prefix = doc.prefix(root).map(|p| format!("{p}:")).unwrap_or_default();
    let r = doc.lookup_prefix(root, REL_NS).unwrap_or_else(|| "r".into());
    let list = match doc.child(root, "pivotCaches") {
        Some(l) => l,
        None => {
            let l = doc.insert_element(ibis_ooxml::XmlDocument::ROOT, 0, &format!("{prefix}pivotCaches"));
            doc.detach(l);
            let after = ["smartTagPr", "smartTagTypes", "webPublishing", "fileRecoveryPr", "webPublishObjects", "extLst"];
            let anchor = doc.elements(root).find(|&c| after.contains(&doc.local_name(c)));
            match anchor {
                Some(a) => doc.insert_before(a, l),
                None => {
                    let n = doc.children(root).len();
                    doc.insert(root, n, l);
                }
            }
            l
        }
    };
    if doc.lookup_prefix(root, REL_NS).is_none() {
        doc.set_attr(root, "xmlns:r", REL_NS);
    }
    let e = doc.append_element(list, &format!("{prefix}pivotCache"));
    doc.set_attr(e, "cacheId", &cache_id.to_string());
    doc.set_attr(e, &format!("{r}:id"), rid);
    Ok(())
}

/// The sheet's pivot tables as the model has them now: built again (its
/// place and `refreshOnLoad`), laid out anew (definitions written again),
/// or new (parts added).
pub(crate) fn write_pivots(package: &mut Package, sheet_part: &str, workbook: &str, old: &[PivotTable], new: &[PivotTable]) -> Result<(), Error> {
    if old.iter().any(|o| o.part.is_some() && !new.iter().any(|n| n.part == o.part)) {
        return Err(unsupported("ピボットテーブルの削除"));
    }
    for n in new {
        let before = n.part.as_ref().and_then(|part| old.iter().find(|o| o.part.as_ref() == Some(part)));
        if before == Some(n) {
            continue;
        }
        match (before, &n.part, &n.cache_part) {
            // Built again only: its place, and its cache built again on opening.
            (Some(o), Some(table), Some(cache)) if layout(o) == layout(n) => {
                let doc = package.xml_mut(table)?;
                if let Some(root) = doc.root_element()
                    && let Some(loc) = doc.child(root, "location")
                {
                    doc.set_attr(loc, "ref", &range(n.location));
                    doc.set_attr(loc, "firstHeaderRow", &n.first_header_row.to_string());
                    doc.set_attr(loc, "firstDataRow", &n.first_data_row.to_string());
                    doc.set_attr(loc, "firstDataCol", &n.first_data_col.to_string());
                }
                let doc = package.xml_mut(cache)?;
                if let Some(root) = doc.root_element() {
                    doc.set_attr(root, "refreshOnLoad", "1");
                }
            }
            // Laid out anew: the table's definition written again. The cache's
            // is kept (its records too) when nothing it holds changed (a filter
            // changed which items are shown, say), else written again with the
            // extensions it had (the slicers find the cache by the number in
            // `x14:pivotCacheDefinition`) and its records emptied.
            (Some(o), Some(table), Some(cache)) => {
                let cache_id = {
                    let doc = package.xml_mut(table)?;
                    doc.root_element().and_then(|r| doc.attr(r, "cacheId")).and_then(|v| v.parse::<u32>().ok()).unwrap_or(1)
                };
                package.set_bytes(table, table_xml(n, cache_id).into_bytes());
                let key = |p: &PivotTable| (p.source.clone(), p.fields.iter().map(|f| (f.name.clone(), f.formula.clone(), f.shared.clone(), f.grouping.clone())).collect::<Vec<_>>(), p.calculated_items.clone());
                // Items only added to the fields' shared items, or a field placed in the table whose cache had no shared items yet.
                let appended = o.source == n.source
                    && o.calculated_items == n.calculated_items
                    && o.fields.len() == n.fields.len()
                    && o.fields.iter().zip(&n.fields).all(|(a, b)| a.name == b.name && a.formula == b.formula && a.grouping == b.grouping && b.shared.starts_with(&a.shared));
                if key(o) == key(n) || appended {
                    let mut emptied_records = false;
                    let doc = package.xml_mut(cache)?;
                    if let Some(root) = doc.root_element() {
                        doc.set_attr(root, "refreshOnLoad", "1");
                        if let Some(fields) = doc.child(root, "cacheFields") {
                            let nodes: Vec<_> = doc.children_named(fields, "cacheField").collect();
                            for (i, (node, f)) in nodes.into_iter().zip(&n.fields).enumerate() {
                                // A group field of the file keeps its group items (not shared items of the source).
                                if f.group_base.is_some() {
                                    continue;
                                }
                                let Some(shared) = doc.child(node, "sharedItems") else { continue };
                                let doc_count = doc.children(shared).iter().filter(|&&c| doc.is_element(c)).count();
                                let axis = n.place_of(i).map(|(a, _)| a);
                                let placed_on_axis = matches!(axis, Some(PivotAxis::Row | PivotAxis::Column | PivotAxis::Page));
                                let needed = doc_count < f.shared.len() && (doc_count > 0 || placed_on_axis);
                                if !needed {
                                    continue;
                                }
                                if doc_count == 0 {
                                    emptied_records = true;
                                }
                                let is_date = doc.attr(shared, "containsDate") == Some("1") || doc.attr(shared, "minDate").is_some() || matches!(f.grouping, Some(ibis_doc_model::PivotFieldGrouping::Date { .. }));
                                let prefix = doc.prefix(shared).map(|p| format!("{p}:")).unwrap_or_default();
                                if is_date {
                                    for name in doc.attributes(shared).iter().map(|a| a.name.clone()).filter(|a| a == "minValue" || a == "maxValue" || a.starts_with("containsNumber") || a.starts_with("containsInteger")).collect::<Vec<_>>() {
                                        doc.remove_attr(shared, &name);
                                    }
                                    doc.set_attr(shared, "containsDate", "1");
                                    doc.set_attr(shared, "containsNonDate", "0");
                                    doc.set_attr(shared, "containsString", "0");
                                    doc.set_attr(shared, "containsSemiMixedTypes", "0");
                                } else {
                                    for name in doc.attributes(shared).iter().map(|a| a.name.clone()).filter(|a| a.starts_with("contains") || a == "minValue" || a == "maxValue").collect::<Vec<_>>() {
                                        doc.remove_attr(shared, &name);
                                    }
                                }
                                for v in &f.shared[doc_count..] {
                                    let (tag, value) = match v {
                                        CellValue::Text(t) => ("s", Some(t.clone())),
                                        CellValue::Number(x) if is_date => ("d", Some(serial_to_iso_date(*x))),
                                        CellValue::Number(x) => ("n", Some(number(*x))),
                                        CellValue::Bool(x) => ("b", Some(u8::from(*x).to_string())),
                                        CellValue::Error(e) => ("e", Some(e.as_str().to_string())),
                                        CellValue::Empty => ("m", None),
                                    };
                                    let count = doc.children(shared).len();
                                    let item = doc.insert_element(shared, count, &format!("{prefix}{tag}"));
                                    if let Some(value) = value {
                                        doc.set_attr(item, "v", &value);
                                    }
                                }
                                doc.set_attr(shared, "count", &f.shared.len().to_string());
                            }
                        }
                        if emptied_records {
                            doc.set_attr(root, "recordCount", "0");
                        }
                    }
                    if emptied_records {
                        let records = package.relationships(cache)?.into_iter().find(|r| r.kind == REL_RECORDS && !r.external);
                        if let Some(r) = &records {
                            if let Some(part) = r.target_part(cache) {
                                package.set_bytes(&part, RECORDS_XML.as_bytes().to_vec());
                            }
                        }
                    }
                    continue;
                }
                let records = package.relationships(cache)?.into_iter().find(|r| r.kind == REL_RECORDS && !r.external);
                let rid = match &records {
                    Some(r) => {
                        if let Some(part) = r.target_part(cache) {
                            package.set_bytes(&part, RECORDS_XML.as_bytes().to_vec());
                        }
                        r.id.clone()
                    }
                    None => {
                        let (part, _) = free_name(package, "xl/pivotCache/pivotCacheRecords{}.xml");
                        package.set_bytes(&part, RECORDS_XML.as_bytes().to_vec());
                        package.set_override(&part, CT_RECORDS)?;
                        package.add_relationship(cache, REL_RECORDS, &relative(cache, &part), false)?
                    }
                };
                let old = package.xml(cache)?;
                let mut fresh = ibis_ooxml::XmlDocument::parse(cache_xml(n, &rid).as_bytes()).map_err(|e| Error::new(format!("ピボットのキャッシュを作れません: {e}")))?;
                if let (Some(old_root), Some(root)) = (old.root_element(), fresh.root_element()) {
                    for a in old.attributes(old_root).iter().filter(|a| a.name.starts_with("xmlns:")) {
                        if fresh.attr(root, &a.name).is_none() {
                            fresh.set_attr(root, &a.name, &a.value);
                        }
                    }
                    // A field whose values the records hold (no shared items of its own): its attributes as they were.
                    if let (Some(old_fields), Some(new_fields)) = (old.child(old_root, "cacheFields"), fresh.child(root, "cacheFields")) {
                        let old_nodes: Vec<_> = old.children_named(old_fields, "cacheField").collect();
                        let new_nodes: Vec<_> = fresh.children_named(new_fields, "cacheField").collect();
                        for (i, nf) in new_nodes.into_iter().enumerate() {
                            let name = fresh.attr(nf, "name").map(str::to_string);
                            let Some(of) = old_nodes.iter().copied().find(|&f| old.attr(f, "name") == name.as_deref()) else { continue };
                            let (Some(os), Some(ns)) = (old.child(of, "sharedItems"), fresh.child(nf, "sharedItems")) else { continue };
                            if old.children(os).iter().any(|&c| old.is_element(c)) || n.place_of(i).is_some() {
                                continue;
                            }
                            let at = fresh.children(nf).iter().position(|&c| c == ns).unwrap_or(0);
                            fresh.detach(ns);
                            let copy = fresh.import(&old, os);
                            fresh.insert(nf, at, copy);
                        }
                    }
                    if let Some(ext) = old.child(old_root, "extLst") {
                        let copy = fresh.import(&old, ext);
                        let at = fresh.children(root).len();
                        fresh.insert(root, at, copy);
                    }
                }
                package.set_xml(cache, fresh);
            }
            // New: its parts, their relationships, and the workbook's cache.
            (None, None, _) => {
                let (cache, k) = free_name(package, "xl/pivotCache/pivotCacheDefinition{}.xml");
                let records = format!("xl/pivotCache/pivotCacheRecords{k}.xml");
                let (table, _) = free_name(package, "xl/pivotTables/pivotTable{}.xml");
                let cache_id = new_cache_id(package, workbook)?;
                package.set_bytes(&records, RECORDS_XML.as_bytes().to_vec());
                package.set_override(&records, CT_RECORDS)?;
                // The cache's part is written before its relationship to the records.
                package.set_bytes(&cache, b"<pivotCacheDefinition/>".to_vec());
                package.set_override(&cache, CT_CACHE)?;
                let rid = package.add_relationship(&cache, REL_RECORDS, &relative(&cache, &records), false)?;
                package.set_bytes(&cache, cache_xml(n, &rid).into_bytes());
                package.set_bytes(&table, table_xml(n, cache_id).into_bytes());
                package.set_override(&table, CT_PIVOT_TABLE)?;
                package.add_relationship(&table, REL_CACHE, &relative(&table, &cache), false)?;
                package.add_relationship(sheet_part, REL_PIVOT_TABLE, &relative(sheet_part, &table), false)?;
                let wb_rid = package.add_relationship(workbook, REL_CACHE, &relative(workbook, &cache), false)?;
                add_workbook_cache(package, workbook, cache_id, &wb_rid)?;
            }
            _ => return Err(unsupported("ピボットテーブルの変更")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::relative;

    #[test]
    fn relative_paths() {
        assert_eq!(relative("xl/worksheets/sheet2.xml", "xl/pivotTables/pivotTable1.xml"), "../pivotTables/pivotTable1.xml");
        assert_eq!(relative("xl/workbook.xml", "xl/pivotCache/pivotCacheDefinition1.xml"), "pivotCache/pivotCacheDefinition1.xml");
        assert_eq!(relative("xl/pivotCache/pivotCacheDefinition1.xml", "xl/pivotCache/pivotCacheRecords1.xml"), "pivotCacheRecords1.xml");
    }
}
