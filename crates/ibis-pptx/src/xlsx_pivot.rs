// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reading a pivot table's definition (`xl/pivotTables/pivotTable*.xml`)
//! with its cache's definition (`pivotCacheDefinition*.xml`), ECMA-376
//! Part 1 §18.10.

use ibis_doc_model::{CellValue, PivotAxis, PivotDataField, PivotField, PivotFilter, PivotItem, PivotLine, PivotPageField, PivotSort, PivotSource, PivotStyleInfo, PivotTable};
use roxmltree::Node;

use crate::xml::{attr, attr_bool, attr_i64, child, children};

/// A shared item's value: a string, number, boolean, error, date (as its
/// serial, from 1900) or missing (`m`, empty).
fn shared_item(n: Node) -> CellValue {
    let v = attr(n, "v").unwrap_or("");
    match n.tag_name().name() {
        "s" => CellValue::Text(v.to_string()),
        "n" => v.parse().map_or(CellValue::Empty, CellValue::Number),
        "b" => CellValue::Bool(v == "1" || v == "true"),
        "e" => CellValue::Error(ibis_doc_model::CellError::parse(v).unwrap_or(ibis_doc_model::CellError::Value)),
        "d" => date_serial(v).map_or(CellValue::Text(v.to_string()), CellValue::Number),
        _ => CellValue::Empty,
    }
}

/// `2026-01-05T00:00:00` as the date's serial (1900 dates, 1 = 1900-01-01,
/// with Excel's 1900-02-29 counted).
pub(crate) fn date_serial(v: &str) -> Option<f64> {
    let (date, time) = v.split_once('T').unwrap_or((v, ""));
    let mut p = date.split('-').map(|s| s.parse::<i64>());
    let (y, m, d) = (p.next()?.ok()?, p.next()?.ok()?, p.next()?.ok()?);
    // Days from 1970-01-01 (Howard Hinnant's days_from_civil).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let mut serial = days as f64 + 25_569.0;
    if serial < 61.0 {
        serial -= 1.0;
    }
    let mut t = time.split(':').map(|s| s.parse::<f64>().unwrap_or(0.0));
    let (h, mi, s) = (t.next().unwrap_or(0.0), t.next().unwrap_or(0.0), t.next().unwrap_or(0.0));
    Some(serial + (h * 3600.0 + mi * 60.0 + s) / 86_400.0)
}

fn lines(n: Option<Node>) -> Vec<PivotLine> {
    n.into_iter()
        .flat_map(|n| children(n, "i"))
        .map(|i| PivotLine {
            kind: attr(i, "t").filter(|t| *t != "data").map(str::to_string),
            repeat: attr_i64(i, "r").unwrap_or(0).max(0) as u32,
            items: children(i, "x").map(|x| attr_i64(x, "v").unwrap_or(0).max(0) as u32).collect(),
            data: attr_i64(i, "i").unwrap_or(0).max(0) as u32,
        })
        .collect()
}

fn fields_of(n: Option<Node>) -> Vec<i32> {
    n.into_iter().flat_map(|n| children(n, "field")).filter_map(|f| attr_i64(f, "x")).map(|x| x as i32).collect()
}

/// The pivot table of `table` with the cache definition `cache` (and its
/// part names); `None` when it has no location.
pub(crate) fn read_pivot(table: Node, cache: Option<Node>, part: &str, cache_part: Option<&str>) -> Option<PivotTable> {
    let loc = child(table, "location")?;
    let location = attr(loc, "ref").and_then(crate::xlsx::parse_range)?;
    let flag = |n: Node, name: &str, default: bool| attr_bool(n, name).unwrap_or(default);
    let compact = flag(table, "compact", true);
    let outline = flag(table, "outline", false);
    let cache_fields: Vec<Node> = cache.and_then(|c| child(c, "cacheFields")).into_iter().flat_map(|f| children(f, "cacheField")).collect();
    let pivot_fields: Vec<Node> = child(table, "pivotFields").into_iter().flat_map(|f| children(f, "pivotField")).collect();
    let count = cache_fields.len().max(pivot_fields.len());
    let fields: Vec<PivotField> = (0..count)
        .map(|i| {
            let cf = cache_fields.get(i).copied();
            let pf = pivot_fields.get(i).copied();
            let axis = pf.and_then(|p| attr(p, "axis")).and_then(|a| match a {
                "axisRow" => Some(PivotAxis::Row),
                "axisCol" => Some(PivotAxis::Column),
                "axisPage" => Some(PivotAxis::Page),
                "axisValues" => Some(PivotAxis::Values),
                _ => None,
            });
            let grouping = cf.and_then(|c| child(c, "fieldGroup")).and_then(|fg| {
                if let Some(rp) = child(fg, "rangePr") {
                    let group_by = attr(rp, "groupBy").unwrap_or("range");
                    let auto_start = attr_bool(rp, "autoStart").unwrap_or(true);
                    let auto_end = attr_bool(rp, "autoEnd").unwrap_or(true);
                    let parse_f64 = |n: Node, name: &str| attr(n, name).and_then(|s| s.parse::<f64>().ok());
                    if group_by == "range" {
                        let start = parse_f64(rp, "startNum").unwrap_or(0.0);
                        let end = parse_f64(rp, "endNum").unwrap_or(0.0);
                        let interval = parse_f64(rp, "groupInterval").unwrap_or(1.0);
                        Some(ibis_doc_model::PivotFieldGrouping::Number {
                            start,
                            end,
                            interval,
                            auto_start,
                            auto_end,
                        })
                    } else if let Some(by) = ibis_doc_model::DateGroupBy::from_str(group_by) {
                        let start = parse_f64(rp, "startNum").or_else(|| attr(rp, "startDate").and_then(date_serial));
                        let end = parse_f64(rp, "endNum").or_else(|| attr(rp, "endDate").and_then(date_serial));
                        Some(ibis_doc_model::PivotFieldGrouping::Date {
                            by,
                            start,
                            end,
                            auto_start,
                            auto_end,
                        })
                    } else {
                        None
                    }
                } else if let Some(dp) = child(fg, "discretePr") {
                    let group_items: Vec<String> = child(fg, "groupItems")
                        .into_iter()
                        .flat_map(|gi| gi.children().filter(|n| n.is_element()))
                        .map(|it| match shared_item(it) {
                            CellValue::Text(t) => t,
                            CellValue::Number(n) => n.to_string(),
                            CellValue::Bool(b) => if b { "TRUE".into() } else { "FALSE".into() },
                            CellValue::Error(e) => e.as_str().to_string(),
                            CellValue::Empty => String::new(),
                        })
                        .collect();
                    let orig_items: Vec<String> = cf.and_then(|cf_node| child(cf_node, "sharedItems"))
                        .into_iter()
                        .flat_map(|si| si.children().filter(|n| n.is_element()))
                        .map(|it| match shared_item(it) {
                            CellValue::Text(t) => t,
                            CellValue::Number(n) => n.to_string(),
                            CellValue::Bool(b) => if b { "TRUE".into() } else { "FALSE".into() },
                            CellValue::Error(e) => e.as_str().to_string(),
                            CellValue::Empty => String::new(),
                        })
                        .collect();
                    let mappings: Vec<(String, String)> = orig_items.into_iter().zip(children(dp, "x"))
                        .filter_map(|(orig_name, x_node)| {
                            let v = attr(x_node, "v").and_then(|v| v.parse::<usize>().ok())?;
                            let group_name = group_items.get(v)?.clone();
                            Some((orig_name, group_name))
                        })
                        .collect();
                    let mut group_to_orig_count = std::collections::HashMap::new();
                    for (_, g) in &mappings {
                        *group_to_orig_count.entry(g.clone()).or_insert(0) += 1;
                    }
                    let mut groups = std::collections::HashMap::new();
                    for (orig_name, group_name) in mappings {
                        if orig_name != group_name || group_to_orig_count.get(&group_name).copied().unwrap_or(0) > 1 {
                            groups.insert(orig_name, group_name);
                        }
                    }
                    Some(ibis_doc_model::PivotFieldGrouping::Manual { groups })
                } else if let Some(di) = child(fg, "discreteItems") {
                    let mut groups = std::collections::HashMap::new();
                    for it in children(di, "item") {
                        if let (Some(name), Some(group)) = (attr(it, "name"), attr(it, "group")) {
                            groups.insert(name.to_string(), group.to_string());
                        }
                    }
                    Some(ibis_doc_model::PivotFieldGrouping::Manual { groups })
                } else {
                    None
                }
            });
            let formula = cf.and_then(|c| attr(c, "formula")).map(str::to_string);
            // A group field the file defines (年 (日付)): the cache field it groups, and its group items' names.
            let group_base = cf
                .filter(|c| attr(*c, "databaseField") == Some("0"))
                .and_then(|c| child(c, "fieldGroup"))
                .filter(|fg| child(*fg, "rangePr").is_some())
                .and_then(|fg| attr_i64(fg, "base"))
                .map(|b| b.max(0) as u32);
            let all_shared_nodes: Vec<Node> = cf
                .and_then(|c| child(c, "sharedItems"))
                .into_iter()
                .flat_map(|s| s.children().filter(|n| n.is_element()))
                .collect();
            let mut shared: Vec<CellValue> = all_shared_nodes
                .iter()
                .copied()
                .filter(|&n| !flag(n, "f", false))
                .map(shared_item)
                .collect();
            if group_base.is_some() && shared.is_empty() {
                shared = cf
                    .and_then(|c| child(c, "fieldGroup"))
                    .and_then(|fg| child(fg, "groupItems"))
                    .into_iter()
                    .flat_map(|g| g.children().filter(|n| n.is_element()))
                    .map(shared_item)
                    .collect();
            }
            PivotField {
                name: cf.and_then(|c| attr(c, "name")).unwrap_or("").to_string(),
                formula,
                axis,
                shared,
                items: pf
                    .and_then(|p| child(p, "items"))
                    .into_iter()
                    .flat_map(|it| children(it, "item"))
                    .filter(|&it| {
                        let Some(x) = attr_i64(it, "x").map(|x| x.max(0) as usize) else {
                            return true;
                        };
                        !all_shared_nodes.get(x).is_some_and(|&n| flag(n, "f", false))
                    })
                    .map(|it| PivotItem {
                        x: attr_i64(it, "x").map(|x| x.max(0) as u32),
                        hidden: flag(it, "h", false),
                        kind: attr(it, "t").filter(|t| *t != "data").map(str::to_string),
                        // `sd="0"`: 詳細の表示 off (the − button folded to +).
                        collapsed: !flag(it, "sd", true),
                    })
                    .collect(),
                compact: pf.map_or(compact, |p| flag(p, "compact", true)),
                outline: pf.map_or(outline, |p| flag(p, "outline", true)),
                subtotal_top: pf.map_or(true, |p| flag(p, "subtotalTop", true)),
                default_subtotal: pf.map_or(true, |p| flag(p, "defaultSubtotal", true)),
                sort: match pf.and_then(|p| attr(p, "sortType")) {
                    Some("ascending") => PivotSort::Ascending,
                    Some("descending") => PivotSort::Descending,
                    _ => PivotSort::Manual,
                },
                sort_data_field: pf.and_then(|p| attr(p, "dataField")).and_then(|s| s.parse().ok()),
                grouping,
                insert_blank_row: pf.map_or(false, |p| flag(p, "insertBlankRow", false)),
                fill_down_labels: pf.and_then(|p| {
                    attr_bool(p, "fillDownLabels").or_else(|| {
                        p.descendants().find(|n| n.tag_name().name() == "pivotField" && attr_bool(*n, "fillDownLabels").is_some()).and_then(|n| attr_bool(n, "fillDownLabels"))
                    })
                }),
                group_base,
            }
        })
        .collect();
    let source = cache.and_then(|c| child(c, "cacheSource")).and_then(|s| child(s, "worksheetSource")).map_or_else(PivotSource::default, |w| PivotSource {
        sheet: attr(w, "sheet").map(str::to_string),
        range: attr(w, "ref").and_then(crate::xlsx::parse_range),
        name: attr(w, "name").map(str::to_string),
    });
    let style = child(table, "pivotTableStyleInfo").map(|s| PivotStyleInfo {
        name: attr(s, "name").unwrap_or("").to_string(),
        row_headers: flag(s, "showRowHeaders", false),
        col_headers: flag(s, "showColHeaders", false),
        row_stripes: flag(s, "showRowStripes", false),
        col_stripes: flag(s, "showColStripes", false),
        last_column: flag(s, "showLastColumn", false),
    });
    let text = |name: &str| attr(table, name).map(str::to_string);
    let aliases = aliases_of(table, &fields);
    Some(PivotTable {
        name: attr(table, "name").unwrap_or("").to_string(),
        part: Some(part.to_string()),
        cache_part: cache_part.map(str::to_string),
        location,
        first_header_row: attr_i64(loc, "firstHeaderRow").unwrap_or(1).max(0) as u32,
        first_data_row: attr_i64(loc, "firstDataRow").unwrap_or(1).max(0) as u32,
        first_data_col: attr_i64(loc, "firstDataCol").unwrap_or(1).max(0) as u32,
        source,
        fields,
        row_fields: fields_of(child(table, "rowFields")),
        col_fields: fields_of(child(table, "colFields")),
        page_fields: child(table, "pageFields")
            .into_iter()
            .flat_map(|p| children(p, "pageField"))
            .filter_map(|p| Some(PivotPageField { field: attr_i64(p, "fld")?.max(0) as u32, item: attr_i64(p, "item").map(|i| i.max(0) as u32) }))
            .collect(),
        data_fields: child(table, "dataFields")
            .into_iter()
            .flat_map(|d| children(d, "dataField"))
            .map(|d| PivotDataField {
                name: attr(d, "name").unwrap_or("").to_string(),
                field: attr_i64(d, "fld").unwrap_or(0).max(0) as u32,
                function: attr(d, "subtotal").unwrap_or("sum").to_string(),
                show_as: attr(d, "showDataAs").filter(|s| *s != "normal").map(str::to_string),
                number_format: attr_i64(d, "numFmtId").and_then(crate::xlsx::builtin_format).map(str::to_string),
            })
            .collect(),
        row_items: lines(child(table, "rowItems")),
        col_items: lines(child(table, "colItems")),
        row_grand_totals: flag(table, "rowGrandTotals", true),
        col_grand_totals: flag(table, "colGrandTotals", true),
        compact,
        outline,
        row_header_caption: text("rowHeaderCaption"),
        col_header_caption: text("colHeaderCaption"),
        grand_total_caption: text("grandTotalCaption"),
        data_caption: text("dataCaption"),
        missing_caption: flag(table, "showMissing", true).then(|| text("missingCaption")).flatten(),
        missing_zero: !flag(table, "showMissing", true),
        filters: child(table, "filters")
            .into_iter()
            .flat_map(|f| children(f, "filter"))
            .filter_map(|f| {
                let kind = attr(f, "type")?.to_string();
                let column = child(f, "autoFilter").and_then(|a| child(a, "filterColumn"));
                let customs: Vec<Node> = column.and_then(|c| child(c, "customFilters")).into_iter().flat_map(|c| children(c, "customFilter")).collect();
                let top10 = column.and_then(|c| child(c, "top10"));
                // Begins / ends / contains are written with wildcards around the value.
                let strip = |v: &str| if kind.starts_with("caption") && (kind.contains("BeginsWith") || kind.contains("EndsWith") || kind.contains("Contains")) { v.trim_matches('*').to_string() } else { v.to_string() };
                let value = match top10 {
                    Some(t) => attr(t, "val").unwrap_or("10").to_string(),
                    None => customs.first().and_then(|c| attr(*c, "val")).map(&strip).unwrap_or_default(),
                };
                Some(PivotFilter {
                    field: attr_i64(f, "fld")?.max(0) as u32,
                    kind,
                    measure: attr_i64(f, "iMeasureFld").unwrap_or(0).max(0) as u32,
                    value,
                    value2: customs.get(1).and_then(|c| attr(*c, "val")).map(str::to_string),
                    top: top10.is_none_or(|t| flag(t, "top", true)),
                })
            })
            .collect(),
        error_caption: flag(table, "showError", false).then(|| text("errorCaption")).flatten(),
        fill_down_labels: table.descendants().any(|n| n.tag_name().name() == "pivotTableDefinition" && attr_bool(n, "fillDownLabelsDefault").unwrap_or(false)),
        style,
        data_on_rows: flag(table, "dataOnRows", false),
        refreshed: false,
        refresh_on_load: cache.is_some_and(|c| flag(c, "refreshOnLoad", false)),
        buttons: Vec::new(),
        corner: None,
        merge_item: flag(table, "mergeItem", false),
        auto_fit_columns: flag(table, "applyWidthHeightFormats", true),
        preserve_formatting: flag(table, "useAutoFormatting", true),
        multiple_field_filters: flag(table, "multipleFieldFilters", false),
        show_drill: flag(table, "showDrill", true),
        aliases,
        calculated_items: cache
            .and_then(|c| child(c, "calculatedItems"))
            .into_iter()
            .flat_map(|ci| children(ci, "calculatedItem"))
            .filter_map(|ci| {
                let field = attr_i64(ci, "field")?.max(0) as u32;
                let name = attr(ci, "name")
                    .map(str::to_string)
                    .or_else(|| {
                        let item_x = child(ci, "pivotArea")
                            .and_then(|pa| child(pa, "references"))
                            .into_iter()
                            .flat_map(|refs| children(refs, "reference"))
                            .find(|r| attr_i64(*r, "field").is_none_or(|f| f.max(0) as u32 == field))
                            .and_then(|r| child(r, "x"))
                            .and_then(|x| attr_i64(x, "v"))?
                            .max(0) as usize;
                        let cf = cache_fields.get(field as usize).copied()?;
                        let si = child(cf, "sharedItems")?;
                        let item_node = si.children().filter(|n| n.is_element()).nth(item_x)?;
                        attr(item_node, "v").map(str::to_string)
                    })
                    .unwrap_or_default();
                Some(ibis_doc_model::pivot::PivotCalculatedItem {
                    field,
                    name,
                    formula: attr(ci, "formula").unwrap_or("").to_string(),
                })
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::date_serial;

    #[test]
    fn date_serials() {
        assert_eq!(date_serial("1900-01-01T00:00:00"), Some(1.0));
        assert_eq!(date_serial("1900-03-01T00:00:00"), Some(61.0));
        assert_eq!(date_serial("2026-01-05T00:00:00"), Some(46027.0));
        assert_eq!(date_serial("2026-01-05T12:00:00"), Some(46027.5));
    }
}

/// The data of `pivot`'s source: a range on its sheet (by name) or on
/// `sheet` itself, or from a table's or defined name's source.
pub fn pivot_source(presentation: &ibis_doc_model::Presentation, sheet: &ibis_doc_model::Sheet, pivot: &PivotTable) -> Option<ibis_doc_model::pivot::PivotData> {
    if let Some(name) = &pivot.source.name {
        // 1. Look for a table matching `name` across all sheets
        for slide in &presentation.slides {
            if let Some(source_sheet) = slide.shapes.iter().find_map(|sh| if let ibis_doc_model::ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None }) {
                if let Some(tbl) = source_sheet.tables.iter().find(|t| t.name.eq_ignore_ascii_case(name)) {
                    let range = ibis_doc_model::CellRange {
                        first_row: tbl.range.first_row,
                        last_row: tbl.range.last_row.saturating_sub(tbl.totals_rows),
                        first_column: tbl.range.first_column,
                        last_column: tbl.range.last_column,
                    };
                    return Some(ibis_doc_model::pivot::source_data(&source_sheet, range));
                }
            }
        }

        // 2. Look for a defined name matching `name` in workbook
        if let Some(wb) = &presentation.workbook {
            if let Some(dn) = wb.names.iter().find(|n| n.name.eq_ignore_ascii_case(name)) {
                let trimmed = dn.formula.trim().trim_start_matches('=');
                if let Some(range) = crate::xlsx::parse_range(trimmed) {
                    if let Some((sh_name, _)) = trimmed.split_once('!') {
                        let clean_sh = sh_name.trim().trim_matches('\'');
                        if let Some(slide) = presentation.slides.iter().find(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(clean_sh))) {
                            if let Some(source_sheet) = slide.shapes.iter().find_map(|sh| if let ibis_doc_model::ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None }) {
                                return Some(ibis_doc_model::pivot::source_data(&source_sheet, range));
                            }
                        }
                    }
                    return Some(ibis_doc_model::pivot::source_data(sheet, range));
                }
            }
        }
    }

    let range = pivot.source.range?;
    match &pivot.source.sheet {
        Some(name) => {
            let slide = presentation.slides.iter().find(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name)))?;
            let source = slide.shapes.iter().find_map(|sh| if let ibis_doc_model::ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None })?;
            Some(ibis_doc_model::pivot::source_data(&source, range))
        }
        None => Some(ibis_doc_model::pivot::source_data(sheet, range)),
    }
}

/// The data of the source of the pivot table `index` on `slide`.
pub fn pivot_data(presentation: &ibis_doc_model::Presentation, slide: ibis_doc_model::SlideId, index: usize) -> Option<ibis_doc_model::pivot::PivotData> {
    let s = presentation.slide(slide)?;
    let sheet = s.shapes.iter().find_map(|sh| if let ibis_doc_model::ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None })?;
    pivot_source(presentation, &sheet, sheet.pivots.get(index)?)
}

/// The sheet of `slide` with its pivot table `index` built again from its
/// source (ピボットテーブルの更新), in its style, marked as built again for
/// the save; `None` when it has no such table or its source cannot be read.
pub fn refreshed_pivot(presentation: &ibis_doc_model::Presentation, slide: ibis_doc_model::SlideId, index: usize) -> Option<ibis_doc_model::Sheet> {
    rebuilt_pivot(presentation, slide, index, None)
}

/// The sheet of `slide` with its pivot table `index` as `pivot` says
/// (a new one when `index` is past the sheet's; `None`: as it is), built
/// from its source in its style, marked as built again for the save.
pub fn rebuilt_pivot(presentation: &ibis_doc_model::Presentation, slide: ibis_doc_model::SlideId, index: usize, pivot: Option<PivotTable>) -> Option<ibis_doc_model::Sheet> {
    let s = presentation.slide(slide)?;
    let sheet = s.shapes.iter().find_map(|sh| if let ibis_doc_model::ShapeKind::Sheet(s) = &sh.kind { Some(s.clone()) } else { None })?;
    let mut out = (*sheet).clone();
    match pivot {
        Some(p) if index < out.pivots.len() => {
            let before = out.pivots[index].clone();
            out.clear_pivot_area(&before);
            out.pivots[index] = p;
        }
        Some(p) if index == out.pivots.len() => out.pivots.push(p),
        Some(_) => return None,
        None => {}
    }
    let mut pivot = out.pivots.get(index)?.clone();
    let data = pivot_source(presentation, &out, &pivot)?;
    pivot.fill_shared(&data);
    let built = pivot.build(&data);
    let workbook = presentation.workbook.as_ref();
    let base = workbook.and_then(|w| w.default_style.clone()).unwrap_or_default();
    let date1904 = workbook.is_some_and(|w| w.date1904);
    out.pivots[index] = pivot;
    out.put_pivot(index, &built, &base, date1904);
    out.pivots[index].refreshed = true;
    let theme_colors = workbook.map(|w| w.theme_colors.clone()).unwrap_or_default();
    let fallback;
    let theme = if theme_colors.is_empty() {
        fallback = crate::new_file::theme_colours();
        crate::theme::Theme::from_colors(&fallback)
    } else {
        crate::theme::Theme::from_colors(&theme_colors)
    };
    let colors = crate::xlsx::SheetColors::new(&theme, None);
    crate::table_style::apply_pivot_style(&mut out, index, &built.rows, &built.columns, None, &colors);
    Some(out)
}

/// The names the user gave fields (`pivotField/@name`) and items (`item/@n`).
fn aliases_of(table: Node, fields: &[PivotField]) -> ibis_doc_model::PivotAliases {
    let mut out = ibis_doc_model::PivotAliases::default();
    let Some(pfs) = child(table, "pivotFields") else { return out };
    for (i, pf) in children(pfs, "pivotField").enumerate() {
        let Some(field) = fields.get(i) else { break };
        if let Some(n) = attr(pf, "name").filter(|n| !n.is_empty() && *n != field.name) {
            out.fields.push((i as u32, n.to_string()));
        }
        for it in pf.children().filter(|n| n.is_element() && n.tag_name().name() == "items").flat_map(|n| children(n, "item")) {
            let (Some(n), Some(x)) = (attr(it, "n"), attr_i64(it, "x")) else { continue };
            if let Some(v) = field.shared.get(x.max(0) as usize) {
                let key = ibis_doc_model::pivot_item_key(v);
                if key != n {
                    out.items.push((i as u32, key, n.to_string()));
                }
            }
        }
    }
    out
}
