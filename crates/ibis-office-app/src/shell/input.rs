// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Text input from the system (typing, the Japanese input method) handed to
//! the open document's format.
//!
//! An application can have one `EntityInputHandler` for `SlideEditor`, and
//! the three formats all take text (a document's paragraphs, a slide's text
//! boxes, a worksheet's cells). So the shell registers an input handler of
//! its own ([`FormatInput`]) over the document area, and it asks the
//! format through `FormatEditor`'s input hooks (`text_for_range`,
//! `replace_text_in_range`, …, all with defaults that take nothing). It is
//! registered only while `FormatEditor::wants_text_input` says so.
//!
//! Positions are in UTF-16 code units of the text the format shows the
//! input method (a paragraph, a cell), as the system's API has them.

use std::ops::Range;

use gpui_kit::*;

use crate::shell::editor::SlideEditor;
use crate::shell::format::with_format;

/// The shell's input handler: every call goes to the format's hooks.
pub(crate) struct FormatInput {
    editor: Entity<SlideEditor>,
    bounds: Bounds<Pixels>,
}

impl FormatInput {
    pub(crate) fn new(editor: Entity<SlideEditor>, bounds: Bounds<Pixels>) -> Self {
        Self { editor, bounds }
    }
}

impl InputHandler for FormatInput {
    fn selected_text_range(&mut self, ignore_disabled_input: bool, window: &mut Window, cx: &mut App) -> Option<UTF16Selection> {
        self.editor.update(cx, |e, cx| with_format!(e, selected_text_range(e, ignore_disabled_input, window, cx)))
    }

    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.editor.update(cx, |e, cx| with_format!(e, marked_text_range(e, window, cx)))
    }

    fn text_for_range(&mut self, range_utf16: Range<usize>, adjusted_range: &mut Option<Range<usize>>, window: &mut Window, cx: &mut App) -> Option<String> {
        self.editor.update(cx, |e, cx| with_format!(e, text_for_range(e, range_utf16, adjusted_range, window, cx)))
    }

    fn replace_text_in_range(&mut self, replacement_range: Option<Range<usize>>, text: &str, window: &mut Window, cx: &mut App) {
        self.editor.update(cx, |e, cx| { if !e.panel_dialog_is_modal() { with_format!(e, replace_text_in_range(e, replacement_range, text, window, cx)); } })
    }

    fn replace_and_mark_text_in_range(&mut self, range_utf16: Option<Range<usize>>, new_text: &str, new_selected_range: Option<Range<usize>>, window: &mut Window, cx: &mut App) {
        self.editor.update(cx, |e, cx| { if !e.panel_dialog_is_modal() { with_format!(e, replace_and_mark_text_in_range(e, range_utf16, new_text, new_selected_range, window, cx)); } })
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut App) {
        self.editor.update(cx, |e, cx| with_format!(e, unmark_text(e, window, cx)))
    }

    fn bounds_for_range(&mut self, range_utf16: Range<usize>, window: &mut Window, cx: &mut App) -> Option<Bounds<Pixels>> {
        let bounds = self.bounds;
        self.editor.update(cx, |e, cx| with_format!(e, bounds_for_range(e, range_utf16, bounds, window, cx)))
    }

    fn character_index_for_point(&mut self, point: Point<Pixels>, window: &mut Window, cx: &mut App) -> Option<usize> {
        self.editor.update(cx, |e, cx| with_format!(e, character_index_for_point(e, point, window, cx)))
    }
}

/// An element over the document area that, while the format wants text,
/// registers [`FormatInput`] for the editor's focus when painted.
pub(crate) fn input_region(cx: &Context<SlideEditor>) -> impl IntoElement {
    let editor = cx.entity();
    canvas(
        |_, _, _| {},
        move |bounds, _, window, cx| {
            let (wants, focus) = editor.update(cx, |e, cx| (!e.panel_dialog_is_modal() && with_format!(e, wants_text_input(e, window, cx)), e.focus_handle.clone()));
            if wants {
                window.handle_input(&focus, FormatInput::new(editor.clone(), bounds), cx);
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
