// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Input fields of a sidebar panel that follow the document (see doc/rule/implimentationRule.md, "パネルの入力欄と文書の同期").
//!
//! A panel that edits the document through text fields must not keep its own copy of the value: the document
//! changes under it (Undo / Redo, another cell selected, an external update) and the copy goes stale. So a field is
//! declared once with the four things that define it, and the registry does everything else:
//!
//! * `read`  — the value the document has now (a *derived state*: never stored by the panel);
//! * `show`  — how that value is written in the field;
//! * `parse` — what the typed text means (`None` while it is not a value yet: a *draft*);
//! * `write` — how a confirmed value is written to the document (one Undo step).
//!
//! `SyncedFields::sync` runs on every render of the panel (xlsx `Workbook::sidebar`) and puts the document's value in
//! every field; a field cannot be left out because the only way to make one is `SyncedFields::add`.
//! In debug builds, `strict_scope` makes the shared field widgets panic for an input that was not made by `add`.
//!
//! The draft rule (the same for every field):
//! * the typed text is in step with the document when it parses to the document's value, or reads the same;
//! * not in step and the document has changed since the last sync (the field's value changed, or Undo / Redo happened):
//!   the document wins, the draft is dropped, the field shows the document's value (also while it has the focus);
//! * not in step and the document has not changed: a field with the focus keeps its draft (the user is typing a value
//!   that is not complete yet, e.g. `FF0` in a colour); a field without the focus goes back to the document's value.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::editor::SlideEditor;

/// When a typed value is written to the document.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Commit {
    /// At every change of the text and at Enter (the value applies as it is typed).
    Typing,
    /// Only at Enter.
    Enter,
}

/// The definition of one field. All the functions are plain `fn`s of the editor: no state of the panel is read.
pub struct FieldSpec<V: 'static> {
    pub id: &'static str,
    pub placeholder: &'static str,
    pub commit: Commit,
    pub read: fn(&SlideEditor) -> V,
    pub show: fn(&V) -> String,
    pub parse: fn(&str) -> Option<V>,
    pub write: fn(&mut SlideEditor, V, &mut Context<SlideEditor>),
}

/// One registered field, seen without its value type.
trait Entry {
    fn id(&self) -> &'static str;
    fn input(&self) -> &Entity<InputState>;
    /// Put the document's value in the field when it is not in step (the draft rule above).
    fn sync(&self, editor: &SlideEditor, history_moved: bool, window: &mut Window, cx: &mut App);
    /// `None` when the field shows the document's value; else what is wrong.
    fn check(&self, editor: &SlideEditor, cx: &App) -> Option<String>;
}

struct TypedEntry<V: Clone + PartialEq + 'static> {
    spec: FieldSpec<V>,
    input: Entity<InputState>,
    /// The document's value at the last sync.
    last: RefCell<Option<V>>,
}

impl<V: Clone + PartialEq + 'static> TypedEntry<V> {
    fn in_step(&self, text: &str, derived: &V) -> bool {
        text == (self.spec.show)(derived) || (self.spec.parse)(text).as_ref() == Some(derived)
    }
}

impl<V: Clone + PartialEq + 'static> Entry for TypedEntry<V> {
    fn id(&self) -> &'static str {
        self.spec.id
    }
    fn input(&self) -> &Entity<InputState> {
        &self.input
    }
    fn sync(&self, editor: &SlideEditor, history_moved: bool, window: &mut Window, cx: &mut App) {
        let derived = (self.spec.read)(editor);
        let changed = history_moved || self.last.borrow().as_ref() != Some(&derived);
        *self.last.borrow_mut() = Some(derived.clone());
        let text = self.input.read(cx).value().to_string();
        if self.in_step(&text, &derived) {
            return;
        }
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        if focused && !changed {
            return; // an unfinished draft of the user
        }
        let shown = (self.spec.show)(&derived);
        self.input.update(cx, |i, cx| i.set_value(shown, window, cx));
    }
    fn check(&self, editor: &SlideEditor, cx: &App) -> Option<String> {
        let derived = (self.spec.read)(editor);
        let text = self.input.read(cx).value().to_string();
        (!self.in_step(&text, &derived)).then(|| format!("{}: the field shows {:?}, the document has {:?}", self.spec.id, text, (self.spec.show)(&derived)))
    }
}

/// The fields of one panel. Cloning shares them.
#[derive(Clone, Default)]
pub struct SyncedFields {
    entries: Rc<RefCell<Vec<Rc<dyn Entry>>>>,
    /// The document's history position at the last sync.
    mark: Rc<Cell<Option<(usize, bool)>>>,
}

impl SyncedFields {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare a field: makes its input (showing the document's value now) and the subscription that writes a
    /// confirmed value. Returns the input to put in the panel (`text_field(&input)` etc.). `subscriptions` keeps the subscription.
    pub fn add<V: Clone + PartialEq + 'static>(
        &self,
        spec: FieldSpec<V>,
        editor: &SlideEditor,
        subscriptions: &mut Vec<Subscription>,
        window: &mut Window,
        cx: &mut Context<SlideEditor>,
    ) -> Entity<InputState> {
        let placeholder = spec.placeholder;
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let derived = (spec.read)(editor);
        let initial = (spec.show)(&derived);
        input.update(cx, |i, cx| i.set_value(initial, window, cx));
        let entry = Rc::new(TypedEntry { spec, input: input.clone(), last: RefCell::new(Some(derived)) });
        let handler = entry.clone();
        subscriptions.push(cx.subscribe_in(&input, window, move |this: &mut SlideEditor, input, event: &InputEvent, _, cx| {
            let enter = matches!(event, InputEvent::PressEnter { .. });
            if !(enter || (handler.spec.commit == Commit::Typing && matches!(event, InputEvent::Change))) {
                return;
            }
            let text = input.read(cx).value().to_string();
            // Only a value that parses and differs from the document's is written: a draft and a no-op never make an Undo step.
            // A text that is what `show` writes for the document's value is not an edit, even when `parse` of it is another value
            // (the field shows 5 for 5.04: Enter without an edit must not write 5; "rounding is for display only").
            let current = (handler.spec.read)(this);
            if text != (handler.spec.show)(&current)
                && let Some(value) = (handler.spec.parse)(&text)
                && value != current
            {
                (handler.spec.write)(this, value, cx);
            }
        }));
        self.entries.borrow_mut().push(entry);
        input
    }

    /// Put the document's values in the fields (render of the panel; see the module comment for the draft rule).
    pub fn sync(&self, editor: &SlideEditor, window: &mut Window, cx: &mut App) {
        let entries: Vec<_> = self.entries.borrow().clone();
        // Undo and Redo move the history mark (a new edit moves it too, but that comes from a field that is in step).
        let mark = (editor.document.undo_len(), editor.document.can_redo());
        let history_moved = self.mark.replace(Some(mark)).is_some_and(|before| before != mark);
        for e in entries {
            e.sync(editor, history_moved, window, cx);
        }
    }

    /// The ids of the fields.
    #[allow(dead_code)]
    pub(crate) fn ids(&self) -> Vec<&'static str> {
        self.entries.borrow().iter().map(|e| e.id()).collect()
    }

    /// The input of the field `id`.
    #[allow(dead_code)]
    pub fn input(&self, id: &str) -> Option<Entity<InputState>> {
        self.entries.borrow().iter().find(|e| e.id() == id).map(|e| e.input().clone())
    }

    /// Whether `input` is one of the fields.
    pub fn contains(&self, input: &Entity<InputState>) -> bool {
        self.entries.borrow().iter().any(|e| e.input().entity_id() == input.entity_id())
    }

    /// The invariant "every field shows the document's value" (for the scenarios): the fields that do not.
    #[allow(dead_code)]
    pub fn mismatches(&self, editor: &SlideEditor, cx: &App) -> Vec<String> {
        self.entries.borrow().iter().filter_map(|e| e.check(editor, cx)).collect()
    }

    /// Debug builds: while the guard lives, a shared field widget given an input that is not one of these fields panics
    /// (a field the panel keeps by itself would go stale; declare it with `add`).
    pub fn strict_scope(&self) -> StrictScope {
        STRICT.with(|s| {
            let previous = s.replace(Some(self.entries.borrow().iter().map(|e| e.input().entity_id()).collect()));
            StrictScope { previous }
        })
    }
}

thread_local! {
    static STRICT: RefCell<Option<Vec<EntityId>>> = const { RefCell::new(None) };
}

/// See [`SyncedFields::strict_scope`].
pub struct StrictScope {
    previous: Option<Vec<EntityId>>,
}

impl Drop for StrictScope {
    fn drop(&mut self) {
        STRICT.with(|s| *s.borrow_mut() = self.previous.take());
    }
}

/// Called by the shared field widgets: in a strict scope, the input must be a registered field.
#[inline]
pub(crate) fn require_registered(input: &Entity<InputState>) {
    #[cfg(any(debug_assertions, feature = "scenarios"))]
    STRICT.with(|s| {
        if let Some(ids) = s.borrow().as_ref() {
            assert!(ids.contains(&input.entity_id()), "an input field of a synced panel was not declared with SyncedFields::add (it would not follow the document)");
        }
    });
    #[cfg(not(any(debug_assertions, feature = "scenarios")))]
    let _ = input;
}
