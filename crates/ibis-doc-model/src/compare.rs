// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 校閲 > 比較: the changes between two versions of a document, as tracked changes (what Word's 「比較」 makes).
//!
//! The paragraphs of the two versions are lined up by their text; a paragraph only in the original is shown deleted and one
//! only in the revision inserted; two that stand at the same place and look alike are compared by words (a run of letters
//! or digits, or one other character), the words only in the original deleted and those only in the revision inserted.
//! Tables of the same shape (rows and cells) at the same place are compared cell by cell; a table only in the revision is
//! inserted, one only in the original deleted (all of its text marked). Words that stand in both but are formatted differently
//! get a formatting change (what the original had is kept).

use crate::flow::{Block, Comment, FlowDocument, FlowParagraph, FlowRun, FlowTable, Note, Revision, RevisionKind};

/// What a comparison looks at (校閲 > 比較の設定, Word's 「比較の設定」): all on unless turned off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompareOptions {
    /// Words formatted differently get a formatting change.
    pub format: bool,
    /// A letter that changes only its case counts as a change.
    pub case: bool,
    /// The cells of tables are compared (otherwise a table stands as it is in the revision).
    pub tables: bool,
    /// White space differences count as changes (if false, white space runs are normalized).
    pub white_space: bool,
    /// Headers and footers are compared.
    pub headers: bool,
    /// Footnotes and endnotes are compared.
    pub footnotes: bool,
    /// Comments from the original document are preserved/compared.
    pub comments: bool,
    /// Fields are compared.
    pub fields: bool,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            format: true,
            case: true,
            tables: true,
            white_space: true,
            headers: true,
            footnotes: true,
            comments: true,
            fields: true,
        }
    }
}

fn normalize_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// A version's paragraph or table, by what it says (for lining the two versions up).
fn key(b: &Block, o: &CompareOptions) -> String {
    let fold = |s: String| {
        let s = if o.case { s } else { s.to_lowercase() };
        if o.white_space { s } else { normalize_whitespace(&s) }
    };
    match b {
        Block::Paragraph(p) => fold(p.text()),
        Block::Table(t) => {
            let mut s = String::from("\u{0}");
            for row in &t.rows {
                for cell in &row.cells {
                    for b in &cell.blocks {
                        if let Block::Paragraph(p) = b {
                            s.push_str(&fold(p.text()));
                            s.push('\u{1}');
                        }
                    }
                }
            }
            s
        }
    }
}

/// The longest common subsequence of `a` and `b` as pairs of indices; empty when the table would be too large.
fn lcs<T: PartialEq>(a: &[T], b: &[T]) -> Option<Vec<(usize, usize)>> {
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > 16_000_000 {
        return None;
    }
    let mut table = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[at(i, j)] = if a[i] == b[j] { table[at(i + 1, j + 1)] + 1 } else { table[at(i + 1, j)].max(table[at(i, j + 1)]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if table[at(i + 1, j)] >= table[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    Some(out)
}

/// The words of `text`: a run of ASCII letters or digits is one, a run of whitespace is one, any other character is one; as `(start, end)` in characters.
fn words(text: &str) -> Vec<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        if chars[i].is_ascii_alphanumeric() {
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                i += 1;
            }
        } else if chars[i].is_whitespace() {
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
        } else {
            i += 1;
        }
        out.push((start, i));
    }
    out
}

/// How much two texts share, 0 to 1 (Dice on their characters).
fn similarity(a: &str, b: &str) -> f32 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let mut count = std::collections::HashMap::new();
    for c in &a {
        *count.entry(*c).or_insert(0i32) += 1;
    }
    let mut shared = 0;
    for c in &b {
        if let Some(n) = count.get_mut(c)
            && *n > 0
        {
            *n -= 1;
            shared += 1;
        }
    }
    2.0 * shared as f32 / (a.len() + b.len()) as f32
}

/// The runs of `p` from character `start` to `end`.
fn slice(p: &FlowParagraph, start: usize, end: usize) -> Vec<FlowRun> {
    let mut q = p.clone();
    q.split_off_runs(end);
    q.split_off_runs(start)
}

fn marked(mut runs: Vec<FlowRun>, kind: RevisionKind, author: &str, date: &str, combine: bool) -> Vec<FlowRun> {
    for r in &mut runs {
        if !combine || r.revision.is_none() {
            r.revision = Some(Revision { kind: kind.clone(), author: author.to_string(), date: date.to_string() });
        }
    }
    runs
}

/// `revised` with the words it changed from `original` marked as tracked changes.
fn compare_paragraphs(original: &FlowParagraph, revised: &FlowParagraph, author: &str, date: &str, opts: &CompareOptions, combine: bool) -> FlowParagraph {
    let (ot, rt) = (original.text(), revised.text());
    let (ow, rw) = (words(&ot), words(&rt));
    let (oc, rc): (Vec<char>, Vec<char>) = (ot.chars().collect(), rt.chars().collect());
    let word = |c: &[char], w: (usize, usize)| {
        let raw: String = c[w.0..w.1].iter().collect();
        let s = if opts.case { raw } else { raw.to_lowercase() };
        if !opts.white_space && s.chars().all(char::is_whitespace) {
            " ".to_string()
        } else {
            s
        }
    };
    let (ok, rk): (Vec<String>, Vec<String>) = (ow.iter().map(|w| word(&oc, *w)).collect(), rw.iter().map(|w| word(&rc, *w)).collect());
    let mut out = revised.clone();
    out.runs.clear();
    let Some(pairs) = lcs(&ok, &rk) else {
        out.runs.extend(marked(slice(original, 0, oc.len()), RevisionKind::Delete, author, date, combine));
        out.runs.extend(marked(slice(revised, 0, rc.len()), RevisionKind::Insert, author, date, combine));
        return out;
    };
    let (mut oi, mut ri) = (0, 0);
    let mut steps: Vec<(usize, usize)> = pairs;
    steps.push((ow.len(), rw.len()));
    for (po, pr) in steps {
        // What lies between the matches: the original's words deleted, the revision's inserted.
        if oi < po {
            out.runs.extend(marked(slice(original, ow[oi].0, ow[po - 1].1), RevisionKind::Delete, author, date, combine));
        }
        if ri < pr {
            out.runs.extend(marked(slice(revised, rw[ri].0, rw[pr - 1].1), RevisionKind::Insert, author, date, combine));
        }
        if po < ow.len() && pr < rw.len() {
            let mut same = slice(revised, rw[pr].0, rw[pr].1);
            let before = slice(original, ow[po].0, ow[po].1);
            if let (Some(o), Some(r)) = (before.first(), same.first())
                && opts.format
                && o.format != r.format
                && (!combine || (o.revision.is_none() && r.revision.is_none()))
            {
                let old = o.format.clone();
                for run in &mut same {
                    if !combine || run.revision.is_none() {
                        run.revision = Some(Revision { kind: RevisionKind::Format(old.clone()), author: author.to_string(), date: date.to_string() });
                    }
                }
            }
            out.runs.extend(same);
        }
        oi = po + 1;
        ri = pr + 1;
    }
    // Neighbouring pieces of the same text and formatting become one run again.
    out
}

/// The largest id in `blocks`, cells, rows and paragraphs included.
fn max_id(blocks: &[Block]) -> u32 {
    blocks
        .iter()
        .map(|b| match b {
            Block::Paragraph(p) => p.id,
            Block::Table(t) => t.rows.iter().flat_map(|r| std::iter::once(r.id).chain(r.cells.iter().map(|c| c.id.max(max_id(&c.blocks))))).max().unwrap_or(0).max(t.id),
        })
        .max()
        .unwrap_or(0)
}

/// `blocks` with fresh ids (the original's blocks put into the revision must not share ids with it).
fn renumber(blocks: &mut [Block], next_id: &mut u32) {
    fn take(next_id: &mut u32) -> u32 {
        let id = *next_id;
        *next_id += 1;
        id
    }
    for b in blocks {
        match b {
            Block::Paragraph(p) => p.id = take(next_id),
            Block::Table(t) => {
                t.id = take(next_id);
                for row in &mut t.rows {
                    row.id = take(next_id);
                    for cell in &mut row.cells {
                        cell.id = take(next_id);
                        renumber(&mut cell.blocks, next_id);
                    }
                }
            }
        }
    }
}

/// `blocks` with all of their text marked `kind`.
fn mark_all(blocks: &[Block], kind: &RevisionKind, author: &str, date: &str, combine: bool) -> Vec<Block> {
    blocks
        .iter()
        .map(|b| match b {
            Block::Paragraph(p) => {
                let mut p = p.clone();
                let len = p.text().chars().count();
                p.runs = marked(slice(&p, 0, len), kind.clone(), author, date, combine);
                Block::Paragraph(p)
            }
            Block::Table(t) => {
                let mut t = t.clone();
                for row in &mut t.rows {
                    for cell in &mut row.cells {
                        cell.blocks = mark_all(&cell.blocks, kind, author, date, combine);
                    }
                }
                Block::Table(t)
            }
        })
        .collect()
}

/// The direct formatting of each character of `p`.
fn formats(p: &FlowParagraph) -> Vec<&crate::flow::RunFormat> {
    p.runs.iter().flat_map(|r| std::iter::repeat_n(&r.format, r.len())).collect()
}

/// Whether two tables have the same rows, each of the same number of cells.
fn same_shape(a: &FlowTable, b: &FlowTable) -> bool {
    a.rows.len() == b.rows.len() && a.rows.iter().zip(&b.rows).all(|(x, y)| x.cells.len() == y.cells.len())
}

/// The body of `revised` with the changes from `original` as tracked changes by `author` at `date`.
pub fn compare_blocks(original: &[Block], revised: &[Block], author: &str, date: &str) -> Vec<Block> {
    compare_blocks_with(original, revised, author, date, &CompareOptions::default())
}

/// [`compare_blocks`] looking at what `opts` says.
pub fn compare_blocks_with(original: &[Block], revised: &[Block], author: &str, date: &str, opts: &CompareOptions) -> Vec<Block> {
    let mut next_id = max_id(original).max(max_id(revised)) + 1;
    compare_in(original, revised, author, date, opts, &mut next_id, false)
}

/// Combines the revisions of two documents (校閲 > 文書の結合, Word's 「文書の結合」).
/// Revisions already present in either document (e.g. from other authors) are preserved with their original
/// authors and timestamps; only newly detected differences are marked with `author` and `date`.
pub fn combine_blocks(original: &[Block], revised: &[Block], author: &str, date: &str) -> Vec<Block> {
    combine_blocks_with(original, revised, author, date, &CompareOptions::default())
}

/// [`combine_blocks`] looking at what `opts` says.
pub fn combine_blocks_with(original: &[Block], revised: &[Block], author: &str, date: &str, opts: &CompareOptions) -> Vec<Block> {
    let mut next_id = max_id(original).max(max_id(revised)) + 1;
    compare_in(original, revised, author, date, opts, &mut next_id, true)
}

fn compare_notes(
    original: &[Note],
    revised: &[Note],
    author: &str,
    date: &str,
    opts: &CompareOptions,
    next_id: &mut u32,
    combine: bool,
) -> Vec<Note> {
    let mut out = Vec::new();
    for (idx, r_note) in revised.iter().enumerate() {
        let o_note = original.iter().find(|n| n.id == r_note.id).or_else(|| original.get(idx));
        if let Some(o_note) = o_note {
            let blocks = compare_in(&o_note.blocks, &r_note.blocks, author, date, opts, next_id, combine);
            out.push(Note { id: r_note.id, blocks, custom_mark: r_note.custom_mark });
        } else {
            out.push(r_note.clone());
        }
    }
    out
}

fn merge_comments(original: &[Comment], revised: &[Comment]) -> Vec<Comment> {
    let mut out = revised.to_vec();
    for oc in original {
        if !out.iter().any(|rc| rc.id == oc.id || (rc.para_id.is_some() && rc.para_id == oc.para_id)) {
            out.push(oc.clone());
        }
    }
    out
}

/// Compares two documents as tracked changes across body, headers, footers, notes, and comments.
pub fn compare_documents(original: &FlowDocument, revised: &FlowDocument, author: &str, date: &str) -> FlowDocument {
    compare_documents_with(original, revised, author, date, &CompareOptions::default())
}

/// [`compare_documents`] looking at what `opts` says.
pub fn compare_documents_with(original: &FlowDocument, revised: &FlowDocument, author: &str, date: &str, opts: &CompareOptions) -> FlowDocument {
    let mut out = revised.clone();
    let mut next_id = max_id(&original.body).max(max_id(&revised.body)) + 1;
    out.body = compare_in(&original.body, &revised.body, author, date, opts, &mut next_id, false);

    if opts.headers {
        out.header = compare_in(&original.header, &revised.header, author, date, opts, &mut next_id, false);
        out.footer = compare_in(&original.footer, &revised.footer, author, date, opts, &mut next_id, false);
        out.header_first = compare_in(&original.header_first, &revised.header_first, author, date, opts, &mut next_id, false);
        out.footer_first = compare_in(&original.footer_first, &revised.footer_first, author, date, opts, &mut next_id, false);
        out.header_even = compare_in(&original.header_even, &revised.header_even, author, date, opts, &mut next_id, false);
        out.footer_even = compare_in(&original.footer_even, &revised.footer_even, author, date, opts, &mut next_id, false);
    }

    if opts.footnotes {
        out.footnotes = compare_notes(&original.footnotes, &revised.footnotes, author, date, opts, &mut next_id, false);
        out.endnotes = compare_notes(&original.endnotes, &revised.endnotes, author, date, opts, &mut next_id, false);
    }

    if opts.comments {
        out.comments = merge_comments(&original.comments, &revised.comments);
    }

    out.track_changes = true;
    out
}

/// Combines the revisions of two documents across body, headers, footers, notes, and comments.
pub fn combine_documents(original: &FlowDocument, revised: &FlowDocument, author: &str, date: &str) -> FlowDocument {
    combine_documents_with(original, revised, author, date, &CompareOptions::default())
}

/// [`combine_documents`] looking at what `opts` says.
pub fn combine_documents_with(original: &FlowDocument, revised: &FlowDocument, author: &str, date: &str, opts: &CompareOptions) -> FlowDocument {
    let mut out = revised.clone();
    let mut next_id = max_id(&original.body).max(max_id(&revised.body)) + 1;
    out.body = compare_in(&original.body, &revised.body, author, date, opts, &mut next_id, true);

    if opts.headers {
        out.header = compare_in(&original.header, &revised.header, author, date, opts, &mut next_id, true);
        out.footer = compare_in(&original.footer, &revised.footer, author, date, opts, &mut next_id, true);
        out.header_first = compare_in(&original.header_first, &revised.header_first, author, date, opts, &mut next_id, true);
        out.footer_first = compare_in(&original.footer_first, &revised.footer_first, author, date, opts, &mut next_id, true);
        out.header_even = compare_in(&original.header_even, &revised.header_even, author, date, opts, &mut next_id, true);
        out.footer_even = compare_in(&original.footer_even, &revised.footer_even, author, date, opts, &mut next_id, true);
    }

    if opts.footnotes {
        out.footnotes = compare_notes(&original.footnotes, &revised.footnotes, author, date, opts, &mut next_id, true);
        out.endnotes = compare_notes(&original.endnotes, &revised.endnotes, author, date, opts, &mut next_id, true);
    }

    if opts.comments {
        out.comments = merge_comments(&original.comments, &revised.comments);
    }

    out.track_changes = true;
    out
}

fn compare_in(original: &[Block], revised: &[Block], author: &str, date: &str, opts: &CompareOptions, next_id: &mut u32, combine: bool) -> Vec<Block> {
    let (ok, rk): (Vec<String>, Vec<String>) = (original.iter().map(|b| key(b, opts)).collect(), revised.iter().map(|b| key(b, opts)).collect());
    let mut anchors = lcs(&ok, &rk).unwrap_or_default();
    anchors.push((original.len(), revised.len()));
    let mut out = Vec::new();
    let (mut oi, mut ri) = (0, 0);
    // An original block only in the original, shown deleted (with ids of its own).
    let gone = |d: &Block, next_id: &mut u32| -> Block {
        let mut one = mark_all(std::slice::from_ref(d), &RevisionKind::Delete, author, date, combine);
        renumber(&mut one, next_id);
        one.remove(0)
    };
    for (po, pr) in anchors {
        // The gap: original[oi..po] only in the original, revised[ri..pr] only in the revision.
        let deleted: Vec<&Block> = original[oi..po].iter().collect();
        let mut at = 0;
        // Blocks only in the revision wait until the deleted ones before them are put, as Word shows a deletion first.
        let mut waiting: Vec<Block> = Vec::new();
        for b in &revised[ri..pr] {
            let found = match b {
                Block::Paragraph(r) => (at..deleted.len()).find(|&j| matches!(deleted[j], Block::Paragraph(o) if similarity(&o.text(), &r.text()) >= 0.5 && !o.text().is_empty())),
                Block::Table(r) => (at..deleted.len()).find(|&j| matches!(deleted[j], Block::Table(o) if same_shape(o, r))),
            };
            match (found, b) {
                (Some(j), _) => {
                    for d in &deleted[at..j] {
                        out.push(gone(d, next_id));
                    }
                    out.append(&mut waiting);
                    match (deleted[j], b) {
                        (Block::Paragraph(o), Block::Paragraph(r)) => out.push(Block::Paragraph(compare_paragraphs(o, r, author, date, opts, combine))),
                        (Block::Table(o), Block::Table(r)) if opts.tables => {
                            let mut t = r.clone();
                            for (orow, rrow) in o.rows.iter().zip(&mut t.rows) {
                                for (ocell, rcell) in orow.cells.iter().zip(&mut rrow.cells) {
                                    rcell.blocks = compare_in(&ocell.blocks, &rcell.blocks, author, date, opts, next_id, combine);
                                }
                            }
                            out.push(Block::Table(t));
                        }
                        (_, b) => out.push(b.clone()),
                    }
                    at = j + 1;
                }
                (None, _) => waiting.extend(mark_all(std::slice::from_ref(b), &RevisionKind::Insert, author, date, combine)),
            }
        }
        for d in &deleted[at..] {
            out.push(gone(d, next_id));
        }
        out.append(&mut waiting);
        if pr < revised.len() {
            // Alike in text: still compared when a word is formatted differently.
            match (original.get(po), &revised[pr]) {
                (Some(Block::Paragraph(o)), Block::Paragraph(r)) if opts.format && formats(o) != formats(r) => out.push(Block::Paragraph(compare_paragraphs(o, r, author, date, opts, combine))),
                (Some(Block::Table(o)), Block::Table(r)) if opts.tables && same_shape(o, r) => {
                    let mut t = r.clone();
                    for (orow, rrow) in o.rows.iter().zip(&mut t.rows) {
                        for (ocell, rcell) in orow.cells.iter().zip(&mut rrow.cells) {
                            rcell.blocks = compare_in(&ocell.blocks, &rcell.blocks, author, date, opts, next_id, combine);
                        }
                    }
                    out.push(Block::Table(t));
                }
                _ => out.push(revised[pr].clone()),
            }
        }
        oi = po + 1;
        ri = pr + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{FlowRunKind, FlowRunStyle};

    fn paragraph(id: u32, text: &str) -> Block {
        let mut p = FlowParagraph { id, ..FlowParagraph::default() };
        if !text.is_empty() {
            p.runs.push(FlowRun::new(FlowRunKind::Text(text.to_string()), FlowRunStyle::default()));
        }
        Block::Paragraph(p)
    }

    /// The paragraph's text with deletions in 【】 and insertions in 《》.
    fn show(b: &Block) -> String {
        let Block::Paragraph(p) = b else { return "[table]".into() };
        p.runs
            .iter()
            .map(|r| {
                let FlowRunKind::Text(t) = &r.kind else { return String::new() };
                match r.revision.as_ref().map(|v| &v.kind) {
                    Some(RevisionKind::Delete | RevisionKind::MoveFrom) => format!("【{t}】"),
                    Some(RevisionKind::Insert | RevisionKind::MoveTo) => format!("《{t}》"),
                    _ => t.clone(),
                }
            })
            .collect()
    }

    #[test]
    fn words_changed_inside_a_paragraph() {
        let o = [paragraph(1, "第1条 甲は乙に100万円を支払う。"), paragraph(2, "第2条 期間は1年とする。")];
        let r = [paragraph(1, "第1条 甲は乙に200万円を支払う。"), paragraph(2, "第2条 期間は1年とする。")];
        let out = compare_blocks(&o, &r, "me", "2026-10-11T00:00:00Z");
        let shown: Vec<String> = out.iter().map(show).collect();
        assert_eq!(shown, ["第1条 甲は乙に【100】《200》万円を支払う。", "第2条 期間は1年とする。"]);
    }

    #[test]
    fn paragraphs_added_and_removed() {
        let o = [paragraph(1, "前文"), paragraph(2, "削る段落です"), paragraph(3, "末尾")];
        let r = [paragraph(1, "前文"), paragraph(2, "足した段落です、まったく別の文"), paragraph(3, "末尾")];
        let out = compare_blocks(&o, &r, "me", "d");
        let shown: Vec<String> = out.iter().map(show).collect();
        assert_eq!(shown, ["前文", "【削る段落です】", "《足した段落です、まったく別の文》", "末尾"]);
        let ids: std::collections::HashSet<u32> = out.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p.id) } else { None }).collect();
        assert_eq!(ids.len(), 4, "ids are distinct");
    }

    #[test]
    fn same_text_has_no_changes() {
        let o = [paragraph(1, "同じ"), paragraph(2, "")];
        let out = compare_blocks(&o, &o.clone(), "me", "d");
        assert_eq!(out.iter().map(show).collect::<Vec<_>>(), ["同じ", ""]);
    }

    fn table(id: u32, cells: &[&str]) -> Block {
        use crate::flow::{FlowCell, FlowRow};
        let row = FlowRow { id: id + 1, cells: cells.iter().enumerate().map(|(i, t)| FlowCell { id: id + 2 + i as u32, blocks: vec![paragraph(id + 100 + i as u32, t)], ..FlowCell::default() }).collect(), ..FlowRow::default() };
        Block::Table(FlowTable { id, rows: vec![row], ..FlowTable::default() })
    }

    fn cell_texts(b: &Block) -> Vec<String> {
        let Block::Table(t) = b else { return vec![] };
        t.rows.iter().flat_map(|r| &r.cells).map(|c| c.blocks.iter().map(show).collect::<String>()).collect()
    }

    #[test]
    fn tables_compared_by_cell_and_added_or_removed() {
        let o = [paragraph(1, "前"), table(10, &["りんご", "100円"]), paragraph(2, "後")];
        let r = [paragraph(1, "前"), table(10, &["りんご", "200円"]), paragraph(2, "後")];
        let out = compare_blocks(&o, &r, "me", "d");
        assert_eq!(cell_texts(&out[1]), ["りんご", "【100】《200》円"]);
        // A table only in the revision is inserted; one only in the original deleted.
        let out = compare_blocks(&[paragraph(1, "前")], &[paragraph(1, "前"), table(10, &["新しい"])], "me", "d");
        assert_eq!(cell_texts(&out[1]), ["《新しい》"]);
        let out = compare_blocks(&[paragraph(1, "前"), table(10, &["古い"])], &[paragraph(1, "前")], "me", "d");
        assert_eq!(out.len(), 2);
        assert_eq!(cell_texts(&out[1]), ["【古い】"]);
        let ids = [max_id(&out[..1]), max_id(&out[1..])];
        assert!(ids[0] < 10 && ids[1] > 10, "the deleted table has fresh ids: {ids:?}");
    }

    #[test]
    fn format_difference_of_the_same_word() {
        let o = [paragraph(1, "太字にする")];
        let mut r = o.clone();
        let Block::Paragraph(p) = &mut r[0] else { unreachable!() };
        p.runs[0].format.bold = Some(true);
        let out = compare_blocks(&o, &r, "me", "d");
        let Block::Paragraph(p) = &out[0] else { unreachable!() };
        assert!(p.runs.iter().all(|r| matches!(r.revision.as_ref().map(|v| &v.kind), Some(RevisionKind::Format(_)))), "every word carries a formatting change");
    }

    #[test]
    fn options_turn_checks_off() {
        let o = [paragraph(1, "Hello world")];
        let r = [paragraph(1, "hello world")];
        let default = compare_blocks(&o, &r, "me", "d");
        assert_eq!(show(&default[0]), "【Hello】《hello》 world");
        let ignore_case = compare_blocks_with(&o, &r, "me", "d", &CompareOptions { case: false, ..CompareOptions::default() });
        assert_eq!(show(&ignore_case[0]), "hello world");
        let mut bold = r.clone();
        let Block::Paragraph(p) = &mut bold[0] else { unreachable!() };
        p.runs[0].format.bold = Some(true);
        let plain = [paragraph(1, "hello world")];
        let none = compare_blocks_with(&plain, &bold, "me", "d", &CompareOptions { format: false, ..CompareOptions::default() });
        let Block::Paragraph(q) = &none[0] else { unreachable!() };
        assert!(q.runs.iter().all(|r| r.revision.is_none()));
    }

    #[test]
    fn combine_preserves_existing_revisions_from_different_authors() {
        use crate::flow::Revision;
        let mut p1 = FlowParagraph { id: 1, ..FlowParagraph::default() };
        p1.runs.push(FlowRun::new(FlowRunKind::Text("Base text ".into()), FlowRunStyle::default()));
        let mut alice_run = FlowRun::new(FlowRunKind::Text("Alice addition".into()), FlowRunStyle::default());
        alice_run.revision = Some(Revision { kind: RevisionKind::Insert, author: "Alice".into(), date: "2026-10-01".into() });
        p1.runs.push(alice_run);

        let mut p2 = FlowParagraph { id: 1, ..FlowParagraph::default() };
        p2.runs.push(FlowRun::new(FlowRunKind::Text("Base text ".into()), FlowRunStyle::default()));
        let mut bob_run = FlowRun::new(FlowRunKind::Text("Bob addition".into()), FlowRunStyle::default());
        bob_run.revision = Some(Revision { kind: RevisionKind::Insert, author: "Bob".into(), date: "2026-10-02".into() });
        p2.runs.push(bob_run);

        let o = [Block::Paragraph(p1)];
        let r = [Block::Paragraph(p2)];

        let out = combine_blocks(&o, &r, "Charlie", "2026-10-03");
        let Block::Paragraph(combined_p) = &out[0] else { unreachable!() };
        let authors: Vec<Option<&str>> = combined_p.runs.iter().map(|run| run.revision.as_ref().map(|v| v.author.as_str())).collect();
        assert!(authors.contains(&Some("Alice")), "Alice's revision author preserved: {authors:?}");
        assert!(authors.contains(&Some("Bob")), "Bob's revision author preserved: {authors:?}");
    }

    #[test]
    fn white_space_option_ignores_whitespace_differences() {
        let o = [paragraph(1, "Hello  world")];
        let r = [paragraph(1, "Hello world")];
        let with_ws = compare_blocks_with(&o, &r, "me", "d", &CompareOptions { white_space: true, ..CompareOptions::default() });
        assert_eq!(show(&with_ws[0]), "Hello【  】《 》world");

        let ignore_ws = compare_blocks_with(&o, &r, "me", "d", &CompareOptions { white_space: false, ..CompareOptions::default() });
        assert_eq!(show(&ignore_ws[0]), "Hello world");
        let Block::Paragraph(p) = &ignore_ws[0] else { unreachable!() };
        assert!(p.runs.iter().all(|r| r.revision.is_none()), "no tracked changes when whitespace differences are ignored");
    }

    #[test]
    fn compare_documents_compares_headers_and_notes_and_comments() {
        let mut doc1 = FlowDocument::default();
        doc1.body = vec![paragraph(1, "本文1")];
        doc1.header = vec![paragraph(2, "ヘッダー旧")];
        doc1.footnotes = vec![Note { id: 1, blocks: vec![paragraph(3, "脚注旧")], ..Default::default() }];
        doc1.comments = vec![Comment { id: 10, author: "Alice".into(), initials: "A".into(), date: "d1".into(), blocks: vec![paragraph(4, "コメント1")], parent: None, done: false, para_id: None }];

        let mut doc2 = FlowDocument::default();
        doc2.body = vec![paragraph(1, "本文2")];
        doc2.header = vec![paragraph(2, "ヘッダー新")];
        doc2.footnotes = vec![Note { id: 1, blocks: vec![paragraph(3, "脚注新")], ..Default::default() }];
        doc2.comments = vec![Comment { id: 20, author: "Bob".into(), initials: "B".into(), date: "d2".into(), blocks: vec![paragraph(5, "コメント2")], parent: None, done: false, para_id: None }];

        let out = compare_documents_with(&doc1, &doc2, "Reviewer", "2026-10-17T00:00:00Z", &CompareOptions::default());
        assert!(out.track_changes, "track_changes is turned on");
        assert_eq!(show(&out.body[0]), "本文【1】《2》");
        assert_eq!(show(&out.header[0]), "ヘッダー【旧】《新》");
        assert_eq!(show(&out.footnotes[0].blocks[0]), "脚注【旧】《新》");
        assert_eq!(out.comments.len(), 2, "both comments are preserved");
        let comment_authors: Vec<&str> = out.comments.iter().map(|c| c.author.as_str()).collect();
        assert!(comment_authors.contains(&"Bob") && comment_authors.contains(&"Alice"));
    }
}


