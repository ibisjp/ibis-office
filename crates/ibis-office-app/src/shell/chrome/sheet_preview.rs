// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Read-only tabular preview using the spreadsheet editor's canvas and cell layout.
use super::*;
use ibis_doc_model::{CellKind, CellStyle, CellValue, EMU_PER_POINT, EmuRect, Sheet, SheetCell};
use std::sync::Arc;
pub(crate) fn sheet_preview(
    id: &'static str,
    fields: &[String],
    records: &[Vec<String>],
    res: &crate::render::shape_view::Resources,
) -> impl IntoElement {
    let rows = std::iter::once(fields.to_vec())
        .chain(records.iter().take(5).cloned())
        .collect::<Vec<_>>();
    let font = res.font(&ibis_doc_model::TextStyle {
        font_size: 1100,
        ..Default::default()
    });
    let columns = (0..fields.len())
        .map(|c| {
            let width = rows
                .iter()
                .filter_map(|r| r.get(c))
                .map(|v| res.text_width(v, &font, 11.) + 8.)
                .fold(64., f32::max)
                .min(320.);
            (width * EMU_PER_POINT as f32).ceil() as i64
        })
        .collect::<Vec<_>>();
    let mut sheet = Sheet {
        columns,
        rows: vec![18 * EMU_PER_POINT; rows.len()],
        styles: vec![
            CellStyle::default(),
            CellStyle {
                bold: true,
                fill: Some(ibis_doc_model::Rgb(0xf3f4f6)),
                ..Default::default()
            },
        ],
        grid_lines: true,
        headings: false,
        ..Sheet::default()
    };
    sheet.cells = rows
        .iter()
        .enumerate()
        .flat_map(|(r, row)| {
            row.iter().enumerate().map(move |(c, text)| SheetCell {
                row: r as u32,
                column: c as u32,
                text: text.clone(),
                value: CellValue::Text(text.clone()),
                kind: CellKind::Text,
                formula: None,
                style: if r == 0 { 1 } else { 0 },
                phonetic: None,
                runs: None,
            })
        })
        .collect::<Vec<_>>()
        .into();
    let frame = EmuRect::new(0, 0, sheet.columns.iter().sum(), sheet.rows.iter().sum());
    let scale = 96. / 72. / EMU_PER_POINT as f32;
    let h = frame.height as f32 * scale;
    div()
        .id(id)
        .w_full()
        .h(px(h + 2.))
        .overflow_x_scroll()
        .overflow_y_hidden()
        .border_1()
        .border_color(rgb(BORDER))
        .child(
            div()
                .w(px(frame.width as f32 * scale))
                .h(px(h))
                .overflow_hidden()
                .child(crate::xlsx::sheet_view::sheet_element(
                    &Arc::new(sheet),
                    frame,
                    scale,
                    res,
                )),
        )
}
