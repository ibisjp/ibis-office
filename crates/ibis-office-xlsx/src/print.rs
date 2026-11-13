// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ページ設定 in the sidebar (ファイル > ページ設定): the orientation, the
//! paper, the margins, the scale (or one page), the print area, manual
//! page breaks, the header and footer (Excel's presets) and the title rows
//! printed on every page (印刷タイトル) of the sheet shown.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, SheetCommand, SheetPrint};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The paper sizes offered: name, width and height (EMU).
pub(crate) const PAPERS: &[(&str, (i64, i64))] = &[
    ("A4", SheetPrint::A4),
    ("A3", (297 * 36_000, 420 * 36_000)),
    ("A5", (148 * 36_000, 210 * 36_000)),
    ("B4", (257 * 36_000, 364 * 36_000)),
    ("B5", (182 * 36_000, 257 * 36_000)),
    ("fix.print.postcard", SheetPrint::HAGAKI),
    ("docx.page.letter", (7_772_400, 10_058_400)),
];

/// Header and footer presets (Excel's list, in the centre): label and code.
pub(crate) const HEADER_PRESETS: &[(&str, Option<&str>)] = &[("_shell.none", None), ("docx.page-number", Some("&C&P")), ("_xlsx.page-1-of", Some("@_xlsx.header-page-of")), ("_xlsx.sheet-name", Some("&C&A")), ("docx.field.filename", Some("&C&F"))];

/// Excel's margin presets (inches; left, right, top, bottom), as the page layout tab's Margins menu of Excel sets them:
/// Normal 0.7 / 0.7 / 0.75 / 0.75, Narrow 0.25 / 0.25 / 0.75 / 0.75, Wide 1 / 1 / 1 / 1 (the header and footer margins,
/// 0.3 / 0.3 / 0.5 in Excel, are not part of this model). The dialog's segments and the Margins menu both use this table
/// (Excel's menu lists them as Normal, Wide, Narrow; the menu orders them so).
pub(crate) const MARGINS: &[(&str, [f64; 4])] = &[("_xlsx.normal", [0.7, 0.7, 0.75, 0.75]), ("_xlsx.narrow", [0.25, 0.25, 0.75, 0.75]), ("_xlsx.wide", [1.0, 1.0, 1.0, 1.0])];

/// 印刷およびPDF書き出しの対象 (Print What: XLSX-SEC-019)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PrintWhat {
    /// 作業中のシート（現在のシートのみ）
    #[default]
    ActiveSheet,
    /// ブック全体（すべてのシート）
    EntireWorkbook,
    /// 選択した部分（現在選択されているセル範囲のみ）
    Selection,
}

#[ibis_office_macros::editor_ext(Print)]
impl SlideEditor {
    /// 現在の印刷対象（作業中のシート / ブック全体 / 選択した部分）
    pub fn print_what(&self) -> PrintWhat {
        self.wb().print_what
    }

    /// 印刷対象を切り替える
    pub fn set_print_what(&mut self, what: PrintWhat, cx: &mut Context<Self>) {
        self.wb_mut().print_what = what;
        cx.notify();
    }

    pub fn current_print(&self) -> Option<SheetPrint> {
        self.current_sheet().map(|(_, s)| s.print.clone())
    }

    /// Change the print settings (and the print area, if `area` is `Some`).
    pub fn set_print(&mut self, change: impl FnOnce(&mut SheetPrint), area: Option<Option<CellRange>>, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(mut print) = self.current_print() else { return };
        let old_print = print.clone();
        let old_area = self.current_sheet().and_then(|(_, s)| s.print_area);
        change(&mut print);
        if print == old_print && (area.is_none() || area == Some(old_area)) {
            return;
        }
        let slide = self.current_slide;
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetPrint { slide, print, area })]) {
            log::warn!("page setup rejected: {err}");
        }
        cx.notify();
    }

    /// ページ レイアウト > 拡大縮小印刷 > 横 (`xlsx.scale-width-*`):
    /// `pages == 0` は自動、1 以上はそのページ数に合わせて横幅を縮小する.
    pub(crate) fn set_print_fit_width(&mut self, pages: u32, cx: &mut Context<Self>) {
        self.set_print(
            move |p| {
                p.fit_to = match (pages, p.fit_to) {
                    (0, Some((_, 0)) | None) => None,
                    (w, Some((_, h))) => Some((w, h)),
                    (w, None) => Some((w, 0)),
                };
            },
            None,
            cx,
        );
    }

    /// ページ レイアウト > 拡大縮小印刷 > 縦 (`xlsx.scale-height-*`):
    /// `pages == 0` は自動、1 以上はそのページ数に合わせて縦幅を縮小する.
    pub(crate) fn set_print_fit_height(&mut self, pages: u32, cx: &mut Context<Self>) {
        self.set_print(
            move |p| {
                p.fit_to = match (pages, p.fit_to) {
                    (0, Some((0, _)) | None) => None,
                    (h, Some((w, _))) => Some((w, h)),
                    (h, None) => Some((0, h)),
                };
            },
            None,
            cx,
        );
    }

    /// ページ レイアウト > 拡大縮小印刷 > 拡大/縮小 (`xlsx.scale-*`):
    /// ページ数指定 (`fit_to`) を解除し、指定の倍率 (%) を設定する.
    pub(crate) fn set_print_scale_percent(&mut self, scale: u32, cx: &mut Context<Self>) {
        let clamped = scale.clamp(10, 400);
        self.set_print(
            move |p| {
                p.fit_to = None;
                p.scale = clamped;
            },
            None,
            cx,
        );
    }

    /// A page break before the selected row (or taken off if there is one).
    pub(crate) fn toggle_row_break(&mut self, cx: &mut Context<Self>) {
        let Some((_, row)) = self.current_cell() else { return };
        if row == 0 {
            return;
        }
        self.set_print(
            |p| {
                if let Some(i) = p.row_breaks.iter().position(|&b| b == row) {
                    p.row_breaks.remove(i);
                } else {
                    p.row_breaks.push(row);
                    p.row_breaks.sort_unstable();
                }
            },
            None,
            cx,
        );
    }

    /// 印刷範囲の設定: the selected range as the sheet's print area.
    pub(crate) fn set_print_area_from_selection(&mut self, cx: &mut Context<Self>) {
        let range = self.selected_range();
        self.set_print(|_| {}, Some(range), cx);
    }

    /// 印刷範囲に追加: expand the sheet's print area to include the selected range.
    pub(crate) fn add_to_print_area_from_selection(&mut self, cx: &mut Context<Self>) {
        let Some(range) = self.selected_range() else { return };
        let existing = self.current_sheet().and_then(|(_, s)| s.print_area);
        let new_area = match existing {
            Some(old) => ibis_doc_model::CellRange {
                first_column: old.first_column.min(range.first_column),
                last_column: old.last_column.max(range.last_column),
                first_row: old.first_row.min(range.first_row),
                last_row: old.last_row.max(range.last_row),
            },
            None => range,
        };
        self.set_print(|_| {}, Some(Some(new_area)), cx);
    }

    /// 印刷範囲のクリア: clear the sheet's print area.
    pub(crate) fn clear_print_area(&mut self, cx: &mut Context<Self>) {
        self.set_print(|_| {}, Some(None), cx);
    }

    /// The sheet's print titles name: its rows (from 0), if any.
    pub fn print_title_rows(&self) -> Option<(u32, u32)> {
        let wb = self.presentation().workbook.as_ref()?;
        let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide))?;
        let name = wb.names.iter().find(|n| n.sheet == Some(index) && n.name.eq_ignore_ascii_case("_xlnm.Print_Titles"))?;
        super::header_footer::parse_titles(&name.formula).0
    }

    /// 印刷タイトル: the selected rows repeated at the top of each page
    /// (`None`: none), the title columns kept.
    pub fn set_print_title_rows(&mut self, rows: Option<(u32, u32)>, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(wb) = self.presentation().workbook.clone() else { return };
        let Some(index) = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide)) else { return };
        let sheet = ibis_calc::formula::quote_sheet(&wb.sheets[index].name);
        let old = wb.names.iter().find(|n| n.sheet == Some(index) && n.name.eq_ignore_ascii_case("_xlnm.Print_Titles"));
        let columns = old.and_then(|n| super::header_footer::parse_titles(&n.formula).1);
        let column = |c: u32| ibis_doc_model::cell_name(c, 0).trim_end_matches('1').to_string();
        let mut parts = Vec::new();
        if let Some((a, b)) = rows {
            parts.push(format!("{sheet}!${}:${}", a + 1, b + 1));
        }
        if let Some((a, b)) = columns {
            parts.push(format!("{sheet}!${}:${}", column(a), column(b)));
        }
        let formula = (!parts.is_empty()).then(|| parts.join(","));
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: "_xlnm.Print_Titles".into(), sheet: Some(index), formula })]) {
            log::warn!("print titles rejected: {err}");
        }
        cx.notify();
    }

    /// The sheet's print titles name: its columns (from 0), if any.
    pub fn print_title_columns(&self) -> Option<(u32, u32)> {
        let wb = self.presentation().workbook.as_ref()?;
        let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide))?;
        let name = wb.names.iter().find(|n| n.sheet == Some(index) && n.name.eq_ignore_ascii_case("_xlnm.Print_Titles"))?;
        super::header_footer::parse_titles(&name.formula).1
    }

    /// 印刷タイトル: the selected columns repeated on the left of each page
    /// (`None`: none), the title rows kept.
    pub fn set_print_title_columns(&mut self, columns: Option<(u32, u32)>, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(wb) = self.presentation().workbook.clone() else { return };
        let Some(index) = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide)) else { return };
        let sheet = ibis_calc::formula::quote_sheet(&wb.sheets[index].name);
        let old = wb.names.iter().find(|n| n.sheet == Some(index) && n.name.eq_ignore_ascii_case("_xlnm.Print_Titles"));
        let rows = old.and_then(|n| super::header_footer::parse_titles(&n.formula).0);
        let column = |c: u32| ibis_doc_model::cell_name(c, 0).trim_end_matches('1').to_string();
        let mut parts = Vec::new();
        if let Some((a, b)) = rows {
            parts.push(format!("{sheet}!${}:${}", a + 1, b + 1));
        }
        if let Some((a, b)) = columns {
            parts.push(format!("{sheet}!${}:${}", column(a), column(b)));
        }
        let formula = (!parts.is_empty()).then(|| parts.join(","));
        if let Err(err) = self.document.execute(vec![Command::Sheet(SheetCommand::SetName { name: "_xlnm.Print_Titles".into(), sheet: Some(index), formula })]) {
            log::warn!("print titles rejected: {err}");
        }
        cx.notify();
    }

    pub fn open_page_setup(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.names = None;
            s.validation = None;
            s.find = None;
            s.link = None;
            s.page_setup = true;
            s.header_footer_edit = None;
        }
        if let Some(s) = self.sheet_state_mut() {
            s.custom_format = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// ファイル > 印刷… (⌘P) in 表計算画面: the 印刷 dialog (a modal panel, as 文書画面's
    /// 印刷の設定 is): the print target (作業中のシート / ブック全体 / 選択した部分), the
    /// orientation, the paper and the scale, with 印刷 / キャンセル at the foot.
    /// 印刷 then makes the PDF (`print_now`), as 文書画面 does.
    pub fn open_xlsx_print_panel(&mut self, cx: &mut Context<Self>) {
        self.open_page_setup(cx);
        if let Some(s) = self.sheet_state_mut() {
            s.page_setup = false;
            s.print_panel = true;
        }
    }

    pub fn close_xlsx_print_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.print_panel = false;
        }
        cx.notify();
    }

    pub fn xlsx_print_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        if !self.sheet_state()?.print_panel {
            return None;
        }
        let print = self.current_print()?;
        let what = self.print_what();
        let radio = |id: &'static str, label: &'static str, on: bool, f: Box<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>)>| -> AnyElement {
            crate::shell::chrome::radio_button(id, on, false, crate::shell::i18n::text(label), cx.listener(move |this, _, _, cx| f(this, cx))).into_any_element()
        };
        let section = |label: &'static str| div().text_sm().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text(label));
        let papers = PAPERS.iter().map(|&(name, size)| radio(Box::leak(format!("xlsx-print-paper-{name}").into_boxed_str()), name, print.paper == size, Box::new(move |this, cx| this.set_print(|p| p.paper = size, None, cx))));
        let body = v_flex()
            .gap_3()
            .child(section("xlsx.print-what"))
            .child(v_flex().gap_1()
                .child(radio("xlsx-print-what-sheet", "xlsx.print-what.active-sheet", what == PrintWhat::ActiveSheet, Box::new(|this, cx| this.set_print_what(PrintWhat::ActiveSheet, cx))))
                .child(radio("xlsx-print-what-book", "xlsx.print-what.entire-workbook", what == PrintWhat::EntireWorkbook, Box::new(|this, cx| this.set_print_what(PrintWhat::EntireWorkbook, cx))))
                .child(radio("xlsx-print-what-selection", "xlsx.print-what.selection", what == PrintWhat::Selection, Box::new(|this, cx| this.set_print_what(PrintWhat::Selection, cx)))))
            .child(section("_xlsx.orientation"))
            .child(h_flex().gap_1()
                .child(radio("xlsx-print-portrait", "_xlsx.portrait", !print.landscape, Box::new(|this, cx| this.set_print(|p| p.landscape = false, None, cx))))
                .child(radio("xlsx-print-landscape", "_xlsx.landscape", print.landscape, Box::new(|this, cx| this.set_print(|p| p.landscape = true, None, cx)))))
            .child(section("_shell.paper-size"))
            .child(h_flex().flex_wrap().gap_1().children(papers))
            .child(section("_xlsx.scale"))
            .child(h_flex().flex_wrap().gap_1()
                .child(radio("xlsx-print-scale-100", "xlsx.print-panel.scale-100", print.fit_to.is_none() && print.scale == 100, Box::new(|this, cx| this.set_print_scale_percent(100, cx))))
                .child(radio("xlsx-print-fit-one", "_xlsx.fit-to-1-page", print.fit_to == Some((1, 1)), Box::new(|this, cx| this.set_print(|p| p.fit_to = Some((1, 1)), None, cx))))
                .child(radio("xlsx-print-fit-width", "xlsx.scale-width-1", print.fit_to.map(|(w, _)| w) == Some(1) && print.fit_to != Some((1, 1)), Box::new(|this, cx| this.set_print_fit_width(1, cx)))))
            .child(crate::shell::chrome::action_button("xlsx-print-page-setup").label(crate::shell::i18n::text("xlsx.page-setup")).on_click(cx.listener(|this, _, _, cx| {
                this.close_xlsx_print_panel(cx);
                this.open_page_setup(cx);
            })));
        // Printing makes a PDF; where there is no print dialog of the system to give it to, the PDF app prints it.
        #[cfg(not(target_os = "macos"))]
        let body = body.child(div().text_sm().text_color(rgb(0x5f6368)).child(crate::shell::i18n::text("xlsx.print-panel.note")));
        let footer = crate::shell::chrome::action_bar()
            .action(crate::shell::chrome::ActionDefinition::new("xlsx-print-apply", crate::shell::i18n::text("docx.print-panel.print"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(|this, _, window, cx| {
                this.close_xlsx_print_panel(cx);
                this.print_now(window, cx);
            })))
            .action(crate::shell::chrome::ActionDefinition::new("xlsx-print-cancel", crate::shell::i18n::text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(|this, _, _, cx| this.close_xlsx_print_panel(cx))));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("xlsx.print-panel.title"), body.into_any_element(), footer))
    }

    /// ページ設定 dialog. Its choices are segment controls (one chosen of a row of items) and its switches are checkboxes;
    /// a command (set / clear / insert) is a button. The rows are a [`label_form`]: the labels are measured, and every
    /// control starts at the same x. The custom header / footer opens its editor in this dialog's place, with a button
    /// back to this dialog (`header_footer_edit_panel`).
    pub fn page_setup_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        use crate::shell::chrome::segment_control::{Mode, SegmentItem};
        use crate::shell::chrome::{SegmentControlRow, action_button, checkbox_button, destructive_button, label_form};
        use crate::shell::i18n::text;
        let state = self.sheet_state()?;
        if !state.page_setup || state.header_footer_edit.is_some() {
            return None;
        }
        let print = self.current_print()?;
        let area = self.current_sheet().and_then(|(_, s)| s.print_area);
        type Change = Box<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>)>;
        let seg = |id: SharedString, label: SharedString, on: bool, f: Change| SegmentItem::label(id, label, on).on_click(cx.listener(move |e, _, _, cx| f(e, cx)));
        let segments = |items: Vec<SegmentItem>| SegmentControlRow::new(Mode::Intrinsic).children(items);
        let check = |id: &'static str, label: &'static str, on: bool, f: Change| checkbox_button(id, text(label), on, false, cx.listener(move |e, _, _, cx| f(e, cx)));
        let command = |id: &'static str, label: &'static str, f: Change| action_button(id).label(text(label)).on_click(cx.listener(move |e, _, _, cx| f(e, cx)));
        let clear = |id: &'static str, f: Change| destructive_button(id, cx).label(text("_xlsx.clear")).on_click(cx.listener(move |e, _, _, cx| f(e, cx)));
        let note = |s: SharedString| div().text_sm().text_color(rgb(0x5f6368)).child(s);

        let orientation = segments(vec![
            seg("portrait".into(), text("_xlsx.portrait"), !print.landscape, Box::new(|e, cx| e.set_print(|p| p.landscape = false, None, cx))),
            seg("landscape".into(), text("_xlsx.landscape"), print.landscape, Box::new(|e, cx| e.set_print(|p| p.landscape = true, None, cx))),
        ]);
        let papers = segments(PAPERS.iter().map(|&(name, size)| seg(format!("paper-{name}").into(), text(name), print.paper == size, Box::new(move |e, cx| e.set_print(|p| p.paper = size, None, cx)))).collect());
        let margins = segments(
            MARGINS
                .iter()
                .enumerate()
                .map(|(i, &(name, m))| {
                    let emu = margins_emu(m);
                    seg(format!("page-margins-{i}").into(), text(name), print.margins == emu, Box::new(move |e, cx| e.set_print(|p| p.margins = emu, None, cx)))
                })
                .collect(),
        );
        let centre = h_flex().gap_4().child(check("center-horizontal", "fix.print.horizontal", print.centered.0, Box::new(|e, cx| e.set_print(|p| p.centered.0 = !p.centered.0, None, cx)))).child(check("center-vertical", "fix.print.vertical", print.centered.1, Box::new(|e, cx| e.set_print(|p| p.centered.1 = !p.centered.1, None, cx))));
        let scale_pct = |pct: u32| seg(format!("scale-{pct}").into(), format!("{pct}%").into(), print.fit_to.is_none() && print.scale == pct, Box::new(move |e, cx| e.set_print_scale_percent(pct, cx)));
        let scale = segments(vec![
            scale_pct(100),
            scale_pct(80),
            scale_pct(50),
            seg("fit-one".into(), text("_xlsx.fit-to-1-page"), print.fit_to == Some((1, 1)), Box::new(|e, cx| e.set_print(|p| p.fit_to = Some((1, 1)), None, cx))),
            seg("fit-width-1".into(), text("xlsx.scale-width-1"), print.fit_to.map(|(w, _)| w) == Some(1), Box::new(|e, cx| e.set_print_fit_width(1, cx))),
            seg("fit-height-1".into(), text("xlsx.scale-height-1"), print.fit_to.map(|(_, h)| h) == Some(1), Box::new(|e, cx| e.set_print_fit_height(1, cx))),
        ]);
        let what = self.print_what();
        let print_what = segments(vec![
            seg("print-active-sheet".into(), text("xlsx.print-what.active-sheet"), what == PrintWhat::ActiveSheet, Box::new(|e, cx| e.set_print_what(PrintWhat::ActiveSheet, cx))),
            seg("print-entire-workbook".into(), text("xlsx.print-what.entire-workbook"), what == PrintWhat::EntireWorkbook, Box::new(|e, cx| e.set_print_what(PrintWhat::EntireWorkbook, cx))),
            seg("print-selection".into(), text("xlsx.print-what.selection"), what == PrintWhat::Selection, Box::new(|e, cx| e.set_print_what(PrintWhat::Selection, cx))),
        ]);
        let area_buttons = h_flex().gap_2().child(command("area-set", "_xlsx.use-selection", Box::new(|e, cx| {
            let range = e.selected_range();
            e.set_print(|_| {}, Some(range), cx);
        }))).child(clear("area-clear", Box::new(|e, cx| e.set_print(|_| {}, Some(None), cx))));
        let area_note = note(match area {
            Some(r) => crate::shell::i18n::format("_xlsx.print-area-range", &[("range", &format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row)))]).into(),
            None => text("_xlsx.print-area-none-used-range"),
        });
        let break_button = h_flex().child(command("row-break", "_xlsx.insert-delete-above-selected-row", Box::new(|e, cx| e.toggle_row_break(cx))));

        // The header and the footer: the presets (one chosen), the custom editor on the next line (indented), the sample (centred).
        let hf_block = |prefix: &'static str, current: Option<String>, header: bool, cx: &Context<Self>| {
            let presets = segments(
                HEADER_PRESETS
                    .iter()
                    .map(|&(label, code)| {
                        let code = code.map(|c| match c.strip_prefix('@') { Some(id) => text(id).to_string(), None => c.to_string() });
                        let on = current.as_deref() == code.as_deref();
                        seg(format!("{prefix}-{label}").into(), text(label), on, Box::new(move |e, cx| e.set_print(|p| if header { p.header_footer.odd_header = code.clone() } else { p.header_footer.odd_footer = code.clone() }, None, cx)))
                    })
                    .collect(),
            );
            let target = if header { super::header_footer_edit::HfTarget::Header } else { super::header_footer_edit::HfTarget::Footer };
            let custom = action_button(if header { "header-custom" } else { "footer-custom" }).label(text(if header { "xlsx.header.custom" } else { "xlsx.footer.custom" })).on_click(cx.listener(move |this, _, window, cx| this.open_header_footer_edit_from_dialog(target, window, cx)));
            let sample = match &current {
                Some(code) => crate::shell::i18n::format(if header { "_xlsx.header-code" } else { "_xlsx.footer-code" }, &[("code", code)]).into(),
                None => text("_xlsx.none"),
            };
            (presets, h_flex().pl(px(INDENT)).child(custom), div().w_full().flex().justify_center().text_sm().text_color(rgb(0x5f6368)).child(sample))
        };
        let (header_presets, header_custom, header_sample) = hf_block("header", print.header_footer.odd_header.clone(), true, cx);
        let (footer_presets, footer_custom, footer_sample) = hf_block("footer", print.header_footer.odd_footer.clone(), false, cx);
        let hf = &print.header_footer;
        let toggle = |id: &'static str, label: &'static str, on: bool, f: fn(&mut ibis_doc_model::SheetHeaderFooter)| check(id, label, on, Box::new(move |e, cx| e.set_print(|p| f(&mut p.header_footer), None, cx)));
        let hf_options = v_flex()
            .gap_1()
            .child(div().text_size(px(crate::shell::chrome::label_form::LABEL_SIZE)).child(text("_xlsx.header-options")))
            .child(
                v_flex()
                    .gap_1()
                    .pl(px(INDENT))
                    .child(toggle("hf-diff-first", "xlsx.hf.diff-first", hf.different_first, |h| h.different_first = !h.different_first))
                    .child(toggle("hf-diff-odd-even", "xlsx.hf.diff-odd-even", hf.different_odd_even, |h| h.different_odd_even = !h.different_odd_even))
                    .child(toggle("hf-scale-doc", "xlsx.hf.scale-with-doc", hf.scale_with_doc, |h| h.scale_with_doc = !h.scale_with_doc))
                    .child(toggle("hf-align-margins", "xlsx.hf.align-margins", hf.align_with_margins, |h| h.align_with_margins = !h.align_with_margins)),
            );

        let rows_buttons = h_flex().gap_2().child(command("titles-set", "_xlsx.use-selected-rows", Box::new(|e, cx| {
            let rows = e.selected_range().map(|r| (r.first_row, r.last_row)).or_else(|| e.current_cell().map(|(_, r)| (r, r)));
            e.set_print_title_rows(rows, cx);
        }))).child(clear("titles-clear", Box::new(|e, cx| e.set_print_title_rows(None, cx))));
        let rows_note = note(match self.print_title_rows() {
            Some((a, b)) => crate::shell::i18n::format("_xlsx.rows-to-repeat-at-top-rows-printed-on-every-page", &[("rows", &format!("{}:{}", a + 1, b + 1))]).into(),
            None => text("_xlsx.rows-to-repeat-at-top-none"),
        });
        let cols_buttons = h_flex().gap_2().child(command("title-cols-set", "fix.print.use-selected-columns", Box::new(|e, cx| {
            let cols = e.selected_range().map(|r| (r.first_column, r.last_column)).or_else(|| e.current_cell().map(|(c, _)| (c, c)));
            e.set_print_title_columns(cols, cx);
        }))).child(clear("title-cols-clear", Box::new(|e, cx| e.set_print_title_columns(None, cx))));
        let cols_note = note(match self.print_title_columns() {
            Some((a, b)) => {
                let col_name = |c: u32| ibis_doc_model::cell_name(c, 0).trim_end_matches('1').to_string();
                crate::shell::i18n::format("fix.print.columns-repeat-left-cols-printed", &[("cols", &format!("{}:{}", col_name(a), col_name(b)))]).into()
            }
            None => text("fix.print.columns-repeat-left-none"),
        });
        let print_options = h_flex()
            .flex_wrap()
            .gap_x_4()
            .gap_y_1()
            .child(check("print-grid-lines", "xlsx.print.gridlines", print.grid_lines, Box::new(|e, cx| e.set_print(|p| p.grid_lines = !p.grid_lines, None, cx))))
            .child(check("print-headings", "xlsx.print.headings", print.headings, Box::new(|e, cx| e.set_print(|p| p.headings = !p.headings, None, cx))))
            .child(check("print-black-white", "xlsx.print.black-white", print.black_and_white, Box::new(|e, cx| e.set_print(|p| p.black_and_white = !p.black_and_white, None, cx))))
            .child(check("print-draft", "xlsx.print.draft", print.draft, Box::new(|e, cx| e.set_print(|p| p.draft = !p.draft, None, cx))));

        let body = label_form("page-setup")
            .row(text("_xlsx.orientation"), orientation)
            .row(text("_shell.paper-size"), papers)
            .row(text("_shell.margins"), margins)
            .row(text("fix.print.center-page"), centre)
            .row(text("_xlsx.scale"), scale)
            .row(text("xlsx.print-what"), print_what)
            .row(text("_xlsx.print-area"), area_buttons)
            .note(area_note)
            .row(text("_xlsx.page-break"), break_button)
            .row(text("_shell.header"), header_presets)
            .full(header_custom)
            .full(header_sample)
            .row(text("_shell.footer"), footer_presets)
            .full(footer_custom)
            .full(footer_sample)
            .full(hf_options)
            .row(text("_xlsx.rows-to-repeat-at-top"), rows_buttons)
            .note(rows_note)
            .row(text("fix.print.columns-repeat-left"), cols_buttons)
            .note(cols_note)
            .row(text("xlsx.print-options"), print_options);
        Some(SidebarPanel { title: text("_shell.page-setup").into(), body: body.into_any_element() })
    }
}

/// The indent of a line that belongs to the one above (one full-width character at the 14 px text size).
const INDENT: f32 = 14.;

/// Margins in inches (left, right, top, bottom) as EMU.
pub(crate) fn margins_emu(m: [f64; 4]) -> [i64; 4] {
    m.map(|v| (v * ibis_doc_model::EMU_PER_INCH as f64).round() as i64)
}
