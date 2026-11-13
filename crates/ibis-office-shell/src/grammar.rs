// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 文法チェック・文章校正 (Grammar and Style Checking):
//! Word 標準の文章校正ルール（重複語、句読点の重複、句読点後の空白欠落、
//! 連続する空白、文頭の大文字化など）をルールベースで検出し、
//! 適切な修正候補（Suggestions）を提供する。
//! OS やプラットフォーム（macOS, Linux, Windows, Web）に依存せず確実に動作する。

/// 文法チェックのルール種別
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GrammarRule {
    /// 単語の重複 (e.g., "the the", "that that", "のの")
    RepeatedWord,
    /// 句読点の重複 (e.g., "。。", ",,", "..")
    RepeatedPunctuation,
    /// 句読点後の空白欠落 (e.g., "hello,world" -> "hello, world")
    MissingSpaceAfterPunctuation,
    /// 連続する空白 (e.g., "  " -> " ")
    MultipleSpaces,
    /// 文頭の小文字 (e.g., ". this" -> ". This")
    Capitalization,
}

impl GrammarRule {
    #[allow(dead_code)]
    pub(crate) fn label(&self) -> gpui_kit::SharedString {
        crate::i18n::text(match self {
            Self::RepeatedWord => "grammar.repeated-word",
            Self::RepeatedPunctuation => "grammar.repeated-punctuation",
            Self::MissingSpaceAfterPunctuation => "grammar.missing-space",
            Self::MultipleSpaces => "grammar.multiple-spaces",
            Self::Capitalization => "grammar.capitalization",
        })
    }
}

/// 検出された文法上の問題
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GrammarIssue {
    /// 文字オフセット開始（0-indexed char count）
    pub start: usize,
    /// 文字オフセット終了（0-indexed char count、exclusive）
    pub end: usize,
    /// 該当箇所の文字列
    pub text: String,
    /// 適用されたルール
    pub(crate) rule: GrammarRule,
    /// 修正候補
    pub suggestions: Vec<String>,
}

/// テキスト全体の文法チェックを実行し、検出された問題のリストを昇順（start位置順）で返す。
pub fn check_grammar(text: &str) -> Vec<GrammarIssue> {
    let mut issues = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if len == 0 {
        return issues;
    }

    check_repeated_words(&chars, &mut issues);
    check_repeated_punctuation(&chars, &mut issues);
    check_missing_space_after_punctuation(&chars, &mut issues);
    check_multiple_spaces(&chars, &mut issues);
    check_capitalization(&chars, &mut issues);

    // 開始位置順にソート
    issues.sort_by_key(|i| i.start);
    issues
}

/// 1. 重複語の検出 (Repeated Words)
fn check_repeated_words(chars: &[char], issues: &mut Vec<GrammarIssue>) {
    let len = chars.len();
    // 欧文単語のトークン分割 (start, end, word_lower)
    let mut words = Vec::new();
    let mut i = 0;
    while i < len {
        if chars[i].is_alphabetic() {
            let start = i;
            while i < len && chars[i].is_alphabetic() {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            words.push((start, i, word));
        } else {
            i += 1;
        }
    }

    for w in 0..words.len().saturating_sub(1) {
        let (s1, e1, ref word1) = words[w];
        let (s2, e2, ref word2) = words[w + 1];

        // 2つの単語の間に空白文字のみが存在するか確認（改行やカンマ・ピリオドが挟まれていない）
        let between = &chars[e1..s2];
        if !between.is_empty() && between.iter().all(|c| *c == ' ' || *c == '\t') {
            if word1.eq_ignore_ascii_case(word2) {
                let match_text: String = chars[s1..e2].iter().collect();
                // 修正候補: 1語のみを残す（先頭単語の大文字小文字を維持）
                issues.push(GrammarIssue {
                    start: s1,
                    end: e2,
                    text: match_text,
                    rule: GrammarRule::RepeatedWord,
                    suggestions: vec![word1.clone()],
                });
            }
        }
    }

    // 和文助詞の誤重複 ("のの", "にに", "はは", "をを", "がが", "でで")
    let japanese_particles = ['の', 'に', 'は', 'を', 'が', 'で', 'と', 'へ'];
    let mut j = 0;
    while j + 1 < len {
        let c1 = chars[j];
        let c2 = chars[j + 1];
        if c1 == c2 && japanese_particles.contains(&c1) {
            // 前後が同一文字でないこと、および直前が漢字/ひらがな等であること
            let prev_same = j > 0 && chars[j - 1] == c1;
            let next_same = j + 2 < len && chars[j + 2] == c1;
            if !prev_same && !next_same {
                // 特殊な表現（「へとへと」「にこにこ」「はは（母）」など）を簡易ガード
                // 1文字助詞が連続している場合
                let match_text: String = chars[j..j + 2].iter().collect();
                issues.push(GrammarIssue {
                    start: j,
                    end: j + 2,
                    text: match_text,
                    rule: GrammarRule::RepeatedWord,
                    suggestions: vec![c1.to_string()],
                });
                j += 2;
                continue;
            }
        }
        j += 1;
    }
}

/// 2. 句読点の重複 (Repeated Punctuation)
fn check_repeated_punctuation(chars: &[char], issues: &mut Vec<GrammarIssue>) {
    let len = chars.len();
    let mut i = 0;
    while i < len {
        let c = chars[i];
        if c == '。' || c == '、' || c == ',' {
            let start = i;
            while i < len && chars[i] == c {
                i += 1;
            }
            if i - start >= 2 {
                let match_text: String = chars[start..i].iter().collect();
                issues.push(GrammarIssue {
                    start,
                    end: i,
                    text: match_text,
                    rule: GrammarRule::RepeatedPunctuation,
                    suggestions: vec![c.to_string()],
                });
                continue;
            }
        } else if c == '.' {
            let start = i;
            while i < len && chars[i] == '.' {
                i += 1;
            }
            let count = i - start;
            // 2つのドット ".." は誤り。3点リーダー "..." は正常。4つ以上は誤り
            if count == 2 || count > 3 {
                let match_text: String = chars[start..i].iter().collect();
                let suggestions = if count == 2 {
                    vec![".".to_string(), "...".to_string()]
                } else {
                    vec!["...".to_string(), ".".to_string()]
                };
                issues.push(GrammarIssue {
                    start,
                    end: i,
                    text: match_text,
                    rule: GrammarRule::RepeatedPunctuation,
                    suggestions,
                });
                continue;
            }
        } else if c == '!' || c == '?' || c == '！' || c == '？' {
            let start = i;
            while i < len && (chars[i] == '!' || chars[i] == '?' || chars[i] == '！' || chars[i] == '？') {
                i += 1;
            }
            if i - start >= 2 {
                let match_text: String = chars[start..i].iter().collect();
                // 最初の記号を代表提案
                issues.push(GrammarIssue {
                    start,
                    end: i,
                    text: match_text,
                    rule: GrammarRule::RepeatedPunctuation,
                    suggestions: vec![c.to_string()],
                });
                continue;
            }
        }
        i += 1;
    }
}

/// 3. 句読点後の空白欠落 (Missing Space After Punctuation)
fn check_missing_space_after_punctuation(chars: &[char], issues: &mut Vec<GrammarIssue>) {
    let len = chars.len();
    for i in 0..len.saturating_sub(1) {
        let c = chars[i];
        let next = chars[i + 1];

        // カンマまたはセミコロンの直後にアルファベットが続く
        if (c == ',' || c == ';') && next.is_ascii_alphabetic() {
            // URL や数値（1,000）の除外
            let is_in_word_or_num = i > 0 && (chars[i - 1].is_ascii_digit() || chars[i - 1] == '/');
            if !is_in_word_or_num {
                let match_text: String = chars[i..i + 2].iter().collect();
                let suggestion = format!("{c} {next}");
                issues.push(GrammarIssue {
                    start: i,
                    end: i + 2,
                    text: match_text,
                    rule: GrammarRule::MissingSpaceAfterPunctuation,
                    suggestions: vec![suggestion],
                });
            }
        }
    }
}

/// 4. 連続する空白 (Multiple Spaces)
fn check_multiple_spaces(chars: &[char], issues: &mut Vec<GrammarIssue>) {
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if chars[i] == ' ' || chars[i] == '　' {
            let space_char = chars[i];
            let start = i;
            while i < len && chars[i] == space_char {
                i += 1;
            }
            if i - start >= 2 {
                let match_text: String = chars[start..i].iter().collect();
                issues.push(GrammarIssue {
                    start,
                    end: i,
                    text: match_text,
                    rule: GrammarRule::MultipleSpaces,
                    suggestions: vec![space_char.to_string()],
                });
                continue;
            }
        }
        i += 1;
    }
}

/// 5. 文頭の大文字化 (Capitalization)
fn check_capitalization(chars: &[char], issues: &mut Vec<GrammarIssue>) {
    let len = chars.len();
    if len == 0 {
        return;
    }

    // テキスト全体の先頭が小文字英字の場合
    let mut first_letter_idx = 0;
    while first_letter_idx < len && (chars[first_letter_idx] == ' ' || chars[first_letter_idx] == '\t' || chars[first_letter_idx] == '"' || chars[first_letter_idx] == '\'') {
        first_letter_idx += 1;
    }
    if first_letter_idx < len && chars[first_letter_idx].is_ascii_lowercase() {
        let c = chars[first_letter_idx];
        issues.push(GrammarIssue {
            start: first_letter_idx,
            end: first_letter_idx + 1,
            text: c.to_string(),
            rule: GrammarRule::Capitalization,
            suggestions: vec![c.to_ascii_uppercase().to_string()],
        });
    }

    // ピリオド・感嘆符・疑問符の後に空白が続いて小文字英字がある場合
    for i in 0..len.saturating_sub(2) {
        let punct = chars[i];
        if punct == '.' || punct == '!' || punct == '?' {
            // 空白をスキップ
            let mut j = i + 1;
            while j < len && (chars[j] == ' ' || chars[j] == '\t' || chars[j] == '"' || chars[j] == '\'') {
                j += 1;
            }
            if j > i + 1 && j < len && chars[j].is_ascii_lowercase() {
                // e.g. "etc." や "i.e." などの略語の直後でないか確認
                let is_abbreviation = i >= 3 && {
                    let prev_word: String = chars[i - 3..i].iter().collect();
                    prev_word.eq_ignore_ascii_case("etc") || prev_word.eq_ignore_ascii_case("e.g") || prev_word.eq_ignore_ascii_case("i.e")
                };
                if !is_abbreviation {
                    let c = chars[j];
                    issues.push(GrammarIssue {
                        start: j,
                        end: j + 1,
                        text: c.to_string(),
                        rule: GrammarRule::Capitalization,
                        suggestions: vec![c.to_ascii_uppercase().to_string()],
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repeated_words_english_and_japanese() {
        let text = "We think that that is the the best result. これのの確認をする。";
        let issues = check_grammar(text);

        let repeated_words: Vec<_> = issues.iter().filter(|i| i.rule == GrammarRule::RepeatedWord).collect();
        assert_eq!(repeated_words.len(), 3);
        assert_eq!(repeated_words[0].text, "that that");
        assert_eq!(repeated_words[0].suggestions, vec!["that"]);
        assert_eq!(repeated_words[1].text, "the the");
        assert_eq!(repeated_words[1].suggestions, vec!["the"]);
        assert_eq!(repeated_words[2].text, "のの");
        assert_eq!(repeated_words[2].suggestions, vec!["の"]);
    }

    #[test]
    fn test_repeated_punctuation() {
        let text = "確認完了。。本当ですか？？ yes,,sure..";
        let issues = check_grammar(text);

        let punc_issues: Vec<_> = issues.iter().filter(|i| i.rule == GrammarRule::RepeatedPunctuation).collect();
        assert_eq!(punc_issues.len(), 4);
        assert_eq!(punc_issues[0].text, "。。");
        assert_eq!(punc_issues[0].suggestions, vec!["。"]);
        assert_eq!(punc_issues[1].text, "？？");
        assert_eq!(punc_issues[1].suggestions, vec!["？"]);
        assert_eq!(punc_issues[2].text, ",,");
        assert_eq!(punc_issues[2].suggestions, vec![","]);
        assert_eq!(punc_issues[3].text, "..");
        assert_eq!(punc_issues[3].suggestions, vec![".", "..."]);
    }

    #[test]
    fn test_missing_space_after_punctuation() {
        let text = "apple,banana and one;two";
        let issues = check_grammar(text);

        let space_issues: Vec<_> = issues.iter().filter(|i| i.rule == GrammarRule::MissingSpaceAfterPunctuation).collect();
        assert_eq!(space_issues.len(), 2);
        assert_eq!(space_issues[0].text, ",b");
        assert_eq!(space_issues[0].suggestions, vec![", b"]);
        assert_eq!(space_issues[1].text, ";t");
        assert_eq!(space_issues[1].suggestions, vec!["; t"]);
    }

    #[test]
    fn test_multiple_spaces() {
        let text = "Hello  world　　test";
        let issues = check_grammar(text);

        let multi_space: Vec<_> = issues.iter().filter(|i| i.rule == GrammarRule::MultipleSpaces).collect();
        assert_eq!(multi_space.len(), 2);
        assert_eq!(multi_space[0].text, "  ");
        assert_eq!(multi_space[0].suggestions, vec![" "]);
        assert_eq!(multi_space[1].text, "　　");
        assert_eq!(multi_space[1].suggestions, vec!["　"]);
    }

    #[test]
    fn test_capitalization() {
        let text = "first sentence. second sentence! third sentence?";
        let issues = check_grammar(text);

        let cap_issues: Vec<_> = issues.iter().filter(|i| i.rule == GrammarRule::Capitalization).collect();
        assert_eq!(cap_issues.len(), 3);
        assert_eq!(cap_issues[0].text, "f");
        assert_eq!(cap_issues[0].suggestions, vec!["F"]);
        assert_eq!(cap_issues[1].text, "s");
        assert_eq!(cap_issues[1].suggestions, vec!["S"]);
        assert_eq!(cap_issues[2].text, "t");
        assert_eq!(cap_issues[2].suggestions, vec!["T"]);
    }
}
