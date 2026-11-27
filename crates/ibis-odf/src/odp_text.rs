// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The text of an OpenDocument presentation ([ODF 1.3] part 1, chapters 5 and 6, 16.28 to 16.31): paragraphs, spans, lists,
//! links and fields as DrawingML paragraphs (`a:p`), with the styles' properties written out in the runs (nothing is left to
//! be inherited from the master).

use std::fmt::Write as _;

use roxmltree::Node;

use crate::odp::Cx;
use crate::odp_style::{LOEXT, PRESENTATION, STYLE, TEXT, XLINK, attr, color, emu, is, percent, points, FO};

pub(crate) fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// What a text body is, for the properties its paragraphs inherit.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Body {
    /// A body placeholder: paragraphs outside a list switch the master's bullets off.
    pub(crate) placeholder_body: bool,
}

type Chain<'n> = Vec<(&'static str, String)>;

fn names<'a>(chain: &'a Chain) -> Vec<(&'static str, &'a str)> {
    chain.iter().map(|(f, n)| (*f, n.as_str())).collect()
}

/// The first value of the property along the chain of styles.
fn value<'a>(cx: &Cx<'_, 'a, '_>, chain: &Chain, ns: &str, local: &str) -> Option<&'a str> {
    cx.st.prop_in(&names(chain), ns, local)
}

/// The run properties (`a:rPr`, `a:endParaRPr`) the styles give.
fn rpr(cx: &mut Cx, chain: &Chain, tag: &str, link: Option<&str>, text: &str) -> String {
    let get = |ns: &str, local: &str| value(cx, chain, ns, local);
    let mut attrs = String::new();
    // PowerPoint writes `lang` of a run as the Asian language of the style (`style:language-asian` and `style:country-asian`: ja-JP, zh-TW)
    // when the FIRST character of the run's text is East Asian, else `fo:language` (measured with topptx on 110 and on runs I wrote into
    // it: it does not split a run by script; `abc日本語def`, `123日本語`, `ab 日本`, `※注意：日本語` (※ is not East Asian) are en-US,
    // `日本語、ABC（かな）１２`, `日本 語 x y`, `　日本語` ja-JP; in 110 85 runs are ja-JP, 4 zh-TW, 14 en-US). The language decides how PowerPoint
    // breaks the lines of the text, kinsoku included: 110 0.751 -> 0.927.
    let asian = text.chars().next().is_some_and(ibis_doc_model::is_east_asian);
    let (lang_ns, country_ns) = if asian { (STYLE, STYLE) } else { (FO, FO) };
    let (lang_key, country_key) = if asian { ("language-asian", "country-asian") } else { ("language", "country") };
    let lang = get(lang_ns, lang_key);
    if let Some(l) = lang.filter(|l| *l != "none") {
        match get(country_ns, country_key).filter(|c| *c != "none") {
            Some(c) => {
                let _ = write!(attrs, r#" lang="{l}-{c}""#);
            }
            None => {
                let _ = write!(attrs, r#" lang="{l}""#);
            }
        }
    }
    if let Some(size) = get(FO, "font-size").and_then(points) {
        let _ = write!(attrs, r#" sz="{}""#, (size * 100.0).round() as i64);
    }
    match get(FO, "font-weight") {
        Some("bold") => attrs += r#" b="1""#,
        Some(w) if w.parse::<u32>().is_ok_and(|n| n >= 600) => attrs += r#" b="1""#,
        Some(_) => attrs += r#" b="0""#,
        None => {}
    }
    match get(FO, "font-style") {
        Some("italic" | "oblique") => attrs += r#" i="1""#,
        Some(_) => attrs += r#" i="0""#,
        None => {}
    }
    let underline = get(STYLE, "text-underline-style").filter(|s| *s != "none").map(|s| {
        let double = get(STYLE, "text-underline-type") == Some("double");
        let bold = matches!(get(STYLE, "text-underline-width"), Some("bold" | "thick"));
        match (s, double, bold) {
            (_, true, _) => "dbl",
            ("dotted", _, false) => "dotted",
            ("dotted", _, true) => "dottedHeavy",
            ("dash" | "long-dash", _, false) => "dash",
            ("dash" | "long-dash", _, true) => "dashHeavy",
            ("dot-dash", _, _) => "dotDash",
            ("dot-dot-dash", _, _) => "dotDotDash",
            ("wave", _, _) => "wavy",
            (_, _, true) => "heavy",
            _ => "sng",
        }
    });
    if let Some(u) = underline {
        let _ = write!(attrs, r#" u="{u}""#);
    } else if get(STYLE, "text-underline-style") == Some("none") {
        attrs += r#" u="none""#;
    }
    match get(STYLE, "text-line-through-style") {
        Some("none") => attrs += r#" strike="noStrike""#,
        Some(_) => attrs += if get(STYLE, "text-line-through-type") == Some("double") { r#" strike="dblStrike""# } else { r#" strike="sngStrike""# },
        None => {}
    }
    if let Some(position) = get(STYLE, "text-position") {
        let first = position.split_whitespace().next().unwrap_or("");
        let baseline = match first {
            "super" => Some(30_000),
            "sub" => Some(-25_000),
            _ => percent(first).map(|p| (p * 100_000.0).round() as i64),
        };
        if let Some(b) = baseline.filter(|b| *b != 0) {
            let _ = write!(attrs, r#" baseline="{b}""#);
        }
    }
    match (get(FO, "text-transform"), get(FO, "font-variant")) {
        (Some("uppercase"), _) => attrs += r#" cap="all""#,
        (_, Some("small-caps")) => attrs += r#" cap="small""#,
        _ => {}
    }
    if let Some(spacing) = get(FO, "letter-spacing").and_then(points).filter(|s| s.abs() > 1e-6) {
        let _ = write!(attrs, r#" spc="{}""#, (spacing * 100.0).round() as i64);
    }
    // Pair kerning (`style:letter-kerning`): PowerPoint's `-SaveAs pptx` of an .odp writes `kern="1200"` (kerning from 12 pt up) for true and
    // `kern="0"` for false on every run (measured on 155 documents: 006 has only false and only `kern="0"`).
    match get(STYLE, "letter-kerning") {
        Some("true") => attrs += r#" kern="1200""#,
        Some("false") => attrs += r#" kern="0""#,
        _ => {}
    }
    let mut inner = String::new();
    // (A colour the styles name; "use the window's font colour" is only the lack of one.)
    if let Some(c) = get(FO, "color").and_then(color) {
        let alpha = get(LOEXT, "opacity").and_then(percent).filter(|a| *a < 0.999);
        match alpha {
            Some(a) => {
                let _ = write!(inner, r#"<a:solidFill><a:srgbClr val="{c}"><a:alpha val="{}"/></a:srgbClr></a:solidFill>"#, (a * 100_000.0).round() as i64);
            }
            None => {
                let _ = write!(inner, r#"<a:solidFill><a:srgbClr val="{c}"/></a:solidFill>"#);
            }
        }
    }
    // `fo:text-shadow` ("x y blur color", lengths in either order with the colour) is `a:effectLst/a:outerShdw` of the run. PowerPoint's reading,
    // measured with topptx on variants of a text shadow (x, y, blur): dist = hypot(x, y) in EMU, dir = atan2(y, x) in 60000ths of a degree
    // (left out when 0), blurRad = blur in EMU (left out when 0); no `algn`. (0.03in 0.03in -> dist 38795 dir 2700000; 0.03in 0 -> dist 27432;
    // -0.03in -0.03in -> dir 13500000; 0.06in 0.03in -> dir 1593903; 1pt 1pt -> dist 17961; blur 0.05in -> blurRad 45720.)
    if let Some(shadow) = get(FO, "text-shadow").filter(|s| *s != "none") {
        let mut lengths = Vec::new();
        let mut colour = None;
        for token in shadow.split_whitespace() {
            match points(token) {
                Some(v) => lengths.push(v * 12_700.0),
                None => colour = colour.or_else(|| color(token)),
            }
        }
        if lengths.len() >= 2 {
            let (x, y) = (lengths[0], lengths[1]);
            let mut a = String::new();
            if let Some(b) = lengths.get(2).filter(|b| **b > 0.5) {
                let _ = write!(a, r#" blurRad="{}""#, b.round() as i64);
            }
            let dist = x.hypot(y).round() as i64;
            let _ = write!(a, r#" dist="{dist}""#);
            let dir = (y.atan2(x).to_degrees().rem_euclid(360.0) * 60_000.0).round() as i64;
            if dir != 0 {
                let _ = write!(a, r#" dir="{dir}""#);
            }
            let c = colour.unwrap_or_else(|| "000000".to_string());
            let _ = write!(inner, r#"<a:effectLst><a:outerShdw{a}><a:srgbClr val="{c}"/></a:outerShdw></a:effectLst>"#);
        }
    }
    if let Some(c) = get(FO, "background-color").and_then(color) {
        let _ = write!(inner, r#"<a:highlight><a:srgbClr val="{c}"/></a:highlight>"#);
    }
    let family = |name: Option<&str>, generic: Option<&str>| -> Option<String> {
        name.map(|n| cx.st.font_family(n)).or_else(|| generic.map(|g| g.trim_matches('\'').to_string()))
    };
    let latin = family(get(STYLE, "font-name"), get(FO, "font-family"));
    let asian = family(get(STYLE, "font-name-asian"), get(STYLE, "font-family-asian"));
    let complex = family(get(STYLE, "font-name-complex"), get(STYLE, "font-family-complex"));
    if let Some(f) = latin {
        let _ = write!(inner, r#"<a:latin typeface="{}"/>"#, esc(&f));
    }
    if let Some(f) = asian {
        let _ = write!(inner, r#"<a:ea typeface="{}"/>"#, esc(&f));
    }
    if let Some(f) = complex {
        let _ = write!(inner, r#"<a:cs typeface="{}"/>"#, esc(&f));
    }
    if let Some(id) = link {
        // PowerPoint's ODP reading: a link is `hlinkClick` with the extension that keeps the text's own colour (`hlinkClr` tx) on every link
        // measured (topptx of 8 documents with 700 text links); a jump to a page has `action="ppaction://hlinksldjump"`.
        let action = if cx.is_slide_link(id) { r#" action="ppaction://hlinksldjump""# } else { "" };
        let _ = write!(inner, r#"<a:hlinkClick r:id="{id}"{action}><a:extLst><a:ext uri="{{A12FA001-AC4F-418D-AE19-62706E023703}}"><ahyp:hlinkClr xmlns:ahyp="http://schemas.microsoft.com/office/drawing/2018/hyperlinkcolor" val="tx"/></a:ext></a:extLst></a:hlinkClick>"#);
    }
    if inner.is_empty() { format!("<a:{tag}{attrs}/>") } else { format!("<a:{tag}{attrs}>{inner}</a:{tag}>") }
}

/// One level of a list style.
struct Level<'a, 'i> {
    node: Node<'a, 'i>,
}

fn level_of<'a, 'i>(cx: &Cx<'_, 'a, 'i>, list_style: &str, level: usize) -> Option<Level<'a, 'i>> {
    let style = cx.st.list_style(list_style)?;
    style.children().find(|c| c.is_element() && c.tag_name().namespace() == Some(TEXT) && c.tag_name().name().starts_with("list-level-style") && attr(*c, TEXT, "level").and_then(|l| l.parse::<usize>().ok()) == Some(level + 1)).map(|node| Level { node })
}

/// The paragraph properties (`a:pPr`) of a paragraph.
fn ppr(cx: &mut Cx, chain: &Chain, list: Option<(&str, usize)>, body: Body) -> String {
    let get = |ns: &str, local: &str| value(cx, chain, ns, local);
    let mut attrs = String::new();
    let mut inner = String::new();
    let rtl = get(STYLE, "writing-mode") == Some("rl-tb");
    let align = match get(FO, "text-align") {
        Some("center") => Some("ctr"),
        Some("justify") => Some("just"),
        Some("end") => Some(if rtl { "l" } else { "r" }),
        Some("right") => Some("r"),
        Some("start") => Some(if rtl { "r" } else { "l" }),
        Some("left") => Some("l"),
        _ => None,
    };
    if let Some(a) = align {
        let _ = write!(attrs, r#" algn="{a}""#);
    }
    if rtl {
        attrs += r#" rtl="1""#;
    }
    let margin_left = get(FO, "margin-left").and_then(emu).unwrap_or(0);
    let margin_right = get(FO, "margin-right").and_then(emu);
    let text_indent = get(FO, "text-indent").and_then(emu).unwrap_or(0);
    let (mut mar_l, mut indent) = (margin_left, text_indent);
    let mut bullet = String::new();
    match list {
        Some((list_style, level)) => {
            let _ = write!(attrs, r#" lvl="{}""#, level.min(8));
            if let Some(l) = level_of(cx, list_style, level) {
                let lp = l.node.children().find(|c| is(*c, STYLE, "list-level-properties"));
                let (before, label) = lp.map_or((0, 0), |p| (attr(p, TEXT, "space-before").and_then(emu).unwrap_or(0), attr(p, TEXT, "min-label-width").and_then(emu).unwrap_or(0)));
                // The newer positioning (label alignment) gives the margin and the indent itself.
                let aligned = lp.and_then(|p| p.children().find(|c| is(*c, STYLE, "list-level-label-alignment")));
                match aligned {
                    Some(a) => {
                        mar_l = margin_left + attr(a, FO, "margin-left").and_then(emu).unwrap_or(0);
                        indent = attr(a, FO, "text-indent").and_then(emu).unwrap_or(0);
                    }
                    // The older positioning (space-before and min-label-width of the list level). PowerPoint, reading an ODP, takes the paragraph's own
                    // fo:margin-left and fo:text-indent as marL and indent and ignores the list level's space-before and min-label-width (measured by
                    // saving ODP files as pptx: paragraph 1in / -0.2in with a list level of 0.5in / 0.3in or 0in / 0.3in read 1in / -0.2in, and
                    // paragraph 0.6in / 0 with a level of 0 / 0 read 0.6in / 0). A paragraph with neither (our own writer's lists, other producers')
                    // takes the list level's, as ODF says.
                    None if get(FO, "margin-left").is_some() || get(FO, "text-indent").is_some() => {}
                    None => {
                        mar_l = margin_left + before + label;
                        indent = text_indent - label;
                    }
                }
                bullet = bullet_xml(cx, &l);
            } else if cx.st.list_style(list_style).is_some() {
                // A list style with no definition of the level (PowerPoint writes `<text:list-style style:name="a1030"/>` for the lists of its
                // paragraphs with no bullet) has no bullet: PowerPoint shows none for it.
                bullet = "<a:buNone/>".to_string();
            } else {
                bullet = r#"<a:buChar char="•"/>"#.to_string();
            }
        }
        None => {
            if body.placeholder_body {
                bullet = "<a:buNone/>".to_string();
            }
        }
    }
    if mar_l != 0 || list.is_some() {
        let _ = write!(attrs, r#" marL="{mar_l}""#);
    }
    if let Some(r) = margin_right.filter(|r| *r != 0) {
        let _ = write!(attrs, r#" marR="{r}""#);
    }
    if indent != 0 || list.is_some() {
        let _ = write!(attrs, r#" indent="{indent}""#);
    }
    // `style:tab-stop-distance` (ODF 1.2 20.347, the default tab interval) is `defTabSz`; PowerPoint saves an ODP's 0.5in as 457200.
    if let Some(d) = get(STYLE, "tab-stop-distance").and_then(emu).filter(|d| *d > 0) {
        let _ = write!(attrs, r#" defTabSz="{d}""#);
    }
    if let Some(h) = get(FO, "line-height") {
        if let Some(p) = percent(h) {
            let _ = write!(inner, r#"<a:lnSpc><a:spcPct val="{}"/></a:lnSpc>"#, (p * 100_000.0).round() as i64);
        } else if let Some(pt) = points(h) {
            // `fo:line-height="0in"` is `a:spcPts val="0"` too (PowerPoint reads it so).
            let _ = write!(inner, r#"<a:lnSpc><a:spcPts val="{}"/></a:lnSpc>"#, (pt * 100.0).round() as i64);
        }
    } else if let Some(pt) = get(STYLE, "line-height-at-least").and_then(points).filter(|p| *p > 0.0) {
        let _ = write!(inner, r#"<a:lnSpc><a:spcPts val="{}"/></a:lnSpc>"#, (pt * 100.0).round() as i64);
    }
    if let Some(pt) = get(FO, "margin-top").and_then(points) {
        let _ = write!(inner, r#"<a:spcBef><a:spcPts val="{}"/></a:spcBef>"#, (pt * 100.0).round() as i64);
    }
    if let Some(pt) = get(FO, "margin-bottom").and_then(points) {
        let _ = write!(inner, r#"<a:spcAft><a:spcPts val="{}"/></a:spcAft>"#, (pt * 100.0).round() as i64);
    }
    inner += &bullet;
    if inner.is_empty() { format!("<a:pPr{attrs}/>") } else { format!("<a:pPr{attrs}>{inner}</a:pPr>") }
}

/// The bullet or numbering of a list level, as DrawingML bullet elements.
fn bullet_xml(cx: &Cx, level: &Level) -> String {
    let node = level.node;
    let mut s = String::new();
    let props = node.children().find(|c| is(*c, STYLE, "text-properties"));
    if let Some(p) = props {
        if attr(p, STYLE, "use-window-font-color") != Some("true") {
            if let Some(c) = attr(p, FO, "color").and_then(color) {
                let _ = write!(s, r#"<a:buClr><a:srgbClr val="{c}"/></a:buClr>"#);
            }
        }
        if let Some(size) = attr(p, FO, "font-size").and_then(percent) {
            let _ = write!(s, r#"<a:buSzPct val="{}"/>"#, (size * 100_000.0).round().clamp(25_000.0, 400_000.0) as i64);
        }
    }
    let kind = node.tag_name().name();
    let font = props.and_then(|p| attr(p, FO, "font-family").or_else(|| attr(p, STYLE, "font-name")));
    let font = font.map(|f| cx.st.font_family(f.trim_matches('\''))).filter(|f| f != "OpenSymbol" && !f.is_empty());
    match kind {
        "list-level-style-bullet" => {
            if let Some(f) = &font {
                let _ = write!(s, r#"<a:buFont typeface="{}"/>"#, esc(f));
            }
            let ch = attr(node, TEXT, "bullet-char").filter(|c| !c.is_empty()).unwrap_or("•");
            let _ = write!(s, r#"<a:buChar char="{}"/>"#, esc(ch));
        }
        "list-level-style-number" => {
            let format = attr(node, STYLE, "num-format").unwrap_or("1");
            let prefix = attr(node, STYLE, "num-prefix").unwrap_or("");
            let suffix = attr(node, STYLE, "num-suffix").unwrap_or("");
            let base = match format {
                "a" => "alphaLc",
                "A" => "alphaUc",
                "i" => "romanLc",
                "I" => "romanUc",
                "" => "",
                _ => "arabic",
            };
            if base.is_empty() {
                s += "<a:buNone/>";
            } else {
                let tail = match (prefix, suffix) {
                    ("(", ")") => "ParenBoth",
                    (_, ")") => "ParenR",
                    (_, ".") => "Period",
                    _ => "Plain",
                };
                let start = attr(node, TEXT, "start-value").and_then(|v| v.parse::<u32>().ok()).filter(|v| *v > 1);
                match start {
                    Some(n) => {
                        let _ = write!(s, r#"<a:buAutoNum type="{base}{tail}" startAt="{n}"/>"#);
                    }
                    None => {
                        let _ = write!(s, r#"<a:buAutoNum type="{base}{tail}"/>"#);
                    }
                }
            }
        }
        "list-level-style-image" => s += r#"<a:buChar char="•"/>"#,
        _ => s += "<a:buNone/>",
    }
    s
}

/// The text of a text content node as runs, appended to `out`.
fn runs(cx: &mut Cx, node: Node, chain: &Chain, out: &mut String, link: Option<&str>) {
    for child in node.children() {
        if child.is_text() {
            let text = child.text().unwrap_or("");
            if !text.is_empty() {
                // (Inside a paragraph, white space collapses to one space, as ODF says.)
                let collapsed = collapse(text);
                if !collapsed.is_empty() {
                    let r = rpr(cx, chain, "rPr", link, &collapsed);
                    let _ = write!(out, "<a:r>{r}<a:t>{}</a:t></a:r>", esc(&collapsed));
                }
            }
            continue;
        }
        if !child.is_element() || child.tag_name().namespace() != Some(TEXT) && child.tag_name().namespace() != Some(PRESENTATION) {
            continue;
        }
        match (child.tag_name().namespace().unwrap_or(""), child.tag_name().name()) {
            (TEXT, "span") => {
                let mut c = chain.clone();
                if let Some(name) = attr(child, TEXT, "style-name") {
                    c.push(("text", name.to_string()));
                }
                runs(cx, child, &c, out, link);
            }
            (TEXT, "a") => {
                let id = attr(child, XLINK, "href").and_then(|href| cx.link_rel(href));
                let mut c = chain.clone();
                if let Some(name) = attr(child, TEXT, "style-name") {
                    c.push(("text", name.to_string()));
                }
                runs(cx, child, &c, out, id.as_deref());
            }
            (TEXT, "s") => {
                let count = attr(child, TEXT, "c").and_then(|c| c.parse::<usize>().ok()).unwrap_or(1);
                let r = rpr(cx, chain, "rPr", link, "");
                let _ = write!(out, "<a:r>{r}<a:t>{}</a:t></a:r>", " ".repeat(count));
            }
            (TEXT, "tab") => {
                let r = rpr(cx, chain, "rPr", link, "");
                let _ = write!(out, "<a:r>{r}<a:t>\t</a:t></a:r>");
            }
            (TEXT, "line-break") => {
                let r = rpr(cx, chain, "rPr", link, "");
                let _ = write!(out, "<a:br>{r}</a:br>");
            }
            (TEXT, "page-number") | (PRESENTATION, "") => {
                let r = rpr(cx, chain, "rPr", link, "");
                let _ = write!(out, r#"<a:fld id="{{B6F15528-21DE-4FAA-801E-634DDDAF4B2B}}" type="slidenum">{r}<a:t>‹#›</a:t></a:fld>"#);
            }
            (TEXT, "date" | "time") | (PRESENTATION, "date-time") => {
                let r = rpr(cx, chain, "rPr", link, "");
                let shown: String = child.descendants().filter(|d| d.is_text()).filter_map(|d| d.text()).collect();
                let _ = write!(out, r#"<a:fld id="{{B6F15528-21DE-4FAA-801E-634DDDAF4B2C}}" type="datetime1">{r}<a:t>{}</a:t></a:fld>"#, esc(&shown));
            }
            (TEXT, "bookmark" | "bookmark-start" | "bookmark-end" | "soft-page-break" | "note" | "annotation") => {}
            _ => runs(cx, child, chain, out, link),
        }
    }
}

/// ODF white-space handling: runs of blanks (and line ends) are one space.
fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = false;
    for c in text.chars() {
        if matches!(c, ' ' | '\t' | '\r' | '\n') {
            if !blank {
                out.push(' ');
            }
            blank = true;
        } else {
            out.push(c);
            blank = false;
        }
    }
    out
}

fn paragraph(cx: &mut Cx, p: Node, base: &Chain, list: Option<(&str, usize)>, body: Body, out: &mut String) {
    let mut chain = base.clone();
    if let Some(name) = attr(p, TEXT, "style-name") {
        chain.push(("paragraph", name.to_string()));
    }
    let ppr = ppr(cx, &chain, list, body);
    let mut content = String::new();
    runs(cx, p, &chain, &mut content, None);
    // Leading and trailing blanks of the paragraph go.
    // PowerPoint writes the paragraph mark's formatting (a:endParaRPr) as a final empty span; reading the ODP back it takes the mark's
    // size and font from that span (an empty paragraph of 28 pt, `<text:p><text:span style a/></text:p>` with a's size 28 pt, is 28 pt high:
    // PowerPoint's pptx of the ODP has endParaRPr sz=2800, ours had the paragraph style's 18 pt).
    let mut end_chain = chain.clone();
    if let Some(last) = p.children().rev().find(|c| c.is_element()) {
        if is(last, TEXT, "span") && last.children().next().is_none() {
            if let Some(name) = attr(last, TEXT, "style-name") {
                end_chain.push(("text", name.to_string()));
            }
        }
    }
    let end = rpr(cx, &end_chain, "endParaRPr", None, "");
    let _ = write!(out, "<a:p>{ppr}{content}{end}</a:p>");
}

fn list(cx: &mut Cx, l: Node, base: &Chain, parent: Option<(&str, usize)>, body: Body, out: &mut String, frame_list: Option<&str>) {
    let style = attr(l, TEXT, "style-name").map(str::to_string).or_else(|| parent.map(|(s, _)| s.to_string())).or_else(|| frame_list.map(str::to_string)).unwrap_or_default();
    let level = parent.map_or(0, |(_, lv)| lv + 1);
    for item in l.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(TEXT)) {
        match item.tag_name().name() {
            "list-item" | "list-header" => {
                let numbered = item.tag_name().name() == "list-item";
                for c in item.children().filter(|c| c.is_element() && c.tag_name().namespace() == Some(TEXT)) {
                    match c.tag_name().name() {
                        "p" | "h" => paragraph(cx, c, base, if numbered { Some((style.as_str(), level)) } else { None }, body, out),
                        "list" => list(cx, c, base, Some((style.as_str(), level)), body, out, frame_list),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// The paragraphs (`a:p`) of a text container (`draw:text-box`, a shape, a table cell). `frame` is the chain of the styles of
/// the object (graphic, presentation or table cell styles), then its default text style.
pub(crate) fn paragraphs(cx: &mut Cx, container: Node, frame: &[(&'static str, String)], body: Body) -> String {
    let base: Chain = frame.to_vec();
    let frame_list = frame.iter().find_map(|(f, n)| cx.st.prop(f, n, STYLE, "list-style-name")).map(str::to_string);
    let mut out = String::new();
    // Sections and legacy numbered paragraphs may wrap the text of a text box or a notes frame. Keep their content in order, without
    // evaluating conditions.
    for c in crate::odf::through(container, |node| crate::odf::visible_section(node) || node.has_tag_name((TEXT, "numbered-paragraph"))).filter(|node| node.tag_name().namespace() == Some(TEXT)) {
        match c.tag_name().name() {
            "p" | "h" => paragraph(cx, c, &base, None, body, &mut out),
            "list" => list(cx, c, &base, None, body, &mut out, frame_list.as_deref()),
            _ => {}
        }
    }
    if out.is_empty() {
        let end = rpr(cx, &base, "endParaRPr", None, "");
        let _ = write!(out, "<a:p>{end}</a:p>");
    }
    out
}

/// Whether the container has any text at all.
pub(crate) fn has_text(container: Node) -> bool {
    container.descendants().any(|d| d.is_text() && d.text().is_some_and(|t| !t.trim().is_empty()))
}
