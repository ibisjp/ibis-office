// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 機械可読性チェック パネル (GitHub issue #194): ブック全体を [`super::machine_readability`] で調べ、
//! 問題の一覧を作業ウィンドウに出す。「表示」でそのシートのセルへ移る（アクセシビリティ チェックと同じ作り）。

use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::sheet_calc::sheet_of_slide;
use ibis_calc::formula::Axis;
use ibis_doc_model::{ShapeKind, SlideId};

use super::machine_readability::{Finding, cell_ref, check_sheet};
use crate::ext::*;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// シートごと・項目ごとの 1 つの問題。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineIssue {
    pub sheet_id: SlideId,
    pub sheet_name: String,
    pub finding: Finding,
    /// The inner ids of the row and of the column of the first place (`Sheet::line_id`): they go with the cell when rows or columns are inserted or deleted, so
    /// 「表示」 moves to the cell the finding is about wherever it went since the check (K0055).
    pub first_anchor: Option<(u32, u32)>,
}

pub struct MachineReadabilityPanel {
    pub issues: Vec<MachineIssue>,
    pub selected_issue: Option<usize>,
    pub(crate) message: String,
    /// リアルタイム検査 (the setting, read when the panel opens and when the box is pressed): the check is made again when the workbook changes.
    pub realtime: bool,
    /// What the workbook looked like at the last check (`workbook_stamp`).
    pub(crate) stamp: u64,
}

/// The settings key of リアルタイム検査 (`1` / `0`; ON when it is not set).
pub const REALTIME_KEY: &str = "xlsx.machine-readability.realtime-check";

/// Whether リアルタイム検査 is on in the settings.
pub fn realtime_setting() -> bool {
    crate::shell::settings::get(REALTIME_KEY).is_none_or(|v| v != "0")
}

/// A number that changes when something in the workbook changes: its sheets (their order, names and contents; an edit makes a new sheet value).
pub(crate) fn workbook_stamp(p: &ibis_doc_model::Presentation) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    p.slides.len().hash(&mut h);
    for slide in &p.slides {
        slide.id.hash(&mut h);
        slide.name.hash(&mut h);
        slide.shapes.len().hash(&mut h);
        for shape in &slide.shapes {
            if let ShapeKind::Sheet(sheet) = &shape.kind {
                (std::sync::Arc::as_ptr(sheet) as usize).hash(&mut h);
                sheet.cells.len().hash(&mut h);
            }
        }
    }
    h.finish()
}

/// ブック全体の検査。
pub(crate) fn check_workbook(p: &ibis_doc_model::Presentation) -> Vec<MachineIssue> {
    let mut out = Vec::new();
    for (idx, slide) in p.slides.iter().enumerate() {
        let Some(sheet) = sheet_of_slide(slide) else { continue };
        let objects = slide
            .shapes
            .iter()
            .filter(|s| matches!(s.kind, ShapeKind::Picture(_) | ShapeKind::Chart(_) | ShapeKind::Auto(_) | ShapeKind::TextBox(_)))
            .count();
        let name = slide.name.clone().unwrap_or_else(|| format!("Sheet{}", idx + 1));
        for finding in check_sheet(sheet, objects) {
            let first_anchor = finding.first.map(|(row, column)| (sheet.line_id(Axis::Rows, row), sheet.line_id(Axis::Columns, column)));
            out.push(MachineIssue { sheet_id: slide.id, sheet_name: name.clone(), finding, first_anchor });
        }
    }
    out
}

/// Where the finding of `sheet` and `rule` stands in the new `issues` (the same finding kept selected).
fn p_position(issues: &[MachineIssue], sheet: SlideId, rule: super::machine_readability::Rule) -> Option<usize> {
    issues.iter().position(|i| i.sheet_id == sheet && i.finding.rule == rule)
}

/// The Digital Agency's page of the rules and of Harunobu, the tool of the rules: the words 「行政データにおける機械可読性に関するルール」 and 「Harunobu」 are links to them.
pub const RULE_URL: &str = "https://www.digital.go.jp/resources/machine-readability-rule";
pub const TOOL_URL: &str = "https://www.digital.go.jp/resources/machine-readability-rule/harunobu";
const LINK_COLOR: u32 = 0x0f6cbd;

/// The text at the top of the panel: the words of `xlsx.machine.desc` with the two links in blue (`{rule}`, `{tool}`) and the version of Harunobu the check corresponds to (`{version}`).
/// The words at the top of the panel and where its links are (byte range, address).
pub(crate) fn description_parts() -> (String, Vec<(std::ops::Range<usize>, &'static str)>) {
    let template = text("xlsx.machine.desc").to_string().replace("{version}", super::machine_readability::HARUNOBU_VERSION);
    let (rule, tool) = (text("xlsx.machine.rule-link").to_string(), text("xlsx.machine.tool-link").to_string());
    // The links go where the text has {rule} and {tool} (in either order).
    let mut parts: Vec<(usize, &str, &str, &str)> = Vec::new();
    for (key, words, url) in [("{rule}", rule.as_str(), RULE_URL), ("{tool}", tool.as_str(), TOOL_URL)] {
        if let Some(at) = template.find(key) {
            parts.push((at, key, words, url));
        }
    }
    parts.sort_by_key(|p| p.0);
    let mut out = String::new();
    let mut ranges: Vec<(std::ops::Range<usize>, &'static str)> = Vec::new();
    let mut rest = template.as_str();
    for (_, key, words, url) in parts {
        let at = rest.find(key).unwrap_or(0);
        out.push_str(&rest[..at]);
        ranges.push((out.len()..out.len() + words.len(), url));
        out.push_str(words);
        rest = &rest[at + key.len()..];
    }
    out.push_str(rest);
    (out, ranges)
}

fn description_text() -> InteractiveText {
    let (out, ranges) = description_parts();
    let link_style = HighlightStyle {
        color: Some(rgb(LINK_COLOR).into()),
        underline: Some(UnderlineStyle { thickness: px(1.), color: Some(rgb(LINK_COLOR).into()), wavy: false }),
        ..Default::default()
    };
    let spans: Vec<_> = ranges.iter().map(|(r, _)| r.clone()).collect();
    let urls: Vec<&'static str> = ranges.iter().map(|(_, u)| *u).collect();
    InteractiveText::new("machine-readability-description", StyledText::new(out).with_highlights(spans.iter().map(|r| (r.clone(), link_style))))
        .on_click(spans, move |ix, _, cx| crate::shell::links::open_external_url(urls[ix], cx))
}

fn title_of(issue: &MachineIssue) -> String {
    let f = &issue.finding;
    let key = format!("xlsx.machine.{}.title", f.rule.key());
    crate::shell::i18n::format(
        "xlsx.machine.issue-title",
        &[("id", &f.rule.id()), ("sheet", &issue.sheet_name), ("title", &text(&key)), ("count", &f.count.to_string())],
    )
}

fn description_of(issue: &MachineIssue) -> String {
    let f = &issue.finding;
    let key = format!("xlsx.machine.{}.desc", f.rule.key());
    let mut s = text(&key).to_string();
    if !f.examples.is_empty() {
        let list = f.examples.iter().map(|&(r, c)| cell_ref(r, c)).collect::<Vec<_>>().join(", ");
        s.push(' ');
        s.push_str(&crate::shell::i18n::format("xlsx.machine.examples", &[("cells", &list)]));
    }
    s
}

#[ibis_office_macros::editor_ext(MachineReadabilityPanelExt)]
impl SlideEditor {
    /// ツール > 機械可読性チェック… を開く。
    pub fn open_machine_readability_panel(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let issues = check_workbook(self.presentation());
        let stamp = workbook_stamp(self.presentation());
        let panel = MachineReadabilityPanel { issues, selected_issue: None, message: String::new(), realtime: realtime_setting(), stamp };
        let state = self.wb_mut();
        state.close_settings();
        state.panels.machine_readability = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    pub fn close_machine_readability_panel(&mut self, cx: &mut Context<Self>) {
        self.wb_mut().panels.machine_readability = None;
        self.sidebar_open = false;
        cx.notify();
    }

    pub fn recheck_machine_readability(&mut self, cx: &mut Context<Self>) {
        let issues = check_workbook(self.presentation());
        let stamp = workbook_stamp(self.presentation());
        if let Some(p) = self.wb_mut().panels.machine_readability.as_mut() {
            p.issues = issues;
            p.selected_issue = None;
            p.stamp = stamp;
            p.message = text("xlsx.machine.recheck-done").to_string();
            cx.notify();
        }
    }

    /// リアルタイム検査: the box is pressed (kept in the settings); turned on, the check is made now.
    pub fn set_machine_readability_realtime(&mut self, on: bool, cx: &mut Context<Self>) {
        crate::shell::settings::set(REALTIME_KEY, if on { "1" } else { "0" });
        let issues = on.then(|| check_workbook(self.presentation()));
        let stamp = workbook_stamp(self.presentation());
        if let Some(p) = self.wb_mut().panels.machine_readability.as_mut() {
            p.realtime = on;
            if let Some(issues) = issues {
                p.issues = issues;
                p.selected_issue = None;
                p.stamp = stamp;
                p.message = String::new();
            }
        }
        cx.notify();
    }

    /// リアルタイム検査 on: the check is made again when the workbook changed since the last one (a cell, a row or a column, a sheet added or deleted, undo): the findings
    /// that are no longer true go. Called where the panel is made (each drawing), so it costs a look at the sheets' values, not a check, while nothing changed.
    fn follow_machine_readability(&mut self) {
        let stamp = workbook_stamp(self.presentation());
        let Some(panel) = self.wb().panels.machine_readability.as_ref() else { return };
        if !panel.realtime || panel.stamp == stamp {
            return;
        }
        let selected = panel.selected_issue.and_then(|i| panel.issues.get(i)).map(|i| (i.sheet_id, i.finding.rule));
        let issues = check_workbook(self.presentation());
        if let Some(p) = self.wb_mut().panels.machine_readability.as_mut() {
            p.selected_issue = selected.and_then(|(sheet, rule)| p_position(&issues, sheet, rule));
            p.issues = issues;
            p.stamp = stamp;
            p.message = String::new();
        }
    }

    /// 問題のあるシートと、最初の場所のセルへ移る。
    pub fn jump_to_machine_readability_issue(&mut self, issue_idx: usize, cx: &mut Context<Self>) {
        let Some(issue) = self.wb().panels.machine_readability.as_ref().and_then(|p| p.issues.get(issue_idx).cloned()) else { return };
        self.set_current_slide(issue.sheet_id);
        // The cell the finding was about, found by its row and column ids: it moved when rows or columns were inserted or deleted after the check.
        let place = match (issue.first_anchor, self.presentation().slide(issue.sheet_id).and_then(sheet_of_slide)) {
            (Some((row_id, column_id)), Some(sheet)) => sheet.line_index(Axis::Rows, row_id).zip(sheet.line_index(Axis::Columns, column_id)),
            _ => None,
        };
        if let Some((row, col)) = place {
            let wb = self.wb_mut();
            wb.cells.insert(issue.sheet_id, (col, row));
            wb.cell_ends.remove(&issue.sheet_id);
        }
        if let Some(p) = self.wb_mut().panels.machine_readability.as_mut() {
            p.selected_issue = Some(issue_idx);
        }
        cx.notify();
    }

    pub fn machine_readability_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.follow_machine_readability();
        let panel = self.wb().panels.machine_readability.as_ref()?;
        let issues = panel.issues.clone();
        let selected = panel.selected_issue;
        let msg = panel.message.clone();
        let realtime = panel.realtime;

        let header = div().text_sm().text_color(cx.theme().muted_foreground).child(description_text()).child(
            div().pt_2().child(crate::shell::chrome::checkbox_button("machine-realtime", text("xlsx.machine.realtime"), realtime, false, cx.listener(move |this, _, _, cx| this.set_machine_readability_realtime(!realtime, cx)))),
        );
        let clean_box = issues.is_empty().then(|| {
            v_flex()
                .p_3()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(cx.theme().primary.opacity(0.3))
                .bg(cx.theme().primary.opacity(0.08))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().text_color(cx.theme().primary).child(text("xlsx.machine.passed").to_string()))
                .child(div().text_sm().text_color(cx.theme().muted_foreground).child(text("xlsx.machine.passed-sub").to_string()))
        });

        let mut list = v_flex().gap_2();
        for (ix, issue) in issues.iter().enumerate() {
            let is_selected = selected == Some(ix);
            let has_place = issue.finding.first.is_some();
            let item = v_flex()
                .w_full()
                .min_w_0()
                .overflow_hidden()
                .p_2()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_color(if is_selected { cx.theme().primary } else { cx.theme().border })
                .bg(if is_selected { cx.theme().primary.opacity(0.08) } else { cx.theme().background })
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(div().font_weight(FontWeight::SEMIBOLD).flex_1().min_w_0().truncate().text_sm().child(title_of(issue)))
                        .when(has_place, |row| {
                            row.child(
                                crate::shell::chrome::action_button(("jump-machine-issue", ix))
                                    .label(text("_shell.view"))
                                    .small()
                                    .on_click(cx.listener(move |this, _, _window, cx| this.jump_to_machine_readability_issue(ix, cx))),
                            )
                        }),
                )
                .child(div().text_sm().text_color(cx.theme().muted_foreground).min_w_0().overflow_hidden().child(description_of(issue)));
            list = list.child(item);
        }

        let note = div().text_xs().text_color(cx.theme().muted_foreground).child(text("xlsx.machine.scope-note").to_string());
        let footer = (!realtime).then(|| crate::shell::chrome::action_bar().action(
            crate::shell::chrome::ActionDefinition::new("machine-recheck", "", crate::shell::chrome::ActionButtonKind::Normal)
                .label(text("xlsx.machine.recheck"))
                .on_click(cx.listener(|this, _, _, cx| this.recheck_machine_readability(cx))),
        ));
        let content = v_flex()
            .gap_3()
            .p_0()
            .child(header)
            .when(!msg.is_empty(), |el| el.child(div().p_2().rounded_md().bg(cx.theme().primary.opacity(0.1)).text_sm().text_color(cx.theme().primary).child(msg)))
            .children(clean_box)
            .child(list)
            .child(note)
            .child(div().h_2());
        // リアルタイム検査 on: the check follows the workbook, so there is no 再検査 (the action bar area is hidden).
        Some(match footer {
            Some(footer) => SidebarPanel::with_footer(text("xlsx.machine.title"), content.into_any_element(), footer),
            None => SidebarPanel { title: text("xlsx.machine.title").into(), body: content.into_any_element() },
        })
    }
}
