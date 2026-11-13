// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! An element whose text is only rasterized, not drawn: the slide show paints the next slides' text through it once (before their
//! animations play), so that the glyphs a moving text needs at each of its subpixel positions are already in the atlas. Where glyphs are
//! rasterized by the browser's canvas (the web version) the first run of an entrance animation such as スローイン otherwise stalls for
//! 50 to 100 ms in two frames (issue 128). See `Window::set_prewarm_glyph_variants` of the gpui fork.

use gpui_kit::*;

pub(crate) struct GlyphPrewarm {
    child: AnyElement,
}

impl GlyphPrewarm {
    pub(crate) fn new(child: impl IntoElement) -> Self {
        Self { child: child.into_any_element() }
    }
}

impl IntoElement for GlyphPrewarm {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for GlyphPrewarm {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }
    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }
    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        self.child.prepaint(window, cx);
    }
    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        window.set_prewarm_glyph_variants(true);
        self.child.paint(window, cx);
        window.set_prewarm_glyph_variants(false);
    }
}
