// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A cell's comment shown while the pointer is on the cell, as in Excel:
//! a threaded comment's posts (who, when, what; replies under the first),
//! (A note's card is notes.rs's.)

use std::rc::Rc;

use gpui_kit::component::{ActiveTheme as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use crate::shell::palette::rgb;

use crate::shell::editor::SlideEditor;
use crate::ext::*;

/// The popup of a comment: a card as tall as its thread, at most as tall as the window allows, which scrolls (the pointer on
/// it scrolls it), at the right of the mark. A component of its own to know the window's height.
#[derive(IntoElement)]
struct CommentPopup {
    list: AnyElement,
    scroll: ScrollHandle,
    at: Point<Pixels>,
    bounds: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    hover: Rc<dyn Fn(&bool, &mut Window, &mut App)>,
}

impl RenderOnce for CommentPopup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        // The most the window holds: its height less the margin at both ends and the card's border (the position snaps
        // into the window, so a card as tall as this starts at the margin).
        let max_height = window.viewport_size().height - px(WINDOW_MARGIN * 2. + 2.);
        let card = div()
            .id("comment-card")
            .relative()
            .w(px(CARD_WIDTH))
            .bg(rgb(crate::shell::comments::THREAD_BG))
            .border_1()
            .border_color(theme.border)
            .rounded_xl()
            .shadow_lg()
            .overflow_hidden()
            .occlude()
            .on_hover(move |hovered, window, cx| (self.hover)(hovered, window, cx))
            .child(div().id("comment-card-scroll").w_full().max_h(max_height).overflow_y_scroll().track_scroll(&self.scroll).child(self.list))
            .child(crate::shell::chrome::always_visible_scrollbar_overlay("comment-card-scrollbar", self.scroll, None))
            .child(canvas(|_, _, _| {}, move |b, _, _, _| self.bounds.set(Some(b))).absolute().top_0().left_0().size_full());
        deferred(anchored().position(self.at).snap_to_window_with_margin(px(WINDOW_MARGIN)).child(card)).with_priority(1)
    }
}

/// `2026-10-07T09:30:00.00` as `2026/10/07 09:30` (the comments' one writing of a time, shell/comments.rs).
pub(crate) fn post_time(stamp: &str) -> String {
    crate::shell::comments::comment_date_label(stamp)
}

/// The pointer on a comment's mark for this long (Excel's feel of "reading it", the user's rule 2026-11-03, 1 s) earns the
/// popup a grace time after the pointer leaves the mark.
pub(crate) const HOVER_LONG_MS: u64 = 1000;
/// How long the popup stays after the pointer left the mark of a comment it was on for `HOVER_LONG_MS` (0.5 s), time to
/// reach the popup; the pointer on the popup keeps it.
pub(crate) const GRACE_MS: u64 = 500;
/// The space kept between the popup and the window's edges (the popup is as tall as the window allows less this at both ends).
const WINDOW_MARGIN: f32 = 8.;
/// The popup's width.
const CARD_WIDTH: f32 = 260.;

/// What the popup of a comment remembers of the pointer: whether it was on the mark long enough, and the timers (dropped
/// when replaced, which cancels them). The timers run on the executor's clock, which a scenario advances.
#[derive(Default)]
pub struct PopupTimers {
    long: bool,
    /// The pointer's last move was on a comment's mark.
    on_mark: bool,
    /// Where the popup was drawn (for the scenarios).
    pub bounds: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    long_timer: Option<Task<()>>,
    grace_timer: Option<Task<()>>,
}

#[ibis_office_macros::editor_ext(CommentCard)]
impl SlideEditor {
    /// The pointer moved with no button down: `position` is on a comment's mark (the popup of that cell shows), or not.
    /// The pointer on the popup itself does not come here (the popup hides the sheet from the pointer, `popup_hovered`).
    pub(crate) fn comment_pointer_moved(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        // On the popup itself (its bounds as drawn, with its 1 px border): it stays, whatever the sheet under it is.
        if self.sheet_state().is_some_and(|s| s.comment_hover.is_some())
            && let Some(card) = self.wb().comment_popup.bounds.get()
            && card.dilate(px(1.)).contains(&position)
        {
            let popup = &mut self.wb_mut().comment_popup;
            popup.grace_timer = None;
            popup.on_mark = false;
            return;
        }
        let mark = self.comment_mark_under(position);
        // (A note is shown by the pointer on the selected cell, notes.rs.)
        let marked = mark.filter(|&(c, r)| self.current_sheet().is_some_and(|(_, s)| s.threads.iter().any(|t| (t.column, t.row) == (c, r))));
        let Some(state) = self.sheet_state() else { return };
        let shown = state.comment_hover;
        self.wb_mut().comment_popup.on_mark = marked.is_some();
        match marked {
            Some(cell) => {
                self.wb_mut().comment_popup.grace_timer = None;
                if shown != Some(cell) {
                    self.show_comment_card(Some(cell), cx);
                    // The time on this mark is counted from now.
                    let timer = cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(std::time::Duration::from_millis(HOVER_LONG_MS)).await;
                        this.update(cx, |this, _| {
                            if this.sheet_state().is_some_and(|s| s.comment_hover == Some(cell))
                                && let Some(s) = this.sheet_state_mut()
                            {
                                s.comment_popup.long = true;
                            }
                        })
                        .ok();
                    });
                    let popup = &mut self.wb_mut().comment_popup;
                    popup.long = false;
                    popup.long_timer = Some(timer);
                }
            }
            None if shown.is_some() => {
                if self.wb().comment_popup.long { self.start_grace(cx) } else { self.hide_comment_card(cx) }
            }
            None => {}
        }
    }

    /// Hide the popup after the grace time, unless the pointer comes back to the mark or onto the popup (they cancel it).
    fn start_grace(&mut self, cx: &mut Context<Self>) {
        if self.wb().comment_popup.grace_timer.is_some() {
            return;
        }
        let timer = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(GRACE_MS)).await;
            this.update(cx, |this, cx| this.hide_comment_card(cx)).ok();
        });
        self.wb_mut().comment_popup.grace_timer = Some(timer);
    }

    /// Hide the popup now and forget the pointer's time on the mark.
    fn hide_comment_card(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.comment_popup = PopupTimers::default();
        }
        self.show_comment_card(None, cx);
    }

    /// The pointer is on the popup (it stays) or left it (it goes after the grace time, unless the pointer is on a mark).
    fn popup_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
        if hovered {
            self.wb_mut().comment_popup.grace_timer = None;
        } else if self.sheet_state().is_some_and(|s| s.comment_hover.is_some() && !s.comment_popup.on_mark) {
            self.start_grace(cx);
        }
    }

    /// Show the comment of `cell` (none: hide), if it has one.
    pub(crate) fn show_comment_card(&mut self, cell: Option<(u32, u32)>, cx: &mut Context<Self>) {
        let has = |(c, r): (u32, u32)| self.current_sheet().is_some_and(|(_, s)| s.threads.iter().any(|t| (t.column, t.row) == (c, r)));
        let shown = cell.filter(|&c| has(c));
        if let Some(s) = self.sheet_state_mut()
            && s.comment_hover != shown
        {
            s.comment_hover = shown;
            s.comment_scroll.set_offset(Point::default());
            cx.notify();
        }
    }

    /// The window position of a cell's top-right corner.
    pub fn cell_top_right(&self, column: u32, row: u32) -> Option<Point<Pixels>> {
        let layout = self.slide_layout()?;
        let (shape, sheet) = self.current_sheet()?;
        let (xs, ys) = (sheet.column_offsets(), sheet.row_offsets());
        let last_column = sheet.merges.iter().find(|m| (m.first_column, m.first_row) == (column, row)).map_or(column, |m| m.last_column);
        let x = shape.frame.x + xs.get(last_column as usize + 1)?;
        let y = shape.frame.y + ys.get(row as usize)?;
        Some(point(layout.origin.x + layout.to_px(x), layout.origin.y + layout.to_px(y)))
    }

    /// The card of the comment under the pointer.
    pub(crate) fn render_comment_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (column, row) = self.sheet_state()?.comment_hover?;
        if self.wb().filter_panel.is_some() || self.cell_edit().is_some() {
            return None;
        }
        let (_, sheet) = self.current_sheet()?;
        let theme = cx.theme().clone();
        // One rounded white rectangle for each post (who, when, what); the user's own sit a little in (as in the panel).
        let post_box = || v_flex().gap(px(2.)).p_2().rounded_lg().bg(rgb(0xffffff)).border_1().border_color(rgb(crate::shell::comments::POST_BORDER));
        let mut list = v_flex().p_2().gap_2().text_size(px(12.));
        if let Some(t) = sheet.threads.iter().find(|t| (t.column, t.row) == (column, row)) {
            for post in t.posts.iter() {
                list = list.child(
                    div().w_full().when(crate::shell::comments::is_own(&post.author), |d| d.pl(crate::shell::comments::OWN_INDENT)).child(
                        post_box()
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(post.author.clone()))
                            .child(div().text_color(theme.muted_foreground).child(post_time(&post.date)))
                            .child(div().child(post.text.clone())),
                    ),
                );
            }
            if t.done {
                list = list.child(div().text_color(theme.muted_foreground).child(crate::shell::i18n::text("_xlsx.resolved")));
            }
        } else {
            return None;
        }
        let scroll = self.sheet_state()?.comment_scroll.clone();
        // The popup's left edge is the right edge of the square that holds the mark (the cell's right edge), its top the
        // cell's top: nothing between the mark and the popup, so the pointer goes across without leaving both.
        let at = self.cell_top_right(column, row)?;
        let owner = cx.weak_entity();
        let hover: Rc<dyn Fn(&bool, &mut Window, &mut App)> = Rc::new(move |hovered, _, cx| {
            owner.update(cx, |this, cx| this.popup_hovered(*hovered, cx)).ok();
        });
        let bounds = self.wb().comment_popup.bounds.clone();
        Some(CommentPopup { list: list.into_any_element(), scroll, at, bounds, hover }.into_any_element())
    }

    /// Move to the next or previous comment/note on the active sheet (wrapping around).
    pub(crate) fn select_next_comment(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some((_, sheet)) = self.current_sheet() else { return };
        let mut cells: Vec<(u32, u32)> = sheet
            .threads
            .iter()
            .map(|t| (t.column, t.row))
            .chain(sheet.comments.iter().map(|c| (c.column, c.row)))
            .collect();
        if cells.is_empty() {
            return;
        }
        cells.sort_by_key(|&(c, r)| (r, c));
        cells.dedup();

        let cur = self.current_cell();
        let target = if forward {
            if let Some((cc, cr)) = cur {
                cells.iter().find(|&&(c, r)| (r, c) > (cr, cc)).copied().unwrap_or(cells[0])
            } else {
                cells[0]
            }
        } else {
            if let Some((cc, cr)) = cur {
                cells.iter().rfind(|&&(c, r)| (r, c) < (cr, cc)).copied().unwrap_or(*cells.last().unwrap())
            } else {
                *cells.last().unwrap()
            }
        };

        self.set_cell(target.0, target.1);
        self.scroll_range_into_view(true);
        if self.note_of(target.0, target.1).is_some() {
            self.show_note_card(target, cx);
        } else {
            self.show_comment_card(Some(target), cx);
        }
    }

    /// Toggle displaying all note cards simultaneously on the sheet.
    pub(crate) fn toggle_show_all_notes(&mut self, cx: &mut Context<Self>) {
        if let Some(s) = self.sheet_state_mut() {
            s.show_all_notes = !s.show_all_notes;
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::post_time;

    #[test]
    fn post_times_as_shown() {
        assert_eq!(post_time("2026-10-07T09:30:00.00"), "2026/10/07 09:30");
        assert_eq!(post_time("2026-10-07"), "2026/10/07");
    }
}
