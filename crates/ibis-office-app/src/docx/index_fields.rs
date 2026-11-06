// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 索引 (Word's INDEX and XE fields): the entries the XE fields of a document mark, and the INDEX field's paragraphs made of
//! them: sorted as Word sorts (Latin letters first, case apart; Japanese by the Shift_JIS code of the text, or of its reading when
//! the field has `\y`), grouped under headings (`\h`), with the pages and ranges the entries are on.

use ibis_doc_model::{Block, FlowDocument, FlowParagraph, FlowRun, FlowRunKind, FlowRunStyle, RunFormat};

/// What an XE field says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct XeSpec {
    /// The entry's levels (`東京:港区` is `["東京", "港区"]`).
    pub(crate) levels: Vec<String>,
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    /// `\r`: the bookmark whose range the entry covers.
    pub(crate) range: Option<String>,
    /// `\t`: the text shown instead of a page (a cross-reference).
    pub(crate) see: Option<String>,
    /// `\f`: the kind of index it belongs to.
    pub(crate) kind: String,
    /// `\y`: the reading.
    pub(crate) yomi: Option<String>,
}

/// The words of a field's code: quoted words without their quotes (`\"` and `\\` in them unescaped), switches as they are.
pub(crate) fn words(instruction: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = instruction.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '"' {
            chars.next();
            let mut word = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '"' => break,
                    '\\' if matches!(chars.peek(), Some('"' | '\\')) => word.push(chars.next().unwrap_or('\\')),
                    c => word.push(c),
                }
            }
            out.push(word);
        } else {
            let mut word = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() {
                    break;
                }
                word.push(c);
                chars.next();
            }
            out.push(word);
        }
    }
    out
}

/// A field's switch argument (`\r "bookmark"`): the word after it.
pub(crate) fn switch_value(words: &[String], switch: &str) -> Option<String> {
    let at = words.iter().position(|w| w.eq_ignore_ascii_case(switch))?;
    words.get(at + 1).cloned()
}

/// The entry an `XE "東京:港区" \b \i \r "range" \t "text" \f "x" \y "とうきょう"` field marks.
pub(crate) fn xe_spec(instruction: &str) -> Option<XeSpec> {
    let words = words(instruction);
    if !words.first()?.eq_ignore_ascii_case("XE") {
        return None;
    }
    let text = words.get(1).filter(|w| !w.starts_with('\\') || w.len() > 2)?;
    // `\:` in the text is a colon of the entry's own.
    let mut levels = Vec::new();
    let mut level = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&':') => {
                level.push(':');
                chars.next();
            }
            ':' => levels.push(std::mem::take(&mut level)),
            c => level.push(c),
        }
    }
    levels.push(level);
    let levels: Vec<String> = levels.into_iter().map(|l| l.trim().to_string()).collect();
    if levels.iter().all(String::is_empty) {
        return None;
    }
    Some(XeSpec {
        levels,
        bold: words.iter().any(|w| w.eq_ignore_ascii_case("\\b")),
        italic: words.iter().any(|w| w.eq_ignore_ascii_case("\\i")),
        range: switch_value(&words, "\\r"),
        see: switch_value(&words, "\\t"),
        kind: switch_value(&words, "\\f").unwrap_or_default(),
        yomi: switch_value(&words, "\\y"),
    })
}

/// An INDEX field's switches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndexSpec {
    pub(crate) columns: u32,
    /// `\h`: a heading above each group (`Some("A")`: the heading's form).
    pub(crate) headings: Option<String>,
    /// `\e`: between an entry and its pages.
    pub(crate) entry_separator: String,
    /// `\l`: between page numbers.
    pub(crate) page_separator: String,
    /// `\g`: between the first and the last page of a range.
    pub(crate) range_separator: String,
    pub(crate) kind: String,
    /// `\y`: sorted by the readings the entries have.
    pub(crate) by_reading: bool,
}

pub(crate) fn index_spec(instruction: &str) -> Option<IndexSpec> {
    let words = words(instruction);
    if !words.first()?.eq_ignore_ascii_case("INDEX") {
        return None;
    }
    Some(IndexSpec {
        columns: switch_value(&words, "\\c").and_then(|v| v.parse().ok()).unwrap_or(1),
        headings: switch_value(&words, "\\h"),
        entry_separator: switch_value(&words, "\\e").unwrap_or_else(|| ", ".to_string()),
        page_separator: switch_value(&words, "\\l").unwrap_or_else(|| ", ".to_string()),
        range_separator: switch_value(&words, "\\g").unwrap_or_else(|| "\u{2013}".to_string()),
        kind: switch_value(&words, "\\f").unwrap_or_default(),
        by_reading: words.iter().any(|w| w.eq_ignore_ascii_case("\\y")),
    })
}

/// One XE field's place: its entry and the page it is on (and where its range ends).
#[derive(Clone, Debug)]
struct Mark {
    spec: XeSpec,
    page: u32,
    end_page: Option<u32>,
}

/// Every XE field of the document's body (cells too), with its page.
fn marks(doc: &FlowDocument, page_of: &dyn Fn(u32) -> Option<u32>) -> Vec<Mark> {
    // Where each bookmark ends, to take a range's last page.
    fn ends(blocks: &[Block], out: &mut Vec<(String, u32)>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => out.extend(p.bookmark_ends.iter().map(|n| (n.clone(), p.id))),
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| ends(&c.blocks, out)),
            }
        }
    }
    fn walk(blocks: &[Block], out: &mut Vec<(u32, XeSpec)>) {
        for b in blocks {
            match b {
                Block::Paragraph(p) => {
                    for r in &p.runs {
                        if let FlowRunKind::Field { instruction, .. } = &r.kind
                            && let Some(spec) = xe_spec(instruction)
                        {
                            out.push((p.id, spec));
                        }
                    }
                }
                Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
            }
        }
    }
    let mut bookmark_ends = Vec::new();
    ends(&doc.body, &mut bookmark_ends);
    let mut found = Vec::new();
    walk(&doc.body, &mut found);
    found
        .into_iter()
        .map(|(para, spec)| {
            let page = page_of(para).unwrap_or(1);
            let end_page = spec.range.as_ref().and_then(|name| bookmark_ends.iter().find(|(n, _)| n == name)).and_then(|(_, p)| page_of(*p)).filter(|e| *e > page);
            Mark { spec, page, end_page }
        })
        .collect()
}

/// The sort key of an entry's text: the Shift_JIS bytes of the text (Latin letters in lower case, which sorts them
/// before the kana and the kanji, whose Shift_JIS codes follow the Japanese order of their readings), the reading's when
/// it is sorted by readings.
pub(crate) fn sort_key(text: &str) -> (Vec<u8>, Vec<u8>) {
    let lower: String = text.chars().map(|c| c.to_ascii_lowercase()).collect();
    // Of letters that differ only in case, the lower case one first.
    let swapped: String = text.chars().map(|c| if c.is_ascii_lowercase() { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() }).collect();
    (ibis_pptx::csv::encode_sjis(&lower), ibis_pptx::csv::encode_sjis(&swapped))
}

/// The heading an entry goes under (Word's `\h`): a Latin letter as a full-width capital, a kana as itself (a katakana as the hiragana),
/// the rest 漢字.
fn heading_of(text: &str) -> String {
    let Some(c) = text.chars().next() else { return crate::shell::i18n::text("_docx.index-heading-symbols").to_string() };
    match c {
        'a'..='z' | 'A'..='Z' => char::from_u32(c.to_ascii_uppercase() as u32 - 'A' as u32 + 'Ａ' as u32).unwrap_or(c).to_string(),
        '\u{30a1}'..='\u{30f6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c).to_string(),
        '\u{3041}'..='\u{3096}' => c.to_string(),
        '0'..='9' | '０'..='９' | ' ' | '!'..='/' | ':'..='@' | '['..='`' | '{'..='~' => crate::shell::i18n::text("_docx.index-heading-symbols").to_string(),
        _ => "漢字".to_string(), // i18n-ok（Word の索引の見出し（文書に保存される）。訳さない）
    }
}

/// An entry of the index: its levels, and what is known of its pages.
#[derive(Clone, Debug, Default)]
struct Entry {
    levels: Vec<String>,
    yomi: Option<String>,
    /// (first page, last page of the range), bold, italic.
    pages: Vec<((u32, Option<u32>), bool, bool)>,
    see: Vec<String>,
}

/// The paragraphs of the INDEX field `instruction`: its entries in order, the heading paragraphs, each with the style
/// `index heading`, `index 1`, `index 2`… The ids start at `first_id`; `width` is the text's width (the right tab stop).
pub(crate) fn index_paragraphs(doc: &FlowDocument, instruction: &str, page_of: &dyn Fn(u32) -> Option<u32>, first_id: u32, width: i64) -> Vec<FlowParagraph> {
    let Some(spec) = index_spec(instruction) else { return Vec::new() };
    let marks: Vec<Mark> = marks(doc, page_of).into_iter().filter(|m| m.spec.kind == spec.kind).collect();
    // The entries: one for each distinct list of levels, and one for each shorter list of one that has levels under it.
    let mut entries: Vec<Entry> = Vec::new();
    let entry_at = |levels: &[String], entries: &mut Vec<Entry>| -> usize {
        match entries.iter().position(|e| e.levels == levels) {
            Some(i) => i,
            None => {
                entries.push(Entry { levels: levels.to_vec(), ..Entry::default() });
                entries.len() - 1
            }
        }
    };
    for m in &marks {
        let levels: Vec<String> = m.spec.levels.iter().take(3).cloned().collect();
        for depth in 1..levels.len() {
            entry_at(&levels[..depth], &mut entries);
        }
        let i = entry_at(&levels, &mut entries);
        let e = &mut entries[i];
        if e.yomi.is_none() {
            e.yomi = m.spec.yomi.clone();
        }
        match &m.spec.see {
            Some(see) => e.see.push(see.clone()),
            None => e.pages.push(((m.page, m.end_page), m.spec.bold, m.spec.italic)),
        }
    }
    // Sorted: by the first level, then the second…; the reading of a level stands for its text with `\y`.
    let key_of = |e: &Entry, depth: usize| -> (Vec<u8>, Vec<u8>) {
        let text = &e.levels[depth];
        // A reading belongs to the entry's last level.
        let own = depth + 1 == e.levels.len();
        match (&e.yomi, own, spec.by_reading) {
            (Some(yomi), true, true) => sort_key(yomi),
            _ => sort_key(text),
        }
    };
    // The reading of a main entry whose own mark may be on another entry of the same level.
    let reading_of = |levels: &[String], entries: &[Entry]| -> Option<String> { entries.iter().find(|e| e.levels == levels).and_then(|e| e.yomi.clone()) };
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&a, &b| {
        let (ea, eb) = (&entries[a], &entries[b]);
        for depth in 0..ea.levels.len().max(eb.levels.len()) {
            match (ea.levels.get(depth), eb.levels.get(depth)) {
                (Some(_), Some(_)) => {
                    // The key of the level: the reading of the entry made of the levels to it.
                    let key = |e: &Entry| {
                        let own = &e.levels[..=depth];
                        match (reading_of(own, &entries), spec.by_reading) {
                            (Some(yomi), true) => sort_key(&yomi),
                            _ => key_of(e, depth),
                        }
                    };
                    let (ka, kb) = (key(ea), key(eb));
                    let order = ka.0.cmp(&kb.0).then_with(|| ka.1.cmp(&kb.1));
                    if order != std::cmp::Ordering::Equal {
                        return order;
                    }
                }
                (None, Some(_)) => return std::cmp::Ordering::Less,
                (Some(_), None) => return std::cmp::Ordering::Greater,
                (None, None) => {}
            }
        }
        std::cmp::Ordering::Equal
    });
    let styles = &doc.styles;
    let base = FlowRunStyle::default();
    let mut out: Vec<FlowParagraph> = Vec::new();
    let mut id = first_id;
    let mut heading: Option<String> = None;
    let tab = ibis_doc_model::TabChange { position: width, stop: Some(ibis_doc_model::TabStop { position: width, align: ibis_doc_model::TabAlign::Right, leader: ibis_doc_model::TabLeader::Dot }) };
    for &i in &order {
        let e = &entries[i];
        if spec.headings.is_some() && e.levels.len() == 1 {
            let label = match (&reading_of(&e.levels, &entries), spec.by_reading) {
                (Some(yomi), true) => heading_of(yomi),
                _ => heading_of(&e.levels[0]),
            };
            if heading.as_deref() != Some(label.as_str()) {
                let mut p = FlowParagraph { id, style: styles.paragraph_style_named("index heading").map(str::to_string), ..FlowParagraph::default() };
                p.direct.tabs = vec![tab.clone()];
                p.runs.push(FlowRun::new(FlowRunKind::Text(label.clone()), base.clone()));
                id += 1;
                p.resolve(styles);
                out.push(p);
                heading = Some(label);
            }
        }
        let level = e.levels.len().min(9);
        let mut p = FlowParagraph { id, style: styles.paragraph_style_named(&format!("index {level}")).map(str::to_string), ..FlowParagraph::default() };
        id += 1;
        p.direct.tabs = vec![tab.clone()];
        let text = e.levels.last().cloned().unwrap_or_default();
        // The pages: one item for each page (or range), in page order; bold or italic when any mark on it says so.
        let mut pages: Vec<((u32, Option<u32>), bool, bool)> = Vec::new();
        let mut sorted = e.pages.clone();
        sorted.sort_by_key(|(p, ..)| *p);
        for (page, bold, italic) in sorted {
            match pages.last_mut() {
                Some(last) if last.0 == page => {
                    last.1 |= bold;
                    last.2 |= italic;
                }
                _ => pages.push((page, bold, italic)),
            }
        }
        let see = e.see.clone();
        let run = |p: &mut FlowParagraph, text: String, bold: bool, italic: bool| {
            if text.is_empty() {
                return;
            }
            let mut r = FlowRun::new(FlowRunKind::Text(text), base.clone());
            r.format = RunFormat { bold: bold.then_some(true), italic: italic.then_some(true), ..RunFormat::default() };
            p.runs.push(r);
        };
        let separated = !pages.is_empty() || !see.is_empty();
        run(&mut p, if separated { format!("{text}{}", spec.entry_separator) } else { text }, false, false);
        let count = pages.len();
        for (n, ((first, last), bold, italic)) in pages.into_iter().enumerate() {
            let label = match last {
                Some(last) => format!("{first}{}{last}", spec.range_separator),
                None => first.to_string(),
            };
            run(&mut p, label, bold, italic);
            if n + 1 < count || !see.is_empty() {
                run(&mut p, spec.page_separator.clone(), false, false);
            }
        }
        for (n, text) in see.iter().enumerate() {
            run(&mut p, text.clone(), false, false);
            if n + 1 < see.len() {
                run(&mut p, spec.page_separator.clone(), false, false);
            }
        }
        p.resolve(styles);
        out.push(p);
    }
    if let Some(first) = out.first_mut() {
        first.field_start = Some(instruction.trim().to_string());
    }
    if let Some(last) = out.last_mut() {
        last.field_end = true;
    }
    out
}

/// The first paragraph of the document's INDEX field (it starts one) and the field's instruction.
pub(crate) fn index_field(doc: &FlowDocument) -> Option<(u32, String)> {
    doc.body.iter().find_map(|b| match b {
        Block::Paragraph(p) => p.field_start.as_ref().filter(|i| i.trim_start().to_ascii_uppercase().starts_with("INDEX")).map(|i| (p.id, i.clone())),
        _ => None,
    })
}

/// The commands that make the index of the document again (or put a new one before paragraph `before`): the styles it
/// uses first, then its paragraphs in place of the old field's. Empty when there is no such field.
pub(crate) fn index_commands(doc: &FlowDocument, before: u32, replace: bool, instruction: &str, page_of: &dyn Fn(u32) -> Option<u32>) -> Vec<ibis_doc_model::FlowCommand> {
    let mut commands: Vec<ibis_doc_model::FlowCommand> = missing_index_styles(&doc.styles, 1..=3).into_iter().map(|(id, style)| ibis_doc_model::FlowCommand::SetStyle { id, style: Some(style) }).collect();
    // The paragraphs are made with the styles the document has once those are set.
    let mut with_styles = doc.clone();
    {
        let styles = std::sync::Arc::make_mut(&mut with_styles.styles);
        for c in &commands {
            if let ibis_doc_model::FlowCommand::SetStyle { id, style: Some(style) } = c {
                styles.styles.insert(id.clone(), style.clone());
            }
        }
    }
    let width = doc.page.size.width - doc.page.left - doc.page.right;
    let mut entries = index_paragraphs(&with_styles, instruction, page_of, doc.new_id(), width);
    if entries.is_empty() {
        let mut p = FlowParagraph { id: doc.new_id(), ..FlowParagraph::default() };
        p.runs.push(FlowRun::new(FlowRunKind::Text(crate::shell::i18n::text("_docx.no-index-entries-found").into()), FlowRunStyle::default()));
        p.field_start = Some(instruction.trim().to_string());
        p.field_end = true;
        p.resolve(&with_styles.styles);
        entries.push(p);
    }
    // An index in columns (`\c 2`) has section breaks: its first paragraph (the field's start, with nothing in it) ends the
    // section before it and its last ends the one of the columns. The new paragraphs take them over from the old.
    let columns = index_spec(instruction).map_or(1, |s| s.columns).min(ibis_doc_model::PageColumns::MAX as u32) as u8;
    let old: Vec<&FlowParagraph> = if replace { doc.body.iter().filter_map(|b| if let Block::Paragraph(p) = b { Some(p) } else { None }).skip_while(|p| p.id != before).collect() } else { Vec::new() };
    let end = old.iter().position(|p| p.field_end).unwrap_or(0);
    let inherited = (old.first().copied(), old.iter().take(end + 1).skip(1).rev().find(|p| p.section.is_some()).copied());
    let lead_id = doc.new_id() + entries.len() as u32;
    if let (Some(first), Some(last)) = inherited
        && first.section.is_some()
        && columns >= 2
    {
        // Made before (by Word or here): the same sections again.
        let mut lead = FlowParagraph { id: lead_id, field_start: entries[0].field_start.take(), section: first.section.clone(), section_break: first.section_break, ..FlowParagraph::default() };
        lead.resolve(&with_styles.styles);
        if let Some(tail) = entries.last_mut() {
            tail.section = last.section.clone();
            tail.section_break = last.section_break;
        }
        entries.insert(0, lead);
    } else if columns >= 2 {
        // A new index in columns: a continuous break before it ends the section it is in; one after the last entry ends the
        // section of the columns (the same page setup, with the columns and no header or footer of its own).
        let section = ibis_doc_model::flow_command::section_of(doc, before);
        if let Some(setup) = ibis_doc_model::flow_command::section_setup(doc, section) {
            let mut before_setup = setup.clone();
            before_setup.parts = Default::default();
            let mut column_setup = before_setup.clone();
            column_setup.page.columns.count = columns;
            column_setup.page.columns.widths = None;
            column_setup.page.columns.space = 720 * 635;
            before_setup.restart_numbering = false;
            column_setup.restart_numbering = false;
            let mut lead = FlowParagraph { id: lead_id, field_start: entries[0].field_start.take(), section: Some(std::sync::Arc::new(before_setup)), section_break: ibis_doc_model::SectionBreak::Continuous, ..FlowParagraph::default() };
            lead.resolve(&with_styles.styles);
            if let Some(tail) = entries.last_mut() {
                tail.section = Some(std::sync::Arc::new(column_setup));
                tail.section_break = ibis_doc_model::SectionBreak::Continuous;
            }
            entries.insert(0, lead);
        }
    }
    commands.push(ibis_doc_model::FlowCommand::SetToc { before, replace, entries, bookmarks: Vec::new() });
    commands
}

/// The styles an index uses (`index 1`…, `index heading`) that the document lacks, as Word defines them.
pub(crate) fn missing_index_styles(styles: &ibis_doc_model::FlowStyles, levels: impl IntoIterator<Item = u32>) -> Vec<(String, ibis_doc_model::FlowStyle)> {
    use ibis_doc_model::{FlowStyle, ParaFormat, StyleKind};
    let normal = styles.default_paragraph.clone();
    let mut out = Vec::new();
    let mut taken: std::collections::HashSet<String> = styles.styles.keys().cloned().collect();
    let mut define = |name: &str, para: ParaFormat, run: RunFormat, next: Option<String>| {
        if styles.paragraph_style_named(name).is_some() {
            return;
        }
        let mut id = name.split_whitespace().map(|w| w.chars().next().map_or(String::new(), |c| c.to_uppercase().collect::<String>() + &w[c.len_utf8()..])).collect::<String>();
        while taken.contains(&id) {
            id.push('_');
        }
        taken.insert(id.clone());
        out.push((id, FlowStyle { kind: StyleKind::Paragraph, name: name.to_string(), based_on: normal.clone(), next: next.or_else(|| normal.clone()), para, run, ..FlowStyle::default() }));
    };
    for level in levels {
        if (1..=9).contains(&level) {
            let step = 210 * 635;
            define(&format!("index {level}"), ParaFormat { left: Some(step * level as i64), first_line: Some(-step), ..ParaFormat::default() }, RunFormat::default(), None);
        }
    }
    define("index heading", ParaFormat::default(), RunFormat { bold: Some(true), ..RunFormat::default() }, styles.paragraph_style_named("index 1").map(str::to_string));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xe_fields() {
        let spec = xe_spec(r#" XE "東京:港区" \b \y "とうきょう" "#).unwrap();
        assert_eq!((spec.levels.as_slice(), spec.bold, spec.italic, spec.yomi.as_deref()), (["東京".to_string(), "港区".to_string()].as_slice(), true, false, Some("とうきょう")));
        let spec = xe_spec(r#"XE "関西" \t "「大阪」を参照""#).unwrap();
        assert_eq!(spec.see.as_deref(), Some("「大阪」を参照"));
        assert_eq!(xe_spec(r#"XE "a\:b""#).unwrap().levels, ["a:b"]);
        assert_eq!(xe_spec("PAGE"), None);
    }

    #[test]
    fn index_switches() {
        let spec = index_spec(r#" INDEX \c 2 \h "A" \e " … " \l "、" \z "1041" \y "#).unwrap();
        assert_eq!((spec.columns, spec.headings.as_deref(), spec.entry_separator.as_str(), spec.page_separator.as_str(), spec.by_reading), (2, Some("A"), " … ", "、", true));
        assert_eq!(index_spec("INDEX").unwrap().entry_separator, ", ");
    }

    #[test]
    fn sorted_as_word_does() {
        let key = |t: &str| sort_key(t);
        // Latin first, the lower case before its capital, then the kana, then kanji in Shift_JIS order.
        let mut words = ["東京", "Banana", "あいうえお", "banana", "Apple", "横浜", "関西", "名古屋"];
        words.sort_by(|a, b| { let (x, y) = (key(a), key(b)); x.0.cmp(&y.0).then(x.1.cmp(&y.1)) });
        assert_eq!(words, ["Apple", "banana", "Banana", "あいうえお", "横浜", "関西", "東京", "名古屋"]);
    }

    #[test]
    fn headings() {
        assert_eq!(heading_of("Apple"), "Ａ");
        assert_eq!(heading_of("きょうと"), "き");
        assert_eq!(heading_of("キョウト"), "き");
        assert_eq!(heading_of("京都"), "漢字");
    }
}
