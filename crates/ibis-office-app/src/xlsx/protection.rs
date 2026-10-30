// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! シートの保護 (ツール > シートの保護…): the sheet protected, with a
//! password or not (its hash only: password.rs), and what may still be
//! done (Excel's check boxes); while it is, locked cells cannot be changed
//! (typing, Delete, pasting, filling from the editing basics) and what is
//! not allowed is refused, with Excel's message. Each cell is locked unless
//! its format says not (表示形式 > セルのロック), and 数式を表示しない
//! hides its formula from the formula bar while protected.
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, ProtectedRange, ProtectionAllowed, SheetCommand, SheetProtection, WorkbookProtection};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// What an operation needs on a protected sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum Guarded {
    /// Changing the cells' content (locked ones refused).
    Cells,
    FormatCells,
    FormatColumns,
    FormatRows,
    InsertColumns,
    InsertRows,
    DeleteColumns,
    DeleteRows,
    Sort,
    AutoFilter,
    InsertHyperlinks,
}

pub(crate) struct ProtectPanel {
    /// Protecting (or, the sheet protected, unprotecting).
    pub(crate) protect: bool,
    pub(crate) password: Entity<InputState>,
    pub(crate) confirm: Entity<InputState>,
    pub(crate) allowed: ProtectionAllowed,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

pub(crate) struct ProtectWorkbookPanel {
    /// Protecting (or, the workbook protected, unprotecting).
    pub(crate) protect: bool,
    pub(crate) password: Entity<InputState>,
    pub(crate) confirm: Entity<InputState>,
    pub(crate) lock_structure: bool,
    pub(crate) lock_windows: bool,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

/// Excel's message for a change a protected sheet refuses.
const REFUSED: &str = "fix.protection.cell-chart-you-re-trying";

/// The check boxes, in Excel's order: the field and its label.
type Flag = (&'static str, fn(&mut ProtectionAllowed) -> &mut bool);
const FLAGS: [Flag; 15] = [
    ("fix.protection.select-locked-cells", |a| &mut a.select_locked),
    ("fix.protection.select-unlocked-cells", |a| &mut a.select_unlocked),
    ("fix.protection.format-cells", |a| &mut a.format_cells),
    ("fix.protection.format-columns", |a| &mut a.format_columns),
    ("fix.protection.format-rows", |a| &mut a.format_rows),
    ("fix.protection.insert-columns", |a| &mut a.insert_columns),
    ("fix.protection.insert-rows", |a| &mut a.insert_rows),
    ("fix.protection.insert-hyperlinks", |a| &mut a.insert_hyperlinks),
    ("fix.protection.delete-columns", |a| &mut a.delete_columns),
    ("fix.protection.delete-rows", |a| &mut a.delete_rows),
    ("docx.table.sort", |a| &mut a.sort),
    ("fix.protection.use-autofilter", |a| &mut a.auto_filter),
    ("fix.protection.use-pivottable-pivotchart", |a| &mut a.pivot_tables),
    ("fix.protection.edit-objects", |a| &mut a.objects),
    ("fix.protection.edit-scenarios", |a| &mut a.scenarios),
];

impl SlideEditor {
    /// The current sheet's protection, if it is protected.
    pub(crate) fn sheet_protection(&self) -> Option<&SheetProtection> {
        self.current_sheet()?.1.protection.as_ref()
    }

    /// Whether `what` on the selected cells (and the ranges selected with
    /// them) is refused by the sheet's protection: if so, Excel's message
    /// is shown and true returned.
    pub(crate) fn protection_refuses(&mut self, what: Guarded, cx: &mut Context<Self>) -> bool {
        let ranges = self.selected_ranges();
        self.protection_refuses_ranges(what, &ranges, cx)
    }

    pub(crate) fn protection_refuses_ranges(&mut self, what: Guarded, ranges: &[CellRange], cx: &mut Context<Self>) -> bool {
        if self.is_read_only() {
            return true;
        }
        let (Some(p), Some((_, sheet))) = (self.sheet_protection(), self.current_sheet()) else { return false };
        let a = p.allowed;
        let refused = match what {
            Guarded::Cells => {
                let unlocked_ranges = &self.wb().unlocked_ranges;
                ranges.iter().any(|r| {
                    (r.first_row..=r.last_row).any(|row| {
                        (r.first_column..=r.last_column).any(|column| {
                            let cell_unlocked = sheet.cell(row, column).is_some_and(|c| sheet.styles.get(c.style as usize).is_some_and(|s| s.unlocked));
                            if cell_unlocked {
                                return false;
                            }
                            let in_allowed_range = sheet.protected_ranges.iter().any(|pr| {
                                let in_range = pr.ranges.iter().any(|rg| row >= rg.first_row && row <= rg.last_row && column >= rg.first_column && column <= rg.last_column);
                                if !in_range {
                                    return false;
                                }
                                let has_password = pr.hash.is_some() || pr.legacy.is_some();
                                !has_password || unlocked_ranges.contains(&pr.name)
                            });
                            !in_allowed_range
                        })
                    })
                })
            },
            Guarded::FormatCells => !a.format_cells,
            Guarded::FormatColumns => !a.format_columns,
            Guarded::FormatRows => !a.format_rows,
            Guarded::InsertColumns => !a.insert_columns,
            Guarded::InsertRows => !a.insert_rows,
            Guarded::DeleteColumns => !a.delete_columns,
            Guarded::DeleteRows => !a.delete_rows,
            Guarded::Sort => !a.sort,
            Guarded::AutoFilter => !a.auto_filter,
            Guarded::InsertHyperlinks => !a.insert_hyperlinks,
        };
        if refused {
            self.status = Some(text(REFUSED));
            cx.notify();
        }
        refused
    }

    /// Whether the selected cell's formula is hidden (数式を表示しない on a
    /// protected sheet).
    pub(crate) fn formula_hidden_here(&self) -> bool {
        let (Some(_), Some((_, sheet)), Some((column, row))) = (self.sheet_protection(), self.current_sheet(), self.current_cell()) else { return false };
        sheet.cell(row, column).is_some_and(|c| sheet.styles.get(c.style as usize).is_some_and(|s| s.formula_hidden))
    }

    pub(crate) fn open_protect_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let protect = self.sheet_protection().is_none();
        let input = |placeholder: &str, window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(text(placeholder)));
        let panel = ProtectPanel {
            protect,
            password: input("fix.protection.password-optional", window, cx),
            confirm: input("fix.protection.reenter-password", window, cx),
            allowed: ProtectionAllowed::default(),
            message: String::new(),
            ui: Default::default(),
        };
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.protect = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn protect_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.protect.as_mut() {
            p.message = message.to_string();
        }
        cx.notify();
    }

    pub(crate) fn apply_protect_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.protect.as_ref()) else { return };
        let (password, confirm) = (p.password.read(cx).value().to_string(), p.confirm.read(cx).value().to_string());
        let slide = self.current_slide;
        let protection = if p.protect {
            if password != confirm {
                return self.protect_message(text("fix.protection.confirmation-password-not"), cx);
            }
            Some(SheetProtection { hash: super::password::password_hash(&password), legacy: None, allowed: p.allowed })
        } else {
            let Some(current) = self.sheet_protection() else { return };
            if !super::password::opens(current, &password) {
                return self.protect_message(text("fix.protection.password-you-supplied-not-correct"), cx);
            }
            None
        };
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetProtection { slide, protection })]) {
            return self.protect_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
        }
        self.wb_mut().panels.protect = None;
        cx.notify();
    }

    /// 表示形式 > セルのロック / 数式を表示しない on the selected cells.
    pub(crate) fn toggle_cell_protection(&mut self, hidden: bool, cx: &mut Context<Self>) {
        if self.protection_refuses(Guarded::FormatCells, cx) {
            return;
        }
        let style = self.current_style();
        let change = if hidden { ibis_doc_model::StyleChange::FormulaHidden(!style.formula_hidden) } else { ibis_doc_model::StyleChange::Locked(style.unlocked) };
        self.format_cells(change, cx);
    }

    pub(crate) fn protect_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.protect.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        // The actions stay available below the scrollable permissions list.
        let buttons = |body: crate::shell::chrome::ActionBar, cx: &mut Context<Self>| {
            body
                .action(mark_action("apply", (if p.protect { crate::shell::chrome::ActionDefinition::new("protect-apply", "", crate::shell::chrome::ActionButtonKind::Primary) } else { crate::shell::chrome::ActionDefinition::new("protect-apply", "", crate::shell::chrome::ActionButtonKind::Destructive) }).label(text(if p.protect { "fix.protection.protect-sheet" } else { "fix.protection.unprotect" })).on_click(cx.listener(|this, _, _, cx| this.apply_protect_panel(cx)))))

        };
        if p.protect {
            body = body.child(div().text_sm().child(text("fix.protection.protect-worksheet-contents-locked"))).child(mark("password", crate::shell::chrome::text_field(&p.password).full_width().into_any_element())).child(mark("confirm", crate::shell::chrome::text_field(&p.confirm).full_width().into_any_element()));
            body = body.child(div().text_sm().child(text("fix.protection.allow-all-users-worksheet")));
            for (i, (label, field)) in FLAGS.iter().enumerate() {
                let mut a = p.allowed;
                let on = *field(&mut a);
                let field = *field;
                body = body.child(mark(
                    &format!("allow-{i}"),
                    crate::shell::chrome::checkbox_button(("protect-allow", i), text(label), on, false, cx.listener(move |this, _, _, cx| {
                            if let Some(p) = this.wb_mut().panels.protect.as_mut() {
                                let v = field(&mut p.allowed);
                                *v = !*v;
                            }
                            cx.notify();
                        }))
                        .into_any_element(),
                ));
            }
        } else {
            body = body.child(div().text_sm().child(text("fix.protection.unprotect-sheet"))).child(mark("password", crate::shell::chrome::text_field(&p.password).full_width().into_any_element()));
        }
        body = body.children(crate::shell::chrome::panel_message(&p.message));
        let footer = buttons(crate::shell::chrome::action_bar(),cx).action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(if p.protect { "fix.protection.protect-sheet-2" } else { "fix.protection.unprotect-sheet" }, body.into_any_element(), footer))
    }

    /// The current workbook's protection, if it is protected.
    pub(crate) fn workbook_protection(&self) -> Option<&WorkbookProtection> {
        self.presentation().workbook.as_ref().and_then(|w| w.protection.as_ref())
    }

    /// Whether the workbook structure is locked.
    pub(crate) fn workbook_structure_locked(&self) -> bool {
        self.workbook_protection().is_some_and(|p| p.lock_structure)
    }

    /// Whether an operation changing the workbook's structure is refused by
    /// workbook protection: if so, Excel's message is shown and true returned.
    pub(crate) fn workbook_protection_refuses(&mut self, cx: &mut Context<Self>) -> bool {
        if self.workbook_structure_locked() {
            self.status = Some(text("_xlsx.workbook-is-protected-and-cannot-be-changed"));
            cx.notify();
            true
        } else {
            false
        }
    }

    pub(crate) fn open_protect_workbook_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let protect = self.workbook_protection().is_none();
        let input = |placeholder: &str, window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(text(placeholder)));
        let panel = ProtectWorkbookPanel {
            protect,
            password: input("fix.protection.password-optional", window, cx),
            confirm: input("fix.protection.reenter-password", window, cx),
            lock_structure: true,
            lock_windows: false,
            message: String::new(),
            ui: Default::default(),
        };
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.protect_workbook = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn protect_workbook_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.protect_workbook.as_mut() {
            p.message = message.to_string();
        }
        cx.notify();
    }

    pub(crate) fn apply_protect_workbook_panel(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.protect_workbook.as_ref()) else { return };
        let (password, confirm) = (p.password.read(cx).value().to_string(), p.confirm.read(cx).value().to_string());
        let protection = if p.protect {
            if password != confirm {
                return self.protect_workbook_message(text("fix.protection.confirmation-password-not"), cx);
            }
            if !p.lock_structure && !p.lock_windows {
                return self.protect_workbook_message(text("_xlsx.select-at-least-one-element-to-protect"), cx);
            }
            Some(WorkbookProtection {
                hash: super::password::password_hash(&password),
                legacy: None,
                lock_structure: p.lock_structure,
                lock_windows: p.lock_windows,
                lock_revision: false,
            })
        } else {
            let Some(current) = self.workbook_protection() else { return };
            if !super::password::opens_workbook(current, &password) {
                return self.protect_workbook_message(text("fix.protection.password-you-supplied-not-correct"), cx);
            }
            None
        };
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetWorkbookProtection { protection })]) {
            return self.protect_workbook_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
        }
        self.wb_mut().panels.protect_workbook = None;
        cx.notify();
    }

    pub(crate) fn protect_workbook_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.protect_workbook.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };
        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        let buttons = |body: crate::shell::chrome::ActionBar, cx: &mut Context<Self>| {
            body
                .action(mark_action("apply", (if p.protect { crate::shell::chrome::ActionDefinition::new("protect-wb-apply", "", crate::shell::chrome::ActionButtonKind::Primary) } else { crate::shell::chrome::ActionDefinition::new("protect-wb-apply", "", crate::shell::chrome::ActionButtonKind::Destructive) }).label(text(if p.protect { "_xlsx.protect-workbook" } else { "fix.protection.unprotect" })).on_click(cx.listener(|this, _, _, cx| this.apply_protect_workbook_panel(cx)))))

        };
        if p.protect {
            body = body
                .child(div().text_sm().child(text("_xlsx.protect-structure-and-windows")))
                .child(mark("password", crate::shell::chrome::text_field(&p.password).full_width().into_any_element()))
                .child(mark("confirm", crate::shell::chrome::text_field(&p.confirm).full_width().into_any_element()));
            body = body.child(div().text_sm().child(text("_xlsx.protect")));
            let lock_structure = p.lock_structure;
            let lock_windows = p.lock_windows;
            body = body.child(mark(
                "lock-structure",
                crate::shell::chrome::checkbox_button("protect-wb-structure", text("_xlsx.structure"), lock_structure, false, cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.protect_workbook.as_mut() {
                            p.lock_structure = !p.lock_structure;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
            body = body.child(mark(
                "lock-windows",
                crate::shell::chrome::checkbox_button("protect-wb-windows", text("_xlsx.windows"), lock_windows, false, cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.protect_workbook.as_mut() {
                            p.lock_windows = !p.lock_windows;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            ));
        } else {
            body = body
                .child(div().text_sm().child(text("_xlsx.unprotect-workbook")))
                .child(mark("password", crate::shell::chrome::text_field(&p.password).full_width().into_any_element()));
        }
        body = body.children(crate::shell::chrome::panel_message(&p.message));
        let footer = buttons(crate::shell::chrome::action_bar(),cx).action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(if p.protect { text("_xlsx.workbook-protection") } else { text("_xlsx.unprotect-workbook") }, body.into_any_element(), footer))
    }

    pub(crate) fn unlock_protected_range(&mut self, name: &str, password: &str) -> bool {
        let Some((_, sheet)) = self.current_sheet() else { return false };
        let Some(pr) = sheet.protected_ranges.iter().find(|p| p.name == name) else { return false };
        let ok = super::password::opens_range(pr, password);
        if ok {
            self.wb_mut().unlocked_ranges.insert(name.to_string());
        }
        ok
    }

    pub(crate) fn open_allow_edit_ranges_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let sqref_str = format_sqref(&self.selected_ranges());
        let current_count = self.current_sheet().map(|(_, s)| s.protected_ranges.len()).unwrap_or(0);
        let default_title = crate::shell::i18n::format("_xlsx.range-n", &[("n", &(current_count + 1))]);
        let input = |placeholder: &str, value: &str, masked: bool, window: &mut Window, cx: &mut Context<Self>| {
            cx.new(|cx| {
                let mut state = InputState::new(window, cx).masked(masked).placeholder(text(placeholder));
                if !value.is_empty() {
                    state.set_value(value.to_string(), window, cx);
                }
                state
            })
        };
        let panel = AllowEditRangesPanel {
            selected: if current_count > 0 { Some(0) } else { None },
            title: input("_shell.title", &default_title, false, window, cx),
            sqref: input("_xlsx.refers-to-cells", &sqref_str, false, window, cx),
            password: input("_xlsx.range-password-optional", "", true, window, cx),
            message: String::new(),
            ui: Default::default(),
        };
        let s = self.wb_mut();
        s.custom_format = None;
        s.page_setup = false;
        s.names = None;
        s.validation = None;
        s.find = None;
        s.link = None;
        s.conditional_edit = None;
        s.sort_edit = None;
        s.close_settings();
        s.panels.allow_edit_ranges = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn allow_edit_ranges_message(&mut self, message: SharedString, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.allow_edit_ranges.as_mut() {
            p.message = message.to_string();
        }
        cx.notify();
    }

    pub(crate) fn apply_add_edit_range(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.allow_edit_ranges.as_ref()) else { return };
        let title = p.title.read(cx).value().trim().to_string();
        if title.is_empty() {
            return self.allow_edit_ranges_message(text("_xlsx.enter-a-title"), cx);
        }
        let sqref_text = p.sqref.read(cx).value();
        let Some(ranges) = parse_ranges(&sqref_text) else {
            return self.allow_edit_ranges_message(text("_xlsx.the-reference-cell-range-is-not-valid"), cx);
        };
        let password = p.password.read(cx).value().to_string();
        let hash = if !password.is_empty() {
            super::password::password_hash(&password)
        } else {
            None
        };
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut new_prs = sheet.protected_ranges.clone();
        if let Some(pos) = new_prs.iter().position(|r| r.name == title) {
            new_prs[pos] = ProtectedRange { name: title, ranges, hash, legacy: None };
        } else {
            new_prs.push(ProtectedRange { name: title, ranges, hash, legacy: None });
        }
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetProtectedRanges { slide, ranges: new_prs })]) {
            return self.allow_edit_ranges_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
        }
        if let Some(p) = self.wb_mut().panels.allow_edit_ranges.as_mut() {
            p.message.clear();
        }
        cx.notify();
    }

    pub(crate) fn delete_edit_range(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.allow_edit_ranges.as_ref()) else { return };
        let Some(idx) = p.selected else { return };
        let slide = self.current_slide;
        let Some((_, sheet)) = self.current_sheet() else { return };
        if idx >= sheet.protected_ranges.len() { return };
        let mut new_prs = sheet.protected_ranges.clone();
        new_prs.remove(idx);
        let new_len = new_prs.len();
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetProtectedRanges { slide, ranges: new_prs })]) {
            return self.allow_edit_ranges_message(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into(), cx);
        }
        if let Some(p) = self.wb_mut().panels.allow_edit_ranges.as_mut() {
            p.selected = if new_len == 0 { None } else { Some(idx.min(new_len - 1)) };
        }
        cx.notify();
    }

    pub(crate) fn allow_edit_ranges_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.allow_edit_ranges.as_ref()?;
        let mark = |id: &str, child: AnyElement| {
            let ui = p.ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                }).absolute().top_0().left_0().size_full())
                .into_any_element()
        };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };

        let mut body = v_flex().gap_2().px(px(18.)).py(px(8.)).p_0();
        body = body.child(div().text_sm().child(text("_xlsx.ranges-unlocked-when-sheet-is-protected")));

        if !p.message.is_empty() {
            body = body.child(div().text_sm().text_color(gpui_kit::red()).child(p.message.clone()));
        }

        // List of registered protected ranges
        if let Some((_, sheet)) = self.current_sheet() {
            let selected_idx = p.selected;
            let mut list_box = v_flex().gap_1().p(px(6.0)).border_1().rounded_md();
            if sheet.protected_ranges.is_empty() {
                list_box = list_box.child(div().text_sm().child(text("_xlsx.no-ranges-registered")));
            } else {
                for (i, pr) in sheet.protected_ranges.iter().enumerate() {
                    let is_sel = selected_idx == Some(i);
                    let range_str = format_sqref(&pr.ranges);
                    let has_pw = pr.hash.is_some() || pr.legacy.is_some();
                    let label = format!("{}: {} ({})", pr.name, range_str, if has_pw { text("_xlsx.password-protected") } else { text("_xlsx.no-password") });
                    let pr_name = pr.name.clone();
                    let pr_sqref = range_str.clone();
                    let btn = crate::shell::chrome::action_button(SharedString::from(format!("range-{i}")))
                        .small()

                        .label(if is_sel { format!("▶ {label}") } else { label })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(p) = this.wb_mut().panels.allow_edit_ranges.as_mut() {
                                p.selected = Some(i);
                                p.title.update(cx, |t, cx| t.set_value(pr_name.clone(), window, cx));
                                p.sqref.update(cx, |s, cx| s.set_value(pr_sqref.clone(), window, cx));
                            }
                            cx.notify();
                        }));
                    list_box = list_box.child(mark(&format!("range-{i}"), btn.into_any_element()));
                }
            }
            body = body.child(list_box);
        }

        // Input fields for adding / editing
        body = body.child(div().text_sm().child(text("_shell.title")));
        body = body.child(mark("title", crate::shell::chrome::text_field(&p.title).full_width().into_any_element()));

        body = body.child(div().text_sm().child(text("_xlsx.refers-to-cells")));
        body = body.child(mark("sqref", super::range_field::local_range_field(&p.sqref,"protection-p-sqref",false,cx).into_any_element()));

        body = body.child(div().text_sm().child(text("_xlsx.range-password-optional")));
        body = body.child(mark("password", crate::shell::chrome::text_field(&p.password).full_width().into_any_element()));

        // Buttons
        let mut btn_row = div().flex().flex_row().gap_2();
        btn_row = btn_row.child(mark(
            "add",
            crate::shell::chrome::action_button("allow-ranges-add")
                .small()
                .label(text(if p.selected.is_some(){"_shell.apply"}else{"ui.add"}))
                .on_click(cx.listener(|this, _, _, cx| this.apply_add_edit_range(cx)))
                .into_any_element(),
        ));
        btn_row = btn_row.child(mark(
            "delete",
            crate::shell::chrome::destructive_button("allow-ranges-delete",cx)
                .small()
                .label(text("_shell.delete"))
                .on_click(cx.listener(|this, _, _, cx| this.delete_edit_range(cx)))
                .into_any_element(),
        ));
        body = body.child(btn_row);

        // Sheet protect link button and Close button
        let mut bottom_row = crate::shell::chrome::action_bar();
        bottom_row = bottom_row.action(mark_action("protect-sheet", crate::shell::chrome::ActionDefinition::new("allow-ranges-protect", "", crate::shell::chrome::ActionButtonKind::Primary)

                .label(text("_xlsx.protect-sheet"))
                .on_click(cx.listener(|this, _, window, cx| this.open_protect_panel(window, cx)))));
        bottom_row = bottom_row.action(crate::shell::chrome::ActionDefinition::new("allow-ranges-close", "", crate::shell::chrome::ActionButtonKind::Normal).label(text("_shell.close")).on_click(cx.listener(|e,_,w,cx|e.close_panel_dialog(w,cx))));


        Some(SidebarPanel::with_footer(text("_xlsx.allow-users-to-edit-ranges"), body.into_any_element(), bottom_row))
    }
}

pub(crate) struct AllowEditRangesPanel {
    pub(crate) selected: Option<usize>,
    pub(crate) title: Entity<InputState>,
    pub(crate) sqref: Entity<InputState>,
    pub(crate) password: Entity<InputState>,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

fn parse_ranges(text: &str) -> Option<Vec<CellRange>> {
    let clean = text.replace(',', " ");
    let ranges: Option<Vec<_>> = clean.split_whitespace().map(|word| {
        let mut ends = word.split(':');
        let a = ibis_calc::formula::parser::parse_cell(ends.next()?)?;
        let b = match ends.next() { Some(s) => ibis_calc::formula::parser::parse_cell(s)?, None => a.clone() };
        if ends.next().is_some() || a.row > b.row || a.column > b.column || b.row >= 1_048_576 || b.column >= 16_384 { return None; }
        Some(CellRange { first_row: a.row, first_column: a.column, last_row: b.row, last_column: b.column })
    }).collect();
    ranges.filter(|r| !r.is_empty())
}

fn format_sqref(ranges: &[CellRange]) -> String {
    ranges.iter().map(|r| {
        if r.first_row == r.last_row && r.first_column == r.last_column {
            ibis_doc_model::cell_name(r.first_column, r.first_row)
        } else {
            format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row))
        }
    }).collect::<Vec<_>>().join(" ")
}

