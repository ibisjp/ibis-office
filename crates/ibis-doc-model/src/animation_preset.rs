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

fn box_var(b: PptBox, extra: Option<f64>) -> impl Fn(&str) -> Option<f64> {
    move |name| match name {
        // PowerPoint writes the exit zoom's start as `ppt_w` (without the `#`) too.
        "#ppt_x" | "ppt_x" => Some(b.x),
        "#ppt_y" | "ppt_y" => Some(b.y),
        "#ppt_w" | "ppt_w" => Some(b.w),
        "#ppt_h" | "ppt_h" => Some(b.h),
        "$" => extra,
        _ => None,
    }
}

/// Where behaviour `anim` is (value of its attribute) at `q` (0 to 1) of its time.
fn anim_value(keys: &[(u32, String)], formula: Option<&str>, discrete: bool, q: f64, b: PptBox) -> Option<f64> {
    let num = |s: &str, extra: Option<f64>| eval_expr(s, &box_var(b, extra));
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

/// The state the `behaviours` of an effect leave the shape in `ms` after the effect begins; `b` is the shape's own box.
pub fn sample(behaviours: &[AnimBehavior], ms: f64, b: PptBox) -> PresetState {
    let mut s = PresetState::default();
    for bh in behaviours {
        if ms < f64::from(bh.begin) {
            continue;
        }
        let q = if bh.dur == 0 { 1.0 } else { ((ms - f64::from(bh.begin)) / f64::from(bh.dur)).clamp(0.0, 1.0) };
        match &bh.kind {
            BehaviorKind::Set { attr, value } => {
                if attr == "style.visibility" {
                    s.visible = Some(value == "visible");
                } else if attr == "style.opacity" {
                    s.opacity = value.parse().ok();
                }
            }
            BehaviorKind::Anim { attr, keys, formula, discrete } => {
                let Some(v) = anim_value(keys, formula.as_deref(), *discrete, q, b) else { continue };
                match attr.as_str() {
                    "ppt_x" => s.x = Some(v),
                    "ppt_y" => s.y = Some(v),
                    "ppt_w" => s.w = Some(v),
                    "ppt_h" => s.h = Some(v),
                    "style.rotation" | "r" => s.rotation = Some(v),
                    "style.opacity" => s.opacity = Some(v),
                    "style.visibility" => s.visible = Some(v >= 0.5),
                    _ => {}
                }
            }
            BehaviorKind::Effect { filter, enter } => {
                s.reveal = Some(Reveal { filter: filter.clone(), enter: *enter, progress: q });
            }
            BehaviorKind::Scale { from, to, by } => {
                let f = |v: (i32, i32)| (f64::from(v.0) / 100_000.0, f64::from(v.1) / 100_000.0);
                let (a, z) = match (from, to, by) {
                    (Some(a), Some(z), _) => (f(*a), f(*z)),
                    (None, None, Some(by)) => ((1.0, 1.0), f(*by)),
                    (Some(a), None, Some(by)) => (f(*a), (f(*a).0 * f(*by).0, f(*a).1 * f(*by).1)),
                    (None, Some(z), _) => ((1.0, 1.0), f(*z)),
                    (Some(a), None, None) => (f(*a), (1.0, 1.0)),
                    (None, None, None) => continue,
                };
                s.scale = Some((a.0 + (z.0 - a.0) * q, a.1 + (z.1 - a.1) * q));
            }
            BehaviorKind::Opaque(_) => {}
            BehaviorKind::Rotate { from, to, by } => {
                let d = |v: i32| f64::from(v) / 60_000.0;
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

/// How many slices a round or diagonal filter is drawn in (rectangles are all a window clips to).
const SLICES: usize = 10;
const SLATS: usize = 6;
const BARS: usize = 12;
/// The bands and columns a sector is drawn in.
const SECTOR_ROWS: usize = 24;
const SECTOR_COLS: usize = 96;

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
            const N: usize = 4;
            let across = arg != "down";
            let mut out = Vec::new();
            for row in 0..N {
                for col in 0..N {
                    let outer: Rect = [col as f64 / N as f64, row as f64 / N as f64, (col + 1) as f64 / N as f64, (row + 1) as f64 / N as f64];
                    // The squares of one colour first, the others after.
                    let local = if (row + col) % 2 == 0 { (q * 2.0).min(1.0) } else { (q * 2.0 - 1.0).max(0.0) };
                    out.push(Cell { outer, inner: strip(outer, if across { 'l' } else { 't' }, local), ring: false });
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
/// clockwise from straight up (0 to 360). Rectangles are all a window clips to, so a sector is a staircase of them. `aspect`
/// is the frame's width over its height (the angle is of the picture, not of the unit square).
pub fn sector_rects(rows: usize, cols: usize, aspect: f64, inside: &dyn Fn(f64) -> bool) -> Vec<Rect> {
    let mut out = Vec::new();
    for r in 0..rows {
        let y = (r as f64 + 0.5) / rows as f64;
        let mut run: Option<usize> = None;
        for c in 0..=cols {
            let hit = c < cols && {
                let x = (c as f64 + 0.5) / cols as f64;
                let angle = ((x - 0.5) * aspect).atan2(0.5 - y).to_degrees();
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

/// The rectangles of the frame (fractions) that are shown while filter `filter` has run `progress` of its time, bringing the shape
/// in (`enter`) or taking it away; `None` when the filter is not one drawn as rectangles (the caller fades).
pub fn reveal_rects(filter: &str, enter: bool, progress: f64, aspect: f64) -> Option<Vec<Rect>> {
    let (name, arg) = split_filter(filter);
    let q = progress.clamp(0.0, 1.0);
    if name == "plus" {
        // A cross grows from the centre; the corners are what goes last (and first, going out).
        let k = q;
        let cross = [[0.5 - k / 2.0, 0.0, 0.5 + k / 2.0, 1.0], [0.0, 0.5 - k / 2.0, 1.0, 0.5 + k / 2.0]];
        let inward = arg == "in";
        let mut v = Vec::new();
        // plus(out): the cross grows to the whole; plus(in): the corners close in on the centre.
        let show_cross = enter != inward;
        if show_cross {
            v.extend(cross);
        } else {
            let c = 0.5 - (1.0 - k) / 2.0;
            for corner in [[0.0, 0.0, c, c], [1.0 - c, 0.0, 1.0, c], [0.0, 1.0 - c, c, 1.0], [1.0 - c, 1.0 - c, 1.0, 1.0]] {
                if corner[2] > corner[0] {
                    v.push(corner);
                }
            }
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
        PresetDef { id: $id, key: concat!("pptx.anim.preset.", $name), options: &[$(($sub, concat!("pptx.anim.preset.", $name, ".", $opt), $filter)),+] }
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
    let effect = |enter| AnimBehavior { kind: BehaviorKind::Effect { filter: (*filter).to_string(), enter }, begin: 0, dur };
    match class {
        AnimationClass::Entrance => Some(vec![
            AnimBehavior { kind: BehaviorKind::Set { attr: "style.visibility".into(), value: "visible".into() }, begin: 0, dur: 1 },
            effect(true),
        ]),
        AnimationClass::Exit => Some(vec![
            effect(false),
            AnimBehavior { kind: BehaviorKind::Set { attr: "style.visibility".into(), value: "hidden".into() }, begin: dur.saturating_sub(1), dur: 1 },
        ]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const B: PptBox = PptBox { x: 0.5, y: 0.25, w: 0.2, h: 0.1 };

    fn num(text: &str) -> Option<f64> {
        eval_expr(text, &box_var(B, Some(0.5)))
    }

    #[test]
    fn expressions_are_evaluated() {
        assert_eq!(num("#ppt_x"), Some(0.5));
        assert_eq!(num("1+#ppt_h/2"), Some(1.05));
        assert_eq!(num("0-#ppt_w/2"), Some(-0.1));
        assert_eq!(num("#ppt_y+(1-$)*2"), Some(1.25));
        assert!((num("sin(pi*$)").unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(num("1+"), None);
        assert_eq!(num("#nope"), None);
    }

    /// Fly-in from the left: x goes from `0-#ppt_w/2` to `#ppt_x`, linearly.
    #[test]
    fn a_fly_in_moves_along_its_values() {
        let x = AnimBehavior {
            kind: BehaviorKind::Anim { attr: "ppt_x".into(), keys: vec![(0, "0-#ppt_w/2".into()), (100_000, "#ppt_x".into())], formula: None, discrete: false },
            begin: 0,
            dur: 500,
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
        };
        let v = sample(&[y], 500.0, B).y.unwrap();
        assert!((v - 0.15).abs() < 1e-9);
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
        assert!(reveal_rects("dissolve", true, 0.5, 1.0).is_none());
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

    #[test]
    fn the_catalogue_builds_an_entrance_and_an_exit() {
        let entrance = build_filter_preset(AnimationClass::Entrance, 3, 5, 500).unwrap();
        assert!(matches!(&entrance[1].kind, BehaviorKind::Effect { filter, enter: true } if filter == "blinds(vertical)"));
        let exit = build_filter_preset(AnimationClass::Exit, 16, 21, 500).unwrap();
        assert!(matches!(&exit[0].kind, BehaviorKind::Effect { filter, enter: false } if filter == "barn(inVertical)"));
        assert_eq!(exit[1].begin, 499);
        assert!(build_filter_preset(AnimationClass::Emphasis, 3, 5, 500).is_none());
    }
}
