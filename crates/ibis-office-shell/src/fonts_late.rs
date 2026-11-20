// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Fonts that arrive while the app runs (GitHub issue #157): the browser version reads the downloaded fonts in the background, one at a time. Each is
//! given to the text system as it comes (`add_fonts_now`), but the windows are drawn again only once no other came for [`QUIET`]: fonts that arrive one
//! after another make one drawing, not one each.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::*;

/// How long no font has to come before the windows are drawn again.
pub const QUIET: Duration = Duration::from_millis(100);

/// The count of arrivals: a drawing waits for its own arrival to be the last.
#[derive(Clone, Default)]
pub struct Arrivals(Rc<Cell<u64>>);

impl Arrivals {
    /// A font came: its number.
    pub fn arrive(&self) -> u64 {
        self.0.set(self.0.get() + 1);
        self.0.get()
    }

    /// Whether `number` is the last arrival (nothing came after it).
    pub fn is_last(&self, number: u64) -> bool {
        self.0.get() == number
    }
}

/// `files` (font files) are given to the text system now; the windows draw again once no more come for [`QUIET`]. False when nothing was added.
pub fn fonts_arrived(cx: &mut App, arrivals: &Arrivals, files: Vec<Vec<u8>>) -> bool {
    if !crate::render::shape_view::add_fonts_now(cx, files) {
        return false;
    }
    let number = arrivals.arrive();
    let arrivals = arrivals.clone();
    cx.spawn(async move |cx: &mut AsyncApp| {
        cx.background_executor().timer(QUIET).await;
        if arrivals.is_last(number) {
            cx.update(|cx| cx.refresh_windows());
        }
    })
    .detach();
    true
}

#[cfg(test)]
mod tests {
    use super::Arrivals;

    #[test]
    fn only_the_last_of_the_fonts_that_come_one_after_another_draws() {
        let arrivals = Arrivals::default();
        let (a, b, c) = (arrivals.arrive(), arrivals.arrive(), arrivals.arrive());
        assert!(!arrivals.is_last(a) && !arrivals.is_last(b), "an earlier one waits for the later ones");
        assert!(arrivals.is_last(c));
        // A later font, after the quiet time: its own drawing.
        let d = arrivals.arrive();
        assert!(arrivals.is_last(d) && !arrivals.is_last(c));
    }
}
