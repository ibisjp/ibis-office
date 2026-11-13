// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The table styles of the テーブルのデザイン panel as small 4:3 previews: a
//! 4 x 5 table laid out in the style (ibis_pptx::styled_with_table, which
//! reads ECMA-376's presetTableStyles.xml), drawn as its fills and lines.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use ibis_doc_model::{Border, BorderLine, CellKind, CellRange, CellStyle, CellValue, Rgb, Sheet, SheetCell, SheetTable, TableStyleInfo};

use crate::shell::palette::rgb;

const COLUMNS: u32 = 4;
const ROWS: u32 = 5;
/// The preview's size: 4:3.
pub(crate) const PREVIEW_W: f32 = 112.;
pub(crate) const PREVIEW_H: f32 = 84.;

#[derive(Clone, Default)]
struct PreviewCell {
    fill: Option<u32>,
    text: u32,
    /// left, right, top, bottom: (colour, width in px).
    lines: [Option<(u32, f32)>; 4],
}

type Preview = Rc<Vec<Vec<PreviewCell>>>;

thread_local! {
    static CACHE: RefCell<HashMap<String, Preview>> = RefCell::new(HashMap::new());
}

fn line(border: &Option<Border>) -> Option<(u32, f32)> {
    border.as_ref().map(|b| (b.color.0, if matches!(b.line, BorderLine::Thin | BorderLine::Hair | BorderLine::Dotted | BorderLine::Dashed | BorderLine::DashDot | BorderLine::DashDotDot) { 1. } else { 2. }))
}

/// The style laid over a 4 x 5 table (a heading row, four rows) with the
/// style's options, cached.
fn preview(info: &TableStyleInfo, theme: &[(String, Rgb)]) -> Preview {
    let theme_key = theme.iter().fold(0u64, |h, (_, c)| h.wrapping_mul(31).wrapping_add(c.0 as u64));
    let key = format!("{}|{}{}{}{}|{theme_key}", info.name, info.first_column as u8, info.last_column as u8, info.row_stripes as u8, info.column_stripes as u8);
    if let Some(found) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return found;
    }
    let cells = (0..ROWS).flat_map(|row| (0..COLUMNS).map(move |column| SheetCell { runs: None, phonetic: None, row, column, value: CellValue::Text("x".into()), text: "x".into(), kind: CellKind::Text, formula: None, style: 0 })).collect::<Vec<_>>();
    let sheet = Sheet { cells: cells.into(), styles: vec![CellStyle::default()], ..Default::default() };
    let table = SheetTable {
        name: "preview".into(),
        range: CellRange { first_row: 0, first_column: 0, last_row: ROWS - 1, last_column: COLUMNS - 1 },
        header_rows: 1,
        totals_rows: 0,
        columns: (0..COLUMNS).map(|i| i.to_string()).collect(),
        style: Some(info.clone()),
        filter: None,
        totals: Vec::new(),
    };
    let styled = ibis_pptx::styled_with_table(&sheet, &table, theme);
    let grid: Vec<Vec<PreviewCell>> = (0..ROWS)
        .map(|row| {
            (0..COLUMNS)
                .map(|column| match styled.cell(row, column) {
                    Some(c) => {
                        let st = &styled.styles[c.style as usize];
                        PreviewCell { fill: st.fill.map(|f| f.0), text: st.color.0, lines: [line(&st.left), line(&st.right), line(&st.top), line(&st.bottom)] }
                    }
                    None => PreviewCell::default(),
                })
                .collect()
        })
        .collect();
    let grid = Rc::new(grid);
    CACHE.with(|c| c.borrow_mut().insert(key, grid.clone()));
    grid
}

/// One preview: the table in the style, framed in the primary colour when
/// `selected`.
pub(crate) fn table_style_preview(info: &TableStyleInfo, theme: &[(String, Rgb)], selected: bool) -> Div {
    let grid = preview(info, theme);
    let edge = |row: usize, column: usize, side: usize| -> Option<(u32, f32)> {
        // A line between two cells is drawn once: by the cell that owns it, or the neighbour's.
        let own = grid[row][column].lines[side];
        let other = match side {
            0 if column > 0 => grid[row][column - 1].lines[1],
            2 if row > 0 => grid[row - 1][column].lines[3],
            1 if column + 1 < COLUMNS as usize => grid[row][column + 1].lines[0],
            3 if row + 1 < ROWS as usize => grid[row + 1][column].lines[2],
            _ => None,
        };
        own.or(other)
    };
    let mut body = v_flex().size_full().bg(rgb(0xffffff));
    for row in 0..ROWS as usize {
        let mut line_of_cells = h_flex().flex_1().w_full();
        for column in 0..COLUMNS as usize {
            let cell = &grid[row][column];
            let mut d = div().relative().flex_1().h_full().flex().items_center().justify_center();
            if let Some(fill) = cell.fill {
                d = d.bg(rgb(fill));
            }
            // A small bar for the text, in the text's colour.
            d = d.child(div().w(px(10.)).h(px(2.)).bg(rgb(cell.text)));
            let draw = |d: Div, side: usize| match edge(row, column, side) {
                Some((color, width)) => {
                    let bar = div().absolute().bg(rgb(color));
                    d.child(match side {
                        0 => bar.top_0().bottom_0().left_0().w(px(width)),
                        1 => bar.top_0().bottom_0().right_0().w(px(width)),
                        2 => bar.left_0().right_0().top_0().h(px(width)),
                        _ => bar.left_0().right_0().bottom_0().h(px(width)),
                    })
                }
                None => d,
            };
            d = draw(d, 0);
            d = draw(d, 2);
            if column + 1 == COLUMNS as usize {
                d = draw(d, 1);
            }
            if row + 1 == ROWS as usize {
                d = draw(d, 3);
            }
            line_of_cells = line_of_cells.child(d);
        }
        body = body.child(line_of_cells);
    }
    let frame = if selected { crate::shell::chrome::PRIMARY_BUTTON_BG } else { 0xc8c8c8 };
    div().w(px(PREVIEW_W)).h(px(PREVIEW_H)).flex_none().p(px(3.)).bg(rgb(0xffffff)).border(px(if selected { 2. } else { 1. })).border_color(rgb(frame)).rounded(px(3.)).child(body)
}
