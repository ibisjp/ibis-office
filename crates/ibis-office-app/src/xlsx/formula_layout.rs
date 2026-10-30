// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Formula-bar visual lines shared by painting, pointer mapping and navigation.
use super::cell_edit::{BAR_LINE_HEIGHT, byte_at};
use crate::shell::editor::SlideEditor;
use gpui_kit::*;
const HEIGHT_KEY: &str = "xlsx.formula-bar.height";
pub(crate) fn shape_bar_line(text: &str, w: &Window) -> ShapedLine {
    let style = w.text_style();
    let run = TextRun {
        len: text.len(),
        font: style.font(),
        color: style.color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    w.text_system()
        .shape_line(text.to_owned().into(), w.rem_size() * 0.875, &[run], None)
}
pub(crate) fn visual_lines(text: &str, width: f32, w: &Window) -> Vec<(usize, String)> {
    let style = w.text_style();
    let size = w.rem_size() * 0.875;
    let mut out = Vec::new();
    for (start, line) in super::cell_edit::text_lines(text) {
        let run = TextRun {
            len: line.len(),
            font: style.font(),
            color: style.color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = w
            .text_system()
            .shape_line(line.to_owned().into(), size, &[run], None);
        let mut begin = 0;
        let mut x = px(0.);
        for (at, ch) in line.char_indices() {
            let end = at + ch.len_utf8();
            let right = shaped.x_for_index(end);
            if at > begin && f32::from(right - x) > width.max(1.) {
                out.push((
                    start + line[..begin].encode_utf16().count(),
                    line[begin..at].to_owned(),
                ));
                begin = at;
                x = shaped.x_for_index(at);
            }
        }
        out.push((
            start + line[..begin].encode_utf16().count(),
            line[begin..].to_owned(),
        ));
    }
    out
}
impl SlideEditor {
    pub(crate) fn cell_editor_width(&self, x: f32) -> f32 {
        self.workspace_bounds
            .zip(self.slide_layout())
            .map_or(f32::MAX, |(bounds, layout)| {
                (f32::from(bounds.right() - layout.origin.x) - x - 14.).max(1.)
            })
    }
    fn formula_bar_max_height(&self, w: &Window) -> f32 {
        let Some(bar) = self.wb().bar_bounds.get() else {
            return super::sheet_ui::BAR_EXPANDED_HEIGHT;
        };
        let top = f32::from(bar.top());
        let bottom = self
            .workspace_bounds
            .map_or(f32::from(w.viewport_size().height), |b| {
                f32::from(b.bottom())
            });
        // Reserve the requested 100px table, 6px splitter and 4px bar padding.
        (bottom - top - 100. - 6. - 4.).max(BAR_LINE_HEIGHT + 4.)
    }
    pub(crate) fn formula_bar_height(&self, w: &Window) -> f32 {
        self.document_memory
            .get::<f32>(HEIGHT_KEY)
            .unwrap_or(super::sheet_ui::BAR_EXPANDED_HEIGHT)
            .clamp(BAR_LINE_HEIGHT + 4., self.formula_bar_max_height(w))
    }
    pub(crate) fn formula_bar_splitter(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let editor = cx.weak_entity();
        div()
            .id("formula-bar-splitter")
            .h(px(6.))
            .w_full()
            .flex_none()
            .cursor(CursorStyle::ResizeUpDown)
            .child(
                canvas(
                    |b, w, _| w.insert_hitbox(b, HitboxBehavior::Normal),
                    move |_, hit, w, cx| {
                        let state = w.use_keyed_state("formula-bar-resize", cx, |_, _| {
                            std::cell::Cell::new(None::<(f32, f32)>)
                        });
                        let down = editor.clone();
                        let down_state = state.clone();
                        let hit_id = hit.id;
                        w.on_mouse_event(move |ev: &MouseDownEvent, phase, w, cx| {
                            if phase == DispatchPhase::Bubble
                                && ev.button == MouseButton::Left
                                && hit.is_hovered(w)
                            {
                                let _ = down.update(cx, |e, cx| {
                                    down_state.read(cx).set(Some((
                                        f32::from(ev.position.y),
                                        e.formula_bar_height(w),
                                    )));
                                    w.capture_pointer(hit_id);
                                    cx.stop_propagation();
                                });
                            }
                        });
                        let moving = editor.clone();
                        let moved_state = state.clone();
                        w.on_mouse_event(move |ev: &MouseMoveEvent, phase, w, cx| {
                            if phase == DispatchPhase::Capture {
                                if let Some((y, h)) = moved_state.read(cx).get() {
                                    let _ = moving.update(cx, |e, cx| {
                                        let max = e.formula_bar_max_height(w);
                                        e.document_memory.set(
                                            HEIGHT_KEY,
                                            (h + f32::from(ev.position.y) - y)
                                                .clamp(BAR_LINE_HEIGHT + 4., max),
                                        );
                                        cx.notify();
                                        cx.stop_propagation();
                                    });
                                }
                            }
                        });
                        w.on_mouse_event(move |ev: &MouseUpEvent, phase, w, cx| {
                            if phase == DispatchPhase::Capture
                                && ev.button == MouseButton::Left
                                && state.read(cx).take().is_some()
                            {
                                w.release_pointer();
                                cx.stop_propagation();
                            }
                        });
                    },
                )
                .size_full(),
            )
    }
    pub(crate) fn formula_edit_key(
        &mut self,
        ev: &KeyDownEvent,
        w: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.focus_handle.is_focused(w)
            || self.panel_dialog_is_modal()
            || self.interaction_layer().is_confirmation()
        {
            return false;
        }
        self.formula_edit_command(ev.keystroke.key.as_str(), ev.keystroke.modifiers, cx)
    }
    pub(crate) fn formula_edit_command(
        &mut self,
        key: &str,
        m: Modifiers,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.panel_dialog_is_modal() || self.interaction_layer().is_confirmation() {
            return false;
        }
        let Some(e) = self.cell_edit() else {
            return false;
        };
        if e.marked.is_some() {
            return false;
        }
        let whole = m.control || m.platform;
        if e.pointed.is_some()
            && !e.dragging_in_bar
            && matches!(key, "left" | "right")
            && whole
            && m.shift
        {
            return false;
        }
        let len = e.text.encode_utf16().count();
        let caret = e.caret;
        let lines = if e.dragging_in_bar {
            self.wb().bar_visual_lines.clone()
        } else {
            e.layout
                .borrow()
                .as_ref()
                .map(|layout| {
                    layout
                        .lines
                        .iter()
                        .map(|line| {
                            (
                                e.text[..line.start].encode_utf16().count(),
                                e.text[line.start..line.end].to_owned(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![(0, e.text.clone())])
        };
        let row = self
            .wb()
            .bar_caret_row
            .filter(|&i| {
                lines
                    .get(i)
                    .is_some_and(|(a, l)| (*a..=*a + l.encode_utf16().count()).contains(&caret))
            })
            .unwrap_or_else(|| lines.iter().rposition(|(s, _)| *s <= caret).unwrap_or(0));
        let to = match key {
            "home" if whole => 0,
            "end" if whole => len,
            "home" => lines.get(row).map_or(0, |l| l.0),
            "end" => lines
                .get(row)
                .map_or(len, |l| l.0 + l.1.encode_utf16().count()),
            "up" | "down" if e.dragging_in_bar => {
                let target = if key == "up" {
                    row.saturating_sub(1)
                } else {
                    (row + 1).min(lines.len().saturating_sub(1))
                };
                if let Some((start, line)) = lines.get(target) {
                    let column = caret.saturating_sub(lines[row].0);
                    if let Some((current, target_shape)) = self
                        .wb()
                        .bar_shapes
                        .get(row)
                        .zip(self.wb().bar_shapes.get(target))
                    {
                        let x = current.x_for_index(byte_at(&lines[row].1, column));
                        let byte = target_shape.closest_index_for_x(x).min(line.len());
                        start + line[..byte].encode_utf16().count()
                    } else {
                        start + column.min(line.encode_utf16().count())
                    }
                } else {
                    caret
                }
            }
            "left" | "right" if whole && m.shift => {
                let b = byte_at(&e.text, caret);
                let mut at = b;
                let boundary = |c: char| !c.is_alphanumeric() && c != '_' && c != '$';
                if key == "left" {
                    let mut it = e.text[..b].char_indices().rev().peekable();
                    while let Some(&(i, c)) = it.peek() {
                        if !boundary(c) {
                            break;
                        }
                        at = i;
                        it.next();
                    }
                    while let Some((i, c)) = it.next() {
                        if boundary(c) {
                            break;
                        }
                        at = i;
                    }
                } else {
                    let mut it = e.text[b..].char_indices().peekable();
                    while let Some(&(i, c)) = it.peek() {
                        if !boundary(c) {
                            break;
                        }
                        at = b + i + c.len_utf8();
                        it.next();
                    }
                    while let Some((i, c)) = it.next() {
                        if boundary(c) {
                            break;
                        }
                        at = b + i + c.len_utf8();
                    }
                }
                e.text[..at].encode_utf16().count()
            }
            _ => return false,
        };
        let in_bar = e.dragging_in_bar;
        let hint = if !in_bar {
            None
        } else {
            match key {
                "home" | "end" if !whole => Some(row),
                "up" => Some(row.saturating_sub(1)),
                "down" => Some((row + 1).min(lines.len().saturating_sub(1))),
                _ => None,
            }
        };
        self.wb_mut().bar_caret_row = hint;
        let e = self.cell_edit_mut().unwrap();
        e.anchor = if m.shift {
            Some(e.anchor.unwrap_or(caret))
        } else {
            None
        };
        e.caret = to;
        e.pending = None;
        cx.notify();
        true
    }
}
