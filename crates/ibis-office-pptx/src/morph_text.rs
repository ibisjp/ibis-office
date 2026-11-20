// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 変形 (Morph) の「単語」「文字」: the words (or characters) the text of a shape has in common with the text of its pair move from
//! where they were to where they are; the others fade out and in. This is the part that needs no drawing: cutting a text into
//! words or characters, finding which of the old are which of the new, and where each of them is at a moment of the transition.
use ibis_doc_model::text::{Paragraph, Run, TextBody};
use crate::render::rich_text::CharBox;

/// What a morph moves in a text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Unit {
    Word,
    Char,
}

impl Unit {
    /// The unit a transition's option (`byWord`, `byChar`) says; `None` for by object.
    pub(crate) fn of_option(option: Option<&str>) -> Option<Unit> {
        match option? {
            "byWord" => Some(Unit::Word),
            "byChar" => Some(Unit::Char),
            _ => None,
        }
    }
}

/// A word or a character of a text, with the runs that draw it and the box it fills (pixels from the top left of the text area).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Token {
    pub(crate) text: String,
    pub(crate) runs: Vec<Run>,
    pub(crate) paragraph: usize,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// The tokens of `body`, given where its characters are (`boxes`, from `Resources::char_boxes`). `None` when the boxes are not those
/// of this text (the number of characters differs: the text has something the layout counts differently), so the caller falls back
/// to fading one text into the other.
pub(crate) fn tokens(body: &TextBody, boxes: &[CharBox], unit: Unit) -> Option<Vec<Token>> {
    let mut out = Vec::new();
    for (n, paragraph) in body.paragraphs.iter().enumerate() {
        let mine: Vec<&CharBox> = boxes.iter().filter(|b| b.paragraph == n).collect();
        let chars = paragraph_chars(paragraph);
        if mine.len() != chars.len() {
            return None;
        }
        let mut current: Option<Token> = None;
        let flush = |t: &mut Option<Token>, out: &mut Vec<Token>| out.extend(t.take());
        for ((c, run), b) in chars.iter().zip(&mine) {
            if c.is_whitespace() {
                flush(&mut current, &mut out);
                continue;
            }
            match &mut current {
                // A word goes on while the next character is a letter or digit on the same line. A punctuation mark is a token of its
                // own (PowerPoint's by-word morph moves the comma of "built," alone: the frames of #M143).
                Some(t) if unit == Unit::Word && c.is_alphanumeric() && t.text.chars().last().is_some_and(char::is_alphanumeric) && (b.y - t.y).abs() < 0.5 => {
                    t.text.push(*c);
                    t.width = b.x + b.width - t.x;
                    match t.runs.last_mut() {
                        Some(last) if last.style == paragraph.runs[*run].style => last.text.push(*c),
                        _ => t.runs.push(Run { text: c.to_string(), style: paragraph.runs[*run].style.clone() }),
                    }
                }
                _ => {
                    flush(&mut current, &mut out);
                    let style = paragraph.runs.get(*run).map(|r| r.style.clone()).unwrap_or_else(|| paragraph.end_style.clone());
                    current = Some(Token { text: c.to_string(), runs: vec![Run { text: c.to_string(), style }], paragraph: n, x: b.x, y: b.y, width: b.width, height: b.height });
                    if unit == Unit::Char {
                        flush(&mut current, &mut out);
                    }
                }
            }
        }
        flush(&mut current, &mut out);
    }
    Some(out)
}

/// The characters of a paragraph with the index of the run each is in.
fn paragraph_chars(paragraph: &Paragraph) -> Vec<(char, usize)> {
    paragraph.runs.iter().enumerate().flat_map(|(i, r)| r.text.chars().map(move |c| (c, i))).collect()
}

/// The words (or characters) both texts have, in order: the longest run of tokens that read the same in the old and the new
/// (a longest common subsequence), as (index in `old`, index in `new`).
pub(crate) fn common(old: &[&str], new: &[&str]) -> Vec<(usize, usize)> {
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..].iter().rev().zip(new[prefix..].iter().rev()).take_while(|(a, b)| a == b).count();
    let (a, b) = (&old[prefix..old.len() - suffix], &new[prefix..new.len() - suffix]);
    let mut out: Vec<(usize, usize)> = (0..prefix).map(|i| (i, i)).collect();
    // table[i][j]: the length of the longest common subsequence of a[i..] and b[j..].
    let width = b.len() + 1;
    let mut table = vec![0u32; (a.len() + 1) * width];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            table[i * width + j] = if a[i] == b[j] { table[(i + 1) * width + j + 1] + 1 } else { table[(i + 1) * width + j].max(table[i * width + j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            out.push((prefix + i, prefix + j));
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out.extend((0..suffix).map(|k| (old.len() - suffix + k, new.len() - suffix + k)));
    out
}

/// What happens to one token.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Move {
    /// In both texts: moves from the old place to the new one.
    Both(Token, Token),
    /// Only in the old text: fades out where it was.
    Out(Token),
    /// Only in the new text: fades in where it is.
    In(Token),
}

/// Which tokens move, which fade out and which fade in.
pub(crate) fn plan(old: Vec<Token>, new: Vec<Token>) -> Vec<Move> {
    let pairs = {
        let a: Vec<&str> = old.iter().map(|t| t.text.as_str()).collect();
        let b: Vec<&str> = new.iter().map(|t| t.text.as_str()).collect();
        common(&a, &b)
    };
    let mut out = Vec::with_capacity(old.len() + new.len());
    let (mut taken_old, mut taken_new) = (vec![false; old.len()], vec![false; new.len()]);
    for &(i, j) in &pairs {
        taken_old[i] = true;
        taken_new[j] = true;
        out.push(Move::Both(old[i].clone(), new[j].clone()));
    }
    out.extend(old.iter().enumerate().filter(|(i, _)| !taken_old[*i]).map(|(_, t)| Move::Out(t.clone())));
    out.extend(new.iter().enumerate().filter(|(i, _)| !taken_new[*i]).map(|(_, t)| Move::In(t.clone())));
    out
}

/// The curve a word or character moves on, from the old place to the new one (progress 0 to 1 of the transition to the share of the way).
///
/// MEASURED, NOT DERIVED (#M143: the frames of PowerPoint's by-word morph, `target/t88/morph/morph-word-1080p-q100.mp4`; a word that moves
/// 188 px, 30 samples): a cubic Bezier through (0,0) and (1,1) with the control points (0.25, 0) and (0.3, 1) fits within about 1 px
/// (sum of squared errors 0.007 in shares of the way). The mechanism PowerPoint uses is not known. Whether this fitted curve is allowed
/// as an exception to the rule against fitted coefficients is for the user to decide (docs/rule/implimentationRule.md).
pub(crate) fn move_curve(progress: f32) -> f32 {
    const P1: (f32, f32) = (0.25, 0.0);
    const P2: (f32, f32) = (0.3, 1.0);
    let x = progress.clamp(0.0, 1.0);
    let at = |t: f32, a: f32, b: f32| 3.0 * (1.0 - t) * (1.0 - t) * t * a + 3.0 * (1.0 - t) * t * t * b + t * t * t;
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if at(mid, P1.0, P2.0) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    at((lo + hi) / 2.0, P1.1, P2.1)
}

/// MEASURED from the same frames (#M143, "old mill" and "new bridge" of the by-word morph, about 3 % error): a word that goes fades out in a
/// straight line from `OUT.0` to `OUT.1` of the transition (0.976 at frame 38, 0.133 at 66, gone at 70 of 30 + 151.5); a word that comes
/// fades in from `IN.0` to `IN.1`, quickly at first (1 - (1 - u)^2). The by-character morph's windows were not separated in the frames.
const OUT: (f32, f32) = (0.05, 0.265);
const IN: (f32, f32) = (0.45, 0.95);

/// Where a token is at `progress` (0 to 1, not eased) of the transition, and how opaque it is.
pub(crate) fn at(m: &Move, progress: f32) -> (Token, f32) {
    match m {
        Move::Both(a, b) => {
            let s = move_curve(progress);
            let l = |x: f32, y: f32| x + (y - x) * s;
            let mut now = b.clone();
            (now.x, now.y, now.width, now.height) = (l(a.x, b.x), l(a.y, b.y), l(a.width, b.width), l(a.height, b.height));
            (now, 1.0)
        }
        Move::Out(a) => (a.clone(), (1.0 - (progress - OUT.0) / (OUT.1 - OUT.0)).clamp(0.0, 1.0)),
        Move::In(b) => {
            let u = ((progress - IN.0) / (IN.1 - IN.0)).clamp(0.0, 1.0);
            (b.clone(), 1.0 - (1.0 - u) * (1.0 - u))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<&str> {
        text.split_whitespace().collect()
    }

    fn token(text: &str, x: f32) -> Token {
        Token { text: text.into(), runs: vec![Run { text: text.into(), style: Default::default() }], paragraph: 0, x, y: 0.0, width: 10.0, height: 12.0 }
    }

    /// About 2000 characters of English where the second text drops, adds and swaps some words.
    fn long_texts() -> (String, String) {
        let base = "The river moved slowly past the old mill while the children counted boats and the farmers argued about rain. ";
        let old: String = (0..19).map(|_| base).collect();
        let new = old.replace("old mill", "new bridge").replacen("farmers", "sailors", 3).replace("counted boats and ", "");
        (old, new)
    }

    #[test]
    fn long_text_keeps_the_common_words_in_order() {
        let (old, new) = long_texts();
        assert!(old.len() > 2000 - 150 && old.len() < 2200);
        let (a, b) = (words(&old), words(&new));
        let pairs = common(&a, &b);
        // Order is kept on both sides, each word is used once, and what is paired reads the same.
        assert!(pairs.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
        assert!(pairs.iter().all(|&(i, j)| a[i] == b[j]));
        // Nothing the second text keeps is missed: the old text has 19 * 3 = 57 words that are gone ("old mill" -> "new bridge"
        // costs 2 each, "counted boats and" 3 each, 3 of the 19 "farmers" 1 each).
        assert_eq!(a.len() - pairs.len(), 19 * 2 + 19 * 3 + 3);
        assert_eq!(b.len() - pairs.len(), 19 * 2 + 3);
        // The characters too (2000 of them, the table is built at once).
        let (ca, cb): (Vec<String>, Vec<String>) = (old.chars().filter(|c| !c.is_whitespace()).map(String::from).collect(), new.chars().filter(|c| !c.is_whitespace()).map(String::from).collect());
        let (ca, cb): (Vec<&str>, Vec<&str>) = (ca.iter().map(String::as_str).collect(), cb.iter().map(String::as_str).collect());
        let chars = common(&ca, &cb);
        assert!(chars.len() >= cb.len() * 8 / 10 && chars.len() <= cb.len());
        assert!(chars.windows(2).all(|w| w[0].0 < w[1].0 && w[0].1 < w[1].1));
    }

    #[test]
    fn moved_words_travel_and_the_others_fade() {
        let old = vec![token("a", 0.0), token("b", 20.0), token("c", 40.0)];
        let new = vec![token("a", 100.0), token("c", 60.0), token("d", 80.0)];
        let moves = plan(old, new);
        assert_eq!(moves.len(), 4);
        let both: Vec<_> = moves.iter().filter(|m| matches!(m, Move::Both(..))).collect();
        assert_eq!(both.len(), 2);
        let a = both[0];
        assert!(at(a, 0.0).0.x.abs() < 1e-3 && at(a, 0.0).1 == 1.0);
        assert!((at(a, 0.5).0.x - 100.0 * move_curve(0.5)).abs() < 1e-3 && at(a, 0.5).1 == 1.0);
        assert!((at(a, 1.0).0.x - 100.0).abs() < 1e-3 && at(a, 1.0).1 == 1.0);
        let out = moves.iter().find(|m| matches!(m, Move::Out(_))).unwrap();
        let inn = moves.iter().find(|m| matches!(m, Move::In(_))).unwrap();
        assert_eq!((at(out, 0.0).1, at(out, 0.265).1, at(out, 1.0).1), (1.0, 0.0, 0.0));
        assert!((at(out, 0.1575).1 - 0.5).abs() < 1e-3);
        assert_eq!((at(inn, 0.0).1, at(inn, 0.45).1, at(inn, 0.95).1, at(inn, 1.0).1), (0.0, 0.0, 1.0, 1.0));
        assert_eq!((at(out, 0.5).0.x, at(inn, 0.5).0.x), (20.0, 80.0));
    }

    #[test]
    fn the_move_curve_is_the_one_read_from_powerpoint() {
        for (p, share) in [(0.106, 0.069), (0.205, 0.234), (0.303, 0.42), (0.403, 0.596), (0.5, 0.734), (0.6, 0.84), (0.7, 0.915), (0.8, 0.963)] {
            assert!((move_curve(p) - share).abs() < 0.04, "{p}: {} against {share}", move_curve(p));
        }
        assert!(move_curve(0.0).abs() < 1e-6 && (move_curve(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn punctuation_is_a_token_of_its_own() {
        let body = TextBody::plain("built, ok.", Default::default());
        let boxes: Vec<CharBox> = (0..10).map(|i| CharBox { paragraph: 0, index: i, x: i as f32 * 10.0, y: 0.0, width: 10.0, height: 12.0 }).collect();
        let words: Vec<String> = tokens(&body, &boxes, Unit::Word).unwrap().into_iter().map(|t| t.text).collect();
        assert_eq!(words, ["built", ",", "ok", "."]);
        assert_eq!(tokens(&body, &boxes, Unit::Char).unwrap().len(), 9);
    }

    #[test]
    fn option_names() {
        assert_eq!(Unit::of_option(Some("byWord")), Some(Unit::Word));
        assert_eq!(Unit::of_option(Some("byChar")), Some(Unit::Char));
        assert_eq!(Unit::of_option(Some("byObject")), None);
        assert_eq!(Unit::of_option(None), None);
    }
}
