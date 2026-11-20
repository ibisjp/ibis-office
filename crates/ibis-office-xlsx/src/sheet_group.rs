// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Sheets selected together, as Excel's 作業グループ (GitHub issue #238): Ctrl (⌘ on a Mac) + click on a tab adds or takes away one sheet, Shift + click
//! selects the sheets from the present one to it, 「すべてのシートを選択」 and 「シートのグループ解除」 are in the tab's menu. While the group stands, what is typed
//! and the formats (font, size, row height, column width, alignment, fill, borders, number format) go to the same cells of every sheet in it, in
//! one step of the history. The commands that cannot be done on several sheets are refused as Excel refuses them. The title says `[グループ]`.
//! Saving writes `tabSelected` on every sheet of the group (`Workbook::selected_tabs`).

use std::borrow::Cow;

use ibis_doc_model::{Command, Presentation, SheetCommand, SlideId};

use crate::ext::*;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;
use gpui_kit::*;

/// Whether `command` can be done on several sheets at once (Excel: 「このコマンドは、複数のシートを選択している場合は使用できません」).
fn refused_in_group(command: &SheetCommand) -> bool {
    matches!(
        command,
        SheetCommand::SetTable { .. }
            | SheetCommand::SetTableFilter { .. }
            | SheetCommand::SetDataTable { .. }
            | SheetCommand::ClearDataTable { .. }
            | SheetCommand::Sort { .. }
            | SheetCommand::SortByKeys { .. }
            | SheetCommand::RemoveDuplicates { .. }
    )
}

/// `command` made for the sheet `to`, when it is one that goes to every sheet of the group (typing, formats, sizes, fitting rows and columns to their contents,
/// hyperlinks, input rules, new conditional formats, inserting and deleting rows and columns); `None` for the others (they are done on the present sheet alone). A
/// width or height fitted to the present sheet's contents is the size every sheet of the group gets (the same width on every sheet, as the sheets are meant to be laid
/// alike: the Windows check of this is the request `2026-11-17-238-excel-group.md`).
fn for_sheet(command: &SheetCommand, to: SlideId) -> Option<SheetCommand> {
    let mut copy = match command {
        SheetCommand::SetCells { .. }
        | SheetCommand::SetRuns { .. }
        | SheetCommand::SetFormat { .. }
        | SheetCommand::SetColumnWidthFitted { .. }
        | SheetCommand::SetColumnWidthAuto { .. }
        | SheetCommand::SetRowHeightFitted { .. }
        | SheetCommand::SetHyperlink { .. }
        | SheetCommand::SetValidation { .. }
        | SheetCommand::AddConditional { .. }
        | SheetCommand::PasteCells { .. }
        | SheetCommand::ClearFormats { .. }
        | SheetCommand::SetSize { .. }
        | SheetCommand::Insert { .. }
        | SheetCommand::Delete { .. }
        | SheetCommand::InsertOrDeleteCells { .. } => command.clone(),
        _ => return None,
    };
    match &mut copy {
        SheetCommand::SetCells { sheet, .. }
        | SheetCommand::SetRuns { sheet, .. }
        | SheetCommand::SetFormat { sheet, .. }
        | SheetCommand::PasteCells { sheet, .. }
        | SheetCommand::ClearFormats { sheet, .. }
        | SheetCommand::SetSize { sheet, .. }
        | SheetCommand::SetColumnWidthFitted { sheet, .. }
        | SheetCommand::SetColumnWidthAuto { sheet, .. }
        | SheetCommand::SetRowHeightFitted { sheet, .. }
        | SheetCommand::SetHyperlink { sheet, .. }
        | SheetCommand::SetValidation { sheet, .. }
        | SheetCommand::AddConditional { sheet, .. }
        | SheetCommand::Insert { sheet, .. }
        | SheetCommand::Delete { sheet, .. }
        | SheetCommand::InsertOrDeleteCells { sheet, .. } => *sheet = to,
        _ => return None,
    }
    Some(copy)
}

/// What the commands of the present sheet become with the group: each one, then its copy for each other sheet of the group. `Err` when one of them
/// cannot be done on several sheets.
pub(crate) fn spread(commands: Vec<Command>, present: SlideId, group: &[SlideId]) -> Result<Vec<Command>, ()> {
    let mut out = Vec::with_capacity(commands.len() * group.len());
    for command in commands {
        if let Command::Sheet(sheet_command) = &command {
            if refused_in_group(sheet_command) {
                return Err(());
            }
            let copies: Vec<Command> = if sheet_target(sheet_command) == Some(present) {
                group.iter().filter(|id| **id != present).filter_map(|id| for_sheet(sheet_command, *id)).map(Command::Sheet).collect()
            } else {
                Vec::new()
            };
            out.push(command);
            out.extend(copies);
        } else {
            out.push(command);
        }
    }
    Ok(out)
}

/// The sheet a command is for, for the ones that go to the group.
pub(crate) fn sheet_target(command: &SheetCommand) -> Option<SlideId> {
    match command {
        SheetCommand::SetCells { sheet, .. }
        | SheetCommand::SetRuns { sheet, .. }
        | SheetCommand::SetFormat { sheet, .. }
        | SheetCommand::PasteCells { sheet, .. }
        | SheetCommand::ClearFormats { sheet, .. }
        | SheetCommand::SetSize { sheet, .. }
        | SheetCommand::SetColumnWidthFitted { sheet, .. }
        | SheetCommand::SetColumnWidthAuto { sheet, .. }
        | SheetCommand::SetRowHeightFitted { sheet, .. }
        | SheetCommand::SetHyperlink { sheet, .. }
        | SheetCommand::SetValidation { sheet, .. }
        | SheetCommand::AddConditional { sheet, .. }
        | SheetCommand::Insert { sheet, .. }
        | SheetCommand::Delete { sheet, .. }
        | SheetCommand::InsertOrDeleteCells { sheet, .. } => Some(*sheet),
        _ => None,
    }
}

#[ibis_office_macros::editor_ext(SheetGroup)]
impl SlideEditor {
    /// The sheets that are not hidden, in the order of the tabs.
    fn visible_sheet_ids(&self) -> Vec<SlideId> {
        let wb = self.presentation().workbook.as_deref();
        self.presentation()
            .slides
            .iter()
            .filter(|s| wb.is_none_or(|w| w.sheets.iter().find(|x| x.slide == Some(s.id)).is_none_or(|x| x.hidden.is_none())))
            .map(|s| s.id)
            .collect()
    }

    /// The sheets selected together with the present one, in the order of the tabs: empty when the present sheet is alone.
    pub fn sheet_group(&self) -> Vec<SlideId> {
        let Some(state) = self.sheet_state() else { return Vec::new() };
        if state.group.len() < 2 || !state.group.contains(&self.current_slide) {
            return Vec::new();
        }
        let group: Vec<SlideId> = self.visible_sheet_ids().into_iter().filter(|id| state.group.contains(id)).collect();
        if group.len() < 2 { Vec::new() } else { group }
    }

    /// Whether the tab of `id` is selected with the others (drawn white).
    pub fn in_sheet_group(&self, id: SlideId) -> bool {
        self.sheet_group().contains(&id)
    }

    fn set_group(&mut self, mut group: Vec<SlideId>) {
        let order = self.visible_sheet_ids();
        group.retain(|id| order.contains(id));
        group.sort_by_key(|id| order.iter().position(|o| o == id));
        group.dedup();
        if group.len() < 2 {
            group.clear();
        }
        if let Some(state) = self.sheet_state_mut() {
            state.group = group;
        }
    }

    /// A file saved with several sheets selected (a group) opens as that group, the present sheet being one of them (GitHub issue #238).
    pub(crate) fn restore_sheet_group(&mut self) {
        let Some(wb) = self.presentation().workbook.as_deref() else { return };
        let group: Vec<SlideId> = wb.selected_tabs.iter().filter_map(|i| wb.sheets.get(*i).and_then(|s| s.slide)).collect();
        if group.len() >= 2 && group.contains(&self.current_slide) {
            self.set_group(group);
        }
    }

    /// The group is gone (a click on a tab outside it, 「シートのグループ解除」).
    pub fn ungroup_sheets(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.sheet_state_mut() {
            state.group.clear();
        }
        cx.notify();
    }

    /// 「すべてのシートを選択」: every sheet that is not hidden.
    pub fn select_all_sheets(&mut self, cx: &mut Context<Self>) {
        let all = self.visible_sheet_ids();
        self.set_group(all);
        cx.notify();
    }

    /// A click on the tab of `id`: with Ctrl (⌘) the sheet is added to the group or taken from it (a new one becomes the present one), with Shift the sheets from
    /// the present one to it are the group, without either a click inside the group changes the present sheet (the group stays; when it is every sheet any tab
    /// ends it) and a click outside it ends the group.
    pub fn sheet_tab_click(&mut self, id: SlideId, ctrl: bool, shift: bool, window: &mut Window, cx: &mut Context<Self>) {
        let order = self.visible_sheet_ids();
        let group = self.sheet_group();
        if shift {
            let (a, b) = (order.iter().position(|s| *s == self.current_slide), order.iter().position(|s| *s == id));
            if let (Some(a), Some(b)) = (a, b) {
                let range = order[a.min(b)..=a.max(b)].to_vec();
                self.set_group(range);
            }
            cx.notify();
            return;
        }
        if ctrl {
            let mut members = if group.is_empty() { vec![self.current_slide] } else { group };
            let mut next = None;
            if members.contains(&id) {
                if members.len() > 1 {
                    members.retain(|s| *s != id);
                    if id == self.current_slide {
                        next = members.first().copied();
                    }
                }
            } else {
                members.push(id);
                next = Some(id);
            }
            self.set_group(members);
            if let Some(next) = next {
                self.select_slide(next, window, cx);
            }
            cx.notify();
            return;
        }
        if !group.is_empty() && group.len() == order.len() {
            self.ungroup_sheets(cx);
        }
        self.select_slide(id, window, cx);
    }

    /// `[グループ]` after the file name while several sheets are selected.
    pub(crate) fn group_title_suffix(&self) -> Option<String> {
        (!self.sheet_group().is_empty()).then(|| text("_xlsx.group-title").to_string())
    }

    /// The commands of the present sheet with the group's copies, for the places that run commands on the document themselves (typing, Delete, paste): the same
    /// cells of every sheet of the group in one step. What cannot be done on several sheets is not spread (those places do not make such commands).
    pub(crate) fn group_commands(&self, commands: Vec<Command>) -> Vec<Command> {
        let group = self.sheet_group();
        if group.is_empty() {
            return commands;
        }
        spread(commands.clone(), self.current_slide, &group).unwrap_or(commands)
    }

    /// The commands of the present sheet with the group's copies (one step of the history); `None` after saying that they cannot be done on several sheets.
    pub(crate) fn commands_for_group(&mut self, commands: Vec<Command>, cx: &mut Context<Self>) -> Option<Vec<Command>> {
        let group = self.sheet_group();
        if group.is_empty() {
            return Some(commands);
        }
        match spread(commands, self.current_slide, &group) {
            Ok(commands) => Some(commands),
            Err(()) => {
                self.status = Some(text("_xlsx.group-blocked"));
                cx.notify();
                None
            }
        }
    }

    /// The presentation the file is written from, with the group (`Workbook::selected_tabs`) in it: a copy only when there is a group to write.
    pub(crate) fn with_group_for_save<'a>(&self, presentation: Cow<'a, Presentation>) -> Cow<'a, Presentation> {
        let group = self.sheet_group();
        let Some(wb) = presentation.workbook.as_deref() else { return presentation };
        if group.is_empty() {
            // No group now: the one the file had (read into `selected_tabs`) is ended with it.
            if wb.selected_tabs.is_empty() {
                return presentation;
            }
            let mut copy = presentation.into_owned();
            if let Some(wb) = copy.workbook.as_mut() {
                std::sync::Arc::make_mut(wb).selected_tabs = Vec::new();
            }
            return Cow::Owned(copy);
        }
        let indexes: Vec<usize> = wb.sheets.iter().enumerate().filter(|(_, s)| s.slide.is_some_and(|id| group.contains(&id))).map(|(i, _)| i).collect();
        let mut copy = presentation.into_owned();
        if let Some(wb) = copy.workbook.as_mut() {
            std::sync::Arc::make_mut(wb).selected_tabs = indexes;
        }
        Cow::Owned(copy)
    }
}
