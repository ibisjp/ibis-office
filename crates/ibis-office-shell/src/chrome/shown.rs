// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! What a panel shows for an attribute of several objects at once (tasks/doc/design/ui-model-binding.md, "混在値").
//!
//! The rule is the one of PowerPoint's panels: when every selected object has the same value the field shows it; when they
//! differ the field is **empty** and nothing is written for it unless the user puts a value in. A panel never decides this
//! itself: it builds a [`Shown`] with [`Shown::of`] and hands it to a shared control (`ShownNumber`, `ShownText`,
//! `SegmentControl::mixed`, `ValueSlider::mixed`, `Dropdown::mixed`, `Checkbox::shown`, `Shown::selects` for a radio group),
//! and at Apply the control answers with a [`Typed`]: only `Typed::Value` is written; `Typed::Untouched` writes nothing.
//! `Shown` and `Typed` are enums, so a panel that does not handle `Mixed` / `Untouched` does not compile.

/// The value of one attribute over the selected objects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shown<V> {
    /// Every object has this value (the field shows it, and Apply writes it, exactly).
    One(V),
    /// The objects differ (the field is empty; Apply writes nothing for it until the user puts a value in).
    Mixed,
    /// There is no object (nothing to show).
    None,
}

impl<V: PartialEq> Shown<V> {
    /// The one function that makes a `Shown`: `value` of every object; all equal gives `One`, any difference `Mixed`.
    pub(crate) fn of<T>(objects: &[T], value: impl Fn(&T) -> V) -> Self {
        Self::from_values(objects.iter().map(value))
    }

    pub(crate) fn from_values(values: impl IntoIterator<Item = V>) -> Self {
        let mut values = values.into_iter();
        let Some(first) = values.next() else { return Shown::None };
        if values.all(|v| v == first) { Shown::One(first) } else { Shown::Mixed }
    }

    /// For a radio group or a list: is `candidate` the selected one? Never when the objects differ or there are none.
    pub(crate) fn selects(&self, candidate: &V) -> bool {
        matches!(self, Shown::One(v) if v == candidate)
    }
}

impl<V> Shown<V> {
    pub(crate) fn is_mixed(&self) -> bool {
        matches!(self, Shown::Mixed)
    }

    /// The common value, if there is one.
    pub(crate) fn one(&self) -> Option<&V> {
        match self {
            Shown::One(v) => Some(v),
            Shown::Mixed | Shown::None => None,
        }
    }

    #[allow(dead_code)] // Part of `Shown`'s API (a value turned into another, mixed stays mixed); not used by a panel yet.
    pub(crate) fn map<U>(self, f: impl FnOnce(V) -> U) -> Shown<U> {
        match self {
            Shown::One(v) => Shown::One(f(v)),
            Shown::Mixed => Shown::Mixed,
            Shown::None => Shown::None,
        }
    }

    /// The text of a dropdown's button: the label of the common value; empty when the objects differ.
    pub(crate) fn label(&self, label: impl FnOnce(&V) -> gpui_kit::SharedString) -> gpui_kit::SharedString {
        match self {
            Shown::One(v) => label(v),
            Shown::Mixed | Shown::None => "".into(),
        }
    }
}

/// What a field means at Apply.
#[derive(Clone, Debug, PartialEq)]
pub enum Typed<V> {
    /// A field that showed "mixed" (empty) and was not touched: write nothing for this attribute.
    Untouched,
    /// The value to write.
    Value(V),
    /// The text is not a value (the panel shows its error).
    Invalid,
}

impl<V> Typed<V> {
    /// Turn the value into another (`None` is invalid, e.g. a range check); `Untouched` and `Invalid` stay as they are.
    pub(crate) fn and_then<U>(self, f: impl FnOnce(V) -> Option<U>) -> Typed<U> {
        match self {
            Typed::Value(v) => f(v).map_or(Typed::Invalid, Typed::Value),
            Typed::Untouched => Typed::Untouched,
            Typed::Invalid => Typed::Invalid,
        }
    }

    pub(crate) fn is_invalid(&self) -> bool {
        matches!(self, Typed::Invalid)
    }

    /// The value to write for one object: the typed one, or the object's own when the field was untouched.
    /// `None` only for `Invalid` (the panel stops before writing).
    pub(crate) fn or(self, own: V) -> Option<V> {
        match self {
            Typed::Value(v) => Some(v),
            Typed::Untouched => Some(own),
            Typed::Invalid => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn the_value_is_shown_only_when_every_object_has_it() {
        assert_eq!(Shown::of(&[3, 3, 3], |v| *v), Shown::One(3));
        assert_eq!(Shown::of(&[3, 4], |v| *v), Shown::Mixed);
        assert_eq!(Shown::of(&[3, 3, 4], |v| *v), Shown::Mixed);
        assert_eq!(Shown::of(&[] as &[i32], |v| *v), Shown::None);
        assert_eq!(Shown::of(&[7], |v| *v), Shown::One(7));
    }

    #[::core::prelude::v1::test]
    fn a_radio_group_selects_nothing_when_mixed() {
        assert!(Shown::One(2).selects(&2));
        assert!(!Shown::One(2).selects(&3));
        assert!(!Shown::<i32>::Mixed.selects(&2));
        assert!(!Shown::<i32>::None.selects(&2));
    }

    #[::core::prelude::v1::test]
    fn an_untouched_field_keeps_the_own_value_of_each_object() {
        assert_eq!(Typed::Untouched.or(5), Some(5));
        assert_eq!(Typed::Value(9).or(5), Some(9));
        assert_eq!(Typed::<i32>::Invalid.or(5), None);
        assert_eq!(Typed::Value(9).and_then(|v| (v < 5).then_some(v)), Typed::Invalid);
    }
}
