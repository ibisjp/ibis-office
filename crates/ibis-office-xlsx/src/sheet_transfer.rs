// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! シートの移動またはコピー (Excel's 移動またはコピー dialog): the current sheet moves or is copied to a place of
//! this workbook, or to another workbook open in another window (issue #100).
//!
//! To another workbook the sheet's cells, formats, sizes and merged ranges are put there as a new sheet (named as
//! Excel names a copy when the name is taken), and the defined names it uses come with it exactly as they come with
//! pasted cells (`paste_names.rs`): the sheet's own names (sheet scope) become names of the new sheet, the workbook's
//! names its formulas use are defined in the destination, and a name the destination already has is asked about
//! (`paste_names_dialog.rs`, in the destination's window). Formulas that refer to the sheet's other sheets refer to the
//! workbook it came from (`[Book.xlsx]Sheet2!A1`). A move takes the sheet out of its workbook once the destination
//! has it (after the questions are answered); a copy leaves it.
//!
//! Like the other copy of a sheet (`SheetCommand::CopySheet`), the sheet's drawings (charts, pictures, shapes) do not go.

use std::sync::Arc;

use gpui_kit::component::v_flex;
use gpui_kit::*;
use ibis_doc_model::{Command, Sheet, SheetCommand, SlideId};

use super::paste_names::{Answer, Brought, NameSource};
use crate::shell::chrome::{ActionButtonKind, ActionDefinition, Dropdown, DropdownItem, SidebarPanel};
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format, text};
use crate::ext::*;

/// A workbook the sheet can go to.
#[derive(Clone)]
pub(crate) struct BookChoice {
    /// The name the workbook's window shows.
    pub(crate) name: String,
    /// The window's editor (`None` for the workbook of this window).
    pub(crate) editor: Option<WeakEntity<SlideEditor>>,
    /// The sheets that are shown: (their place among all the sheets, name).
    pub(crate) sheets: Vec<(usize, String)>,
    /// How many sheets the workbook has, the hidden ones too.
    pub(crate) total: usize,
}

/// The dialog's state.
pub(crate) struct SheetTransferPanel {
    pub(crate) books: Vec<BookChoice>,
    /// The workbook chosen (`books[0]` is this one).
    pub(crate) dest: usize,
    /// The sheet of the chosen workbook the sheet goes before (`sheets.len()`: to the end).
    pub(crate) before: usize,
    /// コピーを作成する.
    pub(crate) copy: bool,
    /// Why the sheet cannot go.
    pub(crate) message: String,
}

/// A sheet on its way into this workbook, waiting for the answers about the names it brings.
#[derive(Clone)]
pub(crate) struct PendingSheet {
    pub(crate) source: NameSource,
    pub(crate) content: Arc<Sheet>,
    /// The name the sheet had.
    pub(crate) name: String,
    /// Its place among this workbook's sheets.
    pub(crate) index: usize,
    /// The formulas to look for names in: the sheet's, and its own names (sheet scope), which all come along.
    pub(crate) formulas: Vec<String>,
    /// For a move: the workbook the sheet leaves, once it is here.
    pub(crate) from: Option<(WeakEntity<SlideEditor>, SlideId)>,
}

fn book_choice(editor: &SlideEditor, entity: Option<WeakEntity<SlideEditor>>) -> BookChoice {
    let name = editor.file_name.clone().map(|n| n.to_string()).unwrap_or_else(|| editor.workbook_display_name());
    let (sheets, total) = match editor.presentation().workbook.as_ref() {
        Some(wb) => (wb.sheets.iter().enumerate().filter(|(_, s)| s.slide.is_some()).map(|(i, s)| (i, s.name.clone())).collect(), wb.sheets.len()),
        None => (Vec::new(), 0),
    };
    BookChoice { name, editor: entity, sheets, total }
}

/// The editors of the other workbooks open in the app's windows (the windows of this workbook are left out).
pub(crate) fn other_workbooks(me: gpui::EntityId, workbook_id: u64, cx: &mut App) -> Vec<(AnyWindowHandle, Entity<SlideEditor>)> {
    let mut found = Vec::new();
    for &w in &cx.windows() {
        let Some(root) = w.downcast::<gpui_kit::component::Root>() else { continue };
        if let Ok(Some(entity)) = root.update(cx, |root, _, app| {
            let entity = root.view().clone().downcast::<SlideEditor>().ok()?;
            if entity.entity_id() == me {
                return None;
            }
            let e = entity.read(app);
            (e.is_workbook() && (workbook_id == 0 || e.file_id != workbook_id)).then_some(entity)
        }) {
            found.push((w, entity));
        }
    }
    found
}

#[ibis_office_macros::editor_ext(SheetTransfer)]
impl SlideEditor {
    /// 移動またはコピー…: open the dialog for the current sheet.
    pub(crate) fn open_sheet_transfer(&mut self, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let mut books = vec![book_choice(self, None)];
        let (me, book_id) = (cx.entity_id(), self.file_id);
        for (_, entity) in other_workbooks(me, book_id, cx) {
            books.push(book_choice(entity.read(cx), Some(entity.downgrade())));
        }
        let before = books[0].sheets.len();
        let state = self.wb_mut();
        state.close_settings();
        state.panels.sheet_transfer = Some(SheetTransferPanel { books, dest: 0, before, copy: false, message: String::new() });
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn set_sheet_transfer_book(&mut self, book: usize, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sheet_transfer.as_mut() {
            p.dest = book;
            p.before = p.books[book].sheets.len();
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn set_sheet_transfer_before(&mut self, before: usize, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sheet_transfer.as_mut() {
            p.before = before;
        }
        cx.notify();
    }

    pub(crate) fn set_sheet_transfer_copy(&mut self, copy: bool, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.sheet_transfer.as_mut() {
            p.copy = copy;
            p.message.clear();
        }
        cx.notify();
    }

    fn sheet_transfer_cancel(&mut self, cx: &mut Context<Self>) {
        self.wb_mut().panels.sheet_transfer = None;
        cx.notify();
    }

    /// OK: do what the dialog says.
    pub(crate) fn confirm_sheet_transfer(&mut self, cx: &mut Context<Self>) {
        let slide = self.current_slide;
        let Some(wb) = self.presentation().workbook.clone() else { return };
        let Some(at) = wb.sheets.iter().position(|s| s.slide == Some(slide)) else { return };
        let Some(p) = self.sheet_state().and_then(|s| s.panels.sheet_transfer.as_ref()) else { return };
        let (dest, copy, before) = (p.books[p.dest].clone(), p.copy, p.before);
        let before_pos = dest.sheets.get(before).map(|(i, _)| *i).unwrap_or(dest.total);
        let Some(target) = dest.editor.as_ref() else {
            // This workbook.
            self.wb_mut().panels.sheet_transfer = None;
            let mut commands = Vec::new();
            let new_slide = self.document.ids().new_slide_id();
            if copy {
                let Some(base) = self.slide().name.clone() else { return };
                let name = ibis_doc_model::sheet_ops::copy_sheet_name(self.presentation(), &base);
                let shape = self.document.ids().new_shape_id();
                commands.push(Command::Sheet(SheetCommand::CopySheet { slide, name, new_slide, shape }));
                // The copy is made right after the sheet; the place it goes to counts it.
                let (current, place) = (at + 1, before_pos + usize::from(before_pos > at));
                let to = if place > current { place - 1 } else { place };
                if to != current {
                    commands.push(Command::Sheet(SheetCommand::MoveSheet { slide: new_slide, to }));
                }
            } else {
                let to = if before_pos > at { before_pos - 1 } else { before_pos };
                if to == at {
                    return cx.notify();
                }
                commands.push(Command::Sheet(SheetCommand::MoveSheet { slide, to }));
            }
            self.run_sheet(commands, cx);
            if copy && self.presentation().slide(new_slide).is_some() {
                self.set_current_slide(new_slide);
                self.selection = None;
            }
            return cx.notify();
        };
        // Another workbook.
        let Some(target) = target.upgrade() else {
            if let Some(p) = self.wb_mut().panels.sheet_transfer.as_mut() {
                p.message = text("xlsx.sheet-transfer.closed").to_string();
            }
            return cx.notify();
        };
        if !copy && wb.sheets.iter().filter(|s| s.slide.is_some()).count() <= 1 {
            if let Some(p) = self.wb_mut().panels.sheet_transfer.as_mut() {
                p.message = text("xlsx.sheet-transfer.last-sheet").to_string();
            }
            return cx.notify();
        }
        let Some(content) = self.presentation().slide(slide).and_then(ibis_doc_model::sheet_calc::sheet_of_slide).cloned() else { return };
        let Some(name) = wb.sheets.get(at).map(|s| s.name.clone()) else { return };
        let mut formulas: Vec<String> = content.cells.iter().filter_map(|c| c.formula.clone()).collect();
        // Excel (measured, Excel 16.0 COM, Sheet.Copy to another workbook): the sheet's own names all come, and so do the workbook's
        // names that refer to no other sheet (a constant or a LAMBDA nothing uses too); a name that refers to another sheet
        // comes only when the sheet's formulas use it (the formulas above bring those).
        let refers_to_another_sheet = |formula: &str| {
            let mut found = false;
            ibis_calc::formula::rewrite_references(formula, |r| {
                if r.sheet.as_ref().is_some_and(|x| x.book.is_some() || !x.first.eq_ignore_ascii_case(&name)) {
                    found = true;
                }
                Some(r.clone())
            });
            found
        };
        formulas.extend(
            wb.names
                .iter()
                .filter(|n| !n.name.starts_with("_xl") && !n.hidden && (n.sheet == Some(at) || (n.sheet.is_none() && !refers_to_another_sheet(&n.formula))))
                .map(|n| n.name.clone()),
        );
        let source = NameSource {
            names: wb.names.clone(),
            sheet_names: wb.sheets.iter().map(|s| s.name.clone()).collect(),
            sheet: Some(at),
            book: self.file_path().as_ref().and_then(|p| Some((format!("{}/", p.parent()?.to_string_lossy().replace('\\', "/")), p.file_name()?.to_string_lossy().to_string()))),
        };
        let from = (!copy).then(|| (cx.weak_entity(), slide));
        let pending = PendingSheet { source, content, name, index: before_pos, formulas, from };
        self.wb_mut().panels.sheet_transfer = None;
        target.update(cx, |t, cx| t.receive_sheet(pending, cx));
        // The destination's window comes forward (it may be asking about a name).
        let id = target.entity_id();
        for w in cx.windows() {
            let Some(root) = w.downcast::<gpui_kit::component::Root>() else { continue };
            let _ = root.update(cx, |root, window, _| {
                if root.view().entity_id() == id {
                    window.activate_window();
                }
            });
        }
        cx.notify();
    }

    /// A sheet comes in from another workbook: ask about the names first when there is a conflict, else take it.
    pub(crate) fn receive_sheet(&mut self, pending: PendingSheet, cx: &mut Context<Self>) {
        if self.workbook_protection_refuses(cx) {
            return;
        }
        self.commit_cell_edit(cx);
        self.wb_mut().panels.sheet_in = Some(pending);
        self.finish_sheet_in(cx);
    }

    /// What comes with the sheet that is on its way in, answers given.
    pub(crate) fn sheet_names_brought(&self, pending: &PendingSheet, answers: &[Answer]) -> Brought {
        let Some(wb) = self.presentation().workbook.as_ref() else { return Brought::default() };
        let index = pending.index.min(wb.sheets.len());
        let name = self.new_sheet_name_for(&pending.name);
        let mut sheet_names: Vec<String> = wb.sheets.iter().map(|s| s.name.clone()).collect();
        sheet_names.insert(index, name);
        // The names' sheets as they will be once the sheet is in.
        let names: Vec<ibis_doc_model::DefinedName> = wb
            .names
            .iter()
            .cloned()
            .map(|mut n| {
                if let Some(i) = n.sheet.as_mut()
                    && *i >= index
                {
                    *i += 1;
                }
                n
            })
            .collect();
        super::paste_names::names_to_bring(&pending.source, &pending.formulas, &names, &sheet_names, index, false, answers)
    }

    /// The name the sheet gets here: its own, or, when the workbook has a sheet of that name, as Excel names a copy.
    fn new_sheet_name_for(&self, name: &str) -> String {
        if ibis_doc_model::sheet_ops::valid_sheet_name(self.presentation(), name, None) { name.to_string() } else { ibis_doc_model::sheet_ops::copy_sheet_name(self.presentation(), name) }
    }

    /// Put the sheet that is on its way in into this workbook (the questions about names are answered), and take a
    /// moved sheet out of the workbook it came from.
    pub(crate) fn finish_sheet_in(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.sheet_state().and_then(|s| s.panels.sheet_in.clone()) else { return };
        let answered = self.sheet_state_mut().and_then(|s| s.panels.paste_answers.take());
        let brought = self.sheet_names_brought(&pending, answered.as_deref().unwrap_or(&[]));
        if answered.is_none()
            && let Some(first) = brought.conflicts.first()
        {
            let panel = super::paste_names_dialog::PasteNamesPanel::asking_about(first.clone(), super::paste_names_dialog::PasteAgain::SheetIn);
            let state = self.wb_mut();
            state.close_settings();
            state.panels.paste_names = Some(panel);
            self.sidebar_open = true;
            return cx.notify();
        }
        self.wb_mut().panels.sheet_in = None;
        let name = self.new_sheet_name_for(&pending.name);
        let index = pending.index.min(self.presentation().workbook.as_ref().map_or(0, |w| w.sheets.len()));
        // The sheet's formulas follow the names the answers renamed, and refer to the workbook it came from for its other sheets.
        let mut content = (*pending.content).clone();
        let mut external = Vec::new();
        for cell in content.cells.iter_mut() {
            if let Some(f) = cell.formula.as_mut() {
                *f = super::paste_names::rename_in_formula(f, &brought.renames);
                *f = super::paste_names::retarget(f, pending.source.sheet_names.get(pending.source.sheet.unwrap_or(usize::MAX)), &name, pending.source.book.as_ref());
                if ibis_calc::formula::has_external_book(f) {
                    external.push(f.clone());
                }
            }
        }
        let mut commands = Vec::new();
        if !external.is_empty() {
            commands.extend(self.link_new_books_commands(&format!("CHOOSE(1,{})", external.join(","))));
        }
        let (slide, shape) = (self.document.ids().new_slide_id(), self.document.ids().new_shape_id());
        commands.push(Command::Sheet(SheetCommand::InsertSheetFrom { index, name, slide, shape, content: Arc::new(content) }));
        commands.extend(self.brought_commands(brought));
        self.run_sheet(commands, cx);
        let taken = self.presentation().slide(slide).is_some();
        if taken {
            self.set_current_slide(slide);
            self.selection = None;
        } else {
            self.status = Some(text("xlsx.sheet-transfer.failed"));
        }
        // Moved: the sheet is gone from the workbook it came from.
        // (Later in this turn: the workbook it comes from may be the window whose OK started all this.)
        if taken && let Some((from, from_slide)) = pending.from {
            cx.defer(move |cx| {
                if let Some(source) = from.upgrade() {
                    source.update(cx, |s, cx| s.run_sheet(vec![Command::Sheet(SheetCommand::DeleteSheet { slide: from_slide })], cx));
                }
            });
        }
        cx.notify();
    }

    pub(crate) fn sheet_transfer_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.sheet_transfer.as_ref()?;
        let owner = cx.weak_entity();
        let dest = &p.books[p.dest];
        let book_items: Vec<DropdownItem> = p
            .books
            .iter()
            .enumerate()
            .map(|(i, b)| {
                let label = if i == 0 { format("xlsx.sheet-transfer.this-book", &[("name", &b.name)]).to_string() } else { b.name.clone() };
                DropdownItem::new(label).checked(i == p.dest).on_select(std::rc::Rc::new(move |e: &mut SlideEditor, _, cx| e.set_sheet_transfer_book(i, cx)))
            })
            .collect();
        let book_label = if p.dest == 0 { format("xlsx.sheet-transfer.this-book", &[("name", &dest.name)]).to_string() } else { dest.name.clone() };
        let mut before_items: Vec<DropdownItem> = dest
            .sheets
            .iter()
            .enumerate()
            .map(|(i, (_, name))| DropdownItem::new(name.clone()).checked(i == p.before).on_select(std::rc::Rc::new(move |e: &mut SlideEditor, _, cx| e.set_sheet_transfer_before(i, cx))))
            .collect();
        let last = dest.sheets.len();
        before_items.push(DropdownItem::new(text("xlsx.sheet-transfer.to-end")).checked(p.before == last).on_select(std::rc::Rc::new(move |e: &mut SlideEditor, _, cx| e.set_sheet_transfer_before(last, cx))));
        let before_label = dest.sheets.get(p.before).map(|(_, n)| n.clone()).unwrap_or_else(|| text("xlsx.sheet-transfer.to-end").to_string());
        let copy = p.copy;
        let message = p.message.clone();
        let mut body = v_flex()
            .gap_3()
            .child(crate::shell::chrome::dropdown_row(text("xlsx.sheet-transfer.book"), Dropdown::new("sheet-transfer-book", book_label, book_items, &owner).fill_width()))
            .child(crate::shell::chrome::dropdown_row(text("xlsx.sheet-transfer.before"), Dropdown::new("sheet-transfer-before", before_label, before_items, &owner).fill_width()))
            .child(crate::shell::chrome::checkbox_button("sheet-transfer-copy", text("xlsx.sheet-transfer.copy"), copy, false, cx.listener(move |e, _, _, cx| e.set_sheet_transfer_copy(!copy, cx))));
        body = body.children(crate::shell::chrome::panel_message(&message));
        let footer = crate::shell::chrome::action_bar()
            .action(ActionDefinition::new("sheet-transfer-ok", text("shell.action.ok"), ActionButtonKind::Primary).on_click(cx.listener(|e, _, _, cx| e.confirm_sheet_transfer(cx))))
            .action(ActionDefinition::new("sheet-transfer-cancel", text("_shell.cancel"), ActionButtonKind::Normal).on_click(cx.listener(|e, _, _, cx| e.sheet_transfer_cancel(cx))));
        Some(SidebarPanel::with_footer(text("xlsx.sheet-transfer.title"), body.into_any_element(), footer))
    }
}
