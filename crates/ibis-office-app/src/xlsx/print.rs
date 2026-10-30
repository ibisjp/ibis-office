// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ページ設定 in the sidebar (ファイル > ページ設定): the orientation, the
//! paper, the margins, the scale (or one page), the print area, manual
//! page breaks, the header and footer (Excel's presets) and the title rows
//! printed on every page (印刷タイトル) of the sheet shown.

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{CellRange, Command, SheetCommand, SheetPrint};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;

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

/// Excel's margin presets (inches): 標準, 狭い, 広い (left, right, top, bottom).
pub(crate) const MARGINS: &[(&str, [f64; 4])] = &[("_xlsx.normal", [0.7, 0.7, 0.75, 0.75]), ("_xlsx.narrow", [0.25, 0.25, 0.75, 0.75]), ("_xlsx.wide", [1.0, 1.0, 1.0, 1.0])];

/// 印刷およびPDF書き出しの対象 (Print What: XLSX-SEC-019)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum PrintWhat {
    /// 作業中のシート（現在のシートのみ）
    #[default]
    ActiveSheet,
    /// ブック全体（すべてのシート）
    EntireWorkbook,
    /// 選択した部分（現在選択されているセル範囲のみ）
    Selection,
}

impl SlideEditor {
    /// 現在の印刷対象（作業中のシート / ブック全体 / 選択した部分）
    pub(crate) fn print_what(&self) -> PrintWhat {
        self.wb().print_what
    }

    /// 印刷対象を切り替える
    pub(crate) fn set_print_what(&mut self, what: PrintWhat, cx: &mut Context<Self>) {
        self.wb_mut().print_what = what;
        cx.notify();
    }

    pub(crate) fn current_print(&self) -> Option<SheetPrint> {
        self.current_sheet().map(|(_, s)| s.print.clone())
    }

    /// Change the print settings (and the print area, if `area` is `Some`).
    pub(crate) fn set_print(&mut self, change: impl FnOnce(&mut SheetPrint), area: Option<Option<CellRange>>, cx: &mut Context<Self>) {
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
    pub(crate) fn print_title_rows(&self) -> Option<(u32, u32)> {
        let wb = self.presentation().workbook.as_ref()?;
        let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide))?;
        let name = wb.names.iter().find(|n| n.sheet == Some(index) && n.name.eq_ignore_ascii_case("_xlnm.Print_Titles"))?;
        super::header_footer::parse_titles(&name.formula).0
    }

    /// 印刷タイトル: the selected rows repeated at the top of each page
    /// (`None`: none), the title columns kept.
    pub(crate) fn set_print_title_rows(&mut self, rows: Option<(u32, u32)>, cx: &mut Context<Self>) {
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
    pub(crate) fn print_title_columns(&self) -> Option<(u32, u32)> {
        let wb = self.presentation().workbook.as_ref()?;
        let index = wb.sheets.iter().position(|s| s.slide == Some(self.current_slide))?;
        let name = wb.names.iter().find(|n| n.sheet == Some(index) && n.name.eq_ignore_ascii_case("_xlnm.Print_Titles"))?;
        super::header_footer::parse_titles(&name.formula).1
    }

    /// 印刷タイトル: the selected columns repeated on the left of each page
    /// (`None`: none), the title rows kept.
    pub(crate) fn set_print_title_columns(&mut self, columns: Option<(u32, u32)>, cx: &mut Context<Self>) {
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

    pub(crate) fn open_page_setup(&mut self, cx: &mut Context<Self>) {
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

    pub(crate) fn page_setup_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        if !self.sheet_state()?.page_setup {
            return None;
        }
        let print = self.current_print()?;
        let area = self.current_sheet().and_then(|(_, s)| s.print_area);
        let choice = |id: SharedString, label: &'static str, on: bool, f: Box<dyn Fn(&mut SlideEditor, &mut Context<SlideEditor>)>| -> AnyElement {
            let command = id.starts_with("area-") || id.starts_with("titles-") || id.starts_with("columns-") || id=="row-break" || label=="_xlsx.clear";
            let independent = id.starts_with("center-") || id.starts_with("hf-") || id.starts_with("print-grid") || id.starts_with("print-head") || id=="print-black-white" || id=="print-draft";
            let click=cx.listener(move |this,_,_,cx|f(this,cx));
            if command {
                let b=if label=="_xlsx.clear" {crate::shell::chrome::destructive_button(id,cx)}else{crate::shell::chrome::action_button(id)};
                b.label(crate::shell::i18n::text(label)).on_click(click).into_any_element()
            } else if independent {
                crate::shell::chrome::checkbox_button(id,crate::shell::i18n::text(label),on,false,click).into_any_element()
            } else {
                crate::shell::chrome::radio_button(id,on,false,crate::shell::i18n::text(label),click).into_any_element()
            }
        };
        let row = |label: &'static str| h_flex().flex_wrap().gap_1().items_center().child(div().w(px(72.)).text_sm().child(crate::shell::i18n::text(label)));
        let papers = PAPERS.iter().map(|&(name, size)| choice(format!("paper-{name}").into(), name, print.paper == size, Box::new(move |this, cx| this.set_print(|p| p.paper = size, None, cx))));
        let margins = MARGINS.iter().enumerate().map(|(i,&(name,m))| {
            let emu=m.map(|v|(v*ibis_doc_model::EMU_PER_INCH as f64).round() as i64);
            crate::shell::chrome::segment_control::SegmentItem::label(("page-margins",i),crate::shell::i18n::text(name),print.margins==emu)
                .on_click(cx.listener(move |e,_,_,cx|e.set_print(|p|p.margins=emu,None,cx)))
        });
        let body = v_flex()
            .gap_2()
            .px(px(18.))
            .py(px(8.))
            .p_0().child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic).label(crate::shell::i18n::text("_xlsx.orientation"))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("portrait",crate::shell::i18n::text("_xlsx.portrait"),!print.landscape).on_click(cx.listener(|e,_,_,cx|e.set_print(|p|p.landscape=false,None,cx))))
                .child(crate::shell::chrome::segment_control::SegmentItem::label("landscape",crate::shell::i18n::text("_xlsx.landscape"),print.landscape).on_click(cx.listener(|e,_,_,cx|e.set_print(|p|p.landscape=true,None,cx)))))
            .child(row("_shell.paper-size").children(papers))
            .child(crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic).label(crate::shell::i18n::text("_shell.margins")).children(margins))
            .child(
                row("fix.print.center-page")
                    .child(choice("center-horizontal".into(), "fix.print.horizontal", print.centered.0, Box::new(|this, cx| this.set_print(|p| p.centered.0 = !p.centered.0, None, cx))))
                    .child(choice("center-vertical".into(), "fix.print.vertical", print.centered.1, Box::new(|this, cx| this.set_print(|p| p.centered.1 = !p.centered.1, None, cx)))),
            )
            .child(
                row("_xlsx.scale")
                    .child(choice("scale-100".into(), "100%", print.fit_to.is_none() && print.scale == 100, Box::new(|this, cx| this.set_print_scale_percent(100, cx))))
                    .child(choice("scale-80".into(), "80%", print.fit_to.is_none() && print.scale == 80, Box::new(|this, cx| this.set_print_scale_percent(80, cx))))
                    .child(choice("scale-50".into(), "50%", print.fit_to.is_none() && print.scale == 50, Box::new(|this, cx| this.set_print_scale_percent(50, cx))))
                    .child(choice("fit-one".into(), "_xlsx.fit-to-1-page", print.fit_to == Some((1, 1)), Box::new(|this, cx| this.set_print(|p| p.fit_to = Some((1, 1)), None, cx))))
                    .child(choice("fit-width-1".into(), "xlsx.scale-width-1", print.fit_to.map(|(w, _)| w) == Some(1), Box::new(|this, cx| this.set_print_fit_width(1, cx))))
                    .child(choice("fit-height-1".into(), "xlsx.scale-height-1", print.fit_to.map(|(_, h)| h) == Some(1), Box::new(|this, cx| this.set_print_fit_height(1, cx)))),
            )
            .child(
                row("xlsx.print-what")
                    .child(choice("print-active-sheet".into(), "xlsx.print-what.active-sheet", self.print_what() == PrintWhat::ActiveSheet, Box::new(|this, cx| this.set_print_what(PrintWhat::ActiveSheet, cx))))
                    .child(choice("print-entire-workbook".into(), "xlsx.print-what.entire-workbook", self.print_what() == PrintWhat::EntireWorkbook, Box::new(|this, cx| this.set_print_what(PrintWhat::EntireWorkbook, cx))))
                    .child(choice("print-selection".into(), "xlsx.print-what.selection", self.print_what() == PrintWhat::Selection, Box::new(|this, cx| this.set_print_what(PrintWhat::Selection, cx)))),
            )
            .child(
                row("_xlsx.print-area")
                    .child(choice("area-set".into(), "_xlsx.use-selection", false, Box::new(|this, cx| {
                        let range = this.selected_range();
                        this.set_print(|_| {}, Some(range), cx);
                    })))
                    .child(choice("area-clear".into(), "_xlsx.clear", false, Box::new(|this, cx| this.set_print(|_| {}, Some(None), cx)))),
            )
            .child(div().text_sm().child(SharedString::from(match area {
                Some(r) => crate::shell::i18n::format("_xlsx.print-area-range", &[("range", &format!("{}:{}", ibis_doc_model::cell_name(r.first_column, r.first_row), ibis_doc_model::cell_name(r.last_column, r.last_row)))]),
                None => crate::shell::i18n::text("_xlsx.print-area-none-used-range").into(),
            })))
            .child(row("_xlsx.page-break").child(choice("row-break".into(), "_xlsx.insert-delete-above-selected-row", false, Box::new(|this, cx| this.toggle_row_break(cx)))))
            .child(
                row("_shell.header")
                    .children(HEADER_PRESETS.iter().map(|&(label, code)| {
                        let code = code.map(|c| match c.strip_prefix('@') { Some(id) => crate::shell::i18n::text(id).to_string(), None => c.to_string() });
                        let on = print.header_footer.odd_header.as_deref() == code.as_deref();
                        choice(format!("header-{label}").into(), label, on, Box::new(move |this, cx| this.set_print(|p| p.header_footer.odd_header = code.clone(), None, cx)))
                    }))
                    .child(crate::shell::chrome::action_button("header-custom").small().label(crate::shell::i18n::text("xlsx.header.custom")).on_click(cx.listener(|this, _, window, cx| {
                        this.open_header_footer_edit(super::header_footer_edit::HfTarget::Header, window, cx);
                    }))),
            )
            .child(div().text_sm().text_color(rgb(0x5f6368)).child(SharedString::from(match &print.header_footer.odd_header {
                Some(h) => crate::shell::i18n::format("_xlsx.header-code", &[("code", h)]),
                None => crate::shell::i18n::text("_xlsx.none").into(),
            })))
            .child(
                row("_shell.footer")
                    .children(HEADER_PRESETS.iter().map(|&(label, code)| {
                        let code = code.map(|c| match c.strip_prefix('@') { Some(id) => crate::shell::i18n::text(id).to_string(), None => c.to_string() });
                        let on = print.header_footer.odd_footer.as_deref() == code.as_deref();
                        choice(format!("footer-{label}").into(), label, on, Box::new(move |this, cx| this.set_print(|p| p.header_footer.odd_footer = code.clone(), None, cx)))
                    }))
                    .child(crate::shell::chrome::action_button("footer-custom").small().label(crate::shell::i18n::text("xlsx.footer.custom")).on_click(cx.listener(|this, _, window, cx| {
                        this.open_header_footer_edit(super::header_footer_edit::HfTarget::Footer, window, cx);
                    }))),
            )
            .child(div().text_sm().text_color(rgb(0x5f6368)).child(SharedString::from(match &print.header_footer.odd_footer {
                Some(f) => crate::shell::i18n::format("_xlsx.footer-code", &[("code", f)]),
                None => crate::shell::i18n::text("_xlsx.none").into(),
            })))
            .child(
                row("_xlsx.header-options")
                    .child(choice("hf-diff-first".into(), "xlsx.hf.diff-first", print.header_footer.different_first, Box::new(|this, cx| this.set_print(|p| p.header_footer.different_first = !p.header_footer.different_first, None, cx))))
                    .child(choice("hf-diff-odd-even".into(), "xlsx.hf.diff-odd-even", print.header_footer.different_odd_even, Box::new(|this, cx| this.set_print(|p| p.header_footer.different_odd_even = !p.header_footer.different_odd_even, None, cx))))
                    .child(choice("hf-scale-doc".into(), "xlsx.hf.scale-with-doc", print.header_footer.scale_with_doc, Box::new(|this, cx| this.set_print(|p| p.header_footer.scale_with_doc = !p.header_footer.scale_with_doc, None, cx))))
                    .child(choice("hf-align-margins".into(), "xlsx.hf.align-margins", print.header_footer.align_with_margins, Box::new(|this, cx| this.set_print(|p| p.header_footer.align_with_margins = !p.header_footer.align_with_margins, None, cx)))),
            )
            .child(
                row("_xlsx.rows-to-repeat-at-top")
                    .child(choice("titles-set".into(), "_xlsx.use-selected-rows", false, Box::new(|this, cx| {
                        let rows = this.selected_range().map(|r| (r.first_row, r.last_row)).or_else(|| this.current_cell().map(|(_, r)| (r, r)));
                        this.set_print_title_rows(rows, cx);
                    })))
                    .child(choice("titles-clear".into(), "_xlsx.clear", false, Box::new(|this, cx| this.set_print_title_rows(None, cx)))),
            )
            .child(div().text_sm().child(SharedString::from(match self.print_title_rows() {
                Some((a, b)) => crate::shell::i18n::format("_xlsx.rows-to-repeat-at-top-rows-printed-on-every-page", &[("rows", &format!("{}:{}", a + 1, b + 1))]),
                None => crate::shell::i18n::text("_xlsx.rows-to-repeat-at-top-none").into(),
            })))
            .child(
                row("fix.print.columns-repeat-left")
                    .child(choice("title-cols-set".into(), "fix.print.use-selected-columns", false, Box::new(|this, cx| {
                        let cols = this.selected_range().map(|r| (r.first_column, r.last_column)).or_else(|| this.current_cell().map(|(c, _)| (c, c)));
                        this.set_print_title_columns(cols, cx);
                    })))
                    .child(choice("title-cols-clear".into(), "_xlsx.clear", false, Box::new(|this, cx| this.set_print_title_columns(None, cx)))),
            )
            .child(div().text_sm().child(SharedString::from(match self.print_title_columns() {
                Some((a, b)) => {
                    let col_name = |c: u32| ibis_doc_model::cell_name(c, 0).trim_end_matches('1').to_string();
                    crate::shell::i18n::format("fix.print.columns-repeat-left-cols-printed", &[("cols", &format!("{}:{}", col_name(a), col_name(b)))])
                }
                None => crate::shell::i18n::text("fix.print.columns-repeat-left-none").into(),
            })))
            .child(
                row("xlsx.print-options")
                    .child(choice("print-grid-lines".into(), "xlsx.print.gridlines", print.grid_lines, Box::new(|this, cx| this.set_print(|p| p.grid_lines = !p.grid_lines, None, cx))))
                    .child(choice("print-headings".into(), "xlsx.print.headings", print.headings, Box::new(|this, cx| this.set_print(|p| p.headings = !p.headings, None, cx))))
                    .child(choice("print-black-white".into(), "xlsx.print.black-white", print.black_and_white, Box::new(|this, cx| this.set_print(|p| p.black_and_white = !p.black_and_white, None, cx))))
                    .child(choice("print-draft".into(), "xlsx.print.draft", print.draft, Box::new(|this, cx| this.set_print(|p| p.draft = !p.draft, None, cx)))),
            );
        Some(SidebarPanel { title: crate::shell::i18n::text("_shell.page-setup").into(), body: body.into_any_element() })
    }
}
