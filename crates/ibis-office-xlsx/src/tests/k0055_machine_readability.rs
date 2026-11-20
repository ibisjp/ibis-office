// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! K0055 (GitHub issue #194): the machine readability check of the workbook: the Review menu's order and the menu's words, the panel's description with its two links, the
//! Window menu's sidebar list, a finding that finds its cell again after rows are deleted (the cell's row and column ids), and リアルタイム検査.
//! The file is the issue's attachment (`test/github-issue/194/`, not in the repository): without it a scenario passes at once.
use super::SheetSteps;
use crate::ext::*;
use crate::machine_readability::Rule;
use crate::machine_readability_panel::{REALTIME_KEY, description_parts, realtime_setting};
use crate::shell::scenario::Scenario;

const FILE: &str = "test/github-issue/194/machine-readability-sample.xlsx";

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::k0055_review_menu_order_and_the_machine_readability_words", review_menu_order_and_words),
        ("xlsx::k0055_the_description_has_two_links_and_the_version", description_has_two_links),
        ("xlsx::k0055_the_panel_is_a_sidebar_of_the_window_menu", panel_is_a_sidebar),
        ("xlsx::k0055_a_finding_finds_its_cell_after_rows_are_deleted", finding_follows_deleted_rows),
        ("xlsx::k0055_realtime_check_follows_the_workbook", realtime_check),
    ]
}

fn have_file() -> bool {
    crate::shell::scenario::repo_root().join(FILE).exists()
}

/// The ids of the Review menu of the open workbook, in order (the separators as "-").
fn review_ids(s: &mut Scenario) -> Vec<String> {
    use crate::shell::chrome::{MenuEntry, MenuId};
    s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let review = menus.menus.iter().find(|(id, _)| *id == MenuId::Review).map(|(_, entries)| entries.clone()).unwrap_or_default();
        review
            .iter()
            .map(|entry| match entry {
                MenuEntry::Item(item) => item.id.to_string(),
                MenuEntry::Submenu(sub) => sub.id.to_string(),
                _ => "-".to_string(),
            })
            .collect()
    })
}

/// The protection first (シートの保護 to 数式を表示しない), then the proofing (スペルチェック to オートコレクト), then the checks (アクセシビリティ to 互換性チェック): K0055 (1).
fn review_menu_order_and_words() {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let ids = review_ids(&mut s);
    let at = |id: &str| ids.iter().position(|x| x == id).unwrap_or_else(|| panic!("{id} in {ids:?}"));
    let order = ["xlsx.protect-sheet", "xlsx.allow-edit-ranges", "xlsx.lock-cells", "xlsx.hide-formulas", "xlsx.spelling.check", "xlsx.thesaurus", "xlsx.autocorrect", "review.accessibility-check", "xlsx.machine-readability-check", "review.compatibility-check"];
    for pair in order.windows(2) {
        assert!(at(pair[0]) < at(pair[1]), "{} before {} in {ids:?}", pair[0], pair[1]);
    }
    // A separator between the three sections.
    let has_sep = |a: &str, b: &str| ids[at(a)..at(b)].iter().any(|x| x == "-");
    assert!(has_sep("xlsx.hide-formulas", "xlsx.spelling.check") && has_sep("xlsx.autocorrect", "review.accessibility-check"));
    // K0055 (2): the words of the menu item.
    let label = crate::shell::i18n::text("xlsx.machine-readability-check").to_string();
    assert!(label.starts_with("デジタル庁: ") || label.starts_with("Digital Agency: "), "{label}"); // i18n-ok（試験の期待する文言）
    let tsv = |lang: &str| std::fs::read_to_string(crate::shell::scenario::repo_root().join(format!("crates/ibis-office-base/i18n/{lang}/xlsx.tsv"))).unwrap();
    assert!(tsv("ja").contains("xlsx.machine-readability-check\tデジタル庁: 機械可読性チェック…")); // i18n-ok（試験の期待する文言）
    assert!(tsv("en").contains("xlsx.machine-readability-check\tDigital Agency: Check Machine Readability…"));
}

/// K0055 (3): the words 「行政データにおける機械可読性に関するルール」 and 「Harunobu」 are links (the Digital Agency's pages), and the version is in the text.
fn description_has_two_links() {
    let (text, links) = description_parts();
    assert_eq!(links.len(), 2, "{text}");
    let word = |i: usize| text[links[i].0.clone()].to_string();
    assert!(word(0) == "行政データにおける機械可読性に関するルール" || word(0) == "Rules on Machine Readability in Administrative Data", "{}", word(0)); // i18n-ok（試験の期待する文言）
    assert_eq!(links[0].1, "https://www.digital.go.jp/resources/machine-readability-rule");
    assert_eq!(word(1), "Harunobu");
    assert_eq!(links[1].1, "https://www.digital.go.jp/resources/machine-readability-rule/harunobu");
    assert!(text.contains(&format!("ver.{}", crate::machine_readability::HARUNOBU_VERSION)) && !text.contains('{'), "{text}");
    let ja = std::fs::read_to_string(crate::shell::scenario::repo_root().join("crates/ibis-office-base/i18n/ja/xlsx.tsv")).unwrap();
    assert!(ja.contains("xlsx.machine.desc\tデジタル庁の「{rule}」に対応しているかを検査します。 {tool} ver.{version} 相当です。")); // i18n-ok（試験の期待する文言）
}

/// K0055 (4): Window > Sidebar lists the panel and a press opens it.
fn panel_is_a_sidebar() {
    use crate::shell::chrome::{MenuEntry, MenuId};
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    let listed = s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let window = menus.menus.iter().find(|(id, _)| *id == MenuId::Window).map(|(_, entries)| entries.clone()).unwrap_or_default();
        let mut found = false;
        for entry in &window {
            if let MenuEntry::Submenu(sub) = entry {
                if sub.id == "menu.window.sidebar" {
                    found = sub.items.iter().any(|i| matches!(i, MenuEntry::Item(item) if item.id == "sidebar.machine-readability"));
                }
            }
        }
        found
    });
    assert!(listed, "the machine readability panel is in the Window menu's sidebar list");
    // The row's own handler (the Window menu's row): it opens the panel.
    let handler = s.with(|e, _| {
        let menus = e.menu_bar_menus();
        let window = menus.menus.iter().find(|(id, _)| *id == MenuId::Window).map(|(_, entries)| entries.clone()).unwrap_or_default();
        window.iter().find_map(|entry| match entry {
            MenuEntry::Submenu(sub) if sub.id == "menu.window.sidebar" => sub.items.iter().find_map(|i| match i {
                MenuEntry::Item(item) if item.id == "sidebar.machine-readability" => item.handler.clone(),
                _ => None,
            }),
            _ => None,
        })
    });
    let handler = handler.expect("the row has a handler");
    s.update(move |e, window, cx| handler(e, window, cx));
    assert!(s.with(|e, _| e.wb().panels.machine_readability.is_some()), "the row opens the panel");
}

fn rule_index(s: &mut Scenario, rule: Rule) -> usize {
    s.with(|e, _| e.wb().panels.machine_readability.as_ref().and_then(|p| p.issues.iter().position(|i| i.finding.rule == rule)).unwrap_or_else(|| panic!("a finding of {rule:?}")))
}

fn press_view(s: &mut Scenario, rule: Rule) {
    let ix = rule_index(s, rule);
    s.update(move |e, _, cx| e.jump_to_machine_readability_issue(ix, cx));
}

/// K0055 (5): 「表示」 of L1-04 goes to A11 and L1-03's to A2; after row 2 is deleted the findings are kept as they were (リアルタイム検査 off) and 「表示」 goes to the cell the
/// finding was about, which moved up a row (A10), not to the cell that is now where it was.
fn finding_follows_deleted_rows() {
    if !have_file() {
        return;
    }
    crate::shell::settings::set(REALTIME_KEY, "0");
    let mut s = Scenario::open(FILE);
    s.menu("xlsx.machine-readability-check");
    press_view(&mut s, Rule::StrayInfo);
    assert_eq!(s.selected(), "A11");
    press_view(&mut s, Rule::SplitData);
    assert_eq!(s.selected(), "A2");
    s.header_menu("2", "xlsx.header-delete-rows");
    // The findings are those of the check before (no real-time check).
    press_view(&mut s, Rule::StrayInfo);
    assert_eq!(s.selected(), "A10", "L1-04's cell moved up with the deleted row");
    // With リアルタイム検査 on the check is made again and 「表示」 is where the cell is now.
    s.update(|e, _, cx| e.set_machine_readability_realtime(true, cx));
    press_view(&mut s, Rule::StrayInfo);
    assert_eq!(s.selected(), "A10");
    crate::shell::settings::remove(REALTIME_KEY);
}

/// K0055 (6): リアルタイム検査 is on unless the setting says off, is kept in the settings, and the panel is checked again when the workbook changes (a finding that is not true any
/// more goes); off, it stays as it was.
fn realtime_check() {
    crate::shell::settings::remove(REALTIME_KEY);
    assert!(realtime_setting(), "on by default");
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.menu("xlsx.machine-readability-check");
    assert!(s.with(|e, _| e.wb().panels.machine_readability.as_ref().unwrap().realtime));
    let count = |s: &mut Scenario| s.with(|e, _| e.wb().panels.machine_readability.as_ref().unwrap().issues.iter().find(|i| i.finding.rule == Rule::Formula).map_or(0, |i| i.finding.count));
    let before = count(&mut s);
    // A formula typed in a cell: the finding of L2-06 counts it by itself.
    s.click_cell("J20");
    s.enter("=1+1", "enter");
    s.settle();
    assert_eq!(count(&mut s), before + 1, "a formula found while the check follows");
    // The cell is cleared: the finding is not true any more.
    s.click_cell("J20");
    s.key("delete");
    s.settle();
    assert_eq!(count(&mut s), before, "the finding goes when the cell is cleared");
    // Off: kept in the settings, and the panel is not checked again by itself.
    s.update(|e, _, cx| e.set_machine_readability_realtime(false, cx));
    assert_eq!(crate::shell::settings::get(REALTIME_KEY).as_deref(), Some("0"));
    s.click_cell("J21");
    s.enter("=2+2", "enter");
    s.settle();
    assert_eq!(count(&mut s), before, "off: the panel is not checked again by itself");
    s.update(|e, _, cx| e.set_machine_readability_realtime(true, cx));
    assert_eq!(crate::shell::settings::get(REALTIME_KEY).as_deref(), Some("1"));
    assert_eq!(count(&mut s), before + 1, "on again: checked at once");
    crate::shell::settings::remove(REALTIME_KEY);
}
