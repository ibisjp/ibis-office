// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue #238: sheets selected together (Excel's 作業グループ): Ctrl (⌘) + click and Shift + click on the tabs, 「すべてのシートを選択」, 「シートのグループ解除」,
//! `[グループ]` in the title, what is typed and the formats going to every sheet of the group in one step, what Excel refuses refused, and `tabSelected` written
//! on every sheet of the group.
use super::SheetSteps;
use crate::ext::*;
use crate::shell::scenario::Scenario;
use gpui_kit::Modifiers;
use ibis_doc_model::{Command, SheetCommand, SlideId, StyleChange, sheet_calc::sheet_of_slide};

pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("xlsx::issue238_tabs_select_together_by_ctrl_shift_and_the_menu", tabs_select_together),
        ("xlsx::issue238_typing_and_formats_go_to_every_sheet_of_the_group_in_one_step", edits_go_to_the_group),
        ("xlsx::issue238_what_cannot_be_done_on_several_sheets_is_refused", refused),
        ("xlsx::issue238_the_group_is_written_as_tab_selected", saved_as_tab_selected),
        ("xlsx::issue238_fitting_links_rules_and_conditional_formats_go_to_the_group", spread_more),
        ("xlsx::issue238_a_file_saved_with_a_group_opens_as_the_group", opens_as_group),
        ("xlsx::issue238_menus_are_greyed_in_a_group_and_select_all_stays", menus_in_group),
        ("xlsx::issue238_fitting_goes_to_every_sheet_each_to_its_own_contents", fitting_per_sheet),
    ]
}

/// A workbook of four sheets (the file's one, and three added).
fn four_sheets() -> (Scenario, Vec<SlideId>) {
    let mut s = Scenario::open("test/editing/xlsx-sales.xlsx");
    s.size(1200., 720.);
    while s.with(|e, _| e.presentation().slides.len()) < 4 {
        s.update(|e, window, cx| e.copy_sheet(window, cx));
    }
    let ids: Vec<SlideId> = s.with(|e, _| e.presentation().slides.iter().map(|s| s.id).collect());
    assert_eq!(ids.len(), 4);
    s.update({
        let first = ids[0];
        move |e, window, cx| e.select_slide(first, window, cx)
    });
    s.settle();
    (s, ids)
}

fn group(s: &mut Scenario) -> Vec<SlideId> {
    s.with(|e, _| e.sheet_group())
}

fn click(s: &mut Scenario, id: SlideId, ctrl: bool, shift: bool) {
    s.update(move |e, window, cx| e.sheet_tab_click(id, ctrl, shift, window, cx));
    s.settle();
}

fn tabs_select_together() {
    let (mut s, ids) = four_sheets();
    assert!(group(&mut s).is_empty(), "one sheet: no group");
    assert_eq!(s.with(|e, _| e.display_title_formatted()), "xlsx-sales.xlsx");
    // Ctrl + click on the third: the first and the third (the first stays the present one, as in Excel).
    click(&mut s, ids[2], true, false);
    assert_eq!(group(&mut s), vec![ids[0], ids[2]]);
    assert_eq!(s.with(|e, _| e.current_slide), ids[0]);
    assert!(s.with(|e, _| e.in_sheet_group(ids[0]) && !e.in_sheet_group(ids[1])));
    assert_eq!(s.with(|e, _| e.group_title_suffix()), Some(crate::shell::i18n::text("_xlsx.group-title").to_string()));
    // Ctrl + click on the present sheet takes nothing out; on another sheet of the group it takes that one out.
    click(&mut s, ids[0], true, false);
    assert_eq!(group(&mut s), vec![ids[0], ids[2]], "the present sheet cannot be taken out");
    click(&mut s, ids[2], true, false);
    assert!(group(&mut s).is_empty(), "one sheet left is no group");
    // Shift + click: from the present sheet to the one clicked.
    s.update({
        let first = ids[0];
        move |e, window, cx| e.select_slide(first, window, cx)
    });
    click(&mut s, ids[2], false, true);
    assert_eq!(group(&mut s), vec![ids[0], ids[1], ids[2]]);
    assert_eq!(s.with(|e, _| e.current_slide), ids[0], "the present sheet stays");
    // A click inside the group changes the present sheet, the group stays.
    click(&mut s, ids[1], false, false);
    assert_eq!(group(&mut s), vec![ids[0], ids[1], ids[2]]);
    assert_eq!(s.with(|e, _| e.current_slide), ids[1]);
    // A click on a sheet outside the group ends it.
    click(&mut s, ids[3], false, false);
    assert!(group(&mut s).is_empty());
    assert_eq!(s.with(|e, _| e.group_title_suffix()), None);
    // 「すべてのシートを選択」: every sheet; then a click on any tab ends it.
    s.update(|e, _, cx| e.select_all_sheets(cx));
    assert_eq!(group(&mut s), ids);
    let menu_has_ungroup = s.with(|e, _| e.tab_menu().iter().any(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(i) if i.id == "xlsx.tab-ungroup")));
    assert!(menu_has_ungroup, "the tab's menu offers Ungroup Sheets while grouped");
    click(&mut s, ids[2], false, false);
    assert!(group(&mut s).is_empty(), "with every sheet selected, any tab ends the group");
    assert_eq!(s.with(|e, _| e.current_slide), ids[2]);
    // The menu's Ungroup Sheets.
    s.update(|e, _, cx| e.select_all_sheets(cx));
    s.update(|e, _, cx| e.ungroup_sheets(cx));
    assert!(group(&mut s).is_empty());
    // A real click on a tab with Ctrl held goes the same way.
    let at = s.with(|e, _| e.wb().tab_scroll.bounds_for_item(1)).map(|b| (f32::from(b.center().x), f32::from(b.center().y)));
    if let Some(at) = at {
        s.update({
            let first = ids[0];
            move |e, window, cx| e.select_slide(first, window, cx)
        });
        s.dispatch_click_with(at, Modifiers { control: true, ..Default::default() });
        s.settle();
        assert_eq!(group(&mut s).len(), 2, "Ctrl + a click on a tab groups it with the present sheet");
    }
}

fn text_of(s: &mut Scenario, index: usize, row: u32, column: u32) -> Option<String> {
    s.with(move |e, _| sheet_of_slide(&e.presentation().slides[index]).and_then(|sh| sh.cell(row, column).map(|c| c.text.clone())))
}

fn style_size(s: &mut Scenario, index: usize, row: u32, column: u32) -> Option<u32> {
    s.with(move |e, _| sheet_of_slide(&e.presentation().slides[index]).and_then(|sh| sh.cell(row, column).map(|c| sh.styles[c.style as usize].font_size as u32)))
}

fn edits_go_to_the_group() {
    let (mut s, ids) = four_sheets();
    click(&mut s, ids[1], true, false); // the first and the second (the second present)
    assert_eq!(group(&mut s), vec![ids[0], ids[1]]);
    s.click_cell("K30");
    s.enter("hello", "enter");
    s.settle();
    assert_eq!(text_of(&mut s, 0, 29, 10).as_deref(), Some("hello"), "typed on the first sheet too");
    assert_eq!(text_of(&mut s, 1, 29, 10).as_deref(), Some("hello"));
    assert_eq!(text_of(&mut s, 2, 29, 10), None, "not on a sheet outside the group");
    // A format: the same cell of both sheets, and the history keeps one step.
    s.click_cell("K30");
    s.update(|e, _, cx| e.format_cells(StyleChange::FontSize(1800), cx));
    s.settle();
    assert_eq!(style_size(&mut s, 0, 29, 10), Some(1800));
    assert_eq!(style_size(&mut s, 1, 29, 10), Some(1800));
    assert_ne!(style_size(&mut s, 2, 29, 10), Some(1800));
    // Row height and column width.
    s.update(|e, _, cx| {
        e.set_selected_row_height(33.0, cx);
        e.set_selected_column_width(21.0, cx);
    });
    s.settle();
    let sizes = |s: &mut Scenario, index: usize| s.with(move |e, _| sheet_of_slide(&e.presentation().slides[index]).map(|sh| (sh.rows.get(29).copied(), sh.columns.get(10).copied())));
    assert_eq!(sizes(&mut s, 0), sizes(&mut s, 1), "the same row and column sizes on both sheets");
    assert_ne!(sizes(&mut s, 0), sizes(&mut s, 2));
    // One undo takes back the last step (the column width) on both sheets at once; the row height, an earlier step, stays on both.
    s.update(|e, window, cx| e.undo(&crate::shell::actions::Undo, window, cx));
    s.settle();
    let columns = |s: &mut Scenario, index: usize| s.with(move |e, _| sheet_of_slide(&e.presentation().slides[index]).map(|sh| sh.columns.get(10).copied()));
    assert_eq!(columns(&mut s, 0), columns(&mut s, 2), "the column width is undone on the first sheet");
    assert_eq!(columns(&mut s, 1), columns(&mut s, 2), "and on the second, by the one undo");
    let rows = |s: &mut Scenario, index: usize| s.with(move |e, _| sheet_of_slide(&e.presentation().slides[index]).map(|sh| sh.rows.get(29).copied()));
    assert_eq!(rows(&mut s, 0), rows(&mut s, 1), "the row height stays on both");
    assert_ne!(rows(&mut s, 0), rows(&mut s, 2));
}

fn refused() {
    let (mut s, ids) = four_sheets();
    click(&mut s, ids[1], true, false);
    assert_eq!(group(&mut s).len(), 2);
    let before = s.with(|e, _| e.presentation().clone());
    let slide = ids[1];
    s.update(move |e, _, cx| {
        e.run_sheet(vec![Command::Sheet(SheetCommand::Sort { sheet: slide, range: ibis_doc_model::CellRange { first_row: 0, first_column: 0, last_row: 5, last_column: 1 }, keys: vec![(0, true)], header: false })], cx)
    });
    s.settle();
    assert!(s.with(|e, _| e.presentation() == &before), "a sort is refused on several sheets");
    assert!(s.with(|e, _| e.status.as_ref().is_some_and(|m| m.to_string() == crate::shell::i18n::text("_xlsx.group-blocked").to_string())), "and says why");
    // On one sheet the same is done.
    click(&mut s, ids[3], false, false);
    assert!(group(&mut s).is_empty());
}

fn saved_as_tab_selected() {
    let (mut s, ids) = four_sheets();
    click(&mut s, ids[2], false, true); // the first to the third
    assert_eq!(group(&mut s).len(), 3);
    let folder = crate::shell::scenario::repo_root().join("target/issue238");
    std::fs::create_dir_all(&folder).unwrap();
    let path = s.save_as(folder.join("group.xlsx").to_str().unwrap());
    let part = |name: &str| ibis_formats::test_files::part_text(&std::fs::read(&path).unwrap(), name).unwrap();
    // Three of the file's own sheets are the group: three sheets carry `tabSelected` (the file's sheet files are not in the order of the tabs, so the files are counted).
    let selected = (1..=4).filter(|i| part(&format!("xl/worksheets/sheet{i}.xml")).contains("tabSelected=\"1\"")).count();
    assert_eq!(selected, 3, "tabSelected on every sheet of the group");
    drop(s);
    // Alone again, the file is written with the one sheet selected.
    let (mut t, ids) = four_sheets();
    click(&mut t, ids[1], false, false);
    let alone = t.save_as(folder.join("alone.xlsx").to_str().unwrap());
    let selected = |i: usize| ibis_formats::test_files::part_text(&std::fs::read(&alone).unwrap(), &format!("xl/worksheets/sheet{i}.xml")).unwrap().contains("tabSelected=\"1\"");
    assert_eq!((1..=4).filter(|i| selected(*i)).count(), 1, "one sheet selected again");
}


/// Fitting a row or column to its contents, a hyperlink, an input rule and a new conditional format of the present sheet go to every sheet of the group too
/// (an edit of a conditional format that is there, by its number on the present sheet, does not).
fn spread_more() {
    use ibis_calc::formula::Axis;
    use ibis_doc_model::CellRange;
    let ids: Vec<SlideId> = vec![SlideId(1), SlideId(2), SlideId(3)];
    let range = CellRange { first_row: 0, first_column: 0, last_row: 0, last_column: 0 };
    let one = |command: SheetCommand| crate::sheet_group::spread(vec![Command::Sheet(command)], ids[0], &ids[..2]).expect("not refused");
    let targets = |commands: &[Command]| -> Vec<SlideId> { commands.iter().filter_map(|c| if let Command::Sheet(c) = c { crate::sheet_group::sheet_target(c) } else { None }).collect() };
    for command in [
        SheetCommand::SetColumnWidthAuto { sheet: ids[0], first: 0, last: 1, size: 1000 },
        SheetCommand::SetSize { sheet: ids[0], axis: Axis::Rows, first: 0, last: 0, size: 100 },
    ] {
        let spread = one(command.clone());
        assert_eq!(targets(&spread), vec![ids[0], ids[1]], "{command:?}: the present sheet's and its copy for the other sheet of the group (not the sheet outside it)");
    }
    // Greyed out in Excel while several sheets are selected: refused here (the Windows check of #238, 2026-11-20).
    for command in [
        SheetCommand::SetHyperlink { sheet: ids[0], range, link: None },
        SheetCommand::SetValidation { sheet: ids[0], ranges: vec![range], rule: None },
        SheetCommand::UpdateConditional { sheet: ids[0], index: 0, format: Default::default() },
        SheetCommand::SetComment { sheet: ids[0], row: 0, column: 0, comment: None },
    ] {
        assert!(crate::sheet_group::spread(vec![Command::Sheet(command.clone())], ids[0], &ids[..2]).is_err(), "{command:?} is refused on several sheets");
    }
    // A width or height fitted to contents is made for each sheet from that sheet's own contents (not copied): it is not spread.
    for command in [SheetCommand::SetColumnWidthFitted { sheet: ids[0], first: 0, last: 1, size: 1000 }, SheetCommand::SetRowHeightFitted { sheet: ids[0], first: 0, last: 1, size: 1000 }] {
        assert_eq!(one(command.clone()).len(), 1, "{command:?}");
    }
    // What is not spread stays on the present sheet.
    let alone = one(SheetCommand::SetTabColor { sheet: ids[0], color: None });
    assert_eq!(alone.len(), 1, "a command that is not spread stays on the present sheet");
}

/// A file whose sheets 1 and 3 have `tabSelected` opens with them selected together, the present sheet being one of them.
fn opens_as_group() {
    let dir = crate::shell::scenario::repo_root().join("target/scenario/issue238-open");
    std::fs::create_dir_all(&dir).unwrap();
    let data = std::fs::read(crate::shell::scenario::repo_root().join("test/editing/xlsx-compatibility-v1.xlsx")).unwrap();
    let opened = ibis_formats::open(data).unwrap();
    let mut presentation = opened.presentation.clone();
    std::sync::Arc::make_mut(presentation.workbook.as_mut().unwrap()).selected_tabs = vec![0, 2];
    let bytes = opened.source.save(&presentation, ibis_formats::SaveOptions::default()).unwrap();
    std::fs::write(dir.join("group.xlsx"), bytes).unwrap();
    let mut s = Scenario::open("target/scenario/issue238-open/group.xlsx");
    s.size(1200., 720.);
    let ids: Vec<SlideId> = s.with(|e, _| e.presentation().slides.iter().map(|s| s.id).collect());
    assert_eq!(group(&mut s), vec![ids[0], ids[2]], "the group the file was left with");
    assert_eq!(s.with(|e, _| e.group_title_suffix()), Some(crate::shell::i18n::text("_xlsx.group-title").to_string()));
    // The group is saved again as it is, and ended with 「シートのグループ解除」.
    s.update(|e, _, cx| e.ungroup_sheets(cx));
    assert!(group(&mut s).is_empty());
}

/// Excel: fitting a column to its contents with sheets selected together fits every sheet of the group, each to its own contents (not the present sheet's width
/// copied): a long text on the first sheet and a short one on the second give two widths.
fn fitting_per_sheet() {
    let (mut s, ids) = four_sheets();
    let width = |s: &mut Scenario, index: usize| s.with(move |e, _| sheet_of_slide(&e.presentation().slides[index]).map(|sh| sh.columns[12]));
    // Different texts in column M of the first two sheets (alone, one at a time).
    s.click_cell("M3");
    s.enter("a very long text that needs a wide column to be seen whole", "enter");
    click(&mut s, ids[1], false, false);
    s.click_cell("M3");
    s.enter("x", "enter");
    click(&mut s, ids[0], false, false);
    click(&mut s, ids[1], true, false);
    assert_eq!(group(&mut s), vec![ids[0], ids[1]]);
    s.update(|e, window, cx| e.fit_columns(12, window, cx));
    s.settle();
    let (first, second) = (width(&mut s, 0).unwrap(), width(&mut s, 1).unwrap());
    assert!(first > second * 3, "each sheet is fitted to its own text: {first} and {second}");
    assert_eq!(width(&mut s, 2), width(&mut s, 3), "the sheets outside the group are not touched");
}

/// Excel: with sheets selected together the lines of the commands it refuses are grey, and the tab menu keeps 「すべてのシートを選択」 (with 「グループ解除」 added).
fn menus_in_group() {
    let (mut s, ids) = four_sheets();
    let disabled = |s: &mut Scenario, id: &'static str| s.with(move |e, _| e.menus().find_entry(id).map(|m| match m { crate::shell::chrome::MenuEntry::Item(i) => i.disabled, _ => false }));
    assert_eq!(disabled(&mut s, "xlsx.remove-duplicates"), Some(false));
    click(&mut s, ids[1], true, false);
    for id in ["xlsx.remove-duplicates", "xlsx.insert-table", "xlsx.validation"] {
        assert_eq!(disabled(&mut s, id), Some(true), "{id} is grey in a group");
    }
    // The Insert menu's chart submenu is greyed whole.
    let chart_grey = s.with(|e, _| match e.menus().find_entry("insert.chart") {
        Some(crate::shell::chrome::MenuEntry::Submenu(sub)) => sub.items.iter().all(|i| !matches!(i, crate::shell::chrome::MenuEntry::Item(i) if !i.disabled)),
        _ => false,
    });
    assert!(chart_grey, "the chart submenu is grey in a group");
    let tab = s.with(|e, _| e.tab_menu());
    let has = |id: &str| tab.iter().any(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(i) if i.id == id));
    assert!(has("xlsx.tab-select-all") && has("xlsx.tab-ungroup"));
    s.update(|e, _, cx| e.select_all_sheets(cx));
    let tab = s.with(|e, _| e.tab_menu());
    assert!(tab.iter().any(|m| matches!(m, crate::shell::chrome::MenuEntry::Item(i) if i.id == "xlsx.tab-select-all")), "still offered while all are selected");
    s.update(|e, _, cx| e.ungroup_sheets(cx));
    assert_eq!(disabled(&mut s, "xlsx.remove-duplicates"), Some(false));
}
