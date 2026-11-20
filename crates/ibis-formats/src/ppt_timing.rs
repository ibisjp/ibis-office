// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The animation of a slide in a PowerPoint 97-2003 file ([MS-PPT] section 2.8): a tree of time nodes
//! (`ExtTimeNodeContainer`) in the slide's `___PPT10` binary tag, written as the `p:timing` of the new package.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use crate::ppt::{Rec, records};

const EXT_TIME_NODE: u16 = 0xF144;
const TIME_NODE_ATOM: u16 = 0xF127;
const TIME_CONDITION_CONTAINER: u16 = 0xF125;
const TIME_CONDITION_ATOM: u16 = 0xF128;
const TIME_MODIFIER: u16 = 0xF129;
const TIME_BEHAVIOR_CONTAINER: u16 = 0xF12A;
const TIME_BEHAVIOR_ATOM: u16 = 0xF133;
const TIME_PROPERTY_LIST: u16 = 0xF13D;
const TIME_VARIANT: u16 = 0xF142;
const TIME_VARIANT_LIST: u16 = 0xF13E;
const CLIENT_VISUAL_ELEMENT: u16 = 0xF13C;
const TIME_ITERATE_DATA: u16 = 0xF140;
const TIME_SEQUENCE_DATA: u16 = 0xF141;
const ANIMATION_VALUE_LIST: u16 = 0xF13F;
const ANIMATION_VALUE: u16 = 0xF143;
const VISUAL_PAGE: u16 = 0x2B01;
const VISUAL_SHAPE: u16 = 0x2AFB;
const BEHAVIOR_ANIMATE: u16 = 0xF12B;
const BEHAVIOR_COLOR: u16 = 0xF12C;
const BEHAVIOR_EFFECT: u16 = 0xF12D;
const BEHAVIOR_MOTION: u16 = 0xF12E;
const BEHAVIOR_ROTATION: u16 = 0xF12F;
const BEHAVIOR_SCALE: u16 = 0xF130;
const BEHAVIOR_SET: u16 = 0xF131;
const BEHAVIOR_COMMAND: u16 = 0xF132;

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

fn f32_at(d: &[u8], at: usize) -> Option<f64> {
    Some(f64::from(f32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?)))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A value of a time variant record.
#[derive(Clone, Debug, PartialEq)]
enum Variant {
    Bool(bool),
    Int(i32),
    Float(f64),
    Text(String),
}

fn variant(r: &Rec) -> Option<Variant> {
    let d = r.data;
    match *d.first()? {
        0 => Some(Variant::Bool(*d.get(1)? != 0)),
        1 => Some(Variant::Int(u32_at(d, 1)? as i32)),
        2 => Some(Variant::Float(f32_at(d, 1)?)),
        3 => {
            let units: Vec<u16> = d.get(1..)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|u| *u != 0).collect();
            Some(Variant::Text(String::from_utf16_lossy(&units)))
        }
        _ => None,
    }
}

fn variant_xml(v: &Variant) -> String {
    match v {
        Variant::Bool(b) => format!(r#"<p:boolVal val="{}"/>"#, u8::from(*b)),
        Variant::Int(i) => format!(r#"<p:intVal val="{i}"/>"#),
        Variant::Float(f) => format!(r#"<p:fltVal val="{f}"/>"#),
        Variant::Text(t) => format!(r#"<p:strVal val="{}"/>"#, esc(t)),
    }
}

fn text_of(r: &Rec) -> Option<String> {
    match variant(r)? {
        Variant::Text(t) => Some(t),
        _ => None,
    }
}

/// The time node's attributes.
#[derive(Default)]
struct Node {
    restart: u32,
    kind: u32,
    fill: u32,
    duration: u32,
    flags: u32,
    props: Vec<(u16, Variant)>,
}

impl Node {
    fn prop(&self, id: u16) -> Option<&Variant> {
        self.props.iter().find(|(p, _)| *p == id).map(|(_, v)| v)
    }

    fn int(&self, id: u16) -> Option<i32> {
        match self.prop(id)? {
            Variant::Int(i) => Some(*i),
            Variant::Bool(b) => Some(i32::from(*b)),
            _ => None,
        }
    }
}

struct Builder {
    next_id: u32,
    /// Shapes whose text is animated, with their build group: for the build list.
    text_builds: BTreeSet<(u32, u32)>,
}

/// The visual element a record names, as `p:tgtEl` content (empty when it is not one that can be written).
fn target(element: &Rec, b: &mut Builder, group: Option<u32>) -> Option<String> {
    for r in element.children() {
        match r.ty {
            VISUAL_PAGE => return Some("<p:sldTgt/>".to_string()),
            VISUAL_SHAPE => {
                let kind = u32_at(r.data, 0)?;
                let ref_type = u32_at(r.data, 4)?;
                let spid = u32_at(r.data, 8)?;
                let (data1, data2) = (u32_at(r.data, 12)?, u32_at(r.data, 16)?);
                if ref_type == 2 {
                    // A sound: its file is not carried.
                    return None;
                }
                return Some(match kind {
                    2 => {
                        b.text_builds.insert((spid, group.unwrap_or(0)));
                        format!(r#"<p:spTgt spid="{spid}"><p:txEl><p:charRg st="{data1}" end="{data2}"/></p:txEl></p:spTgt>"#)
                    }
                    6 => format!(r#"<p:spTgt spid="{spid}"><p:bg/></p:spTgt>"#),
                    _ => format!(r#"<p:spTgt spid="{spid}"/>"#),
                });
            }
            _ => {}
        }
    }
    None
}

fn condition_event(e: u32) -> Option<&'static str> {
    Some(match e {
        1 => "onBegin",
        3 => "begin",
        4 => "end",
        5 => "onClick",
        7 => "onMouseOver",
        9 => "onNext",
        10 => "onPrev",
        11 => "onStopAudio",
        _ => return None,
    })
}

/// One `p:cond`.
fn condition(c: &Rec, b: &mut Builder) -> Option<String> {
    let atom = c.child(TIME_CONDITION_ATOM)?;
    let (object, event, id, delay) = (u32_at(atom.data, 0)?, u32_at(atom.data, 4)?, u32_at(atom.data, 8)?, u32_at(atom.data, 12)?);
    let mut s = String::from("<p:cond");
    if let Some(e) = condition_event(event) {
        let _ = write!(s, r#" evt="{e}""#);
    }
    if delay == 0xFFFF_FFFF {
        s += r#" delay="indefinite""#;
    } else {
        let _ = write!(s, r#" delay="{}""#, delay as i32);
    }
    s.push('>');
    match object {
        1 => {
            if let Some(el) = c.child(CLIENT_VISUAL_ELEMENT).and_then(|e| target(&e, b, None)) {
                let _ = write!(s, "<p:tgtEl>{el}</p:tgtEl>");
            }
        }
        2 => {
            let _ = write!(s, r#"<p:tn val="{id}"/>"#);
        }
        3 => s += r#"<p:rtn val="all"/>"#,
        _ => {}
    }
    s += "</p:cond>";
    Some(s)
}

fn conditions(node: &Rec, instance: u16, tag: &str, b: &mut Builder) -> String {
    let list: String = node.children().iter().filter(|r| r.ty == TIME_CONDITION_CONTAINER && r.inst == instance).filter_map(|r| condition(r, b)).collect();
    if list.is_empty() { String::new() } else { format!("<p:{tag}>{list}</p:{tag}>") }
}

fn read_node(node: &Rec) -> Node {
    let mut n = Node::default();
    for r in node.children() {
        match r.ty {
            TIME_NODE_ATOM => {
                n.restart = u32_at(r.data, 4).unwrap_or(0);
                n.kind = u32_at(r.data, 8).unwrap_or(0);
                n.fill = u32_at(r.data, 12).unwrap_or(0);
                n.duration = u32_at(r.data, 24).unwrap_or(0);
                n.flags = u32_at(r.data, 28).unwrap_or(0);
            }
            TIME_PROPERTY_LIST => {
                for v in r.children().iter().filter(|v| v.ty == TIME_VARIANT) {
                    if let Some(value) = variant(v) {
                        n.props.push((v.inst, value));
                    }
                }
            }
            _ => {}
        }
    }
    n
}

/// The `p:cTn` of a node; `inner` is what goes in its child list (for the nodes with children).
fn common_time_node(rec: &Rec, n: &Node, id: u32, b: &mut Builder, inner: Option<&str>) -> String {
    let mut s = format!(r#"<p:cTn id="{id}""#);
    if let Some(v) = n.int(9) {
        let _ = write!(s, r#" presetID="{v}""#);
        // The class of the effect: 1 entrance, 2 exit, 3 emphasis, 4 motion path, 5 verb, 6 media.
        let class = match n.int(0x0B) {
            Some(1) => "entr",
            Some(2) => "exit",
            Some(3) => "emph",
            Some(4) => "path",
            Some(5) => "verb",
            Some(6) => "mediacall",
            _ => "",
        };
        if !class.is_empty() {
            let _ = write!(s, r#" presetClass="{class}""#);
        }
        let _ = write!(s, r#" presetSubtype="{}""#, n.int(0x0A).unwrap_or(0));
    }
    let group = n.int(0x13);
    if let Some(g) = group {
        let _ = write!(s, r#" grpId="{g}""#);
    }
    if n.flags & 0x01 != 0 {
        let fill = match n.fill {
            1 | 4 => "remove",
            2 => "freeze",
            _ => "hold",
        };
        let _ = write!(s, r#" fill="{fill}""#);
    }
    if n.flags & 0x02 != 0 {
        let restart = match n.restart {
            1 => "always",
            2 => "whenNotActive",
            _ => "never",
        };
        let _ = write!(s, r#" restart="{restart}""#);
    }
    if n.flags & 0x10 != 0 {
        if n.duration == 0xFFFF_FFFF {
            s += r#" dur="indefinite""#;
        } else {
            let _ = write!(s, r#" dur="{}""#, n.duration);
        }
    }
    for r in rec.children().iter().filter(|r| r.ty == TIME_MODIFIER) {
        let (kind, value) = (u32_at(r.data, 0).unwrap_or(99), u32_at(r.data, 4).unwrap_or(0));
        match kind {
            0 => {
                if value == 0xFFFF_FFFF {
                    s += r#" repeatCount="indefinite""#;
                } else {
                    let _ = write!(s, r#" repeatCount="{value}""#);
                }
            }
            1 => {
                if value == 0xFFFF_FFFF {
                    s += r#" repeatDur="indefinite""#;
                } else {
                    let _ = write!(s, r#" repeatDur="{value}""#);
                }
            }
            2 => {
                let _ = write!(s, r#" spd="{value}""#);
            }
            3 => {
                let _ = write!(s, r#" accel="{value}""#);
            }
            4 => {
                let _ = write!(s, r#" decel="{value}""#);
            }
            5 => {
                let _ = write!(s, r#" autoRev="{}""#, u8::from(value != 0));
            }
            _ => {}
        }
    }
    if let Some(node_type) = n.int(0x14) {
        let name = match node_type {
            1 => "clickEffect",
            2 => "withEffect",
            3 => "afterEffect",
            4 => "mainSeq",
            5 => "interactiveSeq",
            6 => "clickPar",
            7 => "withGroup",
            8 => "afterGroup",
            9 => "tmRoot",
            _ => "",
        };
        if !name.is_empty() {
            let _ = write!(s, r#" nodeType="{name}""#);
        }
    }
    if let Some(Variant::Text(t)) = n.prop(0x10) {
        let _ = write!(s, r#" tmFilter="{}""#, esc(t));
    }
    if let Some(Variant::Text(t)) = n.prop(0x11) {
        let _ = write!(s, r#" evtFilter="{}""#, esc(t));
    }
    if n.int(0x15) == Some(1) {
        s += r#" display="0""#;
    }
    s.push('>');
    s += &conditions(rec, 1, "stCondLst", b);
    s += &conditions(rec, 2, "endCondLst", b);
    if let Some(sync) = rec.children().iter().find(|r| r.ty == TIME_CONDITION_CONTAINER && r.inst == 5).and_then(|r| condition(r, b)) {
        let _ = write!(s, "<p:endSync{}", &sync["<p:cond".len()..sync.len() - "</p:cond>".len()]);
        s += "</p:endSync>";
    }
    if let Some(it) = rec.child(TIME_ITERATE_DATA) {
        let (interval, kind, direction, interval_type) = (u32_at(it.data, 0).unwrap_or(0), u32_at(it.data, 4).unwrap_or(0), u32_at(it.data, 8).unwrap_or(1), u32_at(it.data, 12).unwrap_or(0));
        let t = match kind {
            1 => "wd",
            2 => "lt",
            _ => "el",
        };
        let back = if direction == 0 { r#" backwards="1""# } else { "" };
        let time = if interval_type == 1 { format!(r#"<p:tmPct val="{interval}"/>"#) } else { format!(r#"<p:tmAbs val="{interval}"/>"#) };
        let _ = write!(s, r#"<p:iterate type="{t}"{back}>{time}</p:iterate>"#);
    }
    if let Some(inner) = inner {
        if !inner.is_empty() {
            let _ = write!(s, "<p:childTnLst>{inner}</p:childTnLst>");
        }
    }
    s += "</p:cTn>";
    s
}

/// `p:cBhvr`: the node's own `p:cTn`, the target and the attributes.
fn behavior(rec: &Rec, n: &Node, id: u32, container: &Rec, b: &mut Builder) -> Option<String> {
    let atom = container.child(TIME_BEHAVIOR_ATOM)?;
    let flags = u32_at(atom.data, 0)?;
    let mut s = String::from("<p:cBhvr");
    if flags & 0x01 != 0 && u32_at(atom.data, 4)? == 1 {
        s += r#" additive="sum""#;
    }
    s.push('>');
    s += &common_time_node(rec, n, id, b, None);
    let group = n.int(0x13).map(|g| g as u32);
    let element = container.child(CLIENT_VISUAL_ELEMENT)?;
    let target = target(&element, b, group)?;
    let _ = write!(s, "<p:tgtEl>{target}</p:tgtEl>");
    if let Some(list) = container.child(TIME_VARIANT_LIST) {
        let names: String = list.children().iter().filter(|r| r.ty == TIME_VARIANT).filter_map(text_of).map(|t| format!("<p:attrName>{}</p:attrName>", esc(&t))).collect();
        if !names.is_empty() {
            let _ = write!(s, "<p:attrNameLst>{names}</p:attrNameLst>");
        }
    }
    s += "</p:cBhvr>";
    Some(s)
}

fn point_xml(tag: &str, x: f64, y: f64) -> String {
    format!(r#"<p:{tag} x="{}" y="{}"/>"#, (x * 100_000.0).round() as i64, (y * 100_000.0).round() as i64)
}

fn color_xml(model: u32, a: u32, b: u32, c: u32) -> String {
    match model {
        2 => {
            let name = ["bg1", "tx1", "bg2", "tx2", "accent1", "accent2", "accent3", "accent4"].get(a as usize).copied().unwrap_or("tx1");
            format!(r#"<a:schemeClr val="{name}"/>"#)
        }
        _ => format!(r#"<a:srgbClr val="{:02X}{:02X}{:02X}"/>"#, a.min(255), b.min(255), c.min(255)),
    }
}

/// A leaf node: the behavior it holds, written with its common behavior.
fn behavior_node(rec: &Rec, n: &Node, id: u32, b: &mut Builder) -> Option<String> {
    let container = rec.children().into_iter().find(|r| (BEHAVIOR_ANIMATE..=BEHAVIOR_COMMAND).contains(&r.ty))?;
    let common = container.child(TIME_BEHAVIOR_CONTAINER)?;
    let cbhvr = behavior(rec, n, id, &common, b)?;
    let children = container.children();
    let atom = children.first()?;
    let d = atom.data;
    Some(match container.ty {
        BEHAVIOR_SET => {
            let to = children.iter().find(|r| r.ty == TIME_VARIANT).and_then(variant).map(|v| variant_xml(&v)).unwrap_or_default();
            format!("<p:set>{cbhvr}<p:to>{to}</p:to></p:set>")
        }
        BEHAVIOR_EFFECT => {
            let transition = if u32_at(d, 4)? == 1 { "out" } else { "in" };
            let filter = children.iter().find(|r| r.ty == TIME_VARIANT).and_then(text_of).unwrap_or_default();
            format!(r#"<p:animEffect transition="{transition}" filter="{}">{cbhvr}</p:animEffect>"#, esc(&filter))
        }
        BEHAVIOR_ANIMATE => {
            let calc = match u32_at(d, 0)? {
                0 => "discrete",
                2 => "fmla",
                _ => "lin",
            };
            let flags = u32_at(d, 4)?;
            let value_type = match u32_at(d, 8)? {
                0 => "str",
                2 => "clr",
                _ => "num",
            };
            let mut attrs = format!(r#" calcmode="{calc}" valueType="{value_type}""#);
            // The values by, from and to are the string variants after the value list (in this order).
            let texts: Vec<String> = children.iter().filter(|r| r.ty == TIME_VARIANT).filter_map(text_of).collect();
            let mut it = texts.into_iter();
            if flags & 0x01 != 0 {
                if let Some(v) = it.next() {
                    let _ = write!(attrs, r#" by="{}""#, esc(&v));
                }
            }
            if flags & 0x02 != 0 {
                if let Some(v) = it.next() {
                    let _ = write!(attrs, r#" from="{}""#, esc(&v));
                }
            }
            if flags & 0x04 != 0 {
                if let Some(v) = it.next() {
                    let _ = write!(attrs, r#" to="{}""#, esc(&v));
                }
            }
            let mut tav = String::new();
            if let Some(list) = children.iter().find(|r| r.ty == ANIMATION_VALUE_LIST) {
                let entries = list.children();
                let mut i = 0;
                while i < entries.len() {
                    if entries[i].ty == ANIMATION_VALUE {
                        let time = u32_at(entries[i].data, 0).unwrap_or(0) as i32;
                        let mut value = String::new();
                        let mut formula = String::new();
                        let mut j = i + 1;
                        while j < entries.len() && entries[j].ty == TIME_VARIANT {
                            // The first variant is the value; a following string is the formula.
                            if value.is_empty() {
                                if let Some(v) = variant(&entries[j]) {
                                    value = format!("<p:val>{}</p:val>", variant_xml(&v));
                                }
                            } else if let Some(t) = text_of(&entries[j]) {
                                formula = format!(r#" fmla="{}""#, esc(&t));
                            }
                            j += 1;
                        }
                        let _ = write!(tav, r#"<p:tav tm="{time}"{formula}>{value}</p:tav>"#);
                        i = j;
                    } else {
                        i += 1;
                    }
                }
            }
            let tav = if tav.is_empty() { String::new() } else { format!("<p:tavLst>{tav}</p:tavLst>") };
            format!("<p:anim{attrs}>{cbhvr}{tav}</p:anim>")
        }
        BEHAVIOR_MOTION => {
            let flags = u32_at(d, 0)?;
            let f = |i: usize| f32_at(d, 4 + i * 4).unwrap_or(0.0);
            let origin = if u32_at(d, 28)? == 2 { "parent" } else { "layout" };
            let path = children.iter().find(|r| r.ty == TIME_VARIANT).and_then(text_of);
            let mut s = format!(r#"<p:animMotion origin="{origin}""#);
            if let Some(p) = path {
                let _ = write!(s, r#" path="{}""#, esc(&p));
            }
            s.push('>');
            s += &cbhvr;
            if flags & 0x01 != 0 {
                s += &point_xml("by", f(0), f(1));
            }
            if flags & 0x02 != 0 {
                s += &point_xml("from", f(2), f(3));
            }
            if flags & 0x04 != 0 {
                s += &point_xml("to", f(4), f(5));
            }
            s += "</p:animMotion>";
            s
        }
        BEHAVIOR_ROTATION => {
            let flags = u32_at(d, 0)?;
            let angle = |at: usize| (f32_at(d, at).unwrap_or(0.0) * 60_000.0).round() as i64;
            let mut attrs = String::new();
            if flags & 0x01 != 0 {
                let _ = write!(attrs, r#" by="{}""#, angle(4));
            }
            if flags & 0x02 != 0 {
                let _ = write!(attrs, r#" from="{}""#, angle(8));
            }
            if flags & 0x04 != 0 {
                let _ = write!(attrs, r#" to="{}""#, angle(12));
            }
            if flags & 0x08 != 0 {
                let _ = write!(attrs, r#" dir="{}""#, if u32_at(d, 16)? == 1 { "ccw" } else { "cw" });
            }
            format!("<p:animRot{attrs}>{cbhvr}</p:animRot>")
        }
        BEHAVIOR_SCALE => {
            let flags = u32_at(d, 0)?;
            let f = |i: usize| f32_at(d, 4 + i * 4).unwrap_or(0.0);
            let mut s = String::from("<p:animScale");
            if flags & 0x08 != 0 {
                let _ = write!(s, r#" zoomContents="{}""#, u8::from(*d.get(28)? != 0));
            }
            s.push('>');
            s += &cbhvr;
            if flags & 0x01 != 0 {
                s += &point_xml("by", f(0), f(1));
            }
            if flags & 0x02 != 0 {
                s += &point_xml("from", f(2), f(3));
            }
            if flags & 0x04 != 0 {
                s += &point_xml("to", f(4), f(5));
            }
            s += "</p:animScale>";
            s
        }
        BEHAVIOR_COLOR => {
            let flags = u32_at(d, 0)?;
            let color = |at: usize| color_xml(u32_at(d, at).unwrap_or(0), u32_at(d, at + 4).unwrap_or(0), u32_at(d, at + 8).unwrap_or(0), u32_at(d, at + 12).unwrap_or(0));
            let mut s = String::from("<p:animClr");
            // The colour space and direction come with the behavior's own properties.
            if let Some(list) = common.children().iter().find(|r| r.ty == TIME_PROPERTY_LIST) {
                for v in list.children().iter().filter(|v| v.ty == TIME_VARIANT) {
                    match (v.inst, variant(v)) {
                        (4, Some(Variant::Int(m))) => {
                            let _ = write!(s, r#" clrSpc="{}""#, if m == 1 { "hsl" } else { "rgb" });
                        }
                        (5, Some(Variant::Int(m))) => {
                            let _ = write!(s, r#" dir="{}""#, if m == 1 { "ccw" } else { "cw" });
                        }
                        _ => {}
                    }
                }
            }
            s.push('>');
            s += &cbhvr;
            if flags & 0x01 != 0 {
                let model = u32_at(d, 4).unwrap_or(0);
                if model == 0 {
                    let _ = write!(s, r#"<p:by><p:rgb r="{}" g="{}" b="{}"/></p:by>"#, u32_at(d, 8).unwrap_or(0) as i32, u32_at(d, 12).unwrap_or(0) as i32, u32_at(d, 16).unwrap_or(0) as i32);
                } else if model == 1 {
                    let _ = write!(s, r#"<p:by><p:hsl h="{}" s="{}" l="{}"/></p:by>"#, u32_at(d, 8).unwrap_or(0) as i32, u32_at(d, 12).unwrap_or(0) as i32, u32_at(d, 16).unwrap_or(0) as i32);
                }
            }
            if flags & 0x02 != 0 {
                let _ = write!(s, "<p:from>{}</p:from>", color(20));
            }
            if flags & 0x04 != 0 {
                let _ = write!(s, "<p:to>{}</p:to>", color(36));
            }
            s += "</p:animClr>";
            s
        }
        BEHAVIOR_COMMAND => {
            let kind = match u32_at(d, 4)? {
                1 => "call",
                2 => "verb",
                _ => "evt",
            };
            let command = children.iter().find(|r| r.ty == TIME_VARIANT).and_then(text_of).unwrap_or_default();
            format!(r#"<p:cmd type="{kind}" cmd="{}">{cbhvr}</p:cmd>"#, esc(&command))
        }
        _ => return None,
    })
}

/// The time node as an element (`p:par`, `p:seq`, `p:excl` or a behavior).
fn write_node(rec: &Rec, b: &mut Builder) -> Option<String> {
    let n = read_node(rec);
    let id = b.next_id;
    b.next_id += 1;
    match n.kind {
        3 => return behavior_node(rec, &n, id, b),
        4 => {
            // A media node: the sound or video of a shape.
            let element = rec.child(CLIENT_VISUAL_ELEMENT)?;
            let kind = element.children().iter().find(|r| r.ty == VISUAL_SHAPE).and_then(|r| u32_at(r.data, 0)).unwrap_or(3);
            let target = target(&element, b, None)?;
            let ctn = common_time_node(rec, &n, id, b, None);
            let mut attrs = String::new();
            if let Some(Variant::Float(v)) = n.prop(0x16) {
                let _ = write!(attrs, r#" vol="{}""#, (v.round() as i64).clamp(0, 100_000));
            }
            if n.int(0x17) == Some(1) {
                attrs += r#" mute="1""#;
            }
            if let Some(c) = n.int(0x0F) {
                let _ = write!(attrs, r#" numSld="{c}""#);
            }
            if n.int(0x12) == Some(1) {
                attrs += r#" showWhenStopped="0""#;
            }
            let (open, close) = if kind == 4 { ("<p:video>", "</p:video>") } else { ("<p:audio>", "</p:audio>") };
            return Some(format!("{open}<p:cMediaNode{attrs}>{ctn}<p:tgtEl>{target}</p:tgtEl></p:cMediaNode>{close}"));
        }
        _ => {}
    }
    let mut inner = String::new();
    for child in rec.children().iter().filter(|r| r.ty == EXT_TIME_NODE) {
        if let Some(x) = write_node(child, b) {
            inner += &x;
        }
    }
    let node_type = n.int(0x14);
    if matches!(node_type, Some(4 | 5 | 9)) && inner.is_empty() {
        return None;
    }
    let ctn_head_conditions = String::new();
    let _ = ctn_head_conditions;
    let ctn = common_time_node(rec, &n, id, b, Some(&inner));
    Some(match n.kind {
        1 => {
            let mut attrs = String::new();
            let mut extra = String::new();
            if let Some(seq) = rec.child(TIME_SEQUENCE_DATA) {
                if u32_at(seq.data, 0).unwrap_or(0) == 1 {
                    attrs += r#" concurrent="1""#;
                }
                if u32_at(seq.data, 4).unwrap_or(0) == 1 {
                    attrs += r#" nextAc="seek""#;
                }
                if u32_at(seq.data, 8).unwrap_or(0) == 1 {
                    attrs += r#" prevAc="skipTimed""#;
                }
            }
            extra += &conditions(rec, 4, "prevCondLst", b);
            extra += &conditions(rec, 3, "nextCondLst", b);
            format!("<p:seq{attrs}>{ctn}{extra}</p:seq>")
        }
        2 => format!("<p:excl>{ctn}</p:excl>"),
        _ => format!("<p:par>{ctn}</p:par>"),
    })
}

/// The animation of a shape in a PowerPoint 97 to 2000 file (`AnimationInfoAtom`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Anim {
    pub(crate) automatic: bool,
    pub(crate) delay: u32,
    pub(crate) order: i16,
    pub(crate) effect: u8,
    pub(crate) direction: u8,
}

impl Anim {
    pub(crate) fn read(d: &[u8]) -> Option<Anim> {
        let flags = u32_at(d, 4)?;
        let order = i16::from_le_bytes(d.get(16..18)?.try_into().ok()?);
        Some(Anim { automatic: (flags >> 2) & 3 == 1, delay: u32_at(d, 12)?, order, effect: *d.get(21)?, direction: *d.get(22)? })
    }
}

/// How an old effect is written now: the preset (id and subtype), the transition filter (when it is one) and the edge it flies in from.
fn old_effect(a: &Anim) -> (u32, u32, Option<String>, Option<u8>) {
    let d = a.direction;
    match a.effect {
        2 => (3, if d == 0 { 5 } else { 10 }, Some(format!("blinds({})", if d == 0 { "vertical" } else { "horizontal" })), None),
        3 => (5, if d == 0 { 10 } else { 5 }, Some(format!("checkerboard({})", if d == 0 { "across" } else { "down" })), None),
        5 => (9, 0, Some("dissolve".to_string()), None),
        6 => (10, 0, Some("fade".to_string()), None),
        8 => (14, if d == 0 { 10 } else { 5 }, Some(format!("randombar({})", if d == 0 { "horizontal" } else { "vertical" })), None),
        10 => {
            let (name, sub) = match d {
                0 => ("right", 2),
                1 => ("down", 4),
                2 => ("left", 8),
                _ => ("up", 1),
            };
            (22, sub, Some(format!("wipe({name})")), None)
        }
        11 => (4, if d == 0 { 32 } else { 16 }, Some(format!("box({})", if d == 0 { "out" } else { "in" })), None),
        12 => (2, [8, 1, 2, 4, 9, 3, 12, 6].get(usize::from(d)).copied().unwrap_or(8), None, Some(d)),
        _ => (1, 0, None, None),
    }
}

/// The animation of a fly-in from the slide's edge: the horizontal and vertical movements.
fn fly_xml(spid: u32, id: &mut u32, from: u8) -> String {
    let (x, y) = match from {
        0 => ("0-#ppt_w/2", "#ppt_y"),
        1 => ("#ppt_x", "0-#ppt_h/2"),
        2 => ("1+#ppt_w/2", "#ppt_y"),
        3 => ("#ppt_x", "1+#ppt_h/2"),
        4 => ("0-#ppt_w/2", "0-#ppt_h/2"),
        5 => ("1+#ppt_w/2", "0-#ppt_h/2"),
        6 => ("0-#ppt_w/2", "1+#ppt_h/2"),
        _ => ("1+#ppt_w/2", "1+#ppt_h/2"),
    };
    let mut s = String::new();
    for (name, start, end) in [("ppt_x", x, "#ppt_x"), ("ppt_y", y, "#ppt_y")] {
        *id += 1;
        let _ = write!(
            s,
            r#"<p:anim calcmode="lin" valueType="num"><p:cBhvr additive="base"><p:cTn id="{id}" dur="500" fill="hold"/><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl><p:attrNameLst><p:attrName>{name}</p:attrName></p:attrNameLst></p:cBhvr><p:tavLst><p:tav tm="0"><p:val><p:strVal val="{start}"/></p:val></p:tav><p:tav tm="100000"><p:val><p:strVal val="{end}"/></p:val></p:tav></p:tavLst></p:anim>"#,
            id = *id
        );
    }
    s
}

/// A timing for shapes that have only the old animation information: every shape comes in by its effect, by click or after the one before.
pub(crate) fn simple_timing(effects: &[(u32, Anim)]) -> Option<String> {
    if effects.is_empty() {
        return None;
    }
    let mut sorted: Vec<&(u32, Anim)> = effects.iter().collect();
    // An order of -2 follows the master's order: after the rest.
    sorted.sort_by_key(|(_, a)| if a.order < 0 { i32::MAX } else { i32::from(a.order) });
    let mut id = 2;
    let mut groups = String::new();
    // The current click group: its open `p:par` pieces, and how long it has run.
    let mut open: Option<(String, u32)> = None;
    let close = |groups: &mut String, open: &mut Option<(String, u32)>| {
        if let Some((inner, _)) = open.take() {
            groups.push_str(&inner);
            groups.push_str("</p:childTnLst></p:cTn></p:par>");
        }
    };
    for (spid, a) in sorted {
        let (preset, subtype, filter, fly) = old_effect(a);
        let mut effect = String::new();
        let after = a.automatic && open.is_some();
        if !a.automatic || open.is_none() {
            close(&mut groups, &mut open);
            id += 1;
            let start = if a.automatic { r#"<p:cond delay="indefinite"/><p:cond evt="onBegin" delay="0"><p:tn val="2"/></p:cond>"# } else { r#"<p:cond delay="indefinite"/>"# };
            open = Some((format!(r#"<p:par><p:cTn id="{id}" fill="hold"><p:stCondLst>{start}</p:stCondLst><p:childTnLst>"#), 0));
        }
        let Some((inner, elapsed)) = open.as_mut() else { continue };
        id += 1;
        let _ = write!(inner, r#"<p:par><p:cTn id="{id}" fill="hold"><p:stCondLst><p:cond delay="{}"/></p:stCondLst><p:childTnLst>"#, if after { *elapsed } else { 0 });
        id += 1;
        let node_type = if after || a.automatic { "afterEffect" } else { "clickEffect" };
        let _ = write!(
            effect,
            r#"<p:par><p:cTn id="{id}" presetID="{preset}" presetClass="entr" presetSubtype="{subtype}" fill="hold" nodeType="{node_type}"><p:stCondLst><p:cond delay="{}"/></p:stCondLst><p:childTnLst>"#,
            a.delay
        );
        id += 1;
        let _ = write!(
            effect,
            r#"<p:set><p:cBhvr><p:cTn id="{id}" dur="1" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="visible"/></p:to></p:set>"#
        );
        if let Some(filter) = filter {
            id += 1;
            let _ = write!(effect, r#"<p:animEffect transition="in" filter="{filter}"><p:cBhvr><p:cTn id="{id}" dur="500"/><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl></p:cBhvr></p:animEffect>"#);
        }
        if let Some(from) = fly {
            effect += &fly_xml(*spid, &mut id, from);
        }
        effect += "</p:childTnLst></p:cTn></p:par>";
        inner.push_str(&effect);
        inner.push_str("</p:childTnLst></p:cTn></p:par>");
        *elapsed += a.delay + if filter_is_instant(a) { 0 } else { 500 };
    }
    close(&mut groups, &mut open);
    Some(format!(
        r#"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst>{groups}</p:childTnLst></p:cTn><p:prevCondLst><p:cond evt="onPrev" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:prevCondLst><p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"#
    ))
}

fn filter_is_instant(a: &Anim) -> bool {
    old_effect(a).0 == 1
}

/// The `p:timing` of a slide, from the data of its `___PPT10` binary tag; `None` when it holds no animation.
pub(crate) fn timing_xml(blob: &[u8]) -> Option<String> {
    let all = records(blob);
    let root = all.iter().find(|r| r.ty == EXT_TIME_NODE)?;
    let mut b = Builder { next_id: 1, text_builds: BTreeSet::new() };
    let tree = write_node(root, &mut b)?;
    let builds: String = b.text_builds.iter().map(|(spid, group)| format!(r#"<p:bldP spid="{spid}" grpId="{group}" build="p"/>"#)).collect();
    let list = if builds.is_empty() { String::new() } else { format!("<p:bldLst>{builds}</p:bldLst>") };
    Some(format!("<p:timing><p:tnLst>{tree}</p:tnLst>{list}</p:timing>"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(ver: u8, inst: u16, ty: u16, body: &[u8]) -> Vec<u8> {
        let mut v = (u16::from(ver) | (inst << 4)).to_le_bytes().to_vec();
        v.extend(ty.to_le_bytes());
        v.extend((body.len() as u32).to_le_bytes());
        v.extend(body);
        v
    }

    fn container(inst: u16, ty: u16, kids: &[Vec<u8>]) -> Vec<u8> {
        rec(0xF, inst, ty, &kids.concat())
    }

    fn u(v: u32) -> Vec<u8> {
        v.to_le_bytes().to_vec()
    }

    fn node_atom(kind: u32, fill: u32, duration: u32, flags: u32) -> Vec<u8> {
        rec(0, 0, TIME_NODE_ATOM, &[u(0), u(3), u(kind), u(fill), u(0), u(0), u(duration), u(flags)].concat())
    }

    fn int_prop(id: u16, v: u32) -> Vec<u8> {
        rec(0, id, TIME_VARIANT, &[&[1u8][..], &u(v)].concat())
    }

    fn string_variant(inst: u16, s: &str) -> Vec<u8> {
        let mut body = vec![3u8];
        for c in s.encode_utf16().chain([0]) {
            body.extend(c.to_le_bytes());
        }
        rec(0, inst, TIME_VARIANT, &body)
    }

    fn props(list: &[Vec<u8>]) -> Vec<u8> {
        container(0, TIME_PROPERTY_LIST, list)
    }

    fn condition_rec(inst: u16, object: u32, event: u32, id: u32, delay: u32, element: Option<Vec<u8>>) -> Vec<u8> {
        let mut kids = vec![rec(0, 0, TIME_CONDITION_ATOM, &[u(object), u(event), u(id), u(delay)].concat())];
        kids.extend(element);
        container(inst, TIME_CONDITION_CONTAINER, &kids)
    }

    fn shape_element(spid: u32) -> Vec<u8> {
        container(0, CLIENT_VISUAL_ELEMENT, &[rec(0, 0, VISUAL_SHAPE, &[u(0), u(1), u(spid), u(0), u(0)].concat())])
    }

    fn page_element() -> Vec<u8> {
        container(0, CLIENT_VISUAL_ELEMENT, &[rec(0, 0, VISUAL_PAGE, &u(1))])
    }

    fn common_behavior(spid: u32, attr: Option<&str>) -> Vec<u8> {
        let mut kids = vec![rec(0, 0, TIME_BEHAVIOR_ATOM, &[u(0x4), u(0), u(0), u(0)].concat())];
        if let Some(a) = attr {
            kids.push(container(1, TIME_VARIANT_LIST, &[string_variant(0, a)]));
        }
        kids.push(shape_element(spid));
        container(0, TIME_BEHAVIOR_CONTAINER, &kids)
    }

    /// A click effect on shape 4 that makes it appear and fade in, under the main sequence, as PowerPoint writes it.
    fn fade_in_tree(spid: u32) -> Vec<u8> {
        let set = container(
            0,
            BEHAVIOR_SET,
            &[rec(0, 0, 0xF13A, &[u(1), u(1)].concat()), string_variant(1, "visible"), common_behavior(spid, Some("style.visibility"))],
        );
        let set_node = container(1, EXT_TIME_NODE, &[node_atom(3, 0, 1, 0x19), props(&[]), set, condition_rec(1, 0, 0, 0, 0, None)]);
        let effect = container(0, BEHAVIOR_EFFECT, &[rec(0, 0, 0xF136, &[u(3), u(0), u(0), u(0)][..3].concat()), string_variant(1, "fade"), common_behavior(spid, None)]);
        let effect_node = container(1, EXT_TIME_NODE, &[node_atom(3, 0, 500, 0x18), props(&[]), effect, condition_rec(1, 0, 0, 0, 0, None)]);
        let click_effect = container(
            1,
            EXT_TIME_NODE,
            &[
                node_atom(0, 0, 0xFFFF_FFFF, 0x12),
                props(&[int_prop(0x14, 1), int_prop(9, 10), int_prop(0x0A, 0), int_prop(0x0B, 1), int_prop(0x13, 0)]),
                condition_rec(1, 0, 0, 0, 0, None),
                set_node,
                effect_node,
            ],
        );
        let with_group = container(1, EXT_TIME_NODE, &[node_atom(0, 0, 0xFFFF_FFFF, 0x12), props(&[int_prop(0x14, 6)]), condition_rec(1, 0, 0, 0, 0, None), click_effect]);
        let click_par = container(
            1,
            EXT_TIME_NODE,
            &[node_atom(0, 0, 0xFFFF_FFFF, 0x12), props(&[]), condition_rec(1, 0, 0, 0, 0xFFFF_FFFF, None), with_group],
        );
        let main_seq = container(
            1,
            EXT_TIME_NODE,
            &[
                node_atom(1, 0, 0xFFFF_FFFF, 0x18),
                props(&[int_prop(0x14, 4)]),
                rec(0, 0, TIME_SEQUENCE_DATA, &[u(1), u(1), u(1), u(0), u(3)].concat()),
                condition_rec(3, 1, 9, 0, 0, Some(page_element())),
                condition_rec(4, 1, 10, 0, 0, Some(page_element())),
                click_par,
            ],
        );
        container(1, EXT_TIME_NODE, &[node_atom(0, 0, 0xFFFF_FFFF, 0x12), props(&[int_prop(0x14, 9)]), main_seq])
    }

    #[test]
    fn old_animation_information_gives_a_main_sequence() {
        let anim = |order: i16, automatic: bool, effect: u8, direction: u8| Anim { automatic, delay: 0, order, effect, direction };
        let xml = simple_timing(&[(5, anim(2, true, 6, 0)), (4, anim(1, false, 12, 0)), (6, anim(3, false, 10, 2)), (7, anim(4, false, 0, 0))]).expect("timing");
        if let Ok(path) = std::env::var("PPT_OLD_TIMING_OUT") {
            std::fs::write(path, &xml).expect("write");
        }
        // The fly-in (order 1) comes first, by click; the fade follows it automatically; the wipe from the left is the next click.
        let fly = xml.find(r#"presetID="2""#).expect("fly");
        let fade = xml.find(r#"presetID="10""#).expect("fade");
        let wipe = xml.find(r#"filter="wipe(left)""#).expect("wipe");
        assert!(fly < fade && fade < wipe, "{xml}");
        assert!(xml.contains(r#"nodeType="afterEffect""#) && xml.contains(r#"<p:attrName>ppt_x</p:attrName>"#), "{xml}");
        assert!(xml.contains(r#"presetID="1""#), "{xml}");
        assert_eq!(xml.matches(r#"<p:cond delay="indefinite"/>"#).count(), 3, "{xml}");
        assert!(simple_timing(&[]).is_none());
    }

    #[test]
    fn a_click_effect_becomes_a_timing_tree() {
        let blob = fade_in_tree(4);
        let xml = timing_xml(&blob).expect("timing");
        // (For looking at the result: PPT_TIMING_OUT=file cargo test ...)
        if let Ok(path) = std::env::var("PPT_TIMING_OUT") {
            std::fs::write(path, &xml).expect("write");
        }
        assert!(xml.starts_with("<p:timing><p:tnLst><p:par><p:cTn id=\"1\" restart=\"never\" dur=\"indefinite\" nodeType=\"tmRoot\">"), "{xml}");
        assert!(xml.contains(r#"<p:seq concurrent="1" nextAc="seek" prevAc="skipTimed">"#), "{xml}");
        assert!(xml.contains(r#"nodeType="mainSeq""#) && xml.contains(r#"<p:nextCondLst><p:cond evt="onNext" delay="0"><p:tgtEl><p:sldTgt/></p:tgtEl></p:cond></p:nextCondLst>"#), "{xml}");
        assert!(xml.contains(r#"presetID="10" presetClass="entr" presetSubtype="0" grpId="0""#) && xml.contains(r#"nodeType="clickEffect""#), "{xml}");
        assert!(xml.contains(r#"<p:cond delay="indefinite"/>"#.replace("/>", "></p:cond>").as_str()), "{xml}");
        assert!(xml.contains(r#"<p:set><p:cBhvr><p:cTn id="#) && xml.contains(r#"<p:to><p:strVal val="visible"/></p:to></p:set>"#), "{xml}");
        assert!(xml.contains(r#"<p:attrName>style.visibility</p:attrName>"#), "{xml}");
        assert!(xml.contains(r#"<p:animEffect transition="in" filter="fade">"#) && xml.contains(r#"<p:spTgt spid="4"/>"#), "{xml}");
    }
}
