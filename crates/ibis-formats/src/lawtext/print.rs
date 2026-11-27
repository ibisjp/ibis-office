// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.

use std::sync::Arc;

use ibis_doc_model::flow::{
    Block, BorderStyle, FlowBorder, FlowTable, UnderlineStyle, format_list_number,
};
use ibis_doc_model::{
    CellDirection, DocGridType, Emu, EmuSize, FlowAlign, PageSetup, PartKind, Rgb,
};

use super::{LawDoc, Legislation5SetPackage, generate_shinkyu_markdown};
use crate::{Error, SaveOptions, markdown};

const COMPARISON_BOOKMARK: &str = "lawtext_comparison";

const SECTION_BREAK: &str = "\n\n::: section {\"break\":\"next-page\"}\n\n";
const TWIP: Emu = 635;
const MM: Emu = 36_000;
// The Cabinet submission body has 48 full-width slots and 13 vertical lines.
// The pitches approximate the published A4 bill measured in PDF points.
const BILL_CHAR_PITCH: Emu = 281 * TWIP;
const BILL_LINE_PITCH: Emu = 680 * TWIP;
const BILL_FONT_SIZE: u32 = 1_400;

const PAGE_STYLE: &str = "\
---
size: A4
margins.top: 24mm
margins.bottom: 24mm
margins.left: 22mm
margins.right: 22mm
font: Noto Serif CJK JP
font_size: 14pt
paragraph_spacing: compact
footer: '- {page} -'
---

";

impl Legislation5SetPackage {
    /// Produce one editable DOCX with section layouts based on published bill sets.
    pub fn to_print_docx(&self) -> Result<Vec<u8>, Error> {
        let bill = self.bill_markdown.replace(
            &self.aratame_draft.full_text,
            &self
                .aratame_draft
                .full_text
                .lines()
                .map(str::trim)
                .collect::<Vec<_>>()
                .join("\n\n"),
        );
        // Keep the comparison markup and contents from the common diff pipeline.
        let comparison = comparison_markdown(&self.shinkyu_markdown);
        let outline = numbered_outline(&self.youkou_markdown);
        let bill_and_reason = format!(
            "{}\n\n\\newpage\n\n{}",
            bill.trim_end(),
            self.riyuu_markdown.trim_end()
        );
        let references = reference_paragraphs(&self.sansho_markdown);
        let document = [
            outline.trim_end(),
            &bill_and_reason,
            comparison.trim_end(),
            references.trim_end(),
        ]
        .join(SECTION_BREAK);

        let mut opened = markdown::open_markdown(format!("{PAGE_STYLE}{document}").into_bytes())?;
        // The sections come from `::: section` lines, which this tree's Markdown reader does not know yet (masanork's 88f61579d). Without them the
        // layout below would be written onto one section and the markers would stay in the text, so refuse instead.
        let has_sections = opened.presentation.flow.as_ref().is_some_and(|flow| {
            flow.body.iter().any(|block| matches!(block, Block::Paragraph(p) if p.section.is_some()))
        });
        if !has_sections {
            return Err(Error::new("section 指定を読めない Markdown なので、印刷用 DOCX を作れません"));
        }
        let flow = Arc::make_mut(opened.presentation.flow.as_mut().unwrap());

        let portrait = PageSetup {
            line_pitch: None,
            doc_grid_type: DocGridType::Default,
            ..flow.page
        };
        let bill = bill_page(portrait);
        let comparison_page = PageSetup {
            direction: CellDirection::TbRl,
            footer: 12 * MM,
            ..portrait
        };
        let mut footer = std::mem::take(&mut flow.footer);
        for block in &mut footer {
            if let Block::Paragraph(paragraph) = block {
                set_font_size(paragraph, 1_000);
            }
        }
        let footer_index = PartKind::Footer.index();
        let mut section = 0;
        if let Some(Block::Paragraph(title)) = flow.body.first_mut() {
            title.direct.align = Some(FlowAlign::Center);
        }
        for block in &mut flow.body {
            let Block::Paragraph(paragraph) = block else {
                continue;
            };
            if section == 1 {
                paragraph.direct.before = Some(0);
                paragraph.direct.after = Some(0);
                paragraph.direct.align = Some(FlowAlign::Justify);
                set_font_size(paragraph, BILL_FONT_SIZE);
            } else {
                let size = match paragraph.style.as_deref() {
                    Some("Heading1") => 1_400,
                    Some(style) if style.starts_with("Heading") => 1_200,
                    _ => 1_100,
                };
                set_font_size(paragraph, size);
            }
            if let Some(setup) = &mut paragraph.section {
                let setup = Arc::make_mut(setup);
                setup.page = match section {
                    0 => portrait,
                    1 => bill,
                    _ => comparison_page,
                };
                setup.restart_numbering = section != 0;
                if section != 0 {
                    setup.parts[footer_index] = Some(footer.clone());
                }
                section += 1;
            }
        }

        flow.page = PageSetup {
            size: EmuSize::new(portrait.size.height, portrait.size.width),
            columns: ibis_doc_model::PageColumns {
                count: 2,
                space: 10 * MM,
                ..Default::default()
            },
            ..portrait
        };
        // Explicitly end the numbered footer carried over from the comparison.
        flow.linked[footer_index] = false;

        let available = comparison_page.size.height - comparison_page.top - comparison_page.bottom;
        format_comparison_table(flow, available)?;
        opened
            .source
            .save(&opened.presentation, SaveOptions::default())
    }
}

/// Open an editable DOCX containing just the vertical law comparison.
pub fn open_comparison_document(old: &LawDoc, new: &LawDoc) -> Result<crate::Opened, Error> {
    let comparison = comparison_markdown(&generate_shinkyu_markdown(old, new));
    let mut opened = markdown::open_markdown(format!("{PAGE_STYLE}{comparison}").into_bytes())?;
    let flow = Arc::make_mut(opened.presentation.flow.as_mut().unwrap());
    flow.page.direction = CellDirection::TbRl;
    flow.page.line_pitch = None;
    flow.page.doc_grid_type = DocGridType::Default;
    flow.page.footer = 12 * MM;
    let available = flow.page.size.height - flow.page.top - flow.page.bottom;
    format_comparison_table(flow, available)?;
    let bytes = opened
        .source
        .save(&opened.presentation, SaveOptions::default())?;
    crate::open(bytes)
}

fn comparison_markdown(markdown: &str) -> String {
    markdown
        .replacen(
            " 新旧対照表\n",
            &format!(" 新旧対照条文 {{#{COMPARISON_BOOKMARK}}}\n"),
            1,
        )
        .replacen("\n| 改正後 |", "\n| 改正案 |", 1)
        .replacen("| 改正後 |\n", "| 改正案 |\n", 1)
}

fn format_comparison_table(
    flow: &mut ibis_doc_model::flow::FlowDocument,
    available: Emu,
) -> Result<(), Error> {
    let table = flow
        .body
        .iter_mut()
        .skip_while(|block| {
            !matches!(block, Block::Paragraph(p)
            if p.bookmarks.iter().any(|name| name == COMPARISON_BOOKMARK))
        })
        .find_map(|block| match block {
            Block::Table(table) => Some(table),
            _ => None,
        })
        .ok_or_else(|| Error::new("新旧対照条文の表を組版できません"))?;
    format_comparison(table, available);
    Ok(())
}

fn bill_page(portrait: PageSetup) -> PageSetup {
    let top = 27 * MM / TWIP * TWIP;
    let right = 27 * MM / TWIP * TWIP;
    PageSetup {
        top,
        right,
        bottom: portrait.size.height - top - 48 * BILL_CHAR_PITCH,
        left: portrait.size.width - right - 13 * BILL_LINE_PITCH,
        footer: 12 * MM,
        direction: CellDirection::TbRl,
        line_pitch: Some(BILL_LINE_PITCH),
        // OOXML stores the addition to the Normal style's full-width advance.
        char_space: Some(
            BILL_CHAR_PITCH - (BILL_FONT_SIZE as Emu * ibis_doc_model::EMU_PER_POINT / 100),
        ),
        doc_grid_type: DocGridType::LinesAndChars,
        ..portrait
    }
}

fn set_font_size(paragraph: &mut ibis_doc_model::FlowParagraph, size: u32) {
    paragraph.mark_format.size = Some(size);
    for run in &mut paragraph.runs {
        run.format.size = Some(size);
    }
}

fn numbered_outline(outline: &str) -> String {
    let mut article = 0;
    outline
        .lines()
        .map(|line| {
            if let Some(text) = line.strip_prefix("- ") {
                article += 1;
                format!(
                    "{}　{text}\n\n",
                    format_list_number(article, "japaneseCounting")
                )
            } else if let Some(text) = line.strip_prefix("  - ") {
                format!("　{text}\n\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect()
}

fn reference_paragraphs(references: &str) -> String {
    references
        .lines()
        .map(|line| {
            if let Some(text) = line.trim_start().strip_prefix("- ") {
                let depth = (line.len() - line.trim_start().len()) / 2;
                format!("{}　{text}\n\n", "　".repeat(depth))
            } else {
                format!("{line}\n")
            }
        })
        .collect()
}

fn format_comparison(table: &mut FlowTable, available: ibis_doc_model::Emu) {
    // In a vertical section, the logical table columns occupy the upper and
    // lower bands of the paper. Let Word and the PDF renderer split long rows.
    let width = available / 2 / TWIP * TWIP;
    table.columns = vec![width; 2];
    table.style = None;
    let rule = FlowBorder {
        width: 6_350,
        color: Rgb(0),
        style: BorderStyle::Single,
        space: 0,
        art: None,
    };
    table.borders = [Some(Some(rule)); 6];
    table.borders[4] = Some(None);
    for (index, row) in table.rows.iter_mut().enumerate() {
        for cell in &mut row.cells {
            cell.shading = None;
            cell.own_borders = [None; 4];
            if index <= 1 {
                // A single rule separates the repeated labels from the text.
                cell.own_borders[if index == 0 { 2 } else { 0 }] = Some(Some(rule));
            }
            for block in &mut cell.blocks {
                let Block::Paragraph(paragraph) = block else {
                    continue;
                };
                paragraph.table_style = None;
                set_font_size(paragraph, 1_100);
                if index == 0 {
                    paragraph.direct.align = Some(FlowAlign::Center);
                }
                for run in &mut paragraph.runs {
                    if run.style.strike {
                        // The review Markdown strikes deletions. The printed
                        // comparison marks the changed text on both sides.
                        run.format.strike = None;
                        run.format.underline = Some(true);
                        run.format.underline_style = Some(UnderlineStyle::Single);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lawtext::{ShinkyuOptions, build_5set_package, parse_law_input};
    use ibis_doc_model::flow::FlowRunKind;

    #[test]
    fn direct_comparison_preserves_visible_statute_metadata_from_editable_document() {
        let old = parse_law_input("試験法（令和七年法律第一号）\n第一条　赤色を用いる。").unwrap();
        let new = parse_law_input("試験法（令和七年法律第一号）\n第一条　青色を用いる。").unwrap();
        let rendered =
            markdown::open_markdown(super::super::to_markdown(&new).into_bytes()).unwrap();
        let exported = markdown::export_presentation_to_markdown(&rendered.presentation).unwrap();
        let new = super::super::parse_markdown_law(&exported).unwrap();
        assert_eq!(new.law_title, old.law_title);
        assert_eq!(new.law_num, old.law_num);
        let result = open_comparison_document(&old, &new).unwrap();
        let flow = result.presentation.flow.unwrap();
        assert_eq!(flow.page.direction, CellDirection::TbRl);
        assert_eq!(
            flow.body
                .iter()
                .filter(|block| matches!(block, Block::Table(_)))
                .count(),
            1
        );
        let plain = markdown::export_markdown(&flow, &result.presentation.media);
        assert!(plain.contains("新旧対照条文") && plain.contains("青色") && plain.contains("赤色"));
    }

    #[test]
    #[ignore = "needs the Markdown `::: section` reader (masanork 88f61579d), not in this tree yet; to_print_docx returns an error until then"]
    fn print_docx_preserves_page_breaks_rules_and_change_marks() {
        let old = parse_law_input("試験法（令和七年法律第一号）\n第一条　赤色を用いる。").unwrap();
        let new = parse_law_input("試験法（令和七年法律第一号）\n第一条　青色を用いる。").unwrap();
        let set = build_5set_package(&old, &new, "", &ShinkyuOptions::default());
        let bytes = set.to_print_docx().unwrap();
        let opened = crate::open(bytes).unwrap();
        let flow = opened.presentation.flow.unwrap();
        use ibis_doc_model::flow_command::{section_setup, sections};
        let pages: Vec<_> = sections(&flow)
            .iter()
            .map(|&id| section_setup(&flow, id).unwrap().page)
            .collect();
        assert_eq!(
            pages.iter().map(|p| p.direction).collect::<Vec<_>>(),
            [
                CellDirection::LrTb,
                CellDirection::TbRl,
                CellDirection::TbRl,
                CellDirection::LrTb
            ]
        );
        let bill = pages[1];
        assert_eq!(bill.doc_grid_type, DocGridType::LinesAndChars);
        assert_eq!(bill.line_pitch, Some(BILL_LINE_PITCH));
        let normal = flow.styles.default_paragraph.as_deref().unwrap();
        assert_eq!(flow.styles.styles[normal].run.size, Some(BILL_FONT_SIZE));
        assert_eq!(bill.char_space, Some(635));
        assert_eq!(
            bill.size.height - bill.top - bill.bottom,
            48 * BILL_CHAR_PITCH
        );
        assert_eq!(
            bill.size.width - bill.left - bill.right,
            13 * BILL_LINE_PITCH
        );
        assert!(flow.page.size.width > flow.page.size.height);
        assert_eq!(flow.page.columns.count, 2);
        assert!(
            pages
                .iter()
                .enumerate()
                .all(|(i, p)| i == 1 || p.doc_grid_type == DocGridType::Default)
        );
        assert!(flow.page.footer < flow.page.bottom);
        assert!(flow.body.iter().any(|block| matches!(block, Block::Paragraph(p)
            if p.runs.iter().any(|run| matches!(&run.kind, FlowRunKind::Text(t) if t.starts_with("一　第一条"))))));
        let breaks = flow
            .body
            .iter()
            .filter(|block| {
                matches!(block, Block::Paragraph(paragraph) if paragraph.runs.iter().any(|run|
                matches!(run.kind, FlowRunKind::PageBreak)))
            })
            .count();
        assert_eq!(breaks, 1);
        let table = flow
            .body
            .iter()
            .find_map(|block| {
                if let Block::Table(table) = block {
                    Some(table)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(table.columns.len(), 2);
        assert!(
            (table.columns.iter().sum::<ibis_doc_model::Emu>()
                - (pages[2].size.height - pages[2].top - pages[2].bottom))
                .abs()
                <= 1_270
        );
        assert!(table.rows[0].header);
        assert_eq!(table.borders[5].flatten().unwrap().color, Rgb(0));
        for cell in &table.rows[1].cells {
            let Block::Paragraph(p) = &cell.blocks[0] else {
                panic!("missing comparison text")
            };
            assert!(p.runs.iter().any(|run| run.style.underline));
            assert!(p.runs.iter().all(|run| !run.style.strike));
        }
        build_5set_package(&old, &old, "", &ShinkyuOptions::default())
            .to_print_docx()
            .unwrap();
    }
}
