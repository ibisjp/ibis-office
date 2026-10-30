// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! アニメーション: a slide's effects written as its `p:timing`, rebuilt
//! from the model when they changed, as PowerPoint writes its main sequence
//! (ECMA-376 Part 1, 19.5): a click group (`p:par`, started by a click, or
//! with the slide for the effects before the first click) of timed groups
//! (`p:par` with a delay: 直前の動作の後 starts a new one) of effects (`p:par`
//! whose `p:cTn` has the preset). Appear, fade and fly in/out, spin and
//! grow/shrink are written; a slide whose timing holds what the model does
//! not read (triggers, paragraph builds, sounds…) or effects it cannot
//! write (motion paths and others) is refused, not changed.

use std::collections::HashMap;

use ibis_doc_model::{Animation, AnimationAfterEffect, AnimationClass, AnimationLook, AnimationRepeat, AnimationSide, AnimationStart, AnimationTrigger, ShapeId};
use ibis_ooxml::{NodeId, XmlDocument};

use super::pptx_new::Prefixes;
use crate::Error;
use crate::save::unsupported;

/// Elements of a timing tree the model does not read: rewriting it would
/// lose them.
const UNREAD: &[&str] = &["cRg", "oleChartEl", "inkTgt", "sndTgt", "audio", "video", "cmd", "excl", "subTnLst", "bldOleChart"];

/// Whether `timing` (a slide's `p:timing`) can be rebuilt from the model.
fn rewritable(doc: &XmlDocument, timing: NodeId) -> bool {
    let nodes = doc.descendants(timing);
    let unread = nodes.iter().any(|&n| doc.is_element(n) && UNREAD.contains(&doc.local_name(n)))
        // An iteration the model keeps: by word or by letter, with a gap in percent or milliseconds.
        || nodes.iter().any(|&n| {
            doc.is_element(n)
                && doc.local_name(n) == "iterate"
                && !(matches!(doc.attr(n, "type"), Some("wd" | "lt")) && doc.children(n).iter().any(|&c| doc.is_element(c) && matches!(doc.local_name(c), "tmPct" | "tmAbs")))
        })
        || nodes.iter().any(|&n| {
            doc.is_element(n)
                && doc.local_name(n) == "graphicEl"
                && !doc.children(n).iter().any(|&c| doc.is_element(c) && doc.local_name(c) == "dgm")
        })
        || nodes.iter().any(|&n| {
            doc.is_element(n)
                && doc.local_name(n) == "bldGraphic"
                && !doc.children(n).iter().any(|&sub| {
                    doc.is_element(sub)
                        && doc.local_name(sub) == "bldSub"
                        && doc.children(sub).iter().any(|&c| doc.is_element(c) && doc.local_name(c) == "bldChart")
                })
        });
    let unknown_sequences = nodes.iter().any(|&n| {
        doc.is_element(n)
            && doc.local_name(n) == "seq"
            && doc.children(n).iter().any(|&c| {
                doc.is_element(c)
                    && doc.local_name(c) == "cTn"
                    && doc.attr(c, "nodeType").is_some_and(|t| t != "mainSeq" && t != "interactiveSeq")
            })
    });
    // Effects on paragraphs (`txEl/pRg`) and on characters (`txEl/charRg`) are read and written.
    !(unread || unknown_sequences)
}

/// One effect's `p:par`: `id` its time node's id (it takes more for its
/// behaviours: the next free one is returned), `spid` its shape.
fn effect_xml(p: &str, a: &Animation, spid: &str, delay: u32, id: &mut u32) -> Result<String, Error> {
    let node = match a.start {
        AnimationStart::OnClick => "clickEffect",
        AnimationStart::WithPrevious => "withEffect",
        AnimationStart::AfterPrevious => "afterEffect",
    };
    let class = match a.class {
        AnimationClass::Entrance => "entr",
        AnimationClass::Exit => "exit",
        AnimationClass::Emphasis => "emph",
        AnimationClass::Path => "path",
    };
    let dur = a.duration.max(1);
    let effect_id = *id;
    *id += 1;
    let mut next = || {
        let n = *id;
        *id += 1;
        n
    };
    let target = match (a.paragraph, a.chars, a.chart_element, &a.dgm_element) {
        (Some((first, last)), _, _, _) => format!("<{p}tgtEl><{p}spTgt spid=\"{spid}\"><{p}txEl><{p}pRg st=\"{first}\" end=\"{last}\"/></{p}txEl></{p}spTgt></{p}tgtEl>"),
        (None, Some((first, last)), _, _) => format!("<{p}tgtEl><{p}spTgt spid=\"{spid}\"><{p}txEl><{p}charRg st=\"{first}\" end=\"{last}\"/></{p}txEl></{p}spTgt></{p}tgtEl>"),
        (None, None, Some(el), _) => format!("<{p}tgtEl><{p}spTgt spid=\"{spid}\"><{p}cEl><{p}cChartEl seriesIdx=\"{}\" categoryIdx=\"{}\"/></{p}cEl></{p}spTgt></{p}tgtEl>", el.series_idx, el.category_idx),
        (None, None, None, Some(dgm)) => {
            let dgm_id = if dgm.id.starts_with('{') {
                dgm.id.clone()
            } else if let Ok(n) = dgm.id.parse::<u64>() {
                format!("{{{n:08X}-0000-0000-0000-000000000000}}")
            } else {
                format!(
                    "{{{:0>8}-0000-0000-0000-000000000000}}",
                    dgm.id.chars().filter(|c| c.is_ascii_hexdigit()).take(8).collect::<String>().to_ascii_uppercase()
                )
            };
            format!("<{p}tgtEl><{p}spTgt spid=\"{spid}\"><{p}graphicEl><a:dgm id=\"{dgm_id}\" bldStep=\"{}\"/></{p}graphicEl></{p}spTgt></{p}tgtEl>", dgm.bld_step)
        }
        (None, None, None, None) => format!("<{p}tgtEl><{p}spTgt spid=\"{spid}\"/></{p}tgtEl>"),
    };
    let visibility = |n: u32, at: u32, to: &str| {
        format!(
            "<{p}set><{p}cBhvr><{p}cTn id=\"{n}\" dur=\"1\" fill=\"hold\"><{p}stCondLst><{p}cond delay=\"{at}\"/></{p}stCondLst></{p}cTn>{target}<{p}attrNameLst><{p}attrName>style.visibility</{p}attrName></{p}attrNameLst></{p}cBhvr><{p}to><{p}strVal val=\"{to}\"/></{p}to></{p}set>"
        )
    };
    let fly = |n: u32, attr: &str, from: &str, to: &str| {
        format!(
            "<{p}anim calcmode=\"lin\" valueType=\"num\"><{p}cBhvr additive=\"base\"><{p}cTn id=\"{n}\" dur=\"{dur}\" fill=\"hold\"/>{target}<{p}attrNameLst><{p}attrName>{attr}</{p}attrName></{p}attrNameLst></{p}cBhvr><{p}tavLst><{p}tav tm=\"0\"><{p}val><{p}strVal val=\"{from}\"/></{p}val></{p}tav><{p}tav tm=\"100000\"><{p}val><{p}strVal val=\"{to}\"/></{p}val></{p}tav></{p}tavLst></{p}anim>"
        )
    };
    // Where a side puts the shape (`ppt_x`, `ppt_y` outside the slide).
    let off = |side: AnimationSide| match side {
        AnimationSide::Bottom => ("#ppt_x", "1+#ppt_h/2"),
        AnimationSide::Top => ("#ppt_x", "0-#ppt_h/2"),
        AnimationSide::Left => ("0-#ppt_w/2", "#ppt_y"),
        AnimationSide::Right => ("1+#ppt_w/2", "#ppt_y"),
    };
    let side_subtype = |side: AnimationSide| match side {
        AnimationSide::Top => 1,
        AnimationSide::Right => 2,
        AnimationSide::Bottom => 4,
        AnimationSide::Left => 8,
    };
    let (preset, subtype, behaviours) = match (a.class, a.look) {
        (AnimationClass::Entrance, AnimationLook::Appear) => (1, 0, visibility(next(), 0, "visible")),
        (AnimationClass::Exit, AnimationLook::Appear) => (1, 0, visibility(next(), 0, "hidden")),
        (AnimationClass::Entrance, AnimationLook::Fade) => {
            let (v, f) = (next(), next());
            (10, 0, format!("{}<{p}animEffect transition=\"in\" filter=\"fade\"><{p}cBhvr><{p}cTn id=\"{f}\" dur=\"{dur}\"/>{target}</{p}cBhvr></{p}animEffect>", visibility(v, 0, "visible")))
        }
        (AnimationClass::Exit, AnimationLook::Fade) => {
            let (f, v) = (next(), next());
            (10, 0, format!("<{p}animEffect transition=\"out\" filter=\"fade\"><{p}cBhvr><{p}cTn id=\"{f}\" dur=\"{dur}\"/>{target}</{p}cBhvr></{p}animEffect>{}", visibility(v, dur.saturating_sub(1), "hidden")))
        }
        (AnimationClass::Entrance, AnimationLook::Fly(side)) => {
            let (v, x, y) = (next(), next(), next());
            let (fx, fy) = off(side);
            (2, side_subtype(side), format!("{}{}{}", visibility(v, 0, "visible"), fly(x, "ppt_x", fx, "#ppt_x"), fly(y, "ppt_y", fy, "#ppt_y")))
        }
        (AnimationClass::Exit, AnimationLook::Fly(side)) => {
            let (x, y, v) = (next(), next(), next());
            let (tx, ty) = off(side);
            (2, side_subtype(side), format!("{}{}{}", fly(x, "ppt_x", "#ppt_x", tx), fly(y, "ppt_y", "#ppt_y", ty), visibility(v, dur.saturating_sub(1), "hidden")))
        }
        (AnimationClass::Entrance, AnimationLook::Zoom) => {
            let (v, s) = (next(), next());
            (
                23,
                0,
                format!(
                    "{}<{p}animScale><{p}cBhvr><{p}cTn id=\"{s}\" dur=\"{dur}\" fill=\"hold\"/>{target}</{p}cBhvr><{p}from x=\"0\" y=\"0\"/><{p}to x=\"100000\" y=\"100000\"/></{p}animScale>",
                    visibility(v, 0, "visible")
                ),
            )
        }
        (AnimationClass::Exit, AnimationLook::Zoom) => {
            let (s, v) = (next(), next());
            (
                23,
                0,
                format!(
                    "<{p}animScale><{p}cBhvr><{p}cTn id=\"{s}\" dur=\"{dur}\" fill=\"hold\"/>{target}</{p}cBhvr><{p}from x=\"100000\" y=\"100000\"/><{p}to x=\"0\" y=\"0\"/></{p}animScale>{}",
                    visibility(v, dur.saturating_sub(1), "hidden")
                ),
            )
        }
        (AnimationClass::Entrance, AnimationLook::Float(side)) => {
            let (v, f) = (next(), next());
            let sub = if side == AnimationSide::Bottom { 2 } else { 1 };
            (
                30,
                sub,
                format!(
                    "{}<{p}animEffect transition=\"in\" filter=\"fade\"><{p}cBhvr><{p}cTn id=\"{f}\" dur=\"{dur}\"/>{target}</{p}cBhvr></{p}animEffect>",
                    visibility(v, 0, "visible")
                ),
            )
        }
        (AnimationClass::Exit, AnimationLook::Float(side)) => {
            let (f, v) = (next(), next());
            let sub = if side == AnimationSide::Bottom { 2 } else { 1 };
            (
                30,
                sub,
                format!(
                    "<{p}animEffect transition=\"out\" filter=\"fade\"><{p}cBhvr><{p}cTn id=\"{f}\" dur=\"{dur}\"/>{target}</{p}cBhvr></{p}animEffect>{}",
                    visibility(v, dur.saturating_sub(1), "hidden")
                ),
            )
        }
        (AnimationClass::Entrance, AnimationLook::Wipe(side)) => {
            let (v, f) = (next(), next());
            let sub = side_subtype(side);
            (
                22,
                sub,
                format!(
                    "{}<{p}animEffect transition=\"in\" filter=\"wipe\"><{p}cBhvr><{p}cTn id=\"{f}\" dur=\"{dur}\"/>{target}</{p}cBhvr></{p}animEffect>",
                    visibility(v, 0, "visible")
                ),
            )
        }
        (AnimationClass::Exit, AnimationLook::Wipe(side)) => {
            let (f, v) = (next(), next());
            let sub = side_subtype(side);
            (
                22,
                sub,
                format!(
                    "<{p}animEffect transition=\"out\" filter=\"wipe\"><{p}cBhvr><{p}cTn id=\"{f}\" dur=\"{dur}\"/>{target}</{p}cBhvr></{p}animEffect>{}",
                    visibility(v, dur.saturating_sub(1), "hidden")
                ),
            )
        }
        (AnimationClass::Entrance, AnimationLook::Replay) => {
            let (v, f) = (next(), next());
            (
                28,
                0,
                format!(
                    "{}<{p}animEffect transition=\"in\" filter=\"fade\"><{p}cBhvr><{p}cTn id=\"{f}\" dur=\"{dur}\"/>{target}</{p}cBhvr></{p}animEffect>",
                    visibility(v, 0, "visible")
                ),
            )
        }
        (AnimationClass::Exit, AnimationLook::Replay) => {
            let (f, v) = (next(), next());
            (
                28,
                0,
                format!(
                    "<{p}animEffect transition=\"out\" filter=\"fade\"><{p}cBhvr><{p}cTn id=\"{f}\" dur=\"{dur}\"/>{target}</{p}cBhvr></{p}animEffect>{}",
                    visibility(v, dur.saturating_sub(1), "hidden")
                ),
            )
        }
        (AnimationClass::Emphasis, AnimationLook::Spin(by)) => {
            let n = next();
            (8, 0, format!("<{p}animRot by=\"{by}\"><{p}cBhvr><{p}cTn id=\"{n}\" dur=\"{dur}\" fill=\"hold\"/>{target}<{p}attrNameLst><{p}attrName>r</{p}attrName></{p}attrNameLst></{p}cBhvr></{p}animRot>"))
        }
        (AnimationClass::Emphasis, AnimationLook::Scale(x, y)) => {
            let n = next();
            (6, 0, format!("<{p}animScale><{p}cBhvr><{p}cTn id=\"{n}\" dur=\"{dur}\" fill=\"hold\"/>{target}</{p}cBhvr><{p}by x=\"{x}\" y=\"{y}\"/></{p}animScale>"))
        }
        (AnimationClass::Path, AnimationLook::Motion(x, y)) => {
            let n = next();
            let fx = x as f64 / 100_000.0;
            let fy = y as f64 / 100_000.0;
            let subtype = if x == 0 && y > 0 {
                4
            } else if x < 0 && y == 0 {
                8
            } else if x > 0 && y == 0 {
                2
            } else if x == 0 && y < 0 {
                1
            } else {
                a.preset.1
            };
            let preset = if a.preset.0 != 0 { a.preset.0 } else { 61 };
            // カスタム軌跡: a line to each point (preset 0, the custom path).
            let (preset, subtype, path) = if a.path.is_empty() {
                (preset, subtype, format!("M 0 0 L {fx} {fy} E"))
            } else {
                let lines: String = a.path.iter().map(|&(x, y)| format!(" L {} {}", x as f64 / 100_000.0, y as f64 / 100_000.0)).collect();
                (0, 0, format!("M 0 0{lines} E"))
            };
            (
                preset,
                subtype,
                format!(
                    r#"<{p}animMotion origin="layout" path="{path}" pathEditMode="relative" ptsTypes=""><{p}cBhvr><{p}cTn id="{n}" dur="{dur}" fill="hold"/>{target}</{p}cBhvr></{p}animMotion>"#
                ),
            )
        }
        _ => return Err(unsupported("このアニメーションの書き出し")),
    };
    let fill = match a.after_effect {
        AnimationAfterEffect::Hide => "remove",
        _ => "hold",
    };
    let repeat_attr = match a.repeat_count {
        AnimationRepeat::None => String::new(),
        AnimationRepeat::Count(n) => format!(" repeatCount=\"{}\"", n * 1000),
        AnimationRepeat::UntilNextClick | AnimationRepeat::UntilEndOfSlide => " repeatCount=\"indefinite\"".to_string(),
    };
    let auto_rev_attr = if a.auto_reverse { " autoRev=\"1\"" } else { "" };
    let rewind_attr = if a.rewind { " rewind=\"1\"" } else { "" };
    let iterate = match a.text_unit {
        Some((unit, gap)) => {
            let kind = if unit == ibis_doc_model::TextUnit::Word { "wd" } else { "lt" };
            let gap = match gap {
                ibis_doc_model::UnitGap::Percent(v) => format!("<{p}tmPct val=\"{v}\"/>"),
                ibis_doc_model::UnitGap::Millis(v) => format!("<{p}tmAbs val=\"{v}\"/>"),
            };
            format!("<{p}iterate type=\"{kind}\">{gap}</{p}iterate>")
        }
        None => String::new(),
    };
    let accel_attr = if a.accel > 0 { format!(" accel=\"{}\"", a.accel) } else { String::new() };
    let decel_attr = if a.decel > 0 { format!(" decel=\"{}\"", a.decel) } else { String::new() };
    Ok(format!(
        "<{p}par><{p}cTn id=\"{effect_id}\" presetID=\"{preset}\" presetClass=\"{class}\" presetSubtype=\"{subtype}\" fill=\"{fill}\"{repeat_attr}{auto_rev_attr}{rewind_attr}{accel_attr}{decel_attr} grpId=\"0\" nodeType=\"{node}\"><{p}stCondLst><{p}cond delay=\"{delay}\"/></{p}stCondLst>{iterate}<{p}childTnLst>{behaviours}</{p}childTnLst></{p}cTn></{p}par>"
    ))
}

/// The slide's `p:timing` for `animations` (`spid` names each shape's
/// drawing id), with prefix `p`.
fn timing_xml(p: &str, animations: &[Animation], spid: &dyn Fn(ShapeId) -> Option<String>) -> Result<String, Error> {
    let mut id = 3u32;
    let mut clicks = String::new();
    // 1. Main sequence animations (trigger == None)
    let main_animations: Vec<&Animation> = animations.iter().filter(|a| a.trigger.is_none()).collect();
    // Click groups: each a list of timed groups, each (delay, effects).
    let mut groups: Vec<Vec<(u32, Vec<&Animation>)>> = Vec::new();
    for &a in &main_animations {
        match (a.start, groups.last_mut()) {
            (AnimationStart::OnClick, _) | (_, None) => groups.push(vec![(0, vec![a])]),
            (AnimationStart::WithPrevious, Some(g)) => g.last_mut().expect("a timed group").1.push(a),
            (AnimationStart::AfterPrevious, Some(g)) => {
                // After everything before it in the click group has ended.
                let ended = g.iter().map(|(at, effects)| effects.iter().map(|e| at + e.delay + e.duration).max().unwrap_or(*at)).max().unwrap_or(0);
                g.push((ended, vec![a]));
            }
        }
    }
    for (i, group) in groups.iter().enumerate() {
        let click_id = id;
        id += 1;
        let auto = i == 0 && group.first().and_then(|t| t.1.first()).is_some_and(|a| a.start != AnimationStart::OnClick);
        let cond = if auto { format!("<{p}cond delay=\"indefinite\"/><{p}cond evt=\"onBegin\" delay=\"0\"><{p}tn val=\"2\"/></{p}cond>") } else { format!("<{p}cond delay=\"indefinite\"/>") };
        let mut timed_xml = String::new();
        for (at, effects) in group {
            let timed_id = id;
            id += 1;
            let mut inner = String::new();
            for a in effects {
                let shape = spid(a.shape).ok_or_else(|| unsupported("この図形のアニメーション"))?;
                inner.push_str(&effect_xml(p, a, &shape, a.delay, &mut id)?);
            }
            timed_xml.push_str(&format!("<{p}par><{p}cTn id=\"{timed_id}\" fill=\"hold\"><{p}stCondLst><{p}cond delay=\"{at}\"/></{p}stCondLst><{p}childTnLst>{inner}</{p}childTnLst></{p}cTn></{p}par>"));
        }
        clicks.push_str(&format!("<{p}par><{p}cTn id=\"{click_id}\" fill=\"hold\"><{p}stCondLst>{cond}</{p}stCondLst><{p}childTnLst>{timed_xml}</{p}childTnLst></{p}cTn></{p}par>"));
    }

    // 2. Interactive sequences (triggers)
    let mut interactive_seqs = String::new();
    let mut trigger_groups: Vec<(AnimationTrigger, Vec<&Animation>)> = Vec::new();
    for a in animations {
        if a.trigger.is_none() {
            continue;
        }
        if let Some(pos) = trigger_groups.iter().position(|(t, _)| t == &a.trigger) {
            trigger_groups[pos].1.push(a);
        } else {
            trigger_groups.push((a.trigger.clone(), vec![a]));
        }
    }

    for (trigger, group_anims) in &trigger_groups {
        let seq_c_id = id;
        id += 1;
        let trigger_cond = match trigger {
            AnimationTrigger::OnShapeClick(s) => {
                let trigger_spid = spid(*s).ok_or_else(|| unsupported("トリガー図形のアニメーション"))?;
                format!("<{p}cond evt=\"onClick\" delay=\"0\"><{p}tgtEl><{p}spTgt spid=\"{trigger_spid}\"/></{p}tgtEl></{p}cond>")
            }
            AnimationTrigger::OnMediaBookmark { shape, bookmark } => {
                let trigger_spid = spid(*shape).ok_or_else(|| unsupported("トリガーメディア図形のアニメーション"))?;
                let bmk_escaped = bookmark.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
                format!("<{p}cond evt=\"onBegin\" delay=\"0\"><p14:mediaBmKTrigger bmkName=\"{bmk_escaped}\"><p14:tgtEl><p14:spTgt spid=\"{trigger_spid}\"/></p14:tgtEl></p14:mediaBmKTrigger></{p}cond>")
            }
            AnimationTrigger::None => continue,
        };

        let mut timed_xml = String::new();
        let timed_id = id;
        id += 1;
        let mut inner = String::new();
        for a in group_anims {
            let shape = spid(a.shape).ok_or_else(|| unsupported("この図形のアニメーション"))?;
            inner.push_str(&effect_xml(p, a, &shape, a.delay, &mut id)?);
        }
        timed_xml.push_str(&format!("<{p}par><{p}cTn id=\"{timed_id}\" fill=\"hold\"><{p}stCondLst><{p}cond delay=\"0\"/></{p}stCondLst><{p}childTnLst>{inner}</{p}childTnLst></{p}cTn></{p}par>"));

        interactive_seqs.push_str(&format!(
            "<{p}seq concurrent=\"1\" nextAc=\"seek\"><{p}cTn id=\"{seq_c_id}\" dur=\"indefinite\" nodeType=\"interactiveSeq\"><{p}stCondLst>{trigger_cond}</{p}stCondLst><{p}childTnLst>{timed_xml}</{p}childTnLst></{p}cTn><{p}nextCondLst><{p}cond evt=\"onNext\" delay=\"0\"><{p}tgtEl><{p}sldTgt/></{p}tgtEl></{p}cond></{p}nextCondLst></{p}seq>"
        ));
    }

    // The shapes built paragraph by paragraph, chart element, or diagram (`p:bldLst`).
    let mut built_p: Vec<String> = Vec::new();
    let mut built_chart: Vec<(String, ibis_doc_model::ChartBuildStep, bool)> = Vec::new();
    let mut built_dgm: Vec<(String, ibis_doc_model::DiagramBuildStep)> = Vec::new();
    for a in animations {
        if a.paragraph.is_some() {
            if let Some(shape) = spid(a.shape)
                && !built_p.contains(&shape)
            {
                built_p.push(shape);
            }
        }
        if let Some((step, anim_bg)) = a.chart_build {
            if let Some(shape) = spid(a.shape)
                && !built_chart.iter().any(|(s, _, _)| s == &shape)
            {
                built_chart.push((shape, step, anim_bg));
            }
        }
        if let Some(step) = a.dgm_build {
            if let Some(shape) = spid(a.shape)
                && !built_dgm.iter().any(|(s, _)| s == &shape)
            {
                built_dgm.push((shape, step));
            }
        }
    }
    let mut bld_entries = String::new();
    for s in &built_p {
        bld_entries.push_str(&format!("<{p}bldP spid=\"{s}\" grpId=\"0\" build=\"p\"/>"));
    }
    for (s, step, anim_bg) in &built_chart {
        let anim_bg_val = if *anim_bg { "1" } else { "0" };
        bld_entries.push_str(&format!(
            "<{p}bldGraphic spid=\"{s}\" grpId=\"0\"><{p}bldSub><a:bldChart bld=\"{}\" animBg=\"{anim_bg_val}\"/></{p}bldSub></{p}bldGraphic>",
            step.as_str()
        ));
    }
    for (s, step) in &built_dgm {
        bld_entries.push_str(&format!("<{p}bldDgm spid=\"{s}\" grpId=\"0\" bld=\"{}\"/>", step.as_str()));
    }
    let build_list = if bld_entries.is_empty() { String::new() } else { format!("<{p}bldLst>{bld_entries}</{p}bldLst>") };
    let main_seq = if clicks.is_empty() {
        String::new()
    } else {
        format!(
            "<{p}seq concurrent=\"1\" nextAc=\"seek\"><{p}cTn id=\"2\" dur=\"indefinite\" nodeType=\"mainSeq\"><{p}childTnLst>{clicks}</{p}childTnLst></{p}cTn><{p}prevCondLst><{p}cond evt=\"onPrev\" delay=\"0\"><{p}tgtEl><{p}sldTgt/></{p}tgtEl></{p}cond></{p}prevCondLst><{p}nextCondLst><{p}cond evt=\"onNext\" delay=\"0\"><{p}tgtEl><{p}sldTgt/></{p}tgtEl></{p}cond></{p}nextCondLst></{p}seq>"
        )
    };
    Ok(format!(
        "<{p}timing xmlns:p14=\"http://schemas.microsoft.com/office/powerpoint/2010/main\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><{p}tnLst><{p}par><{p}cTn id=\"1\" dur=\"indefinite\" restart=\"never\" nodeType=\"tmRoot\"><{p}childTnLst>{main_seq}{interactive_seqs}</{p}childTnLst></{p}cTn></{p}par></{p}tnLst>{build_list}</{p}timing>"
    ))
}

/// Write `animations` as the slide's timing (its document `doc`), its
/// shapes' drawing ids `native`; none takes the timing away.
pub(crate) fn write_timing(doc: &mut XmlDocument, animations: &[Animation], native: &HashMap<ShapeId, String>) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("スライドが空です"))?;
    let old = doc.child(root, "timing");
    // The playing of videos and audios is not an animation of the model: kept as it is.
    let media = old.map(|t| super::pptx_media_timing::take(doc, t)).unwrap_or_default();
    if let Some(t) = old
        && !rewritable(doc, t)
    {
        // Put back what was taken: nothing is changed.
        super::pptx_media_timing::restore(doc, media)?;
        return Err(unsupported("トリガー・段落ごとの再生・音などのあるアニメーションの変更"));
    }
    if let Some(t) = old {
        doc.detach(t);
    }
    if animations.is_empty() {
        return super::pptx_media_timing::restore(doc, media);
    }
    let x = Prefixes::at(doc, root);
    let xml = timing_xml(&x.p, animations, &|s| native.get(&s).cloned())?;
    let timing = x.import(doc, &xml)?;
    // After the transition (and an AlternateContent holding one), before the extension list.
    let at = doc.children(root).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(root).len());
    doc.insert(root, at, timing);
    super::pptx_media_timing::restore(doc, media)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn effect(shape: u64, class: AnimationClass, start: AnimationStart, look: AnimationLook, duration: u32) -> Animation {
        Animation {
            shape: ShapeId(shape),
            class,
            start,
            look,
            delay: 0,
            duration,
            preset: (0, 0),
            repeat_count: AnimationRepeat::None,
            auto_reverse: false,
            rewind: false,
            accel: 0,
            decel: 0,
            after_effect: ibis_doc_model::AnimationAfterEffect::Hold,
            trigger: AnimationTrigger::None,
            paragraph: None, chars: None, text_unit: None, path: Vec::new(),
            chart_element: None, chart_build: None,
            dgm_element: None, dgm_build: None,
        }
    }

    #[test]
    fn diagram_animation_timing() {
        let mut a = effect(1, AnimationClass::Entrance, AnimationStart::OnClick, AnimationLook::Appear, 1);
        a.dgm_element = Some(ibis_doc_model::DiagramAnimationElement { id: "2".to_string(), bld_step: "sp".to_string() });
        a.dgm_build = Some(ibis_doc_model::DiagramBuildStep::DepthByNode);
        let xml = timing_xml("p:", &[a], &|s| Some(format!("{}", s.0 + 3))).unwrap();
        assert!(xml.contains("<p:graphicEl><a:dgm id=\"{00000002-0000-0000-0000-000000000000}\" bldStep=\"sp\"/></p:graphicEl>"));
        assert!(xml.contains("<p:bldDgm spid=\"4\" grpId=\"0\" bld=\"depthByNode\"/>"));
        assert!(XmlDocument::parse(format!("<r xmlns:p=\"urn:p\" xmlns:a=\"urn:a\">{xml}</r>").as_bytes()).is_ok());
    }

    #[test]
    fn ink_replay_animation_timing() {
        let a = effect(1, AnimationClass::Entrance, AnimationStart::OnClick, AnimationLook::Replay, 2000);
        let xml = timing_xml("p:", &[a], &|s| Some(format!("{}", s.0 + 3))).unwrap();
        assert!(xml.contains("presetID=\"28\" presetClass=\"entr\""));
        assert!(xml.contains("<p:spTgt spid=\"4\"/>"));
        assert!(XmlDocument::parse(format!("<r xmlns:p=\"urn:p\">{xml}</r>").as_bytes()).is_ok());
    }

    #[test]
    fn chart_animation_timing() {
        let mut a = effect(1, AnimationClass::Entrance, AnimationStart::OnClick, AnimationLook::Appear, 1);
        a.chart_element = Some(ibis_doc_model::ChartAnimationElement { series_idx: 0, category_idx: -1 });
        a.chart_build = Some((ibis_doc_model::ChartBuildStep::Series, true));
        let xml = timing_xml("p:", &[a], &|s| Some(format!("{}", s.0 + 5))).unwrap();
        assert!(xml.contains("<p:cEl><p:cChartEl seriesIdx=\"0\" categoryIdx=\"-1\"/></p:cEl>"));
        assert!(xml.contains("<p:bldGraphic spid=\"6\" grpId=\"0\"><p:bldSub><a:bldChart bld=\"series\" animBg=\"1\"/></p:bldSub></p:bldGraphic>"));
        assert!(XmlDocument::parse(format!("<r xmlns:p=\"urn:p\" xmlns:a=\"urn:a\">{xml}</r>").as_bytes()).is_ok());
    }

    #[test]
    fn the_main_sequence_groups_by_click_and_after() {
        let a = [
            effect(1, AnimationClass::Entrance, AnimationStart::AfterPrevious, AnimationLook::Fade, 500),
            effect(2, AnimationClass::Entrance, AnimationStart::OnClick, AnimationLook::Fly(AnimationSide::Bottom), 500),
            effect(1, AnimationClass::Exit, AnimationStart::WithPrevious, AnimationLook::Appear, 1),
        ];
        let xml = timing_xml("p:", &a, &|s| Some(format!("{}", s.0 + 3))).unwrap();
        // With the slide (its first group starts without a click), then a click.
        assert!(xml.contains("<p:cond evt=\"onBegin\" delay=\"0\"><p:tn val=\"2\"/></p:cond>"));
        assert_eq!(xml.matches("<p:cond delay=\"indefinite\"/>").count(), 2);
        assert!(xml.contains("presetID=\"2\" presetClass=\"entr\" presetSubtype=\"4\""));
        assert!(xml.contains("<p:spTgt spid=\"5\"/>"));
        assert!(XmlDocument::parse(format!("<r xmlns:p=\"urn:p\">{xml}</r>").as_bytes()).is_ok());
    }

    #[test]
    fn interactive_sequences_for_triggers() {
        let mut a1 = effect(1, AnimationClass::Entrance, AnimationStart::OnClick, AnimationLook::Fade, 500);
        a1.trigger = AnimationTrigger::OnShapeClick(ShapeId(2));
        let mut a2 = effect(1, AnimationClass::Emphasis, AnimationStart::OnClick, AnimationLook::Spin(180_000), 1000);
        a2.trigger = AnimationTrigger::OnMediaBookmark { shape: ShapeId(3), bookmark: "Bookmark 1".to_string() };
        let a = [a1, a2];
        let xml = timing_xml("p:", &a, &|s| Some(format!("{}", s.0 + 10))).unwrap();
        assert!(xml.contains("nodeType=\"interactiveSeq\""));
        assert!(xml.contains("<p:cond evt=\"onClick\" delay=\"0\"><p:tgtEl><p:spTgt spid=\"12\"/></p:tgtEl></p:cond>"));
        assert!(xml.contains("p14:mediaBmKTrigger bmkName=\"Bookmark 1\""));
        assert!(xml.contains("<p14:spTgt spid=\"13\"/>"));
        assert!(XmlDocument::parse(format!("<r xmlns:p=\"urn:p\">{xml}</r>").as_bytes()).is_ok());
    }
}
