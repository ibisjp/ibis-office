// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表示形式 > 条件付き書式: Excel's ready-made rules on the selected range
//! (ホーム > 条件付き書式): セルの強調表示ルール, 上位/下位ルール, データ バー,
//! カラー スケール, アイコン セット, the rules cleared, and ルールの管理 (the
//! panel of `conditional_edit.rs`). The colours are Excel's defaults.

use ibis_doc_model::{CfKind, CfRule, CfStyle, CfValue, Command, ConditionalFormat, Rgb, SheetCommand};

use crate::shell::chrome::{MenuEntry, MenuItem, Submenu};
use crate::shell::editor::SlideEditor;

/// 濃い赤の文字、明るい赤の背景: Excel's first format for the quick rules.
pub(crate) fn light_red() -> CfStyle {
    CfStyle { fill: Some(Rgb(0xFFC7CE)), color: Some(Rgb(0x9C0006)), ..CfStyle::default() }
}

fn point(kind: &str, value: Option<&str>) -> CfValue {
    CfValue { kind: kind.into(), value: value.map(str::to_string), gte: true }
}

/// The data bars' colours (Excel's 塗りつぶし (グラデーション) row).
const BARS: [(&str, u32); 6] = [("_shell.blue", 0x638EC6), ("_shell.green", 0x63BE7B), ("_shell.red", 0xF8696B), ("_shell.orange", 0xFFB628), ("_shell.light-blue-2", 0x008AEF), ("_shell.purple", 0xD6007B)];

/// The colour scales: their name and colours, low to high.
const SCALES: [(&str, &[u32]); 8] = [
    ("docx.conditional-quick.green-yellow-red", &[0xF8696B, 0xFFEB84, 0x63BE7B]),
    ("docx.conditional-quick.red-yellow-green", &[0x63BE7B, 0xFFEB84, 0xF8696B]),
    ("docx.conditional-quick.green-white-red", &[0xF8696B, 0xFCFCFF, 0x63BE7B]),
    ("docx.conditional-quick.red-white-green", &[0x63BE7B, 0xFCFCFF, 0xF8696B]),
    ("docx.conditional-quick.blue-white-red", &[0xF8696B, 0xFCFCFF, 0x5A8AC6]),
    ("docx.conditional-quick.red-white-blue", &[0x5A8AC6, 0xFCFCFF, 0xF8696B]),
    ("docx.conditional-quick.white-green", &[0xFCFCFF, 0x63BE7B]),
    ("docx.conditional-quick.green-yellow", &[0xFFEF9C, 0x63BE7B]),
];

/// The icon sets offered: name and `iconSet` value, with its icons' count.
const ICONS: [(&str, &str, u32); 18] = [
    // 方向 (Directional)
    ("docx.conditional-quick.3-arrows-colored", "3Arrows", 3),
    ("_xlsx.3-arrows-gray", "3ArrowsGray", 3),
    ("docx.conditional-quick.4-arrows-colored", "4Arrows", 4),
    ("_xlsx.4-arrows-gray", "4ArrowsGray", 4),
    ("docx.conditional-quick.5-arrows-colored", "5Arrows", 5),
    ("_xlsx.5-arrows-gray", "5ArrowsGray", 5),
    // 図形 (Shapes)
    ("docx.conditional-quick.3-traffic-lights-unrimmed", "3TrafficLights1", 3),
    ("_xlsx.3-traffic-lights-rimmed", "3TrafficLights2", 3),
    ("_xlsx.3-signs", "3Signs", 3),
    ("_xlsx.4-traffic-lights", "4TrafficLights", 4),
    ("_xlsx.4-red-to-black", "4RedToBlack", 4),
    // 指標 (Indicators)
    ("docx.conditional-quick.3-symbols-circled", "3Symbols", 3),
    ("_xlsx.3-symbols-uncircled", "3Symbols2", 3),
    ("docx.conditional-quick.3-flags", "3Flags", 3),
    // 評価 (Ratings)
    ("_xlsx.4-ratings", "4Rating", 4),
    ("docx.conditional-quick.5-ratings", "5Rating", 5),
    ("_xlsx.5-quartiles", "5Quarters", 5),
    ("_xlsx.5-boxes", "5Boxes", 5),
];

impl SlideEditor {
    /// A rule over the selected range (one step to undo).
    pub(crate) fn add_quick_rule(&mut self, kind: CfKind, style: Option<CfStyle>, cx: &mut gpui_kit::Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let format = ConditionalFormat { source_index: None, ranges: vec![range], rules: vec![CfRule { kind, priority: 1, stop: false, style }] };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::AddConditional { slide, format })]) {
            log::warn!("conditional format rejected: {err}");
        }
        cx.notify();
    }

    /// A date-based conditional formatting rule over the selected range (yesterday, today, etc.).
    pub(crate) fn add_quick_date_rule(&mut self, period: &'static str, cx: &mut gpui_kit::Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(range) = self.selected_range() else { return };
        let at = ibis_doc_model::cell_name(range.first_column, range.first_row);
        let formula = match period {
            "yesterday" => format!("FLOOR({at},1)=TODAY()-1"),
            "today" => format!("FLOOR({at},1)=TODAY()"),
            "tomorrow" => format!("FLOOR({at},1)=TODAY()+1"),
            "last7Days" => format!("AND(TODAY()-FLOOR({at},1)<=6,FLOOR({at},1)<=TODAY())"),
            "thisMonth" => format!("AND(MONTH({at})=MONTH(TODAY()),YEAR({at})=YEAR(TODAY()))"),
            "lastMonth" => format!("AND(MONTH({at})=MONTH(EDATE(TODAY(),-1)),YEAR({at})=YEAR(EDATE(TODAY(),-1)))"),
            "nextMonth" => format!("AND(MONTH({at})=MONTH(EDATE(TODAY(),1)),YEAR({at})=YEAR(EDATE(TODAY(),1)))"),
            "thisWeek" => format!("AND(TODAY()-ROUNDDOWN({at},0)<=WEEKDAY(TODAY())-1,ROUNDDOWN({at},0)-TODAY()<=7-WEEKDAY(TODAY()))"),
            "lastWeek" => format!("AND(TODAY()-ROUNDDOWN({at},0)>=WEEKDAY(TODAY()),TODAY()-ROUNDDOWN({at},0)<WEEKDAY(TODAY())+7)"),
            "nextWeek" => format!("AND(ROUNDDOWN({at},0)-TODAY()>7-WEEKDAY(TODAY()),ROUNDDOWN({at},0)-TODAY()<15-WEEKDAY(TODAY()))"),
            _ => return,
        };
        let kind = CfKind::TimePeriod { period: period.to_string(), formula };
        self.add_quick_rule(kind, Some(light_red()), cx);
    }

    /// The rules of the whole sheet taken off.
    pub(crate) fn clear_sheet_conditional(&mut self, cx: &mut gpui_kit::Context<Self>) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let ranges: Vec<_> = sheet.conditional.iter().flat_map(|f| f.ranges.iter().copied()).collect();
        if ranges.is_empty() {
            return;
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::ClearConditional { slide, ranges })]) {
            log::warn!("conditional format rejected: {err}");
        }
        cx.notify();
    }

    /// 指定の値より大きい… and the like: the panel with the comparison
    /// chosen (its `OPERATORS` index) and Excel's format.
    pub(crate) fn open_quick_comparison(&mut self, kind: Option<usize>, formula: Option<String>, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) {
        self.open_conditional(None, window, cx);
        self.set_conditional_choice(kind, formula, window, cx);
    }
}

/// The 条件付き書式 submenu of 表示形式.
pub(crate) fn menu() -> MenuEntry {
    let t = |s: &str| crate::shell::i18n::text(s);
    let compare = |id: &'static str, label: &str, kind: usize| -> MenuEntry { MenuItem::new(id, t(label)).on(move |this, window, cx| this.open_quick_comparison(Some(kind), None, window, cx)).into() };
    let highlight = vec![
        compare("xlsx.cf-greater", "_xlsx.greater-than-3", 5),
        compare("xlsx.cf-less", "_xlsx.less-than-3", 0),
        compare("xlsx.cf-between", "_xlsx.between-2", 6),
        compare("xlsx.cf-equal", "_xlsx.equals-2", 2),
        MenuItem::new("xlsx.cf-text", t("docx.conditional-quick.text-contains"))
            .on(|this, window, cx| {
                let at = this.selected_range().map(|r| ibis_doc_model::cell_name(r.first_column, r.first_row)).unwrap_or_else(|| "A1".into());
                this.open_quick_comparison(None, Some(format!("NOT(ISERROR(SEARCH(\"\",{at})))")), window, cx);
            })
            .into(),
        Submenu::tr(
            "xlsx.cf-date-occurring",
            "calendar_today",
            vec![
                MenuItem::tr("xlsx.cf-date.yesterday").on(|this, _, cx| this.add_quick_date_rule("yesterday", cx)).into(),
                MenuItem::tr("xlsx.cf-date.today").on(|this, _, cx| this.add_quick_date_rule("today", cx)).into(),
                MenuItem::tr("xlsx.cf-date.tomorrow").on(|this, _, cx| this.add_quick_date_rule("tomorrow", cx)).into(),
                MenuItem::tr("xlsx.cf-date.last7days").on(|this, _, cx| this.add_quick_date_rule("last7Days", cx)).into(),
                MenuItem::tr("xlsx.cf-date.last-week").on(|this, _, cx| this.add_quick_date_rule("lastWeek", cx)).into(),
                MenuItem::tr("xlsx.cf-date.this-week").on(|this, _, cx| this.add_quick_date_rule("thisWeek", cx)).into(),
                MenuItem::tr("xlsx.cf-date.next-week").on(|this, _, cx| this.add_quick_date_rule("nextWeek", cx)).into(),
                MenuItem::tr("xlsx.cf-date.last-month").on(|this, _, cx| this.add_quick_date_rule("lastMonth", cx)).into(),
                MenuItem::tr("xlsx.cf-date.this-month").on(|this, _, cx| this.add_quick_date_rule("thisMonth", cx)).into(),
                MenuItem::tr("xlsx.cf-date.next-month").on(|this, _, cx| this.add_quick_date_rule("nextMonth", cx)).into(),
            ],
        )
        .into(),
        MenuItem::new("xlsx.cf-duplicate", t("docx.conditional-quick.duplicate-values")).on(|this, _, cx| this.add_quick_rule(CfKind::Duplicate, Some(light_red()), cx)).into(),
        MenuItem::new("xlsx.cf-unique", t("docx.conditional-quick.unique-values")).on(|this, _, cx| this.add_quick_rule(CfKind::Unique, Some(light_red()), cx)).into(),
    ];
    let top = vec![
        MenuItem::new("xlsx.cf-top10", t("docx.conditional-quick.top-10-items")).on(|this, _, cx| this.add_quick_rule(CfKind::Top10 { rank: 10, percent: false, bottom: false }, Some(light_red()), cx)).into(),
        MenuItem::new("xlsx.cf-top10-percent", t("docx.conditional-quick.top-10-percent")).on(|this, _, cx| this.add_quick_rule(CfKind::Top10 { rank: 10, percent: true, bottom: false }, Some(light_red()), cx)).into(),
        MenuItem::new("xlsx.cf-bottom10", t("docx.conditional-quick.bottom-10-items")).on(|this, _, cx| this.add_quick_rule(CfKind::Top10 { rank: 10, percent: false, bottom: true }, Some(light_red()), cx)).into(),
        MenuItem::new("xlsx.cf-bottom10-percent", t("docx.conditional-quick.bottom-10-percent")).on(|this, _, cx| this.add_quick_rule(CfKind::Top10 { rank: 10, percent: true, bottom: true }, Some(light_red()), cx)).into(),
        MenuItem::new("xlsx.cf-above-average", t("_xlsx.above-average")).on(|this, _, cx| this.add_quick_rule(CfKind::AboveAverage { above: true, equal: false, std_dev: 0 }, Some(light_red()), cx)).into(),
        MenuItem::new("xlsx.cf-below-average", t("_xlsx.below-average")).on(|this, _, cx| this.add_quick_rule(CfKind::AboveAverage { above: false, equal: false, std_dev: 0 }, Some(light_red()), cx)).into(),
    ];
    let mut bars: Vec<MenuEntry> = BARS
        .iter()
        .map(|&(name, color)| -> MenuEntry {
            MenuItem::new("xlsx.cf-bar", t(name))
                .on(move |this, _, cx| this.add_quick_rule(CfKind::data_bar(point("min", None), point("max", None), Rgb(color), true), None, cx))
                .into()
        })
        .collect();
    bars.push(MenuEntry::Separator);
    for &(name, color) in &BARS {
        bars.push(
            MenuItem::new("xlsx.cf-bar-solid", crate::shell::i18n::format("_xlsx.name-solid", &[("name", &t(name))]))
                .on(move |this, _, cx| this.add_quick_rule(CfKind::data_bar(point("min", None), point("max", None), Rgb(color), false), None, cx))
                .into(),
        );
    }
    let scales = SCALES
        .iter()
        .map(|&(name, colors)| -> MenuEntry {
            MenuItem::new("xlsx.cf-scale", t(name))
                .on(move |this, _, cx| {
                    let points: Vec<(CfValue, Rgb)> = match colors {
                        [a, b, c] => vec![(point("min", None), Rgb(*a)), (point("percentile", Some("50")), Rgb(*b)), (point("max", None), Rgb(*c))],
                        [a, b] => vec![(point("min", None), Rgb(*a)), (point("max", None), Rgb(*b))],
                        _ => return,
                    };
                    this.add_quick_rule(CfKind::ColorScale(points), None, cx)
                })
                .into()
        })
        .collect();
    let mut icons: Vec<MenuEntry> = ICONS
        .iter()
        .map(|&(name, set, count)| -> MenuEntry {
            MenuItem::new("xlsx.cf-icons", t(name))
                .on(move |this, _, cx| {
                    let values = (0..count).map(|i| point("percent", Some(&(i * 100 / count).to_string()))).collect();
                    this.add_quick_rule(CfKind::icon_set(set, values, false, true), None, cx)
                })
                .into()
        })
        .collect();
    let reverse_icons: Vec<MenuEntry> = ICONS
        .iter()
        .map(|&(name, set, count)| -> MenuEntry {
            MenuItem::new("xlsx.cf-icons-reverse", crate::shell::i18n::format("_xlsx.name-reverse", &[("name", &t(name))]))
                .on(move |this, _, cx| {
                    let values = (0..count).map(|i| point("percent", Some(&(i * 100 / count).to_string()))).collect();
                    this.add_quick_rule(CfKind::icon_set(set, values, true, true), None, cx)
                })
                .into()
        })
        .collect();
    icons.push(MenuEntry::Separator);
    icons.push(Submenu::tr("xlsx.cf-icons-reverse-menu", "swap_vert", reverse_icons).into());
    Submenu::tr(
        "xlsx.conditional-menu",
        "format_color_fill",
        vec![
            Submenu::tr("xlsx.cf-highlight", "rule", highlight).into(),
            Submenu::tr("xlsx.cf-top-bottom", "leaderboard", top).into(),
            Submenu::tr("xlsx.cf-bars", "bar_chart", bars).into(),
            Submenu::tr("xlsx.cf-scales", "palette", scales).into(),
            Submenu::tr("xlsx.cf-icon-sets", "flag", icons).into(),
            MenuEntry::Separator,
            MenuItem::new("xlsx.cf-clear-selection", t("docx.conditional-quick.clear-rules-selected-cells")).on(|this, _, cx| this.clear_conditional_selection(cx)).into(),
            MenuItem::new("xlsx.cf-clear-sheet", t("docx.conditional-quick.clear-rules-entire-sheet")).on(|this, _, cx| this.clear_sheet_conditional(cx)).into(),
            MenuItem::new("xlsx.conditional",t("xlsx.cf.manage-title")).on(|this, window, cx| this.open_conditional_manager(window, cx)).into(),
        ],
    )
    .into()
}
