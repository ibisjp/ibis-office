// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Railroad (syntax) diagrams: `railroad-ebnf-beta`, `railroad-abnf-beta`, `railroad-peg-beta`
//! grammars and the `railroad-beta` function form (`sequence(…)`, `choice(…)`, …). Every rule
//! becomes one track: terminals are rounded boxes, nonterminals square boxes, choices branch
//! below the main line, optional parts get a bypass above and repetitions a loop below.

use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(super) enum Rr {
    Term(String),
    NonTerm(String),
    Seq(Vec<Rr>),
    Choice(Vec<Rr>),
    Opt(Box<Rr>),
    /// One or more, with an optional loop label (`2..5`).
    More(Box<Rr>, Option<String>),
    /// PEG `!x` / `&x`.
    Pred(char, Box<Rr>),
    /// A special sequence box (`? !Keyword ?`, as mermaid.js labels a PEG predicate).
    Special(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Dialect {
    Ebnf,
    Abnf,
    Peg,
    Calls,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Str(String),
    Ident(String),
    Num(u32),
    Sym(char),
}

fn tokenize(src: &str) -> Vec<Tok> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c == '"' || c == '\'' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != c {
                j += 1;
            }
            out.push(Tok::Str(chars[i + 1..j.min(chars.len())].iter().collect()));
            i = j + 1;
        } else if c.is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            out.push(Tok::Num(chars[i..j].iter().collect::<String>().parse().unwrap_or(0)));
            i = j;
        } else if c.is_alphanumeric() || c == '_' {
            let mut j = i;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '.' || (chars[j] == '-' && chars.get(j + 1).is_some_and(|n| n.is_alphanumeric()))) {
                j += 1;
            }
            out.push(Tok::Ident(chars[i..j].iter().collect()));
            i = j;
        } else if c == '<' && chars.get(i + 1) == Some(&'-') {
            out.push(Tok::Sym('='));
            i += 2;
        } else if c == ':' && chars.get(i + 1) == Some(&':') && chars.get(i + 2) == Some(&'=') {
            out.push(Tok::Sym('='));
            i += 3;
        } else {
            out.push(Tok::Sym(c));
            i += 1;
        }
    }
    out
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    dialect: Dialect,
}

fn seq_of(mut items: Vec<Rr>) -> Rr {
    if items.len() == 1 { items.pop().unwrap_or(Rr::Seq(Vec::new())) } else { Rr::Seq(items) }
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn is_sym(&self, c: char) -> bool {
        self.peek() == Some(&Tok::Sym(c))
    }

    fn alt_sym(&self) -> char {
        if self.dialect == Dialect::Ebnf { '|' } else { '/' }
    }

    fn alt(&mut self) -> Rr {
        let mut options = vec![self.seq()];
        while self.is_sym(self.alt_sym()) {
            self.pos += 1;
            options.push(self.seq());
        }
        if options.len() == 1 { options.pop().unwrap_or(Rr::Seq(Vec::new())) } else { Rr::Choice(options) }
    }

    fn seq(&mut self) -> Rr {
        let mut items = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some(Tok::Sym(c)) if *c == self.alt_sym() || matches!(c, ')' | ']' | '}' | ';' | '=') => break,
                Some(Tok::Sym(',')) => self.pos += 1,
                // A new rule starts (`name =`) when a `;` was left out.
                Some(Tok::Ident(_)) if self.toks.get(self.pos + 1) == Some(&Tok::Sym('=')) => break,
                _ => match self.prefixed() {
                    Some(e) => items.push(e),
                    None => {
                        self.pos += 1;
                    }
                },
            }
        }
        seq_of(items)
    }

    fn prefixed(&mut self) -> Option<Rr> {
        if self.dialect == Dialect::Abnf {
            // `n*m elem`, `*elem`, `1*elem`, `n elem`.
            let min = match self.peek() {
                Some(Tok::Num(n)) => {
                    let n = *n;
                    self.pos += 1;
                    Some(n)
                }
                _ => None,
            };
            if self.is_sym('*') {
                self.pos += 1;
                let max = match self.peek() {
                    Some(Tok::Num(n)) => {
                        let n = *n;
                        self.pos += 1;
                        Some(n)
                    }
                    _ => None,
                };
                let item = self.postfixed()?;
                let label = match (min.unwrap_or(0), max) {
                    (0 | 1, None) => None,
                    (lo, None) => Some(format!("{lo}+")),
                    (lo, Some(hi)) => Some(format!("{lo}..{hi}")),
                };
                let more = Rr::More(Box::new(item), label);
                return Some(if min.unwrap_or(0) == 0 { Rr::Opt(Box::new(more)) } else { more });
            }
            if let Some(n) = min {
                let item = self.postfixed()?;
                return Some(if n == 1 { item } else { Rr::More(Box::new(item), Some(format!("{n}×"))) });
            }
        }
        if self.dialect == Dialect::Peg && (self.is_sym('!') || self.is_sym('&')) {
            let Some(Tok::Sym(c)) = self.peek().cloned() else { return None };
            self.pos += 1;
            return Some(match self.postfixed()? {
                Rr::NonTerm(name) => Rr::Special(format!("? {c}{name} ?")),
                Rr::Term(text) => Rr::Special(format!("? {c}\"{text}\" ?")),
                other => Rr::Pred(c, Box::new(other)),
            });
        }
        self.postfixed()
    }

    fn postfixed(&mut self) -> Option<Rr> {
        let mut e = self.primary()?;
        if self.dialect != Dialect::Abnf {
            loop {
                if self.is_sym('?') {
                    e = Rr::Opt(Box::new(e));
                } else if self.is_sym('*') {
                    e = Rr::Opt(Box::new(Rr::More(Box::new(e), None)));
                } else if self.is_sym('+') {
                    e = Rr::More(Box::new(e), None);
                } else {
                    break;
                }
                self.pos += 1;
            }
        }
        Some(e)
    }

    fn close(&mut self, c: char) {
        if self.is_sym(c) {
            self.pos += 1;
        }
    }

    fn primary(&mut self) -> Option<Rr> {
        let tok = self.peek()?.clone();
        self.pos += 1;
        Some(match tok {
            Tok::Str(s) => Rr::Term(s),
            Tok::Ident(name) => Rr::NonTerm(name),
            Tok::Num(n) => Rr::Term(n.to_string()),
            Tok::Sym('(') => {
                let e = self.alt();
                self.close(')');
                e
            }
            Tok::Sym('[') => {
                let e = self.alt();
                self.close(']');
                Rr::Opt(Box::new(e))
            }
            Tok::Sym('{') => {
                let e = self.alt();
                self.close('}');
                Rr::Opt(Box::new(Rr::More(Box::new(e), None)))
            }
            _ => return None,
        })
    }

    /// `name(args…)` of the function form, or a bare string.
    fn call(&mut self) -> Option<Rr> {
        match self.peek()?.clone() {
            Tok::Str(s) => {
                self.pos += 1;
                Some(Rr::Term(s))
            }
            Tok::Ident(name) => {
                self.pos += 1;
                let mut args = Vec::new();
                if self.is_sym('(') {
                    self.pos += 1;
                    while self.peek().is_some() && !self.is_sym(')') {
                        if self.is_sym(',') {
                            self.pos += 1;
                            continue;
                        }
                        match self.call() {
                            Some(a) => args.push(a),
                            None => self.pos += 1,
                        }
                    }
                    self.close(')');
                }
                let text = |args: &[Rr]| match args.first() {
                    Some(Rr::Term(s)) => s.clone(),
                    _ => String::new(),
                };
                let one = |args: Vec<Rr>| seq_of(args);
                Some(match name.as_str() {
                    "terminal" | "t" => Rr::Term(text(&args)),
                    "nonterminal" | "nt" | "n" => Rr::NonTerm(text(&args)),
                    "sequence" | "seq" | "stack" => Rr::Seq(args),
                    "choice" | "alt" | "alternatives" => Rr::Choice(args),
                    "optional" | "opt" => Rr::Opt(Box::new(one(args))),
                    "oneOrMore" | "plus" => Rr::More(Box::new(one(args)), None),
                    "zeroOrMore" | "star" => Rr::Opt(Box::new(Rr::More(Box::new(one(args)), None))),
                    "skip" => Rr::Seq(Vec::new()),
                    _ => Rr::NonTerm(name),
                })
            }
            _ => None,
        }
    }
}

/// (title, rules).
pub(super) fn parse_railroad(source: &str) -> (Option<String>, Vec<(String, Rr)>) {
    let mut lines = source.lines();
    let header = lines.next().unwrap_or("").trim();
    let dialect = if header.starts_with("railroad-ebnf") {
        Dialect::Ebnf
    } else if header.starts_with("railroad-abnf") {
        Dialect::Abnf
    } else if header.starts_with("railroad-peg") {
        Dialect::Peg
    } else {
        Dialect::Calls
    };
    let mut title = None;
    let mut body = String::new();
    for l in lines {
        let t = l.trim();
        if let Some(rest) = t.strip_prefix("title ").or_else(|| t.strip_prefix("title\t")) {
            title = Some(unquote(rest.trim()));
        } else if !t.starts_with("%%") && !t.starts_with("accTitle") && !t.starts_with("accDescr") {
            body.push_str(l);
            body.push('\n');
        }
    }
    let mut p = Parser { toks: tokenize(&body), pos: 0, dialect };
    let mut rules = Vec::new();
    while p.pos < p.toks.len() {
        let (Some(Tok::Ident(name)), Some(Tok::Sym('='))) = (p.toks.get(p.pos).cloned(), p.toks.get(p.pos + 1).cloned()) else {
            p.pos += 1;
            continue;
        };
        p.pos += 2;
        let e = if dialect == Dialect::Calls { p.call().unwrap_or(Rr::Seq(Vec::new())) } else { p.alt() };
        while p.pos < p.toks.len() && !p.is_sym(';') && !(matches!(p.peek(), Some(Tok::Ident(_))) && p.toks.get(p.pos + 1) == Some(&Tok::Sym('='))) {
            p.pos += 1;
        }
        p.close(';');
        rules.push((name, e));
    }
    (title, rules)
}

const FONT: f32 = 13.0;
const GAP: f32 = 10.0;
const ARC: f32 = 10.0;

/// (width, height above the line, height below).
fn measure(e: &Rr) -> (f32, f32, f32) {
    match e {
        Rr::Term(s) | Rr::NonTerm(s) | Rr::Special(s) => (estimate_text_width(s, FONT).max(8.0) + 24.0, 12.0, 12.0),
        Rr::Pred(_, inner) => {
            let (w, up, down) = measure(inner);
            (w + 32.0, up, down)
        }
        Rr::Seq(items) => {
            if items.is_empty() {
                return (GAP, 0.0, 0.0);
            }
            items.iter().map(measure).fold((-GAP, 0.0f32, 0.0f32), |(w, u, d), (iw, iu, id)| (w + iw + GAP, u.max(iu), d.max(id)))
        }
        Rr::Choice(items) => {
            let sizes: Vec<_> = items.iter().map(measure).collect();
            let inner = sizes.iter().map(|s| s.0).fold(0.0f32, f32::max);
            let mut down = sizes[0].2;
            for s in &sizes[1..] {
                down += GAP + s.1 + s.2;
            }
            (inner + 4.0 * ARC, sizes[0].1, down)
        }
        Rr::Opt(inner) => {
            let (w, up, down) = measure(inner);
            (w + 4.0 * ARC, up.max(ARC) + ARC, down)
        }
        Rr::More(inner, label) => {
            let (w, up, down) = measure(inner);
            (w + 4.0 * ARC, up, down + 2.0 * ARC + if label.is_some() { 14.0 } else { 0.0 })
        }
    }
}

fn draw(e: &Rr, x: f32, y: f32, svg: &mut String) {
    let line = |svg: &mut String, x1: f32, x2: f32, y: f32| {
        if x2 > x1 + 0.05 {
            svg.push_str(&format!(r#"<path d="M {x1:.1} {y:.1} H {x2:.1}"/>"#));
        }
    };
    match e {
        Rr::Special(s) => {
            let (w, _, _) = measure(e);
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{:.1}" width="{w:.1}" height="24" rx="4" fill="#fef3c7" stroke="#d97706" stroke-width="1.3" stroke-dasharray="4,2" data-rr="special"/><text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="{FONT}" fill="#0f172a" stroke="none">{}</text>"##,
                y - 12.0,
                x + w / 2.0,
                y + 4.5,
                escape_xml(s)
            ));
        }
        Rr::Term(s) | Rr::NonTerm(s) => {
            let (w, _, _) = measure(e);
            let term = matches!(e, Rr::Term(_));
            let (rx, fill, kind) = if term { (12.0, "#ecfdf5", "terminal") } else { (2.0, "#eff6ff", "nonterminal") };
            let stroke = if term { "#059669" } else { "#2563eb" };
            svg.push_str(&format!(
                r##"<rect x="{x:.1}" y="{:.1}" width="{w:.1}" height="24" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="1.3" data-rr="{kind}"/><text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="{FONT}" fill="#0f172a" stroke="none"{}>{}</text>"##,
                y - 12.0,
                x + w / 2.0,
                y + 4.5,
                if term { r#" font-family="monospace""# } else { "" },
                escape_xml(s)
            ));
        }
        Rr::Pred(c, inner) => {
            svg.push_str(&format!(
                r##"<circle cx="{:.1}" cy="{y:.1}" r="9" fill="#fef3c7" stroke="#d97706" stroke-width="1.3" data-rr="predicate"/><text x="{:.1}" y="{:.1}" text-anchor="middle" font-size="12" font-weight="bold" fill="#92400e" stroke="none">{c}</text>"##,
                x + 9.0,
                x + 9.0,
                y + 4.0
            ));
            line(svg, x + 18.0, x + 32.0, y);
            draw(inner, x + 32.0, y, svg);
        }
        Rr::Seq(items) => {
            if items.is_empty() {
                line(svg, x, x + GAP, y);
            }
            let mut cx = x;
            for (k, item) in items.iter().enumerate() {
                if k > 0 {
                    line(svg, cx, cx + GAP, y);
                    cx += GAP;
                }
                draw(item, cx, y, svg);
                cx += measure(item).0;
            }
        }
        Rr::Choice(items) => {
            let (w, _, _) = measure(e);
            let inner = w - 4.0 * ARC;
            let right = x + w;
            let mut yk = y;
            for (k, item) in items.iter().enumerate() {
                let (iw, iu, idn) = measure(item);
                if k > 0 {
                    yk += GAP + iu;
                    svg.push_str(&format!(
                        r#"<path d="M {x:.1} {y:.1} Q {:.1} {y:.1} {:.1} {:.1} V {:.1} Q {:.1} {yk:.1} {:.1} {yk:.1}"/><path d="M {:.1} {yk:.1} Q {:.1} {yk:.1} {:.1} {:.1} V {:.1} Q {:.1} {y:.1} {right:.1} {y:.1}"/>"#,
                        x + ARC,
                        x + ARC,
                        y + ARC,
                        yk - ARC,
                        x + ARC,
                        x + 2.0 * ARC,
                        right - 2.0 * ARC,
                        right - ARC,
                        right - ARC,
                        yk - ARC,
                        y + ARC,
                        right - ARC
                    ));
                } else {
                    line(svg, x, x + 2.0 * ARC, y);
                    line(svg, right - 2.0 * ARC, right, y);
                }
                draw(item, x + 2.0 * ARC, yk, svg);
                line(svg, x + 2.0 * ARC + iw, x + 2.0 * ARC + inner, yk);
                yk += idn;
            }
        }
        Rr::Opt(inner) => {
            let (iw, iu, _) = measure(inner);
            let right = x + iw + 4.0 * ARC;
            let yb = y - iu.max(ARC) - ARC;
            line(svg, x, x + 2.0 * ARC, y);
            draw(inner, x + 2.0 * ARC, y, svg);
            line(svg, x + 2.0 * ARC + iw, right, y);
            svg.push_str(&format!(
                r#"<path d="M {x:.1} {y:.1} Q {:.1} {y:.1} {:.1} {:.1} V {:.1} Q {:.1} {yb:.1} {:.1} {yb:.1} H {:.1} Q {:.1} {yb:.1} {:.1} {:.1} V {:.1} Q {:.1} {y:.1} {right:.1} {y:.1}" data-rr="bypass"/>"#,
                x + ARC,
                x + ARC,
                y - ARC,
                yb + ARC,
                x + ARC,
                x + 2.0 * ARC,
                right - 2.0 * ARC,
                right - ARC,
                right - ARC,
                yb + ARC,
                y - ARC,
                right - ARC
            ));
        }
        Rr::More(inner, label) => {
            let (iw, _, idn) = measure(inner);
            let right = x + iw + 4.0 * ARC;
            let yl = y + idn + ARC;
            line(svg, x, x + 2.0 * ARC, y);
            draw(inner, x + 2.0 * ARC, y, svg);
            line(svg, x + 2.0 * ARC + iw, right, y);
            svg.push_str(&format!(
                r#"<path d="M {:.1} {y:.1} Q {:.1} {y:.1} {:.1} {:.1} V {:.1} Q {:.1} {yl:.1} {:.1} {yl:.1} H {:.1} Q {:.1} {yl:.1} {:.1} {:.1} V {:.1} Q {:.1} {y:.1} {:.1} {y:.1}" marker-mid="none" data-rr="loop"/>"#,
                right - 2.0 * ARC,
                right - ARC,
                right - ARC,
                y + ARC,
                yl - ARC,
                right - ARC,
                right - 2.0 * ARC,
                x + 2.0 * ARC,
                x + ARC,
                x + ARC,
                yl - ARC,
                y + ARC,
                x + ARC,
                x + 2.0 * ARC
            ));
            let mid = x + 2.0 * ARC + iw / 2.0;
            svg.push_str(&format!(r##"<path d="M {:.1} {:.1} l 6 -4 v 8 z" fill="#475569" stroke="none"/>"##, mid - 3.0, yl));
            if let Some(label) = label {
                svg.push_str(&format!(
                    r##"<text x="{mid:.1}" y="{:.1}" text-anchor="middle" font-size="11" fill="#64748b" stroke="none">{}</text>"##,
                    yl + 14.0,
                    escape_xml(label)
                ));
            }
        }
    }
}

pub(super) fn render_railroad_svg(source: &str) -> Option<(Vec<u8>, u32, u32)> {
    let (title, rules) = parse_railroad(source);
    if rules.is_empty() {
        return None;
    }
    let pad = 16.0;
    let mut body = String::new();
    let mut y = pad;
    let mut width = 0.0f32;
    // Rule names (`name =`, as mermaid.js writes them) sit left of their tracks.
    let names: Vec<String> = rules.iter().map(|(name, _)| format!("{name} =")).collect();
    let name_w = names.iter().map(|n| estimate_text_width(n, 13.0) * 1.08).fold(0.0f32, f32::max);
    for ((name, rule), label) in rules.iter().zip(&names) {
        let (w, up, down) = measure(rule);
        let base = y + up.max(10.0) + 4.0;
        body.push_str(&format!(
            r##"<text x="{pad}" y="{:.1}" font-size="13" font-weight="bold" fill="#0f172a" stroke="none" data-rule="{}">{}</text>"##,
            base + 4.5,
            escape_attr(name),
            escape_xml(label)
        ));
        let x0 = pad + name_w + 10.0;
        // Start and end bars.
        body.push_str(&format!(
            r#"<path d="M {x0:.1} {:.1} V {:.1} M {:.1} {:.1} V {:.1} M {x0:.1} {base:.1} H {:.1}"/>"#,
            base - 8.0,
            base + 8.0,
            x0 + 5.0,
            base - 8.0,
            base + 8.0,
            x0 + 15.0
        ));
        draw(rule, x0 + 15.0, base, &mut body);
        let end = x0 + 15.0 + w;
        body.push_str(&format!(
            r#"<path d="M {end:.1} {base:.1} H {:.1} M {:.1} {:.1} V {:.1} M {:.1} {:.1} V {:.1}"/>"#,
            end + 10.0,
            end + 10.0,
            base - 8.0,
            base + 8.0,
            end + 15.0,
            base - 8.0,
            base + 8.0
        ));
        width = width.max(end + 15.0 + pad);
        y = base + down.max(10.0) + 18.0;
    }
    let height = (y - 6.0).ceil();
    let width = width.max(160.0).ceil();
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="sans-serif"><rect width="{width}" height="{height}" fill="#ffffff"/><g fill="none" stroke="#475569" stroke-width="1.5">{body}</g></svg>"##
    );
    let bytes = svg.into_bytes();
    Some(match &title {
        Some(t) => add_svg_title(bytes, width as u32, height as u32, t),
        None => (bytes, width as u32, height as u32),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mermaid::test_support::*;

    fn t(s: &str) -> Rr {
        Rr::Term(s.into())
    }
    fn n(s: &str) -> Rr {
        Rr::NonTerm(s.into())
    }

    #[test]
    fn railroad_ebnf_and_peg_grammars() {
        let (title, rules) = parse_railroad("railroad-ebnf-beta\ntitle \"Optional Sign\"\nnumber = sign? digit+ ;\nid = letter ( letter | \"_\" )* ;\nobj = \"{\" [ m ( \",\" m )* ] \"}\" ;");
        assert_eq!(title.as_deref(), Some("Optional Sign"));
        assert_eq!(rules[0].1, Rr::Seq(vec![Rr::Opt(Box::new(n("sign"))), Rr::More(Box::new(n("digit")), None)]));
        assert_eq!(rules[1].1, Rr::Seq(vec![n("letter"), Rr::Opt(Box::new(Rr::More(Box::new(Rr::Choice(vec![n("letter"), t("_")])), None)))]));
        assert_eq!(rules[2].0, "obj");
        let (_, peg) = parse_railroad("railroad-peg-beta\nIdentifier <- !Keyword Letter Letter* ;\nKeyword <- \"if\" / \"else\" ;");
        assert_eq!(peg[0].1, Rr::Seq(vec![Rr::Special("? !Keyword ?".into()), n("Letter"), Rr::Opt(Box::new(Rr::More(Box::new(n("Letter")), None)))]));
        assert_eq!(peg[1].1, Rr::Choice(vec![t("if"), t("else")]));
    }

    #[test]
    fn railroad_abnf_repetitions_and_function_form() {
        let (_, rules) = parse_railroad("railroad-abnf-beta\nlocal-part = 1*( ALPHA / \".\" ) ;\ndomain = label *( \".\" label ) ;\nphone = [ \"+\" cc ] 2*3DIGIT ;");
        assert_eq!(rules[0], ("local-part".into(), Rr::More(Box::new(Rr::Choice(vec![n("ALPHA"), t(".")])), None)));
        assert_eq!(rules[1].1, Rr::Seq(vec![n("label"), Rr::Opt(Box::new(Rr::More(Box::new(Rr::Seq(vec![t("."), n("label")])), None)))]));
        assert_eq!(rules[2].1, Rr::Seq(vec![Rr::Opt(Box::new(Rr::Seq(vec![t("+"), n("cc")]))), Rr::More(Box::new(n("DIGIT")), Some("2..3".into()))]));
        let (_, calls) = parse_railroad("railroad-beta\nexpr = sequence(\n  nonterminal(\"term\"),\n  zeroOrMore(choice(terminal(\"+\"), terminal(\"-\")))\n) ;");
        assert_eq!(calls[0].1, Rr::Seq(vec![n("term"), Rr::Opt(Box::new(Rr::More(Box::new(Rr::Choice(vec![t("+"), t("-")])), None)))]));
    }

    #[test]
    fn railroad_svg_tracks_boxes_and_loops() {
        let svg = svg_of("railroad-ebnf-beta\ntitle \"T\"\nsign = \"+\" | \"-\" ;\nnumber = sign? digit+ ;");
        assert_eq!(svg.matches(r#"data-rr="terminal""#).count(), 2);
        assert_eq!(svg.matches(r#"data-rr="nonterminal""#).count(), 2);
        assert!(svg.contains(r#"data-rr="bypass""#) && svg.contains(r#"data-rr="loop""#));
        assert!(text_y(&svg, "number =") > text_y(&svg, "sign ="));
        assert!(text_x(&svg, "+") > text_x(&svg, "number ="));
        // The second alternative branches below the first.
        let plus = tag_with(&svg, r#"data-rr="terminal""#);
        assert!(attr_f32(plus[1], "y").unwrap() > attr_f32(plus[0], "y").unwrap());
        assert!(svg.contains(">T<"));
    }
}
