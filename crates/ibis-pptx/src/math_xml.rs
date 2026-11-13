// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Office Math markup (`m:oMath`) read into a [`Math`] tree.

use ibis_doc_model::math::{FracKind, Math, stacked_by_default};
use roxmltree::Node;

pub(crate) const M: &str = "http://schemas.openxmlformats.org/officeDocument/2006/math";

fn m_child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(M) && c.tag_name().name() == name)
}

/// The `m:val` of `m:<pr>/m:<name>`.
fn pr_val<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    let pr = n.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(M) && c.tag_name().name().ends_with("Pr"))?;
    m_child(pr, name)?.attribute((M, "val"))
}

/// The content of a child such as `m:e` or `m:num`.
fn part(n: Node, name: &str) -> Math {
    m_child(n, name).map(row).unwrap_or(Math::Row(Vec::new()))
}

fn opt(n: Node, name: &str) -> Option<Box<Math>> {
    m_child(n, name).map(row).filter(|m| !m.linear().is_empty()).map(Box::new)
}

/// The children of `n` that are math, as one node.
fn row(n: Node) -> Math {
    let mut items: Vec<Math> = n.children().filter(|c| c.is_element()).filter_map(node).collect();
    if items.len() == 1 { items.remove(0) } else { Math::Row(items) }
}

/// An `m:oMath` or `m:oMathPara` as a node.
pub fn read_math(n: Node) -> Math {
    row(n)
}

fn node(n: Node) -> Option<Math> {
    if n.tag_name().namespace() != Some(M) {
        return None;
    }
    let boxed = |m: Math| Box::new(m);
    Some(match n.tag_name().name() {
        "r" => {
            let text = Math::Text(n.children().filter(|c| c.is_element() && c.tag_name().name() == "t").filter_map(|t| t.text()).collect());
            // The run's own colour (`w:rPr/w:color`; `auto` and the like: the formula's).
            match run_color(n) {
                Some(color) => Math::Colored { color, body: Box::new(text) },
                None => text,
            }
        }
        "oMath" | "e" | "box" | "phant" | "borderBox" => row(n),
        "oMathPara" => row(n),
        "f" => Math::Frac {
            num: boxed(part(n, "num")),
            den: boxed(part(n, "den")),
            kind: match pr_val(n, "type") {
                Some("noBar") => FracKind::NoBar,
                Some("skw") => FracKind::Skew,
                Some("lin") => FracKind::Linear,
                _ => FracKind::Bar,
            },
        },
        "sSup" => Math::Script { base: boxed(part(n, "e")), sub: None, sup: opt(n, "sup") },
        "sSub" => Math::Script { base: boxed(part(n, "e")), sub: opt(n, "sub"), sup: None },
        "sSubSup" => Math::Script { base: boxed(part(n, "e")), sub: opt(n, "sub"), sup: opt(n, "sup") },
        "sPre" => Math::Row(vec![Math::Script { base: boxed(Math::Text(String::new())), sub: opt(n, "sub"), sup: opt(n, "sup") }, part(n, "e")]),
        "rad" => Math::Radical { degree: if pr_val(n, "degHide").is_some_and(|v| v == "1" || v == "true" || v == "on") { None } else { opt(n, "deg") }, body: boxed(part(n, "e")) },
        "nary" => {
            let hide = |name: &str| pr_val(n, name).is_some_and(|v| v == "1" || v == "true" || v == "on");
            let op = pr_val(n, "chr").unwrap_or("∫").to_string();
            let stacked = match pr_val(n, "limLoc") {
                Some("undOvr") => true,
                Some("subSup") => false,
                _ => stacked_by_default(&op),
            };
            Math::Nary {
                stacked,
                op,
                sub: if hide("subHide") { None } else { opt(n, "sub") },
                sup: if hide("supHide") { None } else { opt(n, "sup") },
                body: boxed(part(n, "e")),
            }
        }
        "d" => Math::Delimited {
            open: pr_val(n, "begChr").unwrap_or("(").to_string(),
            close: pr_val(n, "endChr").unwrap_or(")").to_string(),
            sep: pr_val(n, "sepChr").unwrap_or("|").to_string(),
            items: n.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(M) && c.tag_name().name() == "e").map(row).collect(),
        },
        "func" => Math::Function { name: boxed(part(n, "fName")), arg: boxed(part(n, "e")) },
        "acc" => Math::Mark { base: boxed(part(n, "e")), mark: pr_val(n, "chr").unwrap_or("\u{302}").to_string(), over: true },
        "bar" => Math::Mark { base: boxed(part(n, "e")), mark: if pr_val(n, "pos") == Some("bot") { "\u{332}" } else { "\u{305}" }.to_string(), over: true },
        "groupChr" => Math::Mark { base: boxed(part(n, "e")), mark: pr_val(n, "chr").unwrap_or("⏟").to_string(), over: pr_val(n, "pos") == Some("top") },
        "limUpp" => Math::Mark { base: boxed(part(n, "e")), mark: part(n, "lim").linear(), over: true },
        "limLow" => Math::Mark { base: boxed(part(n, "e")), mark: part(n, "lim").linear(), over: false },
        "eqArr" => Math::EqArray(n.children().filter(|c| c.is_element() && c.tag_name().name() == "e").map(row).collect()),
        "m" => Math::Matrix(
            n.children()
                .filter(|c| c.is_element() && c.tag_name().namespace() == Some(M) && c.tag_name().name() == "mr")
                .map(|r| r.children().filter(|c| c.is_element() && c.tag_name().name() == "e").map(row).collect())
                .collect(),
        ),
        _ => return None,
    })
}

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

/// The colour a run (`m:r`) is given (`w:rPr/w:color/@w:val`): Word writes the colour of a formula's text on its runs, each its own (#171).
/// `None`: none (or `auto`).
fn run_color(r: Node) -> Option<ibis_doc_model::drawing::Rgb> {
    let value = r
        .children()
        .filter(|c| c.is_element() && c.tag_name().namespace() == Some(W) && c.tag_name().name() == "rPr")
        .flat_map(|rpr| rpr.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(W) && c.tag_name().name() == "color"))
        .find_map(|c| c.attribute((W, "val")))?;
    (value.len() == 6).then(|| u32::from_str_radix(value, 16).ok()).flatten().map(ibis_doc_model::drawing::Rgb)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn run(text: &str, upright: bool, color: Option<ibis_doc_model::drawing::Rgb>) -> String {
    if text.is_empty() {
        return String::new();
    }
    let props = if upright { "<m:rPr><m:sty m:val=\"p\"/></m:rPr>" } else { "" };
    let color = color.map_or(String::new(), |c| format!("<w:rPr><w:color w:val=\"{:06X}\"/></w:rPr>", c.0));
    format!("<m:r>{props}{color}<m:t xml:space=\"preserve\">{}</m:t></m:r>", esc(text))
}

fn wrap(tag: &str, inner: String) -> String {
    format!("<m:{tag}>{inner}</m:{tag}>")
}

/// The content of a part (`m:e`, `m:num`, ...) as markup.
fn content(m: &Math, upright: bool) -> String {
    match m {
        Math::Text(t) => run(t, upright, None),
        Math::Colored { color, body } => match &**body {
            Math::Text(t) => run(t, upright, Some(*color)),
            other => content(other, upright),
        },
        Math::Row(items) => items.iter().map(|i| content(i, upright)).collect(),
        Math::Frac { num, den, kind } => {
            let props = match kind {
                FracKind::Bar => String::new(),
                FracKind::NoBar => "<m:fPr><m:type m:val=\"noBar\"/></m:fPr>".to_string(),
                FracKind::Skew => "<m:fPr><m:type m:val=\"skw\"/></m:fPr>".to_string(),
                FracKind::Linear => "<m:fPr><m:type m:val=\"lin\"/></m:fPr>".to_string(),
            };
            wrap("f", format!("{props}{}{}", wrap("num", content(num, upright)), wrap("den", content(den, upright))))
        }
        Math::Script { base, sub, sup } => match (sub, sup) {
            (None, None) => content(base, upright),
            (None, Some(u)) => wrap("sSup", format!("{}{}", wrap("e", content(base, upright)), wrap("sup", content(u, upright)))),
            (Some(d), None) => wrap("sSub", format!("{}{}", wrap("e", content(base, upright)), wrap("sub", content(d, upright)))),
            (Some(d), Some(u)) => wrap("sSubSup", format!("{}{}{}", wrap("e", content(base, upright)), wrap("sub", content(d, upright)), wrap("sup", content(u, upright)))),
        },
        Math::Radical { degree, body } => match degree {
            None => wrap("rad", format!("<m:radPr><m:degHide m:val=\"1\"/></m:radPr><m:deg/>{}", wrap("e", content(body, upright)))),
            Some(d) => wrap("rad", format!("{}{}", wrap("deg", content(d, upright)), wrap("e", content(body, upright)))),
        },
        Math::Nary { op, sub, sup, body, stacked } => {
            let mut props = format!("<m:chr m:val=\"{}\"/><m:limLoc m:val=\"{}\"/>", esc(op), if *stacked { "undOvr" } else { "subSup" });
            if sub.is_none() {
                props += "<m:subHide m:val=\"1\"/>";
            }
            if sup.is_none() {
                props += "<m:supHide m:val=\"1\"/>";
            }
            let empty = Math::Row(Vec::new());
            wrap(
                "nary",
                format!(
                    "<m:naryPr>{props}</m:naryPr>{}{}{}",
                    wrap("sub", content(sub.as_deref().unwrap_or(&empty), upright)),
                    wrap("sup", content(sup.as_deref().unwrap_or(&empty), upright)),
                    wrap("e", content(body, upright))
                ),
            )
        }
        Math::Delimited { open, close, sep, items } => {
            let props = format!("<m:dPr><m:begChr m:val=\"{}\"/><m:sepChr m:val=\"{}\"/><m:endChr m:val=\"{}\"/></m:dPr>", esc(open), esc(sep), esc(close));
            wrap("d", format!("{props}{}", items.iter().map(|i| wrap("e", content(i, upright))).collect::<String>()))
        }
        Math::Function { name, arg } => wrap("func", format!("{}{}", wrap("fName", content(name, true)), wrap("e", content(arg, upright)))),
        Math::Mark { base, mark, over } => {
            if mark.chars().count() == 1 && !mark.is_ascii() && matches!(mark.as_str(), "\u{302}" | "\u{303}" | "\u{307}" | "\u{308}" | "\u{30c}") {
                wrap("acc", format!("<m:accPr><m:chr m:val=\"{}\"/></m:accPr>{}", esc(mark), wrap("e", content(base, upright))))
            } else if matches!(mark.as_str(), "\u{305}" | "\u{332}") {
                let pos = if *over { "top" } else { "bot" };
                wrap("bar", format!("<m:barPr><m:pos m:val=\"{pos}\"/></m:barPr>{}", wrap("e", content(base, upright))))
            } else {
                let tag = if *over { "limUpp" } else { "limLow" };
                wrap(tag, format!("{}{}", wrap("e", content(base, upright)), wrap("lim", run(mark, true, None))))
            }
        }
        Math::EqArray(rows) => wrap("eqArr", rows.iter().map(|r| wrap("e", content(r, upright))).collect::<String>()),
        Math::Matrix(rows) => wrap("m", rows.iter().map(|r| wrap("mr", r.iter().map(|c| wrap("e", content(c, upright))).collect::<String>())).collect::<String>()),
    }
}

/// The markup of a display formula: `m:oMathPara` holding an `m:oMath` (its `m` namespace declared on it).
pub fn write_math(math: &Math) -> String {
    format!("<m:oMathPara xmlns:m=\"{M}\" xmlns:w=\"{W}\"><m:oMath>{}</m:oMath></m:oMathPara>", content(math, false))
}

/// The markup of an inline formula: `m:oMath` (its `m` namespace declared on it).
pub fn write_math_inline(math: &Math) -> String {
    format!("<m:oMath xmlns:m=\"{M}\" xmlns:w=\"{W}\">{}</m:oMath>", content(math, false))
}

/// The paragraph content that holds a formula the way PowerPoint writes it: `a14:m` in the `Choice` of a
/// markup-compatibility wrapper, the formula's line of text as its fallback.
pub fn write_math_content(math: &Math, a: &str, inline: bool) -> String {
    // A formula in a line of text is an `m:oMath`; one of its own, an `m:oMathPara` holding it.
    // A run's own colour is Word's (`w:rPr`); PowerPoint's formulas have the colour of the text run.
    let math = &math.without_colors();
    let markup = if inline { format!("<m:oMath xmlns:m=\"{M}\">{}</m:oMath>", content(math, false)) } else { format!("<m:oMathPara xmlns:m=\"{M}\"><m:oMath>{}</m:oMath></m:oMathPara>", content(math, false)) };
    format!(
        "<mc:AlternateContent xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><mc:Choice xmlns:a14=\"http://schemas.microsoft.com/office/drawing/2010/main\" Requires=\"a14\"><a14:m>{markup}</a14:m></mc:Choice><mc:Fallback><{a}r><{a}t>{}</{a}t></{a}r></mc:Fallback></mc:AlternateContent>",
        esc(&math.linear())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_formula() {
        let xml = format!(
            r#"<m:oMath xmlns:m="{M}"><m:f><m:num><m:r><m:t>a</m:t></m:r></m:num><m:den><m:sSup><m:e><m:r><m:t>b</m:t></m:r></m:e><m:sup><m:r><m:t>2</m:t></m:r></m:sup></m:sSup></m:den></m:f><m:r><m:t>=</m:t></m:r><m:rad><m:radPr><m:degHide m:val="1"/></m:radPr><m:deg/><m:e><m:r><m:t>x</m:t></m:r></m:e></m:rad></m:oMath>"#
        );
        let doc = roxmltree::Document::parse(&xml).unwrap();
        assert_eq!(read_math(doc.root_element()).linear(), "a/b²=√x");
    }

    #[test]
    fn what_is_written_reads_back_the_same() {
        for text in ["a/(b+c)", "x²", "x^a", "√(x+1)", "∛x", "∑_(i=1)^n x", "a=b+c", "■(1&2@3&4)", "█(x&=1@yy&=2)", "sin x", "(a+b)/2", "x_i^2", "[a,b]"] {
            let math = ibis_doc_model::math::parse_linear(text);
            let xml = write_math(&math);
            let doc = roxmltree::Document::parse(&xml).unwrap_or_else(|e| panic!("{text}: {e}\n{xml}"));
            assert_eq!(read_math(doc.root_element()).linear(), math.linear(), "{text}\n{xml}");
        }
    }
}
