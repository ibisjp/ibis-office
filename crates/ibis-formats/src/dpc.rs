// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/246)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust Japanese DPC E/F file (tab-delimited) parser, leading-zero-safe
//! workbook builder, and Harunobu medical data quality audit profile.

use std::collections::BTreeMap;
use std::sync::Arc;

use ibis_doc_model::{
    CellKind, CellStyle, CellValue, EmuRect, Presentation, Shape, ShapeKind, Sheet, SheetCell,
    SheetState, Slide, WorkbookSheet,
};

use crate::{Error, Opened};

/// The header columns every DPC E/F file has (all of them are needed to take a tab-separated file for one).
const DPC_EF_HEADER_HINTS: &[&str] = &["施設コード", "データ識別番号", "退院年月日", "入院年月日"];

/// Returns `true` if `data` looks like a tab-delimited DPC E/F file.
pub fn is_dpc_ef(data: &[u8]) -> bool {
    if !crate::csv::is_csv(data) || data.is_empty() {
        return false;
    }
    let Ok(text) = crate::csv::decode(data) else {
        return false;
    };
    let Some(first) = text.lines().map(str::trim).find(|l| !l.is_empty()) else {
        return false;
    };
    if !first.contains('\t') {
        return false;
    }
    // Every one of the four columns, as a cell of the header line.
    let cells: Vec<&str> = first.split('\t').map(|c| c.trim().trim_matches('"')).collect();
    DPC_EF_HEADER_HINTS.iter().all(|h| cells.contains(h))
}

/// One parsed DPC E/F detail record.
#[derive(Debug, Clone, PartialEq)]
pub struct DpcEfRecord {
    pub facility_code: String,
    pub patient_id: String,
    pub discharge_date: String,
    pub admission_date: String,
    pub data_kubun: String,
    pub item_code: String,
    pub item_name: String,
    pub points_or_amount: Option<f64>,
    pub count: Option<f64>,
    pub act_date: String,
    pub raw_fields: Vec<String>,
}

/// Parsed DPC E/F document.
#[derive(Debug, Clone)]
pub struct DpcEfDocument {
    pub headers: Vec<String>,
    pub records: Vec<DpcEfRecord>,
}

/// Parse DPC E/F tab-delimited bytes while preserving all IDs and codes as strings.
pub fn parse_dpc_ef(data: &[u8]) -> Result<DpcEfDocument, Error> {
    let text = crate::csv::decode(data)?;
    let rows = crate::csv::parse_with_delimiter(&text, '\t');
    if rows.len() < 2 {
        return Err(Error::new("DPC E/F ファイルに明細行がありません"));
    }
    let headers: Vec<String> = rows[0].iter().map(|s| s.trim().to_string()).collect();
    let idx_of = |name: &str| headers.iter().position(|h| h == name);

    let c_fac = idx_of("施設コード").unwrap_or(0);
    let c_pid = idx_of("データ識別番号").unwrap_or(1);
    let c_dis = idx_of("退院年月日").unwrap_or(2);
    let c_adm = idx_of("入院年月日").unwrap_or(3);
    let c_kubun = idx_of("データ区分").unwrap_or(4);
    let c_code = idx_of("レセプト電算処理システム用コード")
        .or_else(|| idx_of("診療行為コード"))
        .unwrap_or(6);
    let c_name = idx_of("診療明細名称")
        .or_else(|| idx_of("行為明細名称"))
        .unwrap_or(7);
    let c_pts = idx_of("行為点数").or_else(|| idx_of("点数"));
    let c_cnt = idx_of("行為回数").or_else(|| idx_of("回数"));
    let c_date = idx_of("実施年月日").unwrap_or(12);

    let mut records = Vec::with_capacity(rows.len() - 1);
    for row in rows.iter().skip(1) {
        if row.iter().all(|s| s.trim().is_empty()) {
            continue;
        }
        let get = |i: usize| row.get(i).map(|s| s.trim().to_string()).unwrap_or_default();
        let parse_num = |opt: Option<usize>| {
            opt.and_then(|i| row.get(i))
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .and_then(|s| s.parse::<f64>().ok())
        };
        records.push(DpcEfRecord {
            facility_code: get(c_fac),
            patient_id: get(c_pid),
            discharge_date: get(c_dis),
            admission_date: get(c_adm),
            data_kubun: get(c_kubun),
            item_code: get(c_code),
            item_name: get(c_name),
            points_or_amount: parse_num(c_pts),
            count: parse_num(c_cnt),
            act_date: get(c_date),
            raw_fields: row.clone(),
        });
    }

    Ok(DpcEfDocument { headers, records })
}

/// Open DPC E/F bytes as a two-sheet [`Opened`] workbook (`DPC明細` / `DPCサマリー`).
pub fn open_dpc_ef(data: &[u8]) -> Result<Opened, Error> {
    let doc = parse_dpc_ef(data)?;
    let blank = crate::blank::blank_file(crate::Kind::Workbook)?;
    let mut opened = crate::save::open(blank)?;

    let detail_cells = build_detail_cells(&doc);
    let summary_cells = build_summary_cells(&doc);

    if let Some(wb_arc) = opened.presentation.workbook.as_mut()
        && let Some(first_meta) = Arc::make_mut(wb_arc).sheets.first_mut()
    {
        first_meta.name = crate::words::text("medical.dpc.sheet-detail");
    }
    // The sheet tab shows the slide's name, so set it as well (the blank workbook names it "Sheet1").
    if let Some(slide) = opened.presentation.slides.first_mut() {
        slide.name = Some(crate::words::text("medical.dpc.sheet-detail"));
    }
    if let Some(slide) = opened.presentation.slides.first_mut() {
        for sh in &mut slide.shapes {
            if let ShapeKind::Sheet(sheet_arc) = &mut sh.kind {
                let mut sheet = (**sheet_arc).clone();
                ensure_bold_style(&mut sheet);
                sheet.cells = detail_cells.into();
                *sheet_arc = Arc::new(sheet);
                break;
            }
        }
    }

    let summary_slide_id = opened.presentation.new_slide_id();
    let template_styles = opened
        .presentation
        .slides
        .first()
        .and_then(|s| {
            s.shapes.iter().find_map(|sh| match &sh.kind {
                ShapeKind::Sheet(sheet) => Some(sheet.styles.clone()),
                _ => None,
            })
        })
        .unwrap_or_default();
    let mut summary_sheet = Sheet {
        cells: summary_cells.into(),
        styles: template_styles,
        ..Default::default()
    };
    ensure_bold_style(&mut summary_sheet);

    let frame = opened
        .presentation
        .slides
        .first()
        .and_then(|s| s.shapes.first())
        .map(|sh| sh.frame)
        .unwrap_or_else(|| EmuRect::new(0, 0, 0, 0));
    let mut summary_slide = Slide::new(summary_slide_id);
    summary_slide.name = Some(crate::words::text("medical.dpc.sheet-summary"));
    summary_slide.shapes.push(Shape::new(
        opened.presentation.new_shape_id(),
        frame,
        ShapeKind::Sheet(Arc::new(summary_sheet)),
    ));
    opened.presentation.slides.push(summary_slide);
    if let Some(wb_arc) = opened.presentation.workbook.as_mut() {
        Arc::make_mut(wb_arc).sheets.push(WorkbookSheet {
            name: crate::words::text("medical.dpc.sheet-summary"),
            state: SheetState::Visible,
            slide: Some(summary_slide_id),
            hidden: None,
            other: false,
        });
    }

    Ok(opened)
}

/// What the medical data quality check of a workbook found: cells whose date is written with a Japanese era (令和, 平成, 昭和, or R/H/S and a number), which a
/// system that takes ISO dates would not read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MedicalQualityAudit {
    pub era_dates: usize,
}

/// The audit of the cells of every sheet of the workbook `presentation`.
pub fn audit_medical_workbook(presentation: &Presentation) -> MedicalQualityAudit {
    let mut audit = MedicalQualityAudit::default();
    for slide in &presentation.slides {
        for sh in &slide.shapes {
            let ShapeKind::Sheet(sheet) = &sh.kind else { continue };
            for cell in &sheet.cells {
                let t = cell.text.trim();
                let era_word = ["令和", "平成", "昭和"].iter().any(|e| t.contains(e)); // i18n-ok（和暦の元号）
                let era_letter = t.len() > 1 && matches!(t.as_bytes()[0], b'R' | b'H' | b'S') && t.as_bytes()[1].is_ascii_digit();
                if era_word || era_letter {
                    audit.era_dates += 1;
                }
            }
        }
    }
    audit
}

fn ensure_bold_style(sheet: &mut Sheet) {
    if sheet.styles.len() < 2 {
        let base = sheet.styles.first().cloned().unwrap_or_default();
        if sheet.styles.is_empty() {
            sheet.styles.push(base.clone());
        }
        sheet.styles.push(CellStyle { bold: true, ..base });
    }
}

fn build_detail_cells(doc: &DpcEfDocument) -> Vec<SheetCell> {
    let mut cells = Vec::new();
    for (c, h) in doc.headers.iter().enumerate() {
        cells.push(text_cell(0, c as u32, h, 1));
    }
    for (ri, rec) in doc.records.iter().enumerate() {
        let r = (ri + 1) as u32;
        for (ci, raw) in rec.raw_fields.iter().enumerate() {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                cells.push(text_cell(r, ci as u32, trimmed, 0));
            }
        }
    }
    cells
}

fn build_summary_cells(doc: &DpcEfDocument) -> Vec<SheetCell> {
    let mut cells = Vec::new();
    for (c, h) in ["patient-id", "facility-code", "item-count", "points"].iter().enumerate() {
        cells.push(text_cell(0, c as u32, &crate::words::text(&format!("medical.dpc.summary.{h}")), 1));
    }

    let mut by_patient: BTreeMap<String, (String, usize, f64)> = BTreeMap::new();
    for rec in &doc.records {
        let entry = by_patient
            .entry(rec.patient_id.clone())
            .or_insert_with(|| (rec.facility_code.clone(), 0, 0.0));
        entry.1 += 1;
        let pts = rec.points_or_amount.unwrap_or(0.0);
        let cnt = rec.count.unwrap_or(1.0);
        entry.2 += pts * cnt;
    }

    for (i, (pid, (fac, count, pts))) in by_patient.iter().enumerate() {
        let r = (i + 1) as u32;
        cells.push(text_cell(r, 0, pid, 0));
        cells.push(text_cell(r, 1, fac, 0));
        cells.push(num_cell(r, 2, *count as f64, 0));
        cells.push(num_cell(r, 3, *pts, 0));
    }
    cells
}

fn text_cell(row: u32, column: u32, s: &str, style: u32) -> SheetCell {
    let val = CellValue::Text(s.to_string());
    SheetCell {
        row,
        column,
        kind: CellKind::of(&val),
        value: val,
        text: s.to_string(),
        style,
        formula: None,
        phonetic: None,
        runs: None,
    }
}

fn num_cell(row: u32, column: u32, n: f64, style: u32) -> SheetCell {
    let val = CellValue::Number(n);
    let text = if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    };
    SheetCell {
        row,
        column,
        kind: CellKind::of(&val),
        value: val,
        text,
        style,
        formula: None,
        phonetic: None,
        runs: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYNTHETIC_DPC_EF: &str = "\
施設コード\tデータ識別番号\t退院年月日\t入院年月日\tデータ区分\t順序番号\tレセプト電算処理システム用コード\t診療明細名称\t行為点数\t行為回数\t実施年月日
0123456\t0000123456\t20261110\t20261101\t11\t001\t111000110\t初診料\t291\t1\t20261101
0123456\t0000123456\t20261110\t20261101\t21\t002\t620000001\tテスト錠１０ｍｇ\t33\t3\t20261102
";

    #[test]
    fn opens_dpc_ef_preserving_leading_zeros_and_summarizing() {
        assert!(is_dpc_ef(SYNTHETIC_DPC_EF.as_bytes()));
        assert!(!is_dpc_ef(b"col1\tcol2\n1\t2\n"));

        let opened = open_dpc_ef(SYNTHETIC_DPC_EF.as_bytes()).expect("open_dpc_ef");
        let wb = opened.presentation.workbook.as_ref().unwrap();
        assert_eq!(wb.sheets.len(), 2);
        assert_eq!(wb.sheets[0].name, "DPC明細");
        assert_eq!(wb.sheets[1].name, "DPCサマリー");
        // The tabs show the slides' names.
        assert_eq!(opened.presentation.slides[0].name.as_deref(), Some("DPC明細"));
        assert_eq!(opened.presentation.slides[1].name.as_deref(), Some("DPCサマリー"));

        let ShapeKind::Sheet(summary) = &opened.presentation.slides[1].shapes[0].kind else {
            panic!("expected sheet");
        };
        let pid_cell = summary.cells.iter().find(|c| c.row == 1 && c.column == 0).unwrap();
        assert_eq!(pid_cell.text, "0000123456");
        let pts_cell = summary.cells.iter().find(|c| c.row == 1 && c.column == 3).unwrap();
        assert_eq!(pts_cell.text, "390"); // 291*1 + 33*3

        let audit = audit_medical_workbook(&opened.presentation);
        assert_eq!(audit.era_dates, 0);
    }
}
