// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Document accessibility checking (DOCX-ACCESS-001, DOCX-ACCESS-004,
//! DOCX-ACCESS-005, DOCX-ACCESS-006): scanning for missing alt text,
//! missing table headers, skipped heading levels, and ambiguous links.

use gpui_kit::component::{Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::*;
use crate::shell::palette::rgb;
use ibis_doc_model::{Block, FlowDocument, FlowPos, FlowRunKind, ShapeKind};
use crate::shell::{chrome::SidebarPanel, editor::SlideEditor, icons::Material};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IssueCategory {
    Error,
    Warning,
    Notice,
}

#[derive(Clone, Debug)]
pub(crate) struct AccessibilityIssue {
    pub(crate) at: FlowPos,
    pub(crate) category: IssueCategory,
    pub(crate) title: String,
    pub(crate) description: String,
    pub(crate) target_name: String,
}

/// Scan document for accessibility issues.
pub(crate) fn check_document_accessibility(doc: &FlowDocument) -> Vec<AccessibilityIssue> {
    let mut issues = Vec::new();

    // 1. Check headings sequence (DOCX-ACCESS-004: 見出し階層)
    let mut prev_heading_level = 0u32;
    for b in &doc.body {
        if let Block::Paragraph(p) = b {
            if let Some(level) = ibis_doc_model::flow_command::heading_level(&doc.styles, p) {
                if level > prev_heading_level + 1 && prev_heading_level > 0 {
                    let text = p.text();
                    let preview = if text.len() > 20 { format!("{}...", &text[..text.floor_char_boundary(20)]) } else { text };
                    issues.push(AccessibilityIssue {
                        at: FlowPos::new(p.id, 0),
                        category: IssueCategory::Warning,
                        title: crate::shell::i18n::text("docx.accessibility.heading-skip").to_string(),
                        description: crate::shell::i18n::format(
                            "_docx.heading-level-skipped-from-prev-to-curr-title",
                            &[("prev", &prev_heading_level.to_string()), ("curr", &level.to_string()), ("title", &preview)],
                        ).to_string(),
                        target_name: crate::shell::i18n::format("_docx.heading-level", &[("level", &level)]),
                    });
                }
                prev_heading_level = level;
            }
        }
    }

    // 2. Scan all blocks (body and table cells) for pictures, tables, and links
    fn scan_blocks(blocks: &[Block], doc: &FlowDocument, issues: &mut Vec<AccessibilityIssue>) {
        let _ = doc;
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    let mut offset = 0;
                    for r in &p.runs {
                        match &r.kind {
                            FlowRunKind::Picture { look, .. } => {
                                // DOCX-ACCESS-002, DOCX-ACCESS-003: Check missing alt text
                                if !look.decorative && look.alt_text.title.trim().is_empty() && look.alt_text.description.trim().is_empty() {
                                    let name = if look.name.is_empty() { crate::shell::i18n::text("docx.common.figure").to_string() } else { look.name.clone() };
                                    issues.push(AccessibilityIssue {
                                        at: FlowPos::new(p.id, offset),
                                        category: IssueCategory::Error,
                                        title: crate::shell::i18n::text("docx.accessibility.missing-alt").to_string(),
                                        description: crate::shell::i18n::text("docx.accessibility.missing-alt.desc").to_string(),
                                        target_name: name,
                                    });
                                }
                            }
                            FlowRunKind::Shape(shape) => {
                                if let ShapeKind::Picture(pic) = &shape.kind {
                                    if !pic.decorative && pic.alt_text.title.trim().is_empty() && pic.alt_text.description.trim().is_empty() {
                                        let name = if pic.name.is_empty() { crate::shell::i18n::text("docx.common.figure").to_string() } else { pic.name.clone() };
                                        issues.push(AccessibilityIssue {
                                            at: FlowPos::new(p.id, offset),
                                            category: IssueCategory::Error,
                                            title: crate::shell::i18n::text("docx.accessibility.missing-alt").to_string(),
                                            description: crate::shell::i18n::text("docx.accessibility.missing-alt.desc").to_string(),
                                            target_name: name,
                                        });
                                    }
                                }
                            }
                            FlowRunKind::Text(text) => {
                                // DOCX-ACCESS-006: Check ambiguous links
                                if r.link.is_some() {
                                    let trimmed = text.trim();
                                    let lower = trimmed.to_lowercase();
                                    let is_url = lower.starts_with("http://") || lower.starts_with("https://");
                                    let is_ambiguous = is_url
                                        || trimmed == "ここをクリック" // i18n-ok（照合の語（リンク文字列の判定に使う））
                                        || trimmed == "クリック" // i18n-ok（照合の語（リンク文字列の判定に使う））
                                        || trimmed == "リンク" // i18n-ok（照合の語（リンク文字列の判定に使う））
                                        || lower == "click here"
                                        || lower == "link"
                                        || lower == "here";
                                    if is_ambiguous {
                                        issues.push(AccessibilityIssue {
                                            at: FlowPos::new(p.id, offset),
                                            category: IssueCategory::Notice,
                                            title: crate::shell::i18n::text("docx.accessibility.ambiguous-link").to_string(),
                                            description: crate::shell::i18n::format(
                                                "_docx.unclear-link-text-text-use-descriptive-text-that",
                                                &[("text", &trimmed)],
                                            ).to_string(),
                                            target_name: trimmed.to_string(),
                                        });
                                    }
                                }
                            }
                            _ => {}
                        }
                        offset += r.len();
                    }
                }
                Block::Table(table) => {
                    // DOCX-ACCESS-005: Table missing header row
                    let has_header = table.rows.first().is_some_and(|r| r.header);
                    if !has_header && !table.rows.is_empty() {
                        let first_pos = table.rows[0].cells.first()
                            .and_then(|c| c.blocks.first())
                            .and_then(|b| if let Block::Paragraph(p) = b { Some(FlowPos::new(p.id, 0)) } else { None })
                            .unwrap_or(FlowPos::new(table.id, 0));
                        issues.push(AccessibilityIssue {
                            at: first_pos,
                            category: IssueCategory::Warning,
                            title: crate::shell::i18n::text("docx.accessibility.table-header").to_string(),
                            description: crate::shell::i18n::text("docx.accessibility.table-header.desc").to_string(),
                            target_name: crate::shell::i18n::format("_docx.table-rows-rows-columns-columns", &[("rows", &table.rows.len()), ("columns", &table.columns.len())]),
                        });
                    }
                    // Scan cells recursively
                    for row in &table.rows {
                        for cell in &row.cells {
                            scan_blocks(&cell.blocks, doc, issues);
                        }
                    }
                }
            }
        }
    }

    scan_blocks(&doc.body, doc, &mut issues);
    issues
}

impl SlideEditor {
    /// Open Accessibility Checker pane.
    pub(crate) fn open_accessibility_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if let Some(state) = self.doc_state_mut() {
            state.accessibility_panel = true;
            state.selection_pane = false;
            state.picture_edit = None;
            state.table_size = None;
            state.font_edit = None;
            state.color_edit = None;
            state.paragraph_edit = None;
            state.style_edit = None;
            state.link = None;
            state.find = None;
            state.comments = None;
        }
        self.sidebar_open = true;
        cx.notify();
    }

    /// Returns current accessibility issues in the document.
    #[allow(dead_code)]
    pub(crate) fn accessibility_issues(&self) -> Vec<AccessibilityIssue> {
        let Some(flow) = self.flow() else { return Vec::new(); };
        check_document_accessibility(flow)
    }

    /// Jump to accessibility issue location and select target.
    pub(crate) fn jump_to_accessibility_issue(&mut self, at: FlowPos, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            state.anchor = Some(at);
            state.caret = Some(FlowPos::new(at.para, at.offset + 1));
        }
        cx.notify();
    }

    /// Accessibility sidebar panel UI.
    pub(crate) fn accessibility_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        let is_open = self.doc_state().is_some_and(|s| s.accessibility_panel);
        if !is_open { return None; }
        let flow = self.flow()?;
        let issues = check_document_accessibility(flow);

        let mut body = v_flex().w_full().min_w_0().gap_3().px(px(18.)).py(px(8.)).p_0();

        if issues.is_empty() {
            body = body.child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(0xe6f4ea))
                    .child(Icon::new(Material("check_circle")).size(px(20.)).flex_none().text_color(rgb(0x137333)))
                    .child(
                        v_flex().flex_1().min_w_0()
                            .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(rgb(0x137333)).child(crate::shell::i18n::text("docx.accessibility.passed")))
                            .child(div().text_sm().text_color(rgb(0x1e8e3e)).child(crate::shell::i18n::text("_docx.no-accessibility-issues-were-found-in-the-docume")))
                    )
            );
        } else {
            let error_count = issues.iter().filter(|i| i.category == IssueCategory::Error).count();
            let warn_count = issues.iter().filter(|i| i.category == IssueCategory::Warning).count();
            let notice_count = issues.iter().filter(|i| i.category == IssueCategory::Notice).count();

            body = body.child(
                div().text_sm().text_color(rgb(0x5f6368)).child(
                    crate::shell::i18n::format(
                        "_docx.check-results-errors-errors-warnings-warnings-no",
                        &[("errors", &error_count.to_string()), ("warnings", &warn_count.to_string()), ("notices", &notice_count.to_string())],
                    )
                )
            );

            for (idx, issue) in issues.into_iter().enumerate() {
                let at = issue.at;
                let (icon_name, icon_color, bg_color) = match issue.category {
                    IssueCategory::Error => ("error", rgb(0xd93025), rgb(0xfce8e6)),
                    IssueCategory::Warning => ("warning", rgb(0xf29900), rgb(0xfef7e0)),
                    IssueCategory::Notice => ("info", rgb(0x1a73e8), rgb(0xe8f0fe)),
                };

                let item = v_flex().w_full().min_w_0()
                    .id(("acc-issue", idx))
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(bg_color)
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(0xf1f3f4)))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center().flex_wrap()
                            .child(Icon::new(Material(icon_name)).size(px(16.)).text_color(icon_color))
                            .child(div().flex_1().min_w_0().text_sm().font_weight(FontWeight::MEDIUM).child(issue.title))
                            .child(div().text_sm().text_color(rgb(0x5f6368)).child(issue.target_name))
                    )
                    .child(
                        div().w_full().min_w_0().text_sm().text_color(rgb(0x3c4043)).pl(px(24.)).child(issue.description)
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.jump_to_accessibility_issue(at, cx);
                    }));

                body = body.child(item);
            }
        }

        Some(SidebarPanel {
            title: crate::shell::i18n::text("docx.accessibility.title"),
            body: body.into_any_element(),
        })
    }
}

// =============================================================================
// DOCX-ACCESS-007: スクリーンリーダーの操作 (Screen Reader Navigation & Announcements)
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum ScreenReaderLandmarkKind {
    #[default]
    All,
    Heading,
    Table,
    Graphic,
    Link,
    List,
    Note,
    Comment,
    Field,
}

impl ScreenReaderLandmarkKind {
    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::All => "docx.screen-reader.filter.all",
            Self::Heading => "docx.screen-reader.filter.headings",
            Self::Table => "docx.screen-reader.filter.tables",
            Self::Graphic => "docx.screen-reader.filter.graphics",
            Self::Link => "docx.screen-reader.filter.links",
            Self::List => "docx.screen-reader.filter.lists",
            Self::Note => "docx.screen-reader.filter.footnotes",
            Self::Comment => "docx.screen-reader.filter.comments",
            Self::Field => "docx.screen-reader.filter.fields",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScreenReaderLandmark {
    pub(crate) kind: ScreenReaderLandmarkKind,
    pub(crate) at: FlowPos,
    pub(crate) label: String,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct DocxScreenReaderState {
    pub(crate) open: bool,
    pub(crate) filter: ScreenReaderLandmarkKind,
    pub(crate) last_announcement: String,
}

fn collect_screen_reader_landmarks(doc: &FlowDocument, filter: Option<ScreenReaderLandmarkKind>) -> Vec<ScreenReaderLandmark> {
    let mut out = Vec::new();
    let matches_filter = |kind: ScreenReaderLandmarkKind| match filter {
        None | Some(ScreenReaderLandmarkKind::All) => true,
        Some(f) => f == kind,
    };

    fn scan(blocks: &[Block], doc: &FlowDocument, matches_filter: &dyn Fn(ScreenReaderLandmarkKind) -> bool, out: &mut Vec<ScreenReaderLandmark>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    let text = p.text();
                    let trimmed = text.trim();
                    if let Some(level) = ibis_doc_model::flow_command::heading_level(&doc.styles, p) {
                        if matches_filter(ScreenReaderLandmarkKind::Heading) {
                            out.push(ScreenReaderLandmark {
                                kind: ScreenReaderLandmarkKind::Heading,
                                at: FlowPos::new(p.id, 0),
                                label: format!("Heading level {level}: {trimmed}"), // i18n-ok（スクリーンリーダーのランドマーク案内）
                                detail: format!("H{level}"), // i18n-ok
                            });
                        }
                    }
                    if p.list.is_some() && matches_filter(ScreenReaderLandmarkKind::List) {
                        let lvl = p.list.as_ref().map(|l| l.level + 1).unwrap_or(1);
                        out.push(ScreenReaderLandmark {
                            kind: ScreenReaderLandmarkKind::List,
                            at: FlowPos::new(p.id, 0),
                            label: format!("List item level {lvl}: {trimmed}"), // i18n-ok
                            detail: format!("L{lvl}"), // i18n-ok
                        });
                    }
                    let mut offset = 0usize;
                    for r in &p.runs {
                        if let Some(link) = &r.link {
                            if matches_filter(ScreenReaderLandmarkKind::Link) {
                                let r_text = match &r.kind {
                                    FlowRunKind::Text(s) => s.as_str(),
                                    _ => "",
                                };
                                let link_target = match link {
                                    ibis_doc_model::Link::Url(u) => u.clone(),
                                    ibis_doc_model::Link::Anchor(a) => format!("#{a}"), // i18n-ok
                                };
                                out.push(ScreenReaderLandmark {
                                    kind: ScreenReaderLandmarkKind::Link,
                                    at: FlowPos::new(p.id, offset),
                                    label: format!("Link: {r_text}"), // i18n-ok
                                    detail: link_target,
                                });
                            }
                        }
                        match &r.kind {
                            FlowRunKind::CommentStart(cid) | FlowRunKind::CommentRef(cid) => {
                                if matches_filter(ScreenReaderLandmarkKind::Comment) {
                                    let comment_txt = doc.comments.iter().find(|c| c.id == *cid).map(|c| {
                                        let body_txt: String = c.blocks.iter().filter_map(|cb| {
                                            if let Block::Paragraph(cp) = cb { Some(cp.text()) } else { None }
                                        }).collect::<Vec<_>>().join(" ");
                                        format!("{}: {}", c.author, body_txt)
                                    }).unwrap_or_else(|| format!("Comment #{cid}")); // i18n-ok
                                    out.push(ScreenReaderLandmark {
                                        kind: ScreenReaderLandmarkKind::Comment,
                                        at: FlowPos::new(p.id, offset),
                                        label: format!("Comment: {comment_txt}"), // i18n-ok
                                        detail: format!("#{cid}"), // i18n-ok
                                    });
                                }
                            }
                            FlowRunKind::Picture { look, .. } => {
                                if matches_filter(ScreenReaderLandmarkKind::Graphic) {
                                    let alt = if look.decorative {
                                        "Decorative image".to_string() // i18n-ok
                                    } else if !look.alt_text.description.trim().is_empty() {
                                        look.alt_text.description.trim().to_string()
                                    } else if !look.alt_text.title.trim().is_empty() {
                                        look.alt_text.title.trim().to_string()
                                    } else if !look.name.trim().is_empty() {
                                        look.name.trim().to_string()
                                    } else {
                                        "Unlabeled image".to_string() // i18n-ok
                                    };
                                    out.push(ScreenReaderLandmark {
                                        kind: ScreenReaderLandmarkKind::Graphic,
                                        at: FlowPos::new(p.id, offset),
                                        label: format!("Image: {alt}"), // i18n-ok
                                        detail: if look.decorative { "decorative".into() } else { "graphic".into() }, // i18n-ok
                                    });
                                }
                            }
                            FlowRunKind::Shape(shape) => {
                                if matches_filter(ScreenReaderLandmarkKind::Graphic) {
                                    let name = match &shape.kind {
                                        ShapeKind::Picture(pic) if !pic.name.trim().is_empty() => pic.name.clone(),
                                        _ => "Shape".to_string(), // i18n-ok
                                    };
                                    out.push(ScreenReaderLandmark {
                                        kind: ScreenReaderLandmarkKind::Graphic,
                                        at: FlowPos::new(p.id, offset),
                                        label: format!("Shape: {name}"), // i18n-ok
                                        detail: "shape".into(), // i18n-ok
                                    });
                                }
                            }
                            FlowRunKind::NoteRef { endnote, id } => {
                                if matches_filter(ScreenReaderLandmarkKind::Note) {
                                    let notes = if *endnote { &doc.endnotes } else { &doc.footnotes };
                                    let note_txt = notes.iter().find(|n| n.id == *id).map(|n| {
                                        n.blocks.iter().filter_map(|nb| {
                                            if let Block::Paragraph(np) = nb { Some(np.text()) } else { None }
                                        }).collect::<Vec<_>>().join(" ")
                                    }).unwrap_or_default();
                                    let kind_name = if *endnote { "Endnote" } else { "Footnote" }; // i18n-ok
                                    out.push(ScreenReaderLandmark {
                                        kind: ScreenReaderLandmarkKind::Note,
                                        at: FlowPos::new(p.id, offset),
                                        label: format!("{kind_name} {id}: {note_txt}"), // i18n-ok
                                        detail: format!("#{id}"), // i18n-ok
                                    });
                                }
                            }
                            FlowRunKind::Field { instruction, shown } => {
                                if matches_filter(ScreenReaderLandmarkKind::Field) {
                                    out.push(ScreenReaderLandmark {
                                        kind: ScreenReaderLandmarkKind::Field,
                                        at: FlowPos::new(p.id, offset),
                                        label: format!("Field ({}): {}", instruction.trim(), shown), // i18n-ok
                                        detail: instruction.trim().to_string(),
                                    });
                                }
                            }
                            _ => {}
                        }
                        offset += r.len();
                    }
                }
                Block::Table(table) => {
                    if matches_filter(ScreenReaderLandmarkKind::Table) {
                        let first_pos = table.rows.first()
                            .and_then(|r| r.cells.first())
                            .and_then(|c| c.blocks.first())
                            .and_then(|b| if let Block::Paragraph(p) = b { Some(FlowPos::new(p.id, 0)) } else { None })
                            .unwrap_or(FlowPos::new(table.id, 0));
                        let has_hdr = table.rows.first().is_some_and(|r| r.header);
                        out.push(ScreenReaderLandmark {
                            kind: ScreenReaderLandmarkKind::Table,
                            at: first_pos,
                            label: format!("Table ({} rows, {} columns{})", table.rows.len(), table.columns.len(), if has_hdr { ", with header row" } else { "" }), // i18n-ok
                            detail: format!("{}x{}", table.rows.len(), table.columns.len()), // i18n-ok
                        });
                    }
                    for row in &table.rows {
                        for cell in &row.cells {
                            scan(&cell.blocks, doc, matches_filter, out);
                        }
                    }
                }
            }
        }
    }

    scan(&doc.body, doc, &matches_filter, &mut out);
    out
}

impl SlideEditor {
    pub(crate) fn open_docx_screen_reader_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if let Some(state) = self.doc_state_mut() {
            let mut sr = state.screen_reader.take().unwrap_or_default();
            sr.open = true;
            state.screen_reader = Some(sr);
        }
        let _ = self.docx_screen_reader_announce_at_caret();
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_docx_screen_reader_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            if let Some(sr) = &mut state.screen_reader {
                sr.open = false;
            }
        }
        cx.notify();
    }

    pub(crate) fn is_docx_screen_reader_open(&self) -> bool {
        self.doc_state().and_then(|s| s.screen_reader.as_ref()).is_some_and(|sr| sr.open)
    }

    pub(crate) fn set_docx_screen_reader_filter(&mut self, filter: ScreenReaderLandmarkKind, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let mut sr = state.screen_reader.take().unwrap_or_default();
            sr.filter = filter;
            state.screen_reader = Some(sr);
        }
        cx.notify();
    }

    pub(crate) fn docx_screen_reader_landmarks(&self, filter: Option<ScreenReaderLandmarkKind>) -> Vec<ScreenReaderLandmark> {
        let Some(flow) = self.flow() else { return Vec::new(); };
        collect_screen_reader_landmarks(flow, filter)
    }

    /// Generate and store the screen reader announcement describing the current caret position.
    pub(crate) fn docx_screen_reader_announce_at_caret(&mut self) -> String {
        let caret = self.doc_state().and_then(|s| s.caret);
        let Some(flow) = self.flow() else {
            return String::new();
        };
        let Some(caret) = caret else {
            return "Document".to_string(); // i18n-ok
        };

        let mut parts: Vec<String> = Vec::new();

        // Check if inside a table cell
        for (t_idx, b) in flow.body.iter().enumerate() {
            if let Block::Table(table) = b {
                for (r_idx, row) in table.rows.iter().enumerate() {
                    for (c_idx, cell) in row.cells.iter().enumerate() {
                        if cell.blocks.iter().any(|cb| matches!(cb, Block::Paragraph(p) if p.id == caret.para)) {
                            parts.push(format!(
                                "Table {}, Row {} of {}, Column {} of {}{}",
                                t_idx + 1,
                                r_idx + 1,
                                table.rows.len(),
                                c_idx + 1,
                                row.cells.len(),
                                if row.header { " (Header row)" } else { "" } // i18n-ok
                            ));
                        }
                    }
                }
            }
        }

        // Find paragraph
        if let Some(p) = ibis_doc_model::all_paragraphs(flow).into_iter().find(|p| p.id == caret.para) {
            if let Some(level) = ibis_doc_model::flow_command::heading_level(&flow.styles, p) {
                parts.push(format!("Heading level {level}")); // i18n-ok
            }
            if let Some(list) = &p.list {
                parts.push(format!("List item level {}", list.level + 1)); // i18n-ok
            }
            // Check run at caret offset
            let mut off = 0usize;
            for r in &p.runs {
                let len = r.len();
                if caret.offset >= off && caret.offset <= off + len {
                    if let Some(link) = &r.link {
                        let link_target = match link {
                            ibis_doc_model::Link::Url(u) => u.as_str(),
                            ibis_doc_model::Link::Anchor(a) => a.as_str(),
                        };
                        parts.push(format!("Link to {link_target}")); // i18n-ok
                    }
                    match &r.kind {
                        FlowRunKind::Picture { look, .. } => {
                            if look.decorative {
                                parts.push("Decorative image".to_string()); // i18n-ok
                            } else if !look.alt_text.description.trim().is_empty() {
                                parts.push(format!("Image: {}", look.alt_text.description.trim())); // i18n-ok
                            } else if !look.alt_text.title.trim().is_empty() {
                                parts.push(format!("Image: {}", look.alt_text.title.trim())); // i18n-ok
                            } else {
                                parts.push("Image".to_string()); // i18n-ok
                            }
                        }
                        FlowRunKind::Math { math, display } => {
                            let linear = math.linear();
                            parts.push(format!("{} equation: {}", if *display { "Display" } else { "Inline" }, linear)); // i18n-ok
                        }
                        FlowRunKind::Field { instruction, shown } => {
                            parts.push(format!("Field {}: {}", instruction.trim(), shown)); // i18n-ok
                        }
                        _ => {}
                    }
                    break;
                }
                off += len;
            }
            let text = p.text();
            if !text.trim().is_empty() {
                parts.push(text.trim().to_string());
            } else if parts.is_empty() {
                parts.push("Blank paragraph".to_string()); // i18n-ok
            }
        }

        let announcement = parts.join(", ");
        if let Some(state) = self.doc_state_mut() {
            let mut sr = state.screen_reader.take().unwrap_or_default();
            sr.last_announcement = announcement.clone();
            state.screen_reader = Some(sr);
        }
        announcement
    }

    pub(crate) fn docx_screen_reader_next_landmark(
        &mut self,
        kind: Option<ScreenReaderLandmarkKind>,
        cx: &mut Context<Self>,
    ) -> Option<ScreenReaderLandmark> {
        let filter = kind.or_else(|| self.doc_state().and_then(|s| s.screen_reader.as_ref().map(|sr| sr.filter)));
        let landmarks = self.docx_screen_reader_landmarks(filter);
        if landmarks.is_empty() {
            return None;
        }
        let caret = self.doc_state().and_then(|s| s.caret);
        let para_order: Vec<u32> = self.flow().map(|f| ibis_doc_model::all_paragraphs(f).into_iter().map(|p| p.id).collect()).unwrap_or_default();
        let rank = |pos: FlowPos| -> (usize, usize) {
            let p_idx = para_order.iter().position(|&id| id == pos.para).unwrap_or(0);
            (p_idx, pos.offset)
        };
        let next = if let Some(cur) = caret {
            let cur_rank = rank(cur);
            landmarks.iter().find(|lm| rank(lm.at) > cur_rank).cloned().or_else(|| landmarks.first().cloned())
        } else {
            landmarks.first().cloned()
        }?;

        if let Some(state) = self.doc_state_mut() {
            state.caret = Some(next.at);
            state.anchor = None;
            let mut sr = state.screen_reader.take().unwrap_or_default();
            sr.last_announcement = next.label.clone();
            state.screen_reader = Some(sr);
        }
        cx.notify();
        Some(next)
    }

    pub(crate) fn docx_screen_reader_prev_landmark(
        &mut self,
        kind: Option<ScreenReaderLandmarkKind>,
        cx: &mut Context<Self>,
    ) -> Option<ScreenReaderLandmark> {
        let filter = kind.or_else(|| self.doc_state().and_then(|s| s.screen_reader.as_ref().map(|sr| sr.filter)));
        let landmarks = self.docx_screen_reader_landmarks(filter);
        if landmarks.is_empty() {
            return None;
        }
        let caret = self.doc_state().and_then(|s| s.caret);
        let para_order: Vec<u32> = self.flow().map(|f| ibis_doc_model::all_paragraphs(f).into_iter().map(|p| p.id).collect()).unwrap_or_default();
        let rank = |pos: FlowPos| -> (usize, usize) {
            let p_idx = para_order.iter().position(|&id| id == pos.para).unwrap_or(0);
            (p_idx, pos.offset)
        };
        let prev = if let Some(cur) = caret {
            let cur_rank = rank(cur);
            landmarks.iter().rev().find(|lm| rank(lm.at) < cur_rank).cloned().or_else(|| landmarks.last().cloned())
        } else {
            landmarks.last().cloned()
        }?;

        if let Some(state) = self.doc_state_mut() {
            state.caret = Some(prev.at);
            state.anchor = None;
            let mut sr = state.screen_reader.take().unwrap_or_default();
            sr.last_announcement = prev.label.clone();
            state.screen_reader = Some(sr);
        }
        cx.notify();
        Some(prev)
    }

    pub(crate) fn docx_screen_reader_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        use gpui_kit::component::button::{Button, ButtonVariants as _};
        let sr = self.doc_state().and_then(|s| s.screen_reader.clone())?;
        if !sr.open {
            return None;
        }
        let landmarks = self.docx_screen_reader_landmarks(Some(sr.filter));
        let announcement = if sr.last_announcement.is_empty() {
            self.docx_screen_reader_announce_at_caret()
        } else {
            sr.last_announcement.clone()
        };

        let filters = [
            ScreenReaderLandmarkKind::All,
            ScreenReaderLandmarkKind::Heading,
            ScreenReaderLandmarkKind::Table,
            ScreenReaderLandmarkKind::Graphic,
            ScreenReaderLandmarkKind::Link,
            ScreenReaderLandmarkKind::List,
        ];
        let mut filter_row = h_flex().gap_1().flex_wrap();
        for f in filters {
            let active = sr.filter == f;
            filter_row = filter_row.child(crate::shell::chrome::radio_button(
                SharedString::from(format!("sr-filter-{f:?}")),active,false,crate::shell::i18n::text(f.label_key()),
                cx.listener(move |e,_,_,cx|e.set_docx_screen_reader_filter(f,cx))));
        }

        let mut list = v_flex().gap_1();
        for (idx, lm) in landmarks.into_iter().enumerate() {
            let at = lm.at;
            let lbl = lm.label.clone();
            list = list.child(
                h_flex()
                    .id(("sr-lm", idx))
                    .p_2()
                    .rounded_md()
                    .bg(rgb(0xf8f9fa))
                    .hover(|s| s.bg(rgb(0xe8f0fe)))
                    .cursor_pointer()
                    .justify_between()
                    .min_w_0().overflow_hidden().child(div().flex_1().min_w_0().truncate().text_sm().font_weight(FontWeight::MEDIUM).child(lm.label))
                    .child(div().max_w(px(100.)).truncate().text_sm().text_color(rgb(0x5f6368)).child(lm.detail))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(st) = this.doc_state_mut() {
                            st.caret = Some(at);
                            st.anchor = None;
                            if let Some(sr) = &mut st.screen_reader {
                                sr.last_announcement = lbl.clone();
                            }
                        }
                        cx.notify();
                    })),
            );
        }

        let body = v_flex()
            .gap_3()
            .p_3()
            .p_0().child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(0xe8f0fe))
                    .child(div().text_sm().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("docx.screen-reader.current")))
                    .overflow_hidden().child(div().w_full().text_sm().overflow_hidden().child(announcement)),
            )
            .child(
                crate::shell::chrome::action_bar()
                    .action(
                        crate::shell::chrome::ActionDefinition::new("sr-next-btn", crate::shell::i18n::text("docx.screen-reader.next"), crate::shell::chrome::ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.docx_screen_reader_next_landmark(None, cx);
                            })),
                    )
                    .action(
                        crate::shell::chrome::ActionDefinition::new("sr-prev-btn", crate::shell::i18n::text("docx.screen-reader.prev"), crate::shell::chrome::ActionButtonKind::Normal)
                            .on_click(cx.listener(|this, _, _, cx| {
                                let _ = this.docx_screen_reader_prev_landmark(None, cx);
                            })),
                    ),
            )
            .child(filter_row)
            .child(list)
            ;

        let panel_terminal_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("sr-close-btn", crate::shell::i18n::text("docx.convert.close"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_docx_screen_reader_panel(cx);
                    })));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("docx.screen-reader.title"), body.into_any_element(), panel_terminal_actions))
    }
}

// =============================================================================
// DOCX-ACCESS-009: 読み上げ (Read Aloud / Text-to-Speech)
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum ReadAloudVoice {
    #[default]
    SystemDefault,
    KyokoJa,
    OtoyaJa,
    SamanthaEn,
    AlexEn,
}

impl ReadAloudVoice {
    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::SystemDefault => "docx.read-aloud.voice.default",
            Self::KyokoJa => "docx.read-aloud.voice.kyoko",
            Self::OtoyaJa => "docx.read-aloud.voice.otoya",
            Self::SamanthaEn => "docx.read-aloud.voice.samantha",
            Self::AlexEn => "docx.read-aloud.voice.alex",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DocxReadAloudState {
    pub(crate) open: bool,
    pub(crate) playing: bool,
    pub(crate) speed: f32,
    pub(crate) voice: ReadAloudVoice,
    pub(crate) current_unit_index: usize,
    pub(crate) spoken_text: String,
}

impl Default for DocxReadAloudState {
    fn default() -> Self {
        Self {
            open: false,
            playing: false,
            speed: 1.0,
            voice: ReadAloudVoice::SystemDefault,
            current_unit_index: 0,
            spoken_text: String::new(),
        }
    }
}

fn collect_read_aloud_units(doc: &FlowDocument) -> Vec<(u32, usize, String)> {
    let mut units = Vec::new();
    for p in ibis_doc_model::all_paragraphs(doc) {
        let txt = p.text();
        if !txt.trim().is_empty() {
            units.push((p.id, p.len(), txt));
        }
    }
    units
}

impl SlideEditor {
    pub(crate) fn open_docx_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        let units = self.flow().map(|d| collect_read_aloud_units(d)).unwrap_or_default();
        let caret_para = self.doc_state().and_then(|s| s.caret.map(|c| c.para));
        let start_idx = caret_para
            .and_then(|pid| units.iter().position(|(id, _, _)| *id == pid))
            .unwrap_or(0);
        let (target_pos, target_end, spoken) = units
            .get(start_idx)
            .map(|(id, len, txt)| (Some(FlowPos::new(*id, 0)), Some(FlowPos::new(*id, *len)), txt.clone()))
            .unwrap_or((None, None, String::new()));

        if let Some(state) = self.doc_state_mut() {
            let mut ra = state.read_aloud.take().unwrap_or_default();
            ra.open = true;
            ra.playing = true;
            ra.current_unit_index = start_idx;
            ra.spoken_text = spoken;
            state.read_aloud = Some(ra);
            if let (Some(anchor), Some(caret)) = (target_pos, target_end) {
                state.anchor = Some(anchor);
                state.caret = Some(caret);
            }
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_docx_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            if let Some(ra) = &mut state.read_aloud {
                ra.open = false;
                ra.playing = false;
            }
        }
        cx.notify();
    }

    pub(crate) fn is_docx_read_aloud_open(&self) -> bool {
        self.doc_state().and_then(|s| s.read_aloud.as_ref()).is_some_and(|ra| ra.open)
    }

    pub(crate) fn is_docx_read_aloud_playing(&self) -> bool {
        self.doc_state().and_then(|s| s.read_aloud.as_ref()).is_some_and(|ra| ra.open && ra.playing)
    }

    pub(crate) fn docx_read_aloud_toggle_play(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            if let Some(ra) = &mut state.read_aloud {
                ra.playing = !ra.playing;
            }
        }
        cx.notify();
    }

    pub(crate) fn docx_read_aloud_next(&mut self, cx: &mut Context<Self>) -> bool {
        let units = self.flow().map(|d| collect_read_aloud_units(d)).unwrap_or_default();
        if units.is_empty() {
            return false;
        }
        let cur = self.doc_state().and_then(|s| s.read_aloud.as_ref().map(|ra| ra.current_unit_index)).unwrap_or(0);
        if cur + 1 >= units.len() {
            if let Some(state) = self.doc_state_mut() {
                if let Some(ra) = &mut state.read_aloud {
                    ra.playing = false;
                }
            }
            cx.notify();
            return false;
        }
        let next_idx = cur + 1;
        let (pid, len, ref txt) = units[next_idx];
        if let Some(state) = self.doc_state_mut() {
            if let Some(ra) = &mut state.read_aloud {
                ra.current_unit_index = next_idx;
                ra.spoken_text = txt.clone();
            }
            state.anchor = Some(FlowPos::new(pid, 0));
            state.caret = Some(FlowPos::new(pid, len));
        }
        cx.notify();
        true
    }

    pub(crate) fn docx_read_aloud_prev(&mut self, cx: &mut Context<Self>) -> bool {
        let units = self.flow().map(|d| collect_read_aloud_units(d)).unwrap_or_default();
        if units.is_empty() {
            return false;
        }
        let cur = self.doc_state().and_then(|s| s.read_aloud.as_ref().map(|ra| ra.current_unit_index)).unwrap_or(0);
        if cur == 0 {
            return false;
        }
        let prev_idx = cur - 1;
        let (pid, len, ref txt) = units[prev_idx];
        if let Some(state) = self.doc_state_mut() {
            if let Some(ra) = &mut state.read_aloud {
                ra.current_unit_index = prev_idx;
                ra.spoken_text = txt.clone();
            }
            state.anchor = Some(FlowPos::new(pid, 0));
            state.caret = Some(FlowPos::new(pid, len));
        }
        cx.notify();
        true
    }

    pub(crate) fn docx_read_aloud_set_speed(&mut self, speed: f32, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let mut ra = state.read_aloud.take().unwrap_or_default();
            ra.speed = speed.clamp(0.5, 2.0);
            state.read_aloud = Some(ra);
        }
        cx.notify();
    }

    pub(crate) fn docx_read_aloud_set_voice(&mut self, voice: ReadAloudVoice, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let mut ra = state.read_aloud.take().unwrap_or_default();
            ra.voice = voice;
            state.read_aloud = Some(ra);
        }
        cx.notify();
    }

    pub(crate) fn docx_read_aloud_spoken_text(&self) -> Option<String> {
        self.doc_state().and_then(|s| s.read_aloud.as_ref()).filter(|ra| ra.open).map(|ra| ra.spoken_text.clone())
    }

    pub(crate) fn docx_read_aloud_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        use gpui_kit::component::button::{Button, ButtonVariants as _};
        let ra = self.doc_state().and_then(|s| s.read_aloud.clone())?;
        if !ra.open {
            return None;
        }
        let speeds = [0.5f32, 0.75, 1.0, 1.25, 1.5, 2.0];
        let speed_row = crate::shell::chrome::SegmentControlRow::new(crate::shell::chrome::segment_control::Mode::Intrinsic)
            .children(speeds.into_iter().map(|sp| crate::shell::chrome::segment_control::SegmentItem::label(
                SharedString::from(format!("ra-speed-{sp}")),format!("{sp}x"),(ra.speed-sp).abs()<0.01)
                .on_click(cx.listener(move |e,_,_,cx|e.docx_read_aloud_set_speed(sp,cx)))));

        let voices = [
            ReadAloudVoice::SystemDefault,
            ReadAloudVoice::KyokoJa,
            ReadAloudVoice::OtoyaJa,
            ReadAloudVoice::SamanthaEn,
            ReadAloudVoice::AlexEn,
        ];
        let mut voice_col = v_flex().gap_1();
        for v in voices {
            let active = ra.voice == v;
            voice_col = voice_col.child(crate::shell::chrome::radio_button(
                SharedString::from(format!("ra-voice-{v:?}")),active,false,crate::shell::i18n::text(v.label_key()),
                cx.listener(move |e,_,_,cx|e.docx_read_aloud_set_voice(v,cx))));
        }

        let body = v_flex()
            .gap_3()
            .p_3()
            .p_0().child(
                v_flex()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(rgb(0xf8f9fa))
                    .child(div().text_sm().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("docx.read-aloud.speaking")))
                    .child(div().text_sm().child(ra.spoken_text.clone())),
            )
            .child(
                crate::shell::chrome::action_bar()
                    .action(
                        crate::shell::chrome::ActionDefinition::new("ra-play-pause-btn", if ra.playing {
                                crate::shell::i18n::text("docx.read-aloud.pause")
                            } else {
                                crate::shell::i18n::text("docx.read-aloud.play")
                            }, crate::shell::chrome::ActionButtonKind::Primary)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.docx_read_aloud_toggle_play(cx);
                            })),
                    )
                    .action(
                        crate::shell::chrome::ActionDefinition::new("ra-prev-btn", crate::shell::i18n::text("docx.read-aloud.prev"), crate::shell::chrome::ActionButtonKind::Normal)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.docx_read_aloud_prev(cx);
                            })),
                    )
                    .action(
                        crate::shell::chrome::ActionDefinition::new("ra-next-btn", crate::shell::i18n::text("docx.read-aloud.next"), crate::shell::chrome::ActionButtonKind::Normal)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.docx_read_aloud_next(cx);
                            })),
                    ),
            )
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("docx.read-aloud.speed")))
            .child(speed_row)
            .child(div().text_sm().font_weight(FontWeight::SEMIBOLD).child(crate::shell::i18n::text("docx.read-aloud.voice")))
            .child(voice_col)
            ;

        let panel_terminal_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("ra-close-btn", crate::shell::i18n::text("docx.convert.close"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_docx_read_aloud(cx);
                    })));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("docx.read-aloud.title"), body.into_any_element(), panel_terminal_actions))
    }
}

// =============================================================================
// DOCX-CLOUD-020: リボンとクイックアクセスのカスタマイズ (Toolbar & QAT Customization)
// =============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DocxToolbarGroup {
    Styles,
    Font,
    Color,
    Insert,
    Paragraph,
    Review,
}

impl DocxToolbarGroup {
    pub(crate) const ALL: [Self; 6] = [
        Self::Styles,
        Self::Font,
        Self::Color,
        Self::Insert,
        Self::Paragraph,
        Self::Review,
    ];

    pub(crate) fn label_key(self) -> &'static str {
        match self {
            Self::Styles => "docx.customize.group.styles",
            Self::Font => "docx.customize.group.font",
            Self::Color => "docx.customize.group.color",
            Self::Insert => "docx.customize.group.insert",
            Self::Paragraph => "docx.customize.group.paragraph",
            Self::Review => "docx.customize.group.review",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DocxToolbarCustomizeState {
    pub(crate) open: bool,
    pub(crate) show_styles: bool,
    pub(crate) show_font: bool,
    pub(crate) show_color: bool,
    pub(crate) show_insert: bool,
    pub(crate) show_paragraph: bool,
    pub(crate) show_review: bool,
}

impl Default for DocxToolbarCustomizeState {
    fn default() -> Self {
        Self {
            open: false,
            show_styles: true,
            show_font: true,
            show_color: true,
            show_insert: true,
            show_paragraph: true,
            show_review: true,
        }
    }
}

impl SlideEditor {
    pub(crate) fn open_docx_customize_toolbar_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(s)=self.doc_state_mut(){s.tab_edit=None;}
        if let Some(state) = self.doc_state_mut() {
            let mut c = state.toolbar_customize.take().unwrap_or_default();
            c.open = true;
            state.toolbar_customize = Some(c);
        }
        self.sidebar_open = true;
        cx.notify();
    }

    pub(crate) fn close_docx_customize_toolbar_panel(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            if let Some(c) = &mut state.toolbar_customize {
                c.open = false;
            }
        }
        cx.notify();
    }

    pub(crate) fn is_docx_customize_toolbar_open(&self) -> bool {
        self.doc_state().and_then(|s| s.toolbar_customize.as_ref()).is_some_and(|c| c.open)
    }

    pub(crate) fn is_docx_toolbar_group_visible(&self, group: DocxToolbarGroup) -> bool {
        let Some(c) = self.doc_state().and_then(|s| s.toolbar_customize.as_ref()) else {
            return true;
        };
        match group {
            DocxToolbarGroup::Styles => c.show_styles,
            DocxToolbarGroup::Font => c.show_font,
            DocxToolbarGroup::Color => c.show_color,
            DocxToolbarGroup::Insert => c.show_insert,
            DocxToolbarGroup::Paragraph => c.show_paragraph,
            DocxToolbarGroup::Review => c.show_review,
        }
    }

    pub(crate) fn set_docx_toolbar_group_visible(&mut self, group: DocxToolbarGroup, visible: bool, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let mut c = state.toolbar_customize.take().unwrap_or_default();
            match group {
                DocxToolbarGroup::Styles => c.show_styles = visible,
                DocxToolbarGroup::Font => c.show_font = visible,
                DocxToolbarGroup::Color => c.show_color = visible,
                DocxToolbarGroup::Insert => c.show_insert = visible,
                DocxToolbarGroup::Paragraph => c.show_paragraph = visible,
                DocxToolbarGroup::Review => c.show_review = visible,
            }
            state.toolbar_customize = Some(c);
        }
        cx.notify();
    }

    pub(crate) fn toggle_docx_toolbar_group(&mut self, group: DocxToolbarGroup, cx: &mut Context<Self>) {
        let cur = self.is_docx_toolbar_group_visible(group);
        self.set_docx_toolbar_group_visible(group, !cur, cx);
    }

    pub(crate) fn reset_docx_toolbar_customization(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.doc_state_mut() {
            let open = state.toolbar_customize.as_ref().is_some_and(|c| c.open);
            state.toolbar_customize = Some(DocxToolbarCustomizeState {
                open,
                ..DocxToolbarCustomizeState::default()
            });
        }
        crate::shell::qat::reset();
        cx.notify();
    }

    pub(crate) fn docx_customize_toolbar_panel(&mut self, cx: &mut Context<Self>) -> Option<SidebarPanel> {
        use gpui_kit::component::button::{Button, ButtonVariants as _};
        let c = self.doc_state().and_then(|s| s.toolbar_customize.clone())?;
        if !c.open {
            return None;
        }

        let mut group_col = v_flex().gap_1();
        for g in DocxToolbarGroup::ALL {
            let on = self.is_docx_toolbar_group_visible(g);
            group_col = group_col.child(
                Button::new(SharedString::from(format!("cust-grp-{g:?}")))
                    .child(div().w_full().text_align(TextAlign::Left).child(crate::shell::i18n::text(g.label_key())))
                    .icon(Icon::new(Material(if on { "check_box" } else { "check_box_outline_blank" })))
                    .small()
                    .ghost()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.toggle_docx_toolbar_group(g, cx);
                    })),
            );
        }

        let qat_chosen = crate::shell::qat::commands();
        let docx_qat_candidates = [
            "file.save",
            "file.print",
            "file.export.pdf",
            "edit.find-replace",
            "docx.track.record",
            "docx.track.accept",
            "docx.track.reject",
            "docx.accessibility-check",
            "docx.read-aloud",
            "docx.inspect-document",
            "docx.compatibility-check",
            "docx.print-settings",
            "docx.file.convert-format",
        ];
        let mut qat_col = v_flex().gap_1();
        for &id in &docx_qat_candidates {
            let on = qat_chosen.iter().any(|x| x == id);
            qat_col = qat_col.child(
                Button::new(SharedString::from(format!("cust-qat-{id}")))
                    .child(div().w_full().text_align(TextAlign::Left).child(crate::shell::i18n::text(id)))
                    .icon(Icon::new(Material(if on { "check_box" } else { "check_box_outline_blank" })))
                    .small()
                    .ghost()
                    .on_click(cx.listener(move |_, _, _, cx| {
                        crate::shell::qat::toggle(id);
                        cx.notify();
                    })),
            );
        }

        let body = v_flex()
            .gap_3()
            .p_3()
            .p_0().child(div().text_sm().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("docx.customize.ribbon-groups")))
            .child(group_col)
            .child(div().text_sm().font_weight(FontWeight::BOLD).child(crate::shell::i18n::text("docx.customize.qat-commands")))
            .child(qat_col)
            ;

        let panel_terminal_actions = crate::shell::chrome::action_bar().action(crate::shell::chrome::ActionDefinition::new("cust-reset-btn", crate::shell::i18n::text("docx.customize.reset"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.reset_docx_toolbar_customization(cx);
                    }))).action(crate::shell::chrome::ActionDefinition::new("cust-close-btn", crate::shell::i18n::text("docx.convert.close"), crate::shell::chrome::ActionButtonKind::Normal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.close_docx_customize_toolbar_panel(cx);
                    })));
        Some(SidebarPanel::with_footer(crate::shell::i18n::text("docx.customize.title"), body.into_any_element(), panel_terminal_actions))
    }
}


