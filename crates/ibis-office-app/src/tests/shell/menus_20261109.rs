// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Scenarios of the menu repairs of 2026-11-09: the previews of the theme colours and effects in the menus of the three screens, the chart
//! submenu's scroll bar and its pictures' colours, the recommended-charts panel, and the find panel's tooltips. Pictures go to
//! `target/scenario/<name>/NN-<label>.png`.
use crate::shell::chrome::{FindNav, MenuEntry, MenuId, MenuItem};
use crate::shell::scenario::{Scenario, button_probe as buttons};
use crate::xlsx::ext::*;

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::menus_20261109_theme_menus_show_previews", theme_menus),
        ("shell::menus_20261109_chart_submenu", chart_submenu),
        ("shell::menus_20261109_chart_pictures_have_series_colours", chart_pictures_have_series_colours),
        ("shell::menus_20261109_find_tooltips", find_tooltips),
    ]
}

const FILES: [(&str, &str); 3] = [
    ("docx", "test/editing/docx-report.docx"),
    ("xlsx", "test/editing/xlsx-sales.xlsx"),
    ("pptx", "test/benchmark/earnings-presentation.pptx"),
];

fn find_sub<'a>(entries: &'a [MenuEntry], id: &str) -> Option<&'a [MenuEntry]> {
    entries.iter().find_map(|e| match e {
        MenuEntry::Submenu(sub) if sub.id == id => Some(sub.items.as_slice()),
        MenuEntry::Submenu(sub) => find_sub(&sub.items, id),
        _ => None,
    })
}

/// The keys that open the submenu `id`: for each level the number of lines (items and submenus) down to it, then the right arrow.
fn path_to(entries: &[MenuEntry], id: &str) -> Option<Vec<usize>> {
    let focusable = |e: &MenuEntry| match e {
        MenuEntry::Item(item) => item.enabled(),
        MenuEntry::Submenu(_) => true,
        _ => false,
    };
    for (i, entry) in entries.iter().enumerate() {
        if let MenuEntry::Submenu(sub) = entry {
            let downs = entries[..=i].iter().filter(|e| focusable(e)).count();
            if sub.id == id {
                return Some(vec![downs]);
            }
            if let Some(mut rest) = path_to(&sub.items, id) {
                rest.insert(0, downs);
                return Some(rest);
            }
        }
    }
    None
}

/// Open the menu `menu` of the bar and, down the submenus `ids`, the last one, and take a picture.
fn open_and_shoot(s: &mut Scenario, menu: MenuId, id: &str, label: &str) {
    let (index, path) = s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let index = menus.menus.iter().position(|(m, _)| *m == menu).expect("the menu");
        (index, path_to(&menus.menus[index].1, id).unwrap_or_else(|| panic!("no submenu {id}")))
    });
    s.hover((0., 0.));
    s.update(move |e, w, cx| e.open_menu_at(index, w, cx));
    for downs in path {
        for _ in 0..downs {
            s.key("down");
        }
        s.key("right");
    }
    s.settle();
    s.screenshot(label);
}

/// Colours and Effects of every screen: each line has its six colour chips (the accents) or its three effect samples, and a picture of the
/// open submenu is taken.
fn theme_menus() {
    let _probe = buttons::Probe::new();
    // (file, menu holding it, the colours submenu, the effects submenu)
    let cases = [
        ("docx", FILES[0].1, MenuId::Design, "docx.design.theme-colors", "docx.design.effects"),
        ("xlsx", FILES[1].1, MenuId::PageLayout, "xlsx.theme-palette", "xlsx.theme-effects"),
        ("pptx", FILES[2].1, MenuId::Design, "pptx.theme.palette", "pptx.theme.effects"),
    ];
    for (name, file, menu, colors, effects) in cases {
        let mut s = Scenario::open(file);
        s.size(1200., 900.);
        let (n_colors, n_effects) = s.with(|e, _| {
            let menus = e.menu_bar_menus();
            let (_, entries) = menus.menus.iter().find(|(m, _)| *m == menu).unwrap_or_else(|| panic!("{name}: no menu"));
            let lines = |id: &str| -> Vec<MenuItem> {
                find_sub(entries, id).unwrap_or_else(|| panic!("{name}: no submenu {id}")).iter().filter_map(|e| if let MenuEntry::Item(i) = e { Some(i.clone()) } else { None }).collect()
            };
            let (c, f) = (lines(colors), lines(effects));
            for item in &c {
                if item.id.ends_with("customize") || item.id.contains("-colors") && !item.id.contains("theme-colors.") {
                    continue;
                }
                if !item.lead.is_empty() {
                    assert_eq!(item.lead.len(), 6, "{name}: {} shows six colour chips", item.id);
                }
            }
            for item in &f {
                assert_eq!(item.lead.len(), 3, "{name}: {} shows three effect samples", item.id);
            }
            (c.iter().filter(|i| i.lead.len() == 6).count(), f.len())
        });
        assert!(n_colors >= 7, "{name}: the colour sets have chips ({n_colors})");
        assert!(n_effects >= 10, "{name}: the effect schemes have samples ({n_effects})");
        open_and_shoot(&mut s, menu, colors, &format!("{name}-theme-colors"));
        s.key("escape");
        s.key("escape");
        s.hover((0., 0.));
        open_and_shoot(&mut s, menu, effects, &format!("{name}-theme-effects"));
    }
}

/// 挿入 > グラフ of the spreadsheet: the scroll bar lies on the rows of the submenu (not above or below them), and a picture is taken.
fn chart_submenu() {
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 900.);
    open_and_shoot(&mut s, MenuId::Insert, "insert.chart", "chart-submenu");
    let track = buttons::bounds("submenu-scroll-track").expect("the chart submenu is cut and has a scroll bar");
    let first = buttons::bounds("menu-item-xlsx.recommended-charts").expect("the recommended charts line is the first");
    let rows: Vec<_> = buttons::all().into_iter().filter(|id| id.starts_with("menu-item-")).filter_map(|id| buttons::bounds(&id).map(|b| (id, b))).collect();
    assert!(rows.iter().any(|(id, _)| id.starts_with("menu-item-pptx.insert-chart")), "the tiles are shown");
    let (top, bottom) = (f32::from(track.top()), f32::from(track.bottom()));
    assert!(top >= f32::from(first.top()) - 10.0, "the scroll bar starts at the first line ({top} against {:?})", first.top());
    let last_row_bottom = rows.iter().map(|(_, b)| f32::from(b.bottom())).fold(0.0_f32, f32::max);
    let pitch = rows.iter().filter(|(id, _)| id.contains("insert-chart")).map(|(_, b)| f32::from(b.origin.y)).fold(Vec::<f32>::new(), |mut v, y| { if !v.contains(&y) { v.push(y); } v });
    let mut ys = pitch.clone();
    ys.sort_by(|a, b| a.total_cmp(b));
    assert!(ys.windows(2).all(|w| (w[1] - w[0] - crate::shell::menu_tiles::ROW_HEIGHT).abs() < 0.6), "the rows of tiles are ROW_HEIGHT apart: {ys:?}");
    assert!(bottom <= last_row_bottom + 36.0, "the scroll bar ends at the last row shown ({bottom} against {last_row_bottom})");
}

/// 挿入 > グラフ for a long table of numbers (as a log of issues: 600 rows, 4 numeric columns): the pictures use the series' own colours
/// (accent 1 to 4 of the theme), not black.
fn chart_pictures_have_series_colours() {
    use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, SheetCommand};
    let _probe = buttons::Probe::new();
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 900.);
    s.update(|e, _, cx| {
        let slide = e.current_slide;
        let mut edits = Vec::new();
        for row in 0..600u32 {
            for column in 0..4u32 {
                let input = if row == 0 { CellInput::Value(CellValue::Text(format!("series {column}"))) } else { CellInput::Value(CellValue::Number(((row * (column + 3)) % 97 + 20 * column) as f64)) };
                edits.push(CellEdit { row: 20 + row, column: 10 + column, input });
            }
        }
        e.document.execute(vec![Command::Sheet(SheetCommand::SetCells { slide, edits })]).unwrap();
        e.set_selected_ranges(&[CellRange { first_row: 20, last_row: 619, first_column: 10, last_column: 13 }]);
        cx.notify();
    });
    open_and_shoot(&mut s, MenuId::Insert, "insert.chart", "chart-submenu-long-table");
}

/// The find panel's 前へ / 次へ / 全て buttons have tooltips in the toolbar's form (name and key), 全て without a key.
fn find_tooltips() {
    let next = FindNav::Next.tooltip().to_string();
    let previous = FindNav::Previous.tooltip().to_string();
    let all = FindNav::All.tooltip().to_string();
    if cfg!(target_os = "macos") {
        assert!(next.contains("⌘G") && previous.contains("⇧⌘G"), "{next} / {previous}");
    } else {
        assert!(next.contains("Enter") && previous.contains("Shift+Enter"), "{next} / {previous}");
    }
    assert!(next.starts_with(crate::shell::i18n::text("shell.find.next-tip").as_ref()) && !all.contains('（') && !all.contains('('), "{all}");
}
