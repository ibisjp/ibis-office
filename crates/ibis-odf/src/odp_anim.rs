// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The animations of an OpenDocument presentation ([ODF 1.3] part 1, chapter 14: SMIL animation, `anim:*`) as the `p:timing`
//! of the slide. The two describe the same tree of timed nodes: parallel and sequential containers around the behaviors (set,
//! animate, transition filter, motion, transform, colour, command) that act on the shapes.

use std::collections::HashMap;
use std::fmt::Write as _;

use roxmltree::Node;

use crate::odp::Cx;
use crate::odp_style::{PRESENTATION, SMIL, SVG, attr, is};
use crate::odp_text::esc;

pub(crate) const ANIM: &str = "urn:oasis:names:tc:opendocument:xmlns:animation:1.0";

/// The state while a tree is written.
struct Tree<'c, 's, 'a, 'i> {
    cx: &'c mut Cx<'s, 'a, 'i>,
    next_id: u32,
    /// The ids of the nodes that have an `xml:id`, for `begin` and `end` that name them.
    node_ids: HashMap<String, u32>,
}

/// "0.5s", "500ms", "indefinite", "00:00:01.5" as milliseconds (`None` for indefinite).
fn clock(text: &str) -> Option<u64> {
    let t = text.trim();
    if let Some(v) = t.strip_suffix("ms") {
        return v.trim().parse::<f64>().ok().map(|v| v.round() as u64);
    }
    if let Some(v) = t.strip_suffix('s') {
        return v.trim().parse::<f64>().ok().map(|v| (v * 1000.0).round() as u64);
    }
    if let Some(v) = t.strip_suffix("min") {
        return v.trim().parse::<f64>().ok().map(|v| (v * 60_000.0).round() as u64);
    }
    if t.contains(':') {
        let parts: Vec<f64> = t.split(':').filter_map(|p| p.parse::<f64>().ok()).collect();
        let secs = parts.iter().fold(0.0, |acc, p| acc * 60.0 + p);
        return Some((secs * 1000.0).round() as u64);
    }
    t.parse::<f64>().ok().map(|v| (v * 1000.0).round() as u64)
}

/// The OOXML preset id of an ODF preset ("ooo-entrance-fade-in").
fn preset_id(name: &str) -> Option<u32> {
    let kind_free = name.strip_prefix("ooo-entrance-").or_else(|| name.strip_prefix("ooo-exit-")).or_else(|| name.strip_prefix("ooo-emphasis-"));
    let n = kind_free?;
    let n = n.trim_end_matches("-slow");
    Some(match n {
        "appear" | "disappear" => 1,
        "fly-in" | "fly-out" => 2,
        "venetian-blinds" | "blinds" => 3,
        "box" => 4,
        "checkerboard" => 5,
        "circle" => 6,
        "crawl-in" | "crawl-out" => 7,
        "diamond" => 8,
        "dissolve-in" | "dissolve-out" => 9,
        "fade-in" | "fade-out" => 10,
        "flash-once" => 11,
        "peek-in" | "peek-out" => 12,
        "plus" => 13,
        "random-bars" => 14,
        "spiral-in" | "spiral-out" => 15,
        "split" => 16,
        "stretch" => 17,
        "strips" => 18,
        "swivel" => 19,
        "wedge" => 20,
        "wheel" => 21,
        "wipe" => 22,
        "zoom-in" | "zoom-out" => 23,
        // Emphasis effects.
        "spin" => 8,
        "grow-and-shrink" => 6,
        "change-fill-color" => 7,
        "change-font-color" => 3,
        "transparency" => 9,
        "pulse" => 26,
        "teeter" => 32,
        _ => return None,
    })
}

/// The DrawingML filter of a transition filter's type and subtype.
fn filter_of(kind: &str, sub: &str, reverse: bool) -> String {
    match kind {
        "fade" => "fade".to_string(),
        "barWipe" => format!("wipe({})", match (sub, reverse) {
            ("topToBottom", false) => "up",
            ("topToBottom", true) => "down",
            (_, false) => "left",
            (_, true) => "right",
        }),
        "blindsWipe" => format!("blinds({})", if sub == "vertical" { "vertical" } else { "horizontal" }),
        "checkerBoardWipe" => format!("checkerboard({})", if sub == "down" { "down" } else { "across" }),
        "randomBarWipe" => format!("randombar({})", if sub == "vertical" { "vertical" } else { "horizontal" }),
        "irisWipe" => format!("box({})", if reverse { "in" } else { "out" }),
        "ellipseWipe" => format!("circle({})", if reverse { "in" } else { "out" }),
        "pinWheelWipe" | "fanWipe" | "clockWipe" => "wheel(1)".to_string(),
        "dissolve" => "dissolve".to_string(),
        "pushWipe" | "slideWipe" => "fade".to_string(),
        _ => "fade".to_string(),
    }
}

/// An attribute name of ODF animations as DrawingML's.
fn attr_name(name: &str) -> String {
    match name.trim() {
        "x" => "ppt_x",
        "y" => "ppt_y",
        "width" => "ppt_w",
        "height" => "ppt_h",
        "rotate" => "r",
        "visibility" => "style.visibility",
        "opacity" | "Opacity" => "style.opacity",
        "fill-color" | "FillColor" => "fillcolor",
        "color" | "CharColor" => "style.color",
        "fill" => "fill.on",
        other => other,
    }
    .to_string()
}

/// A value of an animation: the names of the position and size are `#ppt_…` in DrawingML.
fn value_of(text: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if word.is_empty() {
            return;
        }
        match word.as_str() {
            "x" | "y" => {
                let _ = write!(out, "#ppt_{word}");
            }
            "width" => out.push_str("#ppt_w"),
            "height" => out.push_str("#ppt_h"),
            w => out.push_str(w),
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

impl Tree<'_, '_, '_, '_> {
    fn id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn spid(&self, element: &str) -> Option<u32> {
        self.cx.shape_ids.get(element.trim_start_matches('#')).copied()
    }

    /// The conditions `begin` or `end` give (`p:stCondLst` content): a delay, a click or another node's start or end.
    fn conditions(&self, text: &str) -> String {
        let mut out = String::new();
        for part in text.split(';').map(str::trim).filter(|p| !p.is_empty()) {
            if part == "next" || part == "indefinite" {
                out += r#"<p:cond delay="indefinite"/>"#;
            } else if let Some(ms) = clock(part) {
                let _ = write!(out, r#"<p:cond delay="{ms}"/>"#);
            } else if let Some((who, event)) = part.split_once('.') {
                let (event, offset) = match event.split_once('+') {
                    Some((e, o)) => (e, clock(o).unwrap_or(0)),
                    None => (event, 0),
                };
                match (event, self.node_ids.get(who), self.spid(who)) {
                    ("click", _, Some(spid)) => {
                        let _ = write!(out, r#"<p:cond evt="onClick" delay="{offset}"><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl></p:cond>"#);
                    }
                    ("begin" | "end", Some(id), _) => {
                        let _ = write!(out, r#"<p:cond evt="{event}" delay="{offset}"><p:tn val="{id}"/></p:cond>"#);
                    }
                    _ => {}
                }
            }
        }
        out
    }

    /// The `p:cTn` of a node. `dur` is the node's duration in milliseconds when the node does not say it.
    fn ctn(&mut self, n: Node, id: u32, inner: &str, default_dur: Option<&str>, node_type: Option<&str>) -> String {
        let mut s = format!(r#"<p:cTn id="{id}""#);
        if let Some(p) = attr(n, PRESENTATION, "preset-id").and_then(preset_id) {
            let class = match attr(n, PRESENTATION, "preset-class") {
                Some("entrance") => "entr",
                Some("exit") => "exit",
                Some("emphasis") => "emph",
                Some("motion-path") => "path",
                _ => "",
            };
            let _ = write!(s, r#" presetID="{p}""#);
            if !class.is_empty() {
                let _ = write!(s, r#" presetClass="{class}""#);
            }
            let _ = write!(s, r#" presetSubtype="{}""#, attr(n, PRESENTATION, "preset-sub-type").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0));
        }
        match attr(n, SMIL, "fill") {
            Some("hold") => s += r#" fill="hold""#,
            Some("remove") => s += r#" fill="remove""#,
            Some("freeze") => s += r#" fill="freeze""#,
            _ => {}
        }
        match attr(n, SMIL, "restart") {
            Some("always") => s += r#" restart="always""#,
            Some("whenNotActive") => s += r#" restart="whenNotActive""#,
            Some("never") => s += r#" restart="never""#,
            _ => {}
        }
        if let Some(d) = attr(n, SMIL, "dur").or(default_dur) {
            match clock(d) {
                Some(ms) if d != "indefinite" => {
                    let _ = write!(s, r#" dur="{ms}""#);
                }
                _ => s += r#" dur="indefinite""#,
            }
        }
        match attr(n, SMIL, "repeatCount") {
            Some("indefinite") => s += r#" repeatCount="indefinite""#,
            Some(c) => {
                if let Ok(v) = c.parse::<f64>() {
                    let _ = write!(s, r#" repeatCount="{}""#, (v * 1000.0).round() as u64);
                }
            }
            None => {}
        }
        if let Some(a) = attr(n, SMIL, "accelerate").and_then(|v| v.parse::<f64>().ok()) {
            let _ = write!(s, r#" accel="{}""#, (a * 100_000.0).round() as u64);
        }
        if let Some(d) = attr(n, SMIL, "decelerate").and_then(|v| v.parse::<f64>().ok()) {
            let _ = write!(s, r#" decel="{}""#, (d * 100_000.0).round() as u64);
        }
        if attr(n, SMIL, "autoReverse") == Some("true") {
            s += r#" autoRev="1""#;
        }
        let kind = node_type.or_else(|| match attr(n, PRESENTATION, "node-type") {
            Some("timing-root") => Some("tmRoot"),
            Some("main-sequence") => Some("mainSeq"),
            Some("interactive-sequence") => Some("interactiveSeq"),
            Some("on-click") => Some("clickEffect"),
            Some("with-previous") => Some("withEffect"),
            Some("after-previous") => Some("afterEffect"),
            _ => None,
        });
        if let Some(k) = kind {
            let _ = write!(s, r#" nodeType="{k}""#);
        }
        s.push('>');
        let begin = attr(n, SMIL, "begin").map(|b| self.conditions(b)).unwrap_or_default();
        if !begin.is_empty() {
            let _ = write!(s, "<p:stCondLst>{begin}</p:stCondLst>");
        }
        if let Some(end) = attr(n, SMIL, "end").map(|b| self.conditions(b)).filter(|e| !e.is_empty()) {
            let _ = write!(s, "<p:endCondLst>{end}</p:endCondLst>");
        }
        if !inner.is_empty() {
            let _ = write!(s, "<p:childTnLst>{inner}</p:childTnLst>");
        }
        s += "</p:cTn>";
        s
    }

    /// `p:cBhvr`: the behavior's own time node, its target and the attribute it acts on.
    fn behavior(&mut self, n: Node, id: u32) -> Option<String> {
        let target = attr(n, SMIL, "targetElement").and_then(|t| self.spid(t))?;
        let mut s = String::from("<p:cBhvr");
        if attr(n, SMIL, "additive") == Some("sum") {
            s += r#" additive="sum""#;
        }
        s.push('>');
        s += &self.ctn(n, id, "", Some("500ms"), None);
        let _ = write!(s, r#"<p:tgtEl><p:spTgt spid="{target}"/></p:tgtEl>"#);
        if let Some(names) = attr(n, SMIL, "attributeName") {
            let list: String = names.split([';', ',']).filter(|x| !x.trim().is_empty()).map(|x| format!("<p:attrName>{}</p:attrName>", esc(&attr_name(x)))).collect();
            if !list.is_empty() {
                let _ = write!(s, "<p:attrNameLst>{list}</p:attrNameLst>");
            }
        }
        s += "</p:cBhvr>";
        Some(s)
    }

    fn leaf(&mut self, n: Node) -> Option<String> {
        let id = self.id();
        if let Some(xml_id) = n.attribute(("http://www.w3.org/XML/1998/namespace", "id")) {
            self.node_ids.insert(xml_id.to_string(), id);
        }
        let cbhvr = self.behavior(n, id)?;
        Some(match n.tag_name().name() {
            "set" => {
                let to = attr(n, SMIL, "to").unwrap_or("visible");
                format!(r#"<p:set>{cbhvr}<p:to><p:strVal val="{}"/></p:to></p:set>"#, esc(to))
            }
            "transitionFilter" => {
                let reverse = attr(n, SMIL, "direction") == Some("reverse");
                let filter = filter_of(attr(n, SMIL, "type").unwrap_or("fade"), attr(n, SMIL, "subtype").unwrap_or(""), reverse);
                let transition = if attr(n, SMIL, "mode") == Some("out") { "out" } else { "in" };
                format!(r#"<p:animEffect transition="{transition}" filter="{}">{cbhvr}</p:animEffect>"#, esc(&filter))
            }
            "animate" => {
                let calc = match attr(n, SMIL, "calcMode") {
                    Some("discrete") => "discrete",
                    _ => "lin",
                };
                let names = attr(n, SMIL, "attributeName").unwrap_or("");
                let is_color = names.contains("color") || names.contains("Color");
                let value_type = if is_color { "clr" } else if names.contains("visibility") || names == "fill" { "str" } else { "num" };
                let mut tav = String::new();
                if let Some(values) = attr(n, SMIL, "values") {
                    let list: Vec<&str> = values.split(';').collect();
                    let times: Vec<f64> = attr(n, SMIL, "keyTimes").map(|k| k.split(';').filter_map(|t| t.trim().parse::<f64>().ok()).collect()).unwrap_or_default();
                    for (i, v) in list.iter().enumerate() {
                        let tm = times.get(i).copied().unwrap_or(if list.len() > 1 { i as f64 / (list.len() - 1) as f64 } else { 0.0 });
                        let _ = write!(tav, r#"<p:tav tm="{}"><p:val><p:strVal val="{}"/></p:val></p:tav>"#, (tm * 100_000.0).round() as u64, esc(&value_of(v.trim())));
                    }
                } else {
                    // From and to (by as the second value of a movement from the first).
                    let (from, to) = (attr(n, SMIL, "from"), attr(n, SMIL, "to"));
                    if let (Some(a), Some(b)) = (from, to) {
                        let _ = write!(tav, r#"<p:tav tm="0"><p:val><p:strVal val="{}"/></p:val></p:tav><p:tav tm="100000"><p:val><p:strVal val="{}"/></p:val></p:tav>"#, esc(&value_of(a)), esc(&value_of(b)));
                    }
                }
                let tav = if tav.is_empty() { String::new() } else { format!("<p:tavLst>{tav}</p:tavLst>") };
                let by = attr(n, SMIL, "by").map(|b| format!(r#" by="{}""#, esc(&value_of(b)))).unwrap_or_default();
                format!(r#"<p:anim calcmode="{calc}" valueType="{value_type}"{by}>{cbhvr}{tav}</p:anim>"#)
            }
            "animateMotion" => {
                let path = attr(n, SVG, "path").unwrap_or("");
                format!(r#"<p:animMotion origin="layout" path="{}" pathEditMode="relative">{cbhvr}</p:animMotion>"#, esc(path))
            }
            "animateColor" => {
                let color = |v: &str| crate::odp_style::color(v).map(|c| format!(r#"<a:srgbClr val="{c}"/>"#));
                let mut s = format!(r#"<p:animClr clrSpc="rgb">{cbhvr}"#);
                if let Some(c) = attr(n, SMIL, "from").and_then(color) {
                    let _ = write!(s, "<p:from>{c}</p:from>");
                }
                if let Some(c) = attr(n, SMIL, "to").and_then(color) {
                    let _ = write!(s, "<p:to>{c}</p:to>");
                }
                s + "</p:animClr>"
            }
            "animateTransform" => {
                let kind = attr(n, SVG, "type").unwrap_or("scale");
                let number = |name: &str| attr(n, SMIL, name).and_then(|v| v.split([',', ' ']).next().and_then(|t| t.parse::<f64>().ok()));
                match kind {
                    "rotate" => {
                        let angle = |v: Option<f64>| v.map(|a| (a * 60_000.0).round() as i64);
                        let mut attrs = String::new();
                        if let Some(b) = angle(number("by")) {
                            let _ = write!(attrs, r#" by="{b}""#);
                        }
                        if let Some(b) = angle(number("from")) {
                            let _ = write!(attrs, r#" from="{b}""#);
                        }
                        if let Some(b) = angle(number("to")) {
                            let _ = write!(attrs, r#" to="{b}""#);
                        }
                        format!("<p:animRot{attrs}>{cbhvr}</p:animRot>")
                    }
                    _ => {
                        let pair = |name: &str| {
                            let v = attr(n, SMIL, name)?;
                            let parts: Vec<f64> = v.split([',', ' ']).filter_map(|t| t.trim().parse::<f64>().ok()).collect();
                            let x = *parts.first()?;
                            Some((x, parts.get(1).copied().unwrap_or(x)))
                        };
                        let point = |tag: &str, p: Option<(f64, f64)>| p.map(|(x, y)| format!(r#"<p:{tag} x="{}" y="{}"/>"#, (x * 100_000.0).round() as i64, (y * 100_000.0).round() as i64)).unwrap_or_default();
                        format!("<p:animScale>{cbhvr}{}{}{}</p:animScale>", point("by", pair("by")), point("from", pair("from")), point("to", pair("to")))
                    }
                }
            }
            "command" => {
                let cmd = match attr(n, ANIM, "command") {
                    Some("play") => "playFrom(0.0)",
                    Some("stop") => "stop",
                    Some("toggle-pause") => "togglePause",
                    Some(other) => other,
                    None => "playFrom(0.0)",
                };
                format!(r#"<p:cmd type="call" cmd="{}">{cbhvr}</p:cmd>"#, esc(cmd))
            }
            _ => return None,
        })
    }

    fn node(&mut self, n: Node) -> Option<String> {
        if n.tag_name().namespace() != Some(ANIM) {
            return None;
        }
        match n.tag_name().name() {
            "par" | "seq" | "iterate" | "excl" => {
                let id = self.id();
                if let Some(xml_id) = n.attribute(("http://www.w3.org/XML/1998/namespace", "id")) {
                    self.node_ids.insert(xml_id.to_string(), id);
                }
                let mut inner = String::new();
                for c in n.children().filter(|c| c.is_element()) {
                    if let Some(x) = self.node(c) {
                        inner += &x;
                    }
                }
                let seq = n.tag_name().name() == "seq";
                let ctn = self.ctn(n, id, &inner, None, None);
                Some(if seq {
                    let main = attr(n, PRESENTATION, "node-type") == Some("main-sequence");
                    let (prev, next) = if main {
                        (r#"<p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst>"#, r#"<p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst>"#)
                    } else {
                        ("", "")
                    };
                    format!(r#"<p:seq concurrent="1" nextAc="seek">{ctn}{prev}{next}</p:seq>"#)
                } else {
                    format!("<p:par>{ctn}</p:par>")
                })
            }
            _ => self.leaf(n),
        }
    }
}

/// The `p:timing` of a page, from its `anim:par` of type timing-root; empty when the page animates nothing.
pub(crate) fn timing(cx: &mut Cx, page: Node) -> String {
    let Some(root) = page.children().find(|c| is(*c, ANIM, "par") && attr(*c, PRESENTATION, "node-type") == Some("timing-root")) else { return String::new() };
    // Only a main or an interactive sequence animates shapes (a root with one transition filter is a slide transition).
    let has_sequence = root.descendants().any(|d| is(d, ANIM, "seq") && matches!(attr(d, PRESENTATION, "node-type"), Some("main-sequence" | "interactive-sequence")));
    if !has_sequence {
        return String::new();
    }
    let mut tree = Tree { cx, next_id: 1, node_ids: HashMap::new() };
    match tree.node(root) {
        Some(x) => format!("<p:timing><p:tnLst>{x}</p:tnLst></p:timing>"),
        None => String::new(),
    }
}
