// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! アニメーションの効果を、`p:timing` の動作（behaviour: `p:set`・`p:anim`・
//! `p:animEffect`・`p:animScale`）のまま持つ。PowerPoint の開始・終了の効果
//! （`presetID` ごと）は、図形の位置・大きさ・回転・透明度を式
//! （`#ppt_x` など、ECMA-376 Part 1 19.5.1 `p:anim` の `p:tav`）で動かし、
//! `p:animEffect` の `filter`（`wipe(down)`・`blinds(horizontal)` など）で
//! 見える範囲を決める。ここは、その動作を読み込んだとおりに保ち、時刻 `ms` の
//! 状態を計算する（描画は画面側）。

use crate::AnimationClass;

/// One behaviour of an effect (a child of the effect's `p:cTn`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimBehavior {
    pub kind: BehaviorKind,
    /// When it begins after the effect starts (`stCondLst/cond@delay`), milliseconds.
    pub begin: u32,
    /// How long it takes (`cTn@dur`), milliseconds.
    pub dur: u32,
    /// How its own time runs (`cTn@accel`, `@decel`, `@autoRev`; `cBhvr@additive`).
    pub time: BehaviorTime,
}

/// The time options of one behaviour, as the file writes them on its `p:cTn` and `p:cBhvr`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BehaviorTime {
    /// Smooth start and end (`accel`, `decel`), in 1/1000 % of its time.
    pub accel: u32,
    pub decel: u32,
    /// It runs forward and then back (`autoRev="1"`): it takes twice `dur`, and ends where it began.
    pub auto_rev: bool,
    /// Its value is added to what the behaviours before it left (`additive="sum"`), not set.
    pub sum: bool,
    /// What it did goes when it ends (`fill` is not `hold`, and it does not run until the end of the effect): a ブラインド filter
    /// then leaves the whole shape, a 1 s `set` of フラッシュ (1 回) leaves it hidden again.
    pub remove: bool,
    /// A remapping of its time (`tmFilter="0,0; 0.14,0.31; …"`): points (time, progress) in millionths, the progress between two
    /// of them straight. Empty: none.
    pub filter: Vec<(i64, i64)>,
}

impl BehaviorTime {
    /// `progress` (0 to 1) of the behaviour's time as its `tmFilter` remaps it.
    pub fn filtered(&self, progress: f64) -> f64 {
        let f = &self.filter;
        if f.len() < 2 {
            return progress;
        }
        let p = |v: i64| v as f64 / 1_000_000.0;
        let x = progress.clamp(0.0, 1.0);
        for w in f.windows(2) {
            let (t0, t1) = (p(w[0].0), p(w[1].0));
            if x <= t1 || std::ptr::eq(&w[1], f.last().expect("a point")) {
                let k = if t1 > t0 { ((x - t0) / (t1 - t0)).clamp(0.0, 1.0) } else { 1.0 };
                return p(w[0].1) + (p(w[1].1) - p(w[0].1)) * k;
            }
        }
        progress
    }

    /// The points of a `tmFilter` attribute (`"0,0; .2,.5; 1,1"`).
    pub fn parse_filter(text: &str) -> Vec<(i64, i64)> {
        text.split(';')
            .filter_map(|pair| {
                let (a, b) = pair.split_once(',')?;
                Some(((a.trim().parse::<f64>().ok()? * 1_000_000.0).round() as i64, (b.trim().parse::<f64>().ok()? * 1_000_000.0).round() as i64))
            })
            .collect()
    }

    /// The `tmFilter` attribute's text for the points.
    pub fn filter_text(&self) -> String {
        self.filter.iter().map(|(a, b)| format!("{},{}", *a as f64 / 1_000_000.0, *b as f64 / 1_000_000.0)).collect::<Vec<_>>().join("; ")
    }

    /// The time the behaviour runs, given its `dur`.
    pub fn active(&self, dur: u32) -> u32 {
        if self.auto_rev { dur.saturating_mul(2) } else { dur }
    }
}

/// Smooth start and end (`accel`, `decel`, 1/1000 % of the time each; ECMA-376 19.5.3): the speed rises over the first `accel` and falls
/// over the last `decel` of the time, the share of the way `p` (0 to 1) being the distance covered. The speed rises as a half cosine
/// (not SMIL's straight ramp, whose distance is a quadratic): PowerPoint's videos of 16 effects with 9 settings of (accel, decel) at 1 s
/// and 2 s (Windows, 2026-11-18, `tasks/doc/requests/2026-11-18-89-accel-decel-result.md`) agree with it to 0.5 to 1 px
/// (SMIL's: 8 to 100 px). With `V = 1 / (1 - a/2 - d/2)` the speed in the middle, the distance is `V (p/2 - a sin(pi p / a) / (2 pi))` in
/// the first `a`, `V (p - a/2)` after it, and the same turned round in the last `d`. `accel + decel` over 1 leaves `decel = 1 - accel`.
pub fn ease(p: f64, accel: u32, decel: u32) -> f64 {
    let p = p.clamp(0.0, 1.0);
    if accel == 0 && decel == 0 {
        return p;
    }
    let a = (f64::from(accel) / 100_000.0).clamp(0.0, 1.0);
    let d = (f64::from(decel) / 100_000.0).clamp(0.0, 1.0 - a);
    let v = 1.0 / (1.0 - a / 2.0 - d / 2.0);
    let ramp = |x: f64, len: f64| v * (x / 2.0 - len * (std::f64::consts::PI * x / len).sin() / (2.0 * std::f64::consts::PI));
    if a > 0.0 && p < a {
        ramp(p, a)
    } else if d > 0.0 && p > 1.0 - d {
        1.0 - ramp(1.0 - p, d)
    } else {
        v * (p - a / 2.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BehaviorKind {
    /// `p:set`: an attribute set to a value at `begin` (`style.visibility`).
    Set { attr: String, value: String },
    /// `p:anim`: an attribute through the values of its `p:tav` (time in 1/1000 %, value an expression
    /// or a number), or through `from`/`to`/`by`, with an optional formula (`fmla`, `$` is the value).
    Anim { attr: String, keys: Vec<(u32, String)>, formula: Option<String>, discrete: bool },
    /// `p:animEffect`: a filter (`wipe(down)`, `fade`, …), in or out.
    Effect { filter: String, enter: bool },
    /// `p:animScale`: the size, from/to/by in 1/1000 %.
    Scale { from: Option<(i32, i32)>, to: Option<(i32, i32)>, by: Option<(i32, i32)> },
    /// `p:animRot`: the turn, from/to/by in 1/60000 degrees.
    Rotate { from: Option<i32>, to: Option<i32>, by: Option<i32> },
    /// A behaviour the player does not draw (`p:animClr`, `p:animMotion`…), kept as its XML so that it is written back as it
    /// was: its target element is `\u{1}` and each time node id is `\u{2}`, which writing fills in.
    Opaque(String),
}

/// The shape's place and size on the slide, as fractions of the slide (what `#ppt_x`, `#ppt_y`, `#ppt_w`, `#ppt_h` are):
/// the centre and the size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PptBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// What the behaviours say of the shape at a moment. `None`: not changed by them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PresetState {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub w: Option<f64>,
    pub h: Option<f64>,
    /// Degrees added to the shape's own turn.
    pub rotation: Option<f64>,
    pub opacity: Option<f64>,
    pub visible: Option<bool>,
    /// The filter running: what share (0 to 1) of the way it is, whether it brings the shape in.
    pub reveal: Option<Reveal>,
    /// Scale factors (1 is the shape's size) of `p:animScale`.
    pub scale: Option<(f64, f64)>,
    /// How far (in fractions of the slide) a motion path (`p:animMotion`) has taken the shape from where it is.
    pub motion: Option<(f64, f64)>,
    /// Horizontal shear (`xshear`, ライト スピード): a point moves sideways by this much for each unit it is below the shape's middle
    /// (measured on PowerPoint for Windows: -1 leans the top edge a full height to the right, +1 the other way).
    pub shear: Option<f64>,
    /// Colours of the fill, the outline and the text (`p:animClr`, a `p:set` of a colour), in the order the behaviours come.
    pub colors: Vec<crate::animation_color::ColorChange>,
    /// `fill.on`, `stroke.on`: the fill or the outline is drawn.
    pub fill_on: Option<bool>,
    pub line_on: Option<bool>,
    /// `style.fontSize`: the text's size as a multiple of its own.
    pub font_scale: Option<f64>,
    /// `style.fontWeight`, `style.fontStyle`, `style.textDecorationUnderline`, `style.fontFamily`.
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub font: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reveal {
    pub filter: String,
    pub enter: bool,
    pub progress: f64,
}

// ---- expressions (`#ppt_x+#ppt_w*0.5`, `1+#ppt_h/2`, `#ppt_y-sin(pi*$)*0.1`) ----

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
    vars: &'a dyn Fn(&str) -> Option<f64>,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn peek(&mut self) -> Option<u8> {
        self.ws();
        self.s.get(self.i).copied()
    }
    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        loop {
            match self.peek() {
                Some(b'+') => {
                    self.i += 1;
                    v += self.term()?;
                }
                Some(b'-') => {
                    self.i += 1;
                    v -= self.term()?;
                }
                _ => return Some(v),
            }
        }
    }
    fn term(&mut self) -> Option<f64> {
        let mut v = self.factor()?;
        loop {
            match self.peek() {
                Some(b'*') => {
                    self.i += 1;
                    v *= self.factor()?;
                }
                Some(b'/') => {
                    self.i += 1;
                    let d = self.factor()?;
                    v = if d == 0.0 { 0.0 } else { v / d };
                }
                _ => return Some(v),
            }
        }
    }
    fn factor(&mut self) -> Option<f64> {
        match self.peek()? {
            b'-' => {
                self.i += 1;
                Some(-self.factor()?)
            }
            b'+' => {
                self.i += 1;
                self.factor()
            }
            b'(' => {
                self.i += 1;
                let v = self.expr()?;
                (self.peek()? == b')').then(|| self.i += 1)?;
                Some(v)
            }
            b'#' | b'$' | b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = self.i;
                self.i += 1;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_alphanumeric() || self.s[self.i] == b'_' || self.s[self.i] == b'.') {
                    self.i += 1;
                }
                let name = std::str::from_utf8(&self.s[start..self.i]).ok()?;
                if self.peek() == Some(b'(') {
                    self.i += 1;
                    let arg = self.expr()?;
                    // Two arguments (`min(a, b)`, `max(a, b)`).
                    if self.peek() == Some(b',') {
                        self.i += 1;
                        let second = self.expr()?;
                        (self.peek()? == b')').then(|| self.i += 1)?;
                        return match name {
                            "min" => Some(arg.min(second)),
                            "max" => Some(arg.max(second)),
                            _ => None,
                        };
                    }
                    (self.peek()? == b')').then(|| self.i += 1)?;
                    return match name {
                        "sin" => Some(arg.sin()),
                        "cos" => Some(arg.cos()),
                        "tan" => Some(arg.tan()),
                        "abs" => Some(arg.abs()),
                        "sqrt" => Some(arg.max(0.0).sqrt()),
                        "atan" => Some(arg.atan()),
                        "exp" => Some(arg.exp()),
                        "ln" => Some(arg.max(f64::MIN_POSITIVE).ln()),
                        _ => None,
                    };
                }
                if name == "pi" {
                    return Some(std::f64::consts::PI);
                }
                (self.vars)(name)
            }
            b'0'..=b'9' | b'.' => {
                let start = self.i;
                while self.i < self.s.len() && (self.s[self.i].is_ascii_digit() || self.s[self.i] == b'.' || self.s[self.i] == b'e' && self.s.get(self.i + 1).is_some_and(|c| c.is_ascii_digit() || *c == b'-')) {
                    self.i += if self.s[self.i] == b'e' { 2 } else { 1 };
                }
                std::str::from_utf8(&self.s[start..self.i]).ok()?.parse().ok()
            }
            _ => None,
        }
    }
}

/// The value of expression `text` (`vars` gives `#ppt_x`… and `$`).
pub fn eval_expr(text: &str, vars: &dyn Fn(&str) -> Option<f64>) -> Option<f64> {
    let mut p = Parser { s: text.trim().as_bytes(), i: 0, vars };
    let v = p.expr()?;
    p.ws();
    (p.i == p.s.len()).then_some(v)
}

/// `#ppt_x` is the shape's own place; `ppt_x` (no `#`) is where it is now: what the behaviours before this one have left (PowerPoint's
/// exit 43 ends one behaviour at `ppt_y+0.31` and the next says `ppt_y` to stay there).
fn box_var(b: PptBox, cur: PptBox, extra: Option<f64>) -> impl Fn(&str) -> Option<f64> {
    move |name| match name {
        "#ppt_x" => Some(b.x),
        "#ppt_y" => Some(b.y),
        "#ppt_w" => Some(b.w),
        "#ppt_h" => Some(b.h),
        "ppt_x" => Some(cur.x),
        "ppt_y" => Some(cur.y),
        "ppt_w" => Some(cur.w),
        "ppt_h" => Some(cur.h),
        // The text's own size, as the 1 its multiples (`style.fontSize` goes to 1.5) start from.
        "#style.fontSize" => Some(1.0),
        "$" => extra,
        _ => None,
    }
}

/// Where behaviour `anim` is (value of its attribute) at `q` (0 to 1) of its time.
fn anim_value(keys: &[(u32, String)], formula: Option<&str>, discrete: bool, q: f64, b: PptBox, cur: PptBox) -> Option<f64> {
    let num = |s: &str, extra: Option<f64>| eval_expr(s, &box_var(b, cur, extra));
    let t = q.clamp(0.0, 1.0) * 100_000.0;
    let mut sorted: Vec<&(u32, String)> = keys.iter().collect();
    sorted.sort_by_key(|(tm, _)| *tm);
    let raw = match sorted.as_slice() {
        [] => return None,
        [only] => num(&only.1, None)?,
        list => {
            let after = list.iter().position(|(tm, _)| *tm as f64 > t);
            match after {
                None => num(&list[list.len() - 1].1, None)?,
                Some(0) => num(&list[0].1, None)?,
                Some(i) => {
                    let (t0, v0) = (list[i - 1].0 as f64, num(&list[i - 1].1, None)?);
                    if discrete {
                        v0
                    } else {
                        let (t1, v1) = (list[i].0 as f64, num(&list[i].1, None)?);
                        let k = if t1 > t0 { (t - t0) / (t1 - t0) } else { 1.0 };
                        v0 + (v1 - v0) * k
                    }
                }
            }
        }
    };
    match formula {
        Some(f) => num(f, Some(raw)),
        None => Some(raw),
    }
}

/// The points a motion path (`M x y L x y C … Q … Z E`, in fractions of the slide; lower case commands are relative to where the
/// path is) goes through, curves cut into short lines.
fn path_polyline(path: &str) -> Vec<(f64, f64)> {
    let tokens: Vec<&str> = path.split(|c: char| c.is_whitespace() || c == ',').filter(|t| !t.is_empty()).collect();
    let num = |i: usize| -> Option<f64> { tokens.get(i)?.parse().ok() };
    let mut points: Vec<(f64, f64)> = Vec::new();
    let (mut start, mut here) = ((0.0, 0.0), (0.0, 0.0));
    let (mut command, mut relative) = ('E', false);
    let mut i = 0;
    while i < tokens.len() {
        let t = tokens[i];
        if t.len() == 1 && t.chars().all(|c| c.is_ascii_alphabetic()) {
            relative = t.chars().all(|c| c.is_ascii_lowercase());
            command = t.chars().next().unwrap().to_ascii_uppercase();
            i += 1;
            if command == 'Z' {
                here = start;
                points.push(here);
            }
            continue;
        }
        let o = if relative { here } else { (0.0, 0.0) };
        match command {
            'M' | 'L' => {
                let (Some(x), Some(y)) = (num(i), num(i + 1)) else { break };
                let p = (o.0 + x, o.1 + y);
                i += 2;
                if command == 'M' && points.is_empty() {
                    start = p;
                }
                points.push(p);
                here = p;
                command = 'L';
            }
            'C' => {
                let v: Vec<Option<f64>> = (0..6).map(|k| num(i + k)).collect();
                if v.iter().any(Option::is_none) {
                    break;
                }
                let v: Vec<f64> = v.into_iter().flatten().collect();
                let (c1, c2, p) = ((o.0 + v[0], o.1 + v[1]), (o.0 + v[2], o.1 + v[3]), (o.0 + v[4], o.1 + v[5]));
                i += 6;
                for k in 1..=16 {
                    let t = f64::from(k) / 16.0;
                    let m = 1.0 - t;
                    let f = |a: f64, b: f64, c: f64, d: f64| m * m * m * a + 3.0 * m * m * t * b + 3.0 * m * t * t * c + t * t * t * d;
                    points.push((f(here.0, c1.0, c2.0, p.0), f(here.1, c1.1, c2.1, p.1)));
                }
                here = p;
            }
            'Q' => {
                let v: Vec<Option<f64>> = (0..4).map(|k| num(i + k)).collect();
                if v.iter().any(Option::is_none) {
                    break;
                }
                let v: Vec<f64> = v.into_iter().flatten().collect();
                let (c, p) = ((o.0 + v[0], o.1 + v[1]), (o.0 + v[2], o.1 + v[3]));
                i += 4;
                for k in 1..=16 {
                    let t = f64::from(k) / 16.0;
                    let m = 1.0 - t;
                    points.push((m * m * here.0 + 2.0 * m * t * c.0 + t * t * p.0, m * m * here.1 + 2.0 * m * t * c.1 + t * t * p.1));
                }
                here = p;
            }
            _ => break,
        }
    }
    points
}

/// Where `q` (0 to 1) of the way along motion `path` is (by the length, the same speed along every line), in fractions of the slide
/// from where the shape is: the path's own coordinates (`p:animMotion` with `origin="layout"`, `pathEditMode="relative"`).
pub fn motion_point(path: &str, q: f64) -> Option<(f64, f64)> {
    let points = path_polyline(path);
    let first = *points.first()?;
    let length = |a: (f64, f64), b: (f64, f64)| (b.0 - a.0).hypot(b.1 - a.1);
    let total: f64 = points.windows(2).map(|w| length(w[0], w[1])).sum();
    if total <= 0.0 {
        return Some(first);
    }
    let mut left = q.clamp(0.0, 1.0) * total;
    for w in points.windows(2) {
        let l = length(w[0], w[1]);
        if left <= l && l > 0.0 {
            let t = left / l;
            return Some((w[0].0 + (w[1].0 - w[0].0) * t, w[0].1 + (w[1].1 - w[0].1) * t));
        }
        left -= l;
    }
    points.last().copied()
}

/// The `path` of an opaque `p:animMotion`.
fn opaque_motion_path(raw: &str) -> Option<&str> {
    let tag = raw.strip_prefix("<p:animMotion")?;
    let at = tag.find(" path=\"")? + " path=\"".len();
    let end = tag[at..].find('"')?;
    Some(&tag[at..at + end])
}

/// The state the `behaviours` of an effect leave the shape in `ms` after the effect begins; `b` is the shape's own box.
pub fn sample(behaviours: &[AnimBehavior], ms: f64, b: PptBox) -> PresetState {
    sample_eased(behaviours, ms, b, (0, 0))
}

/// [`sample`] for an effect whose own time node says `parent` = (`accel`, `decel`): each behaviour's smooth start and end is the effect's where
/// that is not 0, else its own, applied to its own stretch of time (PowerPoint's videos: the effect's value does not stretch over the
/// whole effect, a behaviour that begins late or ends early runs its own curve; a behaviour's own `decel` stays when the effect's is 0).
pub fn sample_eased(behaviours: &[AnimBehavior], ms: f64, b: PptBox, parent: (u32, u32)) -> PresetState {
    // The behaviours act in the order they begin (the file lists them as it likes): ジッパー（終了） lists the shrink that begins at
    // 1230 ms before the growth that ends there, and the growth must not undo the shrink (PowerPoint's video, 24 frames of 2 s).
    if behaviours.windows(2).any(|w| w[0].begin > w[1].begin) {
        let mut sorted = behaviours.to_vec();
        sorted.sort_by_key(|b| b.begin);
        return sample_eased(&sorted, ms, b, parent);
    }
    let mut s = PresetState::default();
    for (idx, bh) in behaviours.iter().enumerate() {
        if ms < f64::from(bh.begin) {
            continue;
        }
        // The behaviour's own time: forward (and back, `autoRev`), smooth start and end applied.
        if bh.time.remove && ms >= f64::from(bh.begin) + f64::from(bh.time.active(bh.dur)) {
            continue;
        }
        let q = if bh.dur == 0 {
            1.0
        } else {
            let x = (ms - f64::from(bh.begin)) / f64::from(bh.dur);
            let x = if bh.time.auto_rev {
                if x >= 2.0 { 0.0 } else if x > 1.0 { 2.0 - x } else { x }
            } else {
                x.min(1.0)
            };
            bh.time.filtered(ease(x, if parent.0 != 0 { parent.0 } else { bh.time.accel }, if parent.1 != 0 { parent.1 } else { bh.time.decel }))
        };
        // Where the shape is (`ppt_x` without the `#`) when this behaviour begins: what the behaviours before this one have left by
        // then. A behaviour that begins with the effect sees the shape's own box, however far the behaviours running beside it have
        // got (フリップ（終了）: the width goes to -2 times itself while the shift is half of the width it began with: its
        // centre moves by 0.5 of the width, not by the changing one; PowerPoint's video, frames 0 to 24 of 1 s).
        let cur = if bh.begin == 0 {
            b
        } else {
            let before = sample_eased(&behaviours[..idx], f64::from(bh.begin), b, parent);
            PptBox { x: before.x.unwrap_or(b.x), y: before.y.unwrap_or(b.y), w: before.w.unwrap_or(b.w), h: before.h.unwrap_or(b.h) }
        };
        match &bh.kind {
            BehaviorKind::Set { attr, value } => {
                if attr == "style.visibility" {
                    s.visible = Some(value == "visible");
                } else if attr == "style.opacity" {
                    s.opacity = value.parse().ok();
                } else if attr == "style.rotation" {
                    s.rotation = value.parse().ok();
                } else if matches!(attr.as_str(), "ppt_x" | "ppt_y" | "ppt_w" | "ppt_h") {
                    // A place or a size as an expression (`(#ppt_y+0.4)`): アーチ アップ puts the shape somewhere for the whole of its run.
                    let v = eval_expr(value, &box_var(b, cur, None));
                    match attr.as_str() {
                        "ppt_x" => s.x = v,
                        "ppt_y" => s.y = v,
                        "ppt_w" => s.w = v,
                        _ => s.h = v,
                    }
                } else if attr == "xshear" {
                    s.shear = value.parse().ok();
                } else if attr == "fill.on" {
                    s.fill_on = Some(value == "true");
                } else if attr == "stroke.on" {
                    s.line_on = Some(value == "true");
                } else if attr == "style.fontWeight" {
                    s.bold = Some(value == "bold");
                } else if attr == "style.fontStyle" {
                    s.italic = Some(value == "italic");
                } else if attr == "style.textDecorationUnderline" {
                    s.underline = Some(value == "true");
                } else if attr == "style.fontFamily" {
                    s.font = Some(value.clone());
                }
            }
            // Text values that step (`discrete`): 太字（フラッシュ） goes normal, bold, normal.
            BehaviorKind::Anim { attr, keys, .. } if matches!(attr.as_str(), "style.fontWeight" | "style.fontStyle" | "style.textDecorationUnderline") => {
                let t = q * 100_000.0;
                if let Some((_, v)) = keys.iter().filter(|(tm, _)| f64::from(*tm) <= t).max_by_key(|(tm, _)| *tm) {
                    match attr.as_str() {
                        "style.fontWeight" => s.bold = Some(v == "bold"),
                        "style.fontStyle" => s.italic = Some(v == "italic"),
                        _ => s.underline = Some(v == "true"),
                    }
                }
            }
            BehaviorKind::Anim { attr, keys, .. } if attr == "style.visibility" => {
                // The text values step: the last key whose time has come (フラッシュ: hidden, then visible at half the time).
                let t = q * 100_000.0;
                if let Some((_, v)) = keys.iter().filter(|(tm, _)| f64::from(*tm) <= t).max_by_key(|(tm, _)| *tm) {
                    s.visible = Some(v == "visible");
                }
            }
            BehaviorKind::Anim { attr, keys, formula, discrete } => {
                let Some(v) = anim_value(keys, formula.as_deref(), *discrete, q, b, cur) else { continue };
                // `additive="sum"`: added to what the behaviours before it left (the shape's own where none did).
                let add = |now: Option<f64>, own: f64| if bh.time.sum { now.unwrap_or(own) + v } else { v };
                match attr.as_str() {
                    "ppt_x" => s.x = Some(add(s.x, b.x)),
                    "ppt_y" => s.y = Some(add(s.y, b.y)),
                    "ppt_w" => s.w = Some(add(s.w, b.w)),
                    "ppt_h" => s.h = Some(add(s.h, b.h)),
                    "style.rotation" | "r" => s.rotation = Some(add(s.rotation, 0.0)),
                    "style.opacity" => s.opacity = Some(add(s.opacity, 1.0)),
                    "xshear" => s.shear = Some(add(s.shear, 0.0)),
                    "style.fontSize" => s.font_scale = Some(add(s.font_scale, 1.0)),
                    "style.visibility" => s.visible = Some(v >= 0.5),
                    _ => {}
                }
            }
            BehaviorKind::Effect { filter, enter } => {
                s.reveal = Some(Reveal { filter: filter.clone(), enter: *enter, progress: q });
            }
            BehaviorKind::Scale { from, to, by } => {
                let f = |v: (i32, i32)| (f64::from(v.0) / 100_000.0, f64::from(v.1) / 100_000.0);
                // Without a `from` it starts from the size the behaviours before it left.
                let now = s.scale.unwrap_or((1.0, 1.0));
                let (a, z) = match (from, to, by) {
                    (Some(a), Some(z), _) => (f(*a), f(*z)),
                    (None, None, Some(by)) => (now, (now.0 * f(*by).0, now.1 * f(*by).1)),
                    (Some(a), None, Some(by)) => (f(*a), (f(*a).0 * f(*by).0, f(*a).1 * f(*by).1)),
                    (None, Some(z), _) => (now, f(*z)),
                    (Some(a), None, None) => (f(*a), (1.0, 1.0)),
                    (None, None, None) => continue,
                };
                s.scale = Some((a.0 + (z.0 - a.0) * q, a.1 + (z.1 - a.1) * q));
            }
            BehaviorKind::Opaque(raw) => {
                if let Some(spec) = crate::animation_color::color_spec(raw) {
                    let mut change = spec.change;
                    change.q = if spec.is_set { 1.0 } else { q };
                    s.colors.push(change);
                } else if let Some(path) = opaque_motion_path(raw) {
                    if let Some((x, y)) = motion_point(path, q) {
                        s.motion = Some((x, y));
                    }
                }
            }
            BehaviorKind::Rotate { from, to, by } => {
                let d = |v: i32| f64::from(v) / 60_000.0;
                // `by` alone turns the shape this much more than the behaviours before it (the wave's four short turns add up).
                if let (None, None, Some(by)) = (from, to, by) {
                    s.rotation = Some(s.rotation.unwrap_or(0.0) + d(*by) * q);
                    continue;
                }
                let (a, z) = match (from, to, by) {
                    (Some(a), Some(z), _) => (d(*a), d(*z)),
                    (None, None, Some(by)) => (0.0, d(*by)),
                    (Some(a), None, Some(by)) => (d(*a), d(*a) + d(*by)),
                    (None, Some(z), _) => (0.0, d(*z)),
                    _ => continue,
                };
                s.rotation = Some(a + (z - a) * q);
            }
        }
    }
    s
}

// ---- 見える範囲（`p:animEffect` の filter） ----

/// A rectangle in fractions of the shape's frame: left, top, right, bottom.
pub type Rect = [f64; 4];

/// A filter split into its name and argument: `wipe(down)` is (`wipe`, `down`).
pub fn split_filter(filter: &str) -> (&str, &str) {
    match filter.split_once('(') {
        Some((name, rest)) => (name.trim(), rest.trim_end_matches(')').trim()),
        None => (filter.trim(), ""),
    }
}

/// One cell of a filter's pattern: the region it may show (`outer`), the part of it shown while the filter brings the shape in
/// (`inner`), or, `ring`, all of `outer` but `inner`.
struct Cell {
    outer: Rect,
    inner: Rect,
    ring: bool,
}

fn rect_minus(outer: Rect, inner: Rect, out: &mut Vec<Rect>) {
    let [l, t, r, b] = outer;
    let [il, it, ir, ib] = [inner[0].clamp(l, r), inner[1].clamp(t, b), inner[2].clamp(l, r), inner[3].clamp(t, b)];
    if ir <= il || ib <= it {
        out.push(outer);
        return;
    }
    for rect in [[l, t, r, it], [l, ib, r, b], [l, it, il, ib], [ir, it, r, ib]] {
        if rect[2] > rect[0] && rect[3] > rect[1] {
            out.push(rect);
        }
    }
}

/// The part of `outer` an edge-anchored strip of `q` covers: from the left (`'l'`), right, top or bottom.
fn strip(outer: Rect, from: char, q: f64) -> Rect {
    let [l, t, r, b] = outer;
    match from {
        'l' => [l, t, l + (r - l) * q, b],
        'r' => [r - (r - l) * q, t, r, b],
        't' => [l, t, r, t + (b - t) * q],
        _ => [l, b - (b - t) * q, r, b],
    }
}

/// A deterministic share (0 to 1) for bar `i` (random bars begin at their own times).
pub fn scatter(i: usize) -> f64 {
    let mut x = (i as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 32;
    (x % 1000) as f64 / 1000.0
}

/// A progress from 0 to 1 over `t` (0 to 1) that speeds up over the first `a` of the time, goes steadily, and slows down over the last `a`.
fn trapezoid(t: f64, a: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let v = 1.0 / (1.0 - a);
    if t < a {
        v * t * t / (2.0 * a)
    } else if t > 1.0 - a {
        1.0 - v * (1.0 - t) * (1.0 - t) / (2.0 * a)
    } else {
        v * (t - a / 2.0)
    }
}

/// How many slices a round or diagonal filter is drawn in (rectangles are all a window clips to).
const SLICES: usize = 10;
const SLATS: usize = 6;
const BARS: usize = 12;
/// The bands and columns a sector is drawn in.
const SECTOR_ROWS: usize = 180;
const SECTOR_COLS: usize = 320;
/// The squares ディゾルブ is drawn in.
/// The squares ディゾルブ is drawn in (PowerPoint's picture is finer: see `dissolve_value`).
const DISSOLVE_ROWS: usize = 64;
const DISSOLVE_COLS: usize = 64;
/// The lattice of random values PowerPoint's ディゾルブ is made of, along each side of the shape.
const DISSOLVE_LATTICE: usize = 256;

/// The random picture of ディゾルブ at (`u`, `v`) (0 to 1 across the shape's box): a lattice of 256 x 256 fixed random values, linearly interpolated, stretched to
/// the shape's bounding box. PowerPoint for Windows (1080p and 720p, 60 and 30 frames a second, shapes of 20, 50 and 100 % of the slide's width, 16:9 and 4:3):
/// the autocorrelation's half width of the mask is 0.0021 of the shape's width and 0.0022 of its height, in every size, 256 lattice points give the correlations
/// measured along both axes, the same picture stretched to each box matches 96 to 97 % of the pixels, and it is the same in every run.
fn dissolve_value(u: f64, v: f64) -> f64 {
    let n = DISSOLVE_LATTICE;
    let (x, y) = (u.clamp(0.0, 1.0) * (n - 1) as f64, v.clamp(0.0, 1.0) * (n - 1) as f64);
    let (i, j) = ((x.floor() as usize).min(n - 2), (y.floor() as usize).min(n - 2));
    let (fx, fy) = (x - i as f64, y - j as f64);
    let at = |a: usize, b: usize| scatter(b * n + a);
    let top = at(i, j) * (1.0 - fx) + at(i + 1, j) * fx;
    let bottom = at(i, j + 1) * (1.0 - fx) + at(i + 1, j + 1) * fx;
    top * (1.0 - fy) + bottom * fy
}

/// The pattern of filter `name(arg)` at `q` of its way in, as cells; `None` for a filter that is not drawn as rectangles.
/// `aspect` is the frame's width over its height.
fn cells(name: &str, arg: &str, q: f64, aspect: f64) -> Option<Vec<Cell>> {
    let whole: Rect = [0.0, 0.0, 1.0, 1.0];
    let centred = |kx: f64, ky: f64| -> Rect { [0.5 - kx / 2.0, 0.5 - ky / 2.0, 0.5 + kx / 2.0, 0.5 + ky / 2.0] };
    match name {
        "wipe" | "peek" => {
            let from = match arg {
                "left" => 'l',
                "right" => 'r',
                "up" => 't',
                _ => 'b',
            };
            Some(vec![Cell { outer: whole, inner: strip(whole, from, q), ring: false }])
        }
        "blinds" => {
            let horizontal = arg != "vertical";
            Some(
                (0..SLATS)
                    .map(|i| {
                        let (a, z) = (i as f64 / SLATS as f64, (i + 1) as f64 / SLATS as f64);
                        let outer: Rect = if horizontal { [0.0, a, 1.0, z] } else { [a, 0.0, z, 1.0] };
                        Cell { outer, inner: strip(outer, if horizontal { 't' } else { 'l' }, q), ring: false }
                    })
                    .collect(),
            )
        }
        "randombar" => {
            let horizontal = arg != "vertical";
            Some(
                (0..BARS)
                    .map(|i| {
                        let (a, z) = (i as f64 / BARS as f64, (i + 1) as f64 / BARS as f64);
                        let outer: Rect = if horizontal { [0.0, a, 1.0, z] } else { [a, 0.0, z, 1.0] };
                        // Each bar starts at its own time within the first half and takes the second half's length.
                        let start = scatter(i) * 0.5;
                        let local = ((q - start) / 0.5).clamp(0.0, 1.0);
                        Cell { outer, inner: strip(outer, if horizontal { 'l' } else { 't' }, local), ring: false }
                    })
                    .collect(),
            )
        }
        "comb" => {
            // Slats that slide in from the two sides in turn.
            let horizontal = arg != "vertical";
            const COMB: usize = 8;
            Some(
                (0..COMB)
                    .map(|i| {
                        let (a, z) = (i as f64 / COMB as f64, (i + 1) as f64 / COMB as f64);
                        let outer: Rect = if horizontal { [0.0, a, 1.0, z] } else { [a, 0.0, z, 1.0] };
                        let from = match (horizontal, i % 2) {
                            (true, 0) => 'l',
                            (true, _) => 'r',
                            (false, 0) => 't',
                            (false, _) => 'b',
                        };
                        Cell { outer, inner: strip(outer, from, q), ring: false }
                    })
                    .collect(),
            )
        }
        "checkerboard" => {
            // 6 x 6 cells of the shape's own size, every second row (across) or column (down) shifted by half a cell, all
            // cells moving together, each anchored at the left (across) or the top (down) (PowerPoint's video of the exit, a shape
            // of 480 x 180 pt: the holes are 6 per row 107 px apart in 1280 px frames, the rows 40 px).
            const N: usize = 6;
            let across = arg != "down";
            let size = 1.0 / N as f64;
            let clip = |r: Rect| -> Rect { [r[0].clamp(0.0, 1.0), r[1].clamp(0.0, 1.0), r[2].clamp(0.0, 1.0), r[3].clamp(0.0, 1.0)] };
            let mut out = Vec::new();
            for line in 0..N {
                let shift = if line % 2 == 1 { 0.5 } else { 0.0 };
                for k in 0..=N {
                    let a = (k as f64 - shift) * size;
                    if a >= 1.0 - 1e-9 || a + size <= 1e-9 {
                        continue;
                    }
                    let outer: Rect = if across { [a, line as f64 * size, a + size, (line + 1) as f64 * size] } else { [line as f64 * size, a, (line + 1) as f64 * size, a + size] };
                    out.push(Cell { outer: clip(outer), inner: clip(strip(outer, if across { 'l' } else { 't' }, q)), ring: false });
                }
            }
            Some(out)
        }
        "box" => {
            let (entering_out, k) = (arg != "in", q);
            Some(if entering_out {
                vec![Cell { outer: whole, inner: centred(k, k), ring: false }]
            } else {
                vec![Cell { outer: whole, inner: centred(1.0 - k, 1.0 - k), ring: true }]
            })
        }
        "barn" | "split" => {
            let vertical = arg.contains("ertical");
            let out = arg.starts_with("out");
            let (kx, ky) = if vertical { (q, 1.0) } else { (1.0, q) };
            Some(if out {
                vec![Cell { outer: whole, inner: centred(kx, ky), ring: false }]
            } else {
                vec![Cell { outer: whole, inner: centred(if vertical { 1.0 - q } else { 1.0 }, if vertical { 1.0 } else { 1.0 - q }), ring: true }]
            })
        }
        "circle" | "diamond" => {
            // Slices of the disk (or the diamond): the half width of each by its distance from the centre.
            let out = arg != "in";
            // Distances in widths of the frame: a row's height counts 1/aspect.
            let half_diagonal = (0.5f64.powi(2) + (0.5 / aspect.max(f64::MIN_POSITIVE)).powi(2)).sqrt();
            let r = if out { q } else { 1.0 - q } * half_diagonal;
            Some(
                (0..SLICES)
                    .map(|i| {
                        let (a, z) = (i as f64 / SLICES as f64, (i + 1) as f64 / SLICES as f64);
                        let dy_w = ((a + z) / 2.0 - 0.5).abs() / aspect.max(f64::MIN_POSITIVE);
                        let half = if name == "circle" { (r * r - dy_w * dy_w).max(0.0).sqrt() } else { (r - dy_w).max(0.0) };
                        let outer: Rect = [0.0, a, 1.0, z];
                        let half = half.min(0.5);
                        Cell { outer, inner: [0.5 - half, a, 0.5 + half, z], ring: !out }
                    })
                    .collect(),
            )
        }
        "strips" => {
            // A diagonal wipe from a corner: each slice is shown up to the diagonal.
            let from_left = arg.ends_with("Left");
            let from_top = arg.starts_with("up");
            let reach = 2.0 * q;
            Some(
                (0..SLICES)
                    .map(|i| {
                        let (a, z) = (i as f64 / SLICES as f64, (i + 1) as f64 / SLICES as f64);
                        let yc = (a + z) / 2.0;
                        let v = if from_top { yc } else { 1.0 - yc };
                        let width = (reach - v).clamp(0.0, 1.0);
                        let outer: Rect = [0.0, a, 1.0, z];
                        Cell { outer, inner: strip(outer, if from_left { 'l' } else { 'r' }, width), ring: false }
                    })
                    .collect(),
            )
        }
        "plus" => None,
        _ => None,
    }
}

/// The part of the frame (fractions of it) inside a sector shape, as rectangles: `rows` bands, in each the runs of `cols` cells
/// whose centre `inside` says is in. `inside` is given the angle of the cell's centre from the frame's centre, in degrees,
/// clockwise from straight up (0 to 360), taken in the frame's own units (half its width and half its height are both 1, so a
/// wide frame's sector spreads sideways by its width: the angle of PowerPoint's transition wheel is measured so, see
/// `transition_plan::soft_phi`; `aspect` is no longer used by it). A sector is a staircase of rectangles.
pub fn sector_rects(rows: usize, cols: usize, _aspect: f64, inside: &dyn Fn(f64) -> bool) -> Vec<Rect> {
    let mut out = Vec::new();
    for r in 0..rows {
        let y = (r as f64 + 0.5) / rows as f64;
        let mut run: Option<usize> = None;
        for c in 0..=cols {
            let hit = c < cols && {
                let x = (c as f64 + 0.5) / cols as f64;
                let angle = (2.0 * x - 1.0).atan2(1.0 - 2.0 * y).to_degrees();
                inside(angle.rem_euclid(360.0))
            };
            match (hit, run) {
                (true, None) => run = Some(c),
                (false, Some(start)) => {
                    out.push([start as f64 / cols as f64, r as f64 / rows as f64, c as f64 / cols as f64, (r + 1) as f64 / rows as f64]);
                    run = None;
                }
                _ => {}
            }
        }
    }
    out
}

/// ホイール (`wheel(n)`, n spokes) and くさび (`wedge`): the part of the frame the filter has swept after `q` of its time.
/// ホイール: each of the n sectors of the circle (the first from straight up) is swept clockwise from its start, all at once, at a
/// constant angular speed (measured on PowerPoint for Windows, 1 spoke, 2 seconds: the front goes 6 degrees a frame, 360 in
/// 60 frames). くさび: two sectors open from straight up, one each way, to meet straight down.
fn swept_sectors(name: &str, arg: &str, q: f64, aspect: f64, rows: usize, cols: usize) -> Vec<Rect> {
    if name == "wedge" {
        let half = 180.0 * q;
        return sector_rects(rows, cols, aspect, &|a: f64| a <= half || 360.0 - a <= half);
    }
    let spokes = arg.parse::<usize>().ok().filter(|n| (1..=16).contains(n)).unwrap_or(4);
    let width = 360.0 / spokes as f64;
    let swept = width * q;
    sector_rects(rows, cols, aspect, &|a: f64| a.rem_euclid(width) <= swept)
}

/// The soft edge of ホイール (the width of the band as a share of a spoke's sector) and the ends of the sweep, measured on PowerPoint
/// for Windows (Microsoft 365, `CreateVideo` 1280 x 720, 30 frames a second, 2 seconds, a red box of 12 x 3, 5 x 5 and 3 x 7 inches
/// on white, spokes 1, 2, 3, 4 and 8, 15 slides for 開始 and for 終了; the new look's share of each pixel against the angle clockwise
/// from straight up in the box's own units, modulo the sector, `1 - B((phi - c) / w + 1/2)` with B the half cosine step, per frame):
/// `w` is 0.195 to 0.205 of the sector (median 0.197) for every box and spoke count, `c` runs at a constant speed over the effect's
/// time (frame by frame it goes 0.0187 of a sector, the same in the first and the last frames) from -0.065..-0.08 to 1.063..1.082
/// (taken as -0.07 and 1.07); one set of these for all 15 slides leaves an rms error of 0.011 to 0.043 of the share per frame
/// (mean 0.023; the higher with 8 spokes, whose hard edges are thin). The same for 終了, whose box is what has not been swept.
const WHEEL_BAND: f64 = 0.197;
/// The soft edge of the animations' circle, diamond, plus and wedge as a share of the distance the shape covers (`big` of `transition_plan::soft_phi`; a slide
/// transition's is a third of it), the same as ホイール's share of a sector. Measured on PowerPoint for Windows (`CreateVideo`, 1280 x 720, 30 frames a second,
/// 2 seconds, a shape of 640 x 240 px, the new look's share of each pixel against `phi`, `1 - B((phi - c) / w + 1/2)`, per frame): w / big 0.188 to 0.197 (median
/// 0.195) in 円 内・外, ひし形 内・外, プラス 内・外, くさび, ボックス 内・外 and スプリット (外, both ways: 0.097 of the whole extent, `big` being its half; rms error of the
/// share 0.007 to 0.026), `c` at a constant speed from -w/2 to big + w/2, as the transitions.
const SOFT_BAND: f64 = WHEEL_BAND;
const WHEEL_C0: f64 = -0.07;
const WHEEL_C1: f64 = 1.07;
const WHEEL_ROWS: usize = 180;
const WHEEL_COLS: usize = 320;
const WHEEL_LEVELS: usize = 32;

/// The share (0 to 1) of the new look at `phi` (0 to 1 of the sector, from where the sweep started) of ホイール at `q` of its time.
pub fn wheel_share(phi: f64, q: f64) -> f64 {
    let c = WHEEL_C0 + (WHEEL_C1 - WHEEL_C0) * q.clamp(0.0, 1.0);
    let u = ((phi - c) / WHEEL_BAND + 0.5).clamp(0.0, 1.0);
    1.0 - (1.0 - (std::f64::consts::PI * u).cos()) / 2.0
}

/// ストリップ of an animation at `q` (0 to 1): the shape's 16 x 15 blocks with their opacity, sweeping from the corner `(x0, y0)`.
/// Going out (`!enter`) the blocks the sweep has not reached stay.
fn strips_tiles(x0: f64, y0: f64, q: f64, enter: bool) -> Vec<(Rect, f64)> {
    const COLS: usize = 16;
    const ROWS: usize = 15;
    let mut out = Vec::new();
    for j in 0..ROWS {
        for i in 0..COLS {
            let rect: Rect = [i as f64 / COLS as f64, j as f64 / ROWS as f64, (i + 1) as f64 / COLS as f64, (j + 1) as f64 / ROWS as f64];
            let s = (((rect[0] + rect[2]) / 2.0 - x0).abs() + ((rect[1] + rect[3]) / 2.0 - y0).abs()) / 2.0;
            let u = ((q - 0.87 * s) / 0.13).clamp(0.0, 1.0);
            let alpha = (1.0 - (std::f64::consts::PI * u).cos()) / 2.0;
            let alpha = if enter { alpha } else { 1.0 - alpha };
            if alpha > 0.004 {
                out.push((rect, alpha));
            }
        }
    }
    out
}

/// Like `reveal_rects`, with an opacity for each window: ホイール has a soft front (`wheel_share`), the others are whole (1).
pub fn reveal_windows(filter: &str, enter: bool, progress: f64, aspect: f64) -> Option<Vec<(Rect, f64)>> {
    reveal_windows_sized(filter, enter, progress, aspect, None)
}

/// The share of a point (0.75 pt, a pixel of the 96 dpi PowerPoint's 1280 x 720 video) a bar of ランダム ストライプ is thick.
const RANDOM_BAR_PT: f64 = 0.75;

/// ランダム ストライプ at `progress`: the shape cut into bars a pixel thick (`extent_pt` long across), each coming in (or going) at a time of its own, all of
/// them uniformly between a tenth and nine tenths of the effect's time. Measured on PowerPoint for Windows (`CreateVideo`, 1280 x 720, 30 frames a
/// second, a shape of 640 x 240 px, 15 frames of effect): per row (水平) and per column (垂直) every bar is a pixel (425 of 645 runs of a single column, 207 of
/// 245 rows), switches once and without a fade (0.1 % of the rows' pixels in between), at frames spread evenly over 12 of the 15 (counts 5 to 74 over 12 frames).
fn random_bars(horizontal: bool, enter: bool, q: f64, extent_pt: f64) -> Vec<Rect> {
    let n = ((extent_pt / RANDOM_BAR_PT).ceil() as usize).clamp(1, 4096);
    let shown = |i: usize| (q >= 0.1 + 0.8 * scatter(i)) == enter;
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for i in 0..=n {
        match (i < n && shown(i), start) {
            (true, None) => start = Some(i),
            (false, Some(a)) => {
                let (a, z) = (a as f64 / n as f64, i as f64 / n as f64);
                out.push(if horizontal { [0.0, a, 1.0, z] } else { [a, 0.0, z, 1.0] });
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// `reveal_windows` for a shape of `size_pt` (width, height in points), when it is known: ランダム ストライプ takes its bars from the size.
pub fn reveal_windows_sized(filter: &str, enter: bool, progress: f64, aspect: f64, size_pt: Option<(f64, f64)>) -> Option<Vec<(Rect, f64)>> {
    let (name, arg) = split_filter(filter);
    if name == "randombar" && let Some((w, h)) = size_pt {
        let horizontal = arg != "vertical";
        let q = progress.clamp(0.0, 1.0);
        return Some(random_bars(horizontal, enter, q, if horizontal { h } else { w }).into_iter().map(|r| (r, 1.0)).collect());
    }
    if name == "wipe" {
        // ワイプ (and the wipe of のぞき込み): a soft edge, a linear ramp of opacity a seventh of the shape's extent along the wipe, whose front runs
        // from a ramp's width before the start edge to the far edge at a constant speed (PowerPoint for Windows, the video of the 291 effects,
        // 0.5 s at 30 frames a second, a shape of 645 x 245 px, the four directions: the 10 % to 90 % part of the ramp is 0.113 of the width
        // (0.110 to 0.114 of the height), that is a ramp of 0.142 = 1/7; the opacity falls by 0.087 per 8 px of 92, a straight line; the mean
        // opacity rises by 0.0755 a frame, 15 frames, 1 + 1/7 of the extent in all). Going out, what is left is the opposite opacity.
        const WIPE_BAND: f64 = 1.0 / 7.0;
        const STRIPS: usize = 96;
        let q = progress.clamp(0.0, 1.0);
        let from = match arg {
            "left" => 'l',
            "right" => 'r',
            "up" => 't',
            _ => 'b',
        };
        let front = q * (1.0 + WIPE_BAND);
        let mut out = Vec::new();
        for i in 0..STRIPS {
            let (a, z) = (i as f64 / STRIPS as f64, (i + 1) as f64 / STRIPS as f64);
            let mid = (a + z) / 2.0;
            let (rect, distance): (Rect, f64) = match from {
                'l' => ([a, 0.0, z, 1.0], mid),
                'r' => ([a, 0.0, z, 1.0], 1.0 - mid),
                't' => ([0.0, a, 1.0, z], mid),
                _ => ([0.0, a, 1.0, z], 1.0 - mid),
            };
            let shown = ((front - distance) / WIPE_BAND).clamp(0.0, 1.0);
            let alpha = if enter { shown } else { 1.0 - shown };
            if alpha > 0.004 {
                out.push((rect, alpha));
            }
        }
        return Some(out);
    }
    if name == "wheel" {
        let spokes = arg.parse::<usize>().ok().filter(|n| (1..=16).contains(n)).unwrap_or(4);
        let sector = 360.0 / spokes as f64;
        let q = progress.clamp(0.0, 1.0);
        let share: Vec<Vec<f64>> = (0..WHEEL_ROWS)
            .map(|r| {
                let y = (r as f64 + 0.5) / WHEEL_ROWS as f64;
                (0..WHEEL_COLS)
                    .map(|c| {
                        let x = (c as f64 + 0.5) / WHEEL_COLS as f64;
                        let angle = (2.0 * x - 1.0).atan2(1.0 - 2.0 * y).to_degrees().rem_euclid(360.0);
                        let s = wheel_share(angle.rem_euclid(sector) / sector, q);
                        if enter { s } else { 1.0 - s }
                    })
                    .collect()
            })
            .collect();
        return Some(crate::transition_plan::share_windows_of_levels(&share, WHEEL_LEVELS));
    }
    if matches!(name, "circle" | "diamond" | "plus" | "wedge" | "box" | "barn" | "split") {
        // 円・ひし形・プラス・くさび: the slide transitions' soft shapes (`transition_plan::soft_shape_alpha_band`), in the shape's own box (a circle is
        // an ellipse in a wide box), with the narrower soft edge of the animations (`SOFT_BAND`). `(in)` enters from the outside.
        use crate::transition_plan::{SoftShape, soft_shape_alpha_band, SOFT_COLUMNS, SOFT_LEVELS, SOFT_ROWS};
        let shape = match name {
            "circle" => SoftShape::Circle,
            "diamond" => SoftShape::Diamond,
            "plus" => SoftShape::Plus,
            "box" => SoftShape::Box,
            // スプリット: `outVertical` opens along the width, `outHorizontal` along the height; `in...` closes in from the sides.
            "barn" | "split" => if arg.contains("ertical") { SoftShape::SplitAcross } else { SoftShape::SplitDown },
            _ => SoftShape::Wedge,
        };
        let inward = if matches!(name, "barn" | "split") { arg.starts_with("in") } else { arg == "in" };
        let q = progress.clamp(0.0, 1.0);
        let share: Vec<Vec<f64>> = (0..SOFT_ROWS)
            .map(|r| {
                let y = (r as f64 + 0.5) / SOFT_ROWS as f64;
                (0..SOFT_COLUMNS)
                    .map(|c| {
                        let x = (c as f64 + 0.5) / SOFT_COLUMNS as f64;
                        let s = soft_shape_alpha_band(shape, 0, inward, q, x, y, 1.0, Some(SOFT_BAND));
                        if enter { s } else { 1.0 - s }
                    })
                    .collect()
            })
            .collect();
        return Some(crate::transition_plan::share_windows_of_levels(&share, SOFT_LEVELS));
    }
    if name == "strips" {
        // ストリップ: a soft diagonal sweep from the corner the content comes from (`strips(downLeft)` moves down and left, so it
        // starts at the top right, as the slide transition's strips do). Measured on PowerPoint for Windows (the video of the 291
        // effects, 0.5 s, the four directions, a shape of 640 x 240 px on 1280 x 720): the shape is 16 x 15 blocks (40 x 16 px; the
        // count for other sizes is not measured), each fading in by the cosine of `u = (q - 0.87 s) / 0.13`, `q` the share of the
        // time and `s` (0 to 1) the sum of the block's distances from the corner over 2; rms error of the share 0.029 in all four.
        let (x0, y0) = match arg {
            "downLeft" => (1.0, 0.0),
            "downRight" => (0.0, 0.0),
            "upRight" => (0.0, 1.0),
            _ => (1.0, 1.0),
        };
        // Going out the sweep starts at the opposite corner (the content leaves the way it came: 0.7 % against 4.2 % with the
        // coming-in corner, the exits of the 291 effects).
        let (x0, y0) = if enter { (x0, y0) } else { (1.0 - x0, 1.0 - y0) };
        return Some(strips_tiles(x0, y0, progress.clamp(0.0, 1.0), enter));
    }
    reveal_rects(filter, enter, progress, aspect).map(|v| v.into_iter().map(|r| (r, 1.0)).collect())
}

/// The rectangles of the frame (fractions) that are shown while filter `filter` has run `progress` of its time, bringing the shape
/// in (`enter`) or taking it away; `None` when the filter is not one drawn as rectangles (the caller fades).
pub fn reveal_rects(filter: &str, enter: bool, progress: f64, aspect: f64) -> Option<Vec<Rect>> {
    let (name, arg) = split_filter(filter);
    let q = progress.clamp(0.0, 1.0);
    if name == "plus" {
        // プラス: `plus(out)` brings the shape in as a cross growing from the middle, `plus(in)` as four corner squares growing to meet;
        // going out, what is left is what coming in would not have shown (measured on PowerPoint for Windows: exit `plus(in)` takes the corner
        // squares away from the outside, the cross shrinking to a thin line).
        let k = q;
        let inward = arg == "in";
        let mut v = Vec::new();
        let cross = |t: f64| [[0.5 - t / 2.0, 0.0, 0.5 + t / 2.0, 1.0], [0.0, 0.5 - t / 2.0, 1.0, 0.5 + t / 2.0]];
        let corners = |c: f64| [[0.0, 0.0, c, c], [1.0 - c, 0.0, 1.0, c], [0.0, 1.0 - c, c, 1.0], [1.0 - c, 1.0 - c, 1.0, 1.0]];
        match (enter, inward) {
            (true, false) => v.extend(cross(k)),
            (true, true) => v.extend(corners(k / 2.0).into_iter().filter(|r| r[2] > r[0])),
            (false, true) => v.extend(cross(1.0 - k)),
            (false, false) => v.extend(corners((1.0 - k) / 2.0).into_iter().filter(|r| r[2] > r[0])),
        }
        return Some(v);
    }
    if name == "wheel" || name == "wedge" {
        // The swept part is the new one; going out, what is left of the shape is what has not been swept.
        let (rows, cols) = (SECTOR_ROWS, SECTOR_COLS);
        if enter {
            return Some(swept_sectors(name, arg, q, aspect, rows, cols));
        }
        let (name, arg) = (name.to_string(), arg.to_string());
        let width = if name == "wedge" { 360.0 } else { 360.0 / arg.parse::<usize>().ok().filter(|n| (1..=16).contains(n)).unwrap_or(4) as f64 };
        return Some(sector_rects(rows, cols, aspect, &|a: f64| {
            if name == "wedge" {
                let half = 180.0 * q;
                !(a <= half || 360.0 - a <= half)
            } else {
                a.rem_euclid(width) > width * q
            }
        }));
    }
    if name == "dissolve" {
        // ディゾルブ: the frame cut into small squares, each appearing (or going) when the threshold, which runs from 0 to 1 with the time, passes its value of the
        // random picture (`dissolve_value`); the share of the shape drawn then follows an S (0.077 at 0.64 s of 2, 0.30 at 0.8 s, 0.5 at 1.0 s, 0.946 at 1.6 s: PowerPoint's).
        let mut v = Vec::new();
        for r in 0..DISSOLVE_ROWS {
            let mut run: Option<usize> = None;
            for c in 0..=DISSOLVE_COLS {
                let shown = c < DISSOLVE_COLS && (dissolve_value((c as f64 + 0.5) / DISSOLVE_COLS as f64, (r as f64 + 0.5) / DISSOLVE_ROWS as f64) < q) == enter;
                match (shown, run) {
                    (true, None) => run = Some(c),
                    (false, Some(start)) => {
                        v.push([start as f64 / DISSOLVE_COLS as f64, r as f64 / DISSOLVE_ROWS as f64, c as f64 / DISSOLVE_COLS as f64, (r + 1) as f64 / DISSOLVE_ROWS as f64]);
                        run = None;
                    }
                    _ => {}
                }
            }
        }
        return Some(v);
    }
    if name == "blinds" || name == "checkerboard" {
        // The slats / cells open (and close) along a trapezoid of speed: a start and an end of 13 % of the time each, the middle
        // faster (PowerPoint's video of the exits, 0.5 s and 4 s: the share left is linear between 6.8 % and 93.2 % of the time and
        // soft outside it; 6 videos, 130 frames, fitted 0.11 to 0.15). Going out, what is left is the part coming in would show at
        // the time reversed (the tops of the slats, the left of the cells), not what it leaves out.
        let p = trapezoid(q, 0.13);
        let cells = cells(name, arg, if enter { p } else { 1.0 - p }, aspect)?;
        return Some(cells.into_iter().filter(|c| c.inner[2] > c.inner[0] && c.inner[3] > c.inner[1]).map(|c| c.inner).collect());
    }
    let cells = cells(name, arg, q, aspect)?;
    let mut out = Vec::new();
    for c in cells {
        // The shown part of a cell: its inner (or all but it) coming in, the rest going out.
        let show_inner = c.ring != enter;
        // `ring` cells (…in) come in as the ring: all of outer but inner; going out they leave the inner.
        if show_inner {
            if c.inner[2] > c.inner[0] && c.inner[3] > c.inner[1] {
                out.push(c.inner);
            }
        } else {
            rect_minus(c.outer, c.inner, &mut out);
        }
    }
    Some(out)
}

/// Whether an entrance (`enter`) effect's filter hides the shape at its start and shows it at its end, as fade does:
/// the opacity a filter without rectangles (fade, wheel, wedge, dissolve…) leaves.
pub fn reveal_opacity(enter: bool, progress: f64) -> f64 {
    if enter { progress } else { 1.0 - progress }
}

/// The opacity filter `filter` leaves when it is not drawn as rectangles: `fade` (and any filter the player does not know) fades;
/// `image` is PowerPoint's way to write a picture's own properties (`prLst="opacity: 0.5"`, 透明度), no change over time.
pub fn filter_opacity(filter: &str, enter: bool, progress: f64) -> f64 {
    if split_filter(filter).0 == "image" { 1.0 } else { reveal_opacity(enter, progress) }
}

// ---- 新しい効果の定義（追加できる開始・終了の効果） ----

/// An effect of the preset catalogue: PowerPoint's `presetID` with its options (`presetSubtype`) and the filter each plays.
pub struct PresetDef {
    pub id: u32,
    /// The i18n id of its name, `pptx.anim.preset.<name>`.
    pub key: &'static str,
    /// (`presetSubtype`, i18n id of the option's name, filter).
    pub options: &'static [(u32, &'static str, &'static str)],
}

macro_rules! preset {
    ($id:literal, $name:literal, [$(($sub:literal, $opt:literal, $filter:literal)),+ $(,)?]) => {
        PresetDef { id: $id, key: concat!("pptx.animation.preset.", $name), options: &[$(($sub, concat!("pptx.animation.preset.", $name, ".", $opt), $filter)),+] }
    };
}

/// The filter-based effects of PowerPoint's basic entrance and exit effects (the ones that need no formula).
pub const FILTER_PRESETS: &[PresetDef] = &[
    // The subtypes are what PowerPoint (Mac, 16.x) writes for each option (read back from files it saved, 2026-10-31).
    preset!(3, "blinds", [(10, "horizontal", "blinds(horizontal)"), (5, "vertical", "blinds(vertical)")]),
    preset!(4, "box", [(16, "in", "box(in)"), (32, "out", "box(out)")]),
    preset!(5, "checkerboard", [(10, "across", "checkerboard(across)"), (5, "down", "checkerboard(down)")]),
    preset!(6, "circle", [(16, "in", "circle(in)"), (32, "out", "circle(out)")]),
    preset!(8, "diamond", [(16, "in", "diamond(in)"), (32, "out", "diamond(out)")]),
    preset!(13, "plus", [(16, "in", "plus(in)"), (32, "out", "plus(out)")]),
    preset!(14, "randombar", [(10, "horizontal", "randombar(horizontal)"), (5, "vertical", "randombar(vertical)")]),
    preset!(16, "split", [(37, "out-vertical", "barn(outVertical)"), (21, "in-vertical", "barn(inVertical)"), (42, "out-horizontal", "barn(outHorizontal)"), (26, "in-horizontal", "barn(inHorizontal)")]),
    preset!(18, "strips", [(12, "down-left", "strips(downLeft)"), (9, "up-left", "strips(upLeft)"), (6, "down-right", "strips(downRight)"), (3, "up-right", "strips(upRight)")]),
];

/// The catalogue entry for `(id, subtype)`.
pub fn preset_def(id: u32) -> Option<&'static PresetDef> {
    FILTER_PRESETS.iter().find(|d| d.id == id)
}

/// The behaviours of catalogue effect `id` with option `subtype` as an entrance or an exit of `dur` milliseconds, as PowerPoint
/// writes them: the shape made visible, then the filter in (hidden after it, then the filter out).
pub fn build_filter_preset(class: AnimationClass, id: u32, subtype: u32, dur: u32) -> Option<Vec<AnimBehavior>> {
    let def = preset_def(id)?;
    let (_, _, filter) = def.options.iter().find(|(s, _, _)| *s == subtype).or(def.options.first())?;
    let dur = dur.max(1);
    let effect = |enter| AnimBehavior { kind: BehaviorKind::Effect { filter: (*filter).to_string(), enter }, begin: 0, dur, time: BehaviorTime { remove: true, ..BehaviorTime::default() } };
    match class {
        AnimationClass::Entrance => Some(vec![
            AnimBehavior { kind: BehaviorKind::Set { attr: "style.visibility".into(), value: "visible".into() }, begin: 0, dur: 1, time: BehaviorTime::default() },
            effect(true),
        ]),
        AnimationClass::Exit => Some(vec![
            effect(false),
            AnimBehavior { kind: BehaviorKind::Set { attr: "style.visibility".into(), value: "hidden".into() }, begin: dur.saturating_sub(1), dur: 1, time: BehaviorTime::default() },
        ]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: PptBox = PptBox { x: 0.5, y: 0.25, w: 0.2, h: 0.1 };

    fn num(text: &str) -> Option<f64> {
        eval_expr(text, &box_var(B, B, Some(0.5)))
    }

    /// The area (of the shape's box, 0 to 1) a filter's windows cover, each at its opacity.
    fn covered(windows: &[(Rect, f64)]) -> f64 {
        windows.iter().map(|(r, o)| (r[2] - r[0]) * (r[3] - r[1]) * o).sum()
    }

    /// The soft shapes are the slide transitions' (one function, `soft_shape_alpha_band`) with the animations' narrower edge: a diamond coming in from the outside leaves
    /// the middle and a band of soft edge, and what it brings in is the rest; going out is the other way round.
    #[test]
    fn circle_diamond_plus_and_wedge_have_a_soft_edge() {
        for filter in ["circle(out)", "circle(in)", "diamond(out)", "diamond(in)", "plus(out)", "plus(in)", "wedge", "box(out)", "box(in)", "barn(outVertical)", "barn(inVertical)", "barn(outHorizontal)", "barn(inHorizontal)"] {
            let at = |enter: bool, p: f64| reveal_windows(filter, enter, p, 4.0 / 3.0).map(|w| covered(&w)).expect(filter);
            assert!(at(true, 0.0) < 0.01 && at(true, 1.0) > 0.99, "{filter}: {} {}", at(true, 0.0), at(true, 1.0));
            assert!(at(false, 0.0) > 0.99 && at(false, 1.0) < 0.01, "{filter}");
            // Growing steadily: the share shown is not hard-edged (a hard edge shows only opacities of 1).
            let w = reveal_windows(filter, true, 0.4, 4.0).unwrap();
            assert!(w.iter().any(|(_, o)| *o < 0.99) && at(true, 0.3) < at(true, 0.5), "{filter}");
        }
        // A diamond coming in leaves from the middle (`out`) or from the corners (`in`): the middle of the box at a third of the time.
        let middle = |filter: &str| reveal_windows(filter, true, 0.3, 4.0).unwrap().iter().any(|(r, o)| r[0] < 0.5 && r[2] > 0.5 && r[1] < 0.5 && r[3] > 0.5 && *o > 0.9);
        assert!(middle("diamond(out)") && !middle("diamond(in)"));
    }

    /// ランダム ストライプ: bars a pixel (0.75 pt) thick, each coming in once, between a tenth and nine tenths of the time; the share shown grows steadily and
    /// a bar that has come in stays.
    #[test]
    fn random_bars_are_pixels_that_come_in_at_random_times() {
        let size = Some((480.0, 180.0));
        let share = |filter: &str, enter: bool, p: f64| covered(&reveal_windows_sized(filter, enter, p, 480.0 / 180.0, size).unwrap());
        for filter in ["randombar(vertical)", "randombar(horizontal)"] {
            assert!(share(filter, true, 0.05) < 1e-9 && (share(filter, true, 0.95) - 1.0).abs() < 1e-9 && share(filter, false, 0.95) < 1e-9, "{filter}");
            let (a, b, c) = (share(filter, true, 0.3), share(filter, true, 0.5), share(filter, true, 0.7));
            assert!(a < b && b < c && (b - 0.5).abs() < 0.08 && (a - 0.25).abs() < 0.08, "{filter}: {a} {b} {c}");
        }
        // A bar is a pixel thick: 640 of them across 480 pt, so none is thicker than a few of them.
        let w = reveal_windows_sized("randombar(vertical)", true, 0.5, 2.0, size).unwrap();
        assert!(w.len() > 100 && w.iter().all(|(r, o)| *o == 1.0 && r[2] - r[0] < 0.1), "{} windows", w.len());
    }

    #[test]
    fn expressions_are_evaluated() {
        assert_eq!(num("#ppt_x"), Some(0.5));
        assert_eq!(num("1+#ppt_h/2"), Some(1.05));
        assert_eq!(num("0-#ppt_w/2"), Some(-0.1));
        assert_eq!(num("#ppt_y+(1-$)*2"), Some(1.25));
        assert!((num("sin(pi*$)").unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(num("1+"), None);
        // PowerPoint's 'Basic Zoom' from the bottom: `min`/`max` and a number without its leading zero.
        assert!((num("(6*min(max(#ppt_w*#ppt_h,.3),1)-7.4)/-.7*#ppt_w").unwrap() - 0.2 * (6.0 * 0.3 - 7.4) / -0.7).abs() < 1e-9);
        assert_eq!(num("#nope"), None);
    }

    /// ストリップ coming in starts at the corner opposite to the direction's name (`downLeft` at the top right); going out at the
    /// other one; its front is soft (blocks at a share between 0 and 1).
    #[test]
    fn strips_start_at_the_corner_opposite_the_direction() {
        let at = |filter: &str, enter: bool, x: f64, y: f64| {
            let w = reveal_windows(filter, enter, 0.3, 1.0).expect("strips");
            w.iter().find(|(r, _)| r[0] <= x && x < r[2] && r[1] <= y && y < r[3]).map_or(0.0, |w| w.1)
        };
        assert!(at("strips(downLeft)", true, 0.95, 0.05) > at("strips(downLeft)", true, 0.05, 0.95) + 0.5);
        assert!(at("strips(upLeft)", true, 0.95, 0.95) > at("strips(upLeft)", true, 0.05, 0.05) + 0.5);
        assert!(at("strips(downLeft)", false, 0.95, 0.05) > at("strips(downLeft)", false, 0.05, 0.95) + 0.5);
        let soft = reveal_windows("strips(downLeft)", true, 0.5, 1.0).unwrap().iter().filter(|(_, a)| *a > 0.05 && *a < 0.95).count();
        assert!(soft > 0);
    }

    /// ホイール is drawn on a grid twice as fine (320 x 180) and with 32 levels of opacity: PowerPoint's edge is smooth to the pixel, and the
    /// coarser grid (160 x 90, 16 levels) showed steps along it and a square block where 8 spokes meet at the middle.
    #[test]
    fn the_wheels_band_has_many_levels() {
        let w = reveal_windows("wheel(8)", true, 0.5, 1.0).expect("wheel");
        let mut levels: Vec<u32> = w.iter().map(|(_, a)| (a * 1e6) as u32).collect();
        levels.sort_unstable();
        levels.dedup();
        assert!(levels.len() >= 30, "{}", levels.len());
        // The middle: no window of a coarse cell's size (1/160 of the width) reaches into it (a window is as thin as a 320th).
        assert!(w.iter().any(|(r, _)| r[2] - r[0] < 1.5 / 320.0 || r[3] - r[1] < 1.5 / 180.0));
    }

    /// Fly-in from the left: x goes from `0-#ppt_w/2` to `#ppt_x`, linearly.
    #[test]
    fn a_fly_in_moves_along_its_values() {
        let x = AnimBehavior {
            kind: BehaviorKind::Anim { attr: "ppt_x".into(), keys: vec![(0, "0-#ppt_w/2".into()), (100_000, "#ppt_x".into())], formula: None, discrete: false },
            begin: 0,
            dur: 500,
            time: BehaviorTime::default(),
        };
        let at = |ms| sample(std::slice::from_ref(&x), ms, B).x.unwrap();
        assert!((at(0.0) - -0.1).abs() < 1e-9);
        assert!((at(250.0) - 0.2).abs() < 1e-9);
        assert!((at(500.0) - 0.5).abs() < 1e-9);
        assert!((at(900.0) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_formula_sees_the_value_as_a_dollar() {
        let y = AnimBehavior {
            kind: BehaviorKind::Anim { attr: "ppt_y".into(), keys: vec![(0, "0".into()), (100_000, "1".into())], formula: Some("#ppt_y-sin(pi*$)*0.1".into()), discrete: false },
            begin: 0,
            dur: 1000,
            time: BehaviorTime::default(),
        };
        let v = sample(&[y], 500.0, B).y.unwrap();
        assert!((v - 0.15).abs() < 1e-9);
    }

    /// ワイプ as the windows a shape is drawn through: a linear ramp a seventh of the extent wide, the front running from before the start
    /// edge to beyond the far one (PowerPoint for Windows' video of the 291 effects).
    #[test]
    fn a_wipe_has_a_soft_linear_edge_a_seventh_wide() {
        let at = |q: f64, x: f64| -> f64 {
            let w = reveal_windows("wipe(left)", true, q, 2.0).unwrap();
            w.iter().find(|(r, _)| r[0] <= x && x < r[2]).map_or(0.0, |&(_, a)| a)
        };
        // Nothing at the start, everything at the end.
        assert!(at(0.0, 0.01) < 0.01 && (at(1.0, 0.99) - 1.0).abs() < 1e-9);
        // The ramp at half time: its front at 0.5 * 8/7 = 0.5714, the ramp from 0.4286: 10 % at 0.5571 and 90 % at 0.4429.
        assert!((at(0.5, 0.4429) - 0.9).abs() < 0.03 && (at(0.5, 0.5571) - 0.1).abs() < 0.03);
        assert!((at(0.5, 0.50) - 0.5).abs() < 0.04);
        // Going out it is the opposite.
        let out = reveal_windows("wipe(left)", false, 0.5, 2.0).unwrap();
        assert!(out.iter().find(|(r, _)| r[0] <= 0.1 && 0.1 < r[2]).is_none());
        // From the right the same ramp, mirrored.
        let right = reveal_windows("wipe(right)", true, 0.5, 2.0).unwrap();
        assert!((right.iter().find(|(r, _)| r[0] <= 0.5 && 0.5 < r[2]).unwrap().1 - 0.5).abs() < 0.04);
    }

    #[test]
    fn a_wipe_covers_the_frame_by_its_progress() {
        let half = reveal_rects("wipe(down)", true, 0.5, 2.0).unwrap();
        assert_eq!(half, vec![[0.0, 0.5, 1.0, 1.0]]);
        // Going out from the bottom: the top part stays.
        let out = reveal_rects("wipe(down)", false, 0.25, 2.0).unwrap();
        assert_eq!(out, vec![[0.0, 0.0, 1.0, 0.75]]);
        // Everything at the end of an entrance, nothing left at the end of an exit.
        assert_eq!(reveal_rects("blinds(horizontal)", true, 1.0, 1.0).unwrap().len(), SLATS);
        assert!(reveal_rects("blinds(horizontal)", false, 1.0, 1.0).unwrap().is_empty());
        // A wheel of one spoke at half time has swept the right half; a wedge at half time the upper half.
        let wheel = reveal_rects("wheel(1)", true, 0.5, 1.0).unwrap();
        let area = |v: &[Rect]| v.iter().map(|r| (r[2] - r[0]) * (r[3] - r[1])).sum::<f64>();
        assert!((area(&wheel) - 0.5).abs() < 0.03, "{}", area(&wheel));
        assert!(wheel.iter().all(|r| r[0] >= 0.49), "the right half");
        let wedge = reveal_rects("wedge", true, 0.5, 1.0).unwrap();
        assert!((area(&wedge) - 0.5).abs() < 0.03 && wedge.iter().all(|r| r[3] <= 0.51 + 1.0 / SECTOR_ROWS as f64), "{}", area(&wedge));
        assert!(reveal_rects("wheel(4)", true, 1.0, 1.0).unwrap().len() >= SECTOR_ROWS);
        // ディゾルブ: the squares shown grow with time, and what an exit leaves is what an entrance has not yet shown.
        let squares = |q: f64, enter: bool| reveal_rects("dissolve", enter, q, 1.0).unwrap().iter().map(|r| (r[2] - r[0]) * (r[3] - r[1])).sum::<f64>();
        assert!(squares(0.0, true) < 0.01 && (squares(1.0, true) - 1.0).abs() < 0.01 && (squares(0.5, true) - 0.5).abs() < 0.08);
        assert!((squares(0.5, true) + squares(0.5, false) - 1.0).abs() < 1e-9);
        // The share drawn at 0.4 and 0.8 of the time of PowerPoint's (2 s: 0.16 at 0.64 s... the mean opacity 0.30 at 0.8 s, 0.946 at 1.6 s).
        assert!((squares(0.4, true) - 0.30).abs() < 0.05 && (squares(0.8, true) - 0.946).abs() < 0.04, "{} {}", squares(0.4, true), squares(0.8, true));
        // The picture is a fixed one, and a threshold only ever adds squares.
        let a = reveal_rects("dissolve", true, 0.3, 1.0).unwrap();
        assert_eq!(a, reveal_rects("dissolve", true, 0.3, 1.0).unwrap());
        assert!(squares(0.2, true) < squares(0.3, true));
        assert!(reveal_rects("fade", true, 0.5, 1.0).is_none());
    }

    #[test]
    fn centred_filters_grow_from_the_centre_or_close_in() {
        assert_eq!(reveal_rects("box(out)", true, 0.5, 1.0).unwrap(), vec![[0.25, 0.25, 0.75, 0.75]]);
        // box(in): the frame but a shrinking hole.
        let ring = reveal_rects("box(in)", true, 0.5, 1.0).unwrap();
        let area: f64 = ring.iter().map(|r| (r[2] - r[0]) * (r[3] - r[1])).sum();
        assert!((area - 0.75).abs() < 1e-9);
        // A circle covers the frame at its end, from nothing.
        let all: f64 = reveal_rects("circle(out)", true, 1.0, 1.0).unwrap().iter().map(|r| (r[2] - r[0]) * (r[3] - r[1])).sum();
        assert!((all - 1.0).abs() < 1e-9);
        assert!(reveal_rects("circle(out)", true, 0.0, 1.0).unwrap().is_empty());
        // The cross of plus(out).
        assert_eq!(reveal_rects("plus(out)", true, 1.0, 1.0).unwrap().len(), 2);
    }

    /// A behaviour that runs forward and back (`autoRev`) takes twice its `dur` and ends where it began; `additive="sum"` adds to
    /// what the behaviours before it left (PowerPoint's ppt_x by).
    #[test]
    fn a_behaviour_runs_back_and_adds() {
        let slide_in = AnimBehavior {
            kind: BehaviorKind::Anim { attr: "ppt_x".into(), keys: vec![(0, "0".into()), (100_000, "#ppt_x".into())], formula: None, discrete: false },
            begin: 0,
            dur: 600,
            time: BehaviorTime::default(),
        };
        let bump = AnimBehavior {
            kind: BehaviorKind::Anim { attr: "ppt_x".into(), keys: vec![(0, "0".into()), (100_000, "0.1".into())], formula: None, discrete: false },
            begin: 600,
            dur: 200,
            time: BehaviorTime { auto_rev: true, sum: true, ..Default::default() },
        };
        let both = [slide_in, bump];
        let x = |ms| sample(&both, ms, B).x.unwrap();
        assert!((x(500.0) - 0.5 * 500.0 / 600.0).abs() < 1e-9);
        assert!((x(700.0) - 0.55).abs() < 1e-9, "{}", x(700.0));
        assert!((x(800.0) - 0.6).abs() < 1e-9);
        assert!((x(900.0) - 0.55).abs() < 1e-9);
        assert!((x(1000.0) - 0.5).abs() < 1e-9 && (x(5000.0) - 0.5).abs() < 1e-9);
        assert_eq!(BehaviorTime { auto_rev: true, ..Default::default() }.active(200), 400);
    }

    #[test]
    fn smooth_start_and_end_cover_the_whole_way() {
        assert_eq!(ease(0.0, 50_000, 50_000), 0.0);
        assert!((ease(1.0, 50_000, 50_000) - 1.0).abs() < 1e-12);
        assert!((ease(0.5, 50_000, 50_000) - 0.5).abs() < 1e-12);
        // Slower than constant speed at the start, faster in the middle.
        assert!(ease(0.1, 50_000, 0) < 0.1);
        assert!(ease(0.9, 0, 50_000) > 0.9);
    }

    /// The smooth start and end of the effect's own time node go to each behaviour, over the behaviour's own stretch of time; a behaviour's
    /// own is kept where the effect's is 0 (PowerPoint's video of 開始 38: the bounce's `decel` 50 000 stays under the effect's (0, 0), is
    /// replaced by 25 000 under (0, 25 000), and the first behaviour's end does not move with the effect's accel).
    #[test]
    fn the_effects_smooth_start_and_end_go_to_each_behaviour() {
        let set = |begin: u32, dur: u32, decel: u32| AnimBehavior {
            kind: BehaviorKind::Anim { attr: "ppt_x".into(), keys: vec![(0, "0".into()), (100_000, "1".into())], formula: None, discrete: false },
            begin,
            dur,
            time: BehaviorTime { decel, ..Default::default() },
        };
        let b = PptBox { x: 0.5, y: 0.5, w: 0.2, h: 0.2 };
        let x = |bh: &[AnimBehavior], ms: f64, parent: (u32, u32)| sample_eased(bh, ms, b, parent).x.unwrap();
        // A second behaviour of 400 ms after a first of 600 ms: the first ends at 600 ms whatever the effect's accel.
        let two = [set(0, 600, 0), set(600, 400, 50_000)];
        assert!((x(&two, 599.9, (100_000, 0)) - 1.0).abs() < 1e-3 && (x(&two, 599.9, (0, 0)) - 1.0).abs() < 1e-3);
        assert!((x(&two, 300.0, (100_000, 0)) - ease(0.5, 100_000, 0)).abs() < 1e-9);
        // The behaviour's own decel under (0, 0); the effect's replaces it where it is not 0.
        let mid = |parent: (u32, u32)| x(&two, 900.0, parent);
        assert!((mid((0, 0)) - ease(0.75, 0, 50_000)).abs() < 1e-9);
        assert!((mid((0, 25_000)) - ease(0.75, 0, 25_000)).abs() < 1e-9);
        assert!((mid((25_000, 0)) - ease(0.75, 25_000, 50_000)).abs() < 1e-9);
    }

    /// 軌跡 inside an effect (PowerPoint's presetID 52 spiral: `M -0.4 0.9 C … 0 0`): at the start the shape is at the path's start,
    /// at the end where it is.
    #[test]
    fn a_motion_path_takes_the_shape_to_its_place() {
        let path = "M -0.5 0.5  C -0.4 0.4  -0.1 0.1  0.0 0.0  ";
        let (x0, y0) = motion_point(path, 0.0).unwrap();
        assert!((x0 + 0.5).abs() < 1e-9 && (y0 - 0.5).abs() < 1e-9);
        let (x1, y1) = motion_point(path, 1.0).unwrap();
        assert!(x1.abs() < 1e-9 && y1.abs() < 1e-9);
        let (xm, ym) = motion_point("M 0 0 L 0.2 0 L 0.2 0.2 E", 0.5).unwrap();
        assert!((xm - 0.2).abs() < 1e-9 && ym.abs() < 1e-9);
        let raw = "<p:animMotion origin=\"layout\" path=\"M -0.5 0.5 L 0 0\" pathEditMode=\"relative\">".to_string();
        let m = AnimBehavior { kind: BehaviorKind::Opaque(raw), begin: 0, dur: 1000, time: BehaviorTime::default() };
        let at = sample(&[m], 0.0, B).motion.unwrap();
        assert!((at.0 + 0.5).abs() < 1e-9);
    }

    /// フラッシュ (1 回): a `set visible` of 1 s with no `fill="hold"` shows the shape for that second and no longer; the exit's
    /// `anim` of text values (hidden, then visible at half the time) steps.
    #[test]
    fn a_behaviour_without_fill_hold_goes_when_it_ends() {
        let shown = AnimBehavior { kind: BehaviorKind::Set { attr: "style.visibility".into(), value: "visible".into() }, begin: 0, dur: 1000, time: BehaviorTime { remove: true, ..Default::default() } };
        let at = |ms| sample(std::slice::from_ref(&shown), ms, B).visible;
        assert_eq!((at(500.0), at(1500.0)), (Some(true), None));
        let steps = AnimBehavior {
            kind: BehaviorKind::Anim { attr: "style.visibility".into(), keys: vec![(0, "hidden".into()), (50_000, "visible".into())], formula: None, discrete: true },
            begin: 0,
            dur: 1000,
            time: BehaviorTime { remove: true, ..Default::default() },
        };
        let v = |ms| sample(std::slice::from_ref(&steps), ms, B).visible;
        assert_eq!((v(100.0), v(600.0)), (Some(false), Some(true)));
    }

    #[test]
    fn the_catalogue_builds_an_entrance_and_an_exit() {
        let entrance = build_filter_preset(AnimationClass::Entrance, 3, 5, 500).unwrap();
        assert!(matches!(&entrance[1].kind, BehaviorKind::Effect { filter, enter: true } if filter == "blinds(vertical)"));
        let exit = build_filter_preset(AnimationClass::Exit, 16, 21, 500).unwrap();
        assert!(matches!(&exit[0].kind, BehaviorKind::Effect { filter, enter: false } if filter == "barn(inVertical)"));
        assert_eq!(exit[1].begin, 499);
        assert!(build_filter_preset(AnimationClass::Emphasis, 3, 5, 500).is_none());
    }

    #[test]
    fn a_time_filter_remaps_the_progress() {
        let t = BehaviorTime { filter: BehaviorTime::parse_filter("0,0; .5,.2; 1,1"), ..Default::default() };
        assert!((t.filtered(0.25) - 0.1).abs() < 1e-9 && (t.filtered(0.5) - 0.2).abs() < 1e-9 && (t.filtered(0.75) - 0.6).abs() < 1e-9);
        assert_eq!(t.filtered(1.0), 1.0);
        assert_eq!(BehaviorTime::default().filtered(0.3), 0.3);
        assert_eq!(BehaviorTime::parse_filter(&t.filter_text()), t.filter);
    }

    #[test]
    fn a_name_without_the_hash_is_where_the_shape_is_now() {
        // PowerPoint's exit 43 moves the shape 0.31 down and then says `ppt_y` for the rest: it stays there.
        let anim = |from: &str, to: &str, begin: u32, dur: u32| AnimBehavior {
            kind: BehaviorKind::Anim { attr: "ppt_y".into(), keys: vec![(0, from.into()), (100_000, to.into())], formula: None, discrete: false },
            begin,
            dur,
            time: BehaviorTime::default(),
        };
        let behaviours = [anim("ppt_y", "ppt_y+0.31", 0, 600), anim("ppt_y", "ppt_y", 600, 400)];
        let b = PptBox { x: 0.5, y: 0.4, w: 0.2, h: 0.2 };
        assert!((sample(&behaviours, 300.0, b).y.unwrap() - 0.555).abs() < 1e-9);
        assert!((sample(&behaviours, 800.0, b).y.unwrap() - 0.71).abs() < 1e-9);
    }

    #[test]
    fn plus_going_out_takes_away_what_coming_in_would_show() {
        let area = |filter: &str, enter: bool, p: f64| reveal_rects(filter, enter, p, 1.0).unwrap().iter().map(|r| (r[2] - r[0]) * (r[3] - r[1])).sum::<f64>();
        // In: the corner squares grow (0.25 of the frame when each is half way across), out: the cross is what is left.
        assert!((area("plus(in)", true, 1.0) - 1.0).abs() < 1e-9);
        assert!((area("plus(in)", false, 0.0) - 2.0).abs() < 1e-9, "the arms of the cross, overlapping at the middle");
        assert!(area("plus(in)", false, 1.0).abs() < 1e-9);
        assert!((area("plus(out)", false, 0.0) - 4.0 * 0.25).abs() < 1e-9);
    }
}

