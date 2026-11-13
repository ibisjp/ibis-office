// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Formulas: their text, as typed and as a file stores it, and their
//! parsing into expressions.
//!
//! The model keeps a formula as the user sees it (`XLOOKUP(…)`); a file
//! stores the functions newer than Excel 2007 with a prefix
//! (`_xlfn.XLOOKUP(…)`). [`from_file`] and [`to_file`] convert between the
//! two, changing nothing else of the text.

pub mod arrays;
pub mod lexer;
pub mod parser;

pub use parser::{AreaRef, BinaryOp, Corner, Expr, MAX_COLUMN, MAX_ROW, ParseError, SheetRef, parse};

use lexer::{Token, tokenize};

use crate::functions::table::function_info;

/// A formula as a file stores it, as the user sees it: without the
/// `_xlfn.` / `_xlws.` / `_xlpm.` prefixes, except parameter prefixes
/// needed to distinguish names from cell references or Boolean literals.
pub fn from_file(text: &str) -> String {
    if !text.contains("_xl") {
        return text.to_string();
    }
    let text = &single_calls_to_implicit(&anchor_arrays_to_spills(text));
    let Ok(tokens) = tokenize(text) else { return text.to_string() };
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for t in &tokens {
        if let Token::Word(w) = &t.token {
            // An optional LAMBDA parameter: the file spells `[b]` as `_xlop.b` (Excel writes it so; GitHub issue #168).
            if w.get(..6).is_some_and(|p| p.eq_ignore_ascii_case("_xlop.")) && w.len() > 6 {
                out.push_str(&text[at..t.start]);
                out.push('[');
                out.push_str(&w[6..]);
                out.push(']');
                at = t.end;
                continue;
            }
            let stripped = strip_prefixes(w);
            // Removing this parameter marker would change the parsed meaning.
            if w.get(..6).is_some_and(|p|p.eq_ignore_ascii_case("_xlpm."))
                && (parser::parse_cell(stripped).is_some() || stripped.eq_ignore_ascii_case("TRUE") || stripped.eq_ignore_ascii_case("FALSE")) {
                continue;
            }
            if stripped.len() != w.len() {
                out.push_str(&text[at..t.start]);
                out.push_str(stripped);
                at = t.end;
            }
        }
    }
    out.push_str(&text[at..]);
    out
}

fn strip_prefixes(word: &str) -> &str {
    let mut w = word;
    loop {
        if w.get(..7).is_some_and(|p| p.eq_ignore_ascii_case("_xleta.")) {
            w = &w[7..];
            continue;
        }
        let lower = w.get(..6).map(|p| p.to_ascii_lowercase());
        match lower.as_deref() {
            Some("_xlfn." | "_xlws." | "_xlpm.") => w = &w[6..],
            _ => return w,
        }
    }
}

/// Strip whitespace outside string literals for Excel OpenXML compatibility.
pub fn strip_formula_spaces(formula: &str) -> String {
    let mut out = String::with_capacity(formula.len());
    let mut in_str = false;
    for c in formula.chars() {
        if c == '"' {
            in_str = !in_str;
            out.push(c);
        } else if in_str || !c.is_whitespace() {
            out.push(c);
        }
    }
    out
}

/// A formula as the user sees it, as a file stores it: the functions that
/// need a prefix get it (`_xlfn.`, `_xlfn._xlws.`), and the parameters of
/// LAMBDA and LET get `_xlpm.`.
pub fn to_file(text: &str) -> String {
    let text = &implicit_to_single_calls(&spills_to_anchor_arrays(text));
    let Ok(tokens) = tokenize(text) else { return text.to_string() };
    let (params, optional) = lambda_parameters(&tokens);
    let mut out = String::with_capacity(text.len() + 8);
    let mut at = 0;
    for (i, t) in tokens.iter().enumerate() {
        // An optional parameter `[b]` of LAMBDA is `_xlop.b` in the file (GitHub issue #168).
        if optional.contains(&i)
            && let Token::Brackets(raw) = &t.token
        {
            out.push_str(&text[at..t.start]);
            out.push_str("_xlop.");
            out.push_str(&raw[1..raw.len() - 1]);
            at = t.end;
            continue;
        }
        let Token::Word(w) = &t.token else { continue };
        let is_call = matches!(tokens.get(i + 1).map(|t| &t.token), Some(Token::LParen));
        // A function named alone, as an argument (`BYROW(A1:C3, SUM)`): `_xleta.SUM`.
        let before = i.checked_sub(1).map(|k| &tokens[k].token);
        let after = tokens.get(i + 1).map(|t| &t.token);
        let alone = !is_call
            && takes_functions(&tokens, i)
            && matches!(before, Some(Token::LParen | Token::Comma))
            && matches!(after, Some(Token::Comma | Token::RParen))
            && !w.eq_ignore_ascii_case("TRUE")
            && !w.eq_ignore_ascii_case("FALSE")
            && parser::parse_cell(w).is_none()
            && !params.iter().any(|p| p.eq_ignore_ascii_case(w));
        let eta = if alone { function_info(&w.to_uppercase()).map(|f| format!("_xleta.{}", f.prefix)) } else { None };
        let prefix = if let Some(eta) = &eta {
            eta.as_str()
        } else if is_call {
            function_info(&w.to_uppercase()).map_or("", |f| f.prefix)
        } else if params.iter().any(|p| p.eq_ignore_ascii_case(w)) {
            "_xlpm."
        } else {
            ""
        };
        if !prefix.is_empty() && !w.get(..3).is_some_and(|p|p.eq_ignore_ascii_case("_xl")) {
            out.push_str(&text[at..t.start]);
            out.push_str(prefix);
            out.push_str(w);
            at = t.end;
        }
    }
    out.push_str(&text[at..]);
    out
}

/// Whether the call token `i` is an argument of is one that takes functions
/// (BYROW, GROUPBY…): a name there that is a function's is that function.
fn takes_functions(tokens: &[lexer::Spanned], i: usize) -> bool {
    let mut depth = 0i32;
    for k in (0..i).rev() {
        match &tokens[k].token {
            Token::RParen => depth += 1,
            Token::LParen if depth > 0 => depth -= 1,
            Token::LParen => {
                let Some(Token::Word(f)) = k.checked_sub(1).map(|j| &tokens[j].token) else { return false };
                let name = strip_prefixes(f).to_ascii_uppercase();
                return matches!(name.as_str(), "BYROW" | "BYCOL" | "MAP" | "REDUCE" | "SCAN" | "GROUPBY" | "PIVOTBY" | "MAKEARRAY" | "LET");
            }
            _ => {}
        }
    }
    false
}

/// Rewrite only parser-recognised @ operators, never @ inside strings or table brackets.
fn implicit_to_single_calls(text: &str) -> String {
    if !text.contains('@') { return text.to_string(); }
    let Ok(spans) = parser::implicit_spans(text) else { return text.to_string() };
    let mut starts = std::collections::HashSet::new();
    let mut ends = std::collections::HashMap::<usize, usize>::new();
    for (start, end) in spans { starts.insert(start); *ends.entry(end).or_default() += 1; }
    let mut out = String::with_capacity(text.len());
    for (at, c) in text.char_indices() {
        if let Some(count) = ends.get(&at) { out.push_str(&")".repeat(*count)); }
        if starts.contains(&at) { out.push_str("_xlfn.SINGLE("); } else { out.push(c); }
    }
    if let Some(count) = ends.get(&text.len()) { out.push_str(&")".repeat(*count)); }
    out
}

/// Read the file's SINGLE form as the implicit-intersection operator.
fn single_calls_to_implicit(text: &str) -> String {
    let mut out = text.to_string();
    loop {
        let Ok(tokens) = tokenize(&out) else { return out };
        let mut replacement = None;
        for i in (0..tokens.len()).rev() {
            if !matches!(&tokens[i].token, Token::Word(w) if !w.to_ascii_lowercase().starts_with("_xlpm.") && strip_prefixes(w).eq_ignore_ascii_case("SINGLE"))
                || tokens.get(i + 1).map(|t| &t.token) != Some(&Token::LParen) { continue; }
            let mut depth = 1;
            for j in i + 2..tokens.len() {
                match &tokens[j].token {
                    Token::LParen => depth += 1,
                    Token::RParen => depth -= 1,
                    _ => (),
                }
                if depth != 0 { continue; }
                if tokens.get(j + 1).map(|t| &t.token) == Some(&Token::LParen) { break; }
                let inner = &out[tokens[i + 1].end..tokens[j].start];
                if let Ok(expr) = parse(inner) {
                    let simple = matches!(expr, Expr::Ref(_) | Expr::Range(..) | Expr::Bool(_) | Expr::Name { .. } | Expr::Call { .. } | Expr::Invoke { .. } | Expr::Structured { .. } | Expr::Spill(_) | Expr::Implicit(_) | Expr::Number(_) | Expr::Text(_) | Expr::Error(_) | Expr::Array(_));
                    let operator = if simple || inner.trim_start().starts_with('(') { format!("@{inner}") } else { format!("@({inner})") };
                    let suffix_binds = matches!(tokens.get(j + 1).map(|t| &t.token), Some(Token::Op(lexer::Op::Range) | Token::Spill));
                    replacement = Some((tokens[i].start, tokens[j].end, if suffix_binds { format!("({operator})") } else { operator }));
                }
                break;
            }
            if replacement.is_some() { break; }
        }
        let Some((start, end, value)) = replacement else { return out };
        out.replace_range(start..end, &value);
    }
}

/// `A1#` as a file stores it: `_xlfn.ANCHORARRAY(A1)`.
fn spills_to_anchor_arrays(text: &str) -> String {
    if !text.contains('#') {
        return text.to_string();
    }
    let Ok(tokens) = tokenize(text) else { return text.to_string() };
    let mut out = String::with_capacity(text.len() + 20);
    let mut at = 0;
    for (i, t) in tokens.iter().enumerate() {
        if t.token != Token::Spill || i == 0 {
            continue;
        }
        if !matches!(&tokens[i - 1].token, Token::Word(_) | Token::Error(_)) { continue; }
        let start = match tokens.get(i.wrapping_sub(2)).map(|t| &t.token) {
            Some(Token::Sheet { .. }) if i >= 2 => tokens[i - 2].start,
            _ => tokens[i - 1].start,
        };
        out.push_str(&text[at..start]);
        out.push_str("_xlfn.ANCHORARRAY(");
        out.push_str(&text[start..tokens[i - 1].end]);
        out.push(')');
        at = t.end;
    }
    out.push_str(&text[at..]);
    out
}

/// `_xlfn.ANCHORARRAY(A1)` from a file, as the user sees it: `A1#`.
fn anchor_arrays_to_spills(text: &str) -> String {
    if !text.to_ascii_uppercase().contains("ANCHORARRAY") {
        return text.to_string();
    }
    let Ok(tokens) = tokenize(text) else { return text.to_string() };
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    let mut i = 0;
    while i < tokens.len() {
        let is_anchor = matches!(&tokens[i].token, Token::Word(w) if !w.to_ascii_lowercase().starts_with("_xlpm.") && strip_prefixes(w).eq_ignore_ascii_case("ANCHORARRAY"));
        if is_anchor && tokens.get(i + 1).map(|t| &t.token) == Some(&Token::LParen) {
            // [sheet] word )
            let (inner_start, word) = match tokens.get(i + 2).map(|t| &t.token) {
                Some(Token::Sheet { .. }) => (tokens[i + 2].start, i + 3),
                _ => (tokens.get(i + 2).map_or(0, |t| t.start), i + 2),
            };
            if matches!(tokens.get(word).map(|t| &t.token), Some(Token::Word(_) | Token::Error(_))) && tokens.get(word + 1).map(|t| &t.token) == Some(&Token::RParen) {
                out.push_str(&text[at..tokens[i].start]);
                out.push_str(&text[inner_start..tokens[word].end]);
                out.push('#');
                at = tokens[word + 1].end;
                i = word + 2;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&text[at..]);
    out
}

/// The names LAMBDA and LET bind: LAMBDA's arguments but the last, LET's
/// odd arguments but the last.
fn lambda_parameters(tokens: &[lexer::Spanned]) -> (Vec<String>, Vec<usize>) {
    let mut names = Vec::new();
    let mut optional = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let Token::Word(w) = &t.token else { continue };
        let upper = w.to_uppercase();
        let upper = strip_prefixes(&upper);
        let (is_lambda, is_let) = (upper == "LAMBDA", upper == "LET");
        if !(is_lambda || is_let) || tokens.get(i + 1).map(|t| &t.token) != Some(&Token::LParen) {
            continue;
        }
        // Split the arguments at the top level.
        let mut depth = 0;
        let mut args: Vec<Vec<usize>> = vec![Vec::new()];
        for (k, t) in tokens.iter().enumerate().skip(i + 2) {
            match &t.token {
                Token::LParen | Token::LBrace => depth += 1,
                Token::RParen | Token::RBrace if depth == 0 => break,
                Token::RParen | Token::RBrace => depth -= 1,
                Token::Comma if depth == 0 => {
                    args.push(Vec::new());
                    continue;
                }
                _ => {}
            }
            args.last_mut().unwrap().push(k);
        }
        let count = args.len();
        for (k, arg) in args.iter().enumerate() {
            let binds = if is_lambda { k + 1 < count } else { k % 2 == 0 && k + 1 < count };
            if !binds {
                continue;
            }
            match arg[..] {
                [at] => match &tokens[at].token {
                    Token::Word(name) => names.push(name.to_string()),
                    // `[b]`: an optional parameter of LAMBDA.
                    Token::Brackets(raw) if is_lambda && raw.len() > 2 => {
                        names.push(raw[1..raw.len() - 1].to_string());
                        optional.push(at);
                    }
                    _ => {}
                },
                _ => {}
            }
        }
    }
    (names, optional)
}

/// Move the relative references of `formula` by `rows` and `columns` (a
/// formula copied, or a shared formula at one of its cells). Fixed parts
/// (`$`) stay. A reference moved off the sheet becomes `#REF!`.
pub fn shift(formula: &str, rows: i64, columns: i64) -> String {
    rewrite_references(formula, |r| {
        let mut r = r.clone();
        for corner in [&mut r.start, &mut r.end] {
            if !corner.row_fixed && !r.whole_columns {
                let v = corner.row as i64 + rows;
                if !(0..=MAX_ROW as i64).contains(&v) {
                    return None;
                }
                corner.row = v as u32;
            }
            if !corner.column_fixed && !r.whole_rows {
                let v = corner.column as i64 + columns;
                if !(0..=MAX_COLUMN as i64).contains(&v) {
                    return None;
                }
                corner.column = v as u32;
            }
        }
        Some(r)
    })
}

/// The formula of a cell copied from (`from_row`, `from_column`) and pasted transposed at (`row`, `column`) (形式を選択して貼り付け
/// with 行列を入れ替える): the offset of every relative reference swaps its rows and columns (`A1` in `B1`, pasted one down and
/// one across, is the cell above), and a fixed part (`$`) stays fixed on the other axis. Whole rows and columns are left as
/// written. A reference moved off the sheet becomes `#REF!`.
pub fn transpose(formula: &str, from_row: u32, from_column: u32, row: u32, column: u32) -> String {
    rewrite_references(formula, |r| {
        if r.whole_columns || r.whole_rows {
            return Some(r.clone());
        }
        let mut out = r.clone();
        for (new, old) in [(&mut out.start, &r.start), (&mut out.end, &r.end)] {
            // The new row comes from the old column, and the new column from the old row.
            if old.column_fixed {
                new.row = old.column;
                new.row_fixed = true;
            } else {
                let v = row as i64 + (old.column as i64 - from_column as i64);
                if !(0..=MAX_ROW as i64).contains(&v) {
                    return None;
                }
                new.row = v as u32;
                new.row_fixed = false;
            }
            if old.row_fixed {
                new.column = old.row;
                new.column_fixed = true;
            } else {
                let v = column as i64 + (old.row as i64 - from_row as i64);
                if !(0..=MAX_COLUMN as i64).contains(&v) {
                    return None;
                }
                new.column = v as u32;
                new.column_fixed = false;
            }
        }
        Some(out)
    })
}

/// Rewrite each reference of `formula` (cells, ranges, whole rows and
/// columns, with their sheet) through `f`; `None` makes it `#REF!`. The
/// rest of the text is kept as written.
pub fn rewrite_references(formula: &str, mut f: impl FnMut(&AreaRef) -> Option<AreaRef>) -> String {
    let Ok(tokens) = tokenize(formula) else { return formula.to_string() };
    let mut out = String::with_capacity(formula.len());
    let mut at = 0;
    let mut i = 0;
    while i < tokens.len() {
        // A reference: [sheet] word [: word], or [sheet] number : number.
        let start = i;
        let mut j = i;
        let sheet = match &tokens[j].token {
            Token::Sheet { first, last, book, path } => {
                j += 1;
                Some(SheetRef { first: first.clone(), last: last.clone(), book: book.clone(), path: path.clone() })
            }
            _ => None,
        };
        let text_of = |k: usize| -> Option<String> {
            match &tokens.get(k)?.token {
                Token::Word(w) => Some(w.clone()),
                Token::Number(_) => Some(formula[tokens[k].start..tokens[k].end].to_string()),
                _ => None,
            }
        };
        let Some(first) = text_of(j) else {
            i += 1;
            continue;
        };
        // Not a function name or a table name.
        if matches!(tokens.get(j + 1).map(|t| &t.token), Some(Token::LParen | Token::Brackets(_))) {
            i = j + 1;
            continue;
        }
        let range_end = if tokens.get(j + 1).map(|t| &t.token) == Some(&Token::Op(lexer::Op::Range)) { text_of(j + 2) } else { None };
        let parsed = range_end.as_ref().and_then(|second| {
            let expr = parse(&format!("{first}:{second}")).ok()?;
            match expr {
                Expr::Ref(r) => Some((r, j + 3)),
                _ => None,
            }
        });
        let parsed = parsed.or_else(|| match parse(&first) {
            Ok(Expr::Ref(r)) if !matches!(tokens[j].token, Token::Number(_)) => Some((r, j + 1)),
            _ => None,
        });
        let Some((mut area, next)) = parsed else {
            i = j + 1;
            continue;
        };
        area.sheet = sheet;
        out.push_str(&formula[at..tokens[start].start]);
        match f(&area) {
            Some(new) => out.push_str(&write_reference(&new)),
            None => {
                // The sheet stays (`Sheet1!#REF!`), as Excel writes it; a
                // workbook that is gone does not (its number would mean another).
                if let Some(s) = area.sheet.as_ref().filter(|s| s.book.is_none()) {
                    out.push_str(&sheet_prefix(s));
                }
                out.push_str("#REF!");
            }
        }
        at = tokens[next - 1].end;
        i = next;
    }
    out.push_str(&formula[at..]);
    out
}

/// What Excel makes of a formula typed in when it is entered (#185): the letters of cell and range references, whole columns
/// and rows (`a1` -> `A1`, `$b$2:c3` -> `$B$2:C3`, `a:a` -> `A:A`), the names of the functions the engine has (`sum` -> `SUM`) and
/// `TRUE`/`FALSE` go to upper case, and a sheet written before a reference takes the case of the sheet's own name (`sheet2!c3` ->
/// `Sheet2!C3`, `sheets` being the names of the workbook's sheets). Text in quotes, defined names, table and column names, and the
/// spaces and everything else stay as typed. The formula is read by the lexer and the parser (no pattern on the text); one that does not
/// read is returned as it is.
pub fn normalize_case(formula: &str, sheets: &[&str]) -> String {
    let refs = rewrite_references(formula, |area| {
        let mut area = area.clone();
        if let Some(sheet) = area.sheet.as_mut().filter(|s| s.book.is_none()) {
            let own = |name: &str| sheets.iter().find(|s| s.eq_ignore_ascii_case(name)).map(|s| s.to_string());
            if let Some(n) = own(&sheet.first) {
                sheet.first = n;
            }
            if let Some(n) = sheet.last.as_deref().and_then(own) {
                sheet.last = Some(n);
            }
        }
        Some(area)
    });
    let Ok(tokens) = tokenize(&refs) else { return refs };
    let mut out = String::with_capacity(refs.len());
    let mut at = 0;
    for (i, t) in tokens.iter().enumerate() {
        let Token::Word(w) = &t.token else { continue };
        let call = matches!(tokens.get(i + 1).map(|n| &n.token), Some(Token::LParen));
        let upper = w.to_uppercase();
        let is_function = call && !w.contains('_') && w.is_ascii() && crate::functions::lookup(&upper).is_some();
        let is_logical = !call && w.is_ascii() && (upper == "TRUE" || upper == "FALSE") && !matches!(i.checked_sub(1).map(|k| &tokens[k].token), Some(Token::Sheet { .. }));
        if (is_function || is_logical) && upper != *w {
            out.push_str(&refs[at..t.start]);
            out.push_str(&upper);
            at = t.end;
        }
    }
    out.push_str(&refs[at..]);
    out
}

#[cfg(test)]
mod normalize_case_tests {
    use super::normalize_case;

    #[test]
    fn references_and_functions_go_to_upper_case() {
        let n = |f: &str| normalize_case(f, &["Sheet1", "Sheet2", "My Data"]);
        assert_eq!(n("a1"), "A1");
        assert_eq!(n("sum(a1:b2)+\"abc\""), "SUM(A1:B2)+\"abc\"");
        assert_eq!(n("$a$1+b$2"), "$A$1+B$2");
        assert_eq!(n("sheet2!c3"), "Sheet2!C3");
        assert_eq!(n("'my data'!c3"), "'My Data'!C3");
        assert_eq!(n("sum(a:a)+sum(1:3)+sum($b:c)"), "SUM(A:A)+SUM(1:3)+SUM($B:C)");
        assert_eq!(n("if(a1>1, true, false)"), "IF(A1>1, TRUE, FALSE)");
        assert_eq!(n("sum( a1 , b2 )"), "SUM( A1 , B2 )");
    }

    #[test]
    fn text_names_and_spacing_stay() {
        let n = |f: &str| normalize_case(f, &["Sheet1"]);
        assert_eq!(n("\"a1 sum(b2)\"&a1"), "\"a1 sum(b2)\"&A1");
        // A defined name keeps its case, and a function the engine does not have (a LAMBDA's name) is left.
        assert_eq!(n("myRate*a1"), "myRate*A1");
        assert_eq!(n("myFunc(a1)"), "myFunc(A1)");
        assert_eq!(n("Table1[Col a]+sum(a1)"), "Table1[Col a]+SUM(A1)");
        assert_eq!(n("[1]sheet1!a1"), "[1]sheet1!A1");
        // An unfinished formula is corrected as far as it reads.
        assert_eq!(n("sum(a1"), "SUM(A1");
        assert_eq!(n("SUM(A1)"), "SUM(A1)");
    }
}

/// Whether `formula` refers to cells of an external workbook (`[1]Sheet1!A1`,
/// `[Book.xlsx]Sheet1!A1`).
pub fn has_external_book(formula: &str) -> bool {
    formula.contains('[') && tokenize(formula).is_ok_and(|t| t.iter().any(|t| matches!(&t.token, Token::Sheet { book: Some(_), .. })))
}

/// A file's formula with its external workbooks named as Excel shows them:
/// `[1]Sheet1!A1` as `[Book.xlsx]Sheet1!A1`, `books` being the workbook's
/// external links in order (a link with no name is left as its number).
pub fn books_by_name(formula: &str, books: &[String]) -> String {
    if !has_external_book(formula) {
        return formula.to_string();
    }
    rewrite_references(formula, |r| {
        let mut r = r.clone();
        if let Some(s) = r.sheet.as_mut()
            && let Some(name) = s.book.as_deref().and_then(|b| b.parse::<usize>().ok()).and_then(|n| books.get(n.checked_sub(1)?)).filter(|n| !n.is_empty())
        {
            s.book = Some(name.clone());
        }
        Some(r)
    })
}

fn book_raw_name(b: &str) -> &str {
    if let Some(pos) = b.find('[')
        && let Some(end) = b[pos + 1..].find(']')
    {
        &b[pos + 1..pos + 1 + end]
    } else {
        b
    }
}

/// The reverse of [`books_by_name`]: the formula as a file stores it, with the
/// external workbooks by number.
pub fn books_by_index(formula: &str, books: &[String]) -> String {
    if !has_external_book(formula) {
        return formula.to_string();
    }
    rewrite_references(formula, |r| {
        let mut r = r.clone();
        if let Some(s) = r.sheet.as_mut()
            && let Some(i) = s.book.as_deref().and_then(|b| {
                let raw = book_raw_name(b);
                books.iter().position(|n| {
                    !n.is_empty() && (n.eq_ignore_ascii_case(b) || n.eq_ignore_ascii_case(raw) || book_raw_name(n).eq_ignore_ascii_case(raw) || b.ends_with(n))
                })
            })
        {
            s.book = Some((i + 1).to_string());
            s.path = None;
        }
        Some(r)
    })
}

/// A file's formula with its external workbooks renumbered: `[n]` becomes
/// `[map[n]]` (`map` by the old number, 1-based, as `(old, new)` pairs);
/// a workbook with no new number (a broken link) makes its references
/// `#REF!`.
pub fn renumber_books(formula: &str, map: &[(usize, Option<usize>)]) -> String {
    if !has_external_book(formula) {
        return formula.to_string();
    }
    rewrite_references(formula, |r| {
        let mut r = r.clone();
        if let Some(s) = r.sheet.as_mut()
            && let Some(n) = s.book.as_deref().and_then(|b| b.parse::<usize>().ok())
            && let Some(&(_, new)) = map.iter().find(|(old, _)| *old == n)
        {
            match new {
                Some(new) => s.book = Some(new.to_string()),
                None => return None,
            }
        }
        Some(r)
    })
}

/// A formula whose references to the external workbook `name` are `#REF!`
/// (a broken link), as Excel leaves a name that pointed into it.
pub fn without_book(formula: &str, name: &str) -> String {
    if !has_external_book(formula) {
        return formula.to_string();
    }
    let target_raw = book_raw_name(name);
    rewrite_references(formula, |r| {
        if r.sheet.as_ref().and_then(|s| s.book.as_deref()).is_some_and(|b| {
            let b_raw = book_raw_name(b);
            b.eq_ignore_ascii_case(name) || b.eq_ignore_ascii_case(target_raw) || b_raw.eq_ignore_ascii_case(name) || b_raw.eq_ignore_ascii_case(target_raw)
        }) {
            None
        } else {
            Some(r.clone())
        }
    })
}

/// A formula whose references to the external workbook `old_name` are changed to `new_name`,
/// optionally updating its path prefix.
pub fn rename_book(formula: &str, old_name: &str, new_name: &str, new_path: Option<&str>) -> String {
    if !has_external_book(formula) {
        return formula.to_string();
    }
    rewrite_references(formula, |r| {
        let mut r = r.clone();
        if let Some(s) = r.sheet.as_mut() && s.book.as_deref().is_some_and(|b| b.eq_ignore_ascii_case(old_name)) {
            s.book = Some(new_name.to_string());
            if let Some(p) = new_path {
                s.path = if p.is_empty() { None } else { Some(p.to_string()) };
            }
        }
        Some(r)
    })
}

/// The directory path prefix of an external workbook `book_name` if explicitly written in `formula`.
pub fn book_path_in_formula(formula: &str, book_name: &str) -> Option<String> {
    let tokens = tokenize(formula).ok()?;
    for t in tokens {
        if let Token::Sheet { book: Some(b), path: Some(p), .. } = t.token {
            if b.eq_ignore_ascii_case(book_name) {
                return Some(p);
            }
        }
    }
    None
}

/// The external workbooks `formula` names (not by number) that are not among `books`.
pub fn unknown_books(formula: &str, books: &[String]) -> Vec<String> {
    let Ok(tokens) = tokenize(formula) else { return Vec::new() };
    tokens
        .iter()
        .filter_map(|t| match &t.token {
            Token::Sheet { book: Some(b), .. } if b.parse::<usize>().is_err() => {
                let raw = book_raw_name(b);
                let known = books.iter().any(|n| {
                    !n.is_empty() && (n.eq_ignore_ascii_case(b) || n.eq_ignore_ascii_case(raw) || book_raw_name(n).eq_ignore_ascii_case(raw))
                });
                if known { None } else { Some(b.clone()) }
            }
            _ => None,
        })
        .collect()
}

/// Rewrite each structured reference of `formula` (`Table1[Qty]`, or
/// `[@Qty]` without its table) through `f` (the table's name, empty when
/// left out, and the part in brackets): `Some` text replaces it, `None`
/// keeps it. The rest of the text is kept as written.
pub fn rewrite_structured(formula: &str, mut f: impl FnMut(&str, &str) -> Option<String>) -> String {
    let Ok(tokens) = tokenize(formula) else { return formula.to_string() };
    let mut out = String::with_capacity(formula.len());
    let mut at = 0;
    let mut i = 0;
    while i < tokens.len() {
        let (table, spec, first, last) = match (&tokens[i].token, tokens.get(i + 1).map(|t| &t.token)) {
            (Token::Word(w), Some(Token::Brackets(spec))) => (w.clone(), spec.clone(), i, i + 1),
            (Token::Brackets(spec), _) => (String::new(), spec.clone(), i, i),
            _ => {
                i += 1;
                continue;
            }
        };
        if let Some(text) = f(&table, &spec) {
            out.push_str(&formula[at..tokens[first].start]);
            out.push_str(&text);
            at = tokens[last].end;
        }
        i = last + 1;
    }
    out.push_str(&formula[at..]);
    out
}

/// `formula` with the this-row shorthand of structured references (`[@Qty]`,
/// `Table1[@[Qty 1]]`) in the form a file stores them (`Table1[[#This Row],[Qty]]`):
/// the shorthand is how Excel shows them, and a file written with it does
/// not open. A reference with no table named is of `table` (the table the
/// cell is in); one with none to name stays as written.
pub fn this_row_to_file(formula: &str, table: Option<&str>) -> String {
    if !formula.contains("[@") {
        return formula.to_string();
    }
    rewrite_structured(formula, |named, spec| {
        let inner = spec.strip_prefix('[')?.strip_suffix(']')?;
        let rest = inner.strip_prefix('@')?;
        let table = if named.is_empty() { table? } else { named };
        let columns = if rest.is_empty() {
            String::new()
        } else if rest.starts_with('[') {
            format!(",{rest}")
        } else {
            format!(",[{rest}]")
        };
        Some(format!("{table}[[#This Row]{columns}]"))
    })
}

/// `formula` (as stored) with the this-row references of `table` (the table
/// the cell is in) in the short form Excel shows them: `Table1[[#This Row],[Qty]]`
/// is `[@Qty]`, and `[@[Qty 1]]` when the column's name has a space or a
/// symbol. One of another table, or of several columns, is left as it is.
pub fn this_row_to_display(formula: &str, table: Option<&str>) -> String {
    let Some(table) = table else { return formula.to_string() };
    if !formula.to_ascii_lowercase().contains("#this row") {
        return formula.to_string();
    }
    rewrite_structured(formula, |named, spec| {
        if !named.is_empty() && !named.eq_ignore_ascii_case(table) {
            return None;
        }
        let inner = spec.strip_prefix('[')?.strip_suffix(']')?;
        let head = inner.get(..11).filter(|h| h.eq_ignore_ascii_case("[#This Row]"))?;
        let rest = inner[head.len()..].strip_prefix(',')?;
        let column = rest.strip_prefix('[')?.strip_suffix(']')?;
        // A name with a bracket, a quote or a # needs escapes: left long.
        if column.is_empty() || column.contains(['[', ']', '\'', '#']) {
            return None;
        }
        let plain = column.chars().all(|c| c.is_alphanumeric() || c == '_' || (c as u32) > 0x7f && !c.is_whitespace());
        Some(if plain { format!("[@{column}]") } else { format!("[@[{column}]]") })
    })
}

/// A sheet name as a formula writes it: quoted when it has to be.
pub fn quote_sheet(name: &str) -> String {
    let plain = !name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.' || (c as u32) > 0x7f && !c.is_whitespace())
        && !name.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.')
        && parser::parse_cell(name).is_none()
        && !looks_like_r1c1(name)
        && !name.eq_ignore_ascii_case("TRUE")
        && !name.eq_ignore_ascii_case("FALSE");
    if plain { name.to_string() } else { format!("'{}'", name.replace('\'', "''")) }
}

fn looks_like_r1c1(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let b = upper.as_bytes();
    if b.first() != Some(&b'R') && b.first() != Some(&b'C') {
        return false;
    }
    upper.chars().all(|c| c == 'R' || c == 'C' || c.is_ascii_digit())
}

fn sheet_prefix(s: &SheetRef) -> String {
    let mut name = String::new();
    if let Some(path) = &s.path {
        name.push_str(path);
    }
    if let Some(book) = &s.book {
        if book.contains('[') && book.ends_with(']') {
            name.push_str(book);
        } else {
            name.push('[');
            name.push_str(book);
            name.push(']');
        }
    }
    name.push_str(&s.first);
    if let Some(last) = &s.last {
        name.push(':');
        name.push_str(last);
    }
    let quoted = s.path.is_some()
        || quote_sheet(&s.first) != s.first
        || s.last.as_ref().is_some_and(|l| quote_sheet(l) != *l)
        || s.book.as_ref().is_some_and(|b| b.contains(' ') || b.contains('\\') || b.contains('/') || b.contains(':'))
        || name.contains(' ');
    if quoted { format!("'{}'!", name.replace('\'', "''")) } else { format!("{name}!") }
}

fn column_letters(column: u32) -> String {
    let mut letters = String::new();
    let mut c = column + 1;
    while c > 0 {
        let rem = (c - 1) % 26;
        letters.insert(0, (b'A' + rem as u8) as char);
        c = (c - 1) / 26;
    }
    letters
}

/// A reference written out: `Sheet1!$A$1:B2`, `A:C`, `$3:$5`.
pub fn write_reference(r: &AreaRef) -> String {
    let mut out = r.sheet.as_ref().map(sheet_prefix).unwrap_or_default();
    let dollar = |fixed: bool| if fixed { "$" } else { "" };
    let col = |c: &Corner| format!("{}{}", dollar(c.column_fixed), column_letters(c.column));
    let row = |c: &Corner| format!("{}{}", dollar(c.row_fixed), c.row + 1);
    if r.whole_columns {
        out.push_str(&format!("{}:{}", col(&r.start), col(&r.end)));
    } else if r.whole_rows {
        out.push_str(&format!("{}:{}", row(&r.start), row(&r.end)));
    } else if r.is_cell() {
        out.push_str(&format!("{}{}", col(&r.start), row(&r.start)));
    } else {
        out.push_str(&format!("{}{}:{}{}", col(&r.start), row(&r.start), col(&r.end), row(&r.end)));
    }
    out
}

#[cfg(test)]
mod spill_text_tests {
    use super::{from_file, this_row_to_display, this_row_to_file, to_file};

    #[test]
    fn a_function_named_alone_is_written_as_the_file_has_it() {
        assert_eq!(to_file("BYROW(A1:C3,SUM)"), "_xlfn.BYROW(A1:C3,_xleta.SUM)");
        assert_eq!(to_file("GROUPBY(A1:A9,B1:B9,AVERAGE,0)"), "_xlfn.GROUPBY(A1:A9,B1:B9,_xleta.AVERAGE,0)");
        assert_eq!(from_file("_xlfn.BYROW(A1:C3,_xleta.SUM)"), "BYROW(A1:C3,SUM)");
        // A name of an ordinary function's argument is not a function.
        assert_eq!(to_file("SUM(A1,Rate)"), "SUM(A1,Rate)");
        assert_eq!(to_file("BYROW(A1:C3,LAMBDA(x,SUM(x)))"), "_xlfn.BYROW(A1:C3,_xlfn.LAMBDA(_xlpm.x,SUM(_xlpm.x)))");
    }

    #[test]
    fn this_row_shorthand_is_written_in_full() {
        assert_eq!(this_row_to_file("[@Qty]*2", Some("Table1")), "Table1[[#This Row],[Qty]]*2");
        assert_eq!(this_row_to_file("Table2[@[Unit Price]]+[@A]", Some("Table1")), "Table2[[#This Row],[Unit Price]]+Table1[[#This Row],[A]]");
        assert_eq!(this_row_to_file("SUM(Table1[@[A]:[C]])", None), "SUM(Table1[[#This Row],[A]:[C]])");
        assert_eq!(this_row_to_file("[@Qty]", None), "[@Qty]");
        assert_eq!(this_row_to_file("Table1[@]", None), "Table1[[#This Row]]");
        assert_eq!(this_row_to_file("A1+\"[@x]\"", Some("T")), "A1+\"[@x]\"");
    }

    #[test]
    fn this_row_shows_short() {
        let long = "IMAGE(IF(RIGHT(T1[[#This Row],[画像指定]],5)=\".avif\",1,T1[[#This Row],[a b]]))";
        assert_eq!(this_row_to_display(long, Some("T1")), "IMAGE(IF(RIGHT([@画像指定],5)=\".avif\",1,[@[a b]]))");
        assert_eq!(this_row_to_display("T2[[#This Row],[x]]", Some("T1")), "T2[[#This Row],[x]]");
        assert_eq!(this_row_to_display("T1[[#This Row],[a]:[b]]", Some("T1")), "T1[[#This Row],[a]:[b]]");
        assert_eq!(this_row_to_display("[[#This Row],[x]]", None), "[[#This Row],[x]]");
        assert_eq!(this_row_to_display("t1[[#This Row],[x]]+\"T1[[#This Row],[x]]\"", Some("T1")), "[@x]+\"T1[[#This Row],[x]]\"");
        assert_eq!(this_row_to_file(&this_row_to_display("T1[[#This Row],[a b]]", Some("T1")), Some("T1")), "T1[[#This Row],[a b]]");
    }

    #[test]
    fn implicit_intersection_file_representation() {
        for (typed, stored) in [
            ("@A1:A3", "_xlfn.SINGLE(A1:A3)"),
            ("1+@Sheet1!$B$2:$B$5*2", "1+_xlfn.SINGLE(Sheet1!$B$2:$B$5)*2"),
            ("@SUM(A1:A3)", "_xlfn.SINGLE(SUM(A1:A3))"),
            ("@@A1", "_xlfn.SINGLE(_xlfn.SINGLE(A1))"),
            ("@A1#", "_xlfn.SINGLE(_xlfn.ANCHORARRAY(A1))"),
            ("IF(A1=\"@\",@B1:B3,0)", "IF(A1=\"@\",_xlfn.SINGLE(B1:B3),0)"),
            ("Table1[@Column]", "Table1[@Column]"),
            ("@'空 白'!$A$1:$A$3", "_xlfn.SINGLE('空 白'!$A$1:$A$3)"),
            ("@(A1+B1)", "_xlfn.SINGLE((A1+B1))"),
            ("@{1,2;3,4}", "_xlfn.SINGLE({1,2;3,4})"),
            ("@A1%", "_xlfn.SINGLE(A1)%"),
        ] {
            assert_eq!(to_file(typed), stored, "{typed}");
            assert_eq!(from_file(stored), typed, "{stored}");
        }
    }

    #[test]
    fn functions_excel_stores_without_a_prefix() {
        // Excel writes these functions without `_xlfn.` (standard ECMA-376 / its own XFUN files),
        // though they came after Excel 2007; a prefixed one still reads.
        for name in ["NETWORKDAYS.INTL", "WORKDAY.INTL", "ISO.CEILING", "ECMA.CEILING"] {
            let typed = format!("{name}(1,2)");
            assert_eq!(to_file(&typed), typed);
            assert_eq!(from_file(&format!("_xlfn.{typed}")), typed);
        }
        assert_eq!(to_file("XLOOKUP(1,A1:A3,B1:B3)"), "_xlfn.XLOOKUP(1,A1:A3,B1:B3)");
    }

    #[test]
    fn single_conversion_preserves_scope_and_lambda_parameters() {
        assert_eq!(from_file("_xlfn.SINGLE(A1+B1)"), "@(A1+B1)");
        assert_eq!(from_file("_xlfn.SINGLE(A1):B3"), "(@A1):B3");
        assert_eq!(from_file("_xlfn.SINGLE(A1)#"), "(@A1)#");
        assert_eq!(from_file("_xlfn.SINGLE(A1,A2)"), "SINGLE(A1,A2)");
        assert_eq!(from_file("_xlpm.SINGLE(A1)"), "SINGLE(A1)");
        assert_eq!(from_file("_xlfn.SINGLE(LAMBDA(x,x))(2)"), "SINGLE(LAMBDA(x,x))(2)");
        assert_eq!(to_file("IF(A1=\"_xlfn.SINGLE(@A1)\",1,2)"), "IF(A1=\"_xlfn.SINGLE(@A1)\",1,2)");
    }

    #[test]
    fn anchor_arrays() {
        assert_eq!(to_file("SUM(A1#)"), "SUM(_xlfn.ANCHORARRAY(A1))");
        assert_eq!(to_file("SUM(Sheet1!$B$2#)*2"), "SUM(_xlfn.ANCHORARRAY(Sheet1!$B$2))*2");
        assert_eq!(from_file("SUM(_xlfn.ANCHORARRAY(A1))"), "SUM(A1#)");
        assert_eq!(from_file("SUM(_xlfn.ANCHORARRAY(Sheet1!$B$2))*2"), "SUM(Sheet1!$B$2#)*2");
        assert_eq!(to_file("IF(A1=\"#\",1,2)"), "IF(A1=\"#\",1,2)");
        for error in crate::value::CellError::ALL {
            let user=format!("SUM({}#)",error.as_str());let file=format!("SUM(_xlfn.ANCHORARRAY({}))",error.as_str());assert_eq!(to_file(&user),file);assert_eq!(from_file(&file),user);
        }
        for sheet in ["Source", "'売上 集計'", "'O''Brien'"] {
            for error in crate::value::CellError::ALL {
                let user=format!("SUM({sheet}!{}#)",error.as_str());
                let file=format!("SUM(_xlfn.ANCHORARRAY({sheet}!{}))",error.as_str());
                assert_eq!(to_file(&user),file); assert_eq!(from_file(&file),user);
            }
        }
        assert_eq!(from_file("_xlpm.ANCHORARRAY(A1)"),"ANCHORARRAY(A1)");

    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_workbooks_are_renumbered_and_broken() {
        assert_eq!(renumber_books("[1]Alpha!B1+[3]Calc!D2+[2]East!F3", &[(1, None), (2, Some(1)), (3, Some(2))]), "#REF!+[2]Calc!D2+[1]East!F3");
        assert_eq!(renumber_books("SUM(A1:A3)", &[(1, None)]), "SUM(A1:A3)");
        assert_eq!(without_book("[予算.xlsx]Data!A1+Data!B1", "予算.xlsx"), "#REF!+Data!B1");
        assert_eq!(without_book("SUM('C:\\dir\\[予算.xlsx]Data:Summary'!A1)+Data!B1", "予算.xlsx"), "SUM(#REF!)+Data!B1");
    }

    #[test]
    fn external_workbooks_are_named_as_excel_shows_them() {
        let books = vec!["売上.xlsx".to_string(), String::new(), "Budget 2026.xlsx".to_string()];
        assert_eq!(books_by_name("SUM([1]Data!$A$1:$A$3)+[3]Plan!B2", &books), "SUM([売上.xlsx]Data!$A$1:$A$3)+'[Budget 2026.xlsx]Plan'!B2");
        assert_eq!(books_by_name("[2]Data!A1", &books), "[2]Data!A1", "a link with no name stays a number");
        assert_eq!(books_by_name("A1+Sheet2!B1", &books), "A1+Sheet2!B1");
        assert_eq!(books_by_index("SUM([売上.xlsx]Data!$A$1:$A$3)+'[Budget 2026.xlsx]Plan'!B2", &books), "SUM([1]Data!$A$1:$A$3)+[3]Plan!B2");
        assert_eq!(unknown_books("[新規.xlsx]S!A1+[1]S!A1+[売上.xlsx]S!A1", &books), ["新規.xlsx"]);
        assert!(!has_external_book("Table1[Qty]+A1"));
        assert_eq!(rename_book("=[Old.xlsx]Sheet1!A1+[Other.xlsx]Sheet1!B2", "Old.xlsx", "New.xlsx", None), "=[New.xlsx]Sheet1!A1+[Other.xlsx]Sheet1!B2");
        assert_eq!(rename_book("='[Old 2025.xlsx]Plan'!A1", "Old 2025.xlsx", "New 2026.xlsx", None), "='[New 2026.xlsx]Plan'!A1");
        assert_eq!(book_path_in_formula(r"='C:\data\[Book.xlsx]Sheet1'!A1", "Book.xlsx"), Some(r"C:\data\".into()));
        assert_eq!(books_by_index(r"='C:\data\[売上.xlsx]Data'!A1", &books), "=[1]Data!A1");

        // 3D references across sheets in external workbooks:
        assert_eq!(books_by_name("SUM([1]Data:Summary!$A$1:$A$3)+[3]Jan:Mar!B2", &books), "SUM([売上.xlsx]Data:Summary!$A$1:$A$3)+'[Budget 2026.xlsx]Jan:Mar'!B2");
        assert_eq!(books_by_index("SUM([売上.xlsx]Data:Summary!$A$1:$A$3)+'[Budget 2026.xlsx]Jan:Mar'!B2", &books), "SUM([1]Data:Summary!$A$1:$A$3)+[3]Jan:Mar!B2");

        // Directory path representations:
        assert_eq!(books_by_index("SUM('C:\\dir\\[売上.xlsx]Data'!A1)", &books), "SUM([1]Data!A1)");
        assert_eq!(books_by_index("AVERAGE('C:\\dir\\[売上.xlsx]Data:Summary'!A1:B2)", &books), "AVERAGE([1]Data:Summary!A1:B2)");
        assert_eq!(unknown_books("'C:\\dir\\[売上.xlsx]Data'!A1+'C:\\dir\\[Other.xlsx]Sheet1'!A1", &books), ["Other.xlsx"]);
    }

    #[test]
    fn prefixes() {
        assert_eq!(from_file("_xlfn.XLOOKUP(A1,_xlfn._xlws.SORT(B:B),C:C)"), "XLOOKUP(A1,SORT(B:B),C:C)");
        assert_eq!(to_file("XLOOKUP(A1,SORT(B:B),C:C)+SUM(1)"), "_xlfn.XLOOKUP(A1,_xlfn._xlws.SORT(B:B),C:C)+SUM(1)");
        assert_eq!(to_file("LET(x,1,x+1)"), "_xlfn.LET(_xlpm.x,1,_xlpm.x+1)");
        assert_eq!(from_file("_xlfn.LET(_xlpm.x,1,_xlpm.x+1)"), "LET(x,1,x+1)");
        for file in ["_xlfn.MAP({1;2},{3;4},_xlfn.LAMBDA(_xlpm.x0,_xlpm.x1,_xlpm.x0+_xlpm.x1))","_xlfn.LET(_xlpm.A1,5,_xlpm.A1+A1)","_xlfn.LET(_xlpm.TRUE,5,_xlpm.TRUE+IF(TRUE,1,0))","_xlfn.LET(_XLPM.A1,5,_xlpm.a1+A1)"] { assert_eq!(to_file(&from_file(file)),file); }
        // An optional LAMBDA parameter (GitHub issue #168): Excel writes `_xlop.b` where it is declared, `_xlpm.b` where it is used.
        assert_eq!(from_file("_xlfn.LAMBDA(_xlpm.a,_xlop.b,_xlpm.a)(10)"), "LAMBDA(a,[b],a)(10)");
        assert_eq!(to_file("LAMBDA(a,[b],a+b)(10)"), "_xlfn.LAMBDA(_xlpm.a,_xlop.b,_xlpm.a+_xlpm.b)(10)");
        assert_eq!(to_file(&from_file("_xlfn.LAMBDA(_xlpm.a,_xlop.b,_xlpm.a)(10)")), "_xlfn.LAMBDA(_xlpm.a,_xlop.b,_xlpm.a)(10)");
        assert_eq!(to_file("[b]+1"), "[b]+1");
        assert_eq!(to_file("CONCAT(\"XLOOKUP(\",A1)"), "_xlfn.CONCAT(\"XLOOKUP(\",A1)");
    }

    #[test]
    fn transposing() {
        // B1 =A1*2 pasted transposed at A2: the cell above.
        assert_eq!(transpose("A1*2", 0, 1, 1, 0), "A1*2");
        assert_eq!(transpose("A1*2", 0, 1, 5, 4), "E5*2");
        assert_eq!(transpose("SUM($A$1:B1)", 0, 2, 5, 4), "SUM($A$1:E5)");
        assert_eq!(transpose("$A1+B$2", 3, 3, 3, 3), "A$1+$B2");
    }

    #[test]
    fn shifting() {
        assert_eq!(shift("SUM(B2:B10)+$C$1*D$3", 2, 1), "SUM(C4:C12)+$C$1*E$3");
        assert_eq!(shift("'Sheet 1'!A1&\"A1\"", 1, 0), "'Sheet 1'!A2&\"A1\"");
        assert_eq!(shift("LOG10(A1)", 1, 0), "LOG10(A2)");
        assert_eq!(shift("A1+B:B+3:3", 1, 1), "B2+C:C+4:4");
        assert_eq!(shift("A1", -1, 0), "#REF!");
        assert_eq!(shift("Data!A1", -1, 0), "Data!#REF!");
        assert_eq!(shift("SUM(1:3)", 0, 5), "SUM(1:3)");
    }

    #[test]
    fn sheet_names() {
        assert_eq!(quote_sheet("Sheet1"), "Sheet1");
        assert_eq!(quote_sheet("My Sheet"), "'My Sheet'");
        assert_eq!(quote_sheet("A1"), "'A1'");
        assert_eq!(quote_sheet("2024"), "'2024'");
        assert_eq!(quote_sheet("売上"), "売上");
        assert_eq!(quote_sheet("it's"), "'it''s'");
    }
}

/// Rows or columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    Rows,
    Columns,
}

/// Rows or columns inserted (`count` > 0) before `at`, or deleted
/// (`count` < 0: `-count` of them from `at`), on the sheet `target`: the
/// formula `formula` of a cell on the sheet `own` with its references to
/// `target` moved, grown or shrunk as Excel does. A reference to cells that
/// are all deleted becomes `#REF!`.
pub fn adjust_references(formula: &str, own: &str, target: &str, axis: Axis, at: u32, count: i64) -> String {
    if surely_unaffected(formula, own, target, axis) {
        return formula.to_string();
    }
    rewrite_references(formula, |r| adjust_area(r, own, target, axis, at, count))
}

/// Whether `formula` of the sheet `own` certainly has nothing to change when
/// rows or columns of `target` come or go: it is of another sheet and every
/// place it names `target` is a whole column (for rows) or whole row (for
/// columns) range, or names it nowhere. (A workbook of hundreds of thousands
/// of formulas took seconds to read each one for nothing.) Conservative: `false`
/// whenever it is not sure.
fn surely_unaffected(formula: &str, own: &str, target: &str, axis: Axis) -> bool {
    if !(formula.is_ascii() && own.is_ascii() && target.is_ascii()) || own.eq_ignore_ascii_case(target) || target.is_empty() {
        return false;
    }
    let (f, t) = (formula.as_bytes(), target.as_bytes());
    // The extent of a whole-line range right after `!`: columns `$?A:$?B` for rows, rows `$?1:$?2` for columns.
    let whole = |mut i: usize| -> Option<usize> {
        let part = |i: &mut usize| -> bool {
            if f.get(*i) == Some(&b'$') {
                *i += 1;
            }
            let start = *i;
            while f.get(*i).is_some_and(|c| if axis == Axis::Rows { c.is_ascii_alphabetic() } else { c.is_ascii_digit() }) {
                *i += 1;
            }
            *i > start && *i - start <= if axis == Axis::Rows { 3 } else { 7 }
        };
        if !part(&mut i) || f.get(i) != Some(&b':') {
            return None;
        }
        i += 1;
        if !part(&mut i) {
            return None;
        }
        // Not the start of a longer reference, a function or another sheet.
        match f.get(i) {
            Some(c) if c.is_ascii_alphanumeric() || matches!(c, b'(' | b'!' | b':' | b'_' | b'.' | b'[' | b'$') => None,
            _ => Some(i),
        }
    };
    let mut from = 0;
    while from + t.len() <= f.len() {
        let Some(p) = (from..=f.len() - t.len()).find(|&p| f[p..p + t.len()].eq_ignore_ascii_case(t)) else { break };
        let mut i = p + t.len();
        // The name, as `Name!` or `'Name'!`; anything else (part of another name, a text) is not decided here.
        let quoted = p > 0 && f[p - 1] == b'\'';
        if quoted {
            if f.get(i) != Some(&b'\'') {
                return false;
            }
            i += 1;
        } else if p > 0 && (f[p - 1].is_ascii_alphanumeric() || matches!(f[p - 1], b'_' | b'.' | b'\'' | b'!' | b'$' | b':')) {
            return false;
        }
        if f.get(i) != Some(&b'!') {
            return false;
        }
        match whole(i + 1) {
            Some(end) => from = end,
            None => return false,
        }
    }
    true
}

/// One reference adjusted (see [`adjust_references`]); `None` for `#REF!`.
pub fn adjust_area(r: &AreaRef, own: &str, target: &str, axis: Axis, at: u32, count: i64) -> Option<AreaRef> {
    let sheet = match &r.sheet {
        None => own,
        Some(s) if s.book.is_some() || s.last.is_some() => return Some(r.clone()),
        Some(s) => s.first.as_str(),
    };
    if sheet.to_lowercase() != target.to_lowercase() {
        return Some(r.clone());
    }
    // Whole columns do not move with rows, nor whole rows with columns.
    if (axis == Axis::Rows && r.whole_columns) || (axis == Axis::Columns && r.whole_rows) {
        return Some(r.clone());
    }
    let max = if axis == Axis::Rows { MAX_ROW } else { MAX_COLUMN } as i64;
    let get = |c: &Corner| if axis == Axis::Rows { c.row } else { c.column } as i64;
    let (s, e) = (get(&r.start), get(&r.end));
    let at = at as i64;
    let (ns, ne) = if count > 0 {
        let n = count;
        let ns = if s >= at { s + n } else { s };
        let ne = if e >= at { e + n } else { e };
        // Pushed off the sheet: the part beyond is lost.
        if ns > max {
            return None;
        }
        (ns, ne.min(max))
    } else {
        let n = -count;
        let end_del = at + n - 1;
        if s >= at && e <= end_del {
            return None;
        }
        let ns = if s < at { s } else if s <= end_del { at } else { s - n };
        let ne = if e < at { e } else if e <= end_del { at - 1 } else { e - n };
        (ns, ne)
    };
    let mut out = r.clone();
    let set = |c: &mut Corner, v: i64| if axis == Axis::Rows { c.row = v as u32 } else { c.column = v as u32 };
    set(&mut out.start, ns);
    set(&mut out.end, ne);
    Some(out)
}

#[cfg(test)]
mod adjust_tests {
    use super::*;

    #[test]
    fn formulas_of_other_sheets_that_name_the_target_only_by_whole_lines_are_left_alone() {
        // Rows inserted in Raw: a whole column does not move, another sheet's cell neither.
        let f = "SUMIFS(Raw!C:C,Raw!$A:$A,$A2,Raw!$B:$B,\"Store\")";
        assert!(surely_unaffected(f, "Sales", "Raw", Axis::Rows));
        assert_eq!(adjust_references(f, "Sales", "Raw", Axis::Rows, 3, 1), f);
        // A cell of the target, the target's own formulas, a longer name, no `!`, whole rows for columns.
        assert!(!surely_unaffected("Raw!C2+1", "Sales", "Raw", Axis::Rows));
        assert!(!surely_unaffected("A1", "Raw", "Raw", Axis::Rows));
        assert!(!surely_unaffected("Raw_2!C:C", "Sales", "Raw", Axis::Rows));
        assert!(!surely_unaffected("Raw!C:C+Raw!D4", "Sales", "Raw", Axis::Rows));
        assert!(!surely_unaffected("Raw!C:C", "Sales", "Raw", Axis::Columns));
        assert!(surely_unaffected("Raw!3:4", "Sales", "Raw", Axis::Columns));
        assert!(surely_unaffected("1+Other!A1", "Sales", "Raw", Axis::Rows));
        assert_eq!(adjust_references("Raw!C:C+Raw!D4", "Sales", "Raw", Axis::Rows, 0, 1), "Raw!C:C+Raw!D5");
        assert_eq!(adjust_references("'My S'!B:B", "x", "My S", Axis::Rows, 0, 1), "'My S'!B:B");
    }

    #[test]
    fn inserting_and_deleting() {
        // Insert 2 rows before row 3 (index 2) of Sheet1.
        assert_eq!(adjust_references("SUM(B2:B4)+B1+B3", "Sheet1", "Sheet1", Axis::Rows, 2, 2), "SUM(B2:B6)+B1+B5");
        assert_eq!(adjust_references("Data!A5+A5", "Sheet1", "Data", Axis::Rows, 0, 1), "Data!A6+A5");
        assert_eq!(adjust_references("SUM(A:A)+SUM(3:4)", "S", "S", Axis::Rows, 0, 1), "SUM(A:A)+SUM(4:5)");
        // Delete rows 3–4 (index 2, two rows).
        assert_eq!(adjust_references("SUM(B2:B6)+B3+B7", "S", "S", Axis::Rows, 2, -2), "SUM(B2:B4)+#REF!+B5");
        assert_eq!(adjust_references("SUM(B3:B4)", "S", "S", Axis::Rows, 2, -2), "SUM(#REF!)");
        assert_eq!(adjust_references("$C$1*D1", "S", "S", Axis::Columns, 2, 1), "$D$1*E1");
        assert_eq!(adjust_references("'My S'!C1", "x", "My S", Axis::Columns, 0, -1), "'My S'!B1");
    }
}

/// References to the sheet `old` renamed `new` (quoted as need be).
pub fn rename_sheet(formula: &str, old: &str, new: &str) -> String {
    let Ok(tokens) = tokenize(formula) else { return formula.to_string() };
    let same = |s: &str| s.to_lowercase() == old.to_lowercase();
    let mut out = String::with_capacity(formula.len());
    let mut at = 0;
    for t in &tokens {
        if let Token::Sheet { first, last, book: None, .. } = &t.token
            && (same(first) || last.as_deref().is_some_and(same))
        {
            let first = if same(first) { new.to_string() } else { first.clone() };
            let last = last.as_ref().map(|l| if same(l) { new.to_string() } else { l.clone() });
            out.push_str(&formula[at..t.start]);
            out.push_str(&sheet_prefix(&SheetRef { first, last, book: None, path: None }));
            at = t.end;
        }
    }
    out.push_str(&formula[at..]);
    out
}

/// References to the sheet `name`, which is deleted, as `#REF!`.
pub fn remove_sheet(formula: &str, name: &str) -> String {
    let Ok(tokens) = tokenize(formula) else { return formula.to_string() };
    let same = |s: &str| s.to_lowercase() == name.to_lowercase();
    let mut out = String::with_capacity(formula.len());
    let mut at = 0;
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if let Token::Sheet { first, last, book: None, .. } = &t.token
            && (same(first) || last.as_deref().is_some_and(same))
        {
            // The sheet and the reference after it: A1, A1:B2, A:C, 1:3 or a name.
            let mut end = i + 1;
            if matches!(tokens.get(end).map(|t| &t.token), Some(Token::Word(_) | Token::Number(_))) {
                end += 1;
                if tokens.get(end).map(|t| &t.token) == Some(&Token::Op(lexer::Op::Range)) && matches!(tokens.get(end + 1).map(|t| &t.token), Some(Token::Word(_) | Token::Number(_))) {
                    end += 2;
                }
            }
            out.push_str(&formula[at..t.start]);
            out.push_str("#REF!");
            at = tokens[end - 1].end;
            i = end;
            continue;
        }
        i += 1;
    }
    out.push_str(&formula[at..]);
    out
}

/// The words of `formula` that may be defined names (or names of LAMBDA functions called as `Name(...)`),
/// each with the sheet written before it (`Sheet1!Name`), with `_xlfn.` and `_xlpm.` prefixes removed.
/// Cell references, functions and parameters are not told apart here: the caller keeps the words that
/// are names it knows.
pub fn name_candidates(formula: &str) -> Vec<(Option<String>, String)> {
    let Ok(tokens) = tokenize(formula) else { return Vec::new() };
    let mut out = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let Token::Word(w) = &t.token else { continue };
        let sheet = match i.checked_sub(1).map(|k| &tokens[k].token) {
            Some(Token::Sheet { first, book: None, .. }) => Some(first.clone()),
            _ => None,
        };
        out.push((sheet, strip_prefixes(w).to_string()));
    }
    out
}

/// Every use of the defined name `name` in `formula` (the word, with the sheet written before it, `Sheet1!Name`),
/// passed to `f` as the written sheet. `f` returns `Some(sheet)` to write the use as `sheet!Name` (`Some(None)`:
/// without a sheet) and `None` to leave it as it is. Used when a name changes its scope.
pub fn rewrite_name_uses(formula: &str, name: &str, mut f: impl FnMut(Option<&str>) -> Option<Option<String>>) -> String {
    let Ok(tokens) = tokenize(formula) else { return formula.to_string() };
    let mut out = String::with_capacity(formula.len());
    let mut at = 0;
    for (i, t) in tokens.iter().enumerate() {
        let Token::Word(w) = &t.token else { continue };
        if w.to_lowercase() != name.to_lowercase() {
            continue;
        }
        let (start, written) = match i.checked_sub(1).map(|k| &tokens[k]) {
            Some(lexer::Spanned { token: Token::Sheet { first, last: None, book: None, .. }, start, .. }) => (*start, Some(first.as_str())),
            _ => (t.start, None),
        };
        if start < at {
            continue;
        }
        let Some(new) = f(written) else { continue };
        out.push_str(&formula[at..start]);
        if let Some(sheet) = new {
            out.push_str(&quote_sheet(&sheet));
            out.push('!');
        }
        out.push_str(&formula[t.start..t.end]);
        at = t.end;
    }
    out.push_str(&formula[at..]);
    out
}

#[cfg(test)]
mod name_scope_tests {
    use super::*;

    #[test]
    fn uses_are_rewritten_with_their_sheet() {
        let to_sheet = |_: Option<&str>| Some(Some("Data 2".to_string()));
        assert_eq!(rewrite_name_uses("Rate*2+Other", "rate", to_sheet), "'Data 2'!Rate*2+Other");
        let to_book = |s: Option<&str>| s.map(|_| None);
        assert_eq!(rewrite_name_uses("Sheet1!Rate+Rate+SUM(Sheet1!A1)", "Rate", to_book), "Rate+Rate+SUM(Sheet1!A1)");
    }
}

#[cfg(test)]
mod name_candidate_tests {
    use super::*;

    #[test]
    fn finds_names_and_lambda_calls() {
        let c = name_candidates("Dbl(Rate)+Sheet1!Loc*_xlfn.Other(1)");
        let words: Vec<_> = c.iter().map(|(s, w)| (s.as_deref(), w.as_str())).collect();
        assert_eq!(words, vec![(None, "Dbl"), (None, "Rate"), (Some("Sheet1"), "Loc"), (None, "Other")]);
    }
}

#[cfg(test)]
mod sheet_name_tests {
    use super::*;

    #[test]
    fn renaming_and_removing() {
        assert_eq!(rename_sheet("Data!A1+data!B2:C3+'Other'!A1", "Data", "売上 2026"), "'売上 2026'!A1+'売上 2026'!B2:C3+'Other'!A1");
        assert_eq!(rename_sheet("SUM(Jan:Mar!A1)", "Mar", "End"), "SUM(Jan:End!A1)");
        assert_eq!(remove_sheet("Data!A1+SUM(Data!B2:C3)+X!A1", "data"), "#REF!+SUM(#REF!)+X!A1");
    }
}
