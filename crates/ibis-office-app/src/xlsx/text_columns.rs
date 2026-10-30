// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! データ > 区切り位置: the text of a column of cells split into the
//! columns beside it, as Excel's wizard: by delimiters (tab, semicolon,
//! comma, space, another character; runs of them as one; a quote that
//! keeps delimiters inside it) or at fixed character positions; each part
//! read as typed (標準: numbers, dates) or kept as text (文字列); put from a
//! destination cell, asking before it replaces what is there.
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_calc::input::{Typed, parse_date_in_order, parse_input};
use ibis_doc_model::{CellEdit, CellInput, CellRange, CellValue, Command, SheetCommand, SlideId, StyleChange};
use super::import_text::ColumnType;
use gpui_kit::component::menu::DropdownMenu as _;

/// Converts split parts of a row into CellEdits and number format changes according to column types.
pub(crate) fn convert_parts(
    parts: &[String],
    columns: &[ColumnType],
    default_type: ColumnType,
    to_row: u32,
    destination_column: u32,
    source_range: Option<CellRange>,
    date1904: bool,
    sheet: Option<&ibis_doc_model::Sheet>,
) -> (Vec<CellEdit>, Vec<(CellRange, String)>) {
    let mut edits = Vec::new();
    let mut formats = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let kind = columns.get(i).copied().unwrap_or(default_type);
        let column = destination_column + i as u32;
        if kind == ColumnType::Skip {
            if let Some(src) = source_range {
                if column == src.first_column && (src.first_row..=src.last_row).contains(&to_row) {
                    edits.push(CellEdit { row: to_row, column, input: CellInput::Clear });
                }
            }
            continue;
        }
        let one = CellRange { first_row: to_row, last_row: to_row, first_column: column, last_column: column };
        let input = if part.is_empty() {
            CellInput::Clear
        } else {
            match kind {
                ColumnType::Text => {
                    formats.push((one, "@".into()));
                    CellInput::Value(CellValue::Text(part.clone()))
                }
                ColumnType::Date(order) => {
                    if let Some(serial) = parse_date_in_order(part, order, date1904) {
                        formats.push((one, "yyyy/m/d".into()));
                        CellInput::Value(CellValue::Number(serial))
                    } else {
                        formats.push((one, "@".into()));
                        CellInput::Value(CellValue::Text(part.clone()))
                    }
                }
                _ => match parse_input(part, date1904) {
                    Typed::Formula(f) => CellInput::Formula(f),
                    Typed::Value(v, format) => {
                        let general = sheet.is_none_or(|s| s.cell(to_row, column).is_none_or(|c| s.styles.get(c.style as usize).is_none_or(|st| st.number_format == "General")));
                        if let (Some(code), true) = (format, general) {
                            formats.push((one, code.to_string()));
                        }
                        CellInput::Value(v)
                    }
                },
            }
        };
        edits.push(CellEdit { row: to_row, column, input });
    }
    (edits, formats)
}

/// How the text is split.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Split {
    /// At any of the characters; runs of them as one if `consecutive`;
    /// inside `quote` they are text (the quotes go, a doubled one stays).
    Delimited { delimiters: Vec<char>, consecutive: bool, quote: Option<char> },
    /// Before the characters at these positions (counted from 0, rising).
    Fixed(Vec<usize>),
}

/// `text` split as `split` says.
pub(crate) fn split_text(text: &str, split: &Split) -> Vec<String> {
    match split {
        Split::Fixed(breaks) => {
            // Parts past the text's end are left out, as Excel does.
            let chars: Vec<char> = text.chars().collect();
            let (mut out, mut start) = (Vec::new(), 0);
            for &b in breaks {
                if b <= start || b >= chars.len() {
                    continue;
                }
                out.push(chars[start..b].iter().collect::<String>().trim_end().to_string());
                start = b;
            }
            out.push(chars[start..].iter().collect::<String>().trim_end().to_string());
            out
        }
        Split::Delimited { delimiters, consecutive, quote } => {
            let mut out = Vec::new();
            let mut part = String::new();
            let mut chars = text.chars().peekable();
            let mut quoted = false;
            let mut last_was_delimiter = false;
            while let Some(c) = chars.next() {
                if Some(c) == *quote {
                    if quoted && chars.peek() == Some(&c) {
                        part.push(c);
                        chars.next();
                    } else {
                        quoted = !quoted;
                    }
                    last_was_delimiter = false;
                    continue;
                }
                if !quoted && delimiters.contains(&c) {
                    if *consecutive && last_was_delimiter {
                        continue;
                    }
                    out.push(std::mem::take(&mut part));
                    last_was_delimiter = true;
                    continue;
                }
                part.push(c);
                last_was_delimiter = false;
            }
            out.push(part);
            out
        }
    }
}

pub(crate) struct TextColumnsPanel {
    /// The cells split: one column.
    pub(crate) source: CellRange,
    pub(crate) fixed: bool,
    pub(crate) tab: bool,
    pub(crate) semicolon: bool,
    pub(crate) comma: bool,
    pub(crate) space: bool,
    /// Another delimiter (its first character).
    pub(crate) other: Entity<InputState>,
    pub(crate) consecutive: bool,
    /// The quote: `"`, `'` or none.
    pub(crate) quote: Option<char>,
    /// Fixed width: the positions, as `3, 8`.
    pub(crate) breaks: Entity<InputState>,
    /// Each part kept as text (文字列) rather than read as typed (標準).
    pub(crate) as_text: bool,
    /// Per-column data formats (標準, 文字列, 日付, 読み込まない).
    pub(crate) columns: Vec<ColumnType>,
    pub(crate) destination: Entity<InputState>,
    /// Asked whether to replace the cells there: the next apply does.
    pub(crate) confirm: bool,
    pub(crate) message: String,
    pub(crate) ui: std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<String, Bounds<Pixels>>>>,
    slide: SlideId,
}

impl TextColumnsPanel {
    pub(crate) fn column(&self, index: usize) -> ColumnType {
        self.columns.get(index).copied().unwrap_or(if self.as_text { ColumnType::Text } else { ColumnType::Standard })
    }

    pub(crate) fn set_column_type(&mut self, index: usize, kind: ColumnType) {
        let def = if self.as_text { ColumnType::Text } else { ColumnType::Standard };
        if self.columns.len() <= index {
            self.columns.resize(index + 1, def);
        }
        self.columns[index] = kind;
    }

    fn split(&self, cx: &App) -> Result<Split, String> {
        if self.fixed {
            let text = self.breaks.read(cx).value().to_string();
            let mut breaks = Vec::new();
            for piece in text.split([',', '、', ' ']).filter(|p| !p.trim().is_empty()) {
                match piece.trim().parse::<usize>() {
                    Ok(n) if n > 0 => breaks.push(n),
                    _ => return Err(crate::shell::i18n::text("fix.text-columns.enter-break-positions-character").into()),
                }
            }
            if breaks.is_empty() {
                return Err(crate::shell::i18n::text("fix.text-columns.enter-break-positions-e-g").into());
            }
            breaks.sort();
            breaks.dedup();
            return Ok(Split::Fixed(breaks));
        }
        let mut delimiters = Vec::new();
        for (on, c) in [(self.tab, '\t'), (self.semicolon, ';'), (self.comma, ','), (self.space, ' ')] {
            if on {
                delimiters.push(c);
            }
        }
        delimiters.extend(self.other.read(cx).value().chars().next());
        if delimiters.is_empty() {
            return Err(crate::shell::i18n::text("fix.text-columns.select-least-one-delimiter").into());
        }
        Ok(Split::Delimited { delimiters, consecutive: self.consecutive, quote: self.quote })
    }
}

impl SlideEditor {
    pub(crate) fn set_text_columns_type(&mut self, index: usize, kind: ColumnType, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.text_columns.as_mut() {
            p.set_column_type(index, kind);
            p.confirm = false;
        }
        cx.notify();
    }

    pub(crate) fn open_text_columns_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some((column, row)) = self.current_cell() else { return };
        let source = self.selected_range().unwrap_or(CellRange { first_row: row, last_row: row, first_column: column, last_column: column });
        let input = |text: String, placeholder: &'static str, window: &mut Window, cx: &mut Context<Self>| {
            let input = cx.new(|cx| InputState::new(window, cx).placeholder(crate::shell::i18n::text(placeholder)));
            input.update(cx, |i, cx| i.set_value(text, window, cx));
            input
        };
        let panel = TextColumnsPanel {
            source,
            fixed: false,
            tab: true,
            semicolon: false,
            comma: false,
            space: false,
            other: input(String::new(), "fix.text-columns.other-1-character", window, cx),
            consecutive: false,
            quote: Some('"'),
            breaks: input(String::new(), "3, 8", window, cx),
            as_text: false,
            columns: Vec::new(),
            destination: input(ibis_doc_model::cell_name(source.first_column, source.first_row), "A1", window, cx),
            confirm: false,
            message: if source.first_column != source.last_column { crate::shell::i18n::text("fix.text-columns.only-one-column-can-converted").into() } else { String::new() },
            ui: Default::default(),
            slide: self.current_slide,
        };
        let state = self.wb_mut();
        state.custom_format = None;
        state.page_setup = false;
        state.names = None;
        state.validation = None;
        state.find = None;
        state.link = None;
        state.conditional_edit = None;
        state.sort_edit = None;
        state.close_settings();
        state.panels.text_columns = Some(panel);
        self.sidebar_open = true;
        cx.notify();
    }

    fn text_columns_message(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(p) = self.wb_mut().panels.text_columns.as_mut() {
            p.message = text;
        }
        cx.notify();
    }

    /// The texts of the source cells, by row (an empty cell none).
    pub(crate) fn text_columns_source(&self) -> Vec<(u32, String)> {
        let (Some(p), Some((_, sheet))) = (self.sheet_state().and_then(|s| s.panels.text_columns.as_ref()), self.current_sheet()) else { return Vec::new() };
        (p.source.first_row..=p.source.last_row)
            .filter_map(|r| sheet.cell(r, p.source.first_column).filter(|c| !c.text.is_empty() || !c.value.is_empty()).map(|c| (r, if c.text.is_empty() { format!("{:?}", c.value) } else { c.text.clone() })))
            .collect()
    }

    pub(crate) fn apply_text_columns_panel(&mut self, cx: &mut Context<Self>) {
        if self.protection_refuses(crate::xlsx::protection::Guarded::Cells, cx) {
            return;
        }
        let Some(p) = self.sheet_state().and_then(|s| s.panels.text_columns.as_ref()) else { return };
        if p.slide != self.current_slide {
            self.text_columns_message(crate::shell::i18n::text("_xlsx.the-sheet-has-changed-reopen-it").into(), cx);
            return;
        }
        if p.source.first_column != p.source.last_column {
            self.text_columns_message(crate::shell::i18n::text("fix.text-columns.only-one-column-can-converted").into(), cx);
            return;
        }
        let split = match p.split(cx) {
            Ok(s) => s,
            Err(e) => return self.text_columns_message(e, cx),
        };
        let Some(destination) = super::dedupe_edit::parse_range(&p.destination.read(cx).value()) else {
            return self.text_columns_message(crate::shell::i18n::text("fix.text-columns.enter-destination-cell-e-g").into(), cx);
        };
        let (as_text, confirm, slide, source, columns) = (p.as_text, p.confirm, p.slide, p.source, p.columns.clone());
        let date1904 = self.presentation().workbook.as_ref().is_some_and(|w| w.date1904);
        let rows = self.text_columns_source();
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut edits = Vec::new();
        let mut formats: Vec<(CellRange, String)> = Vec::new();
        let mut occupied = false;
        let default_type = if as_text { ColumnType::Text } else { ColumnType::Standard };
        for (row, text) in &rows {
            let to_row = destination.first_row + (row - source.first_row);
            let parts = split_text(text, &split);
            let (part_edits, part_formats) = convert_parts(
                &parts,
                &columns,
                default_type,
                to_row,
                destination.first_column,
                Some(source),
                date1904,
                Some(sheet),
            );
            for edit in &part_edits {
                let at = (edit.row, edit.column);
                let is_source = edit.column == source.first_column && (source.first_row..=source.last_row).contains(&edit.row);
                if !is_source && sheet.cell(at.0, at.1).is_some_and(|c| !c.value.is_empty() || c.formula.is_some()) {
                    occupied = true;
                }
            }
            edits.extend(part_edits);
            formats.extend(part_formats);
        }
        if edits.is_empty() {
            return self.text_columns_message(crate::shell::i18n::text("fix.text-columns.there-no-data-split").into(), cx);
        }
        // Excel asks before it replaces the cells there.
        if occupied && !confirm {
            if let Some(p) = self.wb_mut().panels.text_columns.as_mut() {
                p.confirm = true;
            }
            return self.text_columns_message(crate::shell::i18n::text("fix.text-columns.there-already-data-here-replace").into(), cx);
        }
        let mut commands = vec![Command::Sheet(SheetCommand::SetCells { slide, edits })];
        for (range, code) in formats {
            commands.push(Command::Sheet(SheetCommand::SetFormat { slide, ranges: vec![range], change: StyleChange::NumberFormat(code) }));
        }
        if let Err(error) = self.document.execute(commands) {
            return self.text_columns_message(error.to_string(), cx);
        }
        self.wb_mut().last_fill = None;
        if let Some(p) = self.wb_mut().panels.text_columns.as_mut() {
            p.confirm = false;
        }
        self.text_columns_message(crate::shell::i18n::format("text-columns.done", &[("rows", &rows.len())]), cx);
    }

    pub(crate) fn text_columns_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let preview_rows: Vec<Vec<String>> = {
            let p = self.sheet_state()?.panels.text_columns.as_ref()?;
            match p.split(cx) {
                Ok(split) => self.text_columns_source().iter().take(2).map(|(_, t)| split_text(t, &split)).collect(),
                Err(_) => Vec::new(),
            }
        };
        let p = self.sheet_state()?.panels.text_columns.as_ref()?;
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
        type Toggle = fn(&mut TextColumnsPanel);
        let toggle = |id: &'static str, label: String, flip: Toggle| {
            mark(
                id,
                crate::shell::chrome::action_button(id)
                    .small()
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(p) = this.wb_mut().panels.text_columns.as_mut() {
                            flip(p);
                            p.confirm = false;
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            )
        };
        let option = |id: &'static str, on: bool, name: &str, radio: bool, flip: Toggle| {
            let handler = cx.listener(move |this, _, _, cx| {
                if let Some(p) = this.wb_mut().panels.text_columns.as_mut() {
                    flip(p);
                    p.confirm = false;
                }
                cx.notify();
            });
            let label = crate::shell::i18n::text(name);
            let control = if radio {
                crate::shell::chrome::radio_button(id, on, false, label, handler).into_any_element()
            } else {
                crate::shell::chrome::checkbox_button(id, label, on, false, handler).into_any_element()
            };
            mark(id, control)
        };
        let source = format!("{}:{}", ibis_doc_model::cell_name(p.source.first_column, p.source.first_row), ibis_doc_model::cell_name(p.source.last_column, p.source.last_row));
        let mut body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(crate::shell::i18n::format("text-columns.source", &[("cells", &source)]))
            .child(
                crate::shell::chrome::radio_group().layout(crate::shell::chrome::RadioGroupLayout::Flow)
                    .child(option("text-columns-delimited", !p.fixed, "fix.text-columns.delimiters", true, |p| p.fixed = false))
                    .child(option("text-columns-fixed", p.fixed, "fix.text-columns.fixed-width", true, |p| p.fixed = true)),
            );
        if p.fixed {
            body = body.child(crate::shell::i18n::text("fix.text-columns.break-positions-characters-start")).child(mark("breaks", crate::shell::chrome::text_field(&p.breaks).full_width().into_any_element()));
        } else {
            body = body
                .child(crate::shell::i18n::text("fix.text-columns.delimiters"))
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(option("text-columns-tab", p.tab, "fix.text-columns.tab", false, |p| p.tab = !p.tab))
                        .child(option("text-columns-semicolon", p.semicolon, "fix.text-columns.semicolon", false, |p| p.semicolon = !p.semicolon))
                        .child(option("text-columns-comma", p.comma, "fix.text-columns.comma", false, |p| p.comma = !p.comma))
                        .child(option("text-columns-space", p.space, "fix.text-columns.space", false, |p| p.space = !p.space)),
                )
                .child(mark("other", crate::shell::chrome::text_field(&p.other).width(px(72.)).into_any_element()))
                .child(
                    h_flex()
                        .gap_1()
                        .flex_wrap()
                        .child(option("text-columns-consecutive", p.consecutive, "fix.text-columns.treat-consecutive-delimiters-one", false, |p| p.consecutive = !p.consecutive))
                )
                .child({let quote=p.quote;let entries=[Some('"'),Some('\''),None].into_iter().map(|value|crate::shell::chrome::MenuItem::new("xlsx.text-columns.quote",value.map(|c|c.to_string()).unwrap_or_else(||crate::shell::i18n::text("_shell.none").to_string())).checked(quote==value).on(move|e,_,cx|{if let Some(p)=e.wb_mut().panels.text_columns.as_mut(){p.quote=value;p.confirm=false;}cx.notify();}).into()).collect();
                    crate::shell::chrome::dropdown_row(crate::shell::i18n::text("text-columns.quote-label"),crate::shell::chrome::Dropdown::from_entries("text-columns-quote-dropdown",quote.map(|c|c.to_string()).unwrap_or_else(||crate::shell::i18n::text("_shell.none").to_string()),entries,true,0,&cx.weak_entity()).into_any_element())
                });
        }
        let shown_cols = preview_rows.iter().map(Vec::len).max().unwrap_or(0).min(8);
        let mut fmt_row=v_flex().gap_2();
        for i in 0..shown_cols{
            let kind=p.column(i);let mut candidates=Vec::new();let mut value=ColumnType::Standard;loop{candidates.push(value);value=value.next();if value==ColumnType::Standard{break;}}
            let entries=candidates.into_iter().map(|value|crate::shell::chrome::MenuItem::new("xlsx.text-columns.column-format",crate::shell::i18n::text(value.label())).checked(value==kind).on(move|e,_,cx|{if let Some(p)=e.wb_mut().panels.text_columns.as_mut(){p.set_column_type(i,value);p.confirm=false;}cx.notify();}).into()).collect::<Vec<_>>();let owner=cx.weak_entity();
            let dropdown=crate::shell::chrome::Dropdown::from_entries("text-columns-col-type",crate::shell::i18n::text(kind.label()),entries,true,i,&owner).button_width(crate::shell::chrome::FIELD_WIDTH).into_any_element();
            fmt_row=fmt_row.child(crate::shell::chrome::dropdown_row(crate::shell::i18n::format("text-columns.column-label",&[("n",&(i+1))]),mark(&format!("text-columns-column-{i}"),dropdown.into_any_element())));
        }
        body = body
            .child(fmt_row)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(crate::shell::i18n::text("fix.text-columns.destination"))
                    .child(div().flex_1().child(mark("destination", super::range_field::local_range_field(&p.destination,"text_columns-p-destination",true,cx).into_any_element()))),
            )
            .child(crate::shell::i18n::text("fix.text-columns.preview"));
        for record in &preview_rows {
            let line = record
                .iter()
                .enumerate()
                .map(|(i, val)| {
                    let kind = p.column(i);
                    if kind == ColumnType::Skip {
                        format!("[{val} ({})]", crate::shell::i18n::text("_xlsx.do-not-import-column-skip"))
                    } else {
                        val.clone()
                    }
                })
                .collect::<Vec<_>>()
                .join(" │ ");
            body = body.child(div().text_sm().child(SharedString::from(line)));
        }
        body = body
            .children(crate::shell::chrome::panel_message(&p.message))
            ;
        let panel_actions = crate::shell::chrome::action_bar()
                    .action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("text-columns-apply", "", crate::shell::chrome::ActionButtonKind::Primary).label(crate::shell::i18n::text(if p.confirm { "fix.text-columns.replace" } else { "fix.text-columns.split" })).on_click(cx.listener(|this, _, _, cx| this.apply_text_columns_panel(cx)))))

            .action(crate::shell::chrome::dialog_cancel_definition(cx));
        Some(SidebarPanel::with_footer("fix.text-columns.text-columns", body.into_any_element(), panel_actions))
    }
}

#[cfg(test)]
mod tests {
    use super::{Split, split_text};

    fn delimited(delimiters: &[char], consecutive: bool, quote: Option<char>) -> Split {
        Split::Delimited { delimiters: delimiters.to_vec(), consecutive, quote }
    }

    #[test]
    fn delimiters_runs_and_quotes() {
        assert_eq!(split_text("a,b,,c", &delimited(&[','], false, None)), ["a", "b", "", "c"]);
        assert_eq!(split_text("a,b,,c", &delimited(&[','], true, None)), ["a", "b", "c"]);
        assert_eq!(split_text("a b;c", &delimited(&[' ', ';'], false, None)), ["a", "b", "c"]);
        assert_eq!(split_text("\"x,y\",z", &delimited(&[','], false, Some('"'))), ["x,y", "z"]);
        assert_eq!(split_text("\"say \"\"hi\"\"\",z", &delimited(&[','], false, Some('"'))), ["say \"hi\"", "z"]);
        assert_eq!(split_text("'x,y',z", &delimited(&[','], false, None)), ["'x", "y'", "z"]);
        assert_eq!(split_text("東京都\t千代田区", &delimited(&['\t'], false, Some('"'))), ["東京都", "千代田区"]);
    }

    #[test]
    fn fixed_positions() {
        assert_eq!(split_text("2026-10-07", &Split::Fixed(vec![4, 5, 7, 8])), ["2026", "-", "10", "-", "07"]);
        assert_eq!(split_text("ab", &Split::Fixed(vec![1, 5])), ["a", "b"]);
        assert_eq!(split_text("東京都千代田区", &Split::Fixed(vec![3])), ["東京都", "千代田区"]);
    }

    #[test]
    fn column_conversion_types_and_skipping() {
        use super::super::import_text::ColumnType;
        use ibis_calc::input::DateOrder;
        use ibis_doc_model::{CellInput, CellValue};

        let parts = vec![
            "2026/10/17".to_string(),
            "012345".to_string(),
            "ignore_me".to_string(),
            "100.5".to_string(),
        ];
        let columns = vec![
            ColumnType::Date(DateOrder::Ymd),
            ColumnType::Text,
            ColumnType::Skip,
            ColumnType::Standard,
        ];
        let (edits, formats) = super::convert_parts(
            &parts,
            &columns,
            ColumnType::Standard,
            0,
            0,
            None,
            false,
            None,
        );

        // Column 0: Date -> serial number (> 40000.0) with yyyy/m/d format
        assert_eq!(edits[0].column, 0);
        assert!(matches!(edits[0].input, CellInput::Value(CellValue::Number(n)) if n > 40000.0));
        assert_eq!(formats[0].1, "yyyy/m/d");

        // Column 1: Text -> text value with @ format, leading zero preserved
        assert_eq!(edits[1].column, 1);
        assert_eq!(edits[1].input, CellInput::Value(CellValue::Text("012345".into())));
        assert_eq!(formats[1].1, "@");

        // Column 2: Skip -> omitted! No edit for column 2
        // Column 3: Standard -> number 100.5
        assert_eq!(edits[2].column, 3);
        assert_eq!(edits[2].input, CellInput::Value(CellValue::Number(100.5)));
        assert_eq!(edits.len(), 3);
    }

    #[test]
    fn skip_first_column_clears_source_when_in_place() {
        use super::super::import_text::ColumnType;
        use ibis_doc_model::{CellInput, CellRange, CellValue};

        let parts = vec!["raw_text".to_string(), "second".to_string()];
        let columns = vec![ColumnType::Skip, ColumnType::Standard];
        let source = CellRange { first_row: 0, last_row: 0, first_column: 0, last_column: 0 };
        let (edits, _) = super::convert_parts(
            &parts,
            &columns,
            ColumnType::Standard,
            0,
            0,
            Some(source),
            false,
            None,
        );

        // Column 0 was in-place source -> cleared!
        assert_eq!(edits[0].column, 0);
        assert_eq!(edits[0].input, CellInput::Clear);

        // Column 1 -> "second"
        assert_eq!(edits[1].column, 1);
        assert_eq!(edits[1].input, CellInput::Value(CellValue::Text("second".into())));
    }
}
