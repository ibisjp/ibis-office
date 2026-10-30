// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! オートコレクト (XLSX-OPTIONS-005, DOCX-OPTIONS-007), shared by the formats: the text typed is
//! corrected as Office's AutoCorrect does — two initial capitals, the first letter of a sentence,
//! the names of days, the caps lock slip, and the replace-as-you-type list — and the settings
//! panel (its state kept by the format that opens it).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Mutex;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;

use crate::shell::chrome::SidebarPanel;
use crate::shell::editor::SlideEditor;
use crate::shell::i18n::text;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AutoCorrect {
    /// TWo INitial CApitals → Two Initial Capitals.
    pub(crate) two_initials: bool,
    /// Capitalize the first letter of sentences (after `. `, `! `, `? `).
    pub(crate) sentences: bool,
    /// Capitalize names of days.
    pub(crate) days: bool,
    /// Correct accidental use of cAPS LOCK key.
    pub(crate) caps_lock: bool,
    /// Replace text as you type.
    pub(crate) replace: bool,
    /// Correct as the text is typed (off: what is typed stays as typed).
    pub(crate) as_you_type: bool,
    /// Mark misspelled words as the text is typed (a red wavy line).
    pub(crate) spell_as_you_type: bool,
    /// Straight quotes become curly ones (`"` → “ ”, `'` → ‘ ’).
    pub(crate) smart_quotes: bool,
    /// A paragraph that starts with `* `, `- `, `1. `… becomes a list item (自動の箇条書き・段落番号).
    pub(crate) auto_lists: bool,
    /// A paragraph of `---`, `___`, `===`, `***`, `~~~` or `###` ended with Enter becomes a line (自動の罫線).
    pub(crate) auto_borders: bool,
    /// The spell checker leaves out words in UPPERCASE.
    pub(crate) spell_ignore_upper: bool,
    /// The spell checker leaves out words with numbers in them.
    pub(crate) spell_ignore_numbers: bool,
    /// The spell checker leaves out Internet and file addresses.
    pub(crate) spell_ignore_internet: bool,
    /// The language words are checked in (a BCP 47 tag; empty: the system chooses from the text).
    pub(crate) spell_language: String,
    pub(crate) table: Vec<(String, String)>,
}

impl Default for AutoCorrect {
    fn default() -> Self {
        let pairs: &[(&str, &str)] = &[
            ("(c)", "©"),
            ("(r)", "®"),
            ("(tm)", "™"),
            ("(e)", "€"),
            ("...", "…"),
            ("-->", "→"),
            ("<--", "←"),
            ("==>", "⇒"),
            ("<==", "⇐"),
            ("1/2", "½"),
            ("1/4", "¼"),
            ("3/4", "¾"),
            (":)", "☺"),
            (":-)", "☺"),
            (":(", "☹"),
            (":-(", "☹"),
            ("teh", "the"),
            ("adn", "and"),
            ("taht", "that"),
            ("thier", "their"),
            ("recieve", "receive"),
            ("seperate", "separate"),
            ("occured", "occurred"),
            ("wich", "which"),
            ("becuase", "because"),
            ("definately", "definitely"),
            ("accomodate", "accommodate"),
            ("untill", "until"),
            ("alot", "a lot"),
            ("dont", "don't"),
            ("i", "I"),
            ("cant", "can't"),
            ("wont", "won't"),
            ("didnt", "didn't"),
            ("doesnt", "doesn't"),
            ("isnt", "isn't"),
        ];
        Self { two_initials: true, sentences: true, days: true, caps_lock: true, replace: true, as_you_type: true, spell_as_you_type: true, smart_quotes: true, auto_lists: true, auto_borders: true, spell_ignore_upper: true, spell_ignore_numbers: true, spell_ignore_internet: true, spell_language: String::new(), table: pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() }
    }
}

const DAYS: [&str; 7] = ["sunday", "monday", "tuesday", "wednesday", "thursday", "friday", "saturday"];

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// A word's letters with the punctuation around it set aside:
/// (leading punctuation, the core, trailing punctuation).
fn split_word(word: &str) -> (&str, &str, &str) {
    let start = word.find(|c: char| c.is_alphanumeric()).unwrap_or(word.len());
    let end = word.rfind(|c: char| c.is_alphanumeric()).map_or(start, |i| i + word[i..].chars().next().map_or(1, char::len_utf8));
    (&word[..start], &word[start..end], &word[end..])
}

impl AutoCorrect {
    /// One word (its punctuation with it) as AutoCorrect leaves it; `capitalize_first`: it starts a sentence.
    pub(crate) fn correct_word(&self, word: &str, capitalize_first: bool) -> String {
        let mut w = word.to_string();
        if self.replace {
            // The whole word (with its trailing punctuation) or without it.
            let lower = w.to_lowercase();
            if let Some((_, to)) = self.table.iter().find(|(from, _)| from.to_lowercase() == lower) {
                w = adapt_case(&w, to);
            } else {
                let (before, core, after) = split_word(&w);
                if !core.is_empty() && let Some((_, to)) = self.table.iter().find(|(from, _)| from.to_lowercase() == core.to_lowercase()) {
                    w = format!("{before}{}{after}", adapt_case(core, to));
                }
                // Symbols (`(c)`, `...`, `-->`) are replaced wherever they stand.
                for (from, to) in self.table.iter().filter(|(from, _)| !from.is_empty() && !from.chars().any(char::is_alphanumeric)) {
                    w = w.replace(from.as_str(), to);
                }
            }
        }
        let (before, core, after) = {
            let (b, c, a) = split_word(&w);
            (b.to_string(), c.to_string(), a.to_string())
        };
        let mut core = core;
        let letters: Vec<char> = core.chars().collect();
        if self.two_initials && letters.len() >= 3 && letters.iter().all(|c| c.is_alphabetic()) && letters[0].is_uppercase() && letters[1].is_uppercase() && letters[2..].iter().all(|c| c.is_lowercase()) {
            core = format!("{}{}{}", letters[0], letters[1].to_lowercase(), letters[2..].iter().collect::<String>());
        }
        if self.caps_lock && letters.len() >= 3 && letters.iter().all(|c| c.is_alphabetic()) && letters[0].is_lowercase() && letters[1..].iter().all(|c| c.is_uppercase()) {
            core = format!("{}{}", letters[0].to_uppercase(), letters[1..].iter().flat_map(|c| c.to_lowercase()).collect::<String>());
        }
        if self.days && DAYS.contains(&core.as_str()) {
            core = capitalize(&core);
        }
        if self.sentences && capitalize_first {
            core = capitalize(&core);
        }
        format!("{before}{core}{after}")
    }

    /// 入力中のオートコレクト (a document's): what typing `typed` after `before` (the text of the paragraph up to the
    /// caret) corrects — the characters it takes back (the typed one with them) and what they become. The symbols and the list's other
    /// non-word entries (`(c)`, `-->`, `...`) are replaced at once; a word, when a space or punctuation ends it.
    pub(crate) fn correct_typing(&self, before: &str, typed: char) -> Option<(usize, String)> {
        if !self.as_you_type {
            return None;
        }
        if self.smart_quotes
            && let Some(quote) = smart_quote(before, typed)
        {
            return Some((1, quote.to_string()));
        }
        let text = format!("{before}{typed}");
        let lower = text.to_lowercase();
        if self.replace {
            for (from, to) in self.table.iter().filter(|(from, _)| from.chars().next().is_some_and(|c| !c.is_alphanumeric()) || from.chars().last().is_some_and(|c| !c.is_alphanumeric())) {
                if lower.ends_with(&from.to_lowercase()) {
                    return Some((from.chars().count(), to.clone()));
                }
            }
        }
        if !(typed.is_whitespace() || matches!(typed, '.' | ',' | ';' | '!' | '?')) {
            return None;
        }
        let word_start = before.rfind(char::is_whitespace).map_or(0, |i| i + before[i..].chars().next().map_or(1, char::len_utf8));
        let (prefix, word) = before.split_at(word_start);
        if word.is_empty() {
            return None;
        }
        let ended = prefix.trim_end();
        // After a full stop, `!` or `?` (the start of a paragraph is left: its first word is as typed).
        let capitalize_first = ended.ends_with(['.', '!', '?']);
        let corrected = self.correct_word(word, capitalize_first);
        (corrected != word).then(|| (word.chars().count() + 1, format!("{corrected}{typed}")))
    }

    /// The text corrected as it would be after being typed.
    pub(crate) fn apply(&self, input: &str) -> String {
        // Formulas and the like are not text typed in.
        if !self.as_you_type || input.starts_with('=') || input.starts_with('\'') || input.is_empty() {
            return input.to_string();
        }
        let mut out = String::with_capacity(input.len());
        let mut rest = input;
        // Words and the spaces between them (kept as they are).
        let mut first_word = true;
        let mut sentence_start = false;
        while !rest.is_empty() {
            let space_len = rest.len() - rest.trim_start_matches(char::is_whitespace).len();
            if space_len > 0 {
                out.push_str(&rest[..space_len]);
                rest = &rest[space_len..];
                continue;
            }
            let word_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let word = &rest[..word_len];
            rest = &rest[word_len..];
            let w = self.correct_word(word, sentence_start && !first_word);
            let (_, core, after) = split_word(&w);
            // The next word starts a sentence after `.`, `!` or `?`.
            sentence_start = after.ends_with(['.', '!', '?']) && !core.is_empty();
            first_word = false;
            out.push_str(&w);
        }
        out
    }
}

/// The curly quote that typing the straight quote `typed` after `before` makes: an opening one at the start, after a space or an opening
/// bracket or dash, else a closing one (the apostrophe too); none after a Japanese character (the quotes there are the full-width ones).
pub(crate) fn smart_quote(before: &str, typed: char) -> Option<char> {
    if !matches!(typed, '"' | '\'') {
        return None;
    }
    let last = before.chars().last();
    if last.is_some_and(crate::render::rich_text::is_cjk) {
        return None;
    }
    let opening = last.is_none_or(|c| c.is_whitespace() || matches!(c, '(' | '[' | '{' | '<' | '\u{2018}' | '\u{201c}' | '\u{2014}' | '\u{2013}' | '-' | '/' | '\\'));
    Some(match (typed, opening) {
        ('"', true) => '\u{201c}',
        ('"', false) => '\u{201d}',
        (_, true) => '\u{2018}',
        (_, false) => '\u{2019}',
    })
}

/// The list a paragraph that starts with `marker` (what was typed before the space) becomes: a bullet for `*`, `-`, `•`, `·`; a number for
/// `1.`, `1)`, `(1)`, `a.`, `a)`, `A.`, `i.`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoList {
    Bullet,
    Numbered,
}

pub(crate) fn auto_list_marker(marker: &str) -> Option<AutoList> {
    let chars: Vec<char> = marker.chars().collect();
    if let ['*' | '-' | '•' | '·'] = chars.as_slice() {
        return Some(AutoList::Bullet);
    }
    let digits = |s: &[char]| !s.is_empty() && s.iter().all(|c| c.is_ascii_digit());
    let letter = |s: &[char]| s.len() == 1 && s[0].is_ascii_alphabetic();
    let roman = |s: &[char]| !s.is_empty() && s.iter().all(|c| matches!(c, 'i' | 'v' | 'x' | 'I' | 'V' | 'X'));
    let n = chars.len();
    // `1.` `1)` `a.` `a)` `iv.` and `(1)` `(a)`.
    let body: &[char] = match chars.as_slice() {
        [body @ .., '.' | ')'] if n >= 2 && n <= 5 && !body.contains(&'(') => body,
        ['(', body @ .., ')'] if n >= 3 && n <= 6 => body,
        _ => return None,
    };
    (digits(body) || letter(body) || roman(body)).then_some(AutoList::Numbered)
}

/// The line a paragraph of three or more of a character becomes when Enter ends it: the border below the paragraph (a thin line for
/// `-`, a thick one for `_`, a double one for `=`, dotted for `*`, wavy for `~`, a thick-thin triple for `#`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AutoBorder {
    Thin,
    Thick,
    Double,
    Dotted,
    Wave,
    Triple,
}

pub(crate) fn auto_border_marker(text: &str) -> Option<AutoBorder> {
    let first = text.chars().next()?;
    if text.chars().count() < 3 || !text.chars().all(|c| c == first) {
        return None;
    }
    match first {
        '-' => Some(AutoBorder::Thin),
        '_' => Some(AutoBorder::Thick),
        '=' => Some(AutoBorder::Double),
        '*' => Some(AutoBorder::Dotted),
        '~' => Some(AutoBorder::Wave),
        '#' => Some(AutoBorder::Triple),
        _ => None,
    }
}

/// `to` in the case `from` is typed in (teh → the, Teh → The, TEH → THE).
fn adapt_case(from: &str, to: &str) -> String {
    let letters: Vec<char> = from.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() >= 2 && letters.iter().all(|c| c.is_uppercase()) {
        to.to_uppercase()
    } else if letters.first().is_some_and(|c| c.is_uppercase()) {
        capitalize(to)
    } else {
        to.to_string()
    }
}

static SETTINGS: Mutex<Option<AutoCorrect>> = Mutex::new(None);

fn store_path() -> Option<PathBuf> {
    if cfg!(feature = "scenarios") {
        return None;
    }
    let base = crate::shell::app_data::base();
    Some(base?.join("ibisOffice").join("autocorrect.tsv"))
}

fn parse(content: &str) -> AutoCorrect {
    let mut s = AutoCorrect { table: Vec::new(), ..AutoCorrect::default() };
    let mut listed = false;
    for line in content.lines() {
        let mut parts = line.splitn(3, '\t');
        match (parts.next(), parts.next(), parts.next()) {
            (Some("two_initials"), Some(v), _) => s.two_initials = v == "1",
            (Some("sentences"), Some(v), _) => s.sentences = v == "1",
            (Some("days"), Some(v), _) => s.days = v == "1",
            (Some("caps_lock"), Some(v), _) => s.caps_lock = v == "1",
            (Some("replace"), Some(v), _) => s.replace = v == "1",
            (Some("as_you_type"), Some(v), _) => s.as_you_type = v == "1",
            (Some("spell_as_you_type"), Some(v), _) => s.spell_as_you_type = v == "1",
            (Some("spell_ignore_upper"), Some(v), _) => s.spell_ignore_upper = v == "1",
            (Some("spell_ignore_numbers"), Some(v), _) => s.spell_ignore_numbers = v == "1",
            (Some("spell_ignore_internet"), Some(v), _) => s.spell_ignore_internet = v == "1",
            (Some("smart_quotes"), Some(v), _) => s.smart_quotes = v == "1",
            (Some("auto_lists"), Some(v), _) => s.auto_lists = v == "1",
            (Some("auto_borders"), Some(v), _) => s.auto_borders = v == "1",
            (Some("spell_language"), Some(v), _) => s.spell_language = v.to_string(),
            (Some("entry"), Some(a), Some(b)) => {
                listed = true;
                s.table.push((a.to_string(), b.to_string()));
            }
            _ => {}
        }
    }
    if !listed {
        s.table = AutoCorrect::default().table;
    }
    s
}

fn serialize(s: &AutoCorrect) -> String {
    let flag = |b: bool| if b { "1" } else { "0" };
    let mut text = format!("two_initials\t{}\nsentences\t{}\ndays\t{}\ncaps_lock\t{}\nreplace\t{}\nas_you_type\t{}\nspell_as_you_type\t{}\n", flag(s.two_initials), flag(s.sentences), flag(s.days), flag(s.caps_lock), flag(s.replace), flag(s.as_you_type), flag(s.spell_as_you_type));
    text.push_str(&format!("smart_quotes\t{}\nauto_lists\t{}\nauto_borders\t{}\n", flag(s.smart_quotes), flag(s.auto_lists), flag(s.auto_borders)));
    text.push_str(&format!("spell_ignore_upper\t{}\nspell_ignore_numbers\t{}\nspell_ignore_internet\t{}\nspell_language\t{}\n", flag(s.spell_ignore_upper), flag(s.spell_ignore_numbers), flag(s.spell_ignore_internet), s.spell_language));
    for (a, b) in &s.table {
        text.push_str(&format!("entry\t{a}\t{b}\n"));
    }
    text
}

/// The settings now in force (read from the file the first time).
pub(crate) fn settings() -> AutoCorrect {
    let mut guard = SETTINGS.lock().unwrap_or_else(|e| e.into_inner());
    guard
        .get_or_insert_with(|| store_path().and_then(|p| std::fs::read_to_string(p).ok()).map_or_else(AutoCorrect::default, |c| parse(&c)))
        .clone()
}

/// Back to the defaults (the scenarios start each from them, whatever the one before left).
pub(crate) fn reset() {
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

pub(crate) fn set_settings(s: AutoCorrect) {
    if let Some(path) = store_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, serialize(&s));
    }
    *SETTINGS.lock().unwrap_or_else(|e| e.into_inner()) = Some(s);
}

/// What typing `typed` into a cell leaves as text.
pub(crate) fn correct_typed(typed: &str) -> String {
    settings().apply(typed)
}

/// The languages the spell checker can be set to (a BCP 47 tag and the key of its name); the first is "the system chooses".
const SPELL_LANGUAGES: [(&str, &str); 10] = [
    ("", "autocorrect.lang.auto"),
    ("ja-JP", "autocorrect.lang.ja-JP"),
    ("en-US", "autocorrect.lang.en-US"),
    ("en-GB", "autocorrect.lang.en-GB"),
    ("fr-FR", "autocorrect.lang.fr-FR"),
    ("de-DE", "autocorrect.lang.de-DE"),
    ("es-ES", "autocorrect.lang.es-ES"),
    ("it-IT", "autocorrect.lang.it-IT"),
    ("ko-KR", "autocorrect.lang.ko-KR"),
    ("zh-CN", "autocorrect.lang.zh-CN"),
];

fn spell_language_name(tag: &str) -> String {
    text(SPELL_LANGUAGES.iter().find(|(t, _)| *t == tag).map_or(SPELL_LANGUAGES[0].1, |(_, k)| k)).to_string()
}

/// The language after `tag` in the list (the first again after the last).
pub(crate) fn next_spell_language(tag: &str) -> String {
    let at = SPELL_LANGUAGES.iter().position(|(t, _)| *t == tag).unwrap_or(0);
    SPELL_LANGUAGES[(at + 1) % SPELL_LANGUAGES.len()].0.to_string()
}

pub(crate) struct AutoCorrectPanel {
    pub(crate) entry_open: bool,
    pub(crate) dictionary_tab: bool,
    pub(crate) editing: Option<usize>,
    pub(crate) table_scroll: ScrollHandle,
    pub(crate) working: AutoCorrect,
    pub(crate) replace: Entity<InputState>,
    pub(crate) with: Entity<InputState>,
    pub(crate) selected: Option<usize>,
    pub(crate) message: String,
    pub(crate) ui: Rc<RefCell<BTreeMap<String, Bounds<Pixels>>>>,
}

impl AutoCorrectPanel {
    /// The panel over the settings now in force.
    pub(crate) fn new(window: &mut Window, cx: &mut Context<SlideEditor>) -> Self {
        let make = |window: &mut Window, cx: &mut Context<SlideEditor>| cx.new(|cx| InputState::new(window, cx));
        Self { dictionary_tab: false, entry_open: false, editing: None, table_scroll: ScrollHandle::new(), working: settings(), replace: make(window, cx), with: make(window, cx), selected: None, message: String::new(), ui: Default::default() }
    }
}

/// Where a format keeps the panel's state and what it does with it.
pub(crate) struct PanelAccess {
    pub(crate) get: fn(&mut SlideEditor) -> Option<&mut AutoCorrectPanel>,
    /// OK: the settings are kept and the panel goes.
    pub(crate) apply: fn(&mut SlideEditor, &mut Context<SlideEditor>),
    /// The panel goes without keeping them.
    pub(crate) close: fn(&mut SlideEditor),
}

/// The panel (ツール > オートコレクトのオプション…) over `editor`'s state found by `access`.
pub(crate) fn render_panel(editor: &mut SlideEditor, access: PanelAccess, cx: &mut Context<SlideEditor>) -> Option<SidebarPanel> {
    let PanelAccess { get, apply, close } = access;
    let p = get(editor)?;
    let mark = |id: &str, child: AnyElement| {
        let ui = p.ui.clone();
        let id = id.to_string();
        div()
            .relative()
            .child(child)
            .child(canvas(|_, _, _| {}, move |bounds, _, _, _| {
                ui.borrow_mut().insert(id.clone(), bounds);
            }).absolute().top_0().left_0().size_full())
            .into_any_element()
    };
        let mark_action = |id: &str, action: crate::shell::chrome::ActionDefinition| {
            let ui = p.ui.clone();
            let id = id.to_string();
            action.on_bounds(move |bounds, _, _| {
                ui.borrow_mut().insert(id.clone(), bounds);
            })
        };
    if p.entry_open {
        let body=crate::shell::chrome::panel_form()
            .p_0().child(div().child(text("xlsx.autocorrect.replace-box")))
            .child(mark("from",crate::shell::chrome::text_field(&p.replace).full_width().into_any_element()))
            .child(div().child(text("xlsx.autocorrect.with-box")))
            .child(mark("to",crate::shell::chrome::text_field(&p.with).full_width().into_any_element()))
            .children(crate::shell::chrome::panel_message(&p.message));
        let footer=crate::shell::chrome::action_bar()
            .action(mark_action("entry-ok", crate::shell::chrome::ActionDefinition::new("autocorrect-entry-ok", text("OK"), crate::shell::chrome::ActionButtonKind::Primary).on_click(cx.listener(move|e,_,_,cx|{
                if let Some(p)=get(e){
                    let from=p.replace.read(cx).value().trim().to_string();let to=p.with.read(cx).value().to_string();
                    if from.is_empty(){p.message=text("autocorrect.entry-required").to_string();}
                    else if p.working.table.iter().enumerate().any(|(i,(f,_))|f==&from&&Some(i)!=p.editing){p.message=text("autocorrect.entry-duplicate").to_string();}
                    else {if let Some(i)=p.editing.filter(|i|*i<p.working.table.len()){p.working.table[i]=(from,to);}else{p.working.table.push((from,to));}p.entry_open=false;p.message.clear();}
                }cx.notify();
            }))))
            .action(crate::shell::chrome::ActionDefinition::new("autocorrect-entry-cancel", text("_shell.cancel"), crate::shell::chrome::ActionButtonKind::Normal).on_click(cx.listener(move|e,_,_,cx|{if let Some(p)=get(e){p.entry_open=false;p.message.clear();}cx.notify();})));
        return Some(SidebarPanel::with_footer(text("autocorrect.entry-title"),body.into_any_element(),footer));
    }
    type Set = fn(&mut AutoCorrect);
    let toggle = |id: &'static str, on: bool, label: &'static str, set: Set| {
        mark(
            id,
            crate::shell::chrome::checkbox_button(id, text(label), on, false, cx.listener(move |this, _, _, cx| {
                    if let Some(p) = get(this) {
                        set(&mut p.working);
                    }
                    cx.notify();
                }))
                .into_any_element(),
        )
    };
    let w = &p.working;
    let mut settings_body = v_flex()
        .gap_2()
        .px(px(16.))
        .py(px(8.))
        .child(toggle("as-you-type", w.as_you_type, "xlsx.autocorrect.as-you-type", |s| s.as_you_type = !s.as_you_type))
        .child(toggle("spell-as-you-type", w.spell_as_you_type, "xlsx.autocorrect.spell-as-you-type", |s| s.spell_as_you_type = !s.spell_as_you_type))
        .child(toggle("spell-ignore-upper", w.spell_ignore_upper, "autocorrect.spell-ignore-upper", |s| s.spell_ignore_upper = !s.spell_ignore_upper))
        .child(toggle("spell-ignore-numbers", w.spell_ignore_numbers, "autocorrect.spell-ignore-numbers", |s| s.spell_ignore_numbers = !s.spell_ignore_numbers))
        .child(toggle("spell-ignore-internet", w.spell_ignore_internet, "autocorrect.spell-ignore-internet", |s| s.spell_ignore_internet = !s.spell_ignore_internet))
        .child(v_flex().gap_1().child(div().child(text("autocorrect.spell-language"))).child(mark("spell-language",
            crate::shell::chrome::Dropdown::new("spell-language",spell_language_name(&w.spell_language),
                SPELL_LANGUAGES.iter().map(|&(tag,key)|crate::shell::chrome::DropdownItem::new(text(key)).checked(w.spell_language==tag)
                    .on_select(Rc::new(move|e,_,cx|{if let Some(p)=get(e){p.working.spell_language=tag.to_string();}cx.notify();}))).collect(),&cx.weak_entity()).into_any_element())))
        .child(toggle("two-initials", w.two_initials, "xlsx.autocorrect.two-initials", |s| s.two_initials = !s.two_initials))
        .child(toggle("sentences", w.sentences, "xlsx.autocorrect.sentences", |s| s.sentences = !s.sentences))
        .child(toggle("days", w.days, "xlsx.autocorrect.days", |s| s.days = !s.days))
        .child(toggle("caps-lock", w.caps_lock, "xlsx.autocorrect.caps-lock", |s| s.caps_lock = !s.caps_lock))
        .child(toggle("replace", w.replace, "xlsx.autocorrect.replace", |s| s.replace = !s.replace));
    settings_body=settings_body
        .child(toggle("smart-quotes",w.smart_quotes,"autocorrect.smart-quotes",|s|s.smart_quotes=!s.smart_quotes))
        .child(toggle("auto-lists",w.auto_lists,"autocorrect.auto-lists",|s|s.auto_lists=!s.auto_lists))
        .child(toggle("auto-borders",w.auto_borders,"autocorrect.auto-borders",|s|s.auto_borders=!s.auto_borders))
        ;
    let dictionary_top=h_flex().justify_end().flex_none().child(mark("add",crate::shell::chrome::action_button("autocorrect-add").label(text("fix.common.add")).on_click(cx.listener(move|e,_,w,cx|{
            if let Some(p)=get(e){p.entry_open=true;p.editing=None;p.message.clear();p.replace.update(cx,|i,cx|i.set_value("",w,cx));p.with.update(cx,|i,cx|i.set_value("",w,cx));}cx.notify();
        })).into_any_element()));
    let header=crate::shell::chrome::table_row(vec![div().child(text("xlsx.autocorrect.replace-box")).into_any_element(),div().child(text("xlsx.autocorrect.with-box")).into_any_element()],div().into_any_element(),false).bg(crate::shell::palette::rgb(0xf3f4f6)).font_weight(FontWeight::MEDIUM);
    let mut rows=v_flex();
    for (i,(from,to)) in w.table.iter().enumerate(){
        let actions=h_flex().gap_1()
            .child(crate::shell::chrome::action_button(("autocorrect-edit",i)).w(px(28.)).h(px(28.)).min_w(px(28.)).px_0().icon(crate::shell::chrome::icon("edit",16.,crate::shell::chrome::text_secondary())).on_click(cx.listener(move|e,_,w,cx|{
                cx.stop_propagation();if let Some(p)=get(e){let (from,to)=p.working.table[i].clone();p.entry_open=true;p.editing=Some(i);p.message.clear();p.replace.update(cx,|s,cx|s.set_value(from,w,cx));p.with.update(cx,|s,cx|s.set_value(to,w,cx));}cx.notify();
            })))
            .child(crate::shell::chrome::action_button(("autocorrect-delete",i)).w(px(28.)).h(px(28.)).min_w(px(28.)).px_0().icon(crate::shell::chrome::icon("delete",16.,crate::shell::chrome::text_secondary())).on_click(cx.listener(move|e,_,_,cx|{
                cx.stop_propagation();if let Some(p)=get(e){if i<p.working.table.len(){p.working.table.remove(i);p.selected=None;}}cx.notify();
            })));
        let row=crate::shell::chrome::table_row(vec![div().child(from.clone()).into_any_element(),div().child(to.clone()).into_any_element()],actions.into_any_element(),p.selected==Some(i))
            .id(("autocorrect-row",i)).cursor_pointer().on_click(cx.listener(move|e,_,_,cx|{if let Some(p)=get(e){p.selected=Some(i);}cx.notify();}));
        rows=rows.child(mark(&format!("entry-{i}"),row.into_any_element()));
    }
    let tabs=crate::shell::chrome::tab_control_with_mode([text("autocorrect.tab.settings"),text("autocorrect.tab.dictionary")],usize::from(p.dictionary_tab),"autocorrect-tabs",crate::shell::chrome::TabMode::Intrinsic,move |e,index,cx|{if let Some(p)=get(e){p.dictionary_tab=index==1;}cx.notify();},cx);
    let body=v_flex().size_full().min_h_0().gap_2().p_0().child(tabs.flex_none()).child(
        if p.dictionary_tab {
            crate::shell::chrome::panel_form().flex_1().min_h_0().overflow_hidden()
                .child(dictionary_top)
                .child(crate::shell::chrome::table_fill("autocorrect-replacements",mark("dictionary-header",header.into_any_element()),rows.into_any_element(),p.table_scroll.clone()))
                .children(crate::shell::chrome::panel_message(&p.message)).into_any_element()
        } else {
            div().id("autocorrect-settings-scroll").flex_1().min_h_0().overflow_y_scroll().child(settings_body).into_any_element()
        });
    let footer = crate::shell::chrome::action_bar().action(mark_action("apply", crate::shell::chrome::ActionDefinition::new("autocorrect-ok", text("OK"), crate::shell::chrome::ActionButtonKind::Primary)
            .on_click(cx.listener(move |this, _, _, cx| apply(this, cx)))))
        .action(crate::shell::chrome::dialog_cancel_definition(cx));
    Some(SidebarPanel::with_footer(text("xlsx.autocorrect.title"), body.into_any_element(), footer))

}

impl SlideEditor {
    pub(crate) fn cancel_autocorrect_entry(&mut self)->bool {
        let p=match self.format.kind(){
            ibis_pptx::Kind::Document=>self.doc_state_mut().and_then(|s|s.autocorrect.as_mut()),
            ibis_pptx::Kind::Workbook=>self.wb_mut().panels.autocorrect.as_mut(),
            _=>self.pptx_mut().and_then(|s|s.autocorrect.as_mut()),
        };
        if let Some(p)=p.filter(|p|p.entry_open){p.entry_open=false;p.message.clear();true}else{false}
    }
}

#[cfg(test)]
mod tests {
    use super::AutoCorrect;

    fn fix(s: &str) -> String {
        AutoCorrect::default().apply(s)
    }

    #[test]
    fn smart_quotes_lists_and_borders_are_read_from_what_is_typed() {
        use super::{AutoBorder, AutoList, auto_border_marker, auto_list_marker, smart_quote};
        assert_eq!(smart_quote("", '"'), Some('\u{201c}'));
        assert_eq!(smart_quote("he said ", '"'), Some('\u{201c}'));
        assert_eq!(smart_quote("hello", '"'), Some('\u{201d}'));
        assert_eq!(smart_quote("don", '\''), Some('\u{2019}'));
        assert_eq!(smart_quote("a ", '\''), Some('\u{2018}'));
        assert_eq!(smart_quote("日本", '"'), None, "after Japanese the quote stays");
        assert_eq!(smart_quote("a", 'b'), None);
        let mut on = AutoCorrect::default();
        assert_eq!(on.correct_typing("see \"x", '"'), Some((1, "\u{201d}".to_string())));
        on.smart_quotes = false;
        assert_eq!(on.correct_typing("see \"x", '"'), None);
        assert_eq!(auto_list_marker("*"), Some(AutoList::Bullet));
        assert_eq!(auto_list_marker("-"), Some(AutoList::Bullet));
        assert_eq!(auto_list_marker("1."), Some(AutoList::Numbered));
        assert_eq!(auto_list_marker("12)"), Some(AutoList::Numbered));
        assert_eq!(auto_list_marker("(3)"), Some(AutoList::Numbered));
        assert_eq!(auto_list_marker("a."), Some(AutoList::Numbered));
        assert_eq!(auto_list_marker("iv."), Some(AutoList::Numbered));
        assert_eq!(auto_list_marker("word."), None);
        assert_eq!(auto_list_marker("--"), None);
        assert_eq!(auto_border_marker("---"), Some(AutoBorder::Thin));
        assert_eq!(auto_border_marker("_____"), Some(AutoBorder::Thick));
        assert_eq!(auto_border_marker("==="), Some(AutoBorder::Double));
        assert_eq!(auto_border_marker("--"), None);
        assert_eq!(auto_border_marker("-=-"), None);
    }

    #[test]
    fn two_initial_capitals_and_caps_lock_are_fixed() {
        assert_eq!(fix("TWo INitial CApitals"), "Two Initial Capitals");
        assert_eq!(fix("cAPS lOCK"), "Caps Lock");
        // Three capitals (an acronym) and single capitals are left.
        assert_eq!(fix("USA NASA IBM id"), "USA NASA IBM id");
    }

    #[test]
    fn sentences_and_days_are_capitalized() {
        assert_eq!(fix("see you. monday is good! yes"), "see you. Monday is good! Yes");
        assert_eq!(fix("friday"), "Friday");
        // The first word of the cell is not a sentence start.
        assert_eq!(fix("hello"), "hello");
    }

    #[test]
    fn replace_as_you_type_list_adapts_to_the_case() {
        assert_eq!(fix("teh cat adn dog"), "the cat and dog");
        assert_eq!(fix("Teh end."), "The end.");
        assert_eq!(fix("(c) 2026 (tm)"), "© 2026 ™");
        assert_eq!(fix("wait..."), "wait…");
        assert_eq!(fix("recieve, now"), "receive, now");
    }

    #[test]
    fn formulas_and_numbers_are_not_touched_and_switches_turn_it_off() {
        assert_eq!(fix("=teh"), "=teh");
        assert_eq!(fix("'teh"), "'teh");
        let off = AutoCorrect { replace: false, two_initials: false, ..AutoCorrect::default() };
        assert_eq!(off.apply("TWo teh"), "TWo teh");
    }

    #[test]
    fn typing_in_a_document_corrects_a_word_when_it_is_ended() {
        let a = AutoCorrect::default();
        assert_eq!(a.correct_typing("see teh", ' '), Some((4, "the ".to_string())));
        assert_eq!(a.correct_typing("see teh", 'x'), None, "a letter does not end the word");
        assert_eq!(a.correct_typing("", 'x'), None);
        assert_eq!(a.correct_typing("hello. monday", ' '), Some((7, "Monday ".to_string())));
        assert_eq!(a.correct_typing("start of text. next", ' '), Some((5, "Next ".to_string())), "after a full stop");
        assert_eq!(a.correct_typing("a TWo", ' '), Some((4, "Two ".to_string())));
        assert_eq!(a.correct_typing("価格(c", ')'), Some((3, "©".to_string())), "symbols at once");
        assert_eq!(a.correct_typing("wait..", '.'), Some((3, "…".to_string())));
        assert_eq!(a.correct_typing("USA", ' '), None, "an acronym stays");
        let off = AutoCorrect { replace: false, ..AutoCorrect::default() };
        assert_eq!(off.correct_typing("(c", ')'), None);
        assert_eq!(off.correct_typing("see teh", ' '), None, "the list is off");
    }

    #[test]
    fn the_settings_round_trip_through_the_file_text() {
        let mut s = AutoCorrect::default();
        s.days = false;
        s.table.push(("brb".into(), "be right back".into()));
        let back = super::parse(&super::serialize(&s));
        assert_eq!(back, s);
    }
}
