// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! ジャンプ (DOCX-FIND-010 / Word's Go To dialog / 編集 > ジャンプ…, ⌥⌘G / F5):
//! ページ、セクション、行、ブックマーク、コメント、脚注、文末脚注、見出し、表、図へのジャンプ。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use std::cell::RefCell;

use gpui_kit::component::button::{ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Block, FlowDocument, FlowPos, FlowRunKind};

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

/// 移動先の種別 (Word: 移動先 / Go to what).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum GoToTarget {
    #[default]
    Page,
    Section,
    Line,
    Bookmark,
    Comment,
    Footnote,
    Endnote,
    Heading,
    Table,
    Graphic,
}

impl GoToTarget {
    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::Page => "docx.goto.page",
            Self::Section => "docx.goto.section",
            Self::Line => "docx.goto.line",
            Self::Bookmark => "docx.goto.bookmark",
            Self::Comment => "docx.goto.comment",
            Self::Footnote => "docx.goto.footnote",
            Self::Endnote => "docx.goto.endnote",
            Self::Heading => "docx.goto.heading",
            Self::Table => "docx.goto.table",
            Self::Graphic => "docx.goto.graphic",
        }
    }

    pub(crate) fn input_label_key(self) -> &'static str {
        match self {
            Self::Page => "docx.goto.enter-page",
            Self::Section => "docx.goto.enter-section",
            Self::Line => "docx.goto.enter-line",
            Self::Bookmark => "docx.goto.enter-bookmark",
            Self::Comment => "docx.goto.enter-comment",
            Self::Footnote => "docx.goto.enter-footnote",
            Self::Endnote => "docx.goto.enter-endnote",
            Self::Heading => "docx.goto.enter-heading",
            Self::Table => "docx.goto.enter-table",
            Self::Graphic => "docx.goto.enter-graphic",
        }
    }
}

pub(crate) struct GoToPanel {
    pub(crate) target: GoToTarget,
    pub(crate) value_input: Entity<InputState>,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

/// 本文中の全段落の出現順インデックス (0, 1, 2, ...).
fn body_para_order(doc: &FlowDocument) -> HashMap<u32, usize> {
    let mut order = HashMap::new();
    let mut idx = 0;
    fn walk(blocks: &[Block], idx: &mut usize, order: &mut HashMap<u32, usize>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    order.insert(p.id, *idx);
                    *idx += 1;
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, idx, order);
                        }
                    }
                }
            }
        }
    }
    walk(&doc.body, &mut idx, &mut order);
    order
}

/// 各ページの先頭段落 ID 一覧。
pub(crate) fn document_page_paras(editor: &SlideEditor) -> Vec<u32> {
    let Some(doc) = editor.flow() else { return Vec::new() };
    let pgs = super::flow_layout::pages(doc, &editor.resources);
    let mut paras = Vec::new();
    for page in &pgs.pages {
        if let Some(p) = page.items.iter().find_map(|it| {
            if let super::flow_layout::Item::Line { para, .. } = it {
                Some(*para)
            } else {
                None
            }
        }) {
            paras.push(p);
        }
    }
    paras
}

/// 各セクションの先頭段落 ID 一覧。
pub(crate) fn document_section_paras(editor: &SlideEditor) -> Vec<u32> {
    let Some(doc) = editor.flow() else { return Vec::new() };
    let pgs = super::flow_layout::pages(doc, &editor.resources);
    let mut paras = Vec::new();
    let mut seen = HashSet::new();
    for page in &pgs.pages {
        if seen.insert(page.section) {
            if let Some(p) = page.items.iter().find_map(|it| {
                if let super::flow_layout::Item::Line { para, .. } = it {
                    Some(*para)
                } else {
                    None
                }
            }) {
                paras.push(p);
            }
        }
    }
    if paras.is_empty() && !doc.body.is_empty() {
        if let Block::Paragraph(p) = &doc.body[0] {
            paras.push(p.id);
        }
    }
    paras
}

/// 全行の段落 ID 一覧。
pub(crate) fn document_line_paras(editor: &SlideEditor) -> Vec<u32> {
    let Some(doc) = editor.flow() else { return Vec::new() };
    let pgs = super::flow_layout::pages(doc, &editor.resources);
    let mut paras = Vec::new();
    for page in &pgs.pages {
        for item in &page.items {
            if let super::flow_layout::Item::Line { para, .. } = item {
                paras.push(*para);
            }
        }
    }
    paras
}

/// ブックマーク一覧: (名前, 段落ID).
pub(crate) fn document_bookmarks(doc: &FlowDocument) -> Vec<(String, u32)> {
    fn walk(blocks: &[Block], out: &mut Vec<(String, u32)>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for b in &p.bookmarks {
                        out.push((b.clone(), p.id));
                    }
                }
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&doc.body, &mut out);
    out
}

/// 脚注参照段落 ID 一覧。
pub(crate) fn document_footnote_paras(editor: &SlideEditor) -> Vec<u32> {
    let Some(doc) = editor.flow() else { return Vec::new() };
    let mut paras = Vec::new();
    for b in &doc.body {
        if let Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::NoteRef { endnote: false, .. })) {
                paras.push(p.id);
            }
        }
    }
    paras
}

/// 文末脚注参照段落 ID 一覧。
pub(crate) fn document_endnote_paras(editor: &SlideEditor) -> Vec<u32> {
    let Some(doc) = editor.flow() else { return Vec::new() };
    let mut paras = Vec::new();
    for b in &doc.body {
        if let Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::NoteRef { endnote: true, .. })) {
                paras.push(p.id);
            }
        }
    }
    paras
}

/// 表段落 ID 一覧（各表の最初の段落）。
pub(crate) fn document_table_paras(editor: &SlideEditor) -> Vec<u32> {
    let Some(doc) = editor.flow() else { return Vec::new() };
    let mut paras = Vec::new();
    for b in &doc.body {
        if let Block::Table(t) = b {
            if let Some(row) = t.rows.first() {
                if let Some(cell) = row.cells.first() {
                    if let Some(Block::Paragraph(p)) = cell.blocks.first() {
                        paras.push(p.id);
                    }
                }
            }
        }
    }
    paras
}

/// 図・画像段落 ID 一覧。
pub(crate) fn document_graphic_paras(editor: &SlideEditor) -> Vec<u32> {
    let Some(doc) = editor.flow() else { return Vec::new() };
    let mut paras = Vec::new();
    for b in &doc.body {
        if let Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::Picture { .. } | FlowRunKind::Shape(_))) {
                paras.push(p.id);
            }
        }
    }
    paras
}

/// コメント段落 ID 一覧。
pub(crate) fn document_comment_paras(editor: &SlideEditor) -> Vec<u32> {
    let Some(doc) = editor.flow() else { return Vec::new() };
    let mut paras = Vec::new();
    for b in &doc.body {
        if let Block::Paragraph(p) = b {
            if p.runs.iter().any(|r| matches!(r.kind, FlowRunKind::CommentStart(_) | FlowRunKind::CommentRef(_))) {
                paras.push(p.id);
            }
        }
    }
    paras
}

impl SlideEditor {
    /// 編集 > ジャンプ…: サイドバーにジャンプパネルを開く。
    pub(crate) fn open_go_to_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let value_input = cx.new(|cx| InputState::new(window, cx).placeholder(text("docx.goto.placeholder")));
        let panel = GoToPanel {
            target: GoToTarget::Page,
            value_input,
            message: String::new(),
            ui: Rc::new(RefCell::new(BTreeMap::new())),
        };
        if let Some(s) = self.doc_state_mut() {
            s.helper.bookmarks = None;
            s.helper.page_setup = None;
            s.helper.notes = None;
            s.helper.watermark = None;
            s.helper.word_count = None;
            s.helper.properties = None;
            s.helper.cross_reference = None;
            s.helper.caption = None;
            s.helper.field = None;
            s.helper.update_toc = None;
            s.helper.user_info = None;
            s.helper.go_to = Some(panel);
        }
        self.sidebar_open = true;
        if let Some(inp) = self.doc_state().and_then(|s| s.helper.go_to.as_ref()).map(|p| p.value_input.clone()) {
            inp.update(cx, |inp, cx| inp.focus(window, cx));
        }
        cx.notify();
    }

    /// ジャンプパネルを閉じる。
    pub(crate) fn close_go_to_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.doc_state_mut() {
            s.helper.go_to = None;
        }
        self.sidebar_open = false;
        cx.notify();
    }

    /// 移動先（GoToTarget）を変更。
    pub(crate) fn set_go_to_target(&mut self, target: GoToTarget, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.go_to.as_mut()) {
            p.target = target;
            p.message.clear();
        }
        cx.notify();
    }

    /// 指定された入力値に基づいてジャンプを実行。
    pub(crate) fn go_to_jump(&mut self, cx: &mut Context<Self>) {
        let Some((target, val_str)) = self.doc_state().and_then(|s| s.helper.go_to.as_ref()).map(|p| {
            (p.target, p.value_input.read(cx).text().to_string().trim().to_string())
        }) else { return };

        if val_str.is_empty() {
            self.go_to_next(cx);
            return;
        }

        match target {
            GoToTarget::Page => self.jump_page(&val_str, cx),
            GoToTarget::Section => self.jump_section(&val_str, cx),
            GoToTarget::Line => self.jump_line(&val_str, cx),
            GoToTarget::Bookmark => self.jump_bookmark(&val_str, cx),
            GoToTarget::Heading => self.jump_heading(&val_str, cx),
            GoToTarget::Footnote => self.jump_numbered_list(document_footnote_paras, &val_str, "fix.goto.footnote", cx),
            GoToTarget::Endnote => self.jump_numbered_list(document_endnote_paras, &val_str, "fix.goto.endnote", cx),
            GoToTarget::Table => self.jump_numbered_list(document_table_paras, &val_str, "insert.table", cx),
            GoToTarget::Graphic => self.jump_numbered_list(document_graphic_paras, &val_str, "docx.common.figure", cx),
            GoToTarget::Comment => self.jump_numbered_list(document_comment_paras, &val_str, "_shell.comment", cx),
        }
    }

    /// 「前へ」ボタン実行。
    pub(crate) fn go_to_previous(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.doc_state().and_then(|s| s.helper.go_to.as_ref()).map(|p| p.target) else { return };
        match target {
            GoToTarget::Page => self.jump_page("-1", cx),
            GoToTarget::Section => self.navigate_items(document_section_paras, false, "pptx.goto.section", cx),
            GoToTarget::Line => self.jump_line("-1", cx),
            GoToTarget::Bookmark => self.navigate_bookmarks(false, cx),
            GoToTarget::Heading => self.navigate_headings(false, cx),
            GoToTarget::Footnote => self.navigate_items(document_footnote_paras, false, "fix.goto.footnote", cx),
            GoToTarget::Endnote => self.navigate_items(document_endnote_paras, false, "fix.goto.endnote", cx),
            GoToTarget::Table => self.navigate_items(document_table_paras, false, "insert.table", cx),
            GoToTarget::Graphic => self.navigate_items(document_graphic_paras, false, "docx.common.figure", cx),
            GoToTarget::Comment => self.navigate_items(document_comment_paras, false, "_shell.comment", cx),
        }
    }

    /// 「次へ」ボタン実行。
    pub(crate) fn go_to_next(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.doc_state().and_then(|s| s.helper.go_to.as_ref()).map(|p| p.target) else { return };
        match target {
            GoToTarget::Page => self.jump_page("+1", cx),
            GoToTarget::Section => self.navigate_items(document_section_paras, true, "pptx.goto.section", cx),
            GoToTarget::Line => self.jump_line("+1", cx),
            GoToTarget::Bookmark => self.navigate_bookmarks(true, cx),
            GoToTarget::Heading => self.navigate_headings(true, cx),
            GoToTarget::Footnote => self.navigate_items(document_footnote_paras, true, "fix.goto.footnote", cx),
            GoToTarget::Endnote => self.navigate_items(document_endnote_paras, true, "fix.goto.endnote", cx),
            GoToTarget::Table => self.navigate_items(document_table_paras, true, "insert.table", cx),
            GoToTarget::Graphic => self.navigate_items(document_graphic_paras, true, "docx.common.figure", cx),
            GoToTarget::Comment => self.navigate_items(document_comment_paras, true, "_shell.comment", cx),
        }
    }

    /// ページ移動処理（絶対番号または +N, -N）.
    fn jump_page(&mut self, val_str: &str, cx: &mut Context<Self>) {
        let pages = document_page_paras(self);
        if pages.is_empty() {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
            return;
        }

        let cur_para = self.text_selection().map(|(_, c)| c.para).unwrap_or(0);
        let cur_page_idx = pages.iter().position(|&p| p == cur_para).unwrap_or_else(|| {
            let doc = self.flow().unwrap();
            let order = body_para_order(doc);
            let cur_ord = order.get(&cur_para).copied().unwrap_or(0);
            pages.iter().rposition(|&p| order.get(&p).copied().unwrap_or(0) <= cur_ord).unwrap_or(0)
        });

        let target_idx = if let Some(stripped) = val_str.strip_prefix('+') {
            let count = stripped.trim().parse::<usize>().unwrap_or(1);
            (cur_page_idx + count).min(pages.len() - 1)
        } else if let Some(stripped) = val_str.strip_prefix('-') {
            let count = stripped.trim().parse::<usize>().unwrap_or(1);
            cur_page_idx.saturating_sub(count)
        } else if let Ok(num) = val_str.parse::<usize>() {
            num.saturating_sub(1).min(pages.len() - 1)
        } else {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
            return;
        };

        if let Some(&para) = pages.get(target_idx) {
            self.jump_to(para, cx);
            self.set_goto_message(crate::shell::i18n::format("_docx.moved-to-page-page", &[("page", &(target_idx + 1))]), cx);
        }
    }

    /// セクション移動処理.
    fn jump_section(&mut self, val_str: &str, cx: &mut Context<Self>) {
        let sections = document_section_paras(self);
        if sections.is_empty() {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
            return;
        }
        if let Ok(num) = val_str.parse::<usize>() {
            let target_idx = num.saturating_sub(1).min(sections.len() - 1);
            if let Some(&para) = sections.get(target_idx) {
                self.jump_to(para, cx);
                self.set_goto_message(crate::shell::i18n::format("_docx.moved-to-section-section", &[("section", &(target_idx + 1))]), cx);
            }
        } else {
            self.navigate_items(document_section_paras, true, "pptx.goto.section", cx);
        }
    }

    /// 行移動処理（絶対行番号または +N, -N）.
    fn jump_line(&mut self, val_str: &str, cx: &mut Context<Self>) {
        let lines = document_line_paras(self);
        if lines.is_empty() {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
            return;
        }

        let cur_para = self.text_selection().map(|(_, c)| c.para).unwrap_or(0);
        let cur_line_idx = lines.iter().position(|&p| p == cur_para).unwrap_or(0);

        let target_idx = if let Some(stripped) = val_str.strip_prefix('+') {
            let count = stripped.trim().parse::<usize>().unwrap_or(1);
            (cur_line_idx + count).min(lines.len() - 1)
        } else if let Some(stripped) = val_str.strip_prefix('-') {
            let count = stripped.trim().parse::<usize>().unwrap_or(1);
            cur_line_idx.saturating_sub(count)
        } else if let Ok(num) = val_str.parse::<usize>() {
            num.saturating_sub(1).min(lines.len() - 1)
        } else {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
            return;
        };

        if let Some(&para) = lines.get(target_idx) {
            self.jump_to(para, cx);
            self.set_goto_message(crate::shell::i18n::format("_docx.moved-to-line-line", &[("line", &(target_idx + 1))]), cx);
        }
    }

    /// ブックマーク移動処理（名前検索または前／次）.
    fn jump_bookmark(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let bms = document_bookmarks(doc);
        if let Some((bm_name, para)) = bms.into_iter().find(|(n, _)| n.eq_ignore_ascii_case(name) || n == name) {
            self.jump_to(para, cx);
            self.set_goto_message(crate::shell::i18n::format("_docx.moved-to-bookmark-bm-name", &[("bm_name", &bm_name)]), cx);
        } else {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
        }
    }

    /// 見出し移動処理（番号または見出しテキスト）.
    fn jump_heading(&mut self, query: &str, cx: &mut Context<Self>) {
        let headings = self.outline();
        if headings.is_empty() {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
            return;
        }

        if let Ok(num) = query.parse::<usize>() {
            let idx = num.saturating_sub(1).min(headings.len() - 1);
            let (_, title, para) = &headings[idx];
            self.jump_to(*para, cx);
            self.set_goto_message(crate::shell::i18n::format("_docx.moved-to-heading-title", &[("title", &title)]), cx);
        } else if let Some((_, title, para)) = headings.iter().find(|(_, t, _)| t.to_lowercase().contains(&query.to_lowercase())) {
            self.jump_to(*para, cx);
            self.set_goto_message(crate::shell::i18n::format("_docx.moved-to-heading-title", &[("title", &title)]), cx);
        } else {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
        }
    }

    /// 番号指定リスト（脚注、文末脚注、表、図、コメント）への移動.
    fn jump_numbered_list<F>(&mut self, getter: F, val_str: &str, item_name: &'static str, cx: &mut Context<Self>)
    where
        F: FnOnce(&Self) -> Vec<u32>,
    {
        let item_name = crate::shell::i18n::text(item_name);
        let items = getter(self);
        if items.is_empty() {
            self.set_goto_message(crate::shell::i18n::format("_docx.item-name-was-not-found", &[("item_name", &item_name)]), cx);
            return;
        }

        if let Ok(num) = val_str.parse::<usize>() {
            let idx = num.saturating_sub(1).min(items.len() - 1);
            let para = items[idx];
            self.jump_to(para, cx);
            self.set_goto_message(crate::shell::i18n::format("_docx.moved-to-item-name-n", &[("item_name", &item_name), ("n", &(idx + 1))]), cx);
        } else {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
        }
    }

    /// 相対ナビゲーション（前へ／次へ）.
    pub(crate) fn navigate_items<F>(&mut self, getter: F, forward: bool, item_name: &'static str, cx: &mut Context<Self>)
    where
        F: FnOnce(&Self) -> Vec<u32>,
    {
        let item_name = crate::shell::i18n::text(item_name);
        let items = getter(self);
        if items.is_empty() {
            self.set_goto_message(crate::shell::i18n::format("_docx.item-name-was-not-found", &[("item_name", &item_name)]), cx);
            return;
        }

        let Some(doc) = self.flow() else { return };
        let order = body_para_order(doc);
        let cur_para = self.text_selection().map(|(_, c)| c.para).unwrap_or(0);
        let cur_ord = order.get(&cur_para).copied().unwrap_or(0);

        let target = if forward {
            items.iter().find(|&&p| order.get(&p).copied().unwrap_or(0) > cur_ord).copied().or_else(|| items.first().copied())
        } else {
            items.iter().rfind(|&&p| order.get(&p).copied().unwrap_or(0) < cur_ord).copied().or_else(|| items.last().copied())
        };

        if let Some(para) = target {
            self.jump_to(para, cx);
            let idx = items.iter().position(|&p| p == para).unwrap_or(0) + 1;
            self.set_goto_message(crate::shell::i18n::format("_docx.moved-to-item-name-idx", &[("item_name", &item_name), ("idx", &idx)]), cx);
        }
    }

    /// ブックマーク前へ／次へナビゲーション.
    fn navigate_bookmarks(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(doc) = self.flow() else { return };
        let bms = document_bookmarks(doc);
        if bms.is_empty() {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
            return;
        }

        let order = body_para_order(doc);
        let cur_para = self.text_selection().map(|(_, c)| c.para).unwrap_or(0);
        let cur_ord = order.get(&cur_para).copied().unwrap_or(0);

        let target = if forward {
            bms.iter().find(|(_, p)| order.get(p).copied().unwrap_or(0) > cur_ord).or_else(|| bms.first())
        } else {
            bms.iter().rfind(|(_, p)| order.get(p).copied().unwrap_or(0) < cur_ord).or_else(|| bms.last())
        };

        if let Some((name, para)) = target {
            self.jump_to(*para, cx);
            self.set_goto_message(crate::shell::i18n::format("_docx.moved-to-bookmark-name", &[("name", &name)]), cx);
        }
    }

    /// 見出し前へ／次へナビゲーション.
    fn navigate_headings(&mut self, forward: bool, cx: &mut Context<Self>) {
        let headings = self.outline();
        if headings.is_empty() {
            self.set_goto_message(text("docx.goto.not-found").to_string(), cx);
            return;
        }

        let Some(doc) = self.flow() else { return };
        let order = body_para_order(doc);
        let cur_para = self.text_selection().map(|(_, c)| c.para).unwrap_or(0);
        let cur_ord = order.get(&cur_para).copied().unwrap_or(0);

        let target = if forward {
            headings.iter().find(|(_, _, p)| order.get(p).copied().unwrap_or(0) > cur_ord).or_else(|| headings.first())
        } else {
            headings.iter().rfind(|(_, _, p)| order.get(p).copied().unwrap_or(0) < cur_ord).or_else(|| headings.last())
        };

        if let Some((_, title, para)) = target {
            self.jump_to(*para, cx);
            self.set_goto_message(crate::shell::i18n::format("_docx.moved-to-heading-title", &[("title", &title)]), cx);
        }
    }

    fn set_goto_message(&mut self, msg: String, cx: &mut Context<Self>) {
        if let Some(p) = self.doc_state_mut().and_then(|s| s.helper.go_to.as_mut()) {
            p.message = msg;
        }
        cx.notify();
    }

    /// Render the Go To (ジャンプ) sidebar panel.
    pub(crate) fn go_to_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let p = self.doc_state()?.helper.go_to.as_ref()?;
        let current_target = p.target;
        let theme = cx.theme();
        let ui = p.ui.clone();

        let mark = |id: &str, child: AnyElement| {
            let ui = ui.clone();
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
            let ui = ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                    ui.borrow_mut().insert(id.clone(), bounds);
                })
        };

        let mut body = v_flex().gap_3().px(px(16.)).py(px(10.)).p_0();

        // 1. 移動先ヘッダー
        body = body.child(
            div().text_sm().font_weight(FontWeight::BOLD).text_color(theme.foreground).child(text("docx.goto.what"))
        );

        // 2. 移動先選択ボタン群
        let targets = [
            GoToTarget::Page,
            GoToTarget::Section,
            GoToTarget::Line,
            GoToTarget::Bookmark,
            GoToTarget::Heading,
            GoToTarget::Footnote,
            GoToTarget::Endnote,
            GoToTarget::Table,
            GoToTarget::Graphic,
            GoToTarget::Comment,
        ];

        let mut target_grid = h_flex().gap_1().flex_wrap();
        for t in targets {
            let is_sel = current_target == t;
            let id_str = format!("goto-target-{:?}", t).to_lowercase();
            target_grid = target_grid.child(
                mark(
                    &id_str,
                    crate::shell::chrome::radio_button(SharedString::from(id_str.clone()), is_sel, false, text(t.label_key()), cx.listener(move |this, _, _, cx| {
                            this.set_go_to_target(t, cx);
                        }))
                        .into_any_element(),
                )
            );
        }
        body = body.child(target_grid);

        // 3. 入力欄セクション
        body = body.child(
            v_flex()
                .gap_1()
                .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.muted_foreground).child(text(current_target.input_label_key())))
                .child(mark("goto-input", crate::shell::chrome::text_field(&p.value_input).full_width().into_any_element()))
        );

        // 4. ブックマークまたは見出し選択可能リスト
        if current_target == GoToTarget::Bookmark {
            if let Some(doc) = self.flow() {
                let bms = document_bookmarks(doc);
                if !bms.is_empty() {
                    let mut bm_box = v_flex().gap_1().max_h(px(120.)).overflow_hidden();
                    bm_box = bm_box.child(div().text_sm().text_color(theme.muted_foreground).child(text("_docx.existing-bookmarks")));
                    for (bm_name, _) in bms {
                        let name_clone = bm_name.clone();
                        let btn_id = format!("goto-bookmark-{bm_name}");
                        bm_box = bm_box.child(
                            mark(
                                &btn_id,
                                crate::shell::chrome::action_button(SharedString::from(btn_id.clone()))
                                    .small()

                                    .label(SharedString::from(bm_name))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.jump_bookmark(&name_clone, cx);
                                    }))
                                    .into_any_element(),
                            )
                        );
                    }
                    body = body.child(bm_box);
                }
            }
        } else if current_target == GoToTarget::Heading {
            let headings = self.outline();
            if !headings.is_empty() {
                let mut h_box = v_flex().gap_1().max_h(px(120.)).overflow_hidden();
                h_box = h_box.child(div().text_sm().text_color(theme.muted_foreground).child(text("_docx.headings-in-document")));
                for (lvl, title, para_id) in headings {
                    let indent = "  ".repeat(lvl.saturating_sub(1) as usize);
                    let label_str = format!("{indent}{title}");
                    let btn_id = format!("goto-heading-{para_id}");
                    h_box = h_box.child(
                        mark(
                            &btn_id,
                            crate::shell::chrome::action_button(SharedString::from(btn_id.clone()))
                                .small()

                                .label(SharedString::from(label_str))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.jump_to(para_id, cx);
                                    this.set_goto_message(crate::shell::i18n::format("_docx.moved-to-heading-title", &[("title", &title)]), cx);
                                }))
                                .into_any_element(),
                        )
                    );
                }
                body = body.child(h_box);
            }
        }

        // 5. アクションボタン: 前へ / 次へ / ジャンプ / 閉じる
        let action_row = crate::shell::chrome::action_bar()
            .action(
                mark_action("goto-jump", crate::shell::chrome::ActionDefinition::new("goto-jump", text("docx.goto.btn-jump"), crate::shell::chrome::ActionButtonKind::Primary)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.go_to_jump(cx);
                        })))
            )
            .action(
                mark_action("goto-prev", crate::shell::chrome::ActionDefinition::new("goto-prev", text("docx.goto.btn-prev"), crate::shell::chrome::ActionButtonKind::Normal)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.go_to_previous(cx);
                        })))
            )
            .action(
                mark_action("goto-next", crate::shell::chrome::ActionDefinition::new("goto-next", text("docx.goto.btn-next"), crate::shell::chrome::ActionButtonKind::Normal)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.go_to_next(cx);
                        })))
            )

        .action(crate::shell::chrome::dialog_close_definition(cx));

        // 6. メッセージ表示
        if !p.message.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(SharedString::from(p.message.clone()))
            );
        }

        Some(SidebarPanel::with_footer(text("docx.goto.title"),body.into_any_element(),action_row))
    }
}

impl SlideEditor {
    /// 参考資料 > 次の脚注／前の脚注（文末脚注）: the caret goes to the next (previous) note reference, round the end of the
    /// document (DOCX-REF-017).
    pub(crate) fn go_to_note(&mut self, endnote: bool, forward: bool, cx: &mut Context<Self>) {
        if endnote {
            self.navigate_items(document_endnote_paras, forward, "fix.goto.endnote", cx);
        } else {
            self.navigate_items(document_footnote_paras, forward, "fix.goto.footnote", cx);
        }
    }

    /// 校閲 > 次のコメント／前のコメント (DOCX-REVIEW-022).
    pub(crate) fn go_to_comment(&mut self, forward: bool, cx: &mut Context<Self>) {
        self.navigate_items(document_comment_paras, forward, "_shell.comment", cx);
    }

    /// 参考資料 > 注の表示 (DOCX-REF-018): from a note reference the caret goes to the note's text; from the text of a note, back to
    /// its reference in the body.
    pub(crate) fn show_note(&mut self, cx: &mut Context<Self>) {
        let Some(doc) = self.flow().cloned() else { return };
        let Some((_, caret)) = self.text_selection() else { return };
        // In a note: back to the reference.
        for (endnote, notes) in [(false, &doc.footnotes), (true, &doc.endnotes)] {
            for note in notes {
                if note.blocks.iter().any(|b| matches!(b, Block::Paragraph(p) if p.id == caret.para)) {
                    let found = doc.body.iter().find_map(|b| match b {
                        Block::Paragraph(p) => {
                            let mut offset = 0;
                            for r in &p.runs {
                                if matches!(r.kind, FlowRunKind::NoteRef { endnote: e, id } if e == endnote && id == note.id) {
                                    return Some(FlowPos::new(p.id, offset + 1));
                                }
                                offset += r.len();
                            }
                            None
                        }
                        _ => None,
                    });
                    if let Some(pos) = found {
                        self.set_caret(pos, false, cx);
                        self.scroll_to_caret(cx);
                    }
                    return;
                }
            }
        }
        // At a reference: to the note's first paragraph.
        let Some(Block::Paragraph(p)) = doc.body.iter().find(|b| matches!(b, Block::Paragraph(p) if p.id == caret.para)) else { return };
        let mut offset = 0;
        let mut target = None;
        for r in &p.runs {
            if let FlowRunKind::NoteRef { endnote, id } = r.kind {
                // The reference the caret is just after or at, else the first of the paragraph.
                if target.is_none() || offset < caret.offset {
                    target = Some((endnote, id));
                }
            }
            offset += r.len();
        }
        let Some((endnote, id)) = target else { return };
        let notes = if endnote { &doc.endnotes } else { &doc.footnotes };
        let first = notes.iter().find(|n| n.id == id).and_then(|n| n.blocks.iter().find_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }));
        if let Some(para) = first {
            self.jump_to(para, cx);
        }
    }
}
