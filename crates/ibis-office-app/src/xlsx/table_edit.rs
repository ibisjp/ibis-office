// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! テーブル: 挿入 > テーブル (⌘T) makes a table of a range, with Excel's
//! テーブルの作成 dialog (the range, 先頭行をテーブルの見出しとして使用する);
//! a table grows when a value is typed just under it or just right of it
//! (テーブルの自動拡張), and a formula typed into an empty column of it
//! fills the column (計算列). The table's style is laid on the cells as
//! Excel draws it (`ibis_pptx::styled_with_table`).

use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use ibis_doc_model::sheet_command::PutCell;
use ibis_doc_model::{CellInput, CellRange, CellValue, Command, Sheet, SheetCell, SheetCommand, SheetTable, SlideId, TableStyleInfo};

use crate::shell::editor::SlideEditor;

/// テーブルの作成・サイズ変更, while open.
pub(crate) struct TableDialog {
    pub(crate) range: Entity<InputState>,
    pub(crate) header: bool,
    pub(crate) target_table: Option<usize>,
    pub(crate) error: Option<SharedString>,
    _subscription: Subscription,
}

/// What a cell holds, to be put back as it is.
fn input_of(cell: Option<&SheetCell>) -> CellInput {
    match cell {
        Some(c) => match &c.formula {
            Some(f) => CellInput::Formula(f.clone()),
            None => CellInput::Value(c.value.clone()),
        },
        None => CellInput::Value(CellValue::Empty),
    }
}

/// `A1:C10` (or one cell) as a range.
fn parse_range(text: &str) -> Option<CellRange> {
    let text = text.trim().trim_start_matches('=').replace('$', "");
    let mut ends = text.split(':');
    let a = ibis_calc::formula::parser::parse_cell(ends.next()?)?;
    let b = match ends.next() {
        Some(s) => ibis_calc::formula::parser::parse_cell(s)?,
        None => a.clone(),
    };
    if ends.next().is_some() {
        return None;
    }
    Some(CellRange { first_row: a.row.min(b.row), last_row: a.row.max(b.row), first_column: a.column.min(b.column), last_column: a.column.max(b.column) })
}

/// The headings of a new table: each heading cell's text, or 列1, 列2, …
/// for an empty one, made unique as Excel makes them (名前2).
pub(crate) fn headings(texts: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (i, t) in texts.iter().enumerate() {
        let base = if t.trim().is_empty() { crate::shell::i18n::format("_xlsx.column-n", &[("n", &(i + 1))]) } else { t.clone() };
        let mut name = base.clone();
        let mut k = 2;
        while out.iter().any(|o| o.to_lowercase() == name.to_lowercase()) {
            name = format!("{base}{k}");
            k += 1;
        }
        out.push(name);
    }
    out
}

/// The cells to put so `sheet` shows `table` in its style: each cell of the
/// range whose format differs from that of `base` (the sheet as it will be,
/// a grown table's old edge taken off) with the style laid on, or whose
/// content `contents` gives, with its content.
fn styled_cells(sheet: &Sheet, base: &Sheet, table: &SheetTable, theme: &[(String, ibis_doc_model::Rgb)], contents: &[(u32, u32, CellInput)]) -> Vec<PutCell> {
    let styled = ibis_pptx::styled_with_table(base, table, theme);
    let r = table.range;
    let mut out = Vec::new();
    for row in r.first_row..=r.last_row {
        for column in r.first_column..=r.last_column {
            let before = sheet.cell(row, column);
            let after = styled.cell(row, column);
            let style_before = before.map(|c| &sheet.styles[c.style as usize]);
            let style_after = after.map(|c| &styled.styles[c.style as usize]);
            let content = contents.iter().find(|(a, b, _)| (*a, *b) == (row, column)).map(|(_, _, i)| i.clone());
            if style_before == style_after && content.is_none() {
                continue;
            }
            out.push(PutCell { row, column, input: content.unwrap_or_else(|| input_of(before)), style: style_after.cloned() });
        }
    }
    out
}

/// Whether consecutive cells' formulas are one formula filled down.
fn filled_down(above: &str, below: &str, rows: i64) -> bool {
    ibis_calc::formula::shift(above, rows, 0) == below
}

impl SlideEditor {
    fn theme_colors(&self) -> Vec<(String, ibis_doc_model::Rgb)> {
        self.presentation().workbook.as_ref().map(|w| w.theme_colors.clone()).unwrap_or_default()
    }

    /// The names of every table of the book.
    fn table_names(&self) -> Vec<String> {
        self.presentation().slides.iter().filter_map(ibis_doc_model::sheet_calc::sheet_of_slide).flat_map(|s| s.tables.iter().map(|t| t.name.clone())).collect()
    }

    /// 挿入 > テーブル: the dialog, on the selected range or the table
    /// around the selected cell.
    pub(crate) fn open_table_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let range = match self.selected_range() {
            Some(r) if r.first_row != r.last_row || r.first_column != r.last_column => Some(r),
            _ => self.current_region(),
        };
        let Some(range) = range else { return };
        if sheet.tables.iter().any(|t| overlaps(t.range, range)) {
            self.status = Some(crate::shell::i18n::text("_xlsx.tables-cannot-overlap").into());
            return cx.notify();
        }
        // Excel guesses a heading row: text over a row that is not all text.
        let all_text = |r: u32| (range.first_column..=range.last_column).all(|c| sheet.cell(r, c).is_some_and(|x| matches!(x.value, CellValue::Text(_))));
        let header = range.first_row < range.last_row && all_text(range.first_row) && !all_text(range.first_row + 1);
        let text = format!("={}:{}", ibis_doc_model::cell_name(range.first_column, range.first_row), ibis_doc_model::cell_name(range.last_column, range.last_row));
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |i, cx| {
            i.set_value(text, window, cx);
            i.focus(window, cx);
        });
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.confirm_table_dialog(window, cx);
            }
        });
        self.wb_mut().table_dialog = Some(TableDialog { range: input, header, target_table: None, error: None, _subscription: subscription });
        cx.notify();
    }

    /// テーブルのサイズ変更ダイアログを開く
    pub(crate) fn open_resize_table_dialog(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(t) = sheet.tables.get(index) else { return };
        let range = t.range;
        let text = format!("={}:{}", ibis_doc_model::cell_name(range.first_column, range.first_row), ibis_doc_model::cell_name(range.last_column, range.last_row));
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |i, cx| {
            i.set_value(text, window, cx);
            i.focus(window, cx);
        });
        let subscription = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = event {
                this.confirm_table_dialog(window, cx);
            }
        });
        self.wb_mut().table_dialog = Some(TableDialog { range: input, header: t.header_rows > 0, target_table: Some(index), error: None, _subscription: subscription });
        cx.notify();
    }

    pub(crate) fn close_table_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.wb_mut().table_dialog = None;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn set_table_dialog_header(&mut self, header: bool, cx: &mut Context<Self>) {
        if let Some(d) = self.wb_mut().table_dialog.as_mut() {
            d.header = header;
        }
        cx.notify();
    }

    /// The dialog's OK: the table made, or why not shown in it.
    pub(crate) fn confirm_table_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.wb().table_dialog.as_ref() else { return };
        let (text, header, target_table) = (d.range.read(cx).value().to_string(), d.header, d.target_table);
        let result = match parse_range(&text) {
            Some(range) => {
                if let Some(index) = target_table {
                    self.resize_table(index, range, cx);
                    Ok(())
                } else {
                    self.make_table(range, header, cx)
                }
            }
            None => Err(crate::shell::i18n::text("_xlsx.the-reference-is-not-valid").to_string()),
        };
        match result {
            Ok(()) => self.close_table_dialog(window, cx),
            Err(e) => {
                if let Some(d) = self.wb_mut().table_dialog.as_mut() {
                    d.error = Some(e.into());
                }
                cx.notify();
            }
        }
    }

    /// Make a table of `range` (its first row the headings if `header`,
    /// else a row of 列1, 列2, … put over it, the range moving down one).
    pub(crate) fn make_table(&mut self, range: CellRange, header: bool, cx: &mut Context<Self>) -> Result<(), String> {
        let slide = self.current_slide;
        let theme = self.theme_colors();
        let taken = self.table_names();
        let Some((_, sheet)) = self.current_sheet() else { return Err(crate::shell::i18n::text("_xlsx.there-is-no-sheet").into()) };
        if sheet.tables.iter().any(|t| overlaps(t.range, range)) {
            return Err(crate::shell::i18n::text("_xlsx.tables-cannot-overlap").into());
        }
        if sheet.merges.iter().any(|m| overlaps(*m, range)) {
            return Err(crate::shell::i18n::text("_xlsx.a-range-containing-merged-cells-cannot-be-made-i").into());
        }
        let mut sheet: Sheet = (**sheet).clone();
        let mut contents: Vec<(u32, u32, CellInput)> = Vec::new();
        let mut range = range;
        if !header {
            // The rows move down one, as Excel inserts the heading cells.
            let below = range.last_row + 1;
            if (range.first_column..=range.last_column).any(|c| sheet.cell(below, c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some())) {
                return Err(crate::shell::i18n::text("_xlsx.the-cell-just-below-the-range-is-not-empty-so-a").into());
            }
            for r in (range.first_row..=range.last_row).rev() {
                for c in range.first_column..=range.last_column {
                    let cell = sheet.cell(r, c).cloned();
                    let moved = cell.map(|x| SheetCell { row: r + 1, formula: x.formula.as_ref().map(|f| ibis_calc::formula::shift(f, 1, 0)), ..x });
                    contents.push((r + 1, c, input_of(moved.as_ref())));
                    sheet.put_cell(r + 1, c, moved);
                    sheet.put_cell(r, c, None);
                }
            }
            range.last_row += 1;
        }
        let texts: Vec<String> = (range.first_column..=range.last_column).map(|c| if header { sheet.cell(range.first_row, c).map_or(String::new(), |x| x.text.clone()) } else { String::new() }).collect();
        let columns = headings(&texts);
        for (i, name) in columns.iter().enumerate() {
            let c = range.first_column + i as u32;
            if sheet.cell(range.first_row, c).map(|x| x.text.as_str()) != Some(name.as_str()) || !matches!(sheet.cell(range.first_row, c).map(|x| &x.value), Some(CellValue::Text(_))) {
                contents.retain(|(a, b, _)| (*a, *b) != (range.first_row, c));
                contents.push((range.first_row, c, CellInput::Value(CellValue::Text(name.clone()))));
            }
        }
        let mut n = 1;
        let name = loop {
            let candidate = crate::shell::i18n::format("_xlsx.table-n", &[("n", &n)]);
            if !taken.iter().any(|t| t.eq_ignore_ascii_case(&candidate)) {
                break candidate;
            }
            n += 1;
        };
        let table = SheetTable {
            name,
            range,
            header_rows: 1,
            totals_rows: 0,
            columns,
            style: Some(TableStyleInfo { name: "TableStyleMedium2".into(), row_stripes: true, ..Default::default() }),
            // The headings' ▼, as Excel makes a table.
            filter: Some(ibis_doc_model::AutoFilter { range, columns: Vec::new() }),
            totals: Vec::new(),
        };
        let cells = styled_cells(&sheet, &sheet, &table, &theme, &contents);
        let index = sheet.tables.len();
        self.document.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index, table: Some(table), cells })]).map_err(|e| e.to_string())?;
        cx.notify();
        Ok(())
    }

    /// After a value was typed at (`row`, `column`): a table grows to take
    /// it in when it is just under or just right of one (テーブルの自動
    /// 拡張), and a formula typed into an empty column of a table fills the
    /// column (計算列). Each its own step to undo, as in Excel.
    pub(crate) fn after_table_entry(&mut self, slide: SlideId, row: u32, column: u32, input: &CellInput, cx: &mut Context<Self>) {
        if slide != self.current_slide || matches!(input, CellInput::Clear) {
            return;
        }
        let theme = self.theme_colors();
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(index) = sheet.tables.iter().position(|t| {
            let r = t.range;
            let under = t.totals_rows == 0 && row == r.last_row + 1 && (r.first_column..=r.last_column).contains(&column);
            let right = column == r.last_column + 1 && (r.first_row..=r.last_row).contains(&row);
            let inside = (r.first_row + t.header_rows..=r.last_row - t.totals_rows).contains(&row) && (r.first_column..=r.last_column).contains(&column);
            under || right || (inside && matches!(input, CellInput::Formula(_)))
        }) else {
            return;
        };
        let mut table = sheet.tables[index].clone();
        let r = table.range;
        let mut contents: Vec<(u32, u32, CellInput)> = Vec::new();
        let data = (r.first_row + table.header_rows)..=(r.last_row - table.totals_rows);
        if row == r.last_row + 1 && column <= r.last_column && column >= r.first_column {
            // A row more: the calculated columns' formulas come down.
            table.range.last_row += 1;
            for c in r.first_column..=r.last_column {
                if c == column || sheet.cell(row, c).is_some_and(|x| !x.value.is_empty() || x.formula.is_some()) {
                    continue;
                }
                let formulas: Vec<Option<String>> = data.clone().map(|rr| sheet.cell(rr, c).and_then(|x| x.formula.clone())).collect();
                let calculated = !formulas.is_empty() && formulas.iter().all(Option::is_some) && formulas.windows(2).all(|w| filled_down(w[0].as_ref().unwrap(), w[1].as_ref().unwrap(), 1));
                if calculated && let Some(Some(last)) = formulas.last() {
                    contents.push((row, c, CellInput::Formula(ibis_calc::formula::shift(last, 1, 0))));
                }
            }
        } else if column == r.last_column + 1 {
            // A column more, its heading 列N unless typed into the heading row.
            table.range.last_column += 1;
            let heading = if row == r.first_row && table.header_rows > 0 {
                sheet.cell(row, column).map_or(String::new(), |x| x.text.clone())
            } else {
                String::new()
            };
            let names = headings(&[table.columns.clone(), vec![heading.clone()]].concat());
            let new_name = names.last().cloned().unwrap_or_default();
            table.columns.push(new_name.clone());
            if !table.totals.is_empty() {
                table.totals.push(ibis_doc_model::TableTotal::default());
            }
            if table.header_rows > 0 && heading != new_name {
                contents.push((r.first_row, column, CellInput::Value(CellValue::Text(new_name))));
            }
            // A formula typed there: a calculated column at once.
            if let CellInput::Formula(f) = input
                && data.contains(&row)
                && data.clone().filter(|rr| *rr != row).all(|rr| sheet.cell(rr, column).is_none_or(|x| x.value.is_empty() && x.formula.is_none()))
            {
                for rr in data.clone().filter(|rr| *rr != row) {
                    contents.push((rr, column, CellInput::Formula(ibis_calc::formula::shift(f, rr as i64 - row as i64, 0))));
                }
            }
        } else if let CellInput::Formula(f) = input {
            // A formula into an empty column: the whole column.
            let others_empty = data.clone().filter(|rr| *rr != row).all(|rr| sheet.cell(rr, column).is_none_or(|x| x.value.is_empty() && x.formula.is_none()));
            if !others_empty || data.clone().count() < 2 {
                return;
            }
            for rr in data.clone().filter(|rr| *rr != row) {
                contents.push((rr, column, CellInput::Formula(ibis_calc::formula::shift(f, rr as i64 - row as i64, 0))));
            }
        } else {
            return;
        }
        let mut grown: Sheet = (**sheet).clone();
        for (rr, c, input) in &contents {
            if let CellInput::Value(v) = input {
                grown.put_cell(*rr, *c, Some(SheetCell { runs: None, phonetic: None, row: *rr, column: *c, value: v.clone(), text: String::new(), kind: Default::default(), formula: None, style: grown.cell(*rr, *c).map_or(0, |x| x.style) }));
            }
        }
        // The old table's outer border on the side it grew is an inner one
        // now: taken off where the style put it, before the style is laid
        // again (Excel moves it).
        let old_table = &sheet.tables[index];
        let edge: Vec<(u32, u32)> = if table.range.last_row > r.last_row {
            (r.first_column..=r.last_column).map(|c| (r.last_row, c)).collect()
        } else if table.range.last_column > r.last_column {
            (r.first_row..=r.last_row).map(|rr| (rr, r.last_column)).collect()
        } else {
            Vec::new()
        };
        let bottom = table.range.last_row > r.last_row;
        fn side(st: &mut ibis_doc_model::CellStyle, bottom: bool) -> &mut Option<ibis_doc_model::Border> {
            if bottom { &mut st.bottom } else { &mut st.right }
        }
        let mut probe = grown.clone();
        for &(rr, c) in &edge {
            if let Some(cell) = probe.cell(rr, c).cloned() {
                let mut st = probe.styles[cell.style as usize].clone();
                *side(&mut st, bottom) = None;
                let i = probe.style_index(st);
                probe.put_cell(rr, c, Some(SheetCell { style: i, ..cell }));
            }
        }
        let probed = ibis_pptx::styled_with_table(&probe, old_table, &theme);
        let mut base = grown.clone();
        for &(rr, c) in &edge {
            let (Some(cell), Some(p)) = (base.cell(rr, c).cloned(), probed.cell(rr, c)) else { continue };
            let mut st = base.styles[cell.style as usize].clone();
            let mut from_style = probed.styles[p.style as usize].clone();
            if *side(&mut st, bottom) == *side(&mut from_style, bottom) {
                *side(&mut st, bottom) = None;
                let i = base.style_index(st);
                base.put_cell(rr, c, Some(SheetCell { style: i, ..cell }));
            }
        }
        let cells = styled_cells(&grown, &base, &table, &theme, &contents);
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index, table: Some(table), cells })]) {
            log::warn!("table growth rejected: {err}");
        }
        cx.notify();
    }

    /// 範囲に変換: the table around the selected cell taken off, its look
    /// kept in its cells and its formulas' references made cell references.
    pub(crate) fn table_to_range(&mut self, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let Some((_, sheet)) = self.current_sheet() else { return };
        let Some(index) = sheet.tables.iter().position(|t| (t.range.first_row..=t.range.last_row).contains(&row) && (t.range.first_column..=t.range.last_column).contains(&column)) else {
            self.status = Some(crate::shell::i18n::text("_xlsx.select-a-cell-in-a-table"));
            return cx.notify();
        };
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetTable { slide, index, table: None, cells: Vec::new() })]) {
            log::warn!("table to range rejected: {err}");
        }
        cx.notify();
    }

    /// テーブルの作成 / サイズ変更, under the formula bar.
    pub(crate) fn render_table_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.sheet_range_pick_active(){return None;}
        let d = self.sheet_state()?.table_dialog.as_ref()?;
        let is_resize = d.target_table.is_some();
        let title = if is_resize {
            crate::shell::i18n::text("_xlsx.table-resize")
        } else {
            crate::shell::i18n::text("_xlsx.create-table")
        };
        let desc = if is_resize {
            crate::shell::i18n::text("_xlsx.specify-the-new-data-range-for-your-table")
        } else {
            crate::shell::i18n::text("_xlsx.specify-the-data-range-to-convert-to-a-table")
        };
        let theme = cx.theme().clone();
        let dialog = v_flex()
            .id("table-dialog")
            .absolute()
            .top(px(34.))
            .left(px(8.))
            .w(px(340.))
            .p_3()
            .gap_2()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .rounded_md()
            .shadow_lg()
            .occlude()
            .child(div().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(desc)
            .child(super::range_field::local_range_field(&d.range,"table-dialog-range",false,cx))
            .when(!is_resize, |b| b.child(Checkbox::new("table-header").label(crate::shell::i18n::text("_xlsx.my-table-has-headers")).checked(d.header).on_click(cx.listener(|this, on: &bool, _, cx| this.set_table_dialog_header(*on, cx)))))
            .when_some(d.error.clone(), |b, e| b.child(div().text_xs().text_color(theme.danger).child(e)))
            .child(
                crate::shell::chrome::action_bar_host((crate::shell::chrome::action_bar()
                    .action(crate::shell::chrome::ActionDefinition::new("table-ok", "", crate::shell::chrome::ActionButtonKind::Primary).label("OK").on_click(cx.listener(|this, _, window, cx| this.confirm_table_dialog(window, cx))))
                    .action(crate::shell::chrome::ActionDefinition::new("table-cancel", "", crate::shell::chrome::ActionButtonKind::Normal).label(crate::shell::i18n::text("_shell.cancel")).on_click(cx.listener(|this, _, window, cx| this.close_table_dialog(window, cx)))))),
            );
        Some(deferred(dialog).with_priority(1).into_any_element())
    }
}

fn overlaps(a: CellRange, b: CellRange) -> bool {
    a.first_row <= b.last_row && b.first_row <= a.last_row && a.first_column <= b.last_column && b.first_column <= a.last_column
}

#[cfg(test)]
mod tests {
    use super::{headings, parse_range};

    #[test]
    fn headings_as_excel_names_them() {
        let names = headings(&["名前".into(), "".into(), "名前".into(), "点".into()]);
        // The made-up heading is in the language shown (Japanese or English, by the OS).
        assert!(names[1] == "列2" || names[1] == "Column2", "{names:?}");
        assert_eq!((names[0].as_str(), names[2].as_str(), names[3].as_str()), ("名前", "名前2", "点"));
        assert_eq!(parse_range("=$A$1:C5").map(|r| (r.first_row, r.last_row, r.first_column, r.last_column)), Some((0, 4, 0, 2)));
    }
}
