// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! Reading a slide's animations (`p:timing`): the effects of its main
//! sequence (`nodeType="mainSeq"`), each with its shape, class, start, look
//! and times. The structure PowerPoint writes is a sequence of click groups
//! (`p:par`), each of timed groups (`p:par` with a `delay`), each of
//! effects (`p:par` whose `p:cTn` has a `presetClass`); see ECMA-376
//! Part 1, 19.5 (Animation). Interactive sequences (triggers) are not read.

use std::collections::HashMap;

use ibis_doc_model::{Animation, AnimationAfterEffect, AnimationClass, AnimationLook, AnimationRepeat, AnimationSide, AnimationStart, AnimationTrigger, ShapeId};
use roxmltree::Node;

use crate::xml::{child, elements};

fn ctn<'a, 'i>(node: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    child(node, "cTn")
}

fn is_true(v: Option<&str>) -> bool {
    matches!(v, Some("1" | "true"))
}

/// A time node's own start delay (`stCondLst/cond/@delay`), milliseconds;
/// 0 when `indefinite` or absent.
fn delay(ctn: Node) -> u32 {
    child(ctn, "stCondLst")
        .into_iter()
        .flat_map(elements)
        .filter_map(|c| c.attribute("delay"))
        .filter_map(|d| d.parse::<u32>().ok())
        .max()
        .unwrap_or(0)
}

/// Parse the trigger condition from an interactive sequence's cTn.
fn parse_interactive_trigger(interactive_ctn: Node, shapes: &HashMap<String, ShapeId>) -> AnimationTrigger {
    let Some(st_cond_lst) = child(interactive_ctn, "stCondLst") else { return AnimationTrigger::None };
    for cond in elements(st_cond_lst) {
        if cond.tag_name().name() != "cond" {
            continue;
        }
        // A. オブジェクトのクリック時: evt="onClick" かつ spTgt
        if cond.attribute("evt") == Some("onClick") {
            if let Some(spid) = cond.descendants().find(|n| n.is_element() && n.tag_name().name() == "spTgt").and_then(|n| n.attribute("spid")) {
                if let Some(&shape) = shapes.get(spid) {
                    return AnimationTrigger::OnShapeClick(shape);
                }
            }
        }
        // B. メディアブックマーク時: p14:mediaBmKTrigger
        if let Some(bmk_node) = cond.descendants().find(|n| n.is_element() && n.tag_name().name() == "mediaBmKTrigger") {
            let bmk_name = bmk_node.attribute("bmkName").unwrap_or_default();
            if let Some(spid) = bmk_node.descendants().find(|n| n.is_element() && n.tag_name().name() == "spTgt").and_then(|n| n.attribute("spid")) {
                if let Some(&shape) = shapes.get(spid) {
                    return AnimationTrigger::OnMediaBookmark { shape, bookmark: bmk_name.to_string() };
                }
            }
        }
    }
    AnimationTrigger::None
}

/// The effects of `timing` (a slide's `p:timing`), in order, for the shapes
/// `shapes` names by drawing id.
pub(crate) fn read(slide: Node, shapes: &HashMap<String, ShapeId>, groups: &HashMap<String, Vec<ShapeId>>) -> Vec<Animation> {
    let Some(timing) = child(slide, "timing") else { return Vec::new() };
    let mut chart_builds: HashMap<String, (ibis_doc_model::ChartBuildStep, bool)> = HashMap::new();
    let mut diagram_builds: HashMap<String, ibis_doc_model::DiagramBuildStep> = HashMap::new();
    if let Some(bld_lst) = child(timing, "bldLst") {
        for bld_chart in elements(bld_lst).filter(|n| n.tag_name().name() == "bldChart") {
            if let Some(spid) = bld_chart.attribute("spid") {
                let step = bld_chart
                    .attribute("build")
                    .or_else(|| bld_chart.attribute("bld"))
                    .and_then(ibis_doc_model::ChartBuildStep::parse)
                    .unwrap_or_default();
                let anim_bg = bld_chart.attribute("animBg").map(|v| v == "1" || v == "true").unwrap_or(true);
                chart_builds.insert(spid.to_string(), (step, anim_bg));
            }
        }
        for bld_graphic in elements(bld_lst).filter(|n| n.tag_name().name() == "bldGraphic") {
            if let Some(spid) = bld_graphic.attribute("spid") {
                if let Some(bld_chart) = bld_graphic.descendants().find(|n| n.is_element() && n.tag_name().name() == "bldChart") {
                    let step = bld_chart
                        .attribute("bld")
                        .or_else(|| bld_chart.attribute("build"))
                        .and_then(ibis_doc_model::ChartBuildStep::parse)
                        .unwrap_or_default();
                    let anim_bg = bld_chart.attribute("animBg").map(|v| v == "1" || v == "true").unwrap_or(true);
                    chart_builds.insert(spid.to_string(), (step, anim_bg));
                }
            }
        }
        for bld_dgm in elements(bld_lst).filter(|n| n.tag_name().name() == "bldDgm") {
            if let Some(spid) = bld_dgm.attribute("spid") {
                let step = bld_dgm.attribute("bld").and_then(ibis_doc_model::DiagramBuildStep::parse).unwrap_or_default();
                diagram_builds.insert(spid.to_string(), step);
            }
        }
    }
    let targets = Targets { shapes, groups, chart_builds: &chart_builds, diagram_builds: &diagram_builds };
    let mut out = Vec::new();

    // 1. Main sequence effects (mainSeq)
    if let Some(main) = timing.descendants().find(|n| n.is_element() && n.tag_name().name() == "cTn" && n.attribute("nodeType") == Some("mainSeq")) {
        read_time_node(main,0,&targets,&AnimationTrigger::None,&mut out);
    }

    // 2. Interactive sequence effects (interactiveSeq, triggers)
    for interactive in timing.descendants().filter(|n| n.is_element() && n.tag_name().name() == "cTn" && n.attribute("nodeType") == Some("interactiveSeq")) {
        let trigger = parse_interactive_trigger(interactive, shapes);
        if trigger.is_none() {
            continue;
        }
        read_time_node(interactive,0,&targets,&trigger,&mut out);
    }

    out
}

/// CT_TimeNodeList permits nested parallel and sequential containers. Office
/// versions need not insert exactly the same number of wrapper nodes.
fn read_time_node(node:Node,parent_delay:u32,targets:&Targets<'_>,trigger:&AnimationTrigger,out:&mut Vec<Animation>){
    if node.attribute("presetClass").is_some(){read_effects(node,parent_delay,targets,trigger,out);return;}
    let nested_delay=parent_delay.saturating_add(delay(node));
    for timed in child(node,"childTnLst").into_iter().flat_map(elements){
        if let Some(next)=ctn(timed){read_time_node(next,nested_delay,targets,trigger,out);}
    }
}

/// The effects of time node `e`: one for its shape, or (a group, 19.5: the
/// group is the target) one for each shape of the group, the first starting
/// as `e` does and the others with it.
fn read_effects(e: Node, group_delay: u32, targets: &Targets, trigger: &AnimationTrigger, out: &mut Vec<Animation>) {
    let Some(target_node) = e.descendants().find(|n| n.is_element() && n.tag_name().name() == "spTgt") else { return };
    let Some(target) = target_node.attribute("spid") else { return };
    // A range of paragraphs (`txEl/pRg`).
    let paragraph = child(target_node, "txEl")
        .and_then(|t| child(t, "pRg"))
        .and_then(|r| Some((r.attribute("st")?.parse::<u32>().ok()?, r.attribute("end")?.parse::<u32>().ok()?)))
        .filter(|(st, end)| st <= end);
    // A range of characters (`txEl/charRg`).
    let chars = child(target_node, "txEl")
        .and_then(|t| child(t, "charRg"))
        .and_then(|r| Some((r.attribute("st")?.parse::<u32>().ok()?, r.attribute("end")?.parse::<u32>().ok()?)))
        .filter(|(st, end)| st <= end);
    // A chart element (`cEl/cChartEl`).
    let chart_element = child(target_node, "cEl")
        .and_then(|c| child(c, "cChartEl"))
        .and_then(|el| {
            let s = el.attribute("seriesIdx")?.parse::<i32>().ok()?;
            let c = el.attribute("categoryIdx")?.parse::<i32>().ok()?;
            Some(ibis_doc_model::ChartAnimationElement { series_idx: s, category_idx: c })
        });
    let chart_build = targets.chart_builds.get(target).copied();
    // A diagram element (`graphicEl/dgm` or `subSp`).
    let dgm_element = child(target_node, "graphicEl")
        .and_then(|g| child(g, "dgm"))
        .and_then(|d| {
            let raw_id = d.attribute("id")?;
            let id = raw_id
                .strip_prefix('{')
                .and_then(|s| s.strip_suffix("-0000-0000-0000-000000000000}"))
                .and_then(|hex| u64::from_str_radix(hex, 16).ok())
                .map(|n| n.to_string())
                .unwrap_or_else(|| raw_id.to_string());
            let bld_step = d.attribute("bldStep").unwrap_or("sp").to_string();
            Some(ibis_doc_model::DiagramAnimationElement { id, bld_step })
        })
        .or_else(|| {
            child(target_node, "subSp").and_then(|s| {
                let id = s.attribute("spid")?.to_string();
                Some(ibis_doc_model::DiagramAnimationElement { id, bld_step: "sp".to_string() })
            })
        });
    let dgm_build = targets.diagram_builds.get(target).copied();
    if let Some(&shape) = targets.shapes.get(target) {
        out.extend(read_effect(e, group_delay, shape, trigger.clone()).map(|a| Animation {
            paragraph,
            chars: if paragraph.is_some() { None } else { chars },
            chart_element,
            chart_build,
            dgm_element,
            dgm_build,
            ..a
        }));
    } else if let Some(members) = targets.groups.get(target) {
        for (i, &shape) in members.iter().enumerate() {
            if let Some(mut animation) = read_effect(e, group_delay, shape, trigger.clone()) {
                if i > 0 {
                    animation.start = AnimationStart::WithPrevious;
                }
                animation.chart_element = chart_element;
                animation.chart_build = chart_build;
                animation.dgm_element = dgm_element.clone();
                animation.dgm_build = dgm_build;
                out.push(animation);
            }
        }
    }
}

/// What the effects' targets (`p:spTgt/@spid`) are: drawing ids of shapes, and of groups (their shapes).
pub(crate) struct Targets<'a> {
    pub(crate) shapes: &'a HashMap<String, ShapeId>,
    pub(crate) groups: &'a HashMap<String, Vec<ShapeId>>,
    pub(crate) chart_builds: &'a HashMap<String, (ibis_doc_model::ChartBuildStep, bool)>,
    pub(crate) diagram_builds: &'a HashMap<String, ibis_doc_model::DiagramBuildStep>,
}

pub(crate) fn read_effect(e: Node, group_delay: u32, shape: ShapeId, trigger: AnimationTrigger) -> Option<Animation> {
    let class = match e.attribute("presetClass")? {
        "entr" => AnimationClass::Entrance,
        "emph" => AnimationClass::Emphasis,
        "exit" => AnimationClass::Exit,
        "path" => AnimationClass::Path,
        "mediacall" => AnimationClass::Media,
        _ => return None,
    };
    let start = match e.attribute("nodeType") {
        Some("withEffect") => AnimationStart::WithPrevious,
        Some("afterEffect") => AnimationStart::AfterPrevious,
        _ => AnimationStart::OnClick,
    };
    let preset = (
        e.attribute("presetID").and_then(|v| v.parse().ok()).unwrap_or(0),
        e.attribute("presetSubtype").and_then(|v| v.parse().ok()).unwrap_or(0),
    );
    // The effect lasts as long as its longest behaviour (and its delay).
    let behaviours: Vec<Node> = child(e, "childTnLst").into_iter().flat_map(elements).collect();
    let duration = behaviours
        .iter()
        .filter_map(|b| child(*b, "cBhvr").and_then(ctn))
        .map(|c| delay(c) + c.attribute("dur").and_then(|d| d.parse::<u32>().ok()).unwrap_or(0) * if is_true(c.attribute("autoRev")) { 2 } else { 1 })
        .max()
        .unwrap_or(0);
    let has = |name: &str| behaviours.iter().any(|b| b.tag_name().name() == name);
    let side = |subtype: u32| match subtype {
        1 => Some(AnimationSide::Top),
        2 => Some(AnimationSide::Right),
        4 => Some(AnimationSide::Bottom),
        8 => Some(AnimationSide::Left),
        _ => None,
    };
    let behaviors = parse_behaviors(&behaviours, e.attribute("fill") == Some("hold"));
    let motion_path = behaviours.iter().find(|b| b.tag_name().name() == "animMotion").and_then(|b| b.attribute("path")).and_then(path_points);
    // 再生・一時停止・停止: the call (`p:cmd`) says which; the preset (1, 2, 3) when it names none we know.
    let media_command = behaviours.iter().find(|b| b.tag_name().name() == "cmd").and_then(|b| b.attribute("cmd")).and_then(|c| {
        if c.starts_with("play") || c == "resume" {
            Some(ibis_doc_model::MediaCommand::Play)
        } else if c == "togglePause" || c == "pause" {
            Some(ibis_doc_model::MediaCommand::Pause)
        } else if c == "stop" {
            Some(ibis_doc_model::MediaCommand::Stop)
        } else {
            None
        }
    });
    let look = match class {
        AnimationClass::Media => AnimationLook::Media(media_command.or(match preset.0 {
            1 => Some(ibis_doc_model::MediaCommand::Play),
            2 => Some(ibis_doc_model::MediaCommand::Pause),
            3 => Some(ibis_doc_model::MediaCommand::Stop),
            _ => None,
        })?),
        AnimationClass::Entrance | AnimationClass::Exit => match preset.0 {
            1 => AnimationLook::Appear,
            // Fly in / out (スライドイン): the subtype says from where.
            2 => match side(preset.1) {
                Some(s) => AnimationLook::Fly(s),
                None if !behaviors.is_empty() => AnimationLook::Preset,
                None => AnimationLook::Fade,
            },
            10 => AnimationLook::Fade,
            22 => match side(preset.1) {
                Some(s) => AnimationLook::Wipe(s),
                None if !behaviors.is_empty() => AnimationLook::Preset,
                None => AnimationLook::Wipe(AnimationSide::Bottom),
            },
            // PowerPoint MsoAnimEffectZoom=23 and Float=30 (Microsoft's enum). PowerPoint's own ズーム entry
            // is 53 (subtype 16 from the object's centre, 528 from the slide's centre; issue #62).
            // ズーム (grow from the shape's centre, subtype 16 in / 32 out); its other options move the shape too.
            23 if has("animScale") || matches!((class, preset.1), (AnimationClass::Entrance, 16) | (AnimationClass::Exit, 32)) => AnimationLook::Zoom,
            // フロート as ibisOffice wrote it (a fade with a short rise); PowerPoint's own (with `ppt_y` and a turn) is played as it says.
            30 if !has("anim") => AnimationLook::Float(if preset.1 == 2 { AnimationSide::Bottom } else { AnimationSide::Top }),
            // クレジット (28) as PowerPoint writes it has the behaviours that move the shape; ibisOffice's own had none.
            28 if !has("anim") => AnimationLook::Replay,
            // Every other preset (including 53, PowerPoint's own ズーム, and スライドイン from a corner) is played as
            // the behaviours the file gives it say, not as a fade.
            _ if !behaviors.is_empty() => AnimationLook::Preset,
            _ if has("animEffect") || has("anim") || has("animScale") => AnimationLook::Fade,
            _ => AnimationLook::Appear,
        },
        AnimationClass::Emphasis => {
            // スピン and 拡大／収縮 as one behaviour alone; every other emphasis effect (シマー, ウェーブ…) is its behaviours.
            let only = |name: &str| behaviours.len() == 1 && behaviours[0].tag_name().name() == name;
            if only("animRot") && behaviours[0].attribute("by").is_some() {
                AnimationLook::Spin(behaviours[0].attribute("by").and_then(|v| v.parse().ok()).unwrap_or(0))
            } else if let Some(by) = behaviours.iter().find(|b| only(b.tag_name().name()) && b.tag_name().name() == "animScale").and_then(|b| child(*b, "by")) {
                let v = |a: &str| by.attribute(a).and_then(|v| v.parse().ok()).unwrap_or(100_000);
                AnimationLook::Scale(v("x"), v("y"))
            } else if !behaviors.is_empty() {
                AnimationLook::Preset
            } else {
                AnimationLook::Other
            }
        }
        AnimationClass::Path => match motion_path {
            Some(ref points) => points.last().map_or(AnimationLook::Other, |&(x, y)| AnimationLook::Motion(x, y)),
            // A path the model cannot draw as lines: kept as written.
            None if !behaviors.is_empty() => AnimationLook::Preset,
            None => AnimationLook::Other,
        },
    };
    // A path of more than one straight line (カスタム軌跡) keeps its points.
    let path = match (&class, motion_path) {
        (AnimationClass::Path, Some(points)) if !is_single_line(&points) => points,
        _ => Vec::new(),
    };
    // The timed group's delay is where 直前の動作の後 puts it (the end of
    // what came before, which the slide show works out itself): only the
    // effect's own delay is its delay.
    let _ = group_delay;
    // A preset played as its behaviours say keeps their time options (smooth start, auto-reverse…) on the behaviours.
    let preset_look = look == AnimationLook::Preset;
    let parse_repeat = |val: &str| -> AnimationRepeat {
        if val == "indefinite" {
            AnimationRepeat::UntilNextClick
        } else if let Ok(n) = val.parse::<u32>() {
            let count = if n >= 1000 { n / 1000 } else { n };
            if count <= 1 {
                AnimationRepeat::None
            } else {
                AnimationRepeat::Count(count)
            }
        } else {
            AnimationRepeat::None
        }
    };
    let repeat_count = e.attribute("repeatCount")
        .or_else(|| behaviours.iter().filter(|_| !preset_look).filter_map(|b| child(*b, "cBhvr").and_then(ctn)).find_map(|c| c.attribute("repeatCount")))
        .map_or(AnimationRepeat::None, parse_repeat);
    let auto_reverse = e.attribute("autoRev")
        .or_else(|| behaviours.iter().filter(|_| !preset_look).filter_map(|b| child(*b, "cBhvr").and_then(ctn)).find_map(|c| c.attribute("autoRev")))
        .is_some_and(|v| v == "1" || v == "true");
    let accel = e.attribute("accel")
        .or_else(|| behaviours.iter().filter(|_| !preset_look).filter_map(|b| child(*b, "cBhvr").and_then(ctn)).find_map(|c| c.attribute("accel")))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let decel = e.attribute("decel")
        .or_else(|| behaviours.iter().filter(|_| !preset_look).filter_map(|b| child(*b, "cBhvr").and_then(ctn)).find_map(|c| c.attribute("decel")))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let rewind = e.attribute("rewind")
        .or_else(|| behaviours.iter().filter(|_| !preset_look).filter_map(|b| child(*b, "cBhvr").and_then(ctn)).find_map(|c| c.attribute("rewind")))
        .is_some_and(|v| v == "1" || v == "true");
    let parse_after_effect = |fill: &str| match fill {
        "remove" => AnimationAfterEffect::Hide,
        _ => AnimationAfterEffect::Hold,
    };
    let after_effect = e.attribute("fill")
        .or_else(|| behaviours.iter().filter(|_| !preset_look).filter_map(|b| child(*b, "cBhvr").and_then(ctn)).find_map(|c| c.attribute("fill")))
        .map_or(AnimationAfterEffect::Hold, parse_after_effect);
    // The text a word or a letter at a time (`p:iterate`).
    let text_unit = child(e, "iterate").and_then(|it| {
        let unit = match it.attribute("type") {
            Some("wd") => ibis_doc_model::TextUnit::Word,
            Some("lt") => ibis_doc_model::TextUnit::Letter,
            _ => return None,
        };
        let gap = if let Some(v) = child(it, "tmPct").and_then(|t| t.attribute("val")).and_then(|v| v.parse().ok()) {
            ibis_doc_model::UnitGap::Percent(v)
        } else {
            ibis_doc_model::UnitGap::Millis(child(it, "tmAbs").and_then(|t| t.attribute("val")).and_then(|v| v.parse().ok())?)
        };
        Some((unit, gap))
    });
    Some(Animation { shape, class, start, look, delay: delay(e), duration, preset, repeat_count, auto_reverse, rewind, accel, decel, after_effect, trigger, paragraph: None, chars: None, text_unit, path, chart_element: None, chart_build: None, dgm_element: None, dgm_build: None, behaviors: if look == AnimationLook::Preset { behaviors } else { Vec::new() } })
}

/// An effect's behaviours as the model keeps them (`p:set`, `p:anim`, `p:animEffect`, `p:animScale`, `p:animRot`);
/// others (motion paths, colours) are left out.
fn parse_behaviors(nodes: &[Node], inherit_hold: bool) -> Vec<ibis_doc_model::AnimBehavior> {
    use ibis_doc_model::{AnimBehavior, BehaviorKind};
    let value = |v: Node| -> Option<String> {
        let inner = elements(v).next()?;
        inner.attribute("val").map(str::to_string)
    };
    let pair = |n: Option<Node>| -> Option<(i32, i32)> {
        let n = n?;
        Some((n.attribute("x")?.parse().ok()?, n.attribute("y")?.parse().ok()?))
    };
    // A behaviour kept as its XML, its target and time node ids to be filled in on writing.
    let opaque = |b: Node| -> Option<BehaviorKind> {
        let doc_text = b.document().input_text();
        let mut raw = doc_text[b.range()].to_string();
        if let (Some(a), Some(z)) = (raw.find("<p:tgtEl>"), raw.find("</p:tgtEl>")) {
            raw.replace_range(a..z + "</p:tgtEl>".len(), "\u{1}");
        } else {
            return None;
        }
        let mut out = String::new();
        let mut rest = raw.as_str();
        while let Some(at) = rest.find("<p:cTn id=\"") {
            let after = at + "<p:cTn id=\"".len();
            let end = rest[after..].find('"').map_or(rest.len(), |e| after + e);
            out.push_str(&rest[..after]);
            out.push('\u{2}');
            rest = &rest[end..];
        }
        out.push_str(rest);
        Some(BehaviorKind::Opaque(out))
    };
    let mut out = Vec::new();
    for b in nodes {
        let Some(bhvr) = child(*b, "cBhvr") else { continue };
        let time = ctn(bhvr);
        let begin = time.map_or(0, delay);
        let dur = time.and_then(|c| c.attribute("dur")).and_then(|d| d.parse::<u32>().ok()).unwrap_or(0);
        let time_node = time;
        let flag = |a: &str| time_node.is_some_and(|c| is_true(c.attribute(a)));
        let behavior_time = ibis_doc_model::animation_preset::BehaviorTime {
            accel: time_node.and_then(|c| c.attribute("accel")).and_then(|v| v.parse().ok()).unwrap_or(0),
            decel: time_node.and_then(|c| c.attribute("decel")).and_then(|v| v.parse().ok()).unwrap_or(0),
            auto_rev: flag("autoRev"),
            // `by` alone is the amount added to the value the shape has (シマー's `by="(#ppt_w*0.10)"`), as `additive="sum"` says.
            sum: bhvr.attribute("additive") == Some("sum") || (b.tag_name().name() == "anim" && b.attribute("by").is_some() && b.attribute("from").is_none() && b.attribute("to").is_none()),
            // Held to the end of the effect by `fill="hold"` (or by running as long as the effect does, `dur="indefinite"`); a behaviour
            // that says nothing of it takes its effect's (PowerPoint's exit 43 writes `fill="hold"` on the effect only).
            remove: !(time_node.is_some_and(|c| c.attribute("fill") == Some("hold") || c.attribute("dur") == Some("indefinite") || (c.attribute("fill").is_none() && inherit_hold))),
            filter: time_node.and_then(|c| c.attribute("tmFilter")).map(ibis_doc_model::animation_preset::BehaviorTime::parse_filter).unwrap_or_default(),
        };
        let attr = child(bhvr, "attrNameLst").and_then(|l| child(l, "attrName")).and_then(|n| n.text()).map(|t| t.trim().to_string());
        let kind = match b.tag_name().name() {
            // A colour (`<p:to><p:clrVal>`) is kept as it is written (see `animation_color`).
            "set" if child(*b, "to").and_then(|t| child(t, "clrVal")).is_some() => match opaque(*b) {
                Some(kind) => kind,
                None => continue,
            },
            "set" => {
                let (Some(attr), Some(to)) = (attr, child(*b, "to").and_then(value)) else { continue };
                BehaviorKind::Set { attr, value: to }
            }
            // Colours that step (`valueType="clr"`, `<p:clrVal>`) are kept as they are written (see `animation_color`).
            "anim" if child(*b, "tavLst").into_iter().flat_map(elements).any(|t| child(t, "val").is_some_and(|v| child(v, "clrVal").is_some())) => match opaque(*b) {
                Some(kind) => kind,
                None => continue,
            },
            "anim" => {
                let Some(attr) = attr else { continue };
                let mut keys: Vec<(u32, String)> = child(*b, "tavLst")
                    .into_iter()
                    .flat_map(elements)
                    .filter(|t| t.tag_name().name() == "tav")
                    .filter_map(|t| Some((t.attribute("tm")?.parse().ok()?, child(t, "val").and_then(value)?)))
                    .collect();
                if keys.is_empty() {
                    if let (Some(from), Some(to)) = (b.attribute("from"), b.attribute("to")) {
                        keys = vec![(0, from.to_string()), (100_000, to.to_string())];
                    } else if let Some(by) = b.attribute("by") {
                        // `by`: from nothing to this much more (added to the value, `additive="sum"`).
                        keys = vec![(0, "0".to_string()), (100_000, by.to_string())];
                    } else if let Some(to) = b.attribute("to") {
                        // `to` alone: from the shape's own value.
                        keys = vec![(0, format!("#{attr}")), (100_000, to.to_string())];
                    }
                }
                let formula = child(*b, "tavLst").into_iter().flat_map(elements).filter_map(|t| t.attribute("fmla")).last().map(str::to_string).or_else(|| b.attribute("fmla").map(str::to_string));
                BehaviorKind::Anim { attr, keys, formula, discrete: b.attribute("calcmode") == Some("discrete") }
            }
            "animEffect" => {
                let Some(filter) = b.attribute("filter") else { continue };
                BehaviorKind::Effect { filter: filter.to_string(), enter: b.attribute("transition") != Some("out") }
            }
            "animScale" => BehaviorKind::Scale { from: pair(child(*b, "from")), to: pair(child(*b, "to")), by: pair(child(*b, "by")) },
            "animRot" => {
                let v = |a: &str| b.attribute(a).and_then(|v| v.parse::<i32>().ok());
                BehaviorKind::Rotate { from: v("from"), to: v("to"), by: v("by") }
            }
            // Others (colours, motion…): kept as they are written, to be written back.
            _ => match opaque(*b) {
                Some(kind) => kind,
                None => continue,
            },
        };
        out.push(AnimBehavior { kind, begin, dur, time: behavior_time });
    }
    out
}

/// The points a motion path (`M 0 0 L 0.25 0 C … E`, fractions of the slide)
/// goes through after its start, in 1/100 000 of the slide. A curve (`C`,
/// `Q`) is drawn as short lines; `Z` goes back to the start. Only the
/// commands `M L C Q Z E` are read (upper case from the shape's place, lower case from where the path is).
fn path_points(path: &str) -> Option<Vec<(i32, i32)>> {
    let tokens: Vec<&str> = path.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).collect();
    let mut points: Vec<(f64, f64)> = Vec::new();
    let mut start = (0.0, 0.0);
    let mut here = (0.0, 0.0);
    let mut i = 0;
    let num = |i: usize| -> Option<f64> { tokens.get(i)?.parse().ok() };
    let mut command = "";
    // A lower case command (`l`, `c`, `q`, `m`) takes its points relative to where the path is.
    let mut relative = false;
    while i < tokens.len() {
        let t = tokens[i];
        if t.len() == 1 && t.chars().all(|c| c.is_ascii_alphabetic()) {
            relative = t.chars().all(|c| c.is_ascii_lowercase());
            command = match t {
                "m" => "M",
                "l" => "L",
                "c" => "C",
                "q" => "Q",
                "z" => "Z",
                "e" => "E",
                other => other,
            };
            i += 1;
            if command == "Z" {
                here = start;
                points.push(here);
            }
            continue;
        }
        match command {
            "M" | "L" => {
                let mut p = (num(i)?, num(i + 1)?);
                if relative {
                    p = (here.0 + p.0, here.1 + p.1);
                }
                i += 2;
                if command == "M" && points.is_empty() {
                    start = p;
                } else {
                    points.push(p);
                }
                here = p;
                // Further pairs after an `L` are lines; after an `M`, too.
                command = "L";
            }
            "C" => {
                let o = if relative { here } else { (0.0, 0.0) };
                let (c1, c2, p) = ((o.0 + num(i)?, o.1 + num(i + 1)?), (o.0 + num(i + 2)?, o.1 + num(i + 3)?), (o.0 + num(i + 4)?, o.1 + num(i + 5)?));
                i += 6;
                for k in 1..=8 {
                    let t = k as f64 / 8.0;
                    let m = 1.0 - t;
                    let f = |a: f64, b: f64, c: f64, d: f64| m * m * m * a + 3.0 * m * m * t * b + 3.0 * m * t * t * c + t * t * t * d;
                    points.push((f(here.0, c1.0, c2.0, p.0), f(here.1, c1.1, c2.1, p.1)));
                }
                here = p;
            }
            "Q" => {
                let o = if relative { here } else { (0.0, 0.0) };
                let (c, p) = ((o.0 + num(i)?, o.1 + num(i + 1)?), (o.0 + num(i + 2)?, o.1 + num(i + 3)?));
                i += 4;
                for k in 1..=8 {
                    let t = k as f64 / 8.0;
                    let m = 1.0 - t;
                    points.push((m * m * here.0 + 2.0 * m * t * c.0 + t * t * p.0, m * m * here.1 + 2.0 * m * t * c.1 + t * t * p.1));
                }
                here = p;
            }
            _ => return None,
        }
    }
    if points.is_empty() {
        return None;
    }
    Some(points.iter().map(|&(x, y)| ((x * 100_000.0).round() as i32, (y * 100_000.0).round() as i32)).collect())
}

/// Whether the path is the one line from the start (0, 0) to its end.
fn is_single_line(points: &[(i32, i32)]) -> bool {
    points.len() == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PowerPoint's own paths (ステアステップ, バウンス) are written with lower case commands, relative to where the path is.
    #[test]
    fn relative_path_commands_are_played() {
        let p = path_points("M 0 0 l 0.036 0 l 0 0.036 l 0.036 0 E").unwrap();
        assert_eq!(p, vec![(3600, 0), (3600, 3600), (7200, 3600)]);
        let c = path_points("M 0 0 c -0.004 -0.008 -0.018 -0.016 -0.023 -0.016 c 0.031 0 0.05 0.01 0.05 0.02 E").unwrap();
        assert_eq!(c.len(), 16);
        assert_eq!(c[7], (-2300, -1600));
        assert_eq!(c[15], (2700, 400));
    }


    /// An effect on a group is one on each shape of the group: the first as
    /// the group's, the others with it.
    #[test]
    fn an_effect_on_a_group_is_one_on_each_of_its_shapes() {
        let xml = r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst><p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="4" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="5" presetID="1" presetClass="entr" presetSubtype="0" fill="hold" nodeType="clickEffect"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:cTn id="6" dur="1" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst></p:cTn><p:tgtEl><p:spTgt spid="9"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val="visible"/></p:to></p:set></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing></p:sld>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let shapes: HashMap<String, ShapeId> = [("4".to_string(), ShapeId(40))].into();
        let groups: HashMap<String, Vec<ShapeId>> = [("9".to_string(), vec![ShapeId(1), ShapeId(2), ShapeId(3)])].into();
        let effects = read(doc.root_element(), &shapes, &groups);
        assert_eq!(effects.iter().map(|a| a.shape).collect::<Vec<_>>(), [ShapeId(1), ShapeId(2), ShapeId(3)]);
        assert_eq!(effects.iter().map(|a| a.start).collect::<Vec<_>>(), [AnimationStart::OnClick, AnimationStart::WithPrevious, AnimationStart::WithPrevious]);
        // Effects on paragraphs (`p:txEl/p:pRg`) keep their range.
        let by_paragraph = xml.replace(r#"<p:spTgt spid="9"/>"#, r#"<p:spTgt spid="4"><p:txEl><p:pRg st="2" end="3"/></p:txEl></p:spTgt>"#);
        let doc = roxmltree::Document::parse(&by_paragraph).unwrap();
        let effects = read(doc.root_element(), &shapes, &groups);
        assert_eq!(effects.len(), 1);
        assert_eq!((effects[0].shape, effects[0].paragraph), (ShapeId(40), Some((2, 3))));
        // Effects on characters (`p:txEl/p:charRg`) and by word or letter (`p:iterate`) keep what they say.
        let by_chars = by_paragraph.replace(r#"<p:pRg st="2" end="3"/>"#, r#"<p:charRg st="4" end="9"/>"#);
        let doc = roxmltree::Document::parse(&by_chars).unwrap();
        assert_eq!(read(doc.root_element(), &shapes, &groups)[0].chars, Some((4, 9)));
        let by_word = by_paragraph.replace(r#"</p:stCondLst><p:childTnLst><p:set>"#, r#"</p:stCondLst><p:iterate type="wd"><p:tmPct val="10000"/></p:iterate><p:childTnLst><p:set>"#).replace(r#"<p:pRg st="2" end="3"/>"#, "");
        let doc = roxmltree::Document::parse(&by_word).unwrap();
        let unit = read(doc.root_element(), &shapes, &groups)[0].text_unit;
        assert_eq!(unit, Some((ibis_doc_model::TextUnit::Word, ibis_doc_model::UnitGap::Percent(10_000))));
        // Diagram element ST_Guid (`{00000007-0000-0000-0000-000000000000}`) decodes back to `"7"`.
        let by_dgm = xml.replace(
            r#"<p:spTgt spid="9"/>"#,
            r#"<p:spTgt spid="4"><p:graphicEl><a:dgm xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" id="{00000007-0000-0000-0000-000000000000}" bldStep="sp"/></p:graphicEl></p:spTgt>"#,
        );
        let doc = roxmltree::Document::parse(&by_dgm).unwrap();
        let dgm_el = read(doc.root_element(), &shapes, &groups)[0].dgm_element.clone();
        assert_eq!(dgm_el, Some(ibis_doc_model::DiagramAnimationElement { id: "7".to_string(), bld_step: "sp".to_string() }));
        // A shape that is neither is no effect.
        let doc = roxmltree::Document::parse(xml).unwrap();
        assert!(read(doc.root_element(), &shapes, &HashMap::new()).is_empty());
    }

    #[test]
    fn a_path_is_read_as_points() {
        assert_eq!(path_points("M 0 0 L 0.25 0 E"), Some(vec![(25_000, 0)]));
        // A curve is short lines, ending where it ends.
        let curve = path_points("M 0 0 C 0.1 0.1 0.2 0.2 0.3 -0.05 E").unwrap();
        assert_eq!((curve.len(), curve.last().copied()), (8, Some((30_000, -5_000))));
        // Lines one after another, and back to the start with Z.
        assert_eq!(path_points("M 0 0 L 0.1 0 L 0.1 0.2 Z E"), Some(vec![(10_000, 0), (10_000, 20_000), (0, 0)]));
        assert_eq!(path_points("M 0 0 E"), None);
    }
}
