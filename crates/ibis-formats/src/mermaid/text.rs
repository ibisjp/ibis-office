// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/226)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Text helpers: labels, wrapping, width estimates and XML escaping.


pub(super) fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 && ((t.starts_with('"') && t.ends_with('"')) || (t.starts_with('\'') && t.ends_with('\''))) {
        t[1..t.len() - 1].to_string()
    } else {
        t.to_string()
    }
}

pub(super) fn push_centered_label(svg: &mut String, cx: f32, cy: f32, text: &str, font_size: f32, max_width: f32, fill: &str, weight: &str) {
    if let Some(inner) = markdown_string(text) {
        // A Markdown string: bold / italic runs as `<tspan>`s, wrapped like the plain lines.
        let lines = rich_lines(inner, max_width, font_size);
        let line_h = font_size * 1.25;
        let mut y = cy - lines.len() as f32 * line_h / 2.0 + font_size * 0.8;
        for line in &lines {
            svg.push_str(&format!(
                r##"<text x="{cx:.1}" y="{y:.1}" text-anchor="middle" font-size="{font_size}" font-weight="{weight}" fill="{fill}">{}</text>"##,
                runs_markup(line)
            ));
            y += line_h;
        }
        return;
    }
    let lines = label_lines(text, max_width, font_size);
    let line_h = font_size * 1.25;
    let block = lines.len() as f32 * line_h;
    let mut y = cy - block / 2.0 + font_size * 0.8;
    for line in &lines {
        svg.push_str(&format!(
            r##"<text x="{cx:.1}" y="{y:.1}" text-anchor="middle" font-size="{font_size}" font-weight="{weight}" fill="{fill}">{}</text>"##,
            escape_xml(line)
        ));
        y += line_h;
    }
}

/// `<br>` becomes a line break. A line wider than `max_width` wraps, using
/// [`estimate_text_width`] so Japanese characters take a full em.
pub(super) fn label_lines(text: &str, max_width: f32, font_size: f32) -> Vec<String> {
    if let Some(inner) = markdown_string(text) {
        return rich_lines(inner, max_width, font_size)
            .into_iter()
            .map(|line| line.into_iter().map(|run| run.text).collect())
            .collect();
    }
    let mut lines = Vec::new();
    for piece in break_label(text).split('\n') {
        if piece.is_empty() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            continue;
        }
        lines.extend(wrap_piece(piece, max_width.max(8.0), font_size));
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

pub(super) fn break_label(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let raw = text.as_bytes();
    let folded = lower.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut idx = 0usize;
    while idx < raw.len() {
        if folded[idx..].starts_with(b"<br")
            && let Some(rel) = folded[idx + 3..].iter().position(|ch| *ch == b'>')
        {
            let inside = &folded[idx + 3..idx + 3 + rel];
            if inside.iter().all(|ch| ch.is_ascii_whitespace() || *ch == b'/') {
                out.push('\n');
                idx += 3 + rel + 1;
                continue;
            }
        }
        let ch = text[idx..].chars().next().unwrap_or('\u{FFFD}');
        out.push(ch);
        idx += ch.len_utf8();
    }
    out
}

pub(super) fn wrap_piece(text: &str, max_width: f32, font_size: f32) -> Vec<String> {
    if estimate_text_width(text, font_size) <= max_width {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut width = 0.0f32;
    for token in split_wrap_tokens(text) {
        let token_width = estimate_text_width(&token, font_size);
        if current.is_empty() || width + token_width <= max_width {
            if token_width <= max_width || current.is_empty() && !token.chars().any(|ch| !ch.is_ascii() || ch.is_whitespace()) {
                if token_width <= max_width {
                    current.push_str(&token);
                    width += token_width;
                    continue;
                }
            }
        } else {
            lines.push(std::mem::take(&mut current));
            width = 0.0;
        }
        if estimate_text_width(&token, font_size) <= max_width {
            current = token;
            width = token_width;
            continue;
        }
        for ch in token.chars() {
            let ch_width = estimate_text_width(&ch.to_string(), font_size);
            if !current.is_empty() && width + ch_width > max_width {
                lines.push(std::mem::take(&mut current));
                width = 0.0;
            }
            current.push(ch);
            width += ch_width;
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    // A line that wrapped after a word does not keep the space.
    for line in &mut lines {
        let end = line.trim_end().len();
        line.truncate(end);
    }
    if lines.is_empty() { vec![String::new()] } else { lines }
}

pub(super) fn split_wrap_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut ascii_word = false;
    for ch in text.chars() {
        let word = ch.is_ascii() && !ch.is_ascii_whitespace();
        if current.is_empty() {
            current.push(ch);
            ascii_word = word;
            continue;
        }
        // A word ends at whitespace: `ab cd` is two tokens (`ab `, `cd`), so English wraps by word.
        let after_space = current.ends_with(char::is_whitespace);
        if ch.is_whitespace() || (ascii_word && word && !after_space) {
            current.push(ch);
            continue;
        }
        tokens.push(std::mem::take(&mut current));
        current.push(ch);
        ascii_word = word;
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

pub(super) fn estimate_text_width(s: &str, font_size: f32) -> f32 {
    // `#quot;` is one character once drawn.
    let decoded;
    let s = if s.contains('#') && s.contains(';') {
        decoded = decode_entities(s);
        decoded.as_str()
    } else {
        s
    };
    let mut units = 0.0f32;
    for ch in s.chars() {
        if ch.is_ascii() {
            units += 0.56;
        } else {
            units += 1.0;
        }
    }
    units * font_size
}

/// Text for an SVG text node: Mermaid entity codes (`#quot;`, `#9829;`) decoded, then escaped.
pub(super) fn escape_xml(s: &str) -> String {
    if s.contains('#') && s.contains(';') {
        escape_attr(&decode_entities(s))
    } else {
        escape_attr(s)
    }
}

/// An attribute value (a colour, a dash pattern) escaped as written, without entity codes.
pub(super) fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

// ----------------------------------------------------------------------------
// Mermaid entity codes and Markdown strings, shared by every diagram's labels.
// ----------------------------------------------------------------------------

/// Named entity codes Mermaid documents and diagrams commonly use (`#quot;`, `#infin;`).
const NAMED_ENTITIES: &[(&str, char)] = &[
    ("quot", '"'), ("amp", '&'), ("lt", '<'), ("gt", '>'), ("apos", '\''), ("nbsp", '\u{a0}'),
    ("copy", '©'), ("reg", '®'), ("trade", '™'), ("hellip", '…'), ("mdash", '—'), ("ndash", '–'),
    ("lsquo", '‘'), ("rsquo", '’'), ("ldquo", '“'), ("rdquo", '”'), ("laquo", '«'), ("raquo", '»'),
    ("larr", '←'), ("rarr", '→'), ("uarr", '↑'), ("darr", '↓'), ("harr", '↔'), ("lArr", '⇐'), ("rArr", '⇒'),
    ("infin", '∞'), ("hearts", '♥'), ("spades", '♠'), ("clubs", '♣'), ("diams", '♦'), ("deg", '°'),
    ("plusmn", '±'), ("times", '×'), ("divide", '÷'), ("micro", 'µ'), ("para", '¶'), ("sect", '§'),
    ("middot", '·'), ("bull", '•'), ("euro", '€'), ("yen", '¥'), ("pound", '£'), ("cent", '¢'),
    ("ne", '≠'), ("le", '≤'), ("ge", '≥'), ("asymp", '≈'), ("check", '✓'), ("star", '☆'),
];

/// Mermaid's entity codes: `#` + a name or a decimal code point + `;` (`#quot;`, `#9829;`).
/// Anything else, such as a colour `#fff`, is left alone.
pub(super) fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('#') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let code = after.find(';').filter(|&end| end > 0 && end <= 10).map(|end| &after[..end]);
        let decoded = code.and_then(|code| {
            if code.bytes().all(|b| b.is_ascii_digit()) {
                code.parse::<u32>().ok().and_then(char::from_u32).filter(|c| !c.is_control() || *c == '\n')
            } else {
                NAMED_ENTITIES.iter().find(|(name, _)| *name == code).map(|(_, c)| *c)
            }
        });
        match (code, decoded) {
            (Some(code), Some(ch)) => {
                out.push(ch);
                rest = &after[code.len() + 1..];
            }
            _ => {
                out.push('#');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A Markdown string label (`` "`The **cat**`" `` once unquoted): the text between the backticks.
pub(super) fn markdown_string(text: &str) -> Option<&str> {
    let t = text.trim();
    (t.len() >= 2 && t.starts_with('`') && t.ends_with('`')).then(|| &t[1..t.len() - 1])
}

/// A piece of label text and its emphasis.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct StyledRun {
    pub(super) text: String,
    pub(super) bold: bool,
    pub(super) italic: bool,
}

/// `**bold**` / `__bold__` and `*italic*` / `_italic_` as runs. An underscore only counts at a
/// word edge (so `LINE_ITEM` stays as written); a marker without its pair is plain text.
pub(super) fn markdown_runs(text: &str) -> Vec<StyledRun> {
    let chars: Vec<char> = text.chars().collect();
    let word = |i: usize| chars.get(i).is_some_and(|c| c.is_alphanumeric());
    // Where the marker that opens at `at` (`len` characters of `mark`) is closed.
    let closing = |at: usize, mark: char, len: usize| -> Option<usize> {
        if mark == '_' && at > 0 && word(at - 1) {
            return None;
        }
        let mut i = at + len;
        if chars.get(i).is_none_or(|c| c.is_whitespace()) {
            return None;
        }
        while i + len <= chars.len() {
            if chars[i..i + len].iter().all(|&c| c == mark)
                && i > at + len
                && !chars[i - 1].is_whitespace()
                && (len == 2 || chars.get(i + 1) != Some(&mark))
                && (mark != '_' || !word(i + len))
            {
                return Some(i);
            }
            i += 1;
        }
        None
    };
    let mut runs: Vec<StyledRun> = Vec::new();
    let push = |runs: &mut Vec<StyledRun>, ch: char, bold: bool, italic: bool| match runs.last_mut() {
        Some(last) if last.bold == bold && last.italic == italic => last.text.push(ch),
        _ => runs.push(StyledRun { text: ch.to_string(), bold, italic }),
    };
    // (closing index, marker length, mark) of the open spans.
    let mut open: Vec<(usize, usize, char)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(&(end, len, _)) = open.last()
            && i == end
        {
            open.pop();
            i += len;
            continue;
        }
        if c == '*' || c == '_' {
            let len = if chars.get(i + 1) == Some(&c) { 2 } else { 1 };
            let limit = open.last().map_or(chars.len(), |&(end, _, _)| end);
            if let Some(end) = closing(i, c, len).filter(|&end| end + len <= limit) {
                open.push((end, len, c));
                i += len;
                continue;
            }
        }
        let bold = open.iter().any(|&(_, len, _)| len == 2);
        let italic = open.iter().any(|&(_, len, _)| len == 1);
        push(&mut runs, c, bold, italic);
        i += 1;
    }
    runs
}

/// How much wider bold text is than regular text (measured in Chrome's sans-serif).
pub(super) const BOLD_WIDTH: f32 = 1.06;

/// The width of each line [`label_lines`] gives; bold runs of a Markdown string count wider.
pub(super) fn label_line_widths(text: &str, max_width: f32, font_size: f32) -> Vec<f32> {
    match markdown_string(text) {
        Some(inner) => rich_lines(inner, max_width, font_size)
            .iter()
            .map(|line| line.iter().map(|run| estimate_text_width(&run.text, font_size) * if run.bold { BOLD_WIDTH } else { 1.0 }).sum())
            .collect(),
        None => label_lines(text, max_width, font_size).iter().map(|line| estimate_text_width(line, font_size)).collect(),
    }
}

/// The lines of a Markdown string (`<br>` and newlines break; long lines wrap at `max_width`),
/// each a list of styled runs.
pub(super) fn rich_lines(text: &str, max_width: f32, font_size: f32) -> Vec<Vec<StyledRun>> {
    let max_width = max_width.max(8.0);
    let mut lines: Vec<Vec<StyledRun>> = vec![Vec::new()];
    let mut width = 0.0f32;
    let push = |line: &mut Vec<StyledRun>, text: &str, run: &StyledRun| match line.last_mut() {
        Some(last) if last.bold == run.bold && last.italic == run.italic => last.text.push_str(text),
        _ => line.push(StyledRun { text: text.to_string(), bold: run.bold, italic: run.italic }),
    };
    let broken = break_label(text);
    let source: Vec<&str> = broken.split('\n').map(str::trim).collect();
    for (k, piece) in source.iter().enumerate() {
        if k > 0 {
            lines.push(Vec::new());
            width = 0.0;
        }
        for run in markdown_runs(piece) {
            for token in split_wrap_tokens(&run.text) {
                let w = estimate_text_width(&token, font_size) * if run.bold { BOLD_WIDTH } else { 1.0 };
                let line = lines.last_mut().unwrap();
                if !line.is_empty() && width + w > max_width && !token.trim().is_empty() {
                    // Trailing spaces stay with the line they end.
                    if let Some(last) = line.last_mut() {
                        let trimmed = last.text.trim_end().len();
                        last.text.truncate(trimmed);
                    }
                    lines.push(Vec::new());
                    width = 0.0;
                }
                let line = lines.last_mut().unwrap();
                let token = if line.is_empty() { token.trim_start() } else { token.as_str() };
                push(line, token, &run);
                width += w;
            }
        }
    }
    for line in &mut lines {
        line.retain(|run| !run.text.is_empty());
    }
    lines
}

/// The content of a `<text>` for one line of runs: emphasis as `<tspan>`s.
pub(super) fn runs_markup(line: &[StyledRun]) -> String {
    let mut out = String::new();
    for run in line {
        let text = escape_xml(&run.text);
        match (run.bold, run.italic) {
            (false, false) => out.push_str(&text),
            (bold, italic) => out.push_str(&format!(
                r#"<tspan{}{}>{text}</tspan>"#,
                if bold { r#" font-weight="bold""# } else { "" },
                if italic { r#" font-style="italic""# } else { "" }
            )),
        }
    }
    out
}

/// One-line label text: a Markdown string (or, with `always`, any text) as styled `<tspan>`s,
/// other text escaped as is.
pub(super) fn label_markup(text: &str, always: bool) -> String {
    match markdown_string(text) {
        Some(inner) => runs_markup(&markdown_runs(&break_label(inner).replace('\n', " "))),
        None if always => runs_markup(&markdown_runs(text)),
        None => escape_xml(text),
    }
}

/// Mermaid lets a Markdown string (`` "`...`" ``) run over several source lines. This joins
/// those lines with `<br>` so the line-based parsers see one statement.
pub(super) fn join_markdown_strings(source: &str) -> std::borrow::Cow<'_, str> {
    if !source.contains("\"`") {
        return std::borrow::Cow::Borrowed(source);
    }
    let mut out = String::with_capacity(source.len());
    let mut inside = false;
    let mut chars = source.chars().peekable();
    let mut prev = '\0';
    while let Some(c) = chars.next() {
        if !inside && prev == '"' && c == '`' {
            inside = true;
        } else if inside && c == '`' && chars.peek() == Some(&'"') {
            inside = false;
        } else if inside && c == '\n' {
            out.push_str("<br>");
            while chars.peek().is_some_and(|c| *c == ' ' || *c == '\t') {
                chars.next();
            }
            prev = c;
            continue;
        }
        out.push(c);
        prev = c;
    }
    std::borrow::Cow::Owned(out)
}

/// `accTitle` / `accDescr` (including the `{ ... }` block form) for SVG `<title>` / `<desc>`.
pub(super) fn parse_acc_meta(source: &str) -> (Option<String>, Option<String>) {
    let mut title = None;
    let mut descr = None;
    let mut in_block = false;
    let mut block = String::new();
    for raw in source.lines() {
        let line = raw.trim();
        if in_block {
            if line == "}" || line.ends_with('}') && line.trim_start().starts_with('}') {
                in_block = false;
                let t = block.trim();
                if !t.is_empty() {
                    descr = Some(t.to_string());
                }
                block.clear();
            } else {
                if !block.is_empty() {
                    block.push('\n');
                }
                block.push_str(line);
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("accTitle:") {
            let t = rest.trim();
            if !t.is_empty() {
                title = Some(t.to_string());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("accTitle ") {
            let t = rest.trim().trim_start_matches(':').trim();
            if !t.is_empty() {
                title = Some(t.to_string());
            }
            continue;
        }
        if line == "accDescr {" || line.starts_with("accDescr {") {
            let rest = line["accDescr {".len()..].trim();
            if let Some(inner) = rest.strip_suffix('}') {
                let t = inner.trim();
                if !t.is_empty() {
                    descr = Some(t.to_string());
                }
            } else {
                in_block = true;
                if !rest.is_empty() {
                    block.push_str(rest);
                }
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("accDescr:") {
            let t = rest.trim();
            if !t.is_empty() {
                descr = Some(t.to_string());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("accDescr ") {
            let t = rest.trim().trim_start_matches(':').trim();
            if t == "{" {
                in_block = true;
            } else if !t.is_empty() {
                descr = Some(t.to_string());
            }
        }
    }
    (title, descr)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn plain(lines: &[Vec<StyledRun>]) -> Vec<String> {
        lines.iter().map(|l| l.iter().map(|r| r.text.as_str()).collect()).collect()
    }

    #[test]
    fn entity_codes_decode_in_text_but_not_in_attributes() {
        assert_eq!(decode_entities("A #quot;q#quot; #9829; #infin; #35;"), "A \"q\" ♥ ∞ #");
        assert_eq!(decode_entities("#fff; #nope; # ; #123456789012;"), "#fff; #nope; # ; #123456789012;");
        assert_eq!(escape_xml("say #quot;hi#quot;"), "say &quot;hi&quot;");
        assert_eq!(escape_attr("#123;"), "#123;");
        assert_eq!(estimate_text_width("#quot;", 10.0), estimate_text_width("\"", 10.0));
    }

    #[test]
    fn markdown_runs_bold_italic_and_word_underscores() {
        let runs = markdown_runs("This **is** _Markdown_");
        assert_eq!(
            runs.iter().map(|r| (r.text.as_str(), r.bold, r.italic)).collect::<Vec<_>>(),
            vec![("This ", false, false), ("is", true, false), (" ", false, false), ("Markdown", false, true)]
        );
        let nested = markdown_runs("**bold _both_**");
        assert_eq!(nested.last().map(|r| (r.text.as_str(), r.bold, r.italic)), Some(("both", true, true)), "{nested:?}");
        for literal in ["LINE_ITEM_ID", "2 * 3 * 4", "a ** b", "*open"] {
            let runs = markdown_runs(literal);
            assert_eq!(runs.len(), 1, "{literal}: {runs:?}");
            assert_eq!(runs[0].text, literal);
        }
    }

    #[test]
    fn rich_lines_break_and_wrap() {
        let lines = rich_lines("Line1<br>Line 2 with **bold**", 400.0, 13.0);
        assert_eq!(plain(&lines), vec!["Line1", "Line 2 with bold"]);
        assert!(lines[1].last().unwrap().bold);
        let wrapped = rich_lines("The dog in **the** hog... a *very long text* that wraps to a new line", 200.0, 13.0);
        assert!(wrapped.len() >= 2, "{wrapped:?}");
        assert!(plain(&wrapped).iter().all(|l| estimate_text_width(l, 13.0) <= 215.0 && !l.ends_with(' ')));
        assert!(wrapped.iter().flatten().any(|r| r.italic && r.text.contains("very")));
        assert_eq!(
            runs_markup(&markdown_runs("a **b** _c_ <d>")),
            r#"a <tspan font-weight="bold">b</tspan> <tspan font-style="italic">c</tspan> &lt;d&gt;"#
        );
        assert_eq!(label_markup("`**Two**`", false), r#"<tspan font-weight="bold">Two</tspan>"#);
        assert_eq!(label_markup("**Two**", false), "**Two**");
    }

    #[test]
    fn english_wraps_by_word() {
        assert_eq!(split_wrap_tokens("ab cd  ef"), vec!["ab ", "cd  ", "ef"]);
        let lines = label_lines("Important information! You can write notes.", 150.0, 11.0);
        assert!(lines.len() >= 2, "{lines:?}");
        assert!(lines.iter().all(|l| !l.is_empty()), "{lines:?}");
        assert_eq!(lines.concat().replace(' ', ""), "Importantinformation!Youcanwritenotes.");
        assert_eq!(lines, vec!["Important information!", "You can write notes."]);
    }

    #[test]
    fn markdown_strings_join_source_lines() {
        let src = "flowchart LR\n  a(\"`The **cat**\n  in the hat`\") --> b[\"x\"]\n  c\n";
        assert_eq!(join_markdown_strings(src), "flowchart LR\n  a(\"`The **cat**<br>in the hat`\") --> b[\"x\"]\n  c\n");
        assert!(matches!(join_markdown_strings("graph\n a --> b\n"), std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn acc_title_and_block_description() {
        let (t, d) = parse_acc_meta("stateDiagram\n  accTitle: This is the accessible title\n  accDescr: This is an accessible description\n  [*] --> Still\n");
        assert_eq!(t.as_deref(), Some("This is the accessible title"));
        assert_eq!(d.as_deref(), Some("This is an accessible description"));
        let (t, d) = parse_acc_meta("usecase-beta\naccTitle: Online ordering use cases\naccDescr {\n  A customer places an order through the storefront.\n  Staff review the order and inspect its data.\n}\nactor Customer\n");
        assert_eq!(t.as_deref(), Some("Online ordering use cases"));
        assert_eq!(d.as_deref(), Some("A customer places an order through the storefront.\nStaff review the order and inspect its data."));
    }
}
