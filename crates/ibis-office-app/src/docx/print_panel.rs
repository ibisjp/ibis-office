// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 印刷の設定 (Print Settings: DOCX-SEC-013, DOCX-SEC-015, DOCX-SEC-016):
//! - DOCX-SEC-013: ページ・セクション範囲印刷 (すべてのページ、現在のページ、選択した部分、指定範囲: 1-3, s2, p1s2-p3s2, 奇数/偶数ページ)
//! - DOCX-SEC-015: 両面・部単位印刷 (片面/両面長辺/両面短辺/手動、部単位 1,2,3 vs ページごと 1,1,2,2、部数)
//! - DOCX-SEC-016: 1枚に複数ページ (1, 2, 4, 6, 8, 16 ページ/枚、ページの境界線、用紙サイズ拡大縮小)

use std::sync::Arc;

use gpui_kit::component::button::{ButtonVariants as _};
use crate::shell::chrome::Checkbox;
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::flow::FlowDocument;

use crate::docx::flow_layout;
use crate::render::shape_view::Resources;
use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::{format as i18n_format, text};

/// 印刷範囲の種別 (DOCX-SEC-013)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DocxPrintRange {
    #[default]
    All,
    Current,
    Selection,
    Custom,
}

/// ページの絞り込み (奇数・偶数ページ DOCX-SEC-013)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DocxPageSubset {
    #[default]
    All,
    OddOnly,
    EvenOnly,
}

/// 両面印刷の設定 (DOCX-SEC-015)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DocxDuplex {
    #[default]
    OneSided,
    BothSidesLongEdge,
    BothSidesShortEdge,
    Manual,
}

/// Word 印刷設定 (DOCX-SEC-013, 015, 016)
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DocxPrintSettings {
    pub(crate) range: DocxPrintRange,
    /// ユーザー指定の範囲文字列 (例: "1-3, 5, s2, p1s2-p3s2")
    pub(crate) custom_range: String,
    pub(crate) subset: DocxPageSubset,
    /// 印刷部数 (1..=99)
    pub(crate) copies: u32,
    /// 部単位で印刷 (true: 1,2,3  1,2,3 / false: 1,1,1  2,2,2)
    pub(crate) collate: bool,
    pub(crate) duplex: DocxDuplex,
    /// 1枚に複数ページ (N-up: 1, 2, 4, 6, 8, 16. None は 1 ページ/枚)
    pub(crate) pages_per_sheet: Option<u8>,
    /// ページの境界線（枠線）を付ける
    pub(crate) page_border: bool,
    /// 用紙サイズに拡大縮小 (None: なし, Some("A4"), Some("Letter"), Some("B5"))
    pub(crate) scale_paper: Option<String>,
}

impl Default for DocxPrintSettings {
    fn default() -> Self {
        Self {
            range: DocxPrintRange::All,
            custom_range: String::new(),
            subset: DocxPageSubset::All,
            copies: 1,
            collate: true,
            duplex: DocxDuplex::OneSided,
            pages_per_sheet: None,
            page_border: false,
            scale_paper: None,
        }
    }
}

/// 各ページのセクション情報 (DOCX-SEC-013 範囲計算用)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PageInfo {
    /// 1-based 通算ページ番号 (1..=total)
    pub(crate) global_index: usize,
    /// 1-based セクション番号 (1..=total_sections)
    pub(crate) section_index: usize,
    /// 1-based セクション内ページ番号
    pub(crate) page_in_section: u32,
}

/// 文書から全ページのセクション情報を収集する
pub(crate) fn collect_document_pages(doc: &Arc<FlowDocument>, res: &Resources) -> Vec<PageInfo> {
    let pagination = flow_layout::pages(doc, res);
    pagination
        .pages
        .iter()
        .enumerate()
        .map(|(i, pg)| PageInfo {
            global_index: i + 1,
            section_index: pg.section + 1,
            page_in_section: pg.number,
        })
        .collect()
}

/// ページ指定の端点
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PagePoint {
    Global(usize),
    Section(usize),
    PageInSection { page: u32, section: usize },
}

/// 1つの端点文字列をパースする
fn parse_point(token: &str) -> Option<PagePoint> {
    let s = token.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix('s') {
        if let Ok(sec) = rest.parse::<usize>() {
            return Some(PagePoint::Section(sec));
        }
    } else if let Some(rest) = lower.strip_prefix('p') {
        if let Some((page_str, sec_str)) = rest.split_once('s') {
            if let (Ok(p), Ok(sec)) = (page_str.parse::<u32>(), sec_str.parse::<usize>()) {
                return Some(PagePoint::PageInSection { page: p, section: sec });
            }
        } else if let Ok(p) = rest.parse::<usize>() {
            return Some(PagePoint::Global(p));
        }
    } else if let Ok(n) = s.parse::<usize>() {
        return Some(PagePoint::Global(n));
    }
    None
}

/// Word 形式のページ・セクション指定文字列をパースし、印刷対象の通算ページ番号（1-based）一覧を返す
///
/// 対応構文 (DOCX-SEC-013):
/// - 単一ページ: `3`
/// - ページ範囲: `1-5`
/// - カンマ区切り: `1, 3, 5-8`
/// - セクション指定: `s2` (セクション2の全ページ)
/// - セクション範囲: `s1-s3` (セクション1〜3の全ページ)
/// - セクション内ページ: `p1s2` (セクション2の1ページ目)
/// - セクション跨ぎ範囲: `p1s2-p3s2`, `p2s1-p1s2`, `p1s2-s3`, `s1-p2s2`
pub(crate) fn parse_docx_page_range(spec: &str, pages: &[PageInfo]) -> Result<Vec<usize>, String> {
    if pages.is_empty() {
        return Ok(Vec::new());
    }
    let total = pages.len();
    let mut result = Vec::new();

    // カンマまたはセミコロンで分割
    for item in spec.split([',', ';']) {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }

        // ハイフンで範囲分割
        if let Some((start_str, end_str)) = item.split_once('-') {
            let start_point = parse_point(start_str).ok_or_else(|| {
                i18n_format("docx.print-panel.invalid-range-token", &[("token", &start_str)])
            })?;
            let end_point = parse_point(end_str).ok_or_else(|| {
                i18n_format("docx.print-panel.invalid-range-token", &[("token", &end_str)])
            })?;

            // 開始ページの通算インデックス
            let start_global = match start_point {
                PagePoint::Global(g) => g.clamp(1, total),
                PagePoint::Section(s) => pages
                    .iter()
                    .find(|p| p.section_index == s)
                    .map(|p| p.global_index)
                    .ok_or_else(|| {
                        i18n_format("docx.print-panel.section-not-found", &[("section", &s.to_string())])
                    })?,
                PagePoint::PageInSection { page, section } => pages
                    .iter()
                    .find(|p| p.section_index == section && p.page_in_section == page)
                    .map(|p| p.global_index)
                    .ok_or_else(|| {
                        i18n_format(
                            "docx.print-panel.page-in-section-not-found",
                            &[("page", &page.to_string()), ("section", &section.to_string())],
                        )
                    })?,
            };

            // 終了ページの通算インデックス
            let end_global = match end_point {
                PagePoint::Global(g) => g.clamp(1, total),
                PagePoint::Section(s) => pages
                    .iter()
                    .rfind(|p| p.section_index == s)
                    .map(|p| p.global_index)
                    .ok_or_else(|| {
                        i18n_format("docx.print-panel.section-not-found", &[("section", &s.to_string())])
                    })?,
                PagePoint::PageInSection { page, section } => pages
                    .iter()
                    .rfind(|p| p.section_index == section && p.page_in_section == page)
                    .map(|p| p.global_index)
                    .ok_or_else(|| {
                        i18n_format(
                            "docx.print-panel.page-in-section-not-found",
                            &[("page", &page.to_string()), ("section", &section.to_string())],
                        )
                    })?,
            };

            if start_global <= end_global {
                for p in start_global..=end_global {
                    result.push(p);
                }
            } else {
                for p in (end_global..=start_global).rev() {
                    result.push(p);
                }
            }
        } else {
            // 単一トークン
            let pt = parse_point(item).ok_or_else(|| {
                i18n_format("docx.print-panel.invalid-range-token", &[("token", &item)])
            })?;
            match pt {
                PagePoint::Global(g) => {
                    if g >= 1 && g <= total {
                        result.push(g);
                    } else {
                        return Err(i18n_format(
                            "docx.print-panel.page-out-of-range",
                            &[("page", &g.to_string()), ("total", &total.to_string())],
                        ));
                    }
                }
                PagePoint::Section(s) => {
                    let sec_pages: Vec<usize> = pages
                        .iter()
                        .filter(|p| p.section_index == s)
                        .map(|p| p.global_index)
                        .collect();
                    if sec_pages.is_empty() {
                        return Err(i18n_format(
                            "docx.print-panel.section-not-found",
                            &[("section", &s.to_string())],
                        ));
                    }
                    result.extend(sec_pages);
                }
                PagePoint::PageInSection { page, section } => {
                    let found = pages
                        .iter()
                        .find(|p| p.section_index == section && p.page_in_section == page)
                        .map(|p| p.global_index);
                    if let Some(g) = found {
                        result.push(g);
                    } else {
                        return Err(i18n_format(
                            "docx.print-panel.page-in-section-not-found",
                            &[("page", &page.to_string()), ("section", &section.to_string())],
                        ));
                    }
                }
            }
        }
    }

    Ok(result)
}

/// ページの奇数・偶数絞り込みを適用する (DOCX-SEC-013)
pub(crate) fn apply_page_subset(pages: &[usize], subset: DocxPageSubset) -> Vec<usize> {
    match subset {
        DocxPageSubset::All => pages.to_vec(),
        DocxPageSubset::OddOnly => pages.iter().copied().filter(|&p| p % 2 == 1).collect(),
        DocxPageSubset::EvenOnly => pages.iter().copied().filter(|&p| p % 2 == 0).collect(),
    }
}

/// 印刷部数と部単位/ページごとの展開を行う (DOCX-SEC-015)
pub(crate) fn expand_copies_and_collate(pages: &[usize], copies: u32, collate: bool) -> Vec<usize> {
    if pages.is_empty() || copies <= 1 {
        return pages.to_vec();
    }
    let count = copies as usize;
    if collate {
        // 部単位: 1, 2, 3, 1, 2, 3
        let mut out = Vec::with_capacity(pages.len() * count);
        for _ in 0..count {
            out.extend_from_slice(pages);
        }
        out
    } else {
        // ページごと: 1, 1, 2, 2, 3, 3
        let mut out = Vec::with_capacity(pages.len() * count);
        for &p in pages {
            for _ in 0..count {
                out.push(p);
            }
        }
        out
    }
}

/// 印刷設定に基づいて最終的な通算ページ番号一覧を解決する
pub(crate) fn resolve_print_pages(
    doc: &Arc<FlowDocument>,
    res: &Resources,
    settings: &DocxPrintSettings,
    current_page: usize,
    selected_pages: Option<&[usize]>,
) -> Result<Vec<usize>, String> {
    let pages = collect_document_pages(doc, res);
    let total = pages.len();
    if total == 0 {
        return Ok(Vec::new());
    }

    let raw_pages = match settings.range {
        DocxPrintRange::All => (1..=total).collect(),
        DocxPrintRange::Current => {
            let cur = current_page.clamp(1, total);
            vec![cur]
        }
        DocxPrintRange::Selection => {
            if let Some(sel) = selected_pages {
                if !sel.is_empty() {
                    sel.to_vec()
                } else {
                    vec![current_page.clamp(1, total)]
                }
            } else {
                vec![current_page.clamp(1, total)]
            }
        }
        DocxPrintRange::Custom => {
            if settings.custom_range.trim().is_empty() {
                (1..=total).collect()
            } else {
                parse_docx_page_range(&settings.custom_range, &pages)?
            }
        }
    };

    let filtered = apply_page_subset(&raw_pages, settings.subset);
    let final_pages = expand_copies_and_collate(&filtered, settings.copies, settings.collate);
    Ok(final_pages)
}

/// CLI 引数とスライド一覧を生成する (DOCX-SEC-013, 015, 016)
pub(crate) fn build_print_cli_args(
    pages: &[usize],
    settings: &DocxPrintSettings,
    total_pages: usize,
) -> (Option<Vec<usize>>, Option<Vec<String>>) {
    let mut args = Vec::new();
    let is_all_pages_once = settings.copies <= 1 && pages.len() == total_pages && (1..=total_pages).all(|i| pages.get(i - 1) == Some(&i));

    let page_list = pages.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",");
    if !is_all_pages_once {
        args.extend(["--slides".to_string(), page_list]);
    }

    // 1枚に複数ページ (N-up: DOCX-SEC-016)
    if let Some(per_page) = settings.pages_per_sheet {
        if per_page > 1 {
            args.extend(["--handout".to_string(), per_page.to_string()]);
        }
    }

    // ページの境界線 (枠線: DOCX-SEC-016)
    if settings.page_border {
        args.push("--print-frame".to_string());
    }

    let slides_vec = (!is_all_pages_once).then(|| pages.to_vec());
    let args_vec = (!args.is_empty()).then_some(args);
    (slides_vec, args_vec)
}

/// サイドバーパネルの状態
pub(crate) struct DocxPrintPanelState {
    pub(crate) copies_input: Entity<InputState>,
    pub(crate) custom_input: Entity<InputState>,
    pub(crate) error: Option<String>,
}

impl SlideEditor {
    /// 現在の Word 印刷設定を取得する
    pub(crate) fn docx_print_settings(&self) -> DocxPrintSettings {
        self.doc_state().map(|s| s.print_settings.clone()).unwrap_or_default()
    }

    /// 現在のキャレット位置の通算ページ番号 (1-based) を取得する
    pub(crate) fn current_flow_page(&self) -> usize {
        let lines = self.flow_lines();
        if let Some((_, caret)) = self.text_selection() {
            if let Some(line) = Self::line_of(&lines, caret) {
                return line.page + 1;
            }
        }
        1
    }

    /// 現在のテキスト選択範囲がまたがる通算ページ番号一覧 (1-based) を取得する
    pub(crate) fn selected_flow_pages(&self) -> Option<Vec<usize>> {
        let (anchor, caret) = self.ordered_selection()?;
        if anchor == caret {
            return None;
        }
        let lines = self.flow_lines();
        let mut pages = Vec::new();
        for line in &lines {
            // 行の範囲が選択範囲と重なるか判定
            if line.para >= anchor.para && line.para <= caret.para {
                let pg = line.page + 1;
                if !pages.contains(&pg) {
                    pages.push(pg);
                }
            }
        }
        if pages.is_empty() { None } else { Some(pages) }
    }

    /// 印刷設定パネルを開く (ファイル > 印刷… ⌘P / 印刷の設定…)
    pub(crate) fn open_doc_print_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        self.end_text_edit(window, cx);
        let settings = self.docx_print_settings();
        let custom_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(settings.custom_range.clone())
                .placeholder(text("docx.print-panel.custom-placeholder"))
        });
        let copies_input=cx.new(|cx|InputState::new(window,cx).default_value(settings.copies.to_string()));
        cx.subscribe(&copies_input,|e,input,event,cx| {
            if matches!(event,gpui_kit::component::input::InputEvent::Change) {
                if let Ok(value)=input.read(cx).value().parse::<u32>() { e.edit_docx_print_settings(|s|s.copies=value.clamp(1,99),cx); }
            }
        }).detach();
        let focus = custom_input.clone();

        if let Some(s) = self.doc_state_mut() {
            s.print_panel = Some(DocxPrintPanelState {
                copies_input,
                custom_input,
                error: None,
            });
        }
        self.sidebar_open = true;
        if settings.range == DocxPrintRange::Custom {
            focus.update(cx, |f, cx| f.focus(window, cx));
        }
        cx.notify();
    }

    /// 印刷設定パネルを閉じる
    pub(crate) fn close_doc_print_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.print_panel = None;
        }
        cx.notify();
    }

    /// 印刷設定を編集する
    pub(crate) fn edit_docx_print_settings(
        &mut self,
        change: impl FnOnce(&mut DocxPrintSettings),
        cx: &mut Context<Self>,
    ) {
        if let Some(s) = self.doc_state_mut() {
            change(&mut s.print_settings);
            if let Some(panel) = s.print_panel.as_mut() {
                panel.error = None;
            }
        }
        cx.notify();
    }

    /// 印刷実行時の CLI 引数と対象ページを算出する
    pub(crate) fn docx_print_job_args(&self) -> (Option<Vec<usize>>, Option<Vec<String>>) {
        let Some(doc) = self.flow() else { return (None, None) };
        let settings = self.docx_print_settings();
        let cur = self.current_flow_page();
        let sel = self.selected_flow_pages();
        let resolved = resolve_print_pages(doc, &self.resources, &settings, cur, sel.as_deref())
            .unwrap_or_else(|_| vec![1]);
        let total = flow_layout::pages(doc, &self.resources).pages.len().max(1);
        let (slides, mut args) = build_print_cli_args(&resolved, &settings, total);

        // 印刷モードが N-up または枠線ありの場合、画像経由の PDF-Image が必要
        if settings.pages_per_sheet.is_some_and(|n| n > 1) || settings.page_border {
            let mut full_args = vec!["--export".to_string(), "pdf-image".to_string()];
            if let Some(extra) = args {
                full_args.extend(extra);
            }
            args = Some(full_args);
        }

        (slides, args)
    }

    /// パネルで「印刷…」を実行する
    pub(crate) fn apply_docx_print_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(doc) = self.flow().cloned() else { return };
        let custom_text = if let Some(s) = self.doc_state() {
            s.print_panel.as_ref().map(|p| p.custom_input.read(cx).value().to_string())
        } else {
            None
        };

        if let Some(custom) = custom_text {
            if let Some(s) = self.doc_state_mut() {
                s.print_settings.custom_range = custom;
            }
        }

        let settings = self.docx_print_settings();
        let cur = self.current_flow_page();
        let sel = self.selected_flow_pages();

        // ページ範囲の検証
        match resolve_print_pages(&doc, &self.resources, &settings, cur, sel.as_deref()) {
            Ok(resolved) => {
                if resolved.is_empty() {
                    if let Some(p) = self.doc_state_mut().and_then(|s| s.print_panel.as_mut()) {
                        p.error = Some(text("docx.print-panel.no-pages-selected").to_string());
                    }
                    cx.notify();
                    return;
                }
                // パネルを閉じて即座に印刷実行
                self.close_doc_print_panel(cx);
                self.print_now(window, cx);
            }
            Err(err) => {
                if let Some(p) = self.doc_state_mut().and_then(|s| s.print_panel.as_mut()) {
                    p.error = Some(err);
                }
                cx.notify();
            }
        }
    }

    /// 印刷設定サイドバーパネルの描画
    pub(crate) fn doc_print_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        self.doc_state()?.print_panel.as_ref()?;
        self.render_doc_print_panel(cx)
    }

    #[inline(never)]
    fn render_doc_print_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let (settings, input, error) = {
            let s = self.doc_state()?;
            let panel = s.print_panel.as_ref()?;
            (s.print_settings.clone(), panel.custom_input.clone(), panel.error.clone())
        };

        let current_page = self.current_flow_page();
        let has_selection = self.selected_flow_pages().is_some();
        let total_pages = self.flow().map(|d| flow_layout::pages(d, &self.resources).pages.len()).unwrap_or(1);


        let mut body = v_flex().gap_3().p_3().p_0();

        // エラー表示
        if let Some(err) = error {
            body = body.child(
                h_flex()
                    .p_2()
                    .rounded_md()
                    .bg(cx.theme().danger.opacity(0.12))
                    .border_1()
                    .border_color(cx.theme().danger)
                    .child(
                        v_flex()
                            .child(div().text_sm().text_color(cx.theme().danger).child(err))
                    ),
            );
        }

        // --- セクション 1: 印刷範囲 (DOCX-SEC-013) ---
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.print-panel.section.range").to_string()))
                .child(
                    crate::shell::chrome::radio_button("docx-print-range-all", settings.range == DocxPrintRange::All, false, text("docx.print-panel.range.all"), cx.listener(|this, _, _, cx| {
                            this.edit_docx_print_settings(|s| s.range = DocxPrintRange::All, cx);
                        })),
                )
                .child(
                    crate::shell::chrome::radio_button("docx-print-range-current", settings.range == DocxPrintRange::Current, false, format!("{} ({})", text("docx.print-panel.range.current"), current_page), cx.listener(|this, _, _, cx| {
                            this.edit_docx_print_settings(|s| s.range = DocxPrintRange::Current, cx);
                        })),
                )
                .child(
                    crate::shell::chrome::radio_button("docx-print-range-selection", settings.range == DocxPrintRange::Selection, false, format!("{}{}", text("docx.print-panel.range.selection"), if has_selection { String::new() } else { format!(" ({})", text("docx.print-panel.range.no-selection")) }), cx.listener(|this, _, _, cx| {
                            if this.selected_flow_pages().is_some() {
                                this.edit_docx_print_settings(|s| s.range = DocxPrintRange::Selection, cx);
                            }
                        })),
                )
                .child(
                    crate::shell::chrome::radio_button("docx-print-range-custom", settings.range == DocxPrintRange::Custom, false, text("docx.print-panel.range.custom"), cx.listener(|this, _, _, cx| {
                            this.edit_docx_print_settings(|s| s.range = DocxPrintRange::Custom, cx);
                        })),
                ),
        );

        // ユーザー指定範囲入力フィールド
        if settings.range == DocxPrintRange::Custom {
            body = body.child(
                v_flex()
                    .pl_4()
                    .gap_1()
                    .child(crate::shell::chrome::text_field(&input).full_width())
                    .child(div().text_sm().text_color(cx.theme().muted_foreground).child(text("docx.print-panel.custom-hint").to_string())),
            );
        }

        // 絞り込み (奇数・偶数ページ DOCX-SEC-013)
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.print-panel.section.subset").to_string()))
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            crate::shell::chrome::radio_button("docx-print-subset-all", settings.subset == DocxPageSubset::All, false, text("docx.print-panel.subset.all"), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.subset = DocxPageSubset::All, cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::radio_button("docx-print-subset-odd", settings.subset == DocxPageSubset::OddOnly, false, text("docx.print-panel.subset.odd"), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.subset = DocxPageSubset::OddOnly, cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::radio_button("docx-print-subset-even", settings.subset == DocxPageSubset::EvenOnly, false, text("docx.print-panel.subset.even"), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.subset = DocxPageSubset::EvenOnly, cx);
                                })),
                        ),
                ),
        );

        // --- セクション 2: 部数・両面印刷 (DOCX-SEC-015) ---
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.print-panel.section.copies-duplex").to_string()))
                .child(
                    h_flex().items_center().gap_3()
                        .child(div().text_sm().child(text("docx.print-panel.copies")))
                        .child(crate::shell::chrome::text_field(&self.doc_state()?.print_panel.as_ref()?.copies_input)
                            .spinner("docx-print-copies",crate::shell::chrome::SpinnerSpec::new(1.).range(1.,99.).integer_only(),cx,
                                |e,value,_,cx|e.edit_docx_print_settings(|s|s.copies=value as u32,cx)))
                )
                .child(
                    Checkbox::new("docx-print-collate")
                        .label(text("docx.print-panel.collate"))
                        .checked(settings.collate)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            this.edit_docx_print_settings(|s| s.collate = *on, cx);
                        })),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            crate::shell::chrome::radio_button("docx-print-duplex-one", settings.duplex == DocxDuplex::OneSided, false, text("docx.print-panel.duplex.one-sided"), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.duplex = DocxDuplex::OneSided, cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::radio_button("docx-print-duplex-long", settings.duplex == DocxDuplex::BothSidesLongEdge, false, text("docx.print-panel.duplex.both-long"), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.duplex = DocxDuplex::BothSidesLongEdge, cx);
                                })),
                        ),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            crate::shell::chrome::radio_button("docx-print-duplex-short", settings.duplex == DocxDuplex::BothSidesShortEdge, false, text("docx.print-panel.duplex.both-short"), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.duplex = DocxDuplex::BothSidesShortEdge, cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::radio_button("docx-print-duplex-manual", settings.duplex == DocxDuplex::Manual, false, text("docx.print-panel.duplex.manual"), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.duplex = DocxDuplex::Manual, cx);
                                })),
                        ),
                ),
        );

        // --- セクション 3: 1枚に複数ページ (DOCX-SEC-016) ---
        let nup_label = |n: Option<u8>| {
            match n {
                None | Some(1) => text("docx.print-panel.nup.1"),
                Some(2) => text("docx.print-panel.nup.2"),
                Some(4) => text("docx.print-panel.nup.4"),
                Some(6) => text("docx.print-panel.nup.6"),
                Some(8) => text("docx.print-panel.nup.8"),
                Some(16) => text("docx.print-panel.nup.16"),
                _ => text("docx.print-panel.nup.1"),
            }
        };

        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.print-panel.section.nup").to_string()))
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            crate::shell::chrome::radio_button("docx-print-nup-1", settings.pages_per_sheet.is_none(), false, nup_label(None), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.pages_per_sheet = None, cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::radio_button("docx-print-nup-2", settings.pages_per_sheet == Some(2), false, nup_label(Some(2)), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.pages_per_sheet = Some(2), cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::radio_button("docx-print-nup-4", settings.pages_per_sheet == Some(4), false, nup_label(Some(4)), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.pages_per_sheet = Some(4), cx);
                                })),
                        ),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .child(
                            crate::shell::chrome::radio_button("docx-print-nup-6", settings.pages_per_sheet == Some(6), false, nup_label(Some(6)), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.pages_per_sheet = Some(6), cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::radio_button("docx-print-nup-8", settings.pages_per_sheet == Some(8), false, nup_label(Some(8)), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.pages_per_sheet = Some(8), cx);
                                })),
                        )
                        .child(
                            crate::shell::chrome::radio_button("docx-print-nup-16", settings.pages_per_sheet == Some(16), false, nup_label(Some(16)), cx.listener(|this, _, _, cx| {
                                    this.edit_docx_print_settings(|s| s.pages_per_sheet = Some(16), cx);
                                })),
                        ),
                )
                .child(
                    Checkbox::new("docx-print-page-border")
                        .label(text("docx.print-panel.page-border"))
                        .checked(settings.page_border)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            this.edit_docx_print_settings(|s| s.page_border = *on, cx);
                        })),
                ),
        );

        // --- セクション 4: 印刷オプション (Markup, Backgrounds, Hidden) ---
        let print_markup = self.is_print_markup();
        let print_bg = self.is_print_backgrounds();
        let print_hidden = self.is_print_hidden_text();

        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(text("docx.print-panel.section.options").to_string()))
                .child(
                    Checkbox::new("docx-print-opt-markup")
                        .label(text("docx.options.print.markup"))
                        .checked(print_markup)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            this.set_print_markup(*on, cx);
                        })),
                )
                .child(
                    Checkbox::new("docx-print-opt-bg")
                        .label(text("docx.options.print.backgrounds"))
                        .checked(print_bg)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            this.set_print_backgrounds(*on, cx);
                        })),
                )
                .child(
                    Checkbox::new("docx-print-opt-hidden")
                        .label(text("docx.options.print.hidden-text"))
                        .checked(print_hidden)
                        .on_click(cx.listener(|this, on: &bool, _, cx| {
                            this.set_print_hidden_text(*on, cx);
                        })),
                ),
        );

        // サマリー情報
        let summary_text = if let Some(doc) = self.flow() {
            let sel = self.selected_flow_pages();
            if let Ok(pages) = resolve_print_pages(doc, &self.resources, &settings, current_page, sel.as_deref()) {
                i18n_format(
                    "docx.print-panel.summary",
                    &[
                        ("pages", &pages.len().to_string()),
                        ("total", &total_pages.to_string()),
                        ("copies", &settings.copies.to_string()),
                    ],
                )
            } else {
                crate::shell::i18n::format("docx.print-panel.pages-of-total", &[("total", &total_pages)])
            }
        } else {
            String::new()
        };

        body = body.child(div().text_sm().text_color(cx.theme().muted_foreground).child(summary_text));

        // アクションボタン
        let footer = crate::shell::chrome::action_bar()
            .action(
                crate::shell::chrome::ActionDefinition::new("docx-print-apply", text("docx.print-panel.print"), crate::shell::chrome::ActionButtonKind::Primary)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.apply_docx_print_panel(window, cx);
                    })),
            )
            .action(
                crate::shell::chrome::ActionDefinition::new("docx-print-cancel", text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_doc_print_panel(cx);
                    })),
            );

        let panel_terminal_actions = footer;
        Some(SidebarPanel::with_footer(text("docx.print-panel.title"), body.into_any_element(), panel_terminal_actions))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_page_subset, build_print_cli_args, expand_copies_and_collate, parse_docx_page_range,
        DocxPageSubset, DocxPrintRange, DocxPrintSettings, PageInfo,
    };

    fn sample_pages() -> Vec<PageInfo> {
        // Section 1: pages 1, 2 (page_in_sec 1, 2)
        // Section 2: pages 3, 4, 5 (page_in_sec 1, 2, 3)
        // Section 3: pages 6, 7 (page_in_sec 1, 2)
        vec![
            PageInfo { global_index: 1, section_index: 1, page_in_section: 1 },
            PageInfo { global_index: 2, section_index: 1, page_in_section: 2 },
            PageInfo { global_index: 3, section_index: 2, page_in_section: 1 },
            PageInfo { global_index: 4, section_index: 2, page_in_section: 2 },
            PageInfo { global_index: 5, section_index: 2, page_in_section: 3 },
            PageInfo { global_index: 6, section_index: 3, page_in_section: 1 },
            PageInfo { global_index: 7, section_index: 3, page_in_section: 2 },
        ]
    }

    #[test]
    fn test_parse_docx_page_range_pure_numbers() {
        let pages = sample_pages();
        // 単一ページ
        assert_eq!(parse_docx_page_range("3", &pages).unwrap(), vec![3]);
        // 連続範囲
        assert_eq!(parse_docx_page_range("2-4", &pages).unwrap(), vec![2, 3, 4]);
        // カンマ区切り
        assert_eq!(parse_docx_page_range("1, 3, 6-7", &pages).unwrap(), vec![1, 3, 6, 7]);
    }

    #[test]
    fn test_parse_docx_page_range_sections() {
        let pages = sample_pages();
        // セクション2の全ページ
        assert_eq!(parse_docx_page_range("s2", &pages).unwrap(), vec![3, 4, 5]);
        // セクション1と3
        assert_eq!(parse_docx_page_range("s1, s3", &pages).unwrap(), vec![1, 2, 6, 7]);
        // セクション範囲
        assert_eq!(parse_docx_page_range("s1-s2", &pages).unwrap(), vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_parse_docx_page_range_page_in_section() {
        let pages = sample_pages();
        // セクション2の1ページ目
        assert_eq!(parse_docx_page_range("p1s2", &pages).unwrap(), vec![3]);
        // セクション2の3ページ目
        assert_eq!(parse_docx_page_range("p3s2", &pages).unwrap(), vec![5]);
        // セクション2内の範囲
        assert_eq!(parse_docx_page_range("p1s2-p2s2", &pages).unwrap(), vec![3, 4]);
    }

    #[test]
    fn test_parse_docx_page_range_cross_section() {
        let pages = sample_pages();
        // セクション1の2ページ目からセクション2の2ページ目まで
        assert_eq!(parse_docx_page_range("p2s1-p2s2", &pages).unwrap(), vec![2, 3, 4]);
        // セクション2の1ページ目からセクション3の全末尾まで
        assert_eq!(parse_docx_page_range("p1s2-s3", &pages).unwrap(), vec![3, 4, 5, 6, 7]);
    }

    #[test]
    fn test_apply_page_subset() {
        let list = vec![1, 2, 3, 4, 5];
        assert_eq!(apply_page_subset(&list, DocxPageSubset::All), vec![1, 2, 3, 4, 5]);
        assert_eq!(apply_page_subset(&list, DocxPageSubset::OddOnly), vec![1, 3, 5]);
        assert_eq!(apply_page_subset(&list, DocxPageSubset::EvenOnly), vec![2, 4]);
    }

    #[test]
    fn test_expand_copies_and_collate() {
        let list = vec![1, 2, 3];
        // 1部
        assert_eq!(expand_copies_and_collate(&list, 1, true), vec![1, 2, 3]);
        // 2部・部単位 (1,2,3, 1,2,3)
        assert_eq!(expand_copies_and_collate(&list, 2, true), vec![1, 2, 3, 1, 2, 3]);
        // 2部・ページごと (1,1, 2,2, 3,3)
        assert_eq!(expand_copies_and_collate(&list, 2, false), vec![1, 1, 2, 2, 3, 3]);
    }

    #[test]
    fn test_build_print_cli_args() {
        let settings = DocxPrintSettings {
            range: DocxPrintRange::Custom,
            custom_range: "1,3".to_string(),
            pages_per_sheet: Some(2),
            page_border: true,
            ..Default::default()
        };
        let (slides, args) = build_print_cli_args(&[1, 3], &settings, 5);
        assert_eq!(slides, Some(vec![1, 3]));
        let args = args.unwrap();
        assert!(args.contains(&"--slides".to_string()));
        assert!(args.contains(&"1,3".to_string()));
        assert!(args.contains(&"--handout".to_string()));
        assert!(args.contains(&"2".to_string()));
        assert!(args.contains(&"--print-frame".to_string()));
    }
}
