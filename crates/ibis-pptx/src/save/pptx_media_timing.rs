// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 動画・音声の再生のタイミング: how PowerPoint plays a video or an audio clip.
//!
//! A clip (`p:pic` with `a:videoFile` / `a:audioFile` and `p14:media`) is
//! played by what is in the slide's `p:timing`, not by the picture:
//!
//! - an effect of class `mediacall` in the main sequence whose `p:cmd` calls
//!   `playFrom(0.0)` (a click of its own, or started with the slide: 自動),
//! - a `p:video` / `p:audio` holding the `p:cMediaNode` (volume, `numSld`,
//!   `showWhenStopped`, full screen; `repeatCount` and `fill` on its `p:cTn`),
//! - for a video, an `interactiveSeq` that pauses and resumes it by a click
//!   on the video (`togglePause`).
//!
//! Without them the picture is only a picture: the animation pane lists no
//! 再生 and the slide show cannot play it. The options (volume, loop…) are not
//! attributes of `p14:media` (it has `r:embed` and `r:link` and the trim,
//! fade and bookmarks only).

use ibis_doc_model::{ClipOptions, MediaStart, MediaVolume};
use ibis_ooxml::{NodeId, XmlDocument};

use super::pptx_new::Prefixes;
use crate::Error;

/// A clip to play: the drawing id of its picture, whether it is an audio, its options,
/// and how long it plays (milliseconds, `None` when the file cannot be measured).
pub(crate) struct MediaPlay {
    pub spid: String,
    pub audio: bool,
    pub options: ClipOptions,
    pub duration_ms: Option<u32>,
}

/// The volume as PowerPoint's `vol` (1/1000 %).
pub(crate) fn volume_value(volume: MediaVolume) -> u32 {
    match volume {
        MediaVolume::High => 100_000,
        MediaVolume::Medium => 66_000,
        MediaVolume::Low => 33_000,
        MediaVolume::Mute => 0,
    }
}

/// Play length of a file in milliseconds: the movie header of an MP4 / MOV / M4A
/// (`mvhd`: time scale and duration), or the size of a WAV's data over its byte rate.
pub(crate) fn duration_ms(data: &[u8]) -> Option<u32> {
    let be32 = |at: usize| data.get(at..at + 4).map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]));
    let le32 = |at: usize| data.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WAVE") {
        let mut at = 12;
        let mut byte_rate = None;
        while at + 8 <= data.len() {
            let size = le32(at + 4)? as usize;
            match data.get(at..at + 4)? {
                b"fmt " => byte_rate = le32(at + 16),
                b"data" => {
                    let rate = byte_rate?.max(1) as u64;
                    return Some((size as u64 * 1000 / rate).min(u32::MAX as u64) as u32);
                }
                _ => {}
            }
            at += 8 + size + (size & 1);
        }
        return None;
    }
    let at = data.windows(4).position(|w| w == b"mvhd")? + 4;
    let version = *data.get(at)?;
    let (scale, length) = if version == 1 {
        let hi = be32(at + 4 + 16 + 4)? as u64;
        let lo = be32(at + 4 + 16 + 8)? as u64;
        (be32(at + 4 + 16)?, hi << 32 | lo)
    } else {
        (be32(at + 4 + 8)?, be32(at + 4 + 12)? as u64)
    };
    if scale == 0 {
        return None;
    }
    Some((length * 1000 / scale as u64).min(u32::MAX as u64) as u32)
}

fn max_ctn_id(doc: &XmlDocument, from: NodeId) -> u32 {
    doc.descendants(from).into_iter().filter(|&n| doc.is_element(n) && doc.local_name(n) == "cTn").filter_map(|n| doc.attr(n, "id")?.parse().ok()).max().unwrap_or(0)
}

fn has_node_type(doc: &XmlDocument, seq: NodeId, node_type: &str) -> bool {
    doc.child(seq, "cTn").is_some_and(|c| doc.attr(c, "nodeType") == Some(node_type))
}

/// The `p:childTnLst` of the root time node, and the main sequence's list in it:
/// made when the slide has no timing (or no main sequence).
fn skeleton(doc: &mut XmlDocument, x: &Prefixes) -> Result<(NodeId, NodeId), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("スライドが空です"))?;
    let p = &x.p;
    if doc.child(root, "timing").is_none() {
        let xml = format!("<{p}timing><{p}tnLst><{p}par><{p}cTn id=\"1\" dur=\"indefinite\" restart=\"never\" nodeType=\"tmRoot\"><{p}childTnLst/></{p}cTn></{p}par></{p}tnLst></{p}timing>");
        let timing = x.import(doc, &xml)?;
        let at = doc.children(root).iter().position(|&c| doc.is_element(c) && doc.local_name(c) == "extLst").unwrap_or(doc.children(root).len());
        doc.insert(root, at, timing);
    }
    let timing = doc.child(root, "timing").ok_or_else(|| Error::new("タイミングを作れません"))?;
    let tn_lst = doc.child(timing, "tnLst").ok_or_else(|| Error::new("タイミングが壊れています"))?;
    let tm_root = doc.child(tn_lst, "par").and_then(|par| doc.child(par, "cTn")).and_then(|c| doc.child(c, "childTnLst")).ok_or_else(|| Error::new("タイミングが壊れています"))?;
    let existing = doc.elements(tm_root).find(|&s| doc.local_name(s) == "seq" && has_node_type(doc, s, "mainSeq"));
    let seq = match existing {
        Some(s) => s,
        None => {
            let xml = format!(
                "<{p}seq concurrent=\"1\" nextAc=\"seek\"><{p}cTn id=\"2\" dur=\"indefinite\" nodeType=\"mainSeq\"><{p}childTnLst/></{p}cTn><{p}prevCondLst><{p}cond evt=\"onPrev\" delay=\"0\"><{p}tgtEl><{p}sldTgt/></{p}tgtEl></{p}cond></{p}prevCondLst><{p}nextCondLst><{p}cond evt=\"onNext\" delay=\"0\"><{p}tgtEl><{p}sldTgt/></{p}tgtEl></{p}cond></{p}nextCondLst></{p}seq>"
            );
            let s = x.import(doc, &xml)?;
            doc.insert(tm_root, 0, s);
            s
        }
    };
    let main_list = doc.child(seq, "cTn").and_then(|c| doc.child(c, "childTnLst")).ok_or_else(|| Error::new("タイミングが壊れています"))?;
    Ok((tm_root, main_list))
}

/// The media node (`p:cMediaNode`) of the picture with drawing id `spid`.
fn media_node_of(doc: &XmlDocument, timing: NodeId, spid: &str) -> Option<NodeId> {
    doc.descendants(timing).into_iter().find(|&n| {
        doc.is_element(n)
            && doc.local_name(n) == "cMediaNode"
            && doc.descendants(n).into_iter().any(|t| doc.is_element(t) && doc.local_name(t) == "spTgt" && doc.attr(t, "spid") == Some(spid))
    })
}

/// Bring the timing in line with the clips: those with no media node get the whole
/// playing set; those with one (`update`) get their options set on it.
pub(crate) fn sync(doc: &mut XmlDocument, clips: &[MediaPlay], update: bool) -> Result<(), Error> {
    let root = doc.root_element().ok_or_else(|| Error::new("スライドが空です"))?;
    let x = Prefixes::at(doc, root);
    for clip in clips {
        let existing = doc.child(root, "timing").and_then(|t| media_node_of(doc, t, &clip.spid));
        match existing {
            Some(node) => {
                if update {
                    set_options(doc, node, clip);
                }
            }
            None => add(doc, &x, clip)?,
        }
    }
    Ok(())
}

/// Options on an existing media node: `vol`, `showWhenStopped`, `numSld`, `fullScrn`, the loop and the rewind.
fn set_options(doc: &mut XmlDocument, node: NodeId, clip: &MediaPlay) {
    let o = &clip.options;
    doc.set_attr(node, "vol", &volume_value(o.volume).to_string());
    let hide = if clip.audio { o.hide_during_show } else { o.hide_while_not_playing };
    if hide {
        doc.set_attr(node, "showWhenStopped", "0");
    } else {
        doc.remove_attr(node, "showWhenStopped");
    }
    if o.play_across_slides {
        doc.set_attr(node, "numSld", "999");
    } else {
        doc.remove_attr(node, "numSld");
    }
    if let Some(holder) = doc.parent(node)
        && !clip.audio
    {
        if o.full_screen {
            doc.set_attr(holder, "fullScrn", "1");
        } else {
            doc.remove_attr(holder, "fullScrn");
        }
    }
    if let Some(ctn) = doc.child(node, "cTn") {
        if o.loop_until_stopped {
            doc.set_attr(ctn, "repeatCount", "indefinite");
        } else {
            doc.remove_attr(ctn, "repeatCount");
        }
        doc.set_attr(ctn, "fill", if o.rewind { "remove" } else { "hold" });
    }
}

fn add(doc: &mut XmlDocument, x: &Prefixes, clip: &MediaPlay) -> Result<(), Error> {
    let (tm_root, main_list) = skeleton(doc, x)?;
    let p = &x.p;
    let o = &clip.options;
    let spid = &clip.spid;
    let timing = doc.root_element().and_then(|r| doc.child(r, "timing")).ok_or_else(|| Error::new("タイミングが壊れています"))?;
    let mut id = max_ctn_id(doc, timing) + 1;
    let mut next = || {
        let n = id;
        id += 1;
        n
    };
    let auto = o.start == MediaStart::Automatically;
    let (outer, inner, effect, call) = (next(), next(), next(), next());
    let dur = clip.duration_ms.map_or("indefinite".to_string(), |d| d.to_string());
    let target = format!("<{p}tgtEl><{p}spTgt spid=\"{spid}\"/></{p}tgtEl>");
    let node_type = if auto { "afterEffect" } else { "clickEffect" };
    let cond = if auto { format!("<{p}cond delay=\"indefinite\"/><{p}cond evt=\"onBegin\" delay=\"0\"><{p}tn val=\"2\"/></{p}cond>") } else { format!("<{p}cond delay=\"indefinite\"/>") };
    let click = format!(
        "<{p}par><{p}cTn id=\"{outer}\" fill=\"hold\"><{p}stCondLst>{cond}</{p}stCondLst><{p}childTnLst><{p}par><{p}cTn id=\"{inner}\" fill=\"hold\"><{p}stCondLst><{p}cond delay=\"0\"/></{p}stCondLst><{p}childTnLst><{p}par><{p}cTn id=\"{effect}\" presetID=\"1\" presetClass=\"mediacall\" presetSubtype=\"0\" fill=\"hold\" nodeType=\"{node_type}\"><{p}stCondLst><{p}cond delay=\"0\"/></{p}stCondLst><{p}childTnLst><{p}cmd type=\"call\" cmd=\"playFrom(0.0)\"><{p}cBhvr><{p}cTn id=\"{call}\" dur=\"{dur}\" fill=\"hold\"/>{target}</{p}cBhvr></{p}cmd></{p}childTnLst></{p}cTn></{p}par></{p}childTnLst></{p}cTn></{p}par></{p}childTnLst></{p}cTn></{p}par>"
    );
    // Started with the slide: the first click group of the sequence, joined when it already starts with the slide.
    let first = doc.elements(main_list).next();
    let first_auto = first.is_some_and(|f| doc.descendants(f).into_iter().any(|n| doc.is_element(n) && doc.local_name(n) == "tn" && doc.attr(n, "val") == Some("2")));
    let click_node = x.import(doc, &click)?;
    if auto && first_auto {
        // Into the group that starts with the slide, next to what is there (with it).
        let group_list = first.and_then(|f| doc.child(f, "cTn")).and_then(|c| doc.child(c, "childTnLst")).ok_or_else(|| Error::new("タイミングが壊れています"))?;
        let timed = doc.elements(click_node).next().and_then(|c| doc.child(c, "childTnLst")).and_then(|l| doc.elements(l).next()).ok_or_else(|| Error::new("タイミングが壊れています"))?;
        if let Some(effect_ctn) = doc.descendants(timed).into_iter().find(|&n| doc.is_element(n) && doc.attr(n, "presetClass") == Some("mediacall")) {
            doc.set_attr(effect_ctn, "nodeType", "withEffect");
        }
        let at = doc.children(group_list).len();
        doc.insert(group_list, at, timed);
    } else if auto {
        doc.insert(main_list, 0, click_node);
    } else {
        let at = doc.children(main_list).len();
        doc.insert(main_list, at, click_node);
    }
    // The media node, after the main sequence.
    let media_id = next();
    let mut attrs = format!(" vol=\"{}\"", volume_value(o.volume));
    if o.play_across_slides {
        attrs += " numSld=\"999\"";
    }
    if if clip.audio { o.hide_during_show } else { o.hide_while_not_playing } {
        attrs += " showWhenStopped=\"0\"";
    }
    let mut ctn_attrs = String::new();
    if o.loop_until_stopped {
        ctn_attrs += " repeatCount=\"indefinite\"";
    }
    let fill = if o.rewind { "remove" } else { "hold" };
    let mut start = format!("<{p}cond delay=\"indefinite\"/>");
    if auto {
        start += &format!("<{p}cond evt=\"onBegin\" delay=\"0\"><{p}tn val=\"{effect}\"/></{p}cond>");
    }
    let end = if clip.audio && !o.play_across_slides { format!("<{p}endCondLst><{p}cond evt=\"onStopAudio\" delay=\"0\"><{p}tgtEl><{p}sldTgt/></{p}tgtEl></{p}cond></{p}endCondLst>") } else { String::new() };
    let (holder, holder_attrs) = if clip.audio { ("audio", String::new()) } else { ("video", if o.full_screen { " fullScrn=\"1\"".to_string() } else { String::new() }) };
    let media = format!(
        "<{p}{holder}{holder_attrs}><{p}cMediaNode{attrs}><{p}cTn id=\"{media_id}\" fill=\"{fill}\" display=\"0\"{ctn_attrs}><{p}stCondLst>{start}</{p}stCondLst>{end}</{p}cTn>{target}</{p}cMediaNode></{p}{holder}>"
    );
    let node = x.import(doc, &media)?;
    let after = doc.children(tm_root).iter().rposition(|&c| doc.is_element(c) && matches!(doc.local_name(c), "seq" | "audio" | "video") && !(doc.local_name(c) == "seq" && !has_node_type(doc, c, "mainSeq"))).map_or(0, |i| i + 1);
    doc.insert(tm_root, after, node);
    // A video: a click on it pauses and resumes it (`togglePause`).
    if !clip.audio {
        let (seq_id, a, b, c, d) = (next(), next(), next(), next(), next());
        let click_target = format!("<{p}cond evt=\"onClick\" delay=\"0\"><{p}tgtEl><{p}spTgt spid=\"{spid}\"/></{p}tgtEl></{p}cond>");
        let toggle = format!(
            "<{p}seq concurrent=\"1\" nextAc=\"seek\"><{p}cTn id=\"{seq_id}\" restart=\"whenNotActive\" fill=\"hold\" evtFilter=\"cancelBubble\" nodeType=\"interactiveSeq\"><{p}stCondLst>{click_target}</{p}stCondLst><{p}endSync evt=\"end\" delay=\"0\"><{p}rtn val=\"all\"/></{p}endSync><{p}childTnLst><{p}par><{p}cTn id=\"{a}\" fill=\"hold\"><{p}stCondLst><{p}cond delay=\"0\"/></{p}stCondLst><{p}childTnLst><{p}par><{p}cTn id=\"{b}\" fill=\"hold\"><{p}stCondLst><{p}cond delay=\"0\"/></{p}stCondLst><{p}childTnLst><{p}par><{p}cTn id=\"{c}\" presetID=\"2\" presetClass=\"mediacall\" presetSubtype=\"0\" fill=\"hold\" nodeType=\"clickEffect\"><{p}stCondLst><{p}cond delay=\"0\"/></{p}stCondLst><{p}childTnLst><{p}cmd type=\"call\" cmd=\"togglePause\"><{p}cBhvr><{p}cTn id=\"{d}\" dur=\"1\" fill=\"hold\"/>{target}</{p}cBhvr></{p}cmd></{p}childTnLst></{p}cTn></{p}par></{p}childTnLst></{p}cTn></{p}par></{p}childTnLst></{p}cTn></{p}par></{p}childTnLst></{p}cTn><{p}nextCondLst>{click_target}</{p}nextCondLst></{p}seq>"
        );
        let seq = x.import(doc, &toggle)?;
        let at = doc.children(tm_root).len();
        doc.insert(tm_root, at, seq);
    }
    Ok(())
}

/// The playing set of clips taken out of `timing` before it is rebuilt from the
/// animations (which do not hold them): the click groups whose effects are all
/// `mediacall`, the `p:video` / `p:audio` nodes and the interactive sequences of
/// the calls. They are put back by [`restore`].
pub(crate) fn take(doc: &mut XmlDocument, timing: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    let Some(tm_root) = doc.child(timing, "tnLst").and_then(|l| doc.child(l, "par")).and_then(|p| doc.child(p, "cTn")).and_then(|c| doc.child(c, "childTnLst")) else { return out };
    let has_cmd = |doc: &XmlDocument, n: NodeId| doc.descendants(n).into_iter().any(|d| doc.is_element(d) && doc.local_name(d) == "cmd");
    let only_media = |doc: &XmlDocument, n: NodeId| {
        let classes: Vec<_> = doc.descendants(n).into_iter().filter_map(|d| if doc.is_element(d) && doc.local_name(d) == "cTn" { doc.attr(d, "presetClass") } else { None }).collect();
        classes.iter().all(|c| *c == "mediacall")
    };
    let children: Vec<NodeId> = doc.elements(tm_root).collect();
    for child in children {
        match doc.local_name(child) {
            "audio" | "video" => out.push(child),
            "seq" if has_node_type(doc, child, "interactiveSeq") && has_cmd(doc, child) && only_media(doc, child) => out.push(child),
            "seq" if has_node_type(doc, child, "mainSeq") => {
                let Some(list) = doc.child(child, "cTn").and_then(|c| doc.child(c, "childTnLst")) else { continue };
                let groups: Vec<NodeId> = doc.elements(list).collect();
                for group in groups {
                    if has_cmd(doc, group) && only_media(doc, group) {
                        out.push(group);
                    }
                }
            }
            _ => {}
        }
    }
    for &n in &out {
        doc.detach(n);
    }
    out
}

/// Put what [`take`] took back into the (rebuilt) timing, its time nodes numbered anew.
pub(crate) fn restore(doc: &mut XmlDocument, pieces: Vec<NodeId>) -> Result<(), Error> {
    if pieces.is_empty() {
        return Ok(());
    }
    let root = doc.root_element().ok_or_else(|| Error::new("スライドが空です"))?;
    let x = Prefixes::at(doc, root);
    let (tm_root, main_list) = skeleton(doc, &x)?;
    let timing = doc.child(root, "timing").ok_or_else(|| Error::new("タイミングが壊れています"))?;
    let mut id = max_ctn_id(doc, timing) + 1;
    for piece in pieces {
        for n in doc.descendants(piece) {
            if doc.is_element(n) && doc.local_name(n) == "cTn" {
                doc.set_attr(n, "id", &id.to_string());
                id += 1;
            }
        }
        match doc.local_name(piece) {
            "par" => {
                let at = doc.children(main_list).len();
                doc.insert(main_list, at, piece);
            }
            _ => {
                let at = doc.children(tm_root).len();
                doc.insert(tm_root, at, piece);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_of_wav_and_mp4() {
        // A WAV of one second: 8000 bytes/s, 8000 bytes of data.
        let mut wav = b"RIFF".to_vec();
        wav.extend_from_slice(&0u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&[1, 0, 1, 0]);
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend_from_slice(&[1, 0, 8, 0]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&8000u32.to_le_bytes());
        wav.extend(std::iter::repeat_n(0u8, 8000));
        assert_eq!(duration_ms(&wav), Some(1000));
        // `mvhd` version 0: time scale 600, duration 3600 → 6 s.
        let mut mp4 = vec![0, 0, 0, 0x6c];
        mp4.extend_from_slice(b"mvhd");
        mp4.extend_from_slice(&[0, 0, 0, 0]);
        mp4.extend_from_slice(&[0; 8]);
        mp4.extend_from_slice(&600u32.to_be_bytes());
        mp4.extend_from_slice(&3600u32.to_be_bytes());
        assert_eq!(duration_ms(&mp4), Some(6000));
    }
}
