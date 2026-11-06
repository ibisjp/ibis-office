// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! フォーム コントロール (form controls): the button, check box, option button, scroll bar, spin button, drop-down
//! list, list box, group box and label Excel puts on a sheet (MS-XLSX `controlPr`, `formControlPr`).
//!
//! A control is a floating shape ([`crate::ShapeKind::Control`]). Its state (checked, the value, the item chosen)
//! is kept in it; when it has a linked cell (`fmlaLink`) the state is read from the cell after every command
//! ([`refresh_controls`]), and a click on the control writes the cell.

use crate::model::{Presentation, ShapeKind, SlideId};
use crate::sheet::{CellValue, Sheet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    Button,
    CheckBox,
    Radio,
    Scroll,
    Spin,
    /// A drop-down list (the combo box).
    Drop,
    List,
    GroupBox,
    Label,
}

/// A form control (`formControlPr`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormControl {
    pub kind: ControlKind,
    /// Its name (`Check Box 2`).
    pub name: String,
    /// The text on it (a button's caption, a check box's label).
    pub text: String,
    /// The cell it is linked to (`fmlaLink`: `$H$2`, `Sheet1!$H$2`).
    pub link: Option<String>,
    /// The cells of its items (`fmlaRange`).
    pub range: Option<String>,
    pub min: i64,
    pub max: i64,
    /// The step a click on an arrow makes (`inc`), and a click on the scroll bar's track (`page`).
    pub step: i64,
    pub page: i64,
    pub horizontal: bool,
    /// A check box: 0 unchecked, 1 checked, 2 mixed. An option button: 1 if chosen.
    pub checked: u8,
    /// A scroll bar's or spin button's value; a list's or drop-down's item (1 for the first, 0 none).
    pub value: i64,
    /// The first of a group of option buttons (`firstButton`).
    pub first_button: bool,
    /// Whether 3D shading is enabled (`noThreeD` absent or `"0"`).
    pub three_d: bool,
    /// Optional caption font family (`a:latin` / `a:ea` typeface in the drawing shape).
    pub font: Option<String>,
    /// Optional caption font size in hundredths of a point (`a:rPr/@sz`, e.g. 1200 = 12pt).
    pub font_size: Option<i32>,
    /// Whether the caption text is bold (`a:rPr/@b`).
    pub bold: bool,
    /// Whether the caption text is italic (`a:rPr/@i`).
    pub italic: bool,
    /// Optional caption text color (`a:rPr/a:solidFill/a:srgbClr`).
    pub text_color: Option<crate::Rgb>,
    /// The items (read from `range` after every command).
    pub items: Vec<String>,
    /// The part of the file (`ctrlProp`) it was read from, for saving its state.
    pub part: String,
    /// The `id` of its legacy VML shape (`_x0000_s1043`) when the sheet describes it only by VML
    /// (Excel 2007: no `controls` element); empty when it was read from a `control` element.
    pub vml_id: String,
}

impl FormControl {
    pub fn new(kind: ControlKind, name: &str) -> Self {
        let three_d = matches!(kind, ControlKind::Button | ControlKind::Scroll | ControlKind::Spin);
        Self {
            kind,
            name: name.to_string(),
            text: String::new(),
            link: None,
            range: None,
            min: 0,
            max: if kind == ControlKind::Scroll { 100 } else { 30000 },
            step: 1,
            page: 10,
            horizontal: false,
            checked: 0,
            value: 0,
            first_button: false,
            three_d,
            font: None,
            font_size: None,
            bold: false,
            italic: false,
            text_color: None,
            items: Vec::new(),
            part: String::new(),
            vml_id: String::new(),
        }
    }
}

fn sheet_of(presentation: &Presentation, slide: SlideId) -> Option<&Sheet> {
    presentation.slide(slide).and_then(crate::sheet_calc::sheet_of_slide).map(|s| &**s)
}

/// The sheet (its slide) and the cells a reference such as `$H$2`, `Sheet1!$F$1:$F$4` names. A reference with no
/// sheet is on `own`.
pub fn resolve(presentation: &Presentation, own: SlideId, text: &str) -> Option<(SlideId, crate::CellRange)> {
    use ibis_calc::formula::parser::{Expr, parse};
    let Expr::Ref(r) = parse(text.trim().trim_start_matches('=')).ok()? else { return None };
    let slide = match &r.sheet {
        None => own,
        Some(s) => {
            let w = presentation.workbook.as_ref()?;
            w.sheets.iter().find(|ws| ws.name.eq_ignore_ascii_case(&s.first))?.slide?
        }
    };
    Some((slide, crate::CellRange { first_row: r.start.row.min(r.end.row), first_column: r.start.column.min(r.end.column), last_row: r.start.row.max(r.end.row), last_column: r.start.column.max(r.end.column) }))
}

/// Like [`resolve`], but also resolves hidden and `veryHidden` worksheets (which have no `SlideId`).
fn resolve_sheet<'a>(presentation: &'a Presentation, own: SlideId, text: &str) -> Option<(&'a Sheet, crate::CellRange)> {
    use ibis_calc::formula::parser::{Expr, parse};
    let Expr::Ref(r) = parse(text.trim().trim_start_matches('=')).ok()? else { return None };
    let sheet = match &r.sheet {
        None => sheet_of(presentation, own)?,
        Some(s) => {
            let w = presentation.workbook.as_ref()?;
            let ws = w.sheets.iter().find(|ws| ws.name.eq_ignore_ascii_case(&s.first))?;
            ws.slide.and_then(|id| sheet_of(presentation, id)).or(ws.hidden.as_deref())?
        }
    };
    Some((sheet, crate::CellRange { first_row: r.start.row.min(r.end.row), first_column: r.start.column.min(r.end.column), last_row: r.start.row.max(r.end.row), last_column: r.start.column.max(r.end.column) }))
}

fn number_of(v: &CellValue) -> Option<f64> {
    match v {
        CellValue::Number(n) => Some(*n),
        CellValue::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

/// The state of every control as its linked cell has it (and its items as the cells of its range).
pub fn refresh_controls(presentation: &mut Presentation) {
    if !presentation.slides.iter().any(|s| s.shapes.iter().any(|sh| matches!(sh.kind, ShapeKind::Control(_)))) {
        return;
    }
    let mut updates: Vec<(usize, usize, FormControl)> = Vec::new();
    for (si, slide) in presentation.slides.iter().enumerate() {
        // The option buttons of a group: from one marked first up to the next such.
        let mut group = 0usize;
        let mut position = 0i64;
        for (hi, shape) in slide.shapes.iter().enumerate() {
            let ShapeKind::Control(c) = &shape.kind else { continue };
            let mut now = (**c).clone();
            if c.kind == ControlKind::Radio {
                if c.first_button {
                    group = hi + 1;
                    position = 0;
                }
                position += 1;
            }
            if let Some((link_sheet, range)) = c.link.as_deref().and_then(|l| resolve_sheet(presentation, slide.id, l)) {
                let value = link_sheet.cell(range.first_row, range.first_column).map(|cell| cell.value.clone()).unwrap_or(CellValue::Empty);
                match c.kind {
                    ControlKind::CheckBox => {
                        now.checked = match &value {
                            CellValue::Bool(b) => u8::from(*b),
                            CellValue::Error(_) => 2,
                            v => u8::from(number_of(v).is_some_and(|n| n != 0.0)),
                        }
                    }
                    ControlKind::Radio => {
                        // The cell holds the number of the button chosen in the group (the link of the first one).
                        let chosen = group > 0
                            && match &slide.shapes[group - 1].kind {
                                ShapeKind::Control(first) => first.link.as_deref().and_then(|l| resolve_sheet(presentation, slide.id, l)).and_then(|(sh, r)| sh.cell(r.first_row, r.first_column).and_then(|cell| number_of(&cell.value))),
                                _ => None,
                            }
                            .is_some_and(|n| n as i64 == position);
                        now.checked = u8::from(chosen);
                    }
                    ControlKind::Scroll | ControlKind::Spin | ControlKind::Drop | ControlKind::List => {
                        if let Some(n) = number_of(&value) {
                            now.value = (n as i64).clamp(if matches!(c.kind, ControlKind::Drop | ControlKind::List) { 0 } else { c.min }, if matches!(c.kind, ControlKind::Drop | ControlKind::List) { i64::MAX } else { c.max });
                        }
                    }
                    _ => {}
                }
            }
            if let Some((sheet, range)) = c.range.as_deref().and_then(|r| resolve_sheet(presentation, slide.id, r)) {
                let mut items = Vec::new();
                for row in range.first_row..=range.last_row.min(range.first_row + 255) {
                    for col in range.first_column..=range.last_column.min(range.first_column + 255) {
                        items.push(sheet.cell(row, col).map(|cell| cell.text.clone()).unwrap_or_default());
                    }
                }
                now.items = items;
            }
            if now != **c {
                updates.push((si, hi, now));
            }
        }
    }
    for (si, hi, now) in updates {
        presentation.slides[si].shapes[hi].kind = ShapeKind::Control(std::sync::Arc::new(now));
    }
}

/// Where on a control a press is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlHit {
    /// Anywhere on a button, check box, option button.
    Whole,
    /// An arrow of a scroll bar or spin button that makes the value `delta` more (`step`s).
    Arrow(i64),
    /// The track of a scroll bar, before (`-1`) or after (`1`) the thumb: a page.
    Track(i64),
    /// An item of a list (from 0).
    Item(usize),
    /// A drop-down's box or its arrow.
    Drop,
}

/// The height of a line of a list, in EMU (15 pt).
pub const LINE: i64 = 190_500;

/// A scroll bar's thumb: its start and length along the track `length` long between the arrows.
pub fn thumb(c: &FormControl, length: i64, square: i64) -> (i64, i64) {
    let track = (length - 2 * square).max(1);
    let range = (c.max - c.min).max(1);
    let size = square.min(track);
    let at = square + ((c.value.clamp(c.min, c.max) - c.min) as f64 / range as f64 * (track - size) as f64) as i64;
    (at, size)
}

/// The value a scroll bar's thumb reaches when dragged to `along` EMU from the start of the bar,
/// having been grabbed `grab_offset` EMU from the thumb's start.
pub fn thumb_value(c: &FormControl, length: i64, square: i64, along: i64, grab_offset: i64) -> i64 {
    let track = (length - 2 * square).max(1);
    let size = square.min(track);
    let movable = (track - size).max(1);
    let pos = (along - grab_offset - square).clamp(0, movable);
    let range = (c.max - c.min).max(0);
    (c.min + ((pos as f64 / movable as f64) * range as f64).round() as i64).clamp(c.min, c.max)
}

/// What a press at (`x`, `y`) EMU from the control's top left is on, in a control `width` by `height`.
pub fn hit(c: &FormControl, width: i64, height: i64, x: i64, y: i64) -> Option<ControlHit> {
    if x < 0 || y < 0 || x > width || y > height {
        return None;
    }
    Some(match c.kind {
        ControlKind::Scroll => {
            let (along, across) = if c.horizontal { (x, height) } else { (y, width) };
            let length = if c.horizontal { width } else { height };
            let square = across.min(length / 2).max(1);
            if along < square {
                ControlHit::Arrow(-1)
            } else if along > length - square {
                ControlHit::Arrow(1)
            } else {
                let (at, size) = thumb(c, length, square);
                if along < at {
                    ControlHit::Track(-1)
                } else if along > at + size {
                    ControlHit::Track(1)
                } else {
                    ControlHit::Whole
                }
            }
        }
        // The upper half raises the value, the lower half lowers it.
        ControlKind::Spin => ControlHit::Arrow(if y < height / 2 { 1 } else { -1 }),
        ControlKind::Drop => ControlHit::Drop,
        ControlKind::List => {
            let i = (y / LINE.max(1)) as usize;
            if i < c.items.len() { ControlHit::Item(i) } else { ControlHit::Whole }
        }
        _ => ControlHit::Whole,
    })
}

/// The shapes (their places in the slide) of the option buttons in the group of the one at `index`: from the
/// one marked first to the next marked first.
pub fn radio_group(slide: &crate::model::Slide, index: usize) -> Vec<usize> {
    let radios: Vec<(usize, bool)> = slide.shapes.iter().enumerate().filter_map(|(i, s)| match &s.kind {
        ShapeKind::Control(c) if c.kind == ControlKind::Radio => Some((i, c.first_button)),
        _ => None,
    }).collect();
    let Some(at) = radios.iter().position(|&(i, _)| i == index) else { return Vec::new() };
    let start = radios[..=at].iter().rposition(|&(_, first)| first).unwrap_or(0);
    let end = radios[at + 1..].iter().position(|&(_, first)| first).map_or(radios.len(), |p| at + 1 + p);
    radios[start..end].iter().map(|&(i, _)| i).collect()
}
