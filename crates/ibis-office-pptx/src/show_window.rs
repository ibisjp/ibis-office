// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A window of the slide show on its own display (the presenter's view or
//! the audience's slide show, with two displays or more): the one show of
//! the editor drawn in it, its keys and clicks going to that show.

use gpui_kit::*;

use super::slideshow::{SLIDESHOW_CONTEXT, ShowKind};
use crate::shell::editor::SlideEditor;
use crate::ext::*;

pub(crate) struct ShowView {
    editor: Entity<SlideEditor>,
    kind: ShowKind,
    focus: FocusHandle,
    _follow: Subscription,
}

impl ShowView {
    pub(crate) fn new(editor: Entity<SlideEditor>, kind: ShowKind, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        // Whatever the show does (a slide, an animation, a frame of a video, the clock) is drawn here too.
        let _follow = cx.observe(&editor, |_, _, cx| cx.notify());
        Self { editor, kind, focus, _follow }
    }
}

impl Render for ShowView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let kind = self.kind;
        let content = self.editor.update(cx, |editor, ecx| editor.render_slideshow_for(kind, window, ecx));
        div()
            .size_full()
            .bg(black())
            .key_context(SLIDESHOW_CONTEXT)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &crate::shell::actions::NextSlide, window, cx| this.editor.update(cx, |e, ecx| e.next_slide(&crate::shell::actions::NextSlide, window, ecx))))
            .on_action(cx.listener(|this, _: &crate::shell::actions::PreviousSlide, window, cx| this.editor.update(cx, |e, ecx| e.previous_slide(&crate::shell::actions::PreviousSlide, window, ecx))))
            .on_action(cx.listener(|this, _: &crate::shell::actions::FirstSlide, _, cx| this.editor.update(cx, |e, ecx| e.slideshow_to_end(false, ecx))))
            .on_action(cx.listener(|this, _: &crate::shell::actions::LastSlide, _, cx| this.editor.update(cx, |e, ecx| e.slideshow_to_end(true, ecx))))
            .on_action(cx.listener(|this, _: &crate::shell::actions::BlackScreen, _, cx| this.editor.update(cx, |e, ecx| e.blank_slideshow(0x000000, ecx))))
            .on_action(cx.listener(|this, _: &crate::shell::actions::WhiteScreen, _, cx| this.editor.update(cx, |e, ecx| e.blank_slideshow(0xffffff, ecx))))
            .on_action(cx.listener(|this, _: &crate::shell::actions::CancelTool, window, cx| this.editor.update(cx, |e, ecx| e.cancel_tool(&crate::shell::actions::CancelTool, window, ecx))))
            .children(content)
    }
}
