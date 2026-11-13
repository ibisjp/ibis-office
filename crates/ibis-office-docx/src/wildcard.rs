// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Word ワイルドカード検索・置換（Word Wildcard Search & Replace: DOCX-FIND-005, DOCX-FIND-006）
//!
//! Word の「ワイルドカードを使用する」（Use Wildcards）構文と置換時のグループ参照を処理する。
//!
//! # 構文
//! - `?`: 任意の 1 文字
//! - `*`: 0 文字以上の任意の文字列
//! - `<`: 単語の先頭境界
//! - `>`: 単語の末尾境界
//! - `[abc]`: 指定文字のいずれか 1 文字
//! - `[a-z]`: 文字範囲のいずれか 1 文字
//! - `[!abc]`: 指定文字以外の任意の 1 文字
//! - `@`: 直前の文字または式の 1 回以上の繰り返し
//! - `{n}`: 直前の文字または式の n 回の繰り返し
//! - `{n,}`: 直前の文字または式の n 回以上の繰り返し
//! - `{n,m}`: 直前の文字または式の n 回以上 m 回以下の繰り返し
//! - `(...)`: グループ化（置換時の `\1`, `\2` で参照）
//! - `\c`: ワイルドカード文字のエスケープ
//!
//! # 置換
//! - `\1`, `\2`, ...: キャプチャグループの参照
//! - `\\`: バックスラッシュ自身

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WildcardNode {
    Literal(char),
    AnyChar,
    AnyString,
    WordStart,
    WordEnd,
    CharSet {
        ranges: Vec<(char, char)>,
        chars: Vec<char>,
        negated: bool,
    },
    Repeat {
        node: Box<WildcardNode>,
        min: usize,
        max: Option<usize>,
    },
    Group {
        index: usize,
        nodes: Vec<WildcardNode>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WildcardPattern {
    pub(crate) nodes: Vec<WildcardNode>,
    pub(crate) group_count: usize,
}

/// 単語区切り文字の判定（ASCII/Unicode 記号・空白・約物）
pub(crate) fn is_word_separator(c: char) -> bool {
    c.is_whitespace()
        || (c.is_ascii_punctuation() && c != '_')
        || c.is_control()
        || matches!(
            c,
            '\u{2000}'..='\u{206f}' // General Punctuation
            | '\u{3000}'..='\u{303f}' // CJK Symbols and Punctuation (「」『』【】、。・など)
            | '\u{ff01}'..='\u{ff0f}' // 全角記号
            | '\u{ff1a}'..='\u{ff20}'
            | '\u{ff3b}'..='\u{ff40}'
            | '\u{ff5b}'..='\u{ff65}'
        )
}

/// Word ワイルドカード文字列を解析して AST を構築する
pub(crate) fn parse_wildcard(pattern: &str) -> Result<WildcardPattern, String> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    let mut group_count = 0;
    let nodes = parse_nodes(&chars, &mut i, &mut group_count, false)?;
    Ok(WildcardPattern { nodes, group_count })
}

fn parse_nodes(
    chars: &[char],
    i: &mut usize,
    group_count: &mut usize,
    in_group: bool,
) -> Result<Vec<WildcardNode>, String> {
    let mut nodes: Vec<WildcardNode> = Vec::new();

    while *i < chars.len() {
        let c = chars[*i];
        match c {
            ')' if in_group => {
                *i += 1;
                return Ok(nodes);
            }
            '(' => {
                *i += 1;
                *group_count += 1;
                let group_idx = *group_count;
                let group_nodes = parse_nodes(chars, i, group_count, true)?;
                nodes.push(WildcardNode::Group {
                    index: group_idx,
                    nodes: group_nodes,
                });
            }
            '?' => {
                *i += 1;
                nodes.push(WildcardNode::AnyChar);
            }
            '*' => {
                *i += 1;
                nodes.push(WildcardNode::AnyString);
            }
            '<' => {
                *i += 1;
                nodes.push(WildcardNode::WordStart);
            }
            '>' => {
                *i += 1;
                nodes.push(WildcardNode::WordEnd);
            }
            '[' => {
                *i += 1;
                let mut negated = false;
                if *i < chars.len() && chars[*i] == '!' {
                    negated = true;
                    *i += 1;
                }
                let mut set_chars = Vec::new();
                let mut ranges = Vec::new();
                while *i < chars.len() && chars[*i] != ']' {
                    let ch = chars[*i];
                    if *i + 2 < chars.len() && chars[*i + 1] == '-' && chars[*i + 2] != ']' {
                        ranges.push((ch, chars[*i + 2]));
                        *i += 3;
                    } else {
                        set_chars.push(ch);
                        *i += 1;
                    }
                }
                if *i < chars.len() && chars[*i] == ']' {
                    *i += 1;
                }
                nodes.push(WildcardNode::CharSet {
                    ranges,
                    chars: set_chars,
                    negated,
                });
            }
            '@' => {
                *i += 1;
                if let Some(prev) = nodes.pop() {
                    nodes.push(WildcardNode::Repeat {
                        node: Box::new(prev),
                        min: 1,
                        max: None,
                    });
                } else {
                    nodes.push(WildcardNode::Literal('@'));
                }
            }
            '{' => {
                let start = *i;
                *i += 1;
                let mut num_str = String::new();
                while *i < chars.len() && chars[*i] != '}' {
                    num_str.push(chars[*i]);
                    *i += 1;
                }
                if *i < chars.len() && chars[*i] == '}' {
                    *i += 1;
                    if let Some((min, max)) = parse_range(&num_str) {
                        if let Some(prev) = nodes.pop() {
                            nodes.push(WildcardNode::Repeat {
                                node: Box::new(prev),
                                min,
                                max,
                            });
                            continue;
                        }
                    }
                }
                // パース失敗時は元の文字として扱う
                *i = start + 1;
                nodes.push(WildcardNode::Literal('{'));
            }
            '\\' => {
                *i += 1;
                if *i < chars.len() {
                    let escaped = chars[*i];
                    *i += 1;
                    nodes.push(WildcardNode::Literal(escaped));
                } else {
                    nodes.push(WildcardNode::Literal('\\'));
                }
            }
            _ => {
                *i += 1;
                nodes.push(WildcardNode::Literal(c));
            }
        }
    }

    Ok(nodes)
}

fn parse_range(s: &str) -> Option<(usize, Option<usize>)> {
    if let Some((a, b)) = s.split_once(',') {
        let min = a.trim().parse::<usize>().ok()?;
        let b_trimmed = b.trim();
        let max = if b_trimmed.is_empty() {
            None
        } else {
            Some(b_trimmed.parse::<usize>().ok()?)
        };
        Some((min, max))
    } else {
        let n = s.trim().parse::<usize>().ok()?;
        Some((n, Some(n)))
    }
}

/// 単一のマッチ結果情報
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WildcardMatch {
    /// マッチした開始インデックス（文字単位）
    pub(crate) start: usize,
    /// マッチした終了インデックス（文字単位、exclusive）
    pub(crate) end: usize,
    /// 各キャプチャグループの (start, end) 文字インデックス（1-indexed、0番目はダミー）
    pub(crate) captures: Vec<Option<(usize, usize)>>,
}

/// 指定テキスト内のすべてのマッチ（重複しない最長一致）を検索する
pub(crate) fn find_all_matches(
    text_chars: &[char],
    pattern: &WildcardPattern,
    case_sensitive: bool,
) -> Vec<WildcardMatch> {
    let mut matches = Vec::new();
    let mut i = 0;
    while i <= text_chars.len() {
        let mut initial_captures = vec![None; pattern.group_count + 1];
        if let Some((matched_len, mut captures)) = match_nodes(
            &pattern.nodes,
            text_chars,
            i,
            &mut initial_captures,
            case_sensitive,
        ) {
            let match_end = i + matched_len;
            if !captures.is_empty() {
                captures[0] = Some((i, match_end));
            }
            matches.push(WildcardMatch {
                start: i,
                end: match_end,
                captures,
            });
            // 空マッチの無限ループを防ぐため最低1文字進める
            i = match_end.max(i + 1);
        } else {
            i += 1;
        }
    }
    matches
}

fn match_nodes(
    nodes: &[WildcardNode],
    text: &[char],
    pos: usize,
    captures: &mut [Option<(usize, usize)>],
    case_sensitive: bool,
) -> Option<(usize, Vec<Option<(usize, usize)>>)> {
    if nodes.is_empty() {
        return Some((0, captures.to_vec()));
    }

    let head = &nodes[0];
    let tail = &nodes[1..];

    match head {
        WildcardNode::Literal(expected) => {
            if pos < text.len() && chars_match(text[pos], *expected, case_sensitive) {
                let (rest_len, cap) = match_nodes(tail, text, pos + 1, captures, case_sensitive)?;
                Some((1 + rest_len, cap))
            } else {
                None
            }
        }
        WildcardNode::AnyChar => {
            if pos < text.len() {
                let (rest_len, cap) = match_nodes(tail, text, pos + 1, captures, case_sensitive)?;
                Some((1 + rest_len, cap))
            } else {
                None
            }
        }
        WildcardNode::WordStart => {
            let is_start = (pos == 0 || is_word_separator(text[pos - 1]))
                && (pos < text.len() && !is_word_separator(text[pos]));
            if is_start {
                match_nodes(tail, text, pos, captures, case_sensitive)
            } else {
                None
            }
        }
        WildcardNode::WordEnd => {
            let is_end = (pos > 0 && !is_word_separator(text[pos - 1]))
                && (pos == text.len() || is_word_separator(text[pos]));
            if is_end {
                match_nodes(tail, text, pos, captures, case_sensitive)
            } else {
                None
            }
        }
        WildcardNode::CharSet { ranges, chars, negated } => {
            if pos < text.len() {
                let ch = text[pos];
                let in_set = chars.iter().any(|c| chars_match(ch, *c, case_sensitive))
                    || ranges.iter().any(|(r1, r2)| range_match(ch, *r1, *r2, case_sensitive));
                let matched = if *negated { !in_set } else { in_set };
                if matched {
                    let (rest_len, cap) = match_nodes(tail, text, pos + 1, captures, case_sensitive)?;
                    Some((1 + rest_len, cap))
                } else {
                    None
                }
            } else {
                None
            }
        }
        WildcardNode::Repeat { node, min, max } => {
            // 直前のノードを最小 min 回、最大 max 回マッチ（最長一致）
            let mut repeat_lens = Vec::new();
            let mut cur_pos = pos;
            let max_limit = max.unwrap_or(text.len() - pos);
            let mut count = 0;

            while count < max_limit {
                if let Some((step_len, _)) = match_nodes(std::slice::from_ref(&**node), text, cur_pos, &mut captures.to_vec(), case_sensitive) {
                    if step_len == 0 {
                        break;
                    }
                    cur_pos += step_len;
                    count += 1;
                    if count >= *min {
                        repeat_lens.push((count, cur_pos - pos));
                    }
                } else {
                    break;
                }
            }
            if *min == 0 {
                repeat_lens.insert(0, (0, 0));
            }

            // 最長一致から順に試す
            for (_, total_len) in repeat_lens.into_iter().rev() {
                if let Some((rest_len, cap)) = match_nodes(tail, text, pos + total_len, captures, case_sensitive) {
                    return Some((total_len + rest_len, cap));
                }
            }
            None
        }
        WildcardNode::Group { index, nodes: inner_nodes } => {
            let mut inner_caps = captures.to_vec();
            if let Some((inner_len, mut res_caps)) = match_nodes(inner_nodes, text, pos, &mut inner_caps, case_sensitive) {
                if *index < res_caps.len() {
                    res_caps[*index] = Some((pos, pos + inner_len));
                }
                let (rest_len, final_caps) = match_nodes(tail, text, pos + inner_len, &mut res_caps, case_sensitive)?;
                Some((inner_len + rest_len, final_caps))
            } else {
                None
            }
        }
        WildcardNode::AnyString => {
            // `*`: 0文字からテキスト末尾まで（最短〜最長一致をバックトラッキング）
            // Word では通常最短または貪欲にマッチする
            for len in 0..=(text.len() - pos) {
                if let Some((rest_len, cap)) = match_nodes(tail, text, pos + len, captures, case_sensitive) {
                    return Some((len + rest_len, cap));
                }
            }
            None
        }
    }
}

fn chars_match(c1: char, c2: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        c1 == c2
    } else {
        c1.to_lowercase().eq(c2.to_lowercase())
    }
}

fn range_match(c: char, r1: char, r2: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        c >= r1 && c <= r2
    } else {
        let cl = c.to_lowercase().next().unwrap_or(c);
        let r1l = r1.to_lowercase().next().unwrap_or(r1);
        let r2l = r2.to_lowercase().next().unwrap_or(r2);
        cl >= r1l && cl <= r2l
    }
}

/// 置換文字列内の `\1`, `\2` などのグループ参照、および `\0` や `^&`（マッチ全体）を展開する
pub(crate) fn expand_wildcard_replacement(
    replacement: &str,
    text_chars: &[char],
    captures: &[Option<(usize, usize)>],
) -> String {
    let mut out = String::new();
    let mut chars = replacement.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next.is_ascii_digit() {
                    chars.next();
                    let group_idx = (next as usize) - ('0' as usize);
                    if let Some(Some((start, end))) = captures.get(group_idx) {
                        if *start <= *end && *end <= text_chars.len() {
                            out.extend(&text_chars[*start..*end]);
                        }
                    }
                    continue;
                } else if next == '\\' {
                    chars.next();
                    out.push('\\');
                    continue;
                }
            }
            out.push('\\');
        } else if c == '^' {
            if let Some(&next) = chars.peek() {
                if next == '&' {
                    chars.next();
                    if let Some(Some((start, end))) = captures.get(0) {
                        if *start <= *end && *end <= text_chars.len() {
                            out.extend(&text_chars[*start..*end]);
                        }
                    }
                    continue;
                }
            }
            out.push('^');
        } else {
            out.push(c);
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard_basic_matching() {
        // ?
        let pat = parse_wildcard("b?t").unwrap();
        let text: Vec<char> = "The bat and bit and but sat".chars().collect();
        let matches = find_all_matches(&text, &pat, false);
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].start, 4);
        assert_eq!(matches[0].end, 7);

        // *
        let pat2 = parse_wildcard("b*t").unwrap();
        let text2: Vec<char> = "bt and boot and basket".chars().collect();
        let matches2 = find_all_matches(&text2, &pat2, false);
        assert_eq!(matches2.len(), 3);

        // WordStart < and WordEnd >
        let pat3 = parse_wildcard("<in").unwrap();
        let text3: Vec<char> = "inside the winter".chars().collect();
        let matches3 = find_all_matches(&text3, &pat3, false);
        assert_eq!(matches3.len(), 1);
        assert_eq!(matches3[0].start, 0);

        let pat4 = parse_wildcard("in>").unwrap();
        let text4: Vec<char> = "inside within".chars().collect();
        let matches4 = find_all_matches(&text4, &pat4, false);
        assert_eq!(matches4.len(), 1);
        assert_eq!(matches4[0].start, 11);
    }

    #[test]
    fn test_wildcard_charset_and_repeat() {
        // [ae]
        let pat = parse_wildcard("b[ae]t").unwrap();
        let text: Vec<char> = "bat bet bit".chars().collect();
        let matches = find_all_matches(&text, &pat, false);
        assert_eq!(matches.len(), 2);

        // [!ae]
        let pat2 = parse_wildcard("b[!ae]t").unwrap();
        let matches2 = find_all_matches(&text, &pat2, false);
        assert_eq!(matches2.len(), 1);
        assert_eq!(matches2[0].start, 8); // "bit"

        // @ (1回以上)
        let pat3 = parse_wildcard("ro@t").unwrap();
        let text3: Vec<char> = "rt rot root rooot".chars().collect();
        let matches3 = find_all_matches(&text3, &pat3, false);
        assert_eq!(matches3.len(), 3);

        // {2}
        let pat4 = parse_wildcard("ro{2}t").unwrap();
        let matches4 = find_all_matches(&text3, &pat4, false);
        assert_eq!(matches4.len(), 1);
        assert_eq!(&text3[matches4[0].start..matches4[0].end], &['r', 'o', 'o', 't']);
    }

    #[test]
    fn test_wildcard_group_capture_and_replacement() {
        let pat = parse_wildcard("(John) (Doe)").unwrap();
        let text: Vec<char> = "Hello John Doe!".chars().collect();
        let matches = find_all_matches(&text, &pat, false);
        assert_eq!(matches.len(), 1);

        let m = &matches[0];
        let rep = expand_wildcard_replacement(r"\2 \1", &text, &m.captures);
        assert_eq!(rep, "Doe John");

        // 複数グループ置換
        let pat2 = parse_wildcard("([0-9]{4})-([0-9]{2})-([0-9]{2})").unwrap();
        let text2: Vec<char> = "Date: 2026-10-09".chars().collect();
        let matches2 = find_all_matches(&text2, &pat2, false);
        assert_eq!(matches2.len(), 1);
        let rep2 = expand_wildcard_replacement(r"\1/\2/\3", &text2, &matches2[0].captures);
        assert_eq!(rep2, "2026/10/09");

        // \0 および ^& による全体一致参照
        let rep_zero = expand_wildcard_replacement(r"[\0]", &text2, &matches2[0].captures);
        assert_eq!(rep_zero, "[2026-10-09]");
        let rep_amp = expand_wildcard_replacement(r"[^&]", &text2, &matches2[0].captures);
        assert_eq!(rep_amp, "[2026-10-09]");
    }
}
