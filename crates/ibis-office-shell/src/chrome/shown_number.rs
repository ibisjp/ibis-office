// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! A number a panel shows in a field, tied to the exact value of the model it was read from
//! (tasks/doc/design/ui-model-binding.md, rule "rounding is for display only").
//!
//! A field shows a model value rounded to the digits of its `SpinnerSpec` (a glow radius of 64 008 EMU = 5.04 pt is shown
//! `5`). Reading the field back with a plain parse then turns 5.04 pt into 5 pt, so an Apply without any edit changes the
//! document. `ShownNumber` is made from the exact value, writes the field's text itself (`text`, `input`), and answers
//! `resolve(typed)`: the exact value while the field still has the text it was given, and the typed number (rounded once,
//! to the model's unit) only after the user changed it. A panel keeps one `ShownNumber` per numeric field and has no other
//! way to turn the field into a model value, so the rounding cannot leak into the model.

use gpui_kit::component::input::InputState;
use gpui_kit::*;

use super::{Shown, SpinnerSpec, Typed};

/// The model value (an integer in the model's unit, e.g. EMU) of the objects and the text the field was given for it.
/// When the objects differ (`Shown::Mixed`) the text is empty and `resolve` writes nothing until something is typed.
#[derive(Clone, Debug, PartialEq)]
pub struct ShownNumber {
    raw: Shown<i64>,
    /// Model units in one unit of the field (12 700 EMU in a point, 360 000 in a centimetre, 60 000 in a degree).
    per_unit: f64,
    text: String,
}

impl ShownNumber {
    /// `raw` in the model's unit, shown in the field's unit (`per_unit` model units each) with the digits of `spec`.
    pub(crate) fn new(raw: Shown<i64>, per_unit: f64, spec: SpinnerSpec) -> Self {
        let text = match &raw {
            Shown::One(raw) => spec.format_value((*raw as f64 / per_unit) as f32),
            Shown::Mixed | Shown::None => String::new(),
        };
        Self { raw, per_unit, text }
    }

    /// The same value in every object (a panel of one object).
    pub fn one(raw: i64, per_unit: f64, spec: SpinnerSpec) -> Self {
        Self::new(Shown::One(raw), per_unit, spec)
    }

    /// The text the field is given (empty when the objects differ).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The field, holding this text (the field and the text cannot be out of step: nothing else makes the field).
    pub fn input(&self, window: &mut Window, cx: &mut App) -> Entity<InputState> {
        let text = self.text.clone();
        cx.new(|cx| InputState::new(window, cx).default_value(text))
    }

    /// The model value (in the model's unit) the field's `typed` text means: the exact value while the text is the one
    /// shown; else the typed number in the model's unit, rounded once; nothing (`Untouched`) while a field that showed
    /// "mixed" is still empty. `Invalid` when it is not a finite number.
    pub fn resolve(&self, typed: &str) -> Typed<f64> {
        let typed = typed.trim();
        match &self.raw {
            Shown::One(raw) if typed == self.text => return Typed::Value(*raw as f64),
            Shown::Mixed | Shown::None if typed.is_empty() => return Typed::Untouched,
            _ => {}
        }
        match typed.parse::<f64>() {
            Ok(value) if value.is_finite() => Typed::Value((value * self.per_unit).round()),
            _ => Typed::Invalid,
        }
    }
}

/// A text a panel shows in a field (a colour code, a name): the text of the objects, or empty when they differ.
#[derive(Clone, Debug)]
pub struct ShownText {
    shown: Shown<String>,
}

impl ShownText {
    pub(crate) fn new(shown: Shown<String>) -> Self {
        Self { shown }
    }

    pub(crate) fn text(&self) -> &str {
        self.shown.one().map_or("", String::as_str)
    }

    pub(crate) fn shown(&self) -> &Shown<String> {
        &self.shown
    }

    pub(crate) fn input(&self, window: &mut Window, cx: &mut App) -> Entity<InputState> {
        let text = self.text().to_string();
        cx.new(|cx| InputState::new(window, cx).default_value(text))
    }

    /// The typed text (trimmed): the shown one as it is; `Untouched` while a field that showed "mixed" is empty;
    /// `Invalid` for an empty text of a common value.
    pub(crate) fn resolve(&self, typed: &str) -> Typed<String> {
        let typed = typed.trim();
        match &self.shown {
            Shown::One(_) if typed.is_empty() => Typed::Invalid,
            Shown::Mixed | Shown::None if typed.is_empty() => Typed::Untouched,
            _ => Typed::Value(typed.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small generator of model values (no external crate): a xorshift over a fixed seed.
    fn values(count: usize, max: i64) -> Vec<i64> {
        let mut x: u64 = 0x2545_F491_4F6C_DD1D;
        (0..count)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x % (max as u64 + 1)) as i64
            })
            .collect()
    }

    /// The (field unit, model units per field unit, digits) of the panels that use it: a point shown with one digit (glow,
    /// shadow), a centimetre with two (internal margins), a degree with one (a shadow's angle).
    fn cases() -> [(f64, SpinnerSpec, i64); 3] {
        [(12_700., SpinnerSpec::new(1.0).decimals(1), 2_147_483_647), (360_000., SpinnerSpec::new(0.1).min(0.0).decimals(2), 2_147_483_647), (60_000., SpinnerSpec::new(1.0).decimals(1), 21_599_999)]
    }

    /// The property: showing a value and confirming the field without an edit gives the same value back, for any value.
    #[::core::prelude::v1::test]
    fn an_unedited_field_resolves_to_the_exact_value() {
        for (per_unit, spec, max) in cases() {
            for raw in values(20_000, max).into_iter().chain([0, 1, 63_500, 64_008, 12_699, 12_701, max]) {
                let shown = ShownNumber::one(raw, per_unit, spec);
                assert_eq!(shown.resolve(shown.text()), Typed::Value(raw as f64), "raw {raw} shown {:?} (per unit {per_unit})", shown.text());
                assert_eq!(shown.resolve(&format!(" {} ", shown.text())), Typed::Value(raw as f64), "the spaces around the text are not an edit");
            }
        }
    }

    /// The bug it prevents: the plain parse of the shown text is not the value (this is what the panels did).
    #[::core::prelude::v1::test]
    fn a_plain_parse_of_the_shown_text_changes_the_value() {
        let (per_unit, spec, _) = cases()[0];
        let changed = values(2_000, 2_147_483_647).into_iter().filter(|&raw| {
            let text = ShownNumber::one(raw, per_unit, spec).text().to_string();
            (text.parse::<f64>().unwrap() * per_unit).round() as i64 != raw
        });
        assert!(changed.count() > 1_500, "most values are rounded by the display; that is why the exact value must be kept");
        // The example of the review: 64 008 EMU (5.04 pt) is shown 5.
        let shown = ShownNumber::one(64_008, per_unit, spec);
        assert_eq!(shown.text(), "5");
        assert_eq!(shown.resolve("5"), Typed::Value(64_008.));
    }

    /// An edit is the typed number, rounded once to the model's unit; typing the shown text again is not an edit.
    #[::core::prelude::v1::test]
    fn an_edit_is_the_typed_number() {
        let shown = ShownNumber::one(64_008, 12_700., SpinnerSpec::new(1.0).decimals(1));
        assert_eq!(shown.resolve("6"), Typed::Value(76_200.));
        assert_eq!(shown.resolve("5.04"), Typed::Value(64_008.));
        assert_eq!(shown.resolve("5.5"), Typed::Value(69_850.));
        assert_eq!(shown.resolve("x"), Typed::Invalid);
        assert_eq!(shown.resolve("inf"), Typed::Invalid);
        assert_eq!(shown.resolve(""), Typed::Invalid);
    }

    /// Objects that differ: an empty field that is left alone writes nothing; a typed number is the value.
    #[::core::prelude::v1::test]
    fn a_mixed_field_is_empty_and_writes_only_when_typed() {
        let shown = ShownNumber::new(Shown::Mixed, 12_700., SpinnerSpec::new(1.0).decimals(1));
        assert_eq!(shown.text(), "");
        assert_eq!(shown.resolve(""), Typed::Untouched);
        assert_eq!(shown.resolve("  "), Typed::Untouched);
        assert_eq!(shown.resolve("6"), Typed::Value(76_200.));
        assert_eq!(shown.resolve("x"), Typed::Invalid);
        let text = ShownText::new(Shown::Mixed);
        assert_eq!((text.text(), text.resolve(""), text.resolve(" FF0000 ")), ("", Typed::Untouched, Typed::Value("FF0000".to_string())));
        let text = ShownText::new(Shown::One("00FF00".to_string()));
        assert_eq!((text.text(), text.resolve("00FF00"), text.resolve("")), ("00FF00", Typed::Value("00FF00".to_string()), Typed::Invalid));
    }
}
