// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The page setup of a sheet (ODF 1.3 §16.5 page layout, §20.? `style:page-layout-properties`): margins, orientation, paper size,
//! scale or fit to pages, centering, grid lines and headings, page order; read from the master page the sheet's table style names.

use std::collections::HashMap;

use roxmltree::Node;

use super::{attr, children, points, FO, OFFICE, STYLE, TEXT};

#[derive(Clone, Debug, Default)]
pub(super) struct Layout {
    /// Left, right, top and bottom margins and the header's and footer's distance from the edge, in inches (`pageMargins`).
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    pub header: f64,
    pub footer: f64,
    /// The paper in points, when the layout says (else the default paper).
    pub width: Option<f64>,
    pub height: Option<f64>,
    pub landscape: bool,
    /// `style:scale-to` in percent.
    pub scale: Option<u32>,
    /// `style:scale-to-X` and `style:scale-to-Y`, or `style:scale-to-pages` for both: pages wide and tall (0 is as many as needed).
    pub fit: Option<(u32, u32)>,
    pub horizontal_center: bool,
    pub vertical_center: bool,
    pub grid: bool,
    pub headings: bool,
    pub over_then_down: bool,
    /// `style:first-page-number` when it is a number (`continue` is none).
    pub first_page: Option<u32>,
    /// The room of the header and of the footer (inches) a master page that has them takes from the body.
    pub header_room: f64,
    pub footer_room: f64,
    /// The header and the footer of the master page as Excel's `&L..&C..&R..` codes (empty: none).
    pub header_code: String,
    pub footer_code: String,
}

/// The page layouts by name and the master pages' layout names.
#[derive(Default)]
pub(super) struct Pages {
    layouts: HashMap<String, Layout>,
    /// Master page name -> (layout name, has a header, has a footer).
    masters: HashMap<String, (String, bool, bool, String, String)>,
}

fn inches(v: Option<f64>) -> f64 {
    v.map_or(0.0, |p| p / 72.0)
}

pub(super) fn collect(sources: &[Node]) -> Pages {
    let mut pages = Pages::default();
    for source in sources {
        for holder in children(*source, OFFICE, "automatic-styles") {
            for l in children(holder, STYLE, "page-layout") {
                let Some(name) = attr(l, STYLE, "name") else { continue };
                let Some(p) = children(l, STYLE, "page-layout-properties").next() else { continue };
                let get = |ns: &'static str, k: &str| p.attribute((ns, k)).map(str::to_string);
                let len = |k: &str| get(FO, k).and_then(|v| points(&v));
                let part = |tag: &'static str| children(l, STYLE, tag).next().and_then(|h| children(h, STYLE, "header-footer-properties").next());
                let hf = |tag: &'static str, min: &str, gap: &str| -> (f64, f64) {
                    part(tag).map_or((0.0, 0.0), |h| (h.attribute((FO, min)).and_then(points).unwrap_or(0.0), h.attribute((FO, gap)).and_then(points).unwrap_or(0.0)))
                };
                let (header_min, header_gap) = hf("header-style", "min-height", "margin-bottom");
                let (footer_min, footer_gap) = hf("footer-style", "min-height", "margin-top");
                let mut layout = Layout {
                    left: inches(len("margin-left")),
                    right: inches(len("margin-right")),
                    top: inches(len("margin-top")),
                    bottom: inches(len("margin-bottom")),
                    header: inches(len("margin-top")),
                    footer: inches(len("margin-bottom")),
                    width: len("page-width"),
                    height: len("page-height"),
                    landscape: get(STYLE, "print-orientation").as_deref() == Some("landscape"),
                    ..Default::default()
                };
                layout.header_room = (header_min + header_gap) / 72.0;
                layout.footer_room = (footer_min + footer_gap) / 72.0;
                if let Some(s) = get(STYLE, "scale-to") {
                    layout.scale = s.trim_end_matches('%').parse::<f64>().ok().map(|v| v.round() as u32);
                }
                let pages_of = |k: &str| get(STYLE, k).and_then(|v| v.parse::<u32>().ok());
                if let Some(n) = pages_of("scale-to-pages") {
                    layout.fit = Some((n, n));
                }
                if pages_of("scale-to-X").is_some() || pages_of("scale-to-Y").is_some() {
                    layout.fit = Some((pages_of("scale-to-X").unwrap_or(0), pages_of("scale-to-Y").unwrap_or(0)));
                }
                match get(STYLE, "table-centering").as_deref() {
                    Some("horizontal") => layout.horizontal_center = true,
                    Some("vertical") => layout.vertical_center = true,
                    Some("both") => {
                        layout.horizontal_center = true;
                        layout.vertical_center = true;
                    }
                    _ => {}
                }
                let print = get(STYLE, "print").unwrap_or_default();
                layout.grid = print.split_whitespace().any(|w| w == "grid");
                layout.headings = print.split_whitespace().any(|w| w == "headers");
                layout.over_then_down = get(STYLE, "print-page-order").as_deref() == Some("ltr");
                layout.first_page = get(STYLE, "first-page-number").and_then(|v| v.parse::<u32>().ok()).filter(|n| *n > 0);
                pages.layouts.insert(name.to_string(), layout);
            }
        }
        for holder in children(*source, OFFICE, "master-styles") {
            for m in children(holder, STYLE, "master-page") {
                let (Some(name), Some(layout)) = (attr(m, STYLE, "name"), attr(m, STYLE, "page-layout-name")) else { continue };
                let has = |tag: &'static str| children(m, STYLE, tag).any(|h| h.attribute((STYLE, "display")) != Some("false"));
                let code = |tag: &'static str| children(m, STYLE, tag).find(|h| h.attribute((STYLE, "display")) != Some("false")).map(header_footer_code).unwrap_or_default();
                pages.masters.insert(name.to_string(), (layout.to_string(), has("header"), has("footer"), code("header"), code("footer")));
            }
        }
    }
    pages
}

impl Pages {
    /// The layout of the master page `master`.
    /// The header and the footer take their room from the body when the master page has them.
    pub(super) fn of_master(&self, master: &str) -> Option<Layout> {
        let (layout, header, footer, header_code, footer_code) = self.masters.get(master)?;
        let mut l = self.layouts.get(layout)?.clone();
        l.header_code = header_code.clone();
        l.footer_code = footer_code.clone();
        if *header {
            l.top += l.header_room;
        }
        if *footer {
            l.bottom += l.footer_room;
        }
        Some(l)
    }
}

/// The text of a master page's header or footer (`style:header`, `style:footer`) as Excel's header and footer codes: `&L`, `&C` and `&R` for
/// `style:region-left`, `-center` and `-right` (a header with no region is one centred part, as Excel reads it: measured by saving an ODS with
/// a plain `style:header` as xlsx, `&C...`), `&P` the page number, `&N` the page count, `&A` the sheet name, `&F` the file name, `&D` the date,
/// `&T` the time, `&&` an ampersand; the paragraphs of a part are joined by a line break.
fn header_footer_code(node: Node) -> String {
    fn walk(n: Node, out: &mut String) {
        for c in n.children() {
            if c.is_text() {
                out.push_str(&c.text().unwrap_or("").replace('&', "&&"));
            } else if c.is_element() && c.tag_name().namespace() == Some(TEXT) {
                match c.tag_name().name() {
                    "s" => out.push_str(&" ".repeat(attr(c, TEXT, "c").and_then(|v| v.parse().ok()).unwrap_or(1))),
                    "page-number" => out.push_str("&P"),
                    "page-count" => out.push_str("&N"),
                    "sheet-name" => out.push_str("&A"),
                    "file-name" => out.push_str("&F"),
                    "date" => out.push_str("&D"),
                    "time" => out.push_str("&T"),
                    "tab" => out.push('\t'),
                    "line-break" => out.push('\n'),
                    _ => walk(c, out),
                }
            } else if c.is_element() {
                walk(c, out);
            }
        }
    }
    let part = |holder: Node| -> String {
        let mut paragraphs = Vec::new();
        for p in children(holder, TEXT, "p") {
            let mut t = String::new();
            walk(p, &mut t);
            paragraphs.push(t);
        }
        while paragraphs.last().is_some_and(String::is_empty) {
            paragraphs.pop();
        }
        paragraphs.join("\n")
    };
    let regions: Vec<(&str, String)> = [("&L", "region-left"), ("&C", "region-center"), ("&R", "region-right")]
        .into_iter()
        .filter_map(|(code, tag)| children(node, STYLE, tag).next().map(|r| (code, part(r))))
        .collect();
    if regions.is_empty() {
        let t = part(node);
        return if t.is_empty() { String::new() } else { format!("&C{t}") };
    }
    regions.into_iter().filter(|(_, t)| !t.is_empty()).map(|(code, t)| format!("{code}{t}")).collect()
}

/// `ST_PaperSize` of a paper of `w` by `h` points (portrait or landscape), when it is a common one; A4 when the layout says nothing.
fn paper_size(w: Option<f64>, h: Option<f64>) -> u32 {
    let (Some(w), Some(h)) = (w, h) else { return 9 };
    let (short, long) = (w.min(h) / 72.0 * 25.4, w.max(h) / 72.0 * 25.4);
    let near = |a: f64, b: f64| (a - b).abs() < 2.0;
    for (id, s, l) in [(9, 210.0, 297.0), (8, 297.0, 420.0), (11, 148.0, 210.0), (1, 215.9, 279.4), (5, 215.9, 355.6), (13, 182.0, 257.0), (12, 250.0, 353.0), (7, 184.0, 267.0)] {
        if near(short, s) && near(long, l) {
            return id;
        }
    }
    9
}

/// `sheetPr` (the fit to page flag), and the print options, margins and page setup of a sheet.
pub(super) fn xml(layout: Option<&Layout>) -> (String, String) {
    let Some(l) = layout else {
        return (String::new(), "<pageMargins left=\"0.7\" right=\"0.7\" top=\"0.75\" bottom=\"0.75\" header=\"0.3\" footer=\"0.3\"/>".to_string());
    };
    let sheet_pr = if l.fit.is_some() { "<sheetPr><pageSetUpPr fitToPage=\"1\"/></sheetPr>".to_string() } else { String::new() };
    let mut out = String::new();
    if l.horizontal_center || l.vertical_center || l.grid || l.headings {
        out.push_str("<printOptions");
        if l.horizontal_center {
            out.push_str(" horizontalCentered=\"1\"");
        }
        if l.vertical_center {
            out.push_str(" verticalCentered=\"1\"");
        }
        if l.headings {
            out.push_str(" headings=\"1\"");
        }
        if l.grid {
            out.push_str(" gridLines=\"1\"");
        }
        out.push_str("/>");
    }
    out.push_str(&format!(
        "<pageMargins left=\"{:.4}\" right=\"{:.4}\" top=\"{:.4}\" bottom=\"{:.4}\" header=\"{:.4}\" footer=\"{:.4}\"/>",
        l.left, l.right, l.top, l.bottom, l.header, l.footer
    ));
    out.push_str(&format!("<pageSetup paperSize=\"{}\"", paper_size(l.width, l.height)));
    if let Some(s) = l.scale.filter(|_| l.fit.is_none()) {
        out.push_str(&format!(" scale=\"{s}\""));
    }
    if let Some((w, h)) = l.fit {
        out.push_str(&format!(" fitToWidth=\"{w}\" fitToHeight=\"{h}\""));
    }
    // Excel reads `style:first-page-number="10"` as `firstPageNumber="10" useFirstPageNumber="1"` (`-SaveAs toxlsx`).
    if let Some(n) = l.first_page {
        out.push_str(&format!(" firstPageNumber=\"{n}\" useFirstPageNumber=\"1\""));
    }
    if l.over_then_down {
        out.push_str(" pageOrder=\"overThenDown\"");
    }
    out.push_str(&format!(" orientation=\"{}\"/>", if l.landscape { "landscape" } else { "portrait" }));
    if !l.header_code.is_empty() || !l.footer_code.is_empty() {
        out.push_str("<headerFooter>");
        for (tag, code) in [("oddHeader", &l.header_code), ("oddFooter", &l.footer_code)] {
            if !code.is_empty() {
                out.push_str(&format!("<{tag} xml:space=\"preserve\">{}</{tag}>", super::esc(code)));
            }
        }
        out.push_str("</headerFooter>");
    }
    (sheet_pr, out)
}
