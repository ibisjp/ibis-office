// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! GitHub issue 295: the document screen's 文字カウント gives Word's numbers (design: `tasks/doc/test-design/issue-295.md`; the numbers
//! C1... are that document's). The documents are `test/editing/wc295/*.docx` (made by `scripts/test-data/make-wc295-docs.py`); Word's numbers
//! for them are `test/editing/wc295/word-counts.tsv` (Word on Windows, `Range.ComputeStatistics`, request
//! `tasks/doc/requests/2026-11-25-295-word-count.md`). Until that file has a row for a case its test fails by saying so: a Windows
//! measurement is part of the requirement, and nothing here is "done" before it.
use super::Scenario;
use ibis_office_docx::ext::*;

/// C1 and C3 pass (C3 again since the -1 of c38 is read as Word's own value, 3c8f44977).
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue295_c1_the_issues_own_document_gives_words_numbers", c1_issue_document),
        ("shell::issue295_c3_without_notes_gives_words_numbers", c3_without_notes),
    ]
}

/// C2 stays here while five documents differ from Word (c05, c14, c15, c16, c17).
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
        ("shell::issue295_c2_every_case_gives_words_numbers", c2_every_case),
    ]
}

/// The eight numbers of Word's 文字カウント, in the dialog's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Counts {
    pages: i64,
    words: i64,
    chars_no_spaces: i64,
    chars_with_spaces: i64,
    paragraphs: i64,
    lines: i64,
    half_width_words: i64,
    full_width_chars: i64,
}

const NAMES: [&str; 8] = ["ページ数", "単語数", "文字数(スペースを含めない)", "文字数(スペースを含める)", "段落数", "行数", "半角英数の単語数", "全角文字+半角カタカナの数"];

impl Counts {
    fn array(self) -> [i64; 8] {
        [self.pages, self.words, self.chars_no_spaces, self.chars_with_spaces, self.paragraphs, self.lines, self.half_width_words, self.full_width_chars]
    }
}

fn ours(path: &str) -> Counts {
    ours_with(path, true)
}

fn ours_with(path: &str, include_notes: bool) -> Counts {
    let mut s = Scenario::open(path);
    s.size(1200., 900.);
    let st = s.with(|e, _| e.document_statistics_with(include_notes));
    Counts {
        pages: st.pages as i64,
        words: st.words as i64,
        chars_no_spaces: st.characters_no_spaces as i64,
        chars_with_spaces: st.characters_with_spaces,
        paragraphs: st.paragraphs as i64,
        lines: st.lines as i64,
        half_width_words: st.half_width_words as i64,
        full_width_chars: st.full_width_chars as i64,
    }
}

fn differences(label: &str, word: Counts, ours: Counts) -> Option<String> {
    let (w, o) = (word.array(), ours.array());
    let d: Vec<String> = (0..8).filter(|i| w[*i] != o[*i]).map(|i| format!("{} Word {} ours {}", NAMES[i], w[i], o[i])).collect();
    (!d.is_empty()).then(|| format!("{label}: {}", d.join(", ")))
}

/// C1: the document of the issue's screenshot (Word, 文字カウント with the box checked: 2, 3, 12, 14, 2, 6, 3, 0).
fn c1_issue_document() {
    let word = Counts { pages: 2, words: 3, chars_no_spaces: 12, chars_with_spaces: 14, paragraphs: 2, lines: 6, half_width_words: 3, full_width_chars: 0 };
    let got = ours("test/editing/wc295/c06-d0-issue.docx");
    if let Some(d) = differences("c06-d0-issue", word, got) {
        panic!("{d}");
    }
}

/// Word's rows: `id`, `include_notes` (1: the box checked), then the eight numbers.
fn word_rows() -> Vec<(String, Counts)> {
    word_rows_with("1")
}

fn word_rows_with(include_notes: &str) -> Vec<(String, Counts)> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/wc295/word-counts.tsv");
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.starts_with("id\t") && !l.is_empty())
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            (f.len() >= 10 && f[1] == include_notes).then(|| {
                let n: Vec<i64> = f[2..10].iter().map(|x| x.parse().unwrap()).collect();
                (f[0].to_string(), Counts { pages: n[0], words: n[1], chars_no_spaces: n[2], chars_with_spaces: n[3], paragraphs: n[4], lines: n[5], half_width_words: n[6], full_width_chars: n[7] })
            })
        })
        .collect()
}

/// C2: every document of `cases.tsv`, the eight numbers against Word's (the box checked). All differences are reported.
fn c2_every_case() {
    let cases = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/editing/wc295/cases.tsv")).unwrap();
    let ids: Vec<&str> = cases.lines().skip(1).filter_map(|l| l.split('\t').next()).collect();
    let word = word_rows();
    let missing: Vec<&&str> = ids.iter().filter(|i| !word.iter().any(|(w, _)| w == **i)).collect();
    assert!(missing.is_empty(), "no Word measurement yet for {} of {} cases (test/editing/wc295/word-counts.tsv, request 2026-11-25-295-word-count.md): {missing:?}", missing.len(), ids.len());
    let diffs: Vec<String> = word.iter().filter_map(|(id, w)| differences(id, *w, ours(&format!("test/editing/wc295/{id}.docx")))).collect();
    assert!(diffs.is_empty(), "{} of {} cases differ from Word:\n{}", diffs.len(), word.len(), diffs.join("\n"));
}

/// C3: the box 「テキスト ボックス、脚注、文末脚注を含める」 not checked: the documents with a footnote or a text box (c21, c22, c23, c31) against Word's numbers.
fn c3_without_notes() {
    let word = word_rows_with("0");
    let diffs: Vec<String> = ["c21-footnote", "c22-textbox", "c23-textbox-break", "c31-d0-footnote"]
        .iter()
        .filter_map(|id| {
            let w = word.iter().find(|(i, _)| i == id).unwrap_or_else(|| panic!("no Word measurement for {id} with the box not checked")).1;
            differences(id, w, ours_with(&format!("test/editing/wc295/{id}.docx"), false))
        })
        .collect();
    assert!(diffs.is_empty(), "{} cases differ from Word:\n{}", diffs.len(), diffs.join("\n"));
}
