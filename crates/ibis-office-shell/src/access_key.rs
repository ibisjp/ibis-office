// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Alt+letter access keys for the buttons of the action bar (message boxes and dialogs).
//!
//! One mechanism, behind `ActionBar` (shell/chrome/action_bar.rs). A screen adds an
//! `ActionDefinition` and nothing else: the key is assigned here, the letter is underlined by the
//! button, the key press is handled in `SlideEditor::dialog_key`, and the whole thing is off where
//! the OS has no such convention. Design: tasks/doc/design/alt-access-keys.md.
//!
//! * Label markup (the Windows way): `&Save` underlines S; `保存(&S)` shows `保存(S)` with S
//!   underlined; `&&` is a literal `&`. Without a mark the letter is assigned automatically.
//! * Automatic assignment: first letters of words first, then any other ASCII letter or digit,
//!   skipping letters taken in the same bar. A label with no ASCII letter (Japanese) takes the
//!   letter of the well-known button names (`JA_KEYS`) and shows it in parentheses, as Windows
//!   does (`保存(S)`); any other such label gets none unless it is marked.
//! * Only Windows and Linux. macOS has no Alt access keys; the browser keeps Alt for its own menus
//!   (the Web build's target_os is neither). `supported()` is the one place that decides.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;

use gpui_kit::*;

use crate::chrome::action_bar::ClickHandler;

/// Which layer owns the bar. The key goes to the topmost layer that is showing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Scope {
    /// No access keys (a pane's footer: the pane is not a modal question).
    Off,
    /// A dialog.
    Dialog,
    /// A message box, over everything (confirmations).
    MessageBox,
}

thread_local! {
    static DUPLICATES: Cell<usize> = const { Cell::new(0) };
    static OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
    static FRAME: RefCell<HashMap<WindowId, Vec<Entry>>> = RefCell::new(HashMap::new());
}

struct Entry {
    scope: Scope,
    key: char,
    /// None while the button is disabled: the key stays reserved and does nothing.
    handler: Option<ClickHandler>,
}

/// Whether this OS uses Alt access keys: the one decision point (run time, not a scattering of `cfg`).
pub(crate) fn supported() -> bool {
    OVERRIDE.with(|o| o.get()).unwrap_or(cfg!(any(target_os = "windows", target_os = "linux")))
}

/// For scenarios: pretend to be another OS (`Some(false)` = macOS) until the guard drops.
pub struct OsOverride(Option<bool>);
pub fn override_supported(value: bool) -> OsOverride { OsOverride(OVERRIDE.with(|o| o.replace(Some(value)))) }
impl Drop for OsOverride { fn drop(&mut self) { OVERRIDE.with(|o| o.set(self.0)); } }

/// How many bars so far marked one letter twice (a bug in a label; scenarios assert it stays 0).
pub fn duplicate_marks() -> usize { DUPLICATES.with(|d| d.get()) }

/// The bars are registered while they render; a window starts each draw empty.
pub(crate) fn begin_frame(window: &Window) {
    FRAME.with(|f| { f.borrow_mut().remove(&window.window_handle().window_id()); });
}

pub(crate) fn register(window: &Window, scope: Scope, key: char, handler: Option<ClickHandler>) {
    FRAME.with(|f| f.borrow_mut().entry(window.window_handle().window_id()).or_default().push(Entry { scope, key, handler }));
}

/// What a key press did.
pub(crate) enum Pressed {
    /// Not an access key of this layer.
    No,
    /// The key belongs to a button that is disabled (the press is swallowed, nothing happens).
    Disabled,
    Click(ClickHandler),
}

/// Alt+letter (no other modifier) of the layer `scope`.
pub(crate) fn dispatch(window: &Window, keystroke: &Keystroke, scope: Scope) -> Pressed {
    let m = &keystroke.modifiers;
    if !supported() || scope == Scope::Off || !m.alt || m.control || m.platform || m.shift { return Pressed::No; }
    let mut chars = keystroke.key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else { return Pressed::No };
    let c = c.to_ascii_lowercase();
    FRAME.with(|f| {
        let f = f.borrow();
        match f.get(&window.window_handle().window_id()).and_then(|v| v.iter().find(|e| e.scope == scope && e.key == c)) {
            None => Pressed::No,
            Some(Entry { handler: Some(h), .. }) => Pressed::Click(h.clone()),
            Some(_) => Pressed::Disabled,
        }
    })
}

/// A label with its access key resolved.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Resolved {
    /// The text on the button (markup removed; `(S)` added for a Japanese label).
    pub text: String,
    /// The byte range of the underlined letter in `text`.
    pub underline: Option<Range<usize>>,
    /// The key (lower case); None when this label has no key.
    pub key: Option<char>,
}

/// Well-known Japanese button names and their letters (the Windows convention, `保存(S)`).
const JA_KEYS: &[(&str, char)] = &[
    ("はい", 'y'), ("いいえ", 'n'), ("保存", 's'), ("保存しない", 'n'), ("適用", 'a'), ("削除", 'd'), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("破棄", 'd'), ("キャンセル", 'c'), ("閉じる", 'c'), ("実行", 'r'), ("追加", 'a'), ("開く", 'o'), // i18n-ok (Japanese text under test or a name table that matches labels)
    ("作成", 'n'), ("置換", 'r'), ("すべて置換", 'a'), ("次へ", 'n'), ("戻る", 'b'), ("完了", 'f'), // i18n-ok (Japanese text under test or a name table that matches labels)
];

/// Split `&X` markup. Returns the plain text and the marked letter's byte position in it.
/// `&&` is a literal `&`. A `&` is a mark only before a letter or digit and not right after one
/// (`R&D`, `Q&A` and `A & B` stay as they are).
fn parse(label: &str) -> (String, Option<(usize, char)>) {
    let mut out = String::with_capacity(label.len());
    let mut mark = None;
    let mut prev: Option<char> = None;
    let mut it = label.chars().peekable();
    while let Some(c) = it.next() {
        if c == '&' {
            match it.peek().copied() {
                Some('&') => { it.next(); out.push('&'); prev = Some('&'); continue; }
                Some(n) if n.is_ascii_alphanumeric() && mark.is_none() && !prev.is_some_and(|p| p.is_ascii_alphanumeric()) => {
                    it.next();
                    mark = Some((out.len(), n));
                    out.push(n);
                    prev = Some(n);
                    continue;
                }
                _ => {}
            }
        }
        out.push(c);
        prev = Some(c);
    }
    (out, mark)
}

/// The letter a label marks (`保存(&S)`, `&Save`: s, lower case), if it marks one (the scenarios check the marks against the resolved keys).
pub fn marked_letter(label: &str) -> Option<char> { parse(label).1.map(|(_, c)| c.to_ascii_lowercase()) }

/// The markup removed, nothing else (what macOS, the Web and a pane show).
pub fn plain(label: &str) -> String { parse(label).0 }

/// Resolve the labels of one bar. `on == false` leaves the plain labels and no keys.
/// Marked letters win; the others are assigned in order. A duplicated mark is a bug in the label
/// (debug assert); the later one then gets no key.
pub fn resolve(labels: &[&str], on: bool) -> Vec<Resolved> {
    let parsed: Vec<_> = labels.iter().map(|l| parse(l)).collect();
    if !on {
        return parsed.into_iter().map(|(text, _)| Resolved { text, underline: None, key: None }).collect();
    }
    let mut used: Vec<char> = Vec::new();
    let mut out: Vec<Resolved> = parsed.iter().map(|(text, _)| Resolved { text: text.clone(), ..Default::default() }).collect();
    for (i, (_, mark)) in parsed.iter().enumerate() {
        if let Some((pos, ch)) = mark {
            let key = ch.to_ascii_lowercase();
            let duplicate = used.contains(&key);
            if duplicate { DUPLICATES.with(|d| d.set(d.get() + 1)); }
            debug_assert!(!duplicate, "duplicate access key {ch:?} in the action bar {labels:?}");
            if !duplicate {
                used.push(key);
                out[i].key = Some(key);
                out[i].underline = Some(*pos..*pos + ch.len_utf8());
            }
        }
    }
    for (i, (text, mark)) in parsed.iter().enumerate() {
        if mark.is_some() || text.is_empty() { continue; }
        if let Some(r) = auto_assign(text, &mut used) { out[i] = r; }
    }
    out
}

/// The automatic key of a label that has no mark: a letter of its own words, or (a Japanese name of the well-known list) the list's letter
/// after the name in parentheses. None when it has none or the letter is taken (`used`).
fn auto_assign(text: &str, used: &mut Vec<char>) -> Option<Resolved> {
    let mut out = Resolved { text: text.to_string(), underline: None, key: None };
    if text.chars().any(|c| c.is_ascii_alphabetic()) {
        let (pos, ch) = auto_letter(text, used)?;
        let key = ch.to_ascii_lowercase();
        used.push(key);
        out.key = Some(key);
        out.underline = Some(pos..pos + ch.len_utf8());
        Some(out)
    } else {
        let &(_, key) = JA_KEYS.iter().find(|(name, _)| *name == text)?;
        if used.contains(&key) { return None; }
        used.push(key);
        let start = text.len() + 1;
        out.text = format!("{text}({})", key.to_ascii_uppercase());
        out.key = Some(key);
        out.underline = Some(start..start + 1);
        Some(out)
    }
}

/// The first free letter: word initials, then any letter or digit.
fn auto_letter(text: &str, used: &[char]) -> Option<(usize, char)> {
    let free = |c: char| c.is_ascii_alphanumeric() && !used.contains(&c.to_ascii_lowercase());
    let mut prev_alnum = false;
    let mut initials = Vec::new();
    for (i, c) in text.char_indices() {
        if c.is_ascii_alphanumeric() && !prev_alnum { initials.push((i, c)); }
        prev_alnum = c.is_ascii_alphanumeric();
    }
    initials.into_iter().find(|&(_, c)| free(c)).or_else(|| text.char_indices().find(|&(_, c)| free(c)))
}

/// The label of a menu of the menu bar with its access letter (`letter`, lower case; the table is `MenuId::access_letter`): the letter is
/// underlined where the label has it (an English label: `Insert`, N), and put in parentheses at the end where it has not (a Japanese
/// label: `ファイル(F)`, as Windows does). Where the OS has no access keys (`supported()` is false) the plain label.
pub(crate) fn menu_label(label: &str, letter: char) -> Resolved {
    if !supported() {
        return Resolved { text: label.to_string(), underline: None, key: None };
    }
    let key = letter.to_ascii_lowercase();
    match label.char_indices().find(|(_, c)| c.is_ascii_alphabetic() && c.to_ascii_lowercase() == key) {
        Some((at, c)) => Resolved { text: label.to_string(), underline: Some(at..at + c.len_utf8()), key: Some(key) },
        None => {
            let start = label.len() + 1;
            Resolved { text: format!("{label}({})", key.to_ascii_uppercase()), underline: Some(start..start + 1), key: Some(key) }
        }
    }
}

/// The button's label content, with the access letter underlined.
pub(crate) fn label_element(resolved: &Resolved) -> AnyElement {
    let styled = StyledText::new(resolved.text.clone());
    match &resolved.underline {
        Some(range) => {
            let style = HighlightStyle { underline: Some(UnderlineStyle { thickness: px(1.), color: None, wavy: false }), ..Default::default() };
            styled.with_highlights([(range.clone(), style)]).into_any_element()
        }
        None => styled.into_any_element(),
    }
}

/// How a line of a menu takes its access key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LineKind {
    /// The words of the UI: the letter the tables name (`access.<id>`), else an automatic one.
    Named,
    /// A document's name in a list (recent files, windows): the number 1 to 9 of its place among the numbered lines of the menu.
    Numbered,
    /// The line already has its key (the letter of the bar's menu a ▾ lists): the letter is taken, nothing is assigned.
    Fixed(char),
    /// No key (a font list, a menu search's results, a list of names that is long).
    None,
}

/// The letter the tables give a line (`access.<id>`: a single ASCII letter or digit in each language), if they do.
pub fn explicit_letter(id: &str) -> Option<char> {
    let text = crate::i18n::lookup(&format!("access.{id}"))?;
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

/// The key of a marked letter in a label: the letter underlined where the label has it (a word's first letter before another place),
/// else put in parentheses after the label (`名前を付けて保存(A)`).
fn mark_in(label: &str, key: char) -> Resolved {
    let at = {
        let mut prev_alnum = false;
        let mut initial = None;
        let mut any = None;
        for (i, c) in label.char_indices() {
            if c.is_ascii_alphanumeric() && c.to_ascii_lowercase() == key {
                any.get_or_insert(i);
                if !prev_alnum { initial = Some(i); break; }
            }
            prev_alnum = c.is_ascii_alphanumeric();
        }
        initial.or(any)
    };
    match at {
        Some(at) => Resolved { text: label.to_string(), underline: Some(at..at + 1), key: Some(key) },
        None => {
            let start = label.len() + 1;
            Resolved { text: format!("{label}({})", key.to_ascii_uppercase()), underline: Some(start..start + 1), key: Some(key) }
        }
    }
}

/// One line of a menu level, for [`resolve_level`].
pub(crate) struct Line<'a> {
    pub id: &'a str,
    /// The words shown (no markup).
    pub label: &'a str,
    pub kind: LineKind,
}

/// The access keys of the lines of one menu level (a menu or a submenu): the letters the tables name first (a repeat is a bug in the
/// tables: the later line gets none and it is counted), the numbers of the numbered lines, then automatic letters for the rest.
/// `None` for a line that has no key (its text is the label as it is); nothing at all when `on` is false.
pub(crate) fn resolve_level(lines: &[Line], on: bool) -> Vec<Option<Resolved>> {
    let mut out: Vec<Option<Resolved>> = vec![None; lines.len()];
    if !on { return out; }
    let mut used: Vec<char> = Vec::new();
    let mut number = 0;
    for (i, line) in lines.iter().enumerate() {
        match line.kind {
            LineKind::None => {}
            LineKind::Fixed(key) => used.push(key),
            LineKind::Numbered => {
                number += 1;
                if number <= 9 {
                    let key = char::from_digit(number, 10).unwrap();
                    used.push(key);
                    out[i] = Some(Resolved { text: format!("{key} {}", line.label), underline: Some(0..1), key: Some(key) });
                }
            }
            LineKind::Named => {
                let Some(key) = explicit_letter(line.id) else { continue };
                let duplicate = used.contains(&key);
                if duplicate { DUPLICATES.with(|d| d.set(d.get() + 1)); }
                debug_assert!(!duplicate, "duplicate access key {key:?} in the menu level at {:?}", line.id);
                if !duplicate {
                    used.push(key);
                    out[i] = Some(mark_in(line.label, key));
                }
            }
        }
    }
    for (i, line) in lines.iter().enumerate() {
        if line.kind == LineKind::Named && out[i].is_none() && !line.label.is_empty() && explicit_letter(line.id).is_none() {
            out[i] = auto_assign(line.label, &mut used);
        }
    }
    out
}
