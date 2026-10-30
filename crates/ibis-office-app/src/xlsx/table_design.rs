// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! テーブル デザイン (Excel's Table Design tab), as a panel: the table's
//! name, its header and 集計行 (each column's function), its style and its
//! options (縞模様, 最初の列, 最後の列, フィルター ボタン).

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::{InputState,InputEvent};
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Border, CellInput, CellRange, CellStyle, CellValue, Command, PutCell, Rgb, Sheet, SheetCommand, SheetTable, TableStyleInfo, TableTotal};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

pub(crate) struct TableDesignPanel {
    pub(crate) index: usize,
    pub(crate) name: Entity<InputState>,
    _name_subscription: Subscription,
    /// The column whose 集計行 function is being chosen.
    pub(crate) total_column: Option<usize>,
    pub(crate) message: String,
}

/// The functions of a 集計行 cell: (`totalsRowFunction`, SUBTOTAL's
/// number, the name shown), Excel's list.
pub(crate) const TOTALS: [(&str, u32, &str); 8] = [
    ("average", 101, "_xlsx.average"),
    ("count", 103, "_xlsx.count"),
    ("countNums", 102, "_xlsx.count-numbers"),
    ("max", 104, "_xlsx.max-2"),
    ("min", 105, "_xlsx.min"),
    ("sum", 109, "_xlsx.sum"),
    ("stdDev", 107, "_xlsx.stddev"),
    ("var", 110, "_xlsx.var"),
];

/// The built-in table styles, as Excel lists them.
fn style_names() -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (prefix, label, count) in [("TableStyleLight", "_xlsx.light", 21), ("TableStyleMedium", "_xlsx.medium-2", 28), ("TableStyleDark", "_xlsx.dark", 11)] {
        for i in 1..=count {
            out.push((format!("{prefix}{i}"), format!("{} {i}", text(label))));
        }
    }
    out
}

/// A name a table can take: a letter, `_` or `\` first, then letters,
/// digits, `_`, `.` or `\`; not a cell's reference (`A1`, `R1C1`).
pub(crate) fn valid_table_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return false };
    if !(first.is_alphabetic() || first == '_' || first == '\\') || !chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '\\')) || name.chars().count() > 255 {
        return false;
    }
    let upper = name.to_ascii_uppercase();
    let letters = upper.chars().take_while(|c| c.is_ascii_uppercase()).count();
    let looks_like_cell = letters > 0 && letters <= 3 && upper.len() > letters && upper[letters..].chars().all(|c| c.is_ascii_digit());
    !looks_like_cell && !matches!(upper.as_str(), "R" | "C")
}

/// The formats the old style laid on `sheet`'s cells taken off: each of a
/// cell's fill, borders, bold and colour that is what the style gives it.
fn without_style(sheet: &Sheet, table: &SheetTable, theme: &[(String, Rgb)]) -> Sheet {
    let r = table.range;
    let mut bare = sheet.clone();
    for c in bare.cells.iter_mut().filter(|c| (r.first_row..=r.last_row).contains(&c.row) && (r.first_column..=r.last_column).contains(&c.column)) {
        let mut st = sheet.styles[c.style as usize].clone();
        st.fill = None;
        st.left = None;
        st.right = None;
        st.top = None;
        st.bottom = None;
        st.bold = false;
        st.color = Rgb::BLACK;
        bare.styles.push(st);
        c.style = (bare.styles.len() - 1) as u32;
    }
    let overlay = ibis_pptx::styled_with_table(&bare, table, theme);
    let mut out = sheet.clone();
    for row in r.first_row..=r.last_row {
        for column in r.first_column..=r.last_column {
            let (Some(cell), Some(o)) = (sheet.cell(row, column), overlay.cell(row, column)) else { continue };
            let o = &overlay.styles[o.style as usize];
            let mut st = sheet.styles[cell.style as usize].clone();
            let before = st.clone();
            if o.fill.is_some() && st.fill == o.fill {
                st.fill = None;
            }
            let strip = |side: &mut Option<Border>, given: &Option<Border>| {
                if given.is_some() && side == given {
                    *side = None;
                }
            };
            strip(&mut st.left, &o.left);
            strip(&mut st.right, &o.right);
            strip(&mut st.top, &o.top);
            strip(&mut st.bottom, &o.bottom);
            if o.bold && st.bold {
                st.bold = false;
            }
            if o.color != Rgb::BLACK && st.color == o.color {
                st.color = Rgb::BLACK;
            }
            if st != before {
                let id = out.style_index(st);
                if let Some(c) = out.cells.iter_mut().find(|c| (c.row, c.column) == (row, column)) {
                    c.style = id;
                }
            }
        }
    }
    out
}

impl SlideEditor {
    /// The table the selected cell is in.
    pub(crate) fn table_at_selection(&self) -> Option<usize> {
        let (column, row) = self.current_cell()?;
        let (_, sheet) = self.current_sheet()?;
        sheet.tables.iter().position(|t| (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column))
    }

    /// 表示形式 > テーブルのデザイン: the panel of the table the selected cell is in.
    pub(crate) fn open_table_design(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.table_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-table"));
            return cx.notify();
        };
        let name = self.current_sheet().map(|(_, s)| s.tables[index].name.clone()).unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |i, cx| i.set_value(name, window, cx));
        let name_subscription=cx.subscribe_in(&input,window,|this,_,event:&InputEvent,_,cx|{
            if matches!(event,InputEvent::Change) { this.rename_table(cx); }
        });
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
        s.panels.table_design = Some(TableDesignPanel { index, name: input, _name_subscription:name_subscription, total_column: None, message: String::new() });
        self.sidebar_open = true;
        cx.notify();
    }

    fn table_message(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.table_design.as_mut() {
            p.message = message.into();
        }
        cx.notify();
    }

    /// The table `index` changed by `change` (which gives the cells to put
    /// in, or why not), its look laid again; one step to undo.
    pub(crate) fn change_sheet_table(&mut self, index: usize, change: impl FnOnce(&Sheet, &mut SheetTable) -> Result<Vec<(u32, u32, CellInput)>, String>, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let slide = self.current_slide;
        let theme = self.presentation().workbook.as_ref().map(|w| w.theme_colors.clone()).unwrap_or_default();
        let Some((_, sheet)) = self.current_sheet() else { return };
        let sheet: Sheet = (**sheet).clone();
        let Some(old) = sheet.tables.get(index).cloned() else { return };
        let mut table = old.clone();
        let contents = match change(&sheet, &mut table) {
            Ok(c) => c,
            Err(message) => return self.table_message(message, cx),
        };
        let mut base = without_style(&sheet, &old, &theme);
        for (row, column, input) in &contents {
            if matches!(input, CellInput::Clear) {
                // A row the table leaves (its 集計行, its headings) loses its look.
                if let Some(c) = base.cells.iter_mut().find(|c| (c.row, c.column) == (*row, *column)) {
                    c.style = 0;
                }
            }
        }
        let styled = ibis_pptx::styled_with_table(&base, &table, &theme);
        let area = |r: CellRange| (r.first_row..=r.last_row).flat_map(move |row| (r.first_column..=r.last_column).map(move |c| (row, c)));
        let mut cells: Vec<PutCell> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (row, column) in area(old.range).chain(area(table.range)) {
            if !seen.insert((row, column)) {
                continue;
            }
            let inside = (table.range.first_row..=table.range.last_row).contains(&row) && (table.range.first_column..=table.range.last_column).contains(&column);
            let style_before = sheet.cell(row, column).map(|c| sheet.styles[c.style as usize].clone());
            let style_after: Option<CellStyle> = if inside { styled.cell(row, column).map(|c| styled.styles[c.style as usize].clone()) } else { base.cell(row, column).map(|c| base.styles[c.style as usize].clone()) };
            let content = contents.iter().find(|(a, b, _)| (*a, *b) == (row, column)).map(|(_, _, i)| i.clone());
            if style_before == style_after && content.is_none() {
                continue;
            }
            let input = content.unwrap_or_else(|| match sheet.cell(row, column) {
                Some(c) => match &c.formula {
                    Some(f) => CellInput::Formula(f.clone()),
                    None if c.value.is_empty() => CellInput::Clear,
                    None => CellInput::Value(c.value.clone()),
                },
                None => CellInput::Clear,
            });
            cells.push(PutCell { row, column, input, style: Some(style_after.unwrap_or_else(|| sheet.styles.first().cloned().unwrap_or_default())) });
        }
        self.status = match self.document.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index, table: Some(table), cells })]) {
            Ok(_) => None,
            Err(err) => Some(crate::shell::i18n::format("edit.refused", &[("error", &err)]).into()),
        };
        self.table_message(self.status.clone().map(|s| s.to_string()).unwrap_or_default(), cx);
    }

    /// テーブル名: the name typed, if it can be one (the formulas follow).
    pub(crate) fn rename_table(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.table_design.as_ref()) else { return };
        let (index, name) = (p.index, p.name.read(cx).value().trim().to_string());
        let taken: Vec<String> = self.presentation().slides.iter().filter_map(ibis_doc_model::sheet_calc::sheet_of_slide).flat_map(|s| s.tables.iter().map(|t| t.name.clone())).collect();
        let names: Vec<String> = self.presentation().workbook.as_ref().map(|w| w.names.iter().map(|n| n.name.clone()).collect()).unwrap_or_default();
        let current = self.current_sheet().and_then(|(_, s)| s.tables.get(index).map(|t| t.name.clone())).unwrap_or_default();
        if name == current {
            return;
        }
        if !valid_table_name(&name) {
            return self.table_message(text("_xlsx.this-table-name-is-not-valid"), cx);
        }
        if taken.iter().chain(&names).any(|t| t.eq_ignore_ascii_case(&name) && !t.eq_ignore_ascii_case(&current)) {
            return self.table_message(text("_xlsx.this-name-is-already-in-use"), cx);
        }
        self.change_sheet_table(index, move |_, t| {
            t.name = name;
            Ok(Vec::new())
        }, cx);
    }

    /// 集計行 on (a row under the table: 集計 in its first column, the last
    /// column's SUBTOTAL — 合計 of numbers, else 個数) or off.
    pub(crate) fn toggle_totals_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.change_sheet_table(index, |sheet, t| {
            let r = t.range;
            if t.totals_rows > 0 {
                let row = r.last_row;
                t.range.last_row -= t.totals_rows;
                t.totals_rows = 0;
                t.totals.clear();
                return Ok((r.first_column..=r.last_column).map(|c| (row, c, CellInput::Clear)).collect());
            }
            let row = r.last_row + 1;
            if row as usize >= sheet.rows.len() || (r.first_column..=r.last_column).any(|c| sheet.cell(row, c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some())) {
                return Err(text("_xlsx.the-cells-just-below-the-table-are-not-empty-so").to_string());
            }
            let last = r.last_column;
            let numbers = ((r.first_row + t.header_rows)..=r.last_row).any(|row| matches!(sheet.cell(row, last).map(|c| &c.value), Some(CellValue::Number(_))));
            let (function, code) = if numbers { ("sum", 109) } else { ("count", 103) };
            let column = t.columns.last().cloned().unwrap_or_default();
            t.range.last_row = row;
            t.totals_rows = 1;
            t.totals = (0..t.columns.len()).map(|_| TableTotal::default()).collect();
            let mut contents = vec![(row, last, CellInput::Formula(format!("SUBTOTAL({code},{}[{}])", t.name, escape_column(&column))))];
            if let Some(tt) = t.totals.last_mut() {
                tt.function = Some(function.into());
            }
            if r.first_column != last {
                contents.push((row, r.first_column, CellInput::Value(CellValue::Text(text("_xlsx.summarize").to_string()))));
                if let Some(tt) = t.totals.first_mut() {
                    tt.label = Some(text("_xlsx.summarize").to_string());
                }
            }
            Ok(contents)
        }, cx);
    }

    /// If the current cell is in a table's 集計行 (Totals Row), returns
    /// `(table_index, column_index_within_table)`.
    pub(crate) fn current_cell_table_total(&self) -> Option<(usize, usize)> {
        let (column, row) = self.current_cell()?;
        let (_, sheet) = self.current_sheet()?;
        for (table_idx, table) in sheet.tables.iter().enumerate() {
            if table.totals_rows > 0 && row == table.range.last_row {
                if column >= table.range.first_column && column <= table.range.last_column {
                    let col_idx = (column - table.range.first_column) as usize;
                    return Some((table_idx, col_idx));
                }
            }
        }
        None
    }

    /// Open the 集計行 function menu for a column of an Excel table.
    pub(crate) fn open_table_total_menu(&mut self, table_index: usize, column: usize, position: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        use crate::shell::chrome::{MenuEntry, MenuItem, fill_menu};
        use gpui_kit::component::menu::PopupMenu;

        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(table) = sheet.tables.get(table_index) else { return };
        let current_func = table.totals.get(column).and_then(|t| t.function.as_deref());

        let mut entries: Vec<MenuEntry> = Vec::new();
        // 1. None ("なし")
        let is_none = current_func.is_none();
        entries.push(
            MenuItem::new("xlsx.total-none", text("_shell.none"))
                .checked(is_none)
                .on(move |this, _, cx| {
                    if let Some(s) = this.sheet_state_mut() {
                        s.list_popup = None;
                    }
                    this.set_table_total(table_index, column, None, cx);
                })
                .into()
        );

        // 2. The 8 built-in functions
        for &(f, _, label) in &TOTALS {
            let is_cur = current_func == Some(f);
            let id = match f {
                "average" => "xlsx.total-average",
                "count" => "xlsx.total-count",
                "countNums" => "xlsx.total-count-nums",
                "max" => "xlsx.total-max",
                "min" => "xlsx.total-min",
                "sum" => "xlsx.total-sum",
                "stdDev" => "xlsx.total-stddev",
                "var" => "xlsx.total-var",
                _ => "xlsx.total-custom",
            };
            entries.push(
                MenuItem::new(id, text(label))
                    .checked(is_cur)
                    .on(move |this, _, cx| {
                        if let Some(s) = this.sheet_state_mut() {
                            s.list_popup = None;
                        }
                        this.set_table_total(table_index, column, Some(f), cx);
                    })
                    .into()
            );
        }

        let entries = std::rc::Rc::new(entries);
        let editor = cx.entity().downgrade();
        let menu = PopupMenu::build(window, cx, move |menu, window, cx| fill_menu(menu.min_w(px(140.)), &entries, &editor, window, cx));
        let dismiss = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if let Some(s) = this.sheet_state_mut() {
                s.list_popup = None;
            }
            cx.notify();
            this.focus_handle.focus(window, cx);
        });
        menu.read(cx).focus_handle(cx).focus(window, cx);
        self.wb_mut().list_popup = Some(super::validation::ListPopup { menu, position, _dismiss: dismiss });
        cx.notify();
    }

    /// A column's 集計行 cell: `function` (see [`TOTALS`]) or none.
    pub(crate) fn set_table_total(&mut self, index: usize, column: usize, function: Option<&'static str>, cx: &mut Context<Self>) {
        self.change_sheet_table(index, move |_, t| {
            if t.totals_rows == 0 {
                return Ok(Vec::new());
            }
            let row = t.range.last_row;
            let c = t.range.first_column + column as u32;
            if t.totals.len() < t.columns.len() {
                t.totals.resize(t.columns.len(), TableTotal::default());
            }
            let name = t.columns.get(column).cloned().unwrap_or_default();
            let input = match function.and_then(|f| TOTALS.iter().find(|x| x.0 == f)) {
                Some((f, code, _)) => {
                    t.totals[column] = TableTotal { function: Some((*f).into()), label: None };
                    CellInput::Formula(format!("SUBTOTAL({code},{}[{}])", t.name, escape_column(&name)))
                }
                None => {
                    t.totals[column] = TableTotal::default();
                    CellInput::Clear
                }
            };
            Ok(vec![(row, c, input)])
        }, cx);
    }

    /// 見出し行 off (its row emptied, the table starting under it) or on
    /// (the columns' names in the row above, which must be empty).
    pub(crate) fn toggle_header_row(&mut self, index: usize, cx: &mut Context<Self>) {
        self.change_sheet_table(index, |sheet, t| {
            let r = t.range;
            if t.header_rows > 0 {
                if r.last_row - r.first_row < 1 + t.totals_rows {
                    return Err(text("_xlsx.the-table-has-only-its-header-row").to_string());
                }
                t.range.first_row += 1;
                t.header_rows = 0;
                t.filter = None;
                return Ok((r.first_column..=r.last_column).map(|c| (r.first_row, c, CellInput::Clear)).collect());
            }
            if r.first_row == 0 || (r.first_column..=r.last_column).any(|c| sheet.cell(r.first_row - 1, c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some())) {
                return Err(text("_xlsx.the-cells-just-above-the-table-are-not-empty-so").to_string());
            }
            let row = r.first_row - 1;
            t.range.first_row = row;
            t.header_rows = 1;
            t.filter = Some(ibis_doc_model::AutoFilter { range: CellRange { last_row: t.range.last_row - t.totals_rows, ..t.range }, columns: Vec::new() });
            Ok(t.columns.iter().enumerate().map(|(i, name)| (row, r.first_column + i as u32, CellInput::Value(CellValue::Text(name.clone())))).collect())
        }, cx);
    }

    /// フィルター ボタン on or off.
    pub(crate) fn toggle_table_filter_button(&mut self, index: usize, cx: &mut Context<Self>) {
        self.change_sheet_table(index, |_, t| {
            if t.header_rows == 0 {
                return Ok(Vec::new());
            }
            t.filter = match t.filter {
                Some(_) => None,
                None => Some(ibis_doc_model::AutoFilter { range: CellRange { last_row: t.range.last_row - t.totals_rows, ..t.range }, columns: Vec::new() }),
            };
            Ok(Vec::new())
        }, cx);
    }

    /// The style's options, or another style.
    pub(crate) fn set_table_style(&mut self, index: usize, change: impl FnOnce(&mut TableStyleInfo) + 'static, cx: &mut Context<Self>) {
        self.change_sheet_table(index, move |_, t| {
            let mut st = t.style.clone().unwrap_or_else(|| TableStyleInfo { name: "TableStyleMedium2".into(), row_stripes: true, ..Default::default() });
            change(&mut st);
            t.style = Some(st);
            Ok(Vec::new())
        }, cx);
    }

    pub(crate) fn table_design_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.table_design.as_ref()?;
        let index = p.index;
        let (_, sheet) = self.current_sheet()?;
        let t = sheet.tables.get(index)?.clone();
        let message = p.message.clone();
        let total_column = p.total_column;
        let st = t.style.clone().unwrap_or_default();
        let mut body = v_flex()
            .gap_1()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(text("_xlsx.table-name"))
            .child(crate::shell::chrome::text_field(&p.name).full_width())
            .child(div().pt(px(6.)).child(text("_xlsx.table-style-options")))
            .child(crate::shell::chrome::checkbox_button("table-header", crate::shell::i18n::text("docx.table-design.header-row"), t.header_rows > 0, false, cx.listener(move |this, _, _, cx| this.toggle_header_row(index, cx))))
            .child(crate::shell::chrome::checkbox_button("table-totals", crate::shell::i18n::text("docx.table-design.total-row"), t.totals_rows > 0, false, cx.listener(move |this, _, _, cx| this.toggle_totals_row(index, cx))))
            .child(crate::shell::chrome::checkbox_button("table-row-stripes", crate::shell::i18n::text("_xlsx.banded-rows"), st.row_stripes, false, cx.listener(move |this, _, _, cx| this.set_table_style(index, |s| s.row_stripes = !s.row_stripes, cx))))
            .child(crate::shell::chrome::checkbox_button("table-column-stripes", crate::shell::i18n::text("_xlsx.banded-columns"), st.column_stripes, false, cx.listener(move |this, _, _, cx| this.set_table_style(index, |s| s.column_stripes = !s.column_stripes, cx))))
            .child(crate::shell::chrome::checkbox_button("table-first-column", crate::shell::i18n::text("docx.table-design.first-column"), st.first_column, false, cx.listener(move |this, _, _, cx| this.set_table_style(index, |s| s.first_column = !s.first_column, cx))))
            .child(crate::shell::chrome::checkbox_button("table-last-column", crate::shell::i18n::text("docx.table-design.last-column"), st.last_column, false, cx.listener(move |this, _, _, cx| this.set_table_style(index, |s| s.last_column = !s.last_column, cx))))
            .child(crate::shell::chrome::checkbox_button("table-filter-button", crate::shell::i18n::text("_xlsx.filter-button"), t.filter.is_some(), t.header_rows == 0, cx.listener(move |this, _, _, cx| this.toggle_table_filter_button(index, cx))));
        if t.totals_rows > 0 {
            body = body.child(div().pt(px(6.)).child(text("_xlsx.total-row-functions")));
            for (i, name) in t.columns.iter().enumerate() {
                let current = t.totals.get(i).and_then(|x| x.function.clone());
                let shown = current.as_deref().and_then(|f| TOTALS.iter().find(|x| x.0 == f)).map_or(text("_shell.none").to_string(), |x| text(x.2).to_string());
                body = body.child(crate::shell::chrome::action_button(SharedString::from(format!("table-total-{i}"))).small().label(format!("{name}: {shown}")).on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(p) = this.wb_mut().panels.table_design.as_mut() {
                        p.total_column = if p.total_column == Some(i) { None } else { Some(i) };
                    }
                    cx.notify();
                })));
                if total_column == Some(i) {
                    let mut choices = h_flex().gap_1().flex_wrap().pl(px(12.)).child(crate::shell::chrome::action_button(SharedString::from(format!("table-total-{i}-none"))).small().label(text("_shell.none")).on_click(cx.listener(move |this, _, _, cx| this.set_table_total(index, i, None, cx))));
                    for (f, _, label) in TOTALS {
                        let on = current.as_deref() == Some(f);
                        choices = choices.child(crate::shell::chrome::radio_button(SharedString::from(format!("table-total-{i}-{f}")), on, false, text(label), cx.listener(move |this, _, _, cx| this.set_table_total(index, i, Some(f), cx))));
                    }
                    body = body.child(choices);
                }
            }
        }
        body = body
            .child(div().pt(px(6.)).child(text("_xlsx.options")))
            .child(
                v_flex()
                    .gap_1()
                    .items_start()
                    .child(
                        crate::shell::chrome::action_button("table-design-resize")
                            .small()
                            .label(text("_xlsx.table-resize"))
                            .on_click(cx.listener(|this, _, window, cx| this.resize_table_dialog(window, cx))),
                    )
                    .child(
                        crate::shell::chrome::action_button("table-design-pivot")
                            .small()
                            .label(text("xlsx.pivot-insert"))
                            .on_click(cx.listener(|this, _, window, cx| this.open_create_pivot(window, cx))),
                    )
                    .child(
                        crate::shell::chrome::action_button("table-design-slicer")
                            .small()
                            .label(text("xlsx.slicer-insert"))
                            .on_click(cx.listener(|this, _, window, cx| this.open_slicer_insert_panel(window, cx))),
                    )
                    .child(
                        crate::shell::chrome::action_button("table-design-timeline")
                            .small()
                            .label(text("xlsx.timeline-insert"))
                            .on_click(cx.listener(|this, _, window, cx| this.open_timeline_insert_panel(window, cx))),
                    )
                    .child(
                        crate::shell::chrome::destructive_button("table-design-dedupe",cx)
                            .small()
                            .label(text("xlsx.remove-duplicates"))
                            .on_click(cx.listener(|this, _, window, cx| this.open_dedupe_panel(window, cx))),
                    )
                    .child(
                        crate::shell::chrome::action_button("table-design-to-range")
                            .small()
                            .label(text("xlsx.table-to-range"))
                            .on_click(cx.listener(|this, _, _, cx| this.table_to_range(cx))),
                    ),
            );
        body = body.child(div().pt(px(6.)).child(text("_xlsx.table-styles")));
        let mut styles = v_flex().gap_1();
        for (name, label) in style_names() {
            let on = st.name == name;
            let chosen = name.clone();
            styles = styles.child(crate::shell::chrome::radio_button(SharedString::from(format!("table-style-{name}")), on, false, label, cx.listener(move |this, _, _, cx| {
                let chosen = chosen.clone();
                this.set_table_style(index, move |s| s.name = chosen, cx)
            })));
        }
        body = body.child(styles).child(div().text_sm().child(SharedString::from(message)));
        Some(SidebarPanel{title:"_xlsx.table-design".into(),body:body.into_any_element()})
    }

    /// テーブルの選択 (`mode`: 0=全体, 1=データ, 2=列全体, 3=列データ, 4=行)
    pub(crate) fn select_table_part(&mut self, mode: u8, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.table_at_selection() else { return };
        let Some((cur_col, cur_row)) = self.current_cell() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(t) = sheet.tables.get(index) else { return };
        let r = t.range;
        let data_first = (r.first_row + t.header_rows).min(r.last_row);
        let data_last = r.last_row.saturating_sub(t.totals_rows).max(data_first);
        let col = cur_col.clamp(r.first_column, r.last_column);
        let row = cur_row.clamp(r.first_row, r.last_row);
        let (first_col, first_row, last_col, last_row) = match mode {
            1 => (r.first_column, data_first, r.last_column, data_last),
            2 => (col, r.first_row, col, r.last_row),
            3 => (col, data_first, col, data_last),
            4 => (r.first_column, row, r.last_column, row),
            _ => (r.first_column, r.first_row, r.last_column, r.last_row),
        };
        let slide = self.current_slide;
        let state = self.wb_mut();
        state.cells.insert(slide, (first_col, first_row));
        if first_col == last_col && first_row == last_row {
            state.cell_ends.remove(&slide);
        } else {
            state.cell_ends.insert(slide, (last_col, last_row));
        }
        self.status = Some(text("_xlsx.table-selected"));
        cx.notify();
    }

    /// 選択中セルの列の集計行関数を設定（集計行がオフで関数が指定された場合は集計行も自動で有効化）
    pub(crate) fn set_table_total_at_selection(&mut self, function: Option<&'static str>, cx: &mut Context<Self>) {
        let Some(index) = self.table_at_selection() else { return };
        let Some((col, _)) = self.current_cell() else { return };
        let Some((first_col, has_totals)) = self
            .current_sheet()
            .and_then(|(_, s)| s.tables.get(index))
            .map(|t| (t.range.first_column, t.totals_rows > 0))
        else {
            return;
        };
        let col_idx = col.saturating_sub(first_col) as usize;
        if !has_totals && function.is_some() {
            self.toggle_totals_row(index, cx);
        }
        self.set_table_total(index, col_idx, function, cx);
    }

    /// 見出し行の切り替え（選択中セルが存在するテーブル）
    pub(crate) fn toggle_table_header_row_at_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.table_at_selection() {
            self.toggle_header_row(index, cx);
        }
    }

    /// 集計行の切り替え（選択中セルが存在するテーブル）
    pub(crate) fn toggle_table_totals_row_at_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.table_at_selection() {
            self.toggle_totals_row(index, cx);
        }
    }

    /// 縞模様（行）の切り替え（選択中セルが存在するテーブル）
    pub(crate) fn toggle_table_banded_rows_at_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.table_at_selection() {
            self.set_table_style(index, |s| s.row_stripes = !s.row_stripes, cx);
        }
    }

    /// 縞模様（列）の切り替え（選択中セルが存在するテーブル）
    pub(crate) fn toggle_table_banded_columns_at_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.table_at_selection() {
            self.set_table_style(index, |s| s.column_stripes = !s.column_stripes, cx);
        }
    }

    /// 最初の列の強調切り替え（選択中セルが存在するテーブル）
    pub(crate) fn toggle_table_first_column_at_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.table_at_selection() {
            self.set_table_style(index, |s| s.first_column = !s.first_column, cx);
        }
    }

    /// 最後の列の強調切り替え（選択中セルが存在するテーブル）
    pub(crate) fn toggle_table_last_column_at_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.table_at_selection() {
            self.set_table_style(index, |s| s.last_column = !s.last_column, cx);
        }
    }

    /// フィルターボタンの切り替え（選択中セルが存在するテーブル）
    pub(crate) fn toggle_table_filter_button_at_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.table_at_selection() {
            self.toggle_table_filter_button(index, cx);
        }
    }

    /// テーブルスタイルのクリア（選択中セルが存在するテーブル）
    pub(crate) fn clear_table_style_at_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.table_at_selection() {
            self.set_table_style(index, |s| s.name.clear(), cx);
        }
    }

    /// ホーム > スタイル > テーブルとして書式設定 (`xlsx.format-as-table`):
    /// 選択中セルがテーブル内にあればそのスタイルを変更し、なければ選択範囲または現在の領域から
    /// 指定スタイルのテーブルを即座に作成する.
    pub(crate) fn format_as_table_with_style(&mut self, style_name: &str, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let style_owned = style_name.to_string();
        if let Some(index) = self.table_at_selection() {
            self.set_table_style(index, move |s| s.name = style_owned, cx);
            return;
        }
        let range = match self.selected_range() {
            Some(r) if r.first_row != r.last_row || r.first_column != r.last_column => Some(r),
            _ => self.current_region(),
        };
        let Some(range) = range else { return };
        let header = self
            .current_sheet()
            .map(|(_, sheet)| {
                let all_text = |r: u32| {
                    (range.first_column..=range.last_column)
                        .all(|c| sheet.cell(r, c).is_some_and(|x| matches!(x.value, CellValue::Text(_))))
                };
                range.first_row < range.last_row && all_text(range.first_row) && !all_text(range.first_row + 1)
            })
            .unwrap_or(true);
        match self.make_table(range, header, cx) {
            Ok(()) => {
                if style_owned != "TableStyleMedium2" {
                    if let Some((_, sheet)) = self.current_sheet() {
                        if let Some(index) = sheet.tables.len().checked_sub(1) {
                            self.set_table_style(index, move |s| s.name = style_owned, cx);
                        }
                    }
                }
            }
            Err(e) => {
                self.status = Some(e.into());
                cx.notify();
            }
        }
    }

    /// テーブルのサイズ変更 (`xlsx.table-resize`)
    pub(crate) fn resize_table_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.table_at_selection() else {
            self.status = Some(text("_xlsx.select-a-cell-in-a-table"));
            return cx.notify();
        };
        // 選択範囲があればその範囲にリサイズ、単一セル選択なら現在のテーブル範囲を表示してオープン
        if let Some(r) = self.selected_range() {
            if r.last_row > r.first_row && r.last_column >= r.first_column {
                let current_r = self.current_sheet().map(|(_, s)| s.tables[index].range);
                if current_r != Some(r) {
                    return self.resize_table(index, r, cx);
                }
            }
        }
        self.open_resize_table_dialog(index, window, cx);
    }

    /// テーブルを指定範囲にリサイズ
    pub(crate) fn resize_table(&mut self, index: usize, new_range: CellRange, cx: &mut Context<Self>) {
        self.change_sheet_table(index, move |_, t| {
            if new_range.last_row <= new_range.first_row + t.header_rows + t.totals_rows - 1 {
                return Err(text("_xlsx.the-reference-is-not-valid").to_string());
            }
            let old_cols_count = t.columns.len();
            let new_cols_count = (new_range.last_column - new_range.first_column + 1) as usize;
            t.range = new_range;
            if new_cols_count > old_cols_count {
                for i in old_cols_count..new_cols_count {
                    let col_name = crate::shell::i18n::format("_xlsx.column-n", &[("n", &(i + 1))]);
                    t.columns.push(col_name);
                }
            } else if new_cols_count < old_cols_count {
                t.columns.truncate(new_cols_count);
            }
            if t.totals_rows > 0 {
                t.totals.resize(t.columns.len(), TableTotal::default());
            }
            if t.header_rows > 0 {
                t.filter = Some(ibis_doc_model::AutoFilter {
                    range: CellRange {
                        last_row: t.range.last_row - t.totals_rows,
                        ..t.range
                    },
                    columns: Vec::new(),
                });
            }
            Ok(Vec::new())
        }, cx);
        self.status = Some(text("_xlsx.table-resized").into());
        cx.notify();
    }

    /// 上にテーブルの行を挿入 (`xlsx.table-insert-row-above`)
    pub(crate) fn insert_table_rows_above(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::InsertRows, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(index) = self.table_at_selection() else { return };
        let Some(sel) = self.selected_range() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(t) = sheet.tables.get(index).cloned() else { return };
        let data_first = t.range.first_row + t.header_rows;
        let first = sel.first_row.clamp(t.range.first_row, t.range.last_row);
        let last = sel.last_row.clamp(first, t.range.last_row);
        let count = (last - first + 1).max(1);
        let at = sel.first_row.clamp(data_first, t.range.last_row);
        if at > t.range.first_row && at <= t.range.last_row {
            let slide = self.current_slide;
            self.run_sheet(
                vec![Command::Sheet(SheetCommand::Insert {
                    slide,
                    axis: ibis_doc_model::sheet_command::Axis::Rows,
                    at,
                    count,
                })],
                cx,
            );
        } else {
            self.insert_table_rows_below(cx);
        }
    }

    /// 下にテーブルの行を挿入 (`xlsx.table-insert-row-below`)
    pub(crate) fn insert_table_rows_below(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::InsertRows, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(index) = self.table_at_selection() else { return };
        let Some(sel) = self.selected_range() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(t) = sheet.tables.get(index).cloned() else { return };
        let data_first = t.range.first_row + t.header_rows;
        let data_last = t.range.last_row.saturating_sub(t.totals_rows);
        let first = sel.first_row.clamp(t.range.first_row, t.range.last_row);
        let last = sel.last_row.clamp(first, t.range.last_row);
        let count = (last - first + 1).max(1);
        let at = (sel.last_row.min(data_last) + 1).max(data_first);
        if at > t.range.first_row && at <= t.range.last_row {
            let slide = self.current_slide;
            self.run_sheet(
                vec![Command::Sheet(SheetCommand::Insert {
                    slide,
                    axis: ibis_doc_model::sheet_command::Axis::Rows,
                    at,
                    count,
                })],
                cx,
            );
        } else {
            self.change_sheet_table(
                index,
                move |_, t| {
                    t.range.last_row += count;
                    if t.header_rows > 0 {
                        t.filter = Some(ibis_doc_model::AutoFilter {
                            range: CellRange {
                                last_row: t.range.last_row - t.totals_rows,
                                ..t.range
                            },
                            columns: t.filter.as_ref().map_or_else(Vec::new, |f| f.columns.clone()),
                        });
                    }
                    Ok(Vec::new())
                },
                cx,
            );
        }
    }

    /// 左にテーブルの列を挿入 (`xlsx.table-insert-col-left`)
    pub(crate) fn insert_table_columns_left(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::InsertColumns, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(index) = self.table_at_selection() else { return };
        let Some(sel) = self.selected_range() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(t) = sheet.tables.get(index).cloned() else { return };
        let first = sel.first_column.clamp(t.range.first_column, t.range.last_column);
        let last = sel.last_column.clamp(first, t.range.last_column);
        let count = (last - first + 1).max(1);
        let at = if first > t.range.first_column {
            first
        } else {
            (t.range.first_column + 1).min(t.range.last_column)
        };
        if at > t.range.first_column && at <= t.range.last_column {
            let slide = self.current_slide;
            self.run_sheet(
                vec![Command::Sheet(SheetCommand::Insert {
                    slide,
                    axis: ibis_doc_model::sheet_command::Axis::Columns,
                    at,
                    count,
                })],
                cx,
            );
        } else {
            self.insert_table_columns_right(cx);
        }
    }

    /// 右にテーブルの列を挿入 (`xlsx.table-insert-col-right`)
    pub(crate) fn insert_table_columns_right(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::InsertColumns, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(index) = self.table_at_selection() else { return };
        let Some(sel) = self.selected_range() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(t) = sheet.tables.get(index).cloned() else { return };
        let first = sel.first_column.clamp(t.range.first_column, t.range.last_column);
        let last = sel.last_column.clamp(first, t.range.last_column);
        let count = (last - first + 1).max(1);
        let at = last + 1;
        if at > t.range.first_column && at <= t.range.last_column {
            let slide = self.current_slide;
            self.run_sheet(
                vec![Command::Sheet(SheetCommand::Insert {
                    slide,
                    axis: ibis_doc_model::sheet_command::Axis::Columns,
                    at,
                    count,
                })],
                cx,
            );
        } else {
            self.change_sheet_table(
                index,
                move |_, t| {
                    let old_last_col = t.range.last_column;
                    t.range.last_column += count;
                    let mut contents = Vec::new();
                    for i in 0..count {
                        let col = old_last_col + 1 + i;
                        let names = super::table_edit::headings(&[t.columns.clone(), vec![String::new()]].concat());
                        let new_name = names.last().cloned().unwrap_or_default();
                        t.columns.push(new_name.clone());
                        if !t.totals.is_empty() {
                            t.totals.push(TableTotal::default());
                        }
                        if t.header_rows > 0 {
                            contents.push((t.range.first_row, col, CellInput::Value(CellValue::Text(new_name))));
                        }
                    }
                    if t.header_rows > 0 {
                        t.filter = Some(ibis_doc_model::AutoFilter {
                            range: CellRange {
                                last_row: t.range.last_row - t.totals_rows,
                                ..t.range
                            },
                            columns: t.filter.as_ref().map_or_else(Vec::new, |f| f.columns.clone()),
                        });
                    }
                    Ok(contents)
                },
                cx,
            );
        }
    }

    /// テーブルの行の削除 (`xlsx.table-delete-row`)
    pub(crate) fn delete_table_rows(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::DeleteRows, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(index) = self.table_at_selection() else { return };
        let Some(sel) = self.selected_range() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(t) = sheet.tables.get(index).cloned() else { return };
        let data_first = t.range.first_row + t.header_rows;
        let data_last = t.range.last_row.saturating_sub(t.totals_rows);
        if data_first > data_last {
            return;
        }
        let first = sel.first_row.clamp(data_first, data_last);
        let last = sel.last_row.clamp(first, data_last);
        let count = (last - first + 1).max(1);
        if t.header_rows == 0 && t.totals_rows == 0 && count >= (t.range.last_row - t.range.first_row + 1) {
            return;
        }
        let slide = self.current_slide;
        self.run_sheet(
            vec![Command::Sheet(SheetCommand::Delete {
                slide,
                axis: ibis_doc_model::sheet_command::Axis::Rows,
                at: first,
                count,
            })],
            cx,
        );
    }

    /// テーブルの列の削除 (`xlsx.table-delete-col`)
    pub(crate) fn delete_table_columns(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(super::protection::Guarded::DeleteColumns, cx) {
            return;
        }
        self.commit_cell_edit(cx);
        let Some(index) = self.table_at_selection() else { return };
        let Some(sel) = self.selected_range() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(t) = sheet.tables.get(index).cloned() else { return };
        let first = sel.first_column.clamp(t.range.first_column, t.range.last_column);
        let last = sel.last_column.clamp(first, t.range.last_column);
        let count = (last - first + 1).max(1);
        if count >= (t.range.last_column - t.range.first_column + 1) {
            return;
        }
        let slide = self.current_slide;
        self.run_sheet(
            vec![Command::Sheet(SheetCommand::Delete {
                slide,
                axis: ibis_doc_model::sheet_command::Axis::Columns,
                at: first,
                count,
            })],
            cx,
        );
    }

    /// 新しいテーブル スタイル… パネルを開く.
    pub(crate) fn open_new_table_style(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(index) = self.table_at_selection() else {
            self.status = Some(text("xlsx.new-table-style.select-table"));
            return cx.notify();
        };
        let (_, sheet) = match self.current_sheet() {
            Some(s) => s,
            None => return,
        };
        let t = match sheet.tables.get(index) {
            Some(t) => t.clone(),
            None => return,
        };
        let st = t.style.unwrap_or_default();
        let name_input = cx.new(|cx| InputState::new(window, cx));
        name_input.update(cx, |inp, cx| inp.set_value("TableStyleCustom1".to_string(), window, cx));

        let panel = NewTableStylePanel {
            index,
            name: name_input,
            row_stripes: st.row_stripes,
            col_stripes: st.column_stripes,
            first_col: st.first_column,
            last_col: st.last_column,
            message: String::new(),
            ui: Default::default(),
        };
        let state = self.wb_mut();
        state.close_settings();
        state.panels.new_table_style = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    /// 新しいテーブル スタイルを適用.
    pub(crate) fn apply_new_table_style(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self.sheet_state().and_then(|s| s.panels.new_table_style.as_ref()) else { return };
        let index = p.index;
        let mut style_name = p.name.read(cx).value().trim().to_string();
        if style_name.is_empty() {
            style_name = "TableStyleCustom1".to_string();
        }
        let row_stripes = p.row_stripes;
        let col_stripes = p.col_stripes;
        let first_col = p.first_col;
        let last_col = p.last_col;

        self.set_table_style(index, move |s| {
            s.name = style_name;
            s.row_stripes = row_stripes;
            s.column_stripes = col_stripes;
            s.first_column = first_col;
            s.last_column = last_col;
        }, cx);

        self.wb_mut().panels.new_table_style = None;
        cx.notify();
    }

    pub(crate) fn new_table_style_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.sheet_state()?.panels.new_table_style.as_ref()?;
        let ui = p.ui.clone();
        let mark = |id: &str, child: AnyElement| {
            let ui = ui.clone();
            let id = id.to_string();
            div()
                .relative()
                .child(child)
                .child(
                    canvas(|_, _, _| {}, move |bounds, _, _, _| {
                        ui.borrow_mut().insert(id.clone(), bounds);
                    })
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element()
        };
        let row_stripes = p.row_stripes;
        let col_stripes = p.col_stripes;
        let first_col = p.first_col;
        let last_col = p.last_col;

        let mut body = v_flex()
            .gap_1()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(text("xlsx.new-table-style.name"))
            .child(mark("name", crate::shell::chrome::text_field(&p.name).full_width().into_any_element()))
            .child(div().pt(px(6.)).child(text("_xlsx.table-style-options")))
            .child(crate::shell::chrome::checkbox_button("custom-row-stripes", crate::shell::i18n::text("_xlsx.banded-rows"), row_stripes, false, cx.listener(|this, _, _, cx| {
                if let Some(p) = this.wb_mut().panels.new_table_style.as_mut() {
                    p.row_stripes = !p.row_stripes;
                }
                cx.notify();
            })))
            .child(crate::shell::chrome::checkbox_button("custom-col-stripes", crate::shell::i18n::text("_xlsx.banded-columns"), col_stripes, false, cx.listener(|this, _, _, cx| {
                if let Some(p) = this.wb_mut().panels.new_table_style.as_mut() {
                    p.col_stripes = !p.col_stripes;
                }
                cx.notify();
            })))
            .child(crate::shell::chrome::checkbox_button("custom-first-col", crate::shell::i18n::text("docx.table-design.first-column"), first_col, false, cx.listener(|this, _, _, cx| {
                if let Some(p) = this.wb_mut().panels.new_table_style.as_mut() {
                    p.first_col = !p.first_col;
                }
                cx.notify();
            })))
            .child(crate::shell::chrome::checkbox_button("custom-last-col", crate::shell::i18n::text("docx.table-design.last-column"), last_col, false, cx.listener(|this, _, _, cx| {
                if let Some(p) = this.wb_mut().panels.new_table_style.as_mut() {
                    p.last_col = !p.last_col;
                }
                cx.notify();
            })));

        if !p.message.is_empty() {
            body = body.child(div().text_sm().text_color(rgb(0xD9534F)).child(SharedString::from(p.message.clone())));
        }
        let panel_actions = crate::shell::chrome::action_bar().action(
            crate::shell::chrome::ActionDefinition::new("new-table-style-ok", "", crate::shell::chrome::ActionButtonKind::Primary)
                .label(text("OK"))
                .on_click(cx.listener(|this, _, _, cx| this.apply_new_table_style(cx)))
        )
            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer(text("xlsx.new-table-style.title"), body.into_any_element(), panel_actions))
    }
}

pub(crate) struct NewTableStylePanel {
    pub(crate) index: usize,
    pub(crate) name: Entity<InputState>,
    pub(crate) row_stripes: bool,
    pub(crate) col_stripes: bool,
    pub(crate) first_col: bool,
    pub(crate) last_col: bool,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
}

/// A column's name inside a structured reference (`[` `]` `#` `'` escaped
/// with `'`).
fn escape_column(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if matches!(c, '[' | ']' | '#' | '\'') {
            out.push('\'');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::valid_table_name;

    #[test]
    fn table_names_as_excel_takes_them() {
        assert!(valid_table_name("売上表"));
        assert!(valid_table_name("Sales_2026"));
        assert!(valid_table_name("_t"));
        assert!(!valid_table_name("A1"));
        assert!(!valid_table_name("XFD100"));
        assert!(!valid_table_name("1st"));
        assert!(!valid_table_name("a b"));
        assert!(!valid_table_name("R"));
        assert!(!valid_table_name(""));
    }
}
