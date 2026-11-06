// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 画面切り替え: a slide's `p:transition`, written in place of the one it
//! had (and of the `mc:AlternateContent` PowerPoint 2010 wraps one in).
//! The duration goes in `p14:dur` (in a choice for PowerPoint 2010 and
//! later, with `spd` alone as the fallback), as PowerPoint writes it.

use ibis_doc_model::transition_catalog::{self as catalog, TransitionNs};
use ibis_doc_model::Transition;
use ibis_ooxml::{NodeId, Package, XmlDocument};

use super::pptx_new::escape;
use super::unsupported;
use crate::Error;

const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const NS_P14: &str = "http://schemas.microsoft.com/office/powerpoint/2010/main";
const NS_P15: &str = "http://schemas.microsoft.com/office/powerpoint/2012/main";
const NS_P159: &str = "http://schemas.microsoft.com/office/powerpoint/2015/09/main";
const REL_AUDIO: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/audio";
const SOUND_PART: &str = "ppt/media/transition_sound.wav";

// 最小の無音 WAV データ（RIFF PCM 16-bit 8000Hz mono、0.01秒 = 80サンプル = 160バイトのゼロ）
// ヘッダ 44 バイト + データ 160 バイト = 204 バイト
const SILENT_WAV: &[u8] = &[
    b'R', b'I', b'F', b'F',
    196, 0, 0, 0, // total_len - 8 = 204 - 8 = 196
    b'W', b'A', b'V', b'E',
    b'f', b'm', b't', b' ',
    16, 0, 0, 0, // Subchunk1Size (16 for PCM)
    1, 0,        // AudioFormat (1 = PCM)
    1, 0,        // NumChannels (1 = mono)
    0x40, 0x1F, 0, 0, // SampleRate = 8000
    0x80, 0x3E, 0, 0, // ByteRate = 8000 * 2 = 16000
    2, 0,        // BlockAlign = 2
    16, 0,       // BitsPerSample = 16
    b'd', b'a', b't', b'a',
    160, 0, 0, 0, // Subchunk2Size = 160
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// `p:transition` written with prefixes `p` (`p:`) and, for its duration
/// and the newer effects, `p14` (`None`: the fallback, without them).
fn transition_xml(t: &Transition, p: &str, p14: Option<&str>, extra: Option<(TransitionNs, &str)>, sound_embed: Option<(&str, &str)>) -> String {
    let spd = speed(t.duration);
    let mut attrs = format!(" spd=\"{spd}\"");
    if let Some(p14) = p14 {
        attrs.push_str(&format!(" {p14}dur=\"{}\"", t.duration));
    }
    if !t.on_click {
        attrs.push_str(" advClick=\"0\"");
    }
    if let Some(after) = t.after {
        attrs.push_str(&format!(" advTm=\"{after}\""));
    }
    let base = Transition::BASE_EFFECTS.contains(&t.effect.as_str());
    let effect = match (catalog::effect_of(&t.effect), t.effect.as_str()) {
        (_, "") => String::new(),
        // An effect of the gallery, with its option as the attributes it is read from (without the namespace it needs, a fade).
        (Some(def), id) => {
            let mut attrs: String = catalog::attributes(id, t.direction.as_deref().filter(|d| !d.is_empty())).iter().map(|(n, v)| format!(" {n}=\"{v}\"")).collect();
            if id == "morph" && t.direction.is_none() {
                attrs = " option=\"byObject\"".to_string();
            }
            let element = def.element;
            match def.ns {
                TransitionNs::P => format!("<{p}{element}{attrs}/>"),
                TransitionNs::P14 => match p14 {
                    Some(p14) => format!("<{p14}{element}{attrs}/>"),
                    None => format!("<{p}fade/>"),
                },
                ns => match extra {
                    Some((have, prefix)) if have == ns => format!("<{prefix}{element}{attrs}/>"),
                    _ => format!("<{p}fade/>"),
                },
            }
        }
        (None, name) => match (base, p14) {
            (true, _) => format!("<{p}{name}{}/>", direction(t)),
            (false, Some(p14)) => format!("<{p14}{name}{}/>", direction(t)),
            // Without PowerPoint 2010's names, a fade.
            (false, None) => format!("<{p}fade/>"),
        },
    };
    let sound = match &t.sound {
        None => String::new(),
        Some(ibis_doc_model::TransitionSound::Stop) => format!("<{p}sndAc><{p}endSnd/></{p}sndAc>"),
        Some(ibis_doc_model::TransitionSound::Play { name, loop_sound }) => {
            let loop_attr = if *loop_sound { " loop=\"1\"" } else { "" };
            let embed_attr = match sound_embed {
                Some((r, rid)) => format!(" {r}embed=\"{rid}\""),
                None => String::new(),
            };
            format!("<{p}sndAc><{p}stSnd{loop_attr}><{p}snd{embed_attr} name=\"{}\"/></{p}stSnd></{p}sndAc>", escape(name))
        }
    };
    format!("<{p}transition{attrs}>{effect}{sound}</{p}transition>")
}

fn direction(t: &Transition) -> String {
    let Some(d) = t.direction.as_deref() else { return String::new() };
    if t.effect == "split" {
        match d {
            "in" => " orient=\"vert\" dir=\"in\"".to_string(),
            "out" => " orient=\"vert\" dir=\"out\"".to_string(),
            "horzIn" => " orient=\"horz\" dir=\"in\"".to_string(),
            "horzOut" => " orient=\"horz\" dir=\"out\"".to_string(),
            _ => format!(" dir=\"{}\"", escape(d)),
        }
    } else {
        format!(" dir=\"{}\"", escape(d))
    }
}

fn speed(duration:u32)->&'static str {match duration {0..=600=>"fast",601..=875=>"med",_=>"slow"}}

fn timing_attributes(doc:&mut XmlDocument,node:NodeId,t:&Transition,modern:Option<&str>) {
    doc.set_attr(node,"spd",speed(t.duration));
    if t.on_click {doc.remove_attr(node,"advClick");} else {doc.set_attr(node,"advClick","0");}
    if let Some(after)=t.after {doc.set_attr(node,"advTm",&after.to_string());} else {doc.remove_attr(node,"advTm");}
    let duration=doc.attributes(node).iter().find(|a|a.name.rsplit_once(':').is_some_and(|(p,n)|n=="dur" && doc.resolve_prefix(node,Some(p))==Some(NS_P14))).map(|a|a.name.clone());
    if let Some(p14)=modern {let name=duration.unwrap_or_else(||format!("{p14}dur"));doc.set_attr(node,&name,&t.duration.to_string());} else if let Some(name)=duration {doc.remove_attr(node,&name);}
}

// Timing-only edits retain effect details, sound actions and opaque source data.
fn preserve_timing(doc:&mut XmlDocument,root:NodeId,old:&[NodeId],t:&Transition)->Result<(),Error> {
    if old.len()!=1 {return Err(unsupported("重複または未知の切り替え期間変更"));}
    let node=old[0];
    if doc.namespace(node)==Some(super::pptx_new::NS_P) {
        if t.duration==500 {timing_attributes(doc,node,t,None);return Ok(());}
        let mc=prefix(doc,root,NS_MC,"mc");let p14=prefix(doc,root,NS_P14,"p14");
        let at=doc.children(root).iter().position(|&n|n==node).unwrap();let fallback=doc.deep_clone(node);let ac=doc.insert_element(root,at,&format!("{mc}AlternateContent"));let choice=doc.append_element(ac,&format!("{mc}Choice"));doc.set_attr(choice,"Requires",p14.trim_end_matches(':'));let fb=doc.append_element(ac,&format!("{mc}Fallback"));doc.insert(choice,0,node);doc.insert(fb,0,fallback);timing_attributes(doc,node,t,Some(&p14));timing_attributes(doc,fallback,t,None);return Ok(());
    }
    let branches:Vec<_>=doc.elements(node).filter(|&n|doc.namespace(n)==Some(NS_MC)&&matches!(doc.local_name(n),"Choice"|"Fallback")).collect();
    let first_choice=branches.iter().copied().find(|&n|doc.local_name(n)=="Choice");
    let modern=|branch|doc.local_name(branch)=="Choice" && doc.attr(branch,"Requires").is_some_and(|v|v.split_whitespace().any(|p|doc.resolve_prefix(branch,Some(p))==Some(NS_P14)));
    if t.duration!=500 && !first_choice.is_some_and(modern) {return Err(unsupported("この代替切り替えの期間変更"));}
    let targets:Vec<_>=branches.into_iter().filter_map(|branch|doc.elements(branch).find(|&n|doc.namespace(n)==Some(super::pptx_new::NS_P)&&doc.local_name(n)=="transition").map(|n|(n,modern(branch)))).collect();
    if targets.is_empty() {return Err(unsupported("この切り替えの期間変更"));}
    let p14=prefix(doc,root,NS_P14,"p14");for (node,modern) in targets {timing_attributes(doc,node,t,modern.then_some(p14.as_str()));}Ok(())
}

/// The prefix `doc` binds `ns` to at `node`, with its colon; or `fallback:`
/// declared on `node` when it binds none.
fn prefix(doc: &mut XmlDocument, node: NodeId, ns: &str, fallback: &str) -> String {
    if let Some(p)=doc.lookup_prefix(node,ns) {if !p.is_empty() {return format!("{p}:");}if ns!=NS_P14 {return String::new();}}
    let mut name=fallback.to_string();let mut suffix=1;
    while doc.resolve_prefix(node,Some(&name)).is_some_and(|bound|bound!=ns) {name=format!("{fallback}{suffix}");suffix+=1;}
    doc.set_attr(node,&format!("xmlns:{name}"),ns);format!("{name}:")
}

/// Write `transition` as slide part `part`'s.
pub(crate) fn write_transition(package: &mut Package, part: &str, before: Option<&Transition>, transition: Option<&Transition>) -> Result<(), Error> {
    if transition.is_some_and(|t|t.after.is_some_and(|v|v>i32::MAX as u32)) {return Err(unsupported("自動送り時間の範囲"));}

    // 音声ファイルが指定されている場合、無音 WAV を配置しリレーションシップを解決（doc 借用前に実行）
    let sound_rid = if let Some(Some(ibis_doc_model::TransitionSound::Play { .. })) = transition.map(|t| &t.sound) {
        if !package.contains(SOUND_PART) {
            package.set_bytes(SOUND_PART, SILENT_WAV.to_vec());
        }
        if package.content_type(SOUND_PART)?.is_none() {
            super::pptx_shapes::add_default_content_type(package, "wav", "audio/wav")?;
        }
        let rel_target = super::pptx::relative(part, SOUND_PART);
        let existing = package.relationships(part)?.into_iter().find(|r| r.kind == REL_AUDIO && r.target == rel_target);
        match existing {
            Some(r) => Some(r.id),
            None => Some(package.add_relationship(part, REL_AUDIO, &rel_target, false)?),
        }
    } else {
        None
    };

    let doc = package.xml_mut(part)?;
    let root = doc.root_element().ok_or_else(|| Error::new(format!("{part} が空です")))?;
    // The old one, plain or wrapped.
    let old: Vec<NodeId> = doc
        .children(root)
        .iter()
        .copied()
        .filter(|&n| doc.is_element(n))
        .filter(|&n| match doc.local_name(n) {
            "transition" => doc.namespace(n)==Some(super::pptx_new::NS_P),
            "AlternateContent" => doc.namespace(n)==Some(NS_MC) && doc.descendants(n).into_iter().any(|d| doc.is_element(d) && doc.namespace(d)==Some(super::pptx_new::NS_P) && doc.local_name(d) == "transition"),
            _ => false,
        })
        .collect();
    if let (Some(before),Some(t))=(before,transition) {
        if before.effect==t.effect && before.direction==t.direction && before.sound==t.sound {return preserve_timing(doc,root,&old,t);}
    }
    let at = old.first().and_then(|o| doc.children(root).iter().position(|c| c == o));
    for n in old {
        doc.detach(n);
    }
    let Some(t) = transition else { return Ok(()) };

    let sound_embed = sound_rid.map(|rid| {
        let r_prefix = {
            let p = doc.lookup_prefix(root, super::pptx_new::NS_R);
            match p {
                Some(p) if !p.is_empty() => format!("{p}:"),
                _ => {
                    doc.set_attr(root, "xmlns:r", super::pptx_new::NS_R);
                    "r:".to_string()
                }
            }
        };
        (r_prefix, rid)
    });

    let p = prefix(doc, root, super::pptx_new::NS_P, "p");
    // The namespaces the effect needs besides `p14` (the duration): 変形 (`p159`), PowerPoint 2013's effects (`p15`).
    let extra_ns = catalog::effect_of(&t.effect).map(|d| d.ns).filter(|n| matches!(n, TransitionNs::P15 | TransitionNs::P159));
    let needs_p14 = catalog::effect_of(&t.effect).is_some_and(|d| d.ns == TransitionNs::P14);
    let plain = t.duration == 500 && !needs_p14 && extra_ns.is_none() && (t.effect.is_empty() || catalog::effect_of(&t.effect).map_or_else(|| Transition::BASE_EFFECTS.contains(&t.effect.as_str()), |d| d.ns == TransitionNs::P));
    let (mc, p14) = if plain { (String::new(), String::new()) } else { (prefix(doc, root, NS_MC, "mc"), prefix(doc, root, NS_P14, "p14")) };
    let (extra_prefix, extra_uri) = match extra_ns {
        Some(TransitionNs::P15) => (prefix(doc, root, NS_P15, "p15"), NS_P15),
        Some(_) => (prefix(doc, root, NS_P159, "p159"), NS_P159),
        None => (String::new(), ""),
    };
    let extra = extra_ns.map(|ns| (ns, extra_prefix.as_str()));
    let embed_ref = sound_embed.as_ref().map(|(r, rid)| (r.as_str(), rid.as_str()));
    let xml = if plain {
        transition_xml(t, &p, None, None, embed_ref)
    } else {
        format!(
            "<{mc}AlternateContent><{mc}Choice Requires=\"{}\">{}</{mc}Choice><{mc}Fallback>{}</{mc}Fallback></{mc}AlternateContent>",
            if extra_ns.is_some() { extra_prefix.trim_end_matches(':') } else { p14.trim_end_matches(':') },
            transition_xml(t, &p, Some(&p14), extra, embed_ref),
            transition_xml(t, &p, None, None, embed_ref)
        )
    };
    // Parsed with the prefixes the slide binds (declared on the slide).
    let mut wrapper = String::from("<w");
    let mut bound = vec![(p.as_str(), super::pptx_new::NS_P)];
    if !plain {
        bound.extend([(mc.as_str(), NS_MC), (p14.as_str(), NS_P14)]);
        if extra_ns.is_some() {
            bound.push((extra_prefix.as_str(), extra_uri));
        }
    }
    if let Some((r, _)) = &sound_embed {
        bound.push((r.as_str(), super::pptx_new::NS_R));
    }
    for (name, ns) in bound {
        match name.strip_suffix(':') {
            Some(prefix) => wrapper.push_str(&format!(" xmlns:{prefix}=\"{ns}\"")),
            None => wrapper.push_str(&format!(" xmlns=\"{ns}\"")),
        }
    }
    wrapper.push_str(&format!(">{xml}</w>"));
    let parsed = XmlDocument::parse(wrapper.as_bytes()).map_err(|e| Error::new(format!("切り替え効果を書けません: {e}")))?;
    let w = parsed.root_element().ok_or_else(|| Error::new("切り替え効果を書けません"))?;
    let element = parsed.elements(w).next().ok_or_else(|| Error::new("切り替え効果を書けません"))?;
    let id = doc.import(&parsed, element);
    // After `p:cSld` and `p:clrMapOvr`, before `p:timing` and `p:extLst`.
    let index = at.unwrap_or_else(|| {
        doc.children(root)
            .iter()
            .position(|&c| doc.is_element(c) && matches!(doc.local_name(c), "timing" | "extLst"))
            .unwrap_or(doc.children(root).len())
    });
    doc.insert(root, index, id);
    Ok(())
}
