// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! 画面切り替えの効果の一覧（PowerPoint のギャラリー）と、その効果のオプション。
//!
//! 効果は `p:transition` の子要素で表す（ECMA-376 Part 1 19.5.91 以下。`p:fade`・`p:push`…）。PowerPoint 2010 の効果は
//! `p14:` の要素（`p14:vortex`・`p14:prism`…、`mc:AlternateContent` の `Requires="p14"` の中）、2013 以降の効果は
//! `p15:prstTrans`（`prst` が名前：`crush`・`airplane`…）。効果のオプション（方向など）は要素の属性（`dir`・`orient`・`pattern`・
//! `spokes`・`isInverted`・`invX`…）。読み込み・設定画面・保存・再生がこの表を共通に使う。

/// Which namespace an effect's element is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionNs {
    /// The base schema (`p:`).
    P,
    /// PowerPoint 2010 (`p14:`).
    P14,
    /// PowerPoint 2013 (`p15:`, `prstTrans`).
    P15,
    /// PowerPoint 2016's 変形 (`p159:`).
    P159,
}

impl TransitionNs {
    pub const fn uri(self) -> &'static str {
        match self {
            Self::P => "http://schemas.openxmlformats.org/presentationml/2006/main",
            Self::P14 => "http://schemas.microsoft.com/office/powerpoint/2010/main",
            Self::P15 => "http://schemas.microsoft.com/office/powerpoint/2012/main",
            Self::P159 => "http://schemas.microsoft.com/office/powerpoint/2015/09/main",
        }
    }
}

/// An option of an effect (効果のオプション): the attributes it is on the element.
pub struct TransitionOption {
    /// What the model keeps in `Transition::direction`.
    pub token: &'static str,
    pub attrs: &'static [(&'static str, &'static str)],
    /// The i18n id of its name.
    pub key: &'static str,
}

/// One effect of the gallery.
pub struct TransitionEffect {
    /// The i18n id of its name (`transition.<id>`).
    pub key: &'static str,
    /// What the model keeps in `Transition::effect` (the element's name, except for the effects of one element that are
    /// several: `cube`/`box`/`rotate`/`orbit` of `p14:prism`, and each `p15:prstTrans`).
    pub id: &'static str,
    pub element: &'static str,
    pub ns: TransitionNs,
    /// Attributes that make it this effect and not another of the same element (`prst`, `isContent`…).
    pub fixed: &'static [(&'static str, &'static str)],
    pub options: &'static [TransitionOption],
}

const fn opt(token: &'static str, attrs: &'static [(&'static str, &'static str)], key: &'static str) -> TransitionOption {
    TransitionOption { token, attrs, key }
}

macro_rules! fx {
    ($id:literal, $element:literal, $ns:expr, $fixed:expr, $options:expr $(,)?) => {
        TransitionEffect { key: concat!("transition.", $id), id: $id, element: $element, ns: $ns, fixed: $fixed, options: $options }
    };
}

use TransitionNs::{P, P14, P15, P159};

const DIR_DLRU: &[TransitionOption] = &[
    opt("d", &[("dir", "d")], "transition.dir.down"),
    opt("l", &[("dir", "l")], "transition.dir.left"),
    opt("r", &[("dir", "r")], "transition.dir.right"),
    opt("u", &[("dir", "u")], "transition.dir.up"),
];
const DIR_8: &[TransitionOption] = &[
    opt("d", &[("dir", "d")], "transition.dir.down"),
    opt("l", &[("dir", "l")], "transition.dir.left"),
    opt("r", &[("dir", "r")], "transition.dir.right"),
    opt("u", &[("dir", "u")], "transition.dir.up"),
    opt("ld", &[("dir", "ld")], "transition.dir.left-down"),
    opt("lu", &[("dir", "lu")], "transition.dir.left-up"),
    opt("rd", &[("dir", "rd")], "transition.dir.right-down"),
    opt("ru", &[("dir", "ru")], "transition.dir.right-up"),
];
const DIR_LR: &[TransitionOption] = &[opt("l", &[("dir", "l")], "transition.dir.left"), opt("r", &[("dir", "r")], "transition.dir.right")];
const DIR_HV: &[TransitionOption] = &[opt("horz", &[("dir", "horz")], "transition.dir.horz"), opt("vert", &[("dir", "vert")], "transition.dir.vert")];
const DIR_IN_OUT: &[TransitionOption] = &[opt("in", &[("dir", "in")], "transition.dir.in"), opt("out", &[("dir", "out")], "transition.dir.out")];
const THRU_BLACK: &[TransitionOption] = &[opt("", &[], "transition.dir.smoothly"), opt("thruBlk", &[("thruBlk", "1")], "transition.dir.through-black")];
const LEFT_RIGHT_INV: &[TransitionOption] = &[opt("", &[], "transition.dir.left"), opt("invX", &[("invX", "1")], "transition.dir.right")];

/// The effects, in the order PowerPoint's gallery lists them (subtle, then exciting, then dynamic).
pub const EFFECTS: &[TransitionEffect] = &[
    fx!("cut", "cut", P, &[], THRU_BLACK),
    fx!("morph", "morph", P159, &[], &[opt("byObject", &[("option", "byObject")], "transition.dir.by-object"), opt("byWord", &[("option", "byWord")], "transition.dir.by-word"), opt("byChar", &[("option", "byChar")], "transition.dir.by-char")]),
    fx!("fade", "fade", P, &[], THRU_BLACK),
    fx!("push", "push", P, &[], DIR_DLRU),
    fx!("wipe", "wipe", P, &[], DIR_DLRU),
    fx!(
        "split",
        "split",
        P,
        &[],
        &[
            opt("in", &[("orient", "vert"), ("dir", "in")], "transition.dir.split-in"),
            opt("out", &[("orient", "vert"), ("dir", "out")], "transition.dir.split-out"),
            opt("horzIn", &[("orient", "horz"), ("dir", "in")], "transition.dir.horz-in"),
            opt("horzOut", &[("orient", "horz"), ("dir", "out")], "transition.dir.horz-out"),
        ],
    ),
    fx!("reveal", "reveal", P14, &[], &[opt("l", &[("dir", "l")], "transition.dir.left"), opt("r", &[("dir", "r")], "transition.dir.right")]),
    fx!("randomBar", "randomBar", P, &[], DIR_HV),
    fx!("circle", "circle", P, &[], &[]),
    fx!("diamond", "diamond", P, &[], &[]),
    fx!("plus", "plus", P, &[], &[]),
    fx!("wedge", "wedge", P, &[], &[]),
    fx!("cover", "cover", P, &[], DIR_8),
    fx!("pull", "pull", P, &[], DIR_8),
    fx!("flash", "flash", P14, &[], &[]),
    fx!("dissolve", "dissolve", P, &[], &[]),
    fx!("checker", "checker", P, &[], DIR_HV),
    fx!("blinds", "blinds", P, &[], DIR_HV),
    fx!(
        "wheel",
        "wheel",
        P,
        &[],
        &[
            opt("1", &[("spokes", "1")], "transition.dir.spokes-1"),
            opt("2", &[("spokes", "2")], "transition.dir.spokes-2"),
            opt("3", &[("spokes", "3")], "transition.dir.spokes-3"),
            opt("4", &[("spokes", "4")], "transition.dir.spokes-4"),
            opt("8", &[("spokes", "8")], "transition.dir.spokes-8"),
        ],
    ),
    fx!(
        "wheelReverse",
        "wheelReverse",
        P14,
        &[],
        &[
            opt("1", &[("spokes", "1")], "transition.dir.spokes-1"),
            opt("2", &[("spokes", "2")], "transition.dir.spokes-2"),
            opt("3", &[("spokes", "3")], "transition.dir.spokes-3"),
            opt("4", &[("spokes", "4")], "transition.dir.spokes-4"),
            opt("8", &[("spokes", "8")], "transition.dir.spokes-8"),
        ],
    ),
    fx!("ripple", "ripple", P14, &[], &[
        opt("center", &[("dir", "center")], "transition.dir.center"),
        opt("lu", &[("dir", "lu")], "transition.dir.left-up"),
        opt("ru", &[("dir", "ru")], "transition.dir.right-up"),
        opt("ld", &[("dir", "ld")], "transition.dir.left-down"),
        opt("rd", &[("dir", "rd")], "transition.dir.right-down"),
    ]),
    fx!("honeycomb", "honeycomb", P14, &[], &[]),
    fx!("glitter", "glitter", P14, &[], DIR_DLRU),
    fx!("vortex", "vortex", P14, &[], DIR_DLRU),
    fx!(
        "shred",
        "shred",
        P14,
        &[],
        &[
            opt("strip-in", &[("pattern", "strip"), ("dir", "in")], "transition.dir.strips-in"),
            opt("strip-out", &[("pattern", "strip"), ("dir", "out")], "transition.dir.strips-out"),
            opt("rect-in", &[("pattern", "rectangle"), ("dir", "in")], "transition.dir.rects-in"),
            opt("rect-out", &[("pattern", "rectangle"), ("dir", "out")], "transition.dir.rects-out"),
        ],
    ),
    fx!("switch", "switch", P14, &[], DIR_LR),
    fx!("flip", "flip", P14, &[], DIR_LR),
    fx!("gallery", "gallery", P14, &[], DIR_LR),
    fx!("cube", "prism", P14, &[("isContent", "0"), ("isInverted", "0")], DIR_DLRU),
    fx!("doors", "doors", P14, &[], DIR_HV),
    fx!("box", "prism", P14, &[("isContent", "0"), ("isInverted", "1")], DIR_DLRU),
    fx!("zoom", "zoom", P, &[], DIR_IN_OUT),
    fx!("random", "random", P, &[], &[]),
    fx!("pan", "pan", P14, &[], DIR_DLRU),
    fx!("ferris", "ferris", P14, &[], DIR_LR),
    fx!("conveyor", "conveyor", P14, &[], DIR_LR),
    fx!("rotate", "prism", P14, &[("isContent", "1"), ("isInverted", "0")], DIR_DLRU),
    fx!("window", "window", P14, &[], DIR_HV),
    fx!("orbit", "prism", P14, &[("isContent", "1"), ("isInverted", "1")], DIR_DLRU),
    fx!("flythrough", "flythrough", P14, &[], DIR_IN_OUT),
    fx!("warp", "warp", P14, &[], DIR_IN_OUT),
    fx!("comb", "comb", P, &[], DIR_HV),
    fx!("strips", "strips", P, &[], &[
        opt("ld", &[("dir", "ld")], "transition.dir.left-down"),
        opt("lu", &[("dir", "lu")], "transition.dir.left-up"),
        opt("rd", &[("dir", "rd")], "transition.dir.right-down"),
        opt("ru", &[("dir", "ru")], "transition.dir.right-up"),
    ]),
    fx!("newsflash", "newsflash", P, &[], &[]),
    fx!("fallOver", "prstTrans", P15, &[("prst", "fallOver")], LEFT_RIGHT_INV),
    fx!("drape", "prstTrans", P15, &[("prst", "drape")], LEFT_RIGHT_INV),
    fx!("curtains", "prstTrans", P15, &[("prst", "curtains")], &[]),
    fx!("wind", "prstTrans", P15, &[("prst", "wind")], LEFT_RIGHT_INV),
    fx!("prestige", "prstTrans", P15, &[("prst", "prestige")], &[]),
    fx!("fracture", "prstTrans", P15, &[("prst", "fracture")], &[]),
    fx!("crush", "prstTrans", P15, &[("prst", "crush")], &[]),
    fx!("peelOff", "prstTrans", P15, &[("prst", "peelOff")], LEFT_RIGHT_INV),
    fx!("pageCurlDouble", "prstTrans", P15, &[("prst", "pageCurlDouble")], LEFT_RIGHT_INV),
    fx!("pageCurlSingle", "prstTrans", P15, &[("prst", "pageCurlSingle")], LEFT_RIGHT_INV),
    fx!("airplane", "prstTrans", P15, &[("prst", "airplane")], LEFT_RIGHT_INV),
    fx!("origami", "prstTrans", P15, &[("prst", "origami")], LEFT_RIGHT_INV),
];

/// The effect the model names `id`.
pub fn effect_of(id: &str) -> Option<&'static TransitionEffect> {
    EFFECTS.iter().find(|e| e.id == id)
}

/// Whether attribute `name` is a flag (absent is false).
fn is_flag(name: &str) -> bool {
    matches!(name, "isContent" | "isInverted" | "thruBlk" | "invX" | "invY" | "hasBounce")
}

/// What attribute `name` of `element` is when the file omits it (the schema's default; PowerPoint leaves a default out).
fn default_of(element: &str, name: &str) -> Option<&'static str> {
    match (element, name) {
        ("push" | "wipe" | "cover" | "pull" | "vortex" | "switch" | "flip" | "gallery" | "conveyor" | "ferris" | "pan" | "reveal" | "prism", "dir") => Some("l"),
        ("blinds" | "checker" | "comb" | "randomBar" | "doors" | "window", "dir") => Some("horz"),
        ("strips", "dir") => Some("lu"),
        ("zoom" | "split" | "warp", "dir") => Some("out"),
        ("flythrough" | "shred", "dir") => Some("in"),
        ("ripple", "dir") => Some("center"),
        ("glitter", "dir") => Some("r"),
        ("split", "orient") => Some("horz"),
        ("shred", "pattern") => Some("strip"),
        ("glitter", "pattern") => Some("diamond"),
        ("wheel" | "wheelReverse", "spokes") => Some("4"),
        ("morph", "option") => Some("byObject"),
        _ => None,
    }
}

/// Whether the attribute `name` of `element`, as `get` finds it (or its default), is `want`.
fn attr_is(element: &str, get: &dyn Fn(&str) -> Option<String>, name: &str, want: &str) -> bool {
    let have = get(name);
    if is_flag(name) {
        let on = matches!(have.as_deref(), Some("1" | "true"));
        return on == (want == "1");
    }
    have.as_deref().or(default_of(element, name)) == Some(want)
}

/// The effect (and its option) an element in `ns` named `element` with the attributes `get` finds is: `None` for one the gallery
/// does not list.
pub fn identify(ns: TransitionNs, element: &str, get: &dyn Fn(&str) -> Option<String>) -> Option<(&'static TransitionEffect, Option<&'static TransitionOption>)> {
    let effect = EFFECTS.iter().find(|e| e.ns == ns && e.element == element && e.fixed.iter().all(|(n, v)| attr_is(element, get, n, v)))?;
    // The option whose attributes are all as written; the one with the most attributes wins (`orient`+`dir` over `dir`).
    let option = effect
        .options
        .iter()
        .filter(|o| o.attrs.iter().all(|(n, v)| attr_is(element, get, n, v)))
        .max_by_key(|o| o.attrs.len())
        // An option without attributes is the effect without any of them.
        .filter(|o| !o.attrs.is_empty() || !effect.options.iter().any(|x| !x.attrs.is_empty() && x.attrs.iter().all(|(n, v)| attr_is(element, get, n, v))));
    Some((effect, option))
}

/// The option shown for effect `id` whose direction is `token` (`None`: the file leaves the attributes out, which is the option
/// the schema's defaults make).
pub fn effective_token(id: &str, token: Option<&str>) -> Option<&'static str> {
    let effect = effect_of(id)?;
    if let Some(found) = token.and_then(|t| effect.options.iter().find(|o| o.token == t)) {
        return Some(found.token);
    }
    let fixed = |n: &str| effect.fixed.iter().find(|(k, _)| *k == n).map(|(_, v)| (*v).to_string());
    identify(effect.ns, effect.element, &fixed).and_then(|(_, o)| o).map(|o| o.token)
}

/// The attributes the element of effect `id` with option `token` is written with (the effect's own, then the option's).
pub fn attributes(id: &str, token: Option<&str>) -> Vec<(&'static str, &'static str)> {
    let Some(effect) = effect_of(id) else { return Vec::new() };
    let mut out: Vec<(&'static str, &'static str)> = effect.fixed.iter().copied().filter(|(n, v)| !(is_flag(n) && *v == "0")).collect();
    if let Some(option) = token.and_then(|t| effect.options.iter().find(|o| o.token == t)) {
        out.extend(option.attrs.iter().copied());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs<'a>(list: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |n| list.iter().find(|(k, _)| *k == n).map(|(_, v)| v.to_string())
    }

    #[test]
    fn effects_are_told_apart_by_their_attributes() {
        let (e, o) = identify(P15, "prstTrans", &attrs(&[("prst", "crush")])).unwrap();
        assert_eq!((e.id, o.map(|o| o.token)), ("crush", None));
        let (e, o) = identify(P15, "prstTrans", &attrs(&[("prst", "airplane"), ("invX", "1")])).unwrap();
        assert_eq!((e.id, o.map(|o| o.token)), ("airplane", Some("invX")));
        // p14:prism: cube, box, rotate, orbit by its two flags.
        assert_eq!(identify(P14, "prism", &attrs(&[("dir", "u")])).unwrap().0.id, "cube");
        assert_eq!(identify(P14, "prism", &attrs(&[("isInverted", "1")])).unwrap().0.id, "box");
        assert_eq!(identify(P14, "prism", &attrs(&[("isContent", "1"), ("isInverted", "0")])).unwrap().0.id, "rotate");
        assert_eq!(identify(P14, "prism", &attrs(&[("isContent", "true"), ("isInverted", "true"), ("dir", "r")])).map(|(e, o)| (e.id, o.map(|o| o.token))), Some(("orbit", Some("r"))));
        // split: the option has two attributes.
        assert_eq!(identify(P, "split", &attrs(&[("orient", "horz"), ("dir", "in")])).unwrap().1.map(|o| o.token), Some("horzIn"));
        assert_eq!(identify(P, "push", &attrs(&[("dir", "u")])).unwrap().1.map(|o| o.token), Some("u"));
        // Through black.
        assert_eq!(identify(P, "fade", &attrs(&[("thruBlk", "1")])).unwrap().1.map(|o| o.token), Some("thruBlk"));
        assert_eq!(identify(P, "fade", &attrs(&[])).unwrap().1.map(|o| o.token), Some(""));
        assert!(identify(P14, "unknownEffect", &attrs(&[])).is_none());
    }

    #[test]
    fn an_option_is_written_back_as_the_attributes_it_was_read_from() {
        assert_eq!(attributes("split", Some("horzOut")), vec![("orient", "horz"), ("dir", "out")]);
        assert_eq!(attributes("crush", None), vec![("prst", "crush")]);
        assert_eq!(attributes("orbit", Some("l")), vec![("isContent", "1"), ("isInverted", "1"), ("dir", "l")]);
        assert_eq!(attributes("cube", Some("d")), vec![("dir", "d")]);
        assert_eq!(attributes("fade", Some("thruBlk")), vec![("thruBlk", "1")]);
        // Reading what is written gives the same effect and option, for every one of the gallery.
        for e in EFFECTS {
            for o in e.options {
                let written = attributes(e.id, Some(o.token));
                let get = |n: &str| written.iter().find(|(k, _)| *k == n).map(|(_, v)| v.to_string());
                let (back, back_opt) = identify(e.ns, e.element, &get).unwrap();
                assert_eq!((back.id, back_opt.map(|b| b.token)), (e.id, Some(o.token)), "{} {}", e.id, o.token);
            }
        }
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = EFFECTS.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), EFFECTS.len());
    }
}
