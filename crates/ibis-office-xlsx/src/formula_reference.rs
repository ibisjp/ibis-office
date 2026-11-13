// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Formula reference entry, painted ranges and resizing share the token spans.
use crate::shell::editor::SlideEditor;
use gpui_kit::*;
use ibis_calc::formula::{
    lexer::{Op, Token, tokenize},
    parser::parse_cell,
};
use ibis_doc_model::CellRange;
use crate::ext::*;
pub(crate) const COLORS: [u32; 7] = [
    0x1769E0, 0xD43B36, 0x8D47B5, 0x278D4A, 0xD27B00, 0x0096A6, 0xB33D8F,
];
#[derive(Clone)]
pub struct Reference {
    pub start: usize,
    pub end: usize,
    pub range: CellRange,
    pub sheet: Option<String>,
    pub color: u32,
}
pub fn references(text: &str) -> Vec<Reference> {
    if !is_formula(text){return Vec::new();}
    let offset=usize::from(text.starts_with('='));
    let formula=&text[offset..];
    let Ok(tokens) = tokenize(formula) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let mut sheet = None;
        let start = tokens[i].start;
        if let Token::Sheet {
            first,
            last: None,
            book: None,
            ..
        } = &tokens[i].token
        {
            sheet = Some(first.clone());
            i += 1;
            if i == tokens.len() {
                break;
            }
        }
        if let Token::Word(word) = &tokens[i].token
            && let Some(first) = parse_cell(word)
        {
            let mut last = first;
            let mut end = tokens[i].end;
            if i + 2 < tokens.len()
                && tokens[i + 1].token == Token::Op(Op::Range)
                && let Token::Word(word) = &tokens[i + 2].token
                && let Some(corner) = parse_cell(word)
            {
                last = corner;
                i += 2;
                end = tokens[i].end;
            }
            out.push(Reference {
                start: start + offset,
                end: end + offset,
                range: CellRange {
                    first_row: first.row.min(last.row),
                    last_row: first.row.max(last.row),
                    first_column: first.column.min(last.column),
                    last_column: first.column.max(last.column),
                },
                sheet,
                color: COLORS[out.len() % COLORS.len()],
            });
        }
        i += 1;
    }
    out
}
pub(crate) fn close_parentheses(text: &str) -> String {
    let formula = text.strip_prefix('=').unwrap_or(text);
    let Ok(tokens) = tokenize(formula) else {
        return text.to_string();
    };
    let mut depth = 0usize;
    for token in tokens {
        match token.token {
            Token::LParen => depth += 1,
            Token::RParen => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    format!("{text}{}", ")".repeat(depth))
}

#[ibis_office_macros::editor_ext(FormulaReference)]
impl SlideEditor {
    pub(crate) fn formula_reference_key(
        &mut self,
        dx: i32,
        dy: i32,
        extend: bool,
        edge: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(edit) = self.cell_edit() else {
            return false;
        };
        // Excel points only in Enter mode; in Edit mode (F2, a double-click) the arrows move the caret.
        if edit.mode == super::cell_edit::EditMode::Edit || !is_formula(&edit.text) || edit.marked.is_some() {
            return false;
        }
        let caret = edit.caret_byte();
        let replacing = edit.pointed.filter(|(_, end, _)| *end == caret);
        if replacing.is_none()
            && !edit.text[..caret]
                .trim_end()
                .ends_with(['=', '(', ',', '+', '-', '*', '/', '^', '&', '<', '>', ':'])
        {
            return false;
        }
        let anchor = replacing.map_or((edit.column, edit.row), |(_, _, anchor)| anchor);
        let current = replacing
            .and_then(|(start, end, _)| {
                references(&format!("={}", &edit.text[start..end]))
                    .first()
                    .map(|r| {
                        (
                            if anchor.0 > r.range.first_column {
                                r.range.first_column
                            } else {
                                r.range.last_column
                            },
                            if anchor.1 > r.range.first_row {
                                r.range.first_row
                            } else {
                                r.range.last_row
                            },
                        )
                    })
            })
            .unwrap_or(anchor);
        let Some((_, sheet)) = self.current_sheet() else {
            return false;
        };
        let step = |n: u32, d: i32, max: u32| (n as i64 + d as i64).clamp(0, max as i64) as u32;
        let to = if edge {
            let filled = |c, r| sheet.cell(r, c).is_some_and(|cell| !cell.text.is_empty());
            if dx != 0 {
                (
                    super::sheet_ui::data_edge(current.0, dx, &sheet.columns, |c| {
                        filled(c, current.1)
                    }),
                    current.1,
                )
            } else {
                (
                    current.0,
                    super::sheet_ui::data_edge(current.1, dy, &sheet.rows, |r| {
                        filled(current.0, r)
                    }),
                )
            }
        } else {
            (step(current.0, dx, 16383), step(current.1, dy, 1048575))
        };
        if extend && replacing.is_some() {
            self.finish_pointing(to.0, to.1, cx);
        } else {
            self.point_reference(to.0, to.1, cx);
        }
        true
    }
    pub(crate) fn move_formula_reference_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.cell_edit().is_some_and(|e| e.reference_drag) {
            return false;
        }
        if event.pressed_button == Some(MouseButton::Left) {
            if let Some((c, r)) = self.cell_under(event.position) {
                self.finish_pointing(c, r, cx);
            }
        } else if let Some(edit) = self.cell_edit_mut() {
            edit.reference_drag = false;
        }
        true
    }
    pub(crate) fn render_formula_references(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let edit = self.cell_edit()?;
        let refs = references(&edit.text);
        if refs.is_empty() {
            return None;
        }
        let sheet_name = self.slide().name.as_deref().unwrap_or("");
        let mut layers = Vec::new();
        for reference in refs.into_iter().filter(|r| {
            r.sheet
                .as_deref()
                .is_none_or(|s| s.eq_ignore_ascii_case(sheet_name))
        }) {
            let r = reference.range;
            let end = self.cell_corner(r.last_column, r.last_row)?;
            let first_end = self.cell_corner(r.first_column, r.first_row)?;
            let first_center = self.cell_center(r.first_column, r.first_row)?;
            let start = first_center * 2. - first_end;
            let mut tint = rgba((reference.color << 8) | 24);
            tint.a = 0.09;
            let color = rgb(reference.color);
            let w = end.x - start.x;
            let h = end.y - start.y;
            let outline = canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    window.request_animation_frame();
                    let phase = crate::clock::unix_millis() % 800;
                    let offset = phase as f32 / 100.;
                    let width = f32::from(bounds.size.width);
                    let height = f32::from(bounds.size.height);
                    for (length, horizontal, far) in [
                        (width, true, false),
                        (width, true, true),
                        (height, false, false),
                        (height, false, true),
                    ] {
                        let mut p = -8. + offset;
                        while p < length {
                            let a = p.max(0.);
                            let b = (p + 4.).min(length);
                            if b > a {
                                let (x, y, ww, hh) = if horizontal {
                                    (a, if far { height - 1. } else { 0. }, b - a, 1.)
                                } else {
                                    (if far { width - 1. } else { 0. }, a, 1., b - a)
                                };
                                window.paint_quad(fill(
                                    Bounds::new(
                                        bounds.origin + point(px(x), px(y)),
                                        size(px(ww), px(hh)),
                                    ),
                                    color,
                                ));
                            }
                            p += 8.;
                        }
                    }
                },
            )
            .absolute()
            .size_full();
            let mut layer = div().relative().w(w).h(h).bg(tint).child(outline);
            for (x, y, opposite) in [
                (px(0.), px(0.), (r.last_column, r.last_row)),
                (w, px(0.), (r.first_column, r.last_row)),
                (px(0.), h, (r.last_column, r.first_row)),
                (w, h, (r.first_column, r.first_row)),
            ] {
                let span = (reference.start, reference.end);
                layer = layer.child(
                    div()
                        .absolute()
                        .left(x - px(3.))
                        .top(y - px(3.))
                        .size(px(6.))
                        .bg(color)
                        .cursor_crosshair()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |e, _, _, cx| {
                                if let Some(edit) = e.cell_edit_mut() {
                                    edit.pointed = Some((span.0, span.1, opposite));
                                    edit.caret = edit.text[..span.1].encode_utf16().count();
                                    edit.reference_drag = true;
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                );
            }
            layers.push(
                deferred(anchored().position(start).child(layer))
                    .with_priority(1)
                    .into_any_element(),
            );
        }
        Some(div().children(layers).into_any_element())
    }
}

pub(crate) fn color_runs(text: &str, start: usize, len: usize, base: TextRun) -> Vec<TextRun> {
    let refs = references(text);
    let end = start + len;
    let mut cuts = vec![start, end];
    for r in &refs {
        if r.start < end && r.end > start {
            cuts.push(r.start.max(start));
            cuts.push(r.end.min(end));
        }
    }
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2)
        .map(|span| {
            let mut run = base.clone();
            run.len = span[1] - span[0];
            if let Some(r) = refs.iter().find(|r| span[0] >= r.start && span[0] < r.end) {
                run.color = rgb(r.color).into();
            }
            run
        })
        .collect()
}

pub(crate) fn is_formula(text:&str)->bool {text.starts_with(['=','+','-'])}

/// A cell entry typed as `@sum(a1:a2)` is the formula `=sum(a1:a2)` (GitHub issue #148; Excel 365 measured by keys on a Windows PC,
/// tasks/doc/m60-excel-at-input.md): when the text starts with `@` and then a name of a function we know and `(`, the `@` becomes `=`.
/// Excel refuses every other `@…` with an error dialog (`@1+1`, `@a1`, `@abc`, an unknown name, `@sum` without a bracket, `@ sum(..)`);
/// ibisOffice takes it as text so that a name such as `@user` can be typed (2026-11-11 user decision; measured in tasks/doc/m60-excel-at-input.md).
/// `None` for every other `@…` (`@1+1`, `@a1`, `@abc`, a name that is no function, `@` alone), which stay text as they were. Only what is
/// typed goes through here: a paste, a file, a CSV and a program's assignment keep their `@` (Excel keeps it as text there too).
pub(crate) fn at_entry_to_formula(text: &str) -> Option<String> {
    // The full-width `\u{ff20}` is the same key as `@` for Excel (measured, #M61).
    let rest = text.strip_prefix(['@', '\u{ff20}'])?;
    let name_end = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '.' || c == '_')).unwrap_or(rest.len());
    let (name, after) = rest.split_at(name_end);
    if name.is_empty() || !after.starts_with('(') {
        return None;
    }
    ibis_calc::functions::table::function_info(&name.to_ascii_uppercase())?;
    Some(format!("={rest}"))
}

#[cfg(test)]
mod at_entry_tests {
    use super::at_entry_to_formula;

    #[test]
    fn an_at_and_a_function_make_a_formula() {
        assert_eq!(at_entry_to_formula("@sum(a1:a2)").as_deref(), Some("=sum(a1:a2)"));
        assert_eq!(at_entry_to_formula("@SUM(A1:A2)").as_deref(), Some("=SUM(A1:A2)"));
        assert_eq!(at_entry_to_formula("@today()").as_deref(), Some("=today()"));
        assert_eq!(at_entry_to_formula("@sum(a1:a2)+1").as_deref(), Some("=sum(a1:a2)+1"));
        assert_eq!(at_entry_to_formula("@pi()*2").as_deref(), Some("=pi()*2"));
        assert_eq!(at_entry_to_formula("@if(a1>0,\"y\",\"n\")").as_deref(), Some("=if(a1>0,\"y\",\"n\")"));
        // The full-width mark is the same as `@`.
        assert_eq!(at_entry_to_formula("\u{ff20}sum(a1:a2)").as_deref(), Some("=sum(a1:a2)"));
        // A missing closing bracket is supplied as it is for a typed `=`.
        let open = at_entry_to_formula("@sum(a1:a2").unwrap();
        assert_eq!(super::close_parentheses(&open), "=sum(a1:a2)");
    }

    /// Excel refuses these with an error dialog; ibisOffice keeps them as text (2026-11-11 user decision).
    #[test]
    fn everything_else_stays_as_typed() {
        for text in ["@1+1", "@1", "@(1+1)", "@-1", "@\"abc\"", "@@sum(a1:a2)", "@a1", "@a1+a2", "@abc", "@abc123", "@user@example.com", "@a1:a2", "@", "@foo(1)", "sum(a1)", "=sum(a1)", "+sum(a1:a2)", "@ sum(a1:a2)", "@sum", "@a1("] {
            assert_eq!(at_entry_to_formula(text), None, "{text}");
        }
    }
}
