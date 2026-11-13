// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 表 > 文字列を表にする・表を文字列にする (Word's 表の変換): the selected
//! paragraphs of the body become the rows of a table, a tab making a new
//! cell; a table becomes paragraphs, a tab between the cells' texts.
use gpui_kit::*;
use ibis_doc_model::flow_command::{self, Container};
use ibis_doc_model::{Block, FlowBorder, FlowCell, FlowCommand, FlowParagraph, FlowRow, FlowRun, FlowRunKind, FlowTable, Rgb};

use crate::shell::editor::SlideEditor;
use crate::ext::*;

#[ibis_office_macros::editor_ext(TableConvert)]
impl SlideEditor {
    /// The body's paragraphs (indexes) the selection covers.
    fn convert_range(&self) -> Option<(usize, usize)> {
        let (a, b) = self.ordered_selection()?;
        let d = self.flow()?;
        let (ca, ia) = flow_command::locate(d, a.para)?;
        let (cb, ib) = flow_command::locate(d, b.para)?;
        if ca != Container::Body || cb != Container::Body {
            return None;
        }
        let list = flow_command::blocks(d, Container::Body)?;
        list.get(ia..=ib)?.iter().all(|x| matches!(x, Block::Paragraph(p) if p.section.is_none())).then_some((ia, ib))
    }

    pub(crate) fn can_text_to_table(&self) -> bool {
        self.convert_range().is_some() && self.table_at_caret().is_none()
    }

    /// The table at the caret when each cell holds one paragraph.
    fn convertible_table(&self) -> Option<&FlowTable> {
        let (id, _, _) = self.table_at_caret()?;
        let d = self.flow()?;
        let (container, index) = flow_command::locate(d, id)?;
        if container != Container::Body {
            return None;
        }
        let Block::Table(t) = flow_command::blocks(d, container)?.get(index)? else { return None };
        t.rows.iter().flat_map(|r| &r.cells).all(|c| c.blocks.len() == 1 && matches!(c.blocks[0], Block::Paragraph(_))).then_some(t)
    }

    pub(crate) fn can_table_to_text(&self) -> bool {
        self.convertible_table().is_some()
    }

    pub(crate) fn text_to_table(&mut self, cx: &mut Context<Self>) {
        let Some((first, last)) = self.convert_range() else { return };
        let Some(doc) = self.flow() else { return };
        let Some(Block::Paragraph(_)) = flow_command::blocks(doc, Container::Body).and_then(|l| l.get(first)) else { return };
        let paragraphs: Vec<FlowParagraph> = flow_command::blocks(doc, Container::Body).unwrap()[first..=last].iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.clone()) } else { None }).collect();
        // Each paragraph's texts between tabs.
        let rows: Vec<Vec<Vec<FlowRun>>> = paragraphs
            .iter()
            .map(|p| {
                let mut cells: Vec<Vec<FlowRun>> = vec![Vec::new()];
                for run in &p.runs {
                    if matches!(run.kind, FlowRunKind::Tab) {
                        cells.push(Vec::new());
                    } else {
                        cells.last_mut().unwrap().push(run.clone());
                    }
                }
                cells
            })
            .collect();
        let columns = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
        let width = doc.page.size.width - doc.page.left - doc.page.right;
        let styles = doc.styles.clone();
        let mut next = doc.new_id();
        let mut id = || {
            next += 1;
            next - 1
        };
        let line = FlowBorder { width: 12_700 / 2, color: Rgb::BLACK, style: ibis_doc_model::BorderStyle::Single, space: 0, art: None };
        let mut table = FlowTable { id: id(), borders: [Some(Some(line)); 6], columns: vec![width / columns as i64 / 635 * 635; columns], fit_to_page: true, ..FlowTable::default() };
        for (cells, p) in rows.into_iter().zip(&paragraphs) {
            let mut row = FlowRow { id: id(), ..FlowRow::default() };
            for i in 0..columns {
                let mut para = FlowParagraph { id: id(), runs: cells.get(i).cloned().unwrap_or_default(), section: None, section_break: ibis_doc_model::SectionBreak::None, bookmarks: Vec::new(), field_start: None, field_end: false, table_style: styles.default_table.clone(), ..p.clone() };
                para.resolve(&styles);
                para.normalize();
                row.cells.push(FlowCell { id: id(), span: 1, margins: [0, 108 * 635, 0, 108 * 635], blocks: vec![Block::Paragraph(para)], ..FlowCell::default() });
            }
            table.rows.push(row);
        }
        table.resolve_borders();
        let command = FlowCommand::ReplaceBlocks { container: Container::Body, index: first, remove: last - first + 1, blocks: vec![Block::Table(table)] };
        self.replace_converted(command, cx);
    }

    pub(crate) fn table_to_text(&mut self, cx: &mut Context<Self>) {
        let Some(table) = self.convertible_table() else { return };
        let Some((container, index)) = self.flow().and_then(|d| flow_command::locate(d, table.id)) else { return };
        let Some(doc) = self.flow() else { return };
        let styles = doc.styles.clone();
        let mut next = doc.new_id();
        let mut blocks = Vec::new();
        for row in &table.rows {
            let mut runs: Vec<FlowRun> = Vec::new();
            let mut template: Option<FlowParagraph> = None;
            for (i, cell) in row.cells.iter().enumerate() {
                let Block::Paragraph(p) = &cell.blocks[0] else { continue };
                if i > 0 {
                    let mut tab = runs.last().cloned().or_else(|| p.runs.first().cloned());
                    if let Some(t) = tab.as_mut() {
                        t.kind = FlowRunKind::Tab;
                        runs.push(t.clone());
                    }
                }
                runs.extend(p.runs.iter().cloned());
                template.get_or_insert_with(|| p.clone());
            }
            let Some(p) = template else { continue };
            let mut para = FlowParagraph { id: next, runs, table_style: None, table_regions: Vec::new(), ..p };
            next += 1;
            para.resolve(&styles);
            para.normalize();
            blocks.push(Block::Paragraph(para));
        }
        if blocks.is_empty() {
            return;
        }
        let command = FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks };
        self.replace_converted(command, cx);
    }

    pub(crate) fn can_split_table(&self) -> bool {
        self.table_at_caret().is_some()
    }

    pub(crate) fn split_table(&mut self, cx: &mut Context<Self>) {
        let Some((table_id, row_idx, _)) = self.table_at_caret() else { return };
        let Some(doc) = self.flow() else { return };
        let Some((container, index)) = flow_command::locate(doc, table_id) else { return };
        let Some(Block::Table(table)) = flow_command::blocks(doc, container).and_then(|l| l.get(index)) else { return };
        let styles = doc.styles.clone();
        let mut next_id = doc.new_id();

        let template_p = table.rows.first().and_then(|r| r.cells.first()).and_then(|c| c.blocks.first()).and_then(|b| if let Block::Paragraph(p) = b { Some(p.clone()) } else { None });

        let mut blocks = Vec::new();
        if row_idx == 0 {
            // 表の直前に空段落を挿入
            let mut para = FlowParagraph {
                id: next_id,
                runs: Vec::new(),
                table_style: None,
                table_regions: Vec::new(),
                section: None,
                section_break: ibis_doc_model::SectionBreak::None,
                bookmarks: Vec::new(),
                field_start: None,
                field_end: false,
                ..template_p.unwrap_or_default()
            };
            para.resolve(&styles);
            para.normalize();
            blocks.push(Block::Paragraph(para));
            blocks.push(Block::Table(table.clone()));
        } else {
            // 表を row_idx の前後で2分割
            let mut top_table = table.clone();
            top_table.rows.truncate(row_idx);
            top_table.resolve_borders();

            let mut between_para = FlowParagraph {
                id: next_id,
                runs: Vec::new(),
                table_style: None,
                table_regions: Vec::new(),
                section: None,
                section_break: ibis_doc_model::SectionBreak::None,
                bookmarks: Vec::new(),
                field_start: None,
                field_end: false,
                ..template_p.unwrap_or_default()
            };
            next_id += 1;
            between_para.resolve(&styles);
            between_para.normalize();

            let mut bottom_table = table.clone();
            bottom_table.id = next_id;
            next_id += 1;
            bottom_table.rows = table.rows[row_idx..].to_vec();
            for r in &mut bottom_table.rows {
                r.id = next_id;
                next_id += 1;
                for c in &mut r.cells {
                    c.id = next_id;
                    next_id += 1;
                    for b in &mut c.blocks {
                        if let Block::Paragraph(p) = b {
                            p.id = next_id;
                            next_id += 1;
                        }
                    }
                }
            }
            bottom_table.resolve_borders();

            blocks.push(Block::Table(top_table));
            blocks.push(Block::Paragraph(between_para));
            blocks.push(Block::Table(bottom_table));
        }

        let command = FlowCommand::ReplaceBlocks { container, index, remove: 1, blocks };
        self.replace_converted(command, cx);
    }

    /// Run `command`, after checking the document still saves with it.
    fn replace_converted(&mut self, command: FlowCommand, cx: &mut Context<Self>) {
        if let Some(source) = self.source.as_ref() {
            let mut candidate = ibis_doc_model::Document::new(self.presentation().clone());
            if candidate.execute(vec![ibis_doc_model::Command::Flow(command.clone())]).is_err() {
                return;
            }
            if let Err(error) = source.save(candidate.presentation(), ibis_pptx::SaveOptions::default()) {
                self.status = Some(crate::shell::i18n::format("docx.table-convert.document-cannot-save-table-text", &[("error", &error)]).into());
                cx.notify();
                return;
            }
        }
        self.run_flow(vec![command], cx);
    }
}
