// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reading timelines (タイムライン): the timelines of a sheet (`xl/timelines/timeline*.xml`, MS-XLSX `x15:timeline`) and
//! the caches the workbook lists (`xl/timelineCaches/timelineCache*.xml`, `x15:timelineCacheDefinition`) that say
//! which field of which pivot tables each one is on and what it keeps. The drawing (`tsle:timeslicer`) places it.

use std::collections::HashMap;

use ibis_doc_model::timeline::Timeline;
use roxmltree::Node;

use crate::xml::{attr, attr_bool, attr_i64, child, children, elements};

/// What a timeline cache holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TimelineCache {
    pub name: String,
    pub source_name: String,
    pub pivots: Vec<String>,
    pub bounds: Option<(i64, i64)>,
    pub selection: Option<(i64, i64)>,
    pub part: String,
}

/// A date and time (`2026-01-01T00:00:00`) as a serial number of days.
pub(crate) fn serial_of(text: &str) -> Option<i64> {
    let date = text.split('T').next()?;
    let mut p = date.split('-');
    let (y, m, d) = (p.next()?.parse::<i64>().ok()?, p.next()?.parse::<i64>().ok()?, p.next()?.parse::<i64>().ok()?);
    Some(ibis_calc::eval::convert::days_from_civil(y, m, d) + 25_569)
}

/// A serial number as `2026-01-01T00:00:00`.
#[allow(dead_code)]
pub(crate) fn date_text(serial: i64) -> String {
    let (y, m, d) = ibis_calc::eval::convert::civil_from_days(serial - 25_569);
    format!("{y:04}-{m:02}-{d:02}T00:00:00")
}

pub(crate) fn read_cache(root: Node) -> Option<TimelineCache> {
    if root.tag_name().name() != "timelineCacheDefinition" {
        return None;
    }
    let mut cache = TimelineCache { name: attr(root, "name")?.to_string(), source_name: attr(root, "sourceName").unwrap_or("").to_string(), ..Default::default() };
    if let Some(p) = child(root, "pivotTables") {
        cache.pivots = children(p, "pivotTable").filter_map(|t| attr(t, "name").map(str::to_string)).collect();
    }
    if let Some(state) = child(root, "state") {
        if let Some(b) = child(state, "bounds") {
            cache.bounds = Some((attr(b, "startDate").and_then(serial_of)?, attr(b, "endDate").and_then(serial_of)?));
        }
        if let Some(s) = child(state, "selection") {
            // The end is the last day kept, as the cache has it: the selection runs to the day after.
            cache.selection = Some((attr(s, "startDate").and_then(serial_of)?, attr(s, "endDate").and_then(serial_of)? + 1));
        }
    }
    Some(cache)
}

pub(crate) fn read_timelines(root: Node, part: &str, caches: &HashMap<String, TimelineCache>) -> Vec<Timeline> {
    if root.tag_name().name() != "timelines" {
        return Vec::new();
    }
    elements(root)
        .filter(|n| n.tag_name().name() == "timeline")
        .filter_map(|n| {
            let name = attr(n, "name")?.to_string();
            let cache_name = attr(n, "cache")?.to_string();
            let cache = caches.get(&cache_name)?;
            // The bounds the cache gives, else a year of the selection or around today's data: one year.
            let bounds = cache.bounds.unwrap_or((45_658, 46_023));
            Some(Timeline {
                caption: attr(n, "caption").unwrap_or(&name).to_string(),
                name,
                cache: cache_name,
                source: cache.source_name.clone(),
                pivots: cache.pivots.clone(),
                level: attr_i64(n, "level").map_or(2, |v| v.clamp(0, 3) as u8),
                selection_level: attr_i64(n, "selectionLevel").map_or(2, |v| v.clamp(0, 3) as u8),
                scroll: attr(n, "scrollPosition").and_then(serial_of).unwrap_or(bounds.0),
                bounds,
                selection: cache.selection,
                show_header: attr_bool(n, "showHeader").unwrap_or(true),
                show_selection_label: attr_bool(n, "showSelectionLabel").unwrap_or(true),
                show_time_level: attr_bool(n, "showTimeLevel").unwrap_or(true),
                show_scroll_bar: attr_bool(n, "showHorizontalScrollbar").unwrap_or(true),
                style: attr(n, "style").unwrap_or("").to_string(),
                part: part.to_string(),
                cache_part: cache.part.clone(),
            })
        })
        .collect()
}

/// The timeline a drawing's `graphicFrame` holds (`tsle:timeslicer/@name`), if it is one.
pub(crate) fn drawing_timeline_name<'a>(anchor: Node<'a, '_>) -> Option<&'a str> {
    anchor.descendants().find(|n| n.is_element() && n.tag_name().name() == "timeslicer").and_then(|n| attr(n, "name"))
}
