// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc. and masanork (contributed under the ibisOffice CLA, https://github.com/ibisjp/ibis-office/issues/229)
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Pure-Rust Lawtext / e-Gov 法令標準XML ⇄ Markdown bidirectional converter and
//! 新旧対照表 (side-by-side amendment comparison) generator.
//!
//! Bridges Japanese statutory XML (`<Law>...</Law>`), human-editable Lawtext
//! (inspired by `yamachig/Lawtext`), and ibisOffice's Markdown pipeline so
//! legal drafts can be edited in Markdown/Lawtext and exported to DOCX, PPTX,
//! XLSX, or e-Gov 法令標準XML.

use crate::Error;

pub mod aratame;
pub mod assistant;
pub mod diff;
pub mod lint;
mod print;
pub use print::open_comparison_document;
pub mod refs;
pub use aratame::{
    AratameClause, AratameDraft, VerifyResult, apply_aratame, generate_aratame_draft,
    verify_aratame,
};
pub use assistant::{Legislation5SetPackage, build_5set_package, build_5set_package_with_sources};
pub use diff::{
    AmendmentIr, ArticleChange, ArticleChangeKind, DiffOp, DivisionChange, DivisionChangeKind,
    ItemChange, NodeChangeKind, ParagraphChange, ShinkyuOptions, SubitemChange, SupplChange,
    diff_laws, generate_shinkyu_markdown, generate_shinkyu_markdown_with_options,
};
pub use lint::{LintFinding, LintReport, LintSeverity, lint_amendment, lint_law};
pub use refs::{LawReference, extract_references, generate_sansho_markdown};

/// Structural division kind within `MainProvision` or `SupplProvision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivisionKind {
    Part,
    Chapter,
    Section,
    Subsection,
    Division,
}

impl DivisionKind {
    fn xml_tag(self) -> &'static str {
        match self {
            Self::Part => "Part",
            Self::Chapter => "Chapter",
            Self::Section => "Section",
            Self::Subsection => "Subsection",
            Self::Division => "Division",
        }
    }

    fn title_tag(self) -> &'static str {
        match self {
            Self::Part => "PartTitle",
            Self::Chapter => "ChapterTitle",
            Self::Section => "SectionTitle",
            Self::Subsection => "SubsectionTitle",
            Self::Division => "DivisionTitle",
        }
    }

    fn from_xml_tag(tag: &str) -> Option<Self> {
        match tag {
            "Part" => Some(Self::Part),
            "Chapter" => Some(Self::Chapter),
            "Section" => Some(Self::Section),
            "Subsection" => Some(Self::Subsection),
            "Division" => Some(Self::Division),
            _ => None,
        }
    }

    fn heading_level(self) -> usize {
        match self {
            Self::Part => 2,
            Self::Chapter => 2,
            Self::Section => 3,
            Self::Subsection => 4,
            Self::Division => 5,
        }
    }
}

/// A sub-item (`Subitem1`..`Subitem10`) inside an [`LawItem`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawSubitem {
    pub depth: u8,
    pub num: String,
    pub title: String,
    pub sentence: String,
    pub children: Vec<LawSubitem>,
}

/// An item (`Item`: 一, 二, 三...) inside a [`LawParagraph`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawItem {
    pub num: String,
    pub title: String,
    pub sentence: String,
    pub subitems: Vec<LawSubitem>,
}

/// A paragraph (`Paragraph`: 項) inside a [`LawArticle`] or [`SupplProvision`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawParagraph {
    pub num: String,
    pub sentence: String,
    pub items: Vec<LawItem>,
}

/// An article (`Article`: 条).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawArticle {
    pub num: String,
    pub caption: Option<String>,
    pub title: String,
    pub paragraphs: Vec<LawParagraph>,
}

/// A node inside `MainProvision` or `SupplProvision`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LawNode {
    Division {
        kind: DivisionKind,
        num: String,
        title: String,
        children: Vec<LawNode>,
    },
    Article(LawArticle),
    Paragraph(LawParagraph),
}

/// A supplementary provision (`SupplProvision`: 附則).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplProvision {
    pub amend_law_num: Option<String>,
    pub extract: bool,
    pub label: String,
    pub nodes: Vec<LawNode>,
}

/// An appended table (`AppdxTable`: 別表).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawAppdxTable {
    pub title: String,
    pub related_article_num: Option<String>,
    pub rows: Vec<Vec<String>>,
}

/// Top-level AST for an e-Gov 法令標準XML / Lawtext document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LawDoc {
    pub law_title: String,
    pub law_num: String,
    pub era: Option<String>,
    pub year: Option<String>,
    pub num: Option<String>,
    pub law_type: Option<String>,
    pub lang: Option<String>,
    pub enact_statement: Option<String>,
    pub preamble: Option<String>,
    pub main_provision: Vec<LawNode>,
    pub suppl_provisions: Vec<SupplProvision>,
    pub appdx_tables: Vec<LawAppdxTable>,
}

impl Default for LawDoc {
    fn default() -> Self {
        Self {
            law_title: String::new(),
            law_num: String::new(),
            era: None,
            year: None,
            num: None,
            law_type: None,
            lang: Some("ja".to_string()),
            enact_statement: None,
            preamble: None,
            main_provision: Vec::new(),
            suppl_provisions: Vec::new(),
            appdx_tables: Vec::new(),
        }
    }
}

/// Parse explicit legislative drafting input as e-Gov XML, law Markdown or Lawtext.
pub fn parse_law_input(text: &str) -> Result<LawDoc, Error> {
    let text = text.trim_start_matches('\u{feff}').trim_start();
    let doc = if text.starts_with('<') {
        parse_law_xml(text)?
    } else if text.starts_with("---") && text.contains("type: lawtext") {
        parse_markdown_law(text)?
    } else {
        parse_lawtext(text)?
    };
    if doc.main_provision.is_empty() && doc.suppl_provisions.is_empty() && doc.appdx_tables.is_empty() {
        return Err(Error::new("解析できる本則・附則・別表がありません"));
    }
    Ok(doc)
}

/// Return `true` when `data` looks like an e-Gov 法令標準XML document.
pub fn is_law_xml(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let head = &text[..text.floor_char_boundary(4096)];
    head.contains("<Law") && (head.contains("<LawBody") || head.contains("<LawNum"))
}

/// The upstream Lawtext examples put the law number on its own line.
fn standalone_law_number(line: &str) -> Option<&str> {
    let number = line.strip_prefix('（')?.strip_suffix('）')?.trim();
    (number.ends_with('号') && number.rsplit_once('第').is_some_and(|(prefix, ordinal)| {
        !prefix.is_empty() && ordinal.chars().next().is_some_and(|ch| {
            ch.is_ascii_digit() || "０１２３４５６７８９〇零一二三四五六七八九十百千万".contains(ch)
        })
    })).then_some(number)
}

/// Return `true` when `data` looks like a Lawtext document (`.law.txt` / `.lawtext`)
/// or Markdown with `type: lawtext` frontmatter.
pub fn is_lawtext(data: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(data) else {
        return false;
    };
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.is_empty() {
        return false;
    }
    let mut lines = trimmed.lines().filter(|l| !l.trim().is_empty()).take(30);
    let Some(first) = lines.next() else {
        return false;
    };
    let has_header = (first.contains('（') && first.contains('）'))
        || lines.next().and_then(standalone_law_number).is_some();
    let has_article = trimmed.lines().any(|l| parse_article_header(l.trim()).is_some());
    has_header && has_article
}

/// The file `path` as Lawtext-flavoured Markdown, when it is a law and only then: a `.lawtext` or `.law.txt` file (read as Lawtext), or a `.xml` file whose content is
/// e-Gov 法令標準XML (`<Law` ...). Any other file, whatever its content (a CSV or a text file whose first line has （）and which has a line of 第一条), gives `None` and is
/// never looked at, so it opens as it always did; a `.xml` that is not a law (Office 2003 XML and the others) gives `None` too. An `.md` is Markdown already.
pub fn path_input_to_markdown(path: &std::path::Path, data: &[u8]) -> Result<Option<String>, Error> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    if name.ends_with(".lawtext") || name.ends_with(".law.txt") {
        let text = std::str::from_utf8(data).map_err(|_| Error::new("Lawtext が有効な UTF-8 ではありません"))?;
        return parse_lawtext(text).map(|doc| Some(to_markdown(&doc)));
    }
    if name.ends_with(".xml") && is_law_xml(data) {
        let text = std::str::from_utf8(data).map_err(|_| Error::new("法令XMLが有効な UTF-8 ではありません"))?;
        return parse_law_xml(text).map(|doc| Some(to_markdown(&doc)));
    }
    Ok(None)
}

/// If `data` is e-Gov 法令標準XML or Lawtext, convert it to Lawtext-flavoured Markdown (by its content; a caller that has the file's name uses
/// [`path_input_to_markdown`], which does not take a CSV for Lawtext).
pub fn input_to_markdown(data: &[u8]) -> Result<Option<String>, Error> {
    if is_law_xml(data) {
        let text = std::str::from_utf8(data)
            .map_err(|_| Error::new("法令XMLが有効な UTF-8 ではありません"))?;
        return parse_law_xml(text).map(|doc| Some(to_markdown(&doc)));
    }
    if is_lawtext(data) {
        let text = std::str::from_utf8(data)
            .map_err(|_| Error::new("Lawtext が有効な UTF-8 ではありません"))?;
        return parse_lawtext(text).map(|doc| Some(to_markdown(&doc)));
    }
    Ok(None)
}

/// Export an open [`crate::Presentation`] to e-Gov 法令標準XML via its Markdown representation.
pub fn export_presentation_to_law_xml(p: &crate::Presentation) -> Result<String, Error> {
    let md = crate::markdown::export_presentation_to_markdown(p)?;
    let law = parse_markdown_law(&md)?;
    Ok(to_law_xml(&law))
}

/// Export an open [`crate::Presentation`] to Lawtext plain text via its Markdown representation.
pub fn export_presentation_to_lawtext(p: &crate::Presentation) -> Result<String, Error> {
    let md = crate::markdown::export_presentation_to_markdown(p)?;
    let law = parse_markdown_law(&md)?;
    Ok(to_lawtext(&law))
}

/// Parse an e-Gov 法令標準XML string into a [`LawDoc`].
pub fn parse_law_xml(xml: &str) -> Result<LawDoc, Error> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| Error::new(format!("法令XMLの解析に失敗しました: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "Law" {
        return Err(Error::new("ルート要素が <Law> ではありません"));
    }

    let mut out = LawDoc {
        era: root.attribute("Era").map(str::to_string),
        year: root.attribute("Year").map(str::to_string),
        num: root.attribute("Num").map(str::to_string),
        law_type: root.attribute("LawType").map(str::to_string),
        lang: root.attribute("Lang").map(str::to_string),
        ..LawDoc::default()
    };

    if let Some(law_num) = child_elem(root, "LawNum") {
        out.law_num = collect_text(law_num);
    }

    let body = child_elem(root, "LawBody")
        .ok_or_else(|| Error::new("<LawBody> 要素が見つかりません"))?;
    if let Some(title) = child_elem(body, "LawTitle") {
        out.law_title = collect_text(title);
    }
    if let Some(enact) = child_elem(body, "EnactStatement") {
        let s = collect_text(enact);
        if !s.is_empty() {
            out.enact_statement = Some(s);
        }
    }
    if let Some(pre) = child_elem(body, "Preamble") {
        let s = collect_text(pre);
        if !s.is_empty() {
            out.preamble = Some(s);
        }
    }
    if let Some(main) = child_elem(body, "MainProvision") {
        out.main_provision = parse_xml_nodes(main);
    }
    for suppl in body.children().filter(|n| n.is_element() && n.tag_name().name() == "SupplProvision") {
        let label = child_elem(suppl, "SupplProvisionLabel")
            .map(collect_text)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "附則".to_string());
        out.suppl_provisions.push(SupplProvision {
            amend_law_num: suppl.attribute("AmendLawNum").map(str::to_string),
            extract: suppl.attribute("Extract").is_some_and(|v| v == "true"),
            label,
            nodes: parse_xml_nodes(suppl),
        });
    }
    for appdx in body.children().filter(|n| n.is_element() && n.tag_name().name() == "AppdxTable") {
        let title = child_elem(appdx, "AppdxTableTitle")
            .map(collect_text)
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "別表".to_string());
        let related_article_num = child_elem(appdx, "RelatedArticleNum")
            .map(collect_text)
            .filter(|s| !s.is_empty());
        let mut rows = Vec::new();
        if let Some(ts) = child_elem(appdx, "TableStruct") {
            if let Some(tbl) = child_elem(ts, "Table") {
                for tr in tbl.children().filter(|n| n.is_element() && n.tag_name().name() == "TableRow") {
                    let row: Vec<String> = tr
                        .children()
                        .filter(|n| n.is_element() && n.tag_name().name() == "TableColumn")
                        .map(collect_text)
                        .collect();
                    if !row.is_empty() {
                        rows.push(row);
                    }
                }
            }
        }
        out.appdx_tables.push(LawAppdxTable {
            title,
            related_article_num,
            rows,
        });
    }
    Ok(out)
}

fn child_elem<'a, 'input>(
    parent: roxmltree::Node<'a, 'input>,
    name: &str,
) -> Option<roxmltree::Node<'a, 'input>> {
    parent
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == name)
}

fn collect_text(node: roxmltree::Node<'_, '_>) -> String {
    let mut out = String::new();
    collect_text_into(node, &mut out);
    out.trim().to_string()
}

fn collect_text_into(node: roxmltree::Node<'_, '_>, out: &mut String) {
    if node.is_text() {
        if let Some(t) = node.text().filter(|t| !(t.contains('\n') && t.trim().is_empty())) {
            out.push_str(t);
        }
        return;
    }
    if !node.is_element() {
        return;
    }
    // Skip Ruby annotations <Rt> when flattening plain text so reading stays clean.
    let tag = node.tag_name().name();
    if tag == "Rt" || tag == "Rp" {
        return;
    }
    if tag == "Column" && !out.is_empty() && !out.ends_with(char::is_whitespace) {
        out.push(' ');
    }
    for child in node.children() {
        collect_text_into(child, out);
    }
}

fn collect_sentences(container: roxmltree::Node<'_, '_>) -> String {
    let mut parts = Vec::new();
    for child in container.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "Sentence" {
            let s = collect_text(child);
            if !s.is_empty() {
                parts.push(s);
            }
        }
    }
    if parts.is_empty() {
        collect_text(container)
    } else {
        parts.join("")
    }
}

fn parse_xml_nodes(parent: roxmltree::Node<'_, '_>) -> Vec<LawNode> {
    let mut nodes = Vec::new();
    for child in parent.children().filter(|n| n.is_element()) {
        let tag = child.tag_name().name();
        if let Some(kind) = DivisionKind::from_xml_tag(tag) {
            let num = child.attribute("Num").unwrap_or("").to_string();
            let title = child_elem(child, kind.title_tag())
                .map(collect_text)
                .unwrap_or_default();
            let children = parse_xml_nodes(child);
            nodes.push(LawNode::Division {
                kind,
                num,
                title,
                children,
            });
        } else if tag == "Article" {
            nodes.push(LawNode::Article(parse_xml_article(child)));
        } else if tag == "Paragraph" {
            nodes.push(LawNode::Paragraph(parse_xml_paragraph(child)));
        }
    }
    nodes
}

fn parse_xml_article(node: roxmltree::Node<'_, '_>) -> LawArticle {
    let num = node.attribute("Num").unwrap_or("").to_string();
    let caption = child_elem(node, "ArticleCaption")
        .map(collect_text)
        .filter(|s| !s.is_empty());
    let title = child_elem(node, "ArticleTitle")
        .map(collect_text)
        .unwrap_or_default();
    let paragraphs = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "Paragraph")
        .map(parse_xml_paragraph)
        .collect();
    LawArticle {
        num,
        caption,
        title,
        paragraphs,
    }
}

fn parse_xml_paragraph(node: roxmltree::Node<'_, '_>) -> LawParagraph {
    let num = node.attribute("Num").unwrap_or("1").to_string();
    let sentence = child_elem(node, "ParagraphSentence")
        .map(collect_sentences)
        .unwrap_or_default();
    let items = node
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "Item")
        .map(parse_xml_item)
        .collect();
    LawParagraph {
        num,
        sentence,
        items,
    }
}

fn parse_xml_item(node: roxmltree::Node<'_, '_>) -> LawItem {
    let num = node.attribute("Num").unwrap_or("1").to_string();
    let title = child_elem(node, "ItemTitle")
        .map(collect_text)
        .unwrap_or_default();
    let sentence = child_elem(node, "ItemSentence")
        .map(collect_sentences)
        .unwrap_or_default();
    let subitems = parse_xml_subitems(node, 1);
    LawItem {
        num,
        title,
        sentence,
        subitems,
    }
}

fn parse_xml_subitems(parent: roxmltree::Node<'_, '_>, depth: u8) -> Vec<LawSubitem> {
    if depth > 10 {
        return Vec::new();
    }
    let tag = format!("Subitem{depth}");
    let title_tag = format!("Subitem{depth}Title");
    let sent_tag = format!("Subitem{depth}Sentence");
    parent
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == tag)
        .map(|sub| LawSubitem {
            depth,
            num: sub.attribute("Num").unwrap_or("1").to_string(),
            title: child_elem(sub, &title_tag).map(collect_text).unwrap_or_default(),
            sentence: child_elem(sub, &sent_tag)
                .map(collect_sentences)
                .unwrap_or_default(),
            children: parse_xml_subitems(sub, depth + 1),
        })
        .collect()
}

/// Serialize a [`LawDoc`] into an e-Gov 法令標準XML string.
pub fn to_law_xml(doc: &LawDoc) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Law");
    push_opt_attr(&mut out, "Era", doc.era.as_deref());
    push_opt_attr(&mut out, "Lang", doc.lang.as_deref());
    push_opt_attr(&mut out, "LawType", doc.law_type.as_deref());
    push_opt_attr(&mut out, "Num", doc.num.as_deref());
    push_opt_attr(&mut out, "Year", doc.year.as_deref());
    out.push_str(">\n");
    if !doc.law_num.is_empty() {
        out.push_str("  <LawNum>");
        push_xml_escaped(&mut out, &doc.law_num);
        out.push_str("</LawNum>\n");
    }
    out.push_str("  <LawBody>\n");
    if !doc.law_title.is_empty() {
        out.push_str("    <LawTitle>");
        push_xml_escaped(&mut out, &doc.law_title);
        out.push_str("</LawTitle>\n");
    }
    if let Some(enact) = doc.enact_statement.as_deref().filter(|s| !s.is_empty()) {
        out.push_str("    <EnactStatement>");
        push_xml_escaped(&mut out, enact);
        out.push_str("</EnactStatement>\n");
    }
    if let Some(pre) = doc.preamble.as_deref().filter(|s| !s.is_empty()) {
        out.push_str("    <Preamble><Paragraph Num=\"1\"><ParagraphNum/><ParagraphSentence><Sentence>");
        push_xml_escaped(&mut out, pre);
        out.push_str("</Sentence></ParagraphSentence></Paragraph></Preamble>\n");
    }
    out.push_str("    <MainProvision>\n");
    for node in &doc.main_provision {
        write_xml_node(&mut out, node, 3);
    }
    out.push_str("    </MainProvision>\n");
    for suppl in &doc.suppl_provisions {
        out.push_str("    <SupplProvision");
        push_opt_attr(&mut out, "AmendLawNum", suppl.amend_law_num.as_deref());
        if suppl.extract {
            out.push_str(" Extract=\"true\"");
        }
        out.push_str(">\n");
        out.push_str("      <SupplProvisionLabel>");
        push_xml_escaped(&mut out, &suppl.label);
        out.push_str("</SupplProvisionLabel>\n");
        for node in &suppl.nodes {
            write_xml_node(&mut out, node, 3);
        }
        out.push_str("    </SupplProvision>\n");
    }
    for appdx in &doc.appdx_tables {
        out.push_str("    <AppdxTable>\n      <AppdxTableTitle>");
        push_xml_escaped(&mut out, &appdx.title);
        out.push_str("</AppdxTableTitle>\n");
        if let Some(rel) = appdx.related_article_num.as_deref() {
            out.push_str("      <RelatedArticleNum>");
            push_xml_escaped(&mut out, rel);
            out.push_str("</RelatedArticleNum>\n");
        }
        if !appdx.rows.is_empty() {
            out.push_str("      <TableStruct>\n        <Table>\n");
            for row in &appdx.rows {
                out.push_str("          <TableRow>\n");
                for cell in row {
                    out.push_str("            <TableColumn><Sentence>");
                    push_xml_escaped(&mut out, cell);
                    out.push_str("</Sentence></TableColumn>\n");
                }
                out.push_str("          </TableRow>\n");
            }
            out.push_str("        </Table>\n      </TableStruct>\n");
        }
        out.push_str("    </AppdxTable>\n");
    }
    out.push_str("  </LawBody>\n</Law>\n");
    out
}

fn push_opt_attr(out: &mut String, name: &str, val: Option<&str>) {
    if let Some(v) = val.filter(|s| !s.is_empty()) {
        out.push(' ');
        out.push_str(name);
        out.push_str("=\"");
        push_xml_escaped(out, v);
        out.push('"');
    }
}

fn push_xml_escaped(out: &mut String, text: &str) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_xml_node(out: &mut String, node: &LawNode, depth: usize) {
    match node {
        LawNode::Division {
            kind,
            num,
            title,
            children,
        } => {
            push_indent(out, depth);
            out.push('<');
            out.push_str(kind.xml_tag());
            if !num.is_empty() {
                push_opt_attr(out, "Num", Some(num));
            }
            out.push_str(">\n");
            push_indent(out, depth + 1);
            out.push('<');
            out.push_str(kind.title_tag());
            out.push('>');
            push_xml_escaped(out, title);
            out.push_str("</");
            out.push_str(kind.title_tag());
            out.push_str(">\n");
            for child in children {
                write_xml_node(out, child, depth + 1);
            }
            push_indent(out, depth);
            out.push_str("</");
            out.push_str(kind.xml_tag());
            out.push_str(">\n");
        }
        LawNode::Article(art) => {
            push_indent(out, depth);
            out.push_str("<Article");
            push_opt_attr(out, "Num", Some(&art.num));
            out.push_str(">\n");
            if let Some(cap) = art.caption.as_deref().filter(|s| !s.is_empty()) {
                push_indent(out, depth + 1);
                out.push_str("<ArticleCaption>");
                push_xml_escaped(out, cap);
                out.push_str("</ArticleCaption>\n");
            }
            if !art.title.is_empty() {
                push_indent(out, depth + 1);
                out.push_str("<ArticleTitle>");
                push_xml_escaped(out, &art.title);
                out.push_str("</ArticleTitle>\n");
            }
            for para in &art.paragraphs {
                write_xml_paragraph(out, para, depth + 1);
            }
            push_indent(out, depth);
            out.push_str("</Article>\n");
        }
        LawNode::Paragraph(para) => write_xml_paragraph(out, para, depth),
    }
}

fn write_xml_paragraph(out: &mut String, para: &LawParagraph, depth: usize) {
    push_indent(out, depth);
    out.push_str("<Paragraph");
    push_opt_attr(out, "Num", Some(&para.num));
    out.push_str(">\n");
    push_indent(out, depth + 1);
    out.push_str("<ParagraphNum>");
    if para.num != "1" {
        push_xml_escaped(out, &to_fullwidth_digits(&para.num));
    }
    out.push_str("</ParagraphNum>\n");
    push_indent(out, depth + 1);
    out.push_str("<ParagraphSentence><Sentence>");
    push_xml_escaped(out, &para.sentence);
    out.push_str("</Sentence></ParagraphSentence>\n");
    for item in &para.items {
        push_indent(out, depth + 1);
        out.push_str("<Item");
        push_opt_attr(out, "Num", Some(&item.num));
        out.push_str(">\n");
        push_indent(out, depth + 2);
        out.push_str("<ItemTitle>");
        push_xml_escaped(out, &item.title);
        out.push_str("</ItemTitle>\n");
        push_indent(out, depth + 2);
        out.push_str("<ItemSentence><Sentence>");
        push_xml_escaped(out, &item.sentence);
        out.push_str("</Sentence></ItemSentence>\n");
        for sub in &item.subitems {
            write_xml_subitem(out, sub, depth + 2);
        }
        push_indent(out, depth + 1);
        out.push_str("</Item>\n");
    }
    push_indent(out, depth);
    out.push_str("</Paragraph>\n");
}

fn write_xml_subitem(out: &mut String, sub: &LawSubitem, depth: usize) {
    let tag = format!("Subitem{}", sub.depth);
    let title_tag = format!("Subitem{}Title", sub.depth);
    let sent_tag = format!("Subitem{}Sentence", sub.depth);
    push_indent(out, depth);
    out.push('<');
    out.push_str(&tag);
    push_opt_attr(out, "Num", Some(&sub.num));
    out.push_str(">\n");
    push_indent(out, depth + 1);
    out.push('<');
    out.push_str(&title_tag);
    out.push('>');
    push_xml_escaped(out, &sub.title);
    out.push_str("</");
    out.push_str(&title_tag);
    out.push_str(">\n");
    push_indent(out, depth + 1);
    out.push('<');
    out.push_str(&sent_tag);
    out.push_str("><Sentence>");
    push_xml_escaped(out, &sub.sentence);
    out.push_str("</Sentence></");
    out.push_str(&sent_tag);
    out.push_str(">\n");
    for child in &sub.children {
        write_xml_subitem(out, child, depth + 1);
    }
    push_indent(out, depth);
    out.push_str("</");
    out.push_str(&tag);
    out.push_str(">\n");
}

/// Export a [`LawDoc`] to human-readable Lawtext plain text.
pub fn to_lawtext(doc: &LawDoc) -> String {
    let mut out = String::new();
    if !doc.law_title.is_empty() {
        out.push_str(&doc.law_title);
        if !doc.law_num.is_empty() {
            out.push('（');
            out.push_str(&doc.law_num);
            out.push('）');
        }
        out.push_str("\n\n");
    }
    for node in &doc.main_provision {
        write_lawtext_node(&mut out, node);
    }
    for suppl in &doc.suppl_provisions {
        out.push_str(&suppl.label);
        if let Some(amend) = suppl.amend_law_num.as_deref().filter(|s| !s.is_empty()) {
            out.push('（');
            out.push_str(amend);
            out.push('）');
        }
        out.push_str("\n\n");
        for node in &suppl.nodes {
            write_lawtext_node(&mut out, node);
        }
    }
    for appdx in &doc.appdx_tables {
        out.push_str(&appdx.title);
        if let Some(rel) = appdx.related_article_num.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!("（{rel}）"));
        }
        out.push_str("\n\n");
        for row in &appdx.rows {
            out.push_str("| ");
            out.push_str(&row.join(" | "));
            out.push_str(" |\n");
        }
        if !appdx.rows.is_empty() {
            out.push('\n');
        }
    }
    out.trim_end().to_string() + "\n"
}

fn write_lawtext_node(out: &mut String, node: &LawNode) {
    match node {
        LawNode::Division { title, children, .. } => {
            if !title.is_empty() {
                out.push_str(title);
                out.push_str("\n\n");
            }
            for child in children {
                write_lawtext_node(out, child);
            }
        }
        LawNode::Article(art) => {
            if let Some(cap) = art.caption.as_deref().filter(|s| !s.is_empty()) {
                out.push_str(cap);
                out.push('\n');
            }
            for (idx, para) in art.paragraphs.iter().enumerate() {
                if idx == 0 {
                    out.push_str(&art.title);
                    if !para.sentence.is_empty() {
                        out.push(' ');
                        out.push_str(&para.sentence);
                    }
                    out.push('\n');
                } else {
                    out.push_str(&to_fullwidth_digits(&para.num));
                    if !para.sentence.is_empty() {
                        out.push(' ');
                        out.push_str(&para.sentence);
                    }
                    out.push('\n');
                }
                write_lawtext_items(out, &para.items);
            }
            out.push('\n');
        }
        LawNode::Paragraph(para) => {
            if para.num != "1" {
                out.push_str(&to_fullwidth_digits(&para.num));
                out.push(' ');
            }
            out.push_str(&para.sentence);
            out.push('\n');
            write_lawtext_items(out, &para.items);
            out.push('\n');
        }
    }
}

fn write_lawtext_items(out: &mut String, items: &[LawItem]) {
    for item in items {
        out.push_str("  ");
        out.push_str(&item.title);
        if !item.sentence.is_empty() {
            out.push(' ');
            out.push_str(&item.sentence);
        }
        out.push('\n');
        for sub in &item.subitems {
            write_lawtext_subitem(out, sub);
        }
    }
}

fn write_lawtext_subitem(out: &mut String, sub: &LawSubitem) {
    for _ in 0..=sub.depth {
        out.push_str("  ");
    }
    out.push_str(&sub.title);
    if !sub.sentence.is_empty() {
        out.push(' ');
        out.push_str(&sub.sentence);
    }
    out.push('\n');
    for child in &sub.children {
        write_lawtext_subitem(out, child);
    }
}

/// Parse Lawtext plain text into a [`LawDoc`].
pub fn parse_lawtext(text: &str) -> Result<LawDoc, Error> {
    let mut doc = LawDoc::default();
    let mut in_suppl = false;
    let mut in_appdx = false;
    let mut pending_caption: Option<String> = None;
    let mut cur_article: Option<LawArticle> = None;
    let mut div_stack: Vec<LawNode> = Vec::new();

    let flush_article = |cur_article: &mut Option<LawArticle>,
                         div_stack: &mut Vec<LawNode>,
                         doc: &mut LawDoc,
                         in_suppl: bool| {
        if let Some(art) = cur_article.take() {
            push_node_to_target(LawNode::Article(art), div_stack, doc, in_suppl);
        }
    };

    let mut lines = text.trim_start_matches('\u{feff}').lines()
        .filter(|line| !line.trim().is_empty());
    let mut header = lines.clone();
    if let (Some(title), Some(number)) = (
        header.next().map(str::trim).filter(|title| !title.contains('（') && !title.starts_with('第')),
        header.next().map(str::trim).and_then(standalone_law_number),
    ) {
        doc.law_title = title.to_string();
        doc.law_num = number.to_string();
        lines = header;
    }

    for raw_line in lines {
        let trimmed = raw_line.trim();
        if doc.law_title.is_empty() && cur_article.is_none() && div_stack.is_empty() {
            if let Some((title, law_num)) = split_title_and_parens(trimmed) {
                if !is_caption_line(trimmed) && !trimmed.starts_with('第') {
                    doc.law_title = title;
                    doc.law_num = law_num;
                    continue;
                }
            }
        }
        if trimmed.starts_with("別表") && !trimmed.contains('。') {
            flush_article(&mut cur_article, &mut div_stack, &mut doc, in_suppl);
            flush_all_divisions(&mut div_stack, &mut doc, in_suppl);
            in_appdx = true;
            let (title, rel) = split_title_and_parens(trimmed)
                .map(|(t, r)| (t, Some(r)))
                .unwrap_or_else(|| (trimmed.to_string(), None));
            doc.appdx_tables.push(LawAppdxTable {
                title,
                related_article_num: rel,
                rows: Vec::new(),
            });
            continue;
        }
        if in_appdx && trimmed.starts_with('|') && trimmed.ends_with('|') {
            let cells: Vec<String> = trimmed
                .trim_matches('|')
                .split('|')
                .map(|c| c.trim().replace("\\|", "|"))
                .collect();
            let is_sep = !cells.is_empty()
                && cells
                    .iter()
                    .all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':' | ' ')));
            if !is_sep && !cells.is_empty() {
                if let Some(appdx) = doc.appdx_tables.last_mut() {
                    appdx.rows.push(cells);
                }
            }
            continue;
        }
        if let Some(suppl_label) = parse_suppl_header(trimmed) {
            in_appdx = false;
            flush_article(&mut cur_article, &mut div_stack, &mut doc, in_suppl);
            flush_all_divisions(&mut div_stack, &mut doc, in_suppl);
            in_suppl = true;
            let (label, amend) = split_title_and_parens(&suppl_label)
                .map(|(l, a)| (l, Some(a)))
                .unwrap_or((suppl_label, None));
            doc.suppl_provisions.push(SupplProvision {
                amend_law_num: amend,
                extract: false,
                label,
                nodes: Vec::new(),
            });
            continue;
        }
        if let Some((kind, num)) = parse_division_header(trimmed) {
            flush_article(&mut cur_article, &mut div_stack, &mut doc, in_suppl);
            while let Some(LawNode::Division { kind: top_kind, .. }) = div_stack.last() {
                if *top_kind as u8 >= kind as u8 {
                    let popped = div_stack.pop().unwrap();
                    push_node_to_target(popped, &mut div_stack, &mut doc, in_suppl);
                } else {
                    break;
                }
            }
            div_stack.push(LawNode::Division {
                kind,
                num,
                title: trimmed.to_string(),
                children: Vec::new(),
            });
            continue;
        }
        if is_caption_line(trimmed) && cur_article.as_ref().is_none_or(|a| !a.paragraphs.is_empty())
        {
            flush_article(&mut cur_article, &mut div_stack, &mut doc, in_suppl);
            pending_caption = Some(trimmed.to_string());
            continue;
        }
        if let Some((art_title, art_num, rest)) = parse_article_header(trimmed) {
            flush_article(&mut cur_article, &mut div_stack, &mut doc, in_suppl);
            let mut paragraphs = Vec::new();
            if !rest.is_empty() {
                paragraphs.push(LawParagraph {
                    num: "1".to_string(),
                    sentence: rest.to_string(),
                    items: Vec::new(),
                });
            }
            cur_article = Some(LawArticle {
                num: art_num,
                caption: pending_caption.take(),
                title: art_title,
                paragraphs,
            });
            continue;
        }
        if let Some(art) = cur_article.as_mut() {
            if let Some((p_num, rest)) = parse_numbered_paragraph(trimmed) {
                art.paragraphs.push(LawParagraph {
                    num: p_num,
                    sentence: rest.to_string(),
                    items: Vec::new(),
                });
                continue;
            }
            let indent = raw_line.chars().take_while(|c| *c == ' ' || *c == '\t').count();
            if let Some(para) = art.paragraphs.last_mut() {
                if indent >= 4 {
                    if let Some(item) = para.items.last_mut() {
                        if let Some((sub_title, sub_sent)) = split_leading_token(trimmed) {
                            let idx = (item.subitems.len() + 1).to_string();
                            item.subitems.push(LawSubitem {
                                depth: 1,
                                num: idx,
                                title: sub_title.to_string(),
                                sentence: sub_sent.to_string(),
                                children: Vec::new(),
                            });
                            continue;
                        }
                    }
                }
                if indent >= 2 {
                    if let Some((it_title, it_sent)) = split_leading_token(trimmed) {
                        let idx = (para.items.len() + 1).to_string();
                        para.items.push(LawItem {
                            num: idx,
                            title: it_title.to_string(),
                            sentence: it_sent.to_string(),
                            subitems: Vec::new(),
                        });
                        continue;
                    }
                }
                if !para.sentence.is_empty() {
                    para.sentence.push_str(trimmed);
                } else {
                    para.sentence = trimmed.to_string();
                }
            } else {
                art.paragraphs.push(LawParagraph {
                    num: "1".to_string(),
                    sentence: trimmed.to_string(),
                    items: Vec::new(),
                });
            }
        } else if in_suppl {
            let (p_num, rest) = parse_numbered_paragraph(trimmed)
                .unwrap_or_else(|| ("1".to_string(), trimmed));
            push_node_to_target(
                LawNode::Paragraph(LawParagraph {
                    num: p_num,
                    sentence: rest.to_string(),
                    items: Vec::new(),
                }),
                &mut div_stack,
                &mut doc,
                true,
            );
        }
    }

    flush_article(&mut cur_article, &mut div_stack, &mut doc, in_suppl);
    flush_all_divisions(&mut div_stack, &mut doc, in_suppl);
    Ok(doc)
}

fn push_node_to_target(
    node: LawNode,
    div_stack: &mut [LawNode],
    doc: &mut LawDoc,
    in_suppl: bool,
) {
    if let Some(LawNode::Division { children, .. }) = div_stack.last_mut() {
        children.push(node);
    } else if in_suppl {
        if let Some(suppl) = doc.suppl_provisions.last_mut() {
            suppl.nodes.push(node);
        }
    } else {
        doc.main_provision.push(node);
    }
}

fn flush_all_divisions(div_stack: &mut Vec<LawNode>, doc: &mut LawDoc, in_suppl: bool) {
    while let Some(popped) = div_stack.pop() {
        push_node_to_target(popped, div_stack, doc, in_suppl);
    }
}

/// Export a [`LawDoc`] to Markdown with YAML frontmatter so it can be edited
/// in ibisOffice (DOCX / PPTX) and converted back to Lawtext or e-Gov 法令標準XML.
pub fn to_markdown(doc: &LawDoc) -> String {
    let mut out = String::from("---\ntype: lawtext\n");
    if !doc.law_title.is_empty() {
        out.push_str(&format!("law_title: {}\n", doc.law_title));
    }
    if !doc.law_num.is_empty() {
        out.push_str(&format!("law_num: {}\n", doc.law_num));
    }
    if let Some(era) = doc.era.as_deref() {
        out.push_str(&format!("era: {era}\n"));
    }
    if let Some(year) = doc.year.as_deref() {
        out.push_str(&format!("year: {year}\n"));
    }
    if let Some(num) = doc.num.as_deref() {
        out.push_str(&format!("num: {num}\n"));
    }
    if let Some(lt) = doc.law_type.as_deref() {
        out.push_str(&format!("law_type: {lt}\n"));
    }
    out.push_str("---\n\n");

    if !doc.law_title.is_empty() {
        out.push_str(&format!("# {}\n\n", doc.law_title));
    }
    if !doc.law_num.is_empty() {
        out.push_str(&format!("> 法令番号: {}\n\n", doc.law_num));
    }
    if let Some(preamble) = doc.preamble.as_deref().filter(|text| !text.is_empty()) {
        out.push_str(preamble);
        out.push_str("\n\n");
    }
    for node in &doc.main_provision {
        write_markdown_node(&mut out, node, 2);
    }
    for suppl in &doc.suppl_provisions {
        if let Some(amend) = suppl.amend_law_num.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!("## {}（{}）\n\n", suppl.label, amend));
        } else {
            out.push_str(&format!("## {}\n\n", suppl.label));
        }
        for node in &suppl.nodes {
            write_markdown_node(&mut out, node, 3);
        }
    }
    for appdx in &doc.appdx_tables {
        if let Some(rel) = appdx.related_article_num.as_deref() {
            out.push_str(&format!("## {}（{}）\n\n", appdx.title, rel));
        } else {
            out.push_str(&format!("## {}\n\n", appdx.title));
        }
        if !appdx.rows.is_empty() {
            let cols = appdx.rows.iter().map(|r| r.len()).max().unwrap_or(1);
            for (r_idx, row) in appdx.rows.iter().enumerate() {
                out.push('|');
                for c in 0..cols {
                    let cell = row.get(c).map(|s| s.replace('|', "\\|")).unwrap_or_default();
                    out.push_str(&format!(" {cell} |"));
                }
                out.push('\n');
                if r_idx == 0 {
                    out.push('|');
                    for _ in 0..cols {
                        out.push_str(" :--- |");
                    }
                    out.push('\n');
                }
            }
            out.push('\n');
        }
    }
    out.trim_end().to_string() + "\n"
}

fn write_markdown_node(out: &mut String, node: &LawNode, heading_level: usize) {
    match node {
        LawNode::Division {
            kind,
            title,
            children,
            ..
        } => {
            let lvl = kind.heading_level().max(heading_level).min(6);
            out.push_str(&"#".repeat(lvl));
            out.push(' ');
            out.push_str(title);
            out.push_str("\n\n");
            for child in children {
                write_markdown_node(out, child, (lvl + 1).min(6));
            }
        }
        LawNode::Article(art) => {
            let lvl = heading_level.min(6);
            out.push_str(&"#".repeat(lvl));
            out.push(' ');
            out.push_str(&art.title);
            if let Some(cap) = art.caption.as_deref().filter(|s| !s.is_empty()) {
                out.push(' ');
                out.push_str(cap);
            }
            out.push_str("\n\n");
            for (idx, para) in art.paragraphs.iter().enumerate() {
                if idx == 0 {
                    out.push_str(&para.sentence);
                    out.push_str("\n\n");
                } else {
                    out.push_str(&format!(
                        "{}. {}\n\n",
                        para.num, para.sentence
                    ));
                }
                for item in &para.items {
                    out.push_str(&format!("- {} {}\n", item.title, item.sentence));
                    for sub in &item.subitems {
                        write_markdown_subitem(out, sub);
                    }
                }
                if !para.items.is_empty() {
                    out.push('\n');
                }
            }
        }
        LawNode::Paragraph(para) => {
            if para.num == "1" {
                out.push_str(&para.sentence);
            } else {
                out.push_str(&format!("{}. {}", para.num, para.sentence));
            }
            out.push_str("\n\n");
        }
    }
}

fn write_markdown_subitem(out: &mut String, sub: &LawSubitem) {
    for _ in 0..sub.depth {
        out.push_str("  ");
    }
    out.push_str(&format!("- {} {}\n", sub.title, sub.sentence));
    for child in &sub.children {
        write_markdown_subitem(out, child);
    }
}

/// Parse a Lawtext-flavoured Markdown document back into a [`LawDoc`].
pub fn parse_markdown_law(md: &str) -> Result<LawDoc, Error> {
    let mut doc = LawDoc::default();
    let mut body = md.trim_start_matches('\u{feff}');
    if let Some(after_open) = body.strip_prefix("---\n").or_else(|| body.strip_prefix("---\r\n")) {
        if let Some(end_idx) = after_open.find("\n---") {
            let fm = &after_open[..end_idx];
            body = &after_open[end_idx + 4..];
            for line in fm.lines() {
                if let Some((k, v)) = line.split_once(':') {
                    let v = v.trim().to_string();
                    match k.trim() {
                        "law_title" => doc.law_title = v,
                        "law_num" => doc.law_num = v,
                        "era" => doc.era = Some(v),
                        "year" => doc.year = Some(v),
                        "num" => doc.num = Some(v),
                        "law_type" => doc.law_type = Some(v),
                        "lang" => doc.lang = Some(v),
                        _ => {}
                    }
                }
            }
        }
    }

    // An editable DOCX keeps the visible heading and number when exporting
    // back to Markdown, even if the law-specific front matter is absent.
    if let Some(number) = body.lines().find_map(|line| line.trim().strip_prefix("> 法令番号:")) {
        if doc.law_num.is_empty() {
            doc.law_num = number.trim().to_owned();
        }
        if doc.law_title.is_empty() {
            if let Some(title) = body.lines().find_map(|line| line.trim().strip_prefix("# ")) {
                doc.law_title = title.trim().to_owned();
            }
        }
    }

    // Reconstruct Lawtext lines from Markdown headings/lists and delegate to `parse_lawtext`
    // so both formats share one structural parser.
    let mut lawtext_lines = Vec::new();
    if !doc.law_title.is_empty() {
        if !doc.law_num.is_empty() {
            lawtext_lines.push(format!("{}（{}）", doc.law_title, doc.law_num));
        } else {
            lawtext_lines.push(doc.law_title.clone());
        }
    }
    let mut seen_first_article = false;
    for raw in body.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with("> 法令番号:") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let heading = rest.trim_start_matches('#').trim();
            if heading == doc.law_title && !seen_first_article {
                continue;
            }
            if let Some((art_title, caption)) = split_article_title_and_caption(heading) {
                seen_first_article = true;
                if let Some(cap) = caption {
                    lawtext_lines.push(cap);
                }
                lawtext_lines.push(art_title);
                continue;
            }
            lawtext_lines.push(heading.to_string());
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("- ") {
            let indent = raw.chars().take_while(|c| *c == ' ').count();
            if indent >= 2 {
                lawtext_lines.push(format!("    {rest}"));
            } else {
                lawtext_lines.push(format!("  {rest}"));
            }
            continue;
        }
        if let Some((p_num, rest)) = parse_md_ordered_paragraph(trimmed) {
            lawtext_lines.push(format!("{} {}", to_fullwidth_digits(&p_num), rest));
            continue;
        }
        // Attach first body paragraph onto the pending `第N条` line if it has no sentence yet.
        if let Some(last) = lawtext_lines.last_mut() {
            if parse_article_header(last.trim()).is_some_and(|(_, _, r)| r.is_empty()) {
                last.push(' ');
                last.push_str(trimmed);
                continue;
            }
        }
        lawtext_lines.push(trimmed.to_string());
    }

    let mut parsed = parse_lawtext(&lawtext_lines.join("\n"))?;
    if doc.law_title.is_empty() {
        doc.law_title = parsed.law_title;
    }
    if doc.law_num.is_empty() {
        doc.law_num = parsed.law_num;
    }
    doc.main_provision = std::mem::take(&mut parsed.main_provision);
    doc.suppl_provisions = std::mem::take(&mut parsed.suppl_provisions);
    doc.appdx_tables = std::mem::take(&mut parsed.appdx_tables);
    Ok(doc)
}

fn split_title_and_parens(s: &str) -> Option<(String, String)> {
    let open = s.rfind('（')?;
    let close = s.rfind('）')?;
    if close > open && close == s.len() - '）'.len_utf8() {
        Some((s[..open].trim().to_string(), s[open + '（'.len_utf8()..close].trim().to_string()))
    } else {
        None
    }
}

fn is_caption_line(s: &str) -> bool {
    s.starts_with('（') && s.ends_with('）')
}

fn split_article_title_and_caption(s: &str) -> Option<(String, Option<String>)> {
    if let Some((art, _, rest)) = parse_article_header(s) {
        if rest.is_empty() {
            return Some((art, None));
        }
        if is_caption_line(rest) {
            return Some((art, Some(rest.to_string())));
        }
    }
    None
}

fn parse_suppl_header(s: &str) -> Option<String> {
    if s == "附則" || s.starts_with("附則（") || s == "附 則" {
        Some(s.to_string())
    } else {
        None
    }
}

fn parse_division_header(s: &str) -> Option<(DivisionKind, String)> {
    let rest = s.strip_prefix('第')?;
    let (num_part, suffix) = rest.split_once(|c| matches!(c, '編' | '章' | '節' | '款' | '目'))?;
    let marker = rest.chars().nth(num_part.chars().count())?;
    let kind = match marker {
        '編' => DivisionKind::Part,
        '章' => DivisionKind::Chapter,
        '節' => DivisionKind::Section,
        '款' => DivisionKind::Subsection,
        '目' => DivisionKind::Division,
        _ => return None,
    };
    let num = kanji_num_to_ascii(num_part)?;
    let _ = suffix;
    Some((kind, num))
}

fn parse_article_header(s: &str) -> Option<(String, String, &str)> {
    let rest = s.strip_prefix('第')?;
    let jo_idx = rest.find('条')?;
    let main_kanji = &rest[..jo_idx];
    let main_num = kanji_num_to_ascii(main_kanji)?;
    let mut after_jo = &rest[jo_idx + '条'.len_utf8()..];
    let mut num = main_num;
    let mut title_end = '第'.len_utf8() + jo_idx + '条'.len_utf8();
    while let Some(branch_rest) = after_jo.strip_prefix('の') {
        let len = branch_rest
            .chars()
            .take_while(|c| is_kanji_digit(*c) || fullwidth_or_ascii_digit(*c).is_some())
            .map(char::len_utf8)
            .sum::<usize>();
        if len == 0 {
            break;
        }
        let branch_num = kanji_num_to_ascii(&branch_rest[..len])?;
        num.push('_');
        num.push_str(&branch_num);
        title_end += 'の'.len_utf8() + len;
        after_jo = &branch_rest[len..];
    }
    if !after_jo.is_empty() && !after_jo.starts_with(|c: char| c.is_whitespace() || c == '（') {
        return None;
    }
    Some((s[..title_end].to_string(), num, after_jo.trim_start()))
}

fn parse_numbered_paragraph(s: &str) -> Option<(String, &str)> {
    let mut chars = s.char_indices();
    let mut digits = String::new();
    let mut end = 0usize;
    for (idx, ch) in &mut chars {
        if let Some(d) = fullwidth_or_ascii_digit(ch) {
            digits.push(d);
            end = idx + ch.len_utf8();
        } else if ch == ' ' || ch == '\u{3000}' {
            if !digits.is_empty() {
                return Some((digits, s[idx + ch.len_utf8()..].trim_start()));
            }
            return None;
        } else {
            return None;
        }
    }
    if digits.is_empty() {
        None
    } else {
        Some((digits, s[end..].trim_start()))
    }
}

fn parse_md_ordered_paragraph(s: &str) -> Option<(String, &str)> {
    let (num, rest) = s.split_once(". ")?;
    if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
        Some((num.to_string(), rest.trim_start()))
    } else {
        None
    }
}

fn split_leading_token(s: &str) -> Option<(&str, &str)> {
    s.split_once(|c: char| c == ' ' || c == '\u{3000}')
        .map(|(a, b)| (a.trim(), b.trim_start()))
        .filter(|(a, _)| !a.is_empty())
}

fn fullwidth_or_ascii_digit(c: char) -> Option<char> {
    match c {
        '0'..='9' => Some(c),
        '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32),
        _ => None,
    }
}

fn to_fullwidth_digits(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '0'..='9' => char::from_u32(c as u32 - '0' as u32 + '０' as u32).unwrap_or(c),
            _ => c,
        })
        .collect()
}

fn is_kanji_digit(c: char) -> bool {
    matches!(
        c,
        '〇' | '一' | '二' | '三' | '四' | '五' | '六' | '七' | '八' | '九' | '十' | '百' | '千'
    )
}

fn kanji_num_to_ascii(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let normalized: String = s
        .chars()
        .filter_map(fullwidth_or_ascii_digit)
        .collect();
    if normalized.len() == s.chars().count() {
        return Some(normalized);
    }
    let mut total = 0u32;
    let mut current = 0u32;
    for ch in s.chars() {
        match ch {
            '〇' => current = 0,
            '一' => current = 1,
            '二' => current = 2,
            '三' => current = 3,
            '四' => current = 4,
            '五' => current = 5,
            '六' => current = 6,
            '七' => current = 7,
            '八' => current = 8,
            '九' => current = 9,
            '十' => {
                total += if current == 0 { 1 } else { current } * 10;
                current = 0;
            }
            '百' => {
                total += if current == 0 { 1 } else { current } * 100;
                current = 0;
            }
            '千' => {
                total += if current == 0 { 1 } else { current } * 1000;
                current = 0;
            }
            _ => return None,
        }
    }
    Some((total + current).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_LAW_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Law Era="Heisei" Lang="ja" LawType="Act" Num="088" Year="05">
  <LawNum>平成五年法律第八十八号</LawNum>
  <LawBody>
    <LawTitle>行政手続法</LawTitle>
    <MainProvision>
      <Chapter Num="1">
        <ChapterTitle>第一章 総則</ChapterTitle>
        <Article Num="1">
          <ArticleCaption>（目的等）</ArticleCaption>
          <ArticleTitle>第一条</ArticleTitle>
          <Paragraph Num="1">
            <ParagraphNum></ParagraphNum>
            <ParagraphSentence><Sentence>この法律は、処分、行政指導及び届出に関する手続を定める。</Sentence></ParagraphSentence>
          </Paragraph>
          <Paragraph Num="2">
            <ParagraphNum>２</ParagraphNum>
            <ParagraphSentence><Sentence>他の法律に特別の定めがある場合は、その定めるところによる。</Sentence></ParagraphSentence>
            <Item Num="1">
              <ItemTitle>一</ItemTitle>
              <ItemSentence><Sentence>国の機関に対する処分</Sentence></ItemSentence>
            </Item>
          </Paragraph>
        </Article>
      </Chapter>
    </MainProvision>
    <SupplProvision>
      <SupplProvisionLabel>附則</SupplProvisionLabel>
      <Paragraph Num="1">
        <ParagraphNum></ParagraphNum>
        <ParagraphSentence><Sentence>この法律は、公布の日から起算して一年を超えない範囲内において施行する。</Sentence></ParagraphSentence>
      </Paragraph>
    </SupplProvision>
  </LawBody>
</Law>"#;

    #[test]
    fn law_xml_probe_handles_utf8_across_scan_boundary() {
        let markdown = format!("# Heading\n{}日本語", "a".repeat(4085));
        assert!(!is_law_xml(markdown.as_bytes()));
        assert!(input_to_markdown(markdown.as_bytes()).unwrap().is_none());
    }

    #[test]
    fn roundtrips_law_xml_to_lawtext_and_back() {
        assert!(is_law_xml(SAMPLE_LAW_XML.as_bytes()));
        let parsed = parse_law_xml(SAMPLE_LAW_XML).expect("parse_law_xml");
        assert_eq!(parsed.law_title, "行政手続法");
        assert_eq!(parsed.law_num, "平成五年法律第八十八号");

        let lt = to_lawtext(&parsed);
        assert!(is_lawtext(lt.as_bytes()));
        assert!(lt.contains("行政手続法（平成五年法律第八十八号）"));
        assert!(lt.contains("（目的等）\n第一条 この法律は、処分、行政指導及び届出に関する手続を定める。"));
        assert!(lt.contains("２ 他の法律に特別の定めがある場合は、その定めるところによる。"));

        let reparsed = parse_lawtext(&lt).expect("parse_lawtext");
        assert_eq!(reparsed.law_title, parsed.law_title);
        assert_eq!(reparsed.law_num, parsed.law_num);
        assert_eq!(reparsed.main_provision, parsed.main_provision);

        let xml_out = to_law_xml(&reparsed);
        let xml_reparsed = parse_law_xml(&xml_out).expect("reparse generated xml");
        assert_eq!(xml_reparsed.main_provision, parsed.main_provision);
    }

    #[test]
    fn roundtrips_law_xml_via_markdown() {
        let parsed = parse_law_xml(SAMPLE_LAW_XML).expect("parse_law_xml");
        let md = to_markdown(&parsed);
        assert!(md.contains("type: lawtext"));
        assert!(md.contains("# 行政手続法"));
        assert!(md.contains("### 第一条 （目的等）"));

        let from_md = parse_markdown_law(&md).expect("parse_markdown_law");
        assert_eq!(from_md.law_title, "行政手続法");
        assert_eq!(from_md.law_num, "平成五年法律第八十八号");
        assert_eq!(from_md.era.as_deref(), Some("Heisei"));
        assert_eq!(from_md.main_provision, parsed.main_provision);

        let xml = to_law_xml(&from_md);
        assert!(xml.contains("<Law Era=\"Heisei\" Lang=\"ja\" LawType=\"Act\" Num=\"088\" Year=\"05\">"));
        assert!(xml.contains("<Article Num=\"1\">"));
    }

    #[test]
    fn generates_shinkyu_markdown_and_opens_as_docx() {
        let old = parse_law_xml(SAMPLE_LAW_XML).expect("old");
        let mut new = old.clone();
        if let LawNode::Division { children, .. } = &mut new.main_provision[0] {
            if let LawNode::Article(art) = &mut children[0] {
                art.paragraphs[0].sentence =
                    "この法律は、処分、行政指導、届出及び命令等に関する手続を定める。".to_string();
            }
            children.push(LawNode::Article(LawArticle {
                num: "2".to_string(),
                caption: Some("（定義）".to_string()),
                title: "第二条".to_string(),
                paragraphs: vec![LawParagraph {
                    num: "1".to_string(),
                    sentence: "この法律において用語の意義を定める。".to_string(),
                    items: Vec::new(),
                }],
            }));
        }

        let shinkyu_md = generate_shinkyu_markdown(&old, &new);
        assert!(shinkyu_md.contains("# 行政手続法 新旧対照表"));
        assert!(shinkyu_md.contains("| 改正後 | 現行 |"));
        assert!(shinkyu_md.contains("行政指導++、++届出++及び命令等++"), "unexpected shinkyu_md:\n{shinkyu_md}");
        assert!(shinkyu_md.contains("行政指導~~及び~~届出"), "unexpected shinkyu_md:\n{shinkyu_md}");
        assert!(shinkyu_md.contains("（新設）"));

        let docx = crate::markdown::open_markdown(shinkyu_md.as_bytes().to_vec())
            .expect("open shinkyu markdown as docx");
        assert!(docx.presentation.flow.is_some());
    }

    #[test]
    fn only_law_files_are_read_as_law() {
        use std::path::Path;
        // A CSV and a text file that look like Lawtext by their content (（）in the first line, a line of 第一条) are not laws.
        let looks = "名簿（令和七年）,a,b\n第一条,c,d\n".as_bytes();
        for name in ["list.csv", "memo.txt", "notes.md", "plain.xml", "x.docx"] {
            assert!(path_input_to_markdown(Path::new(name), looks).unwrap().is_none(), "{name}");
        }
        // A .xml that is not a law (Office 2003 XML) is left to the other readers, and a broken .xml of a non-law does not fail.
        assert!(path_input_to_markdown(Path::new("a.xml"), b"<?xml version=\"1.0\"?><Workbook/>").unwrap().is_none());
        assert!(path_input_to_markdown(Path::new("a.xml"), b"\xff\xfe not utf-8").unwrap().is_none());
        // A .lawtext and a law .xml are read.
        let lt = "行政手続法（平成五年法律第八十八号）\n\n第一条　この法律は、目的を定める。\n";
        assert!(path_input_to_markdown(Path::new("a.lawtext"), lt.as_bytes()).unwrap().is_some());
        assert!(path_input_to_markdown(Path::new("a.law.txt"), lt.as_bytes()).unwrap().is_some());
    }

    #[test]
    fn handles_real_egov_xml_ruby_enact_quote_and_appdx_table() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<Law Era="Reiwa" Lang="ja" LawType="Act" Num="001" Year="07">
  <LawNum>令和七年法律第一号</LawNum>
  <LawBody>
    <LawTitle>情報公開法</LawTitle>
    <EnactStatement>情報公開法をここに公布する。</EnactStatement>
    <TOC><TOCLabel>目次</TOCLabel></TOC>
    <MainProvision>
      <Article Num="1">
        <ArticleTitle>第一条</ArticleTitle>
        <Paragraph Num="1">
          <ParagraphNum/>
          <ParagraphSentence>
            <Sentence>この法律は、<Ruby>行政<Rt>ぎょうせい</Rt></Ruby>の<QuoteStruct>「情報公開」</QuoteStruct>を定める。</Sentence>
          </ParagraphSentence>
          <List>
            <ListSentence>
              <Column><Sentence>第一欄</Sentence></Column>
              <Column><Sentence>第二欄</Sentence></Column>
            </ListSentence>
          </List>
        </Paragraph>
      </Article>
    </MainProvision>
    <AppdxTable>
      <AppdxTableTitle>別表第一</AppdxTableTitle>
      <RelatedArticleNum>第一条関係</RelatedArticleNum>
      <TableStruct>
        <Table>
          <TableRow>
            <TableColumn><Sentence>区分</Sentence></TableColumn>
            <TableColumn><Sentence>手数料</Sentence></TableColumn>
          </TableRow>
          <TableRow>
            <TableColumn><Sentence>電子申請</Sentence></TableColumn>
            <TableColumn><Sentence>三百円</Sentence></TableColumn>
          </TableRow>
        </Table>
      </TableStruct>
    </AppdxTable>
  </LawBody>
</Law>"#;

        let parsed = parse_law_xml(xml).expect("parse real e-Gov XML");
        assert_eq!(parsed.enact_statement.as_deref(), Some("情報公開法をここに公布する。"));
        assert_eq!(parsed.appdx_tables.len(), 1);
        assert_eq!(parsed.appdx_tables[0].title, "別表第一");
        assert_eq!(parsed.appdx_tables[0].related_article_num.as_deref(), Some("第一条関係"));
        assert_eq!(parsed.appdx_tables[0].rows.len(), 2);

        if let LawNode::Article(a) = &parsed.main_provision[0] {
            assert_eq!(a.paragraphs[0].sentence, "この法律は、行政の「情報公開」を定める。");
        } else {
            panic!("expected Article");
        }

        let xml_roundtrip = to_law_xml(&parsed);
        let reparsed = parse_law_xml(&xml_roundtrip).expect("reparse XML");
        assert_eq!(reparsed.enact_statement, parsed.enact_statement);
        assert_eq!(reparsed.appdx_tables, parsed.appdx_tables);

        let md = to_markdown(&parsed);
        assert!(md.contains("## 別表第一（第一条関係）"));
        assert!(md.contains("| 区分 | 手数料 |"));
    }

    #[test]
    fn supports_arabic_numeral_corporate_and_municipal_rules() {
        let old_lt = "\
就業規則（2025年規程第1号）
第1条 社員の服務規律を定める。
第2条の2 特別休暇を定める。";
        let new_lt = "\
就業規則（2025年規程第1号）
第1条 社員及び嘱託社員の服務規律を定める。
第2条の2 特別休暇及び育児休暇を定める。";

        let old_doc = parse_lawtext(old_lt).unwrap();
        let new_doc = parse_lawtext(new_lt).unwrap();
        assert_eq!(old_doc.main_provision.len(), 2);

        let shinkyu = generate_shinkyu_markdown(&old_doc, &new_doc);
        assert!(shinkyu.contains("社員++及び嘱託社員++の服務規律"));
        assert!(shinkyu.contains("特別休暇++及び育児休暇++を定める"));
    }
}
