// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The slide show's pointer modes: the window's actions (laser, pen, highlighter, arrow, eraser) name them, the presentation's slide
//! show (`pptx/slideshow.rs`) acts on them.

/// The pointer mode during the slide show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PointerMode {
    /// Normal arrow pointer: click advances slides or activates links.
    #[default]
    Arrow,
    /// Laser pointer: moves a bright red dot, clicks do not advance.
    Laser,
    /// Pen drawing mode: drag draws freehand ink strokes.
    Pen,
    /// Highlighter drawing mode: drag draws translucent wide highlight strokes.
    Highlighter,
    /// Eraser mode: click removes nearby ink strokes.
    Eraser,
}
