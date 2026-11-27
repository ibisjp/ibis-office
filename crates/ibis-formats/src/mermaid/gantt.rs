// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! `gantt`.
//!
//! Times are seconds since 1970-01-01 (no time zone), so `dateFormat HH:mm`, `X` and
//! minute / second durations work alongside calendar dates.

use super::*;

const DAY: i64 = 86_400;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GanttStatus {
    Normal,
    Done,
    Active,
    Crit,
    Milestone,
}

pub(super) struct GanttTask {
    pub(super) section: String,
    pub(super) name: String,
    pub(super) status: GanttStatus,
    pub(super) start: i64,
    pub(super) end: i64,
}

/// A `vert` marker: a labelled vertical line, not a row.
pub(super) struct GanttVert {
    pub(super) name: String,
    pub(super) at: i64,
}

/// `tickInterval` units.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TickUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
    Year,
}

struct TodayMarker {
    stroke: String,
    width: f64,
    opacity: f64,
}

impl Default for TodayMarker {
    fn default() -> Self {
        Self { stroke: "#dc2626".into(), width: 2.0, opacity: 1.0 }
    }
}

impl TodayMarker {
    fn parse(style: &str) -> Option<Self> {
        if style.eq_ignore_ascii_case("off") {
            return None;
        }
        let mut marker = Self::default();
        for (key, value) in style.split(',').filter_map(|part| part.split_once(':')) {
            match key.trim() {
                "stroke" => {
                    if let Some(color) = config::css_color(value) {
                        marker.stroke = color;
                    }
                }
                "stroke-width" => {
                    if let Some(width) = config::px_number(value) {
                        marker.width = width;
                    }
                }
                "opacity" => {
                    if let Ok(opacity) = value.trim().parse::<f64>()
                        && (0.0..=1.0).contains(&opacity)
                    {
                        marker.opacity = opacity;
                    }
                }
                _ => {}
            }
        }
        Some(marker)
    }
}

pub(super) struct GanttChart {
    pub(super) title: Option<String>,
    today_marker: Option<TodayMarker>,
    pub(super) tasks: Vec<GanttTask>,
    pub(super) verts: Vec<GanttVert>,
    /// Days (since 1970-01-01) of the week that `excludes` shades: `weekends` (Saturday and
    /// Sunday, or Friday and Saturday after `weekend friday`), weekday names.
    pub(super) excluded_weekdays: Vec<i64>,
    /// `excludes 2024-01-10`: single days, as day numbers.
    pub(super) excluded_days: Vec<i64>,
    pub(super) axis_format: Option<String>,
    pub(super) tick_interval: Option<(i64, TickUnit)>,
    /// `weekday monday`: the first day of a week tick (0 = Sunday, the default).
    pub(super) week_start: i64,
}

/// The `config.gantt.*` options the renderer reads.
pub(super) struct GanttLook {
    /// `useWidth`: the whole diagram's width in px.
    pub(super) use_width: Option<f32>,
    /// `topAxis`: tick labels above the rows too.
    pub(super) top_axis: bool,
    /// `rightPadding`, in px.
    pub(super) right_padding: f32,
    /// `numberSectionStyles`: sections take turns over this many row colours.
    pub(super) section_styles: Option<usize>,
    /// `barHeight` / `barGap`, in px.
    pub(super) bar_height: f32,
    pub(super) bar_gap: f32,
}

impl GanttLook {
    pub(super) fn from_config(config: &MermaidConfig) -> Self {
        let number = |key: &str, lo: f32, hi: f32| {
            config.option(key).and_then(|v| px_number(v)).map(|v| v as f32).filter(|v| (lo..=hi).contains(v))
        };
        GanttLook {
            use_width: number("gantt.useWidth", 200.0, 4000.0),
            top_axis: config.flag("gantt.topAxis", false),
            right_padding: number("gantt.rightPadding", 0.0, 400.0).unwrap_or(24.0),
            section_styles: number("gantt.numberSectionStyles", 1.0, 4.0).map(|n| n as usize),
            bar_height: number("gantt.barHeight", 6.0, 80.0).unwrap_or(18.0),
            bar_gap: number("gantt.barGap", 0.0, 60.0).unwrap_or(12.0),
        }
    }
}

/// Row colours that `numberSectionStyles` cycles through, one per section.
const SECTION_BACKGROUNDS: [&str; 4] = ["#eef2ff", "#ffffff", "#fefce8", "#f0fdf4"];

pub(super) fn render_gantt_svg(source: &str, config: &MermaidConfig) -> Option<(Vec<u8>, u32, u32)> {
    // Today from the calculator's clock (the crate's rule: no direct clock call, wasm has none); without a clock no marker.
    // 25569 is the serial of 1970-01-01.
    let now = ibis_calc::eval::convert::today_serial(false).map_or(i64::MIN, |serial| (serial - 25_569) * 86_400);
    render_gantt_svg_at(source, config, now)
}

fn render_gantt_svg_at(source: &str, config: &MermaidConfig, now: i64) -> Option<(Vec<u8>, u32, u32)> {
    let look = GanttLook::from_config(config);
    let chart = parse_gantt_chart(source)?;
    let tasks = &chart.tasks;

    let origin = days_from_civil(2026, 1, 1) * DAY;
    let times = tasks.iter().flat_map(|t| [t.start, t.end]).chain(chart.verts.iter().map(|v| v.at));
    let mut min_t = times.clone().min().unwrap_or(origin);
    let mut max_t = times.max().unwrap_or(origin + 7 * DAY);
    if max_t <= min_t {
        max_t = min_t + if tasks.iter().all(|t| (t.end - t.start) % DAY == 0) { DAY } else { 60 };
    }

    let label_col_w = tasks
        .iter()
        .map(|t| estimate_text_width(&t.name, 12.0))
        .fold(120.0f32, f32::max)
        .min(240.0)
        + 24.0;
    let mut ticks = gantt_ticks(min_t, max_t, chart.tick_interval, chart.week_start);
    let format = chart.axis_format.as_deref().unwrap_or("%Y-%m-%d");
    let mut labels: Vec<String> = ticks.iter().map(|&t| gantt_strftime(t, format)).collect();
    // A date-only axis should not keep two ticks that format to the same day (12h auto steps
    // on a 5-day chart). Time formats (`%H`, `%s`, …) keep the finer ticks.
    let date_only = !["%H", "%I", "%M", "%S", "%s", "%X"].iter().any(|t| format.contains(t));
    if date_only {
        let mut uniq_t = Vec::new();
        let mut uniq_l = Vec::new();
        let mut last = None::<String>;
        for (t, l) in ticks.into_iter().zip(labels) {
            if last.as_deref() != Some(l.as_str()) {
                last = Some(l.clone());
                uniq_t.push(t);
                uniq_l.push(l);
            }
        }
        ticks = uniq_t;
        labels = uniq_l;
    }
    if let Some(&first) = ticks.first() {
        min_t = min_t.min(first);
    }
    if let Some(&last) = ticks.last() {
        max_t = max_t.max(last);
    }
    let span = (max_t - min_t) as f32;
    let widest = labels.iter().map(|l| estimate_text_width(l, 10.5)).fold(0.0f32, f32::max) + 8.0;
    // Keep the last tick label inside the viewBox, unless the author fixed the width.
    let right_pad = if look.use_width.is_some() {
        look.right_padding
    } else {
        look.right_padding.max(widest / 2.0 + 4.0)
    };
    // `useWidth` fixes the whole width; otherwise grow the axis so date labels do not overlap.
    let chart_w = if let Some(w) = look.use_width {
        (w - label_col_w - right_pad).max(120.0)
    } else {
        let needed = if ticks.len() > 1 { widest * ticks.len() as f32 } else { 480.0 };
        needed.max(480.0).min(1100.0)
    };
    let vert_band = if chart.verts.is_empty() { 0.0 } else { 18.0 };
    let top_axis_band = if look.top_axis { 20.0 } else { 0.0 };
    let top_pad = if chart.title.is_some() { 52.0f32 } else { 28.0f32 } + vert_band + top_axis_band;
    let row_h = look.bar_height + look.bar_gap;
    let axis_h = 28.0f32;
    let total_w = (label_col_w + chart_w + right_pad).ceil() as u32;
    let total_h = (top_pad + tasks.len() as f32 * row_h + axis_h + 12.0).ceil() as u32;
    let x_of = |t: i64| label_col_w + ((t - min_t) as f32 / span) * chart_w;

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}" viewBox="0 0 {total_w} {total_h}" font-family="sans-serif">"##
    ));
    svg.push_str(&format!(r##"<rect width="{total_w}" height="{total_h}" fill="#ffffff"/>"##));
    if let Some(t) = &chart.title {
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="28" text-anchor="middle" font-size="15" font-weight="bold" fill="#0f172a">{}</text>"##,
            total_w as f32 / 2.0,
            escape_xml(t)
        ));
    }

    // Grid lines and tick labels: `tickInterval`, or a readable step of about twenty ticks (closer to mermaid.js).
    let bottom_y = top_pad + tasks.len() as f32 * row_h;
    let gap = if ticks.len() > 1 { chart_w * ((ticks[1] - ticks[0]) as f32 / span) } else { chart_w };
    // Only skip labels when they would clearly overlap; mermaid.js keeps weekly %Y-%m-%d ticks
    // and daily labels on short charts, so do not thin a modest set once the axis is wide enough.
    let every = if ticks.len() <= 20 && gap >= widest * 0.85 {
        1
    } else {
        ((widest / (gap * 1.2).max(1.0)).ceil() as usize).max(1)
    };
    for (i, (&t, label)) in ticks.iter().zip(&labels).enumerate() {
        let gx = x_of(t);
        svg.push_str(&format!(
            r##"<line x1="{gx:.1}" y1="{top_pad:.1}" x2="{gx:.1}" y2="{bottom_y:.1}" stroke="#e2e8f0" stroke-width="1" data-role="tick"/>"##
        ));
        if i % every == 0 {
            svg.push_str(&format!(
                r##"<text x="{gx:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#64748b">{}</text>"##,
                bottom_y + 18.0,
                escape_xml(label)
            ));
            if look.top_axis {
                svg.push_str(&format!(
                    r##"<text x="{gx:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#64748b" data-role="top-axis">{}</text>"##,
                    top_pad - 7.0,
                    escape_xml(label)
                ));
            }
        }
    }

    // Rows
    let mut prev_section: Option<&str> = None;
    let mut section_index = 0usize;
    for (idx, task) in tasks.iter().enumerate() {
        let ry = top_pad + idx as f32 * row_h;
        if idx > 0 && prev_section != Some(task.section.as_str()) {
            section_index += 1;
        }
        let bg = match look.section_styles {
            Some(n) => SECTION_BACKGROUNDS[section_index % n],
            None if idx % 2 == 0 => "#f8fafc",
            None => "#ffffff",
        };
        svg.push_str(&format!(
            r##"<rect x="8" y="{ry:.1}" width="{:.1}" height="{row_h:.1}" fill="{bg}" fill-opacity="0.7"/>"##,
            total_w as f32 - 16.0
        ));
        let label = if prev_section != Some(task.section.as_str()) {
            prev_section = Some(task.section.as_str());
            format!("[{}] {}", task.section, task.name)
        } else {
            task.name.clone()
        };
        svg.push_str(&format!(
            r##"<text x="14" y="{:.1}" font-size="12" fill="#1e293b">{}</text>"##,
            ry + row_h / 2.0 + 4.0,
            escape_xml(&label)
        ));
    }

    // `excludes` shades the axis. Day (and longer) durations skip those days, like mermaid.js.
    let (first_day, last_day) = (min_t.div_euclid(DAY), (max_t - 1).div_euclid(DAY));
    if last_day - first_day < 3660 {
        for day in first_day..=last_day {
            let weekday = (day + 4).rem_euclid(7);
            let kind = if chart.excluded_days.contains(&day) {
                "date"
            } else if chart.excluded_weekdays.contains(&weekday) {
                "weekend"
            } else {
                continue;
            };
            let x = x_of((day * DAY).max(min_t));
            let w = x_of(((day + 1) * DAY).min(max_t)) - x;
            let band_h = bottom_y - top_pad;
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{top_pad:.1}" width="{w:.1}" height="{band_h:.1}" fill="#e2e8f0" fill-opacity="0.9" data-nonworking="{kind}"/>"##
            ));
        }
    }

    for (idx, task) in tasks.iter().enumerate() {
        let ry = top_pad + idx as f32 * row_h;
        let x1 = x_of(task.start);
        let x2 = x_of(task.end);
        let bw = (x2 - x1).max(6.0);
        let start_iso = gantt_iso(task.start);
        let (fill, stroke) = match task.status {
            GanttStatus::Done => ("#cbd5e1", "#64748b"),
            GanttStatus::Active => ("#93c5fd", "#2563eb"),
            GanttStatus::Crit => ("#fca5a5", "#dc2626"),
            GanttStatus::Milestone => ("#fde047", "#ca8a04"),
            GanttStatus::Normal => ("#a5b4fc", "#4f46e5"),
        };
        if task.status == GanttStatus::Milestone {
            // A milestone sits in the middle of its (usually zero) duration.
            let mx = (x1 + x2) / 2.0;
            let cy = ry + row_h / 2.0;
            let m = (look.bar_height / 2.0 - 1.0).max(8.0);
            svg.push_str(&format!(
                r##"<polygon points="{mx:.1},{:.1} {:.1},{cy:.1} {mx:.1},{:.1} {:.1},{cy:.1}" fill="{fill}" stroke="{stroke}" stroke-width="1.3" data-start="{start_iso}"/>"##,
                cy - m,
                mx + m,
                cy + m,
                mx - m
            ));
        } else {
            svg.push_str(&format!(
                r##"<rect x="{x1:.1}" y="{:.1}" width="{bw:.1}" height="{}" rx="4" fill="{fill}" stroke="{stroke}" stroke-width="1.2" data-start="{start_iso}"/>"##,
                ry + look.bar_gap / 2.0,
                look.bar_height
            ));
        }
    }

    // `vert` markers run across every row, labelled above the chart.
    for v in &chart.verts {
        let vx = x_of(v.at);
        svg.push_str(&format!(
            r##"<line x1="{vx:.1}" y1="{:.1}" x2="{vx:.1}" y2="{bottom_y:.1}" stroke="#7c3aed" stroke-width="1.5" stroke-dasharray="4,3" data-role="vert" data-start="{}"/>"##,
            top_pad - 4.0,
            gantt_iso(v.at)
        ));
        svg.push_str(&format!(
            r##"<text x="{vx:.1}" y="{:.1}" text-anchor="middle" font-size="10.5" fill="#6d28d9">{}</text>"##,
            top_pad - top_axis_band - 7.0,
            escape_xml(&v.name)
        ));
    }

    if let Some(marker) = chart.today_marker
        && (min_t..=max_t).contains(&now)
    {
        let x = x_of(now);
        svg.push_str(&format!(
            r#"<line x1="{x:.1}" y1="{top_pad:.1}" x2="{x:.1}" y2="{bottom_y:.1}" stroke="{}" stroke-width="{}" opacity="{}" data-role="today-marker"/>"#,
            marker.stroke, marker.width, marker.opacity
        ));
    }

    svg.push_str("</svg>");
    Some((svg.into_bytes(), total_w, total_h))
}

pub(super) enum GanttAnchor {
    Date(i64),
    After(Vec<String>),
    Follow,
}

pub(super) enum GanttEnd {
    Duration(i64),
    Date(i64),
    /// `until a b`: the earliest start of the named tasks.
    Until(Vec<String>),
}

pub(super) struct GanttPending {
    pub(super) section: String,
    pub(super) name: String,
    pub(super) status: GanttStatus,
    pub(super) vert: bool,
    pub(super) id: Option<String>,
    pub(super) anchor: GanttAnchor,
    pub(super) end: GanttEnd,
}

pub(super) const WEEKDAYS: [&str; 7] = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];

pub(super) fn parse_gantt_chart(source: &str) -> Option<GanttChart> {
    let mut chart = GanttChart {
        title: None,
        today_marker: Some(TodayMarker::default()),
        tasks: Vec::new(),
        verts: Vec::new(),
        excluded_weekdays: Vec::new(),
        excluded_days: Vec::new(),
        axis_format: None,
        tick_interval: None,
        week_start: 0,
    };
    let mut current_section = String::from("Tasks");
    let mut pending = Vec::new();
    let mut date_format = String::new();
    let mut exclude_weekends = false;
    let mut weekend_friday = false;
    let mut excludes: Vec<String> = Vec::new();

    for raw in source.lines().skip(1) {
        // `%%` starts a comment anywhere on the line.
        let l = match raw.find("%%") {
            Some(at) => raw[..at].trim(),
            None => raw.trim(),
        };
        if l.is_empty() {
            continue;
        }
        if let Some(rest) = l.strip_prefix("title ") {
            chart.title = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = l.strip_prefix("dateFormat ") {
            date_format = rest.trim().to_string();
            continue;
        }
        if let Some(rest) = l.strip_prefix("axisFormat ") {
            chart.axis_format = Some(rest.trim().to_string());
            continue;
        }
        if let Some(rest) = l.strip_prefix("tickInterval ") {
            // An interval Mermaid does not accept (`1decade`) is ignored, as Mermaid does.
            chart.tick_interval = parse_tick_interval(rest.trim());
            continue;
        }
        if let Some(rest) = l.strip_prefix("weekday ") {
            if let Some(day) = WEEKDAYS.iter().position(|d| d.eq_ignore_ascii_case(rest.trim())) {
                chart.week_start = day as i64;
            }
            continue;
        }
        if let Some(rest) = l.strip_prefix("weekend ") {
            weekend_friday = rest.trim().eq_ignore_ascii_case("friday");
            continue;
        }
        if let Some(rest) = l.strip_prefix("excludes ") {
            for word in rest.split([',', ' ']).map(str::trim).filter(|w| !w.is_empty()) {
                if word.eq_ignore_ascii_case("weekends") {
                    exclude_weekends = true;
                } else {
                    excludes.push(word.to_string());
                }
            }
            continue;
        }
        if let Some(style) = l.strip_prefix("todayMarker").filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace)) {
            chart.today_marker = TodayMarker::parse(style.trim());
            continue;
        }
        if l.starts_with("includes ") || l.starts_with("inclusiveEndDates") || l.starts_with("topAxis") {
            continue;
        }
        if let Some(rest) = l.strip_prefix("section ") {
            current_section = rest.trim().to_string();
            continue;
        }
        let Some((name_part, meta_part)) = l.split_once(':') else {
            continue;
        };
        let task_name = name_part.trim().to_string();
        if task_name.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = meta_part.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
        if tokens.is_empty() {
            continue;
        }
        let mut status = GanttStatus::Normal;
        let mut vert = false;
        let mut idx = 0;
        while idx < tokens.len() {
            match tokens[idx] {
                "done" => status = GanttStatus::Done,
                "active" => status = GanttStatus::Active,
                "crit" => status = GanttStatus::Crit,
                "milestone" => status = GanttStatus::Milestone,
                "vert" => vert = true,
                _ => break,
            }
            idx += 1;
        }
        let rem = &tokens[idx..];
        if rem.is_empty() {
            continue;
        }
        let is_start = |tok: &str| tok.starts_with("after ") || parse_gantt_date_formatted(tok, &date_format).is_some();
        let (task_id, start_tok, end_tok) = match rem.len() {
            1 => (None, None, rem[0]),
            2 if is_start(rem[0]) => (None, Some(rem[0]), rem[1]),
            2 => (Some(rem[0].to_string()), None, rem[1]),
            _ => (Some(rem[0].to_string()), Some(rem[1]), rem[2]),
        };
        let anchor = match start_tok {
            Some(st) => match st.strip_prefix("after ") {
                Some(rest) => {
                    let ids: Vec<String> = rest.split_whitespace().map(str::to_string).collect();
                    if ids.is_empty() { GanttAnchor::Follow } else { GanttAnchor::After(ids) }
                }
                None => parse_gantt_date_formatted(st, &date_format).map_or(GanttAnchor::Follow, GanttAnchor::Date),
            },
            None => GanttAnchor::Follow,
        };
        let end = if let Some(rest) = end_tok.strip_prefix("until ") {
            GanttEnd::Until(rest.split_whitespace().map(str::to_string).collect())
        } else if let Some(t) = parse_gantt_date_formatted(end_tok, &date_format) {
            GanttEnd::Date(t)
        } else {
            GanttEnd::Duration(parse_gantt_duration(end_tok).unwrap_or(DAY))
        };
        pending.push(GanttPending {
            section: current_section.clone(),
            name: task_name,
            status,
            vert,
            id: task_id,
            anchor,
            end,
        });
    }

    chart.excluded_weekdays = match (exclude_weekends, weekend_friday) {
        (false, _) => Vec::new(),
        (true, false) => vec![6, 0],
        (true, true) => vec![5, 6],
    };
    for word in excludes {
        if let Some(day) = WEEKDAYS.iter().position(|d| d.eq_ignore_ascii_case(&word)) {
            chart.excluded_weekdays.push(day as i64);
        } else if let Some(t) = parse_gantt_date_formatted(&word, &date_format) {
            chart.excluded_days.push(t.div_euclid(DAY));
        }
    }

    if pending.is_empty() {
        // Settings only (`tickInterval`, `weekday` ...): an empty axis, like Mermaid.
        let header_only = source.lines().skip(1).any(|l| !l.trim().is_empty());
        return header_only.then_some(chart);
    }

    // A chart with no dated task still starts at 2026-01-01. A dated chart
    // never pulls an unresolved `after` back to that origin.
    let origin = days_from_civil(2026, 1, 1) * DAY;
    let absolute_end = pending
        .iter()
        .filter_map(|t| match (&t.anchor, &t.end) {
            (GanttAnchor::Date(s), GanttEnd::Date(e)) => Some((*e).max(*s)),
            (GanttAnchor::Date(s), GanttEnd::Duration(d)) => Some(s + d),
            _ => None,
        })
        .max();
    // `until` may name a later task: resolve until nothing changes.
    let mut resolved: Vec<(i64, i64)> = Vec::new();
    for _ in 0..8 {
        let mut id_start: HashMap<&str, i64> = HashMap::new();
        let mut id_end: HashMap<&str, i64> = HashMap::new();
        for (task, &(start, end)) in pending.iter().zip(&resolved) {
            if let Some(id) = &task.id {
                id_start.insert(id, start);
                id_end.insert(id, end);
            }
        }
        let mut next = Vec::with_capacity(pending.len());
        let mut last_end: Option<i64> = None;
        let mut known_end: HashMap<&str, i64> = HashMap::new();
        for task in &pending {
            let start = match &task.anchor {
                GanttAnchor::Date(t) => *t,
                GanttAnchor::Follow => last_end.unwrap_or(origin),
                GanttAnchor::After(ids) => {
                    let known: Vec<i64> = ids.iter().filter_map(|id| known_end.get(id.as_str()).or(id_end.get(id.as_str())).copied()).collect();
                    if !known.is_empty() && known.len() == ids.len() {
                        known.into_iter().max().unwrap_or(origin)
                    } else {
                        absolute_end.unwrap_or(origin)
                    }
                }
            };
            let end = match &task.end {
                GanttEnd::Date(t) => (*t).max(start),
                GanttEnd::Duration(d) => gantt_add_duration(start, *d, &chart.excluded_weekdays, &chart.excluded_days)?,
                GanttEnd::Until(ids) => ids
                    .iter()
                    .filter_map(|id| id_start.get(id.as_str()).copied())
                    .min()
                    .map_or(start + DAY, |t| t.max(start)),
            };
            if let Some(id) = &task.id {
                known_end.insert(id, end);
            }
            last_end = Some(end);
            next.push((start, end));
        }
        if next == resolved {
            break;
        }
        resolved = next;
    }
    for (task, (start, end)) in pending.into_iter().zip(resolved) {
        if task.vert {
            chart.verts.push(GanttVert { name: task.name, at: start });
        } else {
            chart.tasks.push(GanttTask {
                section: task.section,
                name: task.name,
                status: task.status,
                start,
                end,
            });
        }
    }
    Some(chart)
}

/// `1day`, `2week`, `30minute` ... (`millisecond` is read as one second).
/// Add a duration, skipping excluded calendar days when the duration is a whole number of days
/// (mermaid.js `excludes` stretches day/week/month bars over weekends).
pub(super) fn gantt_add_duration(start: i64, duration: i64, excluded_weekdays: &[i64], excluded_days: &[i64]) -> Option<i64> {
    if duration <= 0 {
        return Some(start);
    }
    if duration % DAY != 0 || (excluded_weekdays.is_empty() && excluded_days.is_empty()) {
        return start.checked_add(duration);
    }
    // An impossible calendar or an enormous span must not hang document import.
    if (0..7).all(|day| excluded_weekdays.contains(&day)) {
        return None;
    }
    let is_excluded = |t: i64| {
        let day = t.div_euclid(DAY);
        let weekday = (day + 4).rem_euclid(7);
        excluded_days.contains(&day) || excluded_weekdays.contains(&weekday)
    };
    let mut t = start;
    let mut remaining = duration / DAY;
    let mut scanned = 0;
    while remaining > 0 {
        if scanned >= 1_000_000 {
            return None;
        }
        scanned += 1;
        t = t.checked_add(DAY)?;
        if !is_excluded(t - DAY) {
            remaining -= 1;
        }
    }
    Some(t)
}

pub(super) fn parse_tick_interval(s: &str) -> Option<(i64, TickUnit)> {
    let digits = s.find(|c: char| !c.is_ascii_digit())?;
    let n: i64 = s[..digits].parse().ok().filter(|n| *n > 0)?;
    let unit = match &s[digits..] {
        "millisecond" | "second" => TickUnit::Second,
        "minute" => TickUnit::Minute,
        "hour" => TickUnit::Hour,
        "day" => TickUnit::Day,
        "week" => TickUnit::Week,
        "month" => TickUnit::Month,
        _ => return None,
    };
    Some((n, unit))
}

/// Tick times inside `[min, max]`: every `interval` from an aligned start, or a readable
/// automatic step.
pub(super) fn gantt_ticks(min: i64, max: i64, interval: Option<(i64, TickUnit)>, week_start: i64) -> Vec<i64> {
    let span = max - min;
    let (n, unit) = interval.filter(|&(n, unit)| span / gantt_unit_secs(n, unit) <= 400).unwrap_or_else(|| {
        const STEPS: &[(i64, TickUnit)] = &[
            (1, TickUnit::Second),
            (5, TickUnit::Second),
            (15, TickUnit::Second),
            (30, TickUnit::Second),
            (1, TickUnit::Minute),
            (5, TickUnit::Minute),
            (15, TickUnit::Minute),
            (30, TickUnit::Minute),
            (1, TickUnit::Hour),
            (3, TickUnit::Hour),
            (6, TickUnit::Hour),
            (12, TickUnit::Hour),
            (1, TickUnit::Day),
            (2, TickUnit::Day),
            (1, TickUnit::Week),
            (1, TickUnit::Month),
            (3, TickUnit::Month),
            (1, TickUnit::Year),
            (5, TickUnit::Year),
            (10, TickUnit::Year),
            (50, TickUnit::Year),
            (100, TickUnit::Year),
        ];
        *STEPS.iter().find(|&&(n, unit)| span / gantt_unit_secs(n, unit) <= 20).unwrap_or(&(1000, TickUnit::Year))
    });
    let mut ticks = Vec::new();
    let mut t = gantt_align(min, n, unit, week_start);
    while t <= max && ticks.len() <= 400 {
        if t >= min {
            ticks.push(t);
        }
        let next = gantt_step(t, n, unit);
        if next <= t {
            break;
        }
        t = next;
    }
    // mermaid.js / d3 nice the domain for time-of-day axes (e.g. 17:58 → 18:00).
    // Calendar week/month intervals should not grow an extra period past the last task.
    if matches!(unit, TickUnit::Second | TickUnit::Minute | TickUnit::Hour)
        && ticks.last().copied().unwrap_or(min) < max
        && t > max
        && ticks.len() <= 400
    {
        ticks.push(t);
    }
    ticks
}

/// The (average) length of `n` units, for counting ticks.
pub(super) fn gantt_unit_secs(n: i64, unit: TickUnit) -> i64 {
    n * match unit {
        TickUnit::Second => 1,
        TickUnit::Minute => 60,
        TickUnit::Hour => 3600,
        TickUnit::Day => DAY,
        TickUnit::Week => 7 * DAY,
        TickUnit::Month => 30 * DAY,
        TickUnit::Year => 365 * DAY,
    }
}

/// The last unit boundary at or before `t` (weeks start on `week_start`, years on a multiple of `n`).
pub(super) fn gantt_align(t: i64, n: i64, unit: TickUnit, week_start: i64) -> i64 {
    match unit {
        TickUnit::Second | TickUnit::Minute | TickUnit::Hour => {
            let size = gantt_unit_secs(n, unit);
            t.div_euclid(size) * size
        }
        TickUnit::Day => {
            if n <= 1 {
                t.div_euclid(DAY) * DAY
            } else {
                // Align multi-day steps to the first of the month (closer to d3 calendar intervals).
                let (y, m, _) = civil_from_days(t.div_euclid(DAY));
                let first = days_from_civil(y, m, 1);
                let off = (t.div_euclid(DAY) - first).div_euclid(n) * n;
                (first + off) * DAY
            }
        }
        TickUnit::Week => {
            let day = t.div_euclid(DAY);
            let back = ((day + 4).rem_euclid(7) - week_start).rem_euclid(7);
            (day - back) * DAY
        }
        TickUnit::Month => {
            let (y, m, _) = civil_from_days(t.div_euclid(DAY));
            days_from_civil(y, m, 1) * DAY
        }
        TickUnit::Year => {
            let (y, _, _) = civil_from_days(t.div_euclid(DAY));
            days_from_civil(y.div_euclid(n) * n, 1, 1) * DAY
        }
    }
}

pub(super) fn gantt_step(t: i64, n: i64, unit: TickUnit) -> i64 {
    match unit {
        TickUnit::Month | TickUnit::Year => {
            let (y, m, d) = civil_from_days(t.div_euclid(DAY));
            let months = if unit == TickUnit::Month { n } else { n * 12 };
            let index = y * 12 + (m as i64 - 1) + months;
            days_from_civil(index.div_euclid(12), (index.rem_euclid(12) + 1) as u32, d) * DAY + t.rem_euclid(DAY)
        }
        _ => t + gantt_unit_secs(n, unit),
    }
}

/// d3 `timeFormat` directives used by `axisFormat`.
pub(super) fn gantt_strftime(t: i64, format: &str) -> String {
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let day = t.div_euclid(DAY);
    let secs = t.rem_euclid(DAY);
    let (y, m, d) = civil_from_days(day);
    let (hh, mm, ss) = (secs / 3600, secs / 60 % 60, secs % 60);
    let weekday = (day + 4).rem_euclid(7) as usize;
    let mut out = String::new();
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut spec = chars.next();
        // `%-d` / `%_d` / `%0d` padding flags.
        let pad = match spec {
            Some('-') | Some('_') | Some('0') => {
                let flag = spec;
                spec = chars.next();
                flag
            }
            _ => None,
        };
        let num = |v: i64, width: usize| match pad {
            Some('-') => v.to_string(),
            Some('_') => format!("{v:>width$}"),
            _ => format!("{v:0width$}"),
        };
        match spec {
            Some('Y') => out.push_str(&y.to_string()),
            Some('y') => out.push_str(&num(y.rem_euclid(100), 2)),
            Some('m') => out.push_str(&num(m as i64, 2)),
            Some('d') => out.push_str(&num(d as i64, 2)),
            Some('e') => out.push_str(&format!("{d:>2}")),
            Some('H') => out.push_str(&num(hh, 2)),
            Some('I') => out.push_str(&num((hh + 11) % 12 + 1, 2)),
            Some('M') => out.push_str(&num(mm, 2)),
            Some('S') => out.push_str(&num(ss, 2)),
            Some('L') => out.push_str("000"),
            Some('p') => out.push_str(if hh < 12 { "AM" } else { "PM" }),
            Some('s') => out.push_str(&t.to_string()),
            Some('Q') => out.push_str(&(t * 1000).to_string()),
            Some('j') => out.push_str(&num(day - days_from_civil(y, 1, 1) + 1, 3)),
            Some('b') => out.push_str(&MONTHS[m as usize - 1][..3]),
            Some('B') => out.push_str(MONTHS[m as usize - 1]),
            Some('a') => {
                let name = WEEKDAYS[weekday];
                out.push_str(&name[..1].to_ascii_uppercase());
                out.push_str(&name[1..3]);
            }
            Some('A') => {
                let name = WEEKDAYS[weekday];
                out.push_str(&name[..1].to_ascii_uppercase());
                out.push_str(&name[1..]);
            }
            Some('w') => out.push_str(&weekday.to_string()),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// `2026-01-05`, or `2026-01-05T17:49` / `...T17:49:30` when the time is not midnight.
pub(super) fn gantt_iso(t: i64) -> String {
    let (y, m, d) = civil_from_days(t.div_euclid(DAY));
    let secs = t.rem_euclid(DAY);
    match (secs, secs % 60) {
        (0, _) => format!("{y:04}-{m:02}-{d:02}"),
        (_, 0) => format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}", secs / 3600, secs / 60 % 60),
        _ => format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60),
    }
}

/// Read a time with the `dateFormat` tokens `YYYY`, `YY`, `MM`, `DD`, `HH`, `mm`, `ss`,
/// `X` (Unix seconds) and `x` (Unix milliseconds). Missing date parts default to
/// 2026-01-01 (a time of day) or to January / the 1st (`YYYY`, `YYYY-MM`).
pub(super) fn parse_gantt_date_formatted(text: &str, format: &str) -> Option<i64> {
    let format = format.trim();
    if format.is_empty() {
        return parse_gantt_date(text).map(|day| day * DAY);
    }
    let text = text.trim().as_bytes();
    let format = format.as_bytes();
    let (mut ti, mut fi) = (0usize, 0usize);
    let (mut year, mut month, mut day) = (None, None, None);
    let (mut hour, mut minute, mut second) = (0i64, 0i64, 0i64);
    let token = |fi: usize, t: &[u8]| format[fi..].starts_with(t);
    while fi < format.len() {
        if token(fi, b"YYYY") || token(fi, b"yyyy") {
            year = Some(take_gantt_digits(text, &mut ti, 4)?);
            fi += 4;
        } else if token(fi, b"YY") {
            year = Some(2000 + take_gantt_digits(text, &mut ti, 2)?);
            fi += 2;
        } else if token(fi, b"MM") {
            month = Some(take_gantt_digits_upto(text, &mut ti, 2)? as u32);
            fi += 2;
        } else if token(fi, b"DD") || token(fi, b"dd") {
            day = Some(take_gantt_digits_upto(text, &mut ti, 2)? as u32);
            fi += 2;
        } else if token(fi, b"HH") || token(fi, b"hh") {
            hour = take_gantt_digits_upto(text, &mut ti, 2)?;
            fi += 2;
        } else if token(fi, b"mm") {
            minute = take_gantt_digits_upto(text, &mut ti, 2)?;
            fi += 2;
        } else if token(fi, b"ss") {
            second = take_gantt_digits_upto(text, &mut ti, 2)?;
            fi += 2;
        } else if token(fi, b"X") || token(fi, b"x") {
            let start = ti;
            if ti < text.len() && text[ti] == b'-' {
                ti += 1;
            }
            while ti < text.len() && text[ti].is_ascii_digit() {
                ti += 1;
            }
            let value: i64 = std::str::from_utf8(&text[start..ti]).ok()?.parse().ok()?;
            let secs = if format[fi] == b'x' { value.div_euclid(1000) } else { value };
            return (ti == text.len()).then_some(secs);
        } else {
            // A literal separator must match a non-digit.
            if ti >= text.len() || text[ti].is_ascii_digit() {
                return None;
            }
            fi += 1;
            ti += 1;
        }
    }
    if ti != text.len() || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let date = match (year, month, day) {
        (None, None, None) => days_from_civil(2026, 1, 1),
        (Some(y), m, d) => {
            let (m, d) = (m.unwrap_or(1), d.unwrap_or(1));
            if !(1..=12).contains(&m) || d == 0 || d > 31 {
                return None;
            }
            days_from_civil(y, m, d)
        }
        _ => return None,
    };
    Some(date * DAY + hour * 3600 + minute * 60 + second)
}

pub(super) fn take_gantt_digits(text: &[u8], ti: &mut usize, n: usize) -> Option<i64> {
    if *ti + n > text.len() {
        return None;
    }
    let slice = &text[*ti..*ti + n];
    if slice.iter().any(|b| !b.is_ascii_digit()) {
        return None;
    }
    let value = std::str::from_utf8(slice).ok()?.parse().ok()?;
    *ti += n;
    Some(value)
}

/// One or two digits (`9:05` and `09:05` both read with `HH:mm`).
pub(super) fn take_gantt_digits_upto(text: &[u8], ti: &mut usize, n: usize) -> Option<i64> {
    let len = text[*ti..].iter().take(n).take_while(|b| b.is_ascii_digit()).count();
    if len == 0 {
        return None;
    }
    take_gantt_digits(text, ti, len)
}

pub(super) fn parse_gantt_date(s: &str) -> Option<i64> {
    let t = s.trim();
    let sep = if t.contains('-') { '-' } else if t.contains('/') { '/' } else { return None };
    let parts: Vec<&str> = t.split(sep).collect();
    if parts.len() != 3 {
        return None;
    }
    let y: i64 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    let d: u32 = parts[2].parse().ok()?;
    Some(days_from_civil(y, m, d))
}

/// `30d`, `1w`, `24h`, `10m`, `1s`, `500ms`, `1.5d` → seconds.
pub(super) fn parse_gantt_duration(s: &str) -> Option<i64> {
    let t = s.trim();
    let split = t.find(|c: char| !(c.is_ascii_digit() || c == '.'))?;
    let n: f64 = t[..split].parse().ok()?;
    let unit = match &t[split..] {
        "ms" => 0.001,
        "s" => 1.0,
        "m" => 60.0,
        "h" => 3600.0,
        "d" => DAY as f64,
        "w" => 7.0 * DAY as f64,
        "M" => 30.0 * DAY as f64,
        "y" => 365.0 * DAY as f64,
        _ => return None,
    };
    Some((n * unit).round() as i64)
}

/// Howard Hinnant's civil-from-days / days-from-civil algorithms (proleptic Gregorian).
pub(super) fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = y - if m <= 2 { 1 } else { 0 };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = m as i64;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(super) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    (y, m as u32, d as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    #[test]
    fn gantt_today_marker_style_position_and_visibility() {
        let chart = "gantt\n  dateFormat YYYY-MM-DD\n  A :a, 2026-11-16, 4d";
        let now = days_from_civil(2026, 11, 18) * DAY;
        let render = |directive, now| {
            let source = format!("{chart}\n  {directive}\n");
            let (bytes, _, _) = render_gantt_svg_at(&source, &MermaidConfig::default(), now).unwrap();
            String::from_utf8(bytes).unwrap()
        };
        let default = render("", now);
        let styled = render("todayMarker stroke-width:3px,stroke:#06f,opacity:0.5", now);
        let marker = |svg: &str| {
            let tags = tag_with(svg, r#"data-role="today-marker""#);
            assert_eq!(tags.len(), 1);
            tags[0].to_string()
        };
        assert_eq!(
            marker(&default).replace(r##"stroke="#dc2626" stroke-width="2" opacity="1""##,
                r##"stroke="#0066ff" stroke-width="3" opacity="0.5""##),
            marker(&styled)
        );
        let earlier = marker(&render("", now - DAY));
        assert_ne!(earlier, marker(&default));
        assert!(!render("todayMarker off", now).contains("today-marker"));
        assert!(!render("", now + 30 * DAY).contains("today-marker"));
        assert!(render("todayMarker stroke:red\" onclick=oops", now).contains(r##"stroke="#dc2626""##));
    }

    #[test]
    fn renders_gantt_svg() {
        let src = "\
gantt
  title 開発スケジュール
  dateFormat YYYY-MM-DD
  section 設計
  要件定義 :done, req, 2026-01-01, 5d
  基本設計 :active, des, after req, 7d
  section 実装
  プロトタイプ :crit, impl, after des, 10d
  リリース :milestone, rel, after impl, 1d
";
        let Some(MermaidOutput::Svg {
            bytes,
            width_px,
            height_px,
        }) = render_mermaid(src)
        else {
            panic!("expected svg for gantt");
        };
        assert!(width_px > 400 && height_px > 150);
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(svg.contains("開発スケジュール"));
        assert!(svg.contains("要件定義"));
        assert!(svg.contains("基本設計"));
        assert!(svg.contains("2026-01-01"));
    }

    #[test]
    fn gantt_date_format_weekends_and_dependencies() {
        let formatted = "\
gantt
  dateFormat DD.MM.YYYY
  section S
  開始 :a, 15.04.2026, 2d
";
        let svg = svg_of(formatted);
        assert!(svg.contains("data-start=\"2026-04-15\""), "{svg}");

        let weekends = "\
gantt
  dateFormat YYYY-MM-DD
  excludes weekends
  section S
  作業 :a, 2026-01-02, 5d
";
        let svg = svg_of(weekends);
        assert!(svg.contains("data-start=\"2026-01-02\""), "{svg}");
        assert!(svg.contains("data-nonworking=\"weekend\""), "{svg}");
        // mermaid.js skips excluded days in day durations: Fri 2026-01-02 + 5 work days ends 2026-01-09.
        let chart = parse_gantt_chart(weekends).unwrap();
        assert_eq!(gantt_iso(chart.tasks[0].end), "2026-01-09");

        let after = "\
gantt
  dateFormat YYYY-MM-DD
  section S
  甲 :a, 2026-03-01, 2d
  乙 :b, 2026-03-01, 5d
  丙 :c, after a b, 2d
";
        let svg = svg_of(after);
        assert!(svg.contains("data-start=\"2026-03-06\""), "after a b starts at the later end:\n{svg}");

        let unresolved = "\
gantt
  dateFormat YYYY-MM-DD
  section S
  既知 :done, k, 2026-03-01, 3d
  不明 :after nope, 2d
";
        let svg = svg_of(unresolved);
        assert!(svg.contains("不明"));
        assert!(svg.contains("data-start=\"2026-03-04\""), "{svg}");
        assert!(!svg.contains("2026-01-01"), "a dated chart must not snap an unresolved after to the origin:\n{svg}");

        let only = "\
gantt
  section S
  のみ :after missing, 3d
";
        let svg = svg_of(only);
        assert!(svg.contains("のみ"));
        assert!(svg.contains("data-start=\"2026-01-01\""), "with no dated task the origin stays 2026-01-01:\n{svg}");
    }

    #[test]
    fn gantt_time_formats_durations_and_axis_format() {
        let day = |y, m, d| days_from_civil(y, m, d) * DAY;
        assert_eq!(parse_gantt_date_formatted("17:49", "HH:mm"), Some(day(2026, 1, 1) + 17 * 3600 + 49 * 60));
        assert_eq!(parse_gantt_date_formatted("1906", "YYYY"), Some(day(1906, 1, 1)));
        assert_eq!(parse_gantt_date_formatted("71", "X"), Some(71));
        assert_eq!(parse_gantt_date_formatted("1500", "x"), Some(1));
        assert_eq!(parse_gantt_date_formatted("15.04.2026", "DD.MM.YYYY"), Some(day(2026, 4, 15)));
        assert_eq!(parse_gantt_date_formatted("2026-04-15 09:05", "YYYY-MM-DD HH:mm"), Some(day(2026, 4, 15) + 9 * 3600 + 5 * 60));
        assert_eq!(parse_gantt_date_formatted("1s", "YYYY"), None);
        assert_eq!(parse_gantt_date_formatted("2m", "HH:mm"), None);
        assert_eq!(parse_gantt_duration("10m"), Some(600));
        assert_eq!(parse_gantt_duration("1s"), Some(1));
        assert_eq!(parse_gantt_duration("24h"), Some(DAY));
        assert_eq!(parse_gantt_duration("1.5d"), Some(DAY * 3 / 2));
        assert_eq!(parse_gantt_duration("2w"), Some(14 * DAY));
        assert_eq!(parse_gantt_duration("soon"), None);

        let t = day(2026, 1, 5) + 17 * 3600 + 49 * 60 + 30;
        assert_eq!(gantt_strftime(t, "%H:%M"), "17:49");
        assert_eq!(gantt_strftime(t, "%Y-%m-%d %y"), "2026-01-05 26");
        assert_eq!(gantt_strftime(t, "%a %b %e, %A %B %-d %I%p %j %%"), "Mon Jan  5, Monday January 5 05PM 005 %");
        assert_eq!(gantt_strftime(71, "%s"), "71");
        assert_eq!(gantt_iso(t), "2026-01-05T17:49:30");
        assert_eq!(gantt_iso(day(2026, 1, 5)), "2026-01-05");
    }

    #[test]
    fn gantt_ticks_follow_tick_interval_and_weekday() {
        assert_eq!(parse_tick_interval("1week"), Some((1, TickUnit::Week)));
        assert_eq!(parse_tick_interval("30minute"), Some((30, TickUnit::Minute)));
        assert_eq!(parse_tick_interval("1decade"), None);
        assert_eq!(parse_tick_interval("0day"), None);

        let start = days_from_civil(2026, 1, 1) * DAY; // a Thursday
        let end = start + 30 * DAY;
        let mondays = gantt_ticks(start, end, Some((1, TickUnit::Week)), 1);
        assert_eq!(mondays.len(), 4, "{mondays:?}");
        assert!(mondays.iter().all(|t| (t.div_euclid(DAY) + 4).rem_euclid(7) == 1), "{mondays:?}");
        let sundays = gantt_ticks(start, end, Some((1, TickUnit::Week)), 0);
        assert!(sundays.iter().all(|t| (t.div_euclid(DAY) + 4).rem_euclid(7) == 0), "{sundays:?}");
        let months = gantt_ticks(start, start + 100 * DAY, Some((1, TickUnit::Month)), 0);
        assert_eq!(months.iter().map(|&t| gantt_iso(t)).collect::<Vec<_>>(), ["2026-01-01", "2026-02-01", "2026-03-01", "2026-04-01"]);
        // Without an interval: a handful of aligned ticks at any scale.
        for span in [90, 3600 * 5, DAY * 20, DAY * 400, DAY * 365 * 30] {
            let ticks = gantt_ticks(start, start + span, None, 0);
            assert!((2..=24).contains(&ticks.len()), "{span}: {ticks:?}");
        }

        let svg = svg_of("gantt\n  dateFormat YYYY-MM-DD\n  axisFormat %d/%m\n  tickInterval 1day\n  A :a, 2026-03-01, 3d\n");
        assert_eq!(tag_with(&svg, r#"data-role="tick""#).len(), 4, "{svg}");
        assert!(svg.contains(">01/03<") && svg.contains(">04/03<"), "{svg}");
        // Settings without tasks still draw an (empty) axis.
        assert!(render_mermaid("gantt\n  tickInterval 1week\n  weekday monday\n").is_some());
        // A ~50-day chart should get weekly %Y-%m-%d ticks, like mermaid.js (not one per month).
        let month = svg_of("gantt\n    dateFormat YYYY-MM-DD\n    section S\n        A task :a1, 2014-01-01, 30d\n        Another :after a1, 20d\n");
        for day in ["2014-01-05", "2014-01-12", "2014-01-19", "2014-02-02", "2014-02-16"] {
            assert!(month.contains(&format!(">{day}<")), "{day}: {month}");
        }
    }

    #[test]
    fn gantt_vert_markers_until_and_friday_weekends() {
        let svg = svg_of(
            "gantt\n  dateFormat HH:mm\n  axisFormat %H:%M\n  Initial vert : vert, v1, 17:30, 2m\n  Task A : 3m\n  Task B : 8m\n  Final vert : vert, v2, 17:58, 4m\n",
        );
        let verts = tag_with(&svg, r#"data-role="vert""#);
        assert_eq!(verts.len(), 2, "{svg}");
        assert!(verts[0].contains(r#"data-start="2026-01-01T17:30""#), "{svg}");
        assert!(svg.contains(">Initial vert<") && svg.contains(">Final vert<"), "{svg}");
        assert!(!svg.contains("] Initial vert"), "a vert marker is not a row: {svg}");
        // Task A follows the marker; minutes are kept, not rounded to days.
        assert!(svg.contains(r#"data-start="2026-01-01T17:32""#) && svg.contains(r#"data-start="2026-01-01T17:35""#), "{svg}");
        assert!(svg.contains(">17:30<") || svg.contains(">17:35<"), "{svg}");

        // `until` may name a later task.
        let chart = parse_gantt_chart("gantt\n  dateFormat YYYY-MM-DD\n  a :a, 2026-01-01, 2d\n  b :b, 2026-01-02, until c\n  c :c, 2026-01-06, 1d\n").unwrap();
        assert_eq!(chart.tasks[1].end, chart.tasks[2].start);
        let unknown = parse_gantt_chart("gantt\n  dateFormat YYYY-MM-DD\n  b :b, 2026-01-02, until nope\n").unwrap();
        assert_eq!(unknown.tasks[0].end - unknown.tasks[0].start, DAY);

        let fri = "gantt\n  dateFormat YYYY-MM-DD\n  excludes weekends\n  weekend friday\n  A :a, 2026-01-05, 7d\n";
        assert_eq!(parse_gantt_chart(fri).unwrap().excluded_weekdays, vec![5, 6]);
        let svg = svg_of(fri);
        let shaded = tag_with(&svg, r#"data-nonworking="weekend""#);
        assert_eq!(shaded.len(), 2, "Friday and Saturday: {svg}");
        let days = svg_of("gantt\n  dateFormat YYYY-MM-DD\n  excludes 2026-01-07, sunday\n  A :a, 2026-01-05, 7d\n");
        assert_eq!(tag_with(&days, r#"data-nonworking="date""#).len(), 1, "{days}");
        assert_eq!(tag_with(&days, r#"data-nonworking="weekend""#).len(), 1, "{days}");

        // An impossible work calendar falls back instead of looping forever.
        let impossible = "gantt\n  dateFormat YYYY-MM-DD\n  excludes monday,tuesday,wednesday,thursday,friday,saturday,sunday\n  A :a, 2026-01-05, 1d\n";
        assert!(parse_gantt_chart(impossible).is_none());
        assert!(gantt_add_duration(0, 1_000_001 * DAY, &[0], &[]).is_none());

        // Day durations skip excluded days, so a 30d+20d chart with Fri–Sat weekends reaches March.
        let long = svg_of("gantt\n    title A Gantt Diagram Excluding Fri - Sat weekends\n    dateFormat YYYY-MM-DD\n    excludes weekends\n    weekend friday\n    section Section\n        A task          :a1, 2024-01-01, 30d\n        Another task    :after a1, 20d\n");
        for day in ["2024-02-25", "2024-03-03", "2024-03-10"] {
            assert!(long.contains(&format!(">{day}<")), "{day}: {long}");
        }

        // Time-of-day axis is nicened to the next 5-minute tick (17:58 → 18:00).
        let time = svg_of("gantt\n    dateFormat HH:mm\n    axisFormat %H:%M\n    Initial vert : vert, v1, 17:30, 2m\n    Task A : 3m\n    Task B : 8m\n    Final vert : vert, v2, 17:58, 4m\n");
        assert!(time.contains(">18:00<") && time.contains(">17:35<"), "{time}");

        // Unix-second charts (`dateFormat X`) use 5-second ticks up to the end.
        let unix = svg_of("gantt\n    title Git Issues\n    dateFormat X\n    axisFormat %s\n    section S\n    71   : 0, 71\n");
        for tick in ["10", "25", "40", "55", "70"] {
            assert!(unix.contains(&format!(">{tick}<")), "{tick}: {unix}");
        }
    }

    #[test]
    fn gantt_config_width_top_axis_padding_and_section_styles() {
        let body = "gantt\n  dateFormat YYYY-MM-DD\n  section S1\n  A :a, 2026-03-01, 3d\n  section S2\n  B :b, after a, 2d\n  section S3\n  C :c, after b, 2d\n";
        let plain = svg_of(body);
        assert!(!plain.contains("top-axis"));
        let svg = svg_of(&format!(
            "---\nconfig:\n  gantt:\n    useWidth: 400\n    rightPadding: 0\n    topAxis: true\n    numberSectionStyles: 2\n    barHeight: 24\n---\n{body}"
        ));
        assert_eq!(view_box(&svg).0, 400.0, "useWidth sets the whole width");
        let top: Vec<_> = tag_with(&svg, r#"data-role="top-axis""#);
        let bottom_count = tag_with(&svg, r##"fill="#64748b">"##).len();
        assert!(!top.is_empty() && top.len() == bottom_count, "top labels mirror the bottom ones");
        // The rightmost bar ends at the right edge (no right padding).
        let ends: Vec<f32> = tag_with(&svg, "data-start=")
            .iter()
            .filter_map(|t| Some(attr_f32(t, "x")? + attr_f32(t, "width")?))
            .collect();
        assert!(ends.iter().any(|e| (e - 400.0).abs() < 0.6), "{ends:?}");
        assert!(tag_with(&svg, "data-start=").iter().all(|t| t.contains(r#"height="24""#)));
        // Two section styles: S1 and S3 share a colour, S2 differs.
        let rows: Vec<&str> = tag_with(&svg, r#"fill-opacity="0.7""#);
        assert_eq!(rows.len(), 3);
        assert!(rows[0].contains("#eef2ff") && rows[1].contains("#ffffff") && rows[2].contains("#eef2ff"), "{rows:?}");
    }
}
