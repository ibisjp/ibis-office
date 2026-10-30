// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pptx defects the corpus comparison found (PowerPoint for the web's
//! PDF as the reference) that show only when drawn: each smallest file of
//! `test/corpus-report/pptx/` exported as PNG (`export.rs`, the pictures
//! `--export png` writes, in `target/scenario/corpus_report/<test>/`), and
//! its pixels checked against what PowerPoint shows. Colours are read as
//! the mean of a few pixels around a point, so anti-aliasing does not
//! decide a check.
//!
//! TODO: このファイルは、実文書の比較（コーパス）から見つかった崩れの試験の
//! 仮置き場で、ずっと残すものではない。試験が通るようになったら（崩れを直した
//! ら）、その試験を本番の試験に移すか、試験として要らなければ消し、最小文書
//! （`test/corpus-report/pptx/`）も一緒に移すか消す。
//! 本番の移し先は `crates/ibis-office-app/src/pptx/scenarios.rs`。
//! 崩れが残っている試験（赤）はここに置いたままにする。

use std::path::PathBuf;

use ibis_doc_model::{EmuRect, Presentation};

use crate::export::{Format, Options, export};

/// Pixels across a 16:9 slide (2 pixels a point); a 4:3 slide is as wide.
const WIDTH: u32 = 1920;

/// A slide drawn: its picture and how many pixels an EMU is.
struct Page {
    image: image::RgbaImage,
    per_emu: f64,
}

type Rgb = [i32; 3];

impl Page {
    fn px(&self, emu: i64) -> i64 {
        (emu as f64 * self.per_emu).round() as i64
    }

    fn pixel(&self, x: i64, y: i64) -> Option<Rgb> {
        (x >= 0 && y >= 0 && (x as u32) < self.image.width() && (y as u32) < self.image.height()).then(|| {
            let p = self.image.get_pixel(x as u32, y as u32).0;
            [p[0] as i32, p[1] as i32, p[2] as i32]
        })
    }

    /// The mean colour of the 5 × 5 pixels around (`x`, `y`) EMU.
    fn at(&self, x: i64, y: i64) -> Rgb {
        let (cx, cy) = (self.px(x), self.px(y));
        let mut sum = [0; 3];
        let mut n = 0;
        for dy in -2..=2 {
            for dx in -2..=2 {
                if let Some(p) = self.pixel(cx + dx, cy + dy) {
                    (0..3).for_each(|i| sum[i] += p[i]);
                    n += 1;
                }
            }
        }
        sum.map(|s| s / n.max(1))
    }

    /// The point at fractions (`fx`, `fy`) of `frame`.
    fn in_frame(&self, frame: EmuRect, fx: f64, fy: f64) -> Rgb {
        self.at(frame.x + (frame.width as f64 * fx) as i64, frame.y + (frame.height as f64 * fy) as i64)
    }

    /// The pixels of `area` (EMU), clipped to the picture, with their
    /// positions in pixels.
    fn pixels(&self, area: EmuRect) -> Vec<(i64, i64, Rgb)> {
        let (x0, y0) = (self.px(area.x).max(0), self.px(area.y).max(0));
        let (x1, y1) = (self.px(area.x + area.width).min(self.image.width() as i64), self.px(area.y + area.height).min(self.image.height() as i64));
        (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter_map(|(x, y)| self.pixel(x, y).map(|p| (x, y, p))).collect()
    }

    fn pt(&self, points: f64) -> f64 {
        points * 12_700.0 * self.per_emu
    }
}

fn rect(x: i64, y: i64, width: i64, height: i64) -> EmuRect {
    EmuRect::new(x, y, width, height)
}

fn near(c: Rgb, want: Rgb, tolerance: i32) -> bool {
    (0..3).all(|i| (c[i] - want[i]).abs() <= tolerance)
}

fn spread(c: Rgb) -> i32 {
    c.iter().max().unwrap() - c.iter().min().unwrap()
}

/// The file's slides drawn as `--export png` draws them (`slides` 1-based,
/// all when empty), into `target/scenario/corpus_report/<label>/`.
fn render_slides(file: &str, label: &str, slides: &[usize]) -> Vec<Page> {
    render_slides_at(file, label, slides, WIDTH)
}

/// `render_slides`, `width` pixels across (960 draws a slide of 720pt at 96 dpi: a picture of 96 dpi shows at its own
/// pixels).
fn render_slides_at(file: &str, label: &str, slides: &[usize], width: u32) -> Vec<Page> {
    let path = crate::shell::scenario::repo_root().join("test/corpus-report/pptx").join(file);
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let presentation: Presentation = ibis_pptx::read(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let out_dir: PathBuf = crate::shell::scenario::repo_root().join("target/scenario/corpus_report").join(label);
    let _ = std::fs::remove_dir_all(&out_dir);
    std::fs::create_dir_all(&out_dir).unwrap();
    let sizes: Vec<i64> = presentation.slides.iter().map(|s| presentation.size_of(s).width).collect();
    // The command line's defaults (`--export png` without options).
    let options = Options {
        format: Format::Png,
        out_dir: out_dir.clone(),
        name: "slide".into(),
        width,
        slides: slides.to_vec(),
        layout: false,
        file_name: Some(file.into()),
        print_backgrounds: false,
        print_hidden_text: false,
        print_markup: true,
        color: Default::default(),
        handout: None,
        frame: false,
        handout_landscape: false,
        notes_pages: false,
        outline_pages: false,
        notes_source: None,
        handout_marks: Default::default(),
    };
    let result = export(presentation, &options);
    // The export draws without font smoothing; the window's scenarios after
    // this one draw with it.
    gpui_macos::set_font_smoothing(true);
    result.unwrap_or_else(|e| panic!("{file}: {e}"));
    let mut files: Vec<PathBuf> = std::fs::read_dir(&out_dir).unwrap().filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "png")).collect();
    files.sort();
    files
        .iter()
        .map(|f| {
            let number: usize = f.file_stem().unwrap().to_string_lossy().rsplit('-').next().unwrap().parse().unwrap();
            let image = image::open(f).unwrap_or_else(|e| panic!("{}: {e}", f.display())).to_rgba8();
            let per_emu = image.width() as f64 / sizes[number - 1] as f64;
            Page { image, per_emu }
        })
        .collect()
}

fn render(file: &str, label: &str) -> Vec<Page> {
    render_slides(file, label, &[])
}

/// Failed checks, reported together.
#[derive(Default)]
struct Checks(Vec<String>);

impl Checks {
    fn check(&mut self, ok: bool, what: String) {
        if !ok {
            self.0.push(what);
        }
    }

    fn done(self, defect: &str) {
        assert!(self.0.is_empty(), "{defect}:\n  {}", self.0.join("\n  "));
    }
}

/// The number of lines of text in `area`: bands of rows holding text ink
/// (`ink`), at least `min_height` pixels high, apart by a row without ink.
fn lines(page: &Page, area: EmuRect, ink: impl Fn(Rgb) -> bool, min_height: f64) -> usize {
    let mut rows = std::collections::BTreeSet::new();
    for (_, y, c) in page.pixels(area) {
        if ink(c) {
            rows.insert(y);
        }
    }
    let mut bands: Vec<(i64, i64)> = Vec::new();
    for y in rows {
        match bands.last_mut() {
            Some((_, end)) if *end + 1 == y => *end = y,
            _ => bands.push((y, y)),
        }
    }
    bands.iter().filter(|(a, b)| (b - a + 1) as f64 >= min_height).count()
}

fn dark(c: Rgb) -> bool {
    c.iter().all(|v| *v < 90)
}

/// P1b: 文字の細い輪郭線（`a:rPr/a:ln`）を、字形をずらした重ね塗りで塗りの上に描くため、
/// 塗りが輪郭線の色で隠れる（明るい黄色が暗い黄土色になる）。
/// 上の行（0.5pt の暗い輪郭線）も下の行（輪郭線なし）も、字の内側は同じ明るい塗りの色。
/// 塗りの色は下の行で地の色（枠の隅）から離れた画素のうちいちばん多い色とし、その色（±24）の画素が上の行にも
/// 下の行の 4 割以上あること（上の行のほうが字数が多い。輪郭線は 0.5pt で縁だけ）。
pub(crate) fn corpus_report_pptx_p1b_thin_text_outline_keeps_fill() {
    let pages = render("text-thin-outline.pptx", "p1b");
    let page = &pages[0];
    let outlined = rect(457_200, 1_500_000, 8_229_600, 900_000);
    let plain = rect(457_200, 3_500_000, 8_229_600, 900_000);
    let ground = page.at(plain.x + 12_700, plain.y + 12_700);
    let mut counts = std::collections::HashMap::<Rgb, usize>::new();
    for (_, _, c) in page.pixels(plain) {
        if !near(c, ground, 60) {
            *counts.entry(c.map(|v| v / 8 * 8)).or_default() += 1;
        }
    }
    let fill = counts.into_iter().max_by_key(|(_, n)| *n).map(|(c, _)| c.map(|v| v + 4)).expect("the plain line is drawn");
    let count = |area| page.pixels(area).into_iter().filter(|(_, _, c)| near(*c, fill, 24)).count();
    let (with_outline, without) = (count(outlined), count(plain));
    assert!(
        with_outline as f64 >= 0.4 * without as f64,
        "the outlined line's insides keep the fill {fill:?}: {with_outline} pixels of it, the plain line {without}"
    );
}

/// P2a: 背景・図形の画像の塗りで、透明度・明るさ・二値化・伸縮の範囲（`a:blip` の子要素、
/// `srcRect`、`fillRect`）がすべて捨てられ、元の写真がそのまま全面に描かれる。
/// 写真（photo.png）の上半分は青 (30,90,200)。後ろの黒い帯 (y 1905000〜2667000) に
/// 重なる所で、透明度 30% の塗りは黒と混ざって青の 3 割（±25）、タイルも 3 割の濃さ以下。
/// fillRect l=r=25% の塗りは左右 1/4 が空く（白い地）。背景の fillRect b=9% は下の
/// 9% が白、alphaModFix 14% の背景（青 0,60,200）は白に近い淡い色。
pub(crate) fn corpus_report_pptx_p2a_picture_fill_effects() {
    let mut checks = Checks::default();
    let pages = render("picture-effects.pptx", "p2a-picture-effects");
    let page = &pages[1];
    let (top, y_bar) = (1_143_000, 2_100_000);
    let frame = |x: i64| rect(x, top, 2_286_000, 2_286_000);
    let plain = page.at(508_000 + 1_143_000, y_bar);
    checks.check(near(plain, [30, 90, 200], 25), format!("control: the plain picture fill shows its blue {plain:?}"));
    let faded = page.at(2_921_000 + 1_143_000, y_bar);
    let want = plain.map(|v| v * 3 / 10);
    checks.check(near(faded, want, 25), format!("alphaModFix 30% over the black bar: {want:?} (±25), drawn {faded:?}"));
    let tiled = page.pixels(rect(7_747_000, 1_955_000, 2_286_000, 330_000));
    let brightest = tiled.iter().map(|(_, _, c)| *c.iter().max().unwrap()).max().unwrap_or(0);
    checks.check(brightest <= 100, format!("the 30% tiles over the black bar are dark (no channel above 100): brightest {brightest}"));
    let side = page.in_frame(frame(10_160_000), 0.1, 0.15);
    checks.check(near(side, [255, 255, 255], 20), format!("fillRect l=r=25% leaves the left quarter empty (white): {side:?}"));
    let bg = render("bg-fillrect.pptx", "p2a-bg-fillrect");
    let bottom = bg[0].at(12_192_000 / 2, 6_858_000 * 955 / 1000);
    checks.check(near(bottom, [255, 255, 255], 20), format!("background fillRect b=9%: the bottom 9% is white, drawn {bottom:?}"));
    let faint = render("p3d-alphamodfix.pptx", "p2a-alphamodfix");
    let corner = faint[0].at(12_192_000 * 9 / 10, 6_858_000 * 9 / 10);
    let want = [255 - (255 - 0) * 14 / 100, 255 - (255 - 60) * 14 / 100, 255 - (255 - 200) * 14 / 100];
    checks.check(near(corner, want, 20), format!("background alphaModFix 14%: {want:?} (±20), drawn {corner:?}"));
    checks.done("P2a picture fills");
}

/// P2b: 図（`p:pic`）の `a:alphaModFix` が無視され、半透明の図が不透明で濃く描かれる。
/// picture-effects 1 枚目の 2 つ目（30%）は後ろの黒い帯と混ざって青の 3 割（±25）。
/// p3d の左の赤い図（200,0,0、30%）は、その左に見える背景の色と 3:7 で混ざる（±25。
/// 背景そのものの透明度は P2a で見る）。
pub(crate) fn corpus_report_pptx_p2b_picture_alpha() {
    let mut checks = Checks::default();
    let pages = render_slides("picture-effects.pptx", "p2b", &[1]);
    let page = &pages[0];
    let plain = page.at(508_000 + 1_143_000, 2_100_000);
    checks.check(near(plain, [30, 90, 200], 25), format!("control: the plain picture is blue {plain:?}"));
    let faded = page.at(2_921_000 + 1_143_000, 2_100_000);
    let want = plain.map(|v| v * 3 / 10);
    checks.check(near(faded, want, 25), format!("alphaModFix 30% over the black bar: {want:?} (±25), drawn {faded:?}"));
    let p3d = render("p3d-alphamodfix.pptx", "p2b-p3d");
    let ground = p3d[0].at(228_600, 1_500_000 + 1_828_800);
    let left = p3d[0].in_frame(rect(457_200, 1_500_000, 3_657_600, 3_657_600), 0.5, 0.5);
    let want = [0, 1, 2].map(|i| ([200, 0, 0][i] * 3 + ground[i] * 7) / 10);
    checks.check(near(left, want, 25), format!("the 30% red picture over the ground {ground:?}: {want:?} (±25), drawn {left:?}"));
    let right = p3d[0].in_frame(rect(4_800_000, 1_500_000, 3_657_600, 3_657_600), 0.5, 0.5);
    checks.check(near(right, [200, 0, 0], 25), format!("control: the opaque red picture {right:?}"));
    checks.done("P2b picture alphaModFix");
}

/// P2c: 図の `a:grayscl`（白黒化）が無視され、カラーのまま描かれる。
/// 写真の青 (30,90,200) の所が灰色（3 つの値の差が 12 以下）。
pub(crate) fn corpus_report_pptx_p2c_picture_grayscale() {
    let pages = render_slides("picture-effects.pptx", "p2c", &[1]);
    let grey = pages[0].in_frame(rect(5_334_000, 1_143_000, 2_286_000, 2_286_000), 0.5, 0.15);
    assert!(spread(grey) <= 12, "grayscl: the blue part is grey, drawn {grey:?}");
}

/// P2d: 図の `a:biLevel`（白黒の二値化）が無視され、元の色のまま描かれる。
/// しきい値 50%：写真の青（輝度 約 84）は黒、オレンジ（約 159）は白。
/// 灰色の階調（grey.png）は左が黒、右が白で、途中の灰色が無い（黒と白の境目に掛かる 1 点は除く）。
pub(crate) fn corpus_report_pptx_p2d_picture_bilevel() {
    let mut checks = Checks::default();
    let pages = render_slides("picture-effects.pptx", "p2d", &[1]);
    let page = &pages[0];
    let photo = rect(7_747_000, 1_143_000, 2_286_000, 2_286_000);
    let blue = page.in_frame(photo, 0.5, 0.15);
    checks.check(blue.iter().all(|v| *v <= 40), format!("the blue part (dark) is black, drawn {blue:?}"));
    let orange = page.in_frame(photo, 0.25, 0.85);
    checks.check(orange.iter().all(|v| *v >= 215), format!("the orange part (light) is white, drawn {orange:?}"));
    let ramp = rect(5_334_000, 4_318_000, 2_286_000, 1_524_000);
    let greys: Vec<Rgb> = (1..20).map(|i| page.in_frame(ramp, i as f64 / 20.0, 0.5)).collect();
    let black = |c: &Rgb| c.iter().all(|v| *v <= 40);
    let white = |c: &Rgb| c.iter().all(|v| *v >= 215);
    // A point on the edge between the black and the white half, between a
    // black and a white point, takes both in its 5 × 5 pixels.
    let edge = |i: usize| i > 0 && i + 1 < greys.len() && ((black(&greys[i - 1]) && white(&greys[i + 1])) || (white(&greys[i - 1]) && black(&greys[i + 1])));
    let middling: Vec<&Rgb> = greys.iter().enumerate().filter(|(i, c)| !black(c) && !white(c) && !edge(*i)).map(|(_, c)| c).collect();
    checks.check(middling.is_empty(), format!("the grey ramp is only black and white: {} of 19 points grey, {middling:?}", middling.len()));
    checks.done("P2d biLevel");
}

/// P2e: 図の `a:lum` のコントラストが無視され、明るさも白か黒の膜を枠全体に重ねる近似に
/// なっている。contrast +40% は色を中間の灰色から遠ざけるので、青 (30,90,200) の
/// 3 つの値の差（170）は、何も指定しない図より 25 以上広がる。
pub(crate) fn corpus_report_pptx_p2e_picture_contrast() {
    let pages = render_slides("picture-effects.pptx", "p2e", &[1]);
    let plain = pages[0].in_frame(rect(508_000, 1_143_000, 2_286_000, 2_286_000), 0.5, 0.15);
    let contrast = pages[0].in_frame(rect(10_160_000, 1_143_000, 2_286_000, 2_286_000), 0.5, 0.15);
    assert!(spread(contrast) >= spread(plain) + 25, "contrast +40% spreads the colour: plain {plain:?}, contrast {contrast:?}");
}

/// P3a: ベクターだけの EMF（中にビットマップが無いもの）は何も描かれず、代わりの枠も出ない。
/// band.emf は紺 (10,30,100) の帯と白い四角 3 つ、image1.emf・image3.emf は青
/// (0,94,184) の箱と黄色い三角形。比べるための PNG（同じ青）は描かれる。
pub(crate) fn corpus_report_pptx_p3a_vector_emf_drawn() {
    let mut checks = Checks::default();
    let band = render("emf-vector-only.pptx", "p3a-band");
    let navy = band[0].at(12_192_000 * 9 / 10, 152_400);
    checks.check(near(navy, [10, 30, 100], 30), format!("the band EMF's navy at the slide's top right, drawn {navy:?}"));
    let pages = render("p3a-vector-emf.pptx", "p3a-boxes");
    let page = &pages[0];
    let png = page.in_frame(rect(4_800_000, 1_500_000, 3_657_600, 1_828_800), 0.1, 0.1);
    checks.check(near(png, [0, 94, 184], 30), format!("control: the PNG's blue {png:?}"));
    let slide_emf = page.in_frame(rect(457_200, 1_500_000, 3_657_600, 1_828_800), 0.1, 0.1);
    checks.check(near(slide_emf, [0, 94, 184], 30), format!("the slide's EMF box is blue, drawn {slide_emf:?}"));
    let layout_emf = page.in_frame(rect(9_500_000, 300_000, 2_286_000, 1_143_000), 0.1, 0.1);
    checks.check(near(layout_emf, [0, 94, 184], 30), format!("the layout's EMF box is blue, drawn {layout_emf:?}"));
    checks.done("P3a vector EMF");
}

/// P3b: WMF の画像は描かれず、「WMF 画像（未対応）」の灰色の枠になる。
/// image1.wmf は緑 (0,160,60) の矩形を枠いっぱいに描く。
pub(crate) fn corpus_report_pptx_p3b_wmf_drawn() {
    let pages = render("p3b-wmf.pptx", "p3b");
    let frame = rect(457_200, 1_500_000, 3_657_600, 1_828_800);
    let colours: Vec<Rgb> = [(0.2, 0.2), (0.5, 0.5), (0.8, 0.8)].iter().map(|(x, y)| pages[0].in_frame(frame, *x, *y)).collect();
    assert!(colours.iter().all(|c| near(*c, [0, 160, 60], 30)), "the WMF's green rectangle fills its frame, drawn {colours:?}");
}

/// P3c: ビットマップとベクターが混ざった EMF で、いちばん大きいビットマップだけを描き、
/// トリミングもそれに掛けるので別の絵が出る。
/// part.emf は水色 (160,216,240) の正方形の中央 50% に市松模様。image1.emf は上 80% に
/// 赤 (220,30,30)、82.5〜87.5% に黒い線、90〜100% に青 (30,60,220) のビットマップ。
pub(crate) fn corpus_report_pptx_p3c_mixed_emf_parts_in_place() {
    let mut checks = Checks::default();
    let part = render("emf-bitmap-part.pptx", "p3c-part");
    let corner = part[0].in_frame(rect(1_524_000, 1_016_000, 3_810_000, 3_810_000), 0.1, 0.1);
    checks.check(near(corner, [160, 216, 240], 30), format!("part.emf: the corner is the light blue square, drawn {corner:?}"));
    let pages = render("p3c-emf-mixed-crop.pptx", "p3c-crop");
    let page = &pages[0];
    let whole = rect(457_200, 1_300_000, 4_000_000, 2_000_000);
    let red = page.in_frame(whole, 0.5, 0.4);
    checks.check(near(red, [220, 30, 30], 30), format!("whole EMF at 40%: red, drawn {red:?}"));
    let line = page.in_frame(whole, 0.5, 0.85);
    checks.check(near(line, [0, 0, 0], 40), format!("whole EMF at 85%: the black line, drawn {line:?}"));
    let blue = page.in_frame(whole, 0.5, 0.95);
    checks.check(near(blue, [30, 60, 220], 30), format!("whole EMF at 95%: the blue bitmap, drawn {blue:?}"));
    let cropped = page.in_frame(rect(457_200, 4_200_000, 8_000_000, 400_000), 0.5, 0.5);
    checks.check(near(cropped, [30, 60, 220], 30), format!("srcRect t=90%: the blue bitmap, drawn {cropped:?}"));
    checks.done("P3c mixed EMF");
}

/// P4a: 非表示のスライド（`show="0"`）も PDF・PNG・印刷に出てしまい、PowerPoint の PDF より
/// 1 枚多くなる。3 枚のうち 2 枚目が非表示なので、PNG は 2 枚。
pub(crate) fn corpus_report_pptx_p4a_hidden_slide_not_exported() {
    let pages = render("p4a-hidden-middle-slide.pptx", "p4a");
    assert_eq!(pages.len(), 2, "the hidden slide 2 is left out of the export");
}

/// The picture at fractions of `frame`, `turned` 180°.
fn sample(page: &Page, frame: EmuRect, fx: f64, fy: f64, turned: bool) -> Rgb {
    if turned { page.in_frame(frame, 1.0 - fx, 1.0 - fy) } else { page.in_frame(frame, fx, fy) }
}

/// P5a: CMYK（YCCK）の JPEG に透明色（`a:clrChange`）を付けると、画像の右 1/4 が黒い四角に
/// なる（回転・反転・duotone でも崩れる）。PowerPoint では 3 つとも同じ絵：
/// 透明色の図は白くない所が加工なしの図と同じ色、180° 回した図は回した位置で同じ色
/// （各値 ±40。40 × 10 点のうち、加工なしの図で同じ色の部分の内側にある点を比べ、合わないのが 5% まで）。
fn same_logo_three_ways(file: &str, label: &str) {
    let pages = render(file, label);
    let page = &pages[0];
    let plain = rect(508_000, 1_016_000, 5_080_000, 1_270_000);
    let clear = rect(6_350_000, 1_016_000, 5_080_000, 1_270_000);
    let turned = rect(508_000, 3_302_000, 5_080_000, 1_270_000);
    let (mut clear_off, mut clear_n, mut turned_off, mut total) = (Vec::new(), 0, Vec::new(), 0);
    for j in 0..10 {
        for i in 0..40 {
            let (fx, fy) = ((i as f64 + 0.5) / 40.0, (j as f64 + 0.5) / 10.0);
            let want = sample(page, plain, fx, fy, false);
            // Inside a part of one colour, off the parts' edges.
            let uniform = [(-0.01, 0.0), (0.01, 0.0), (0.0, -0.03), (0.0, 0.03)].iter().all(|(dx, dy)| near(sample(page, plain, fx + dx, fy + dy, false), want, 20));
            if !uniform {
                continue;
            }
            total += 1;
            if want.iter().sum::<i32>() < 700 {
                clear_n += 1;
                let got = sample(page, clear, fx, fy, false);
                if !near(got, want, 40) {
                    clear_off.push((fx, got, want));
                }
            }
            let got = sample(page, turned, fx, fy, true);
            if !near(got, want, 40) {
                turned_off.push((fx, got, want));
            }
        }
    }
    let mut checks = Checks::default();
    let right = |off: &[(f64, Rgb, Rgb)]| off.iter().filter(|(fx, _, _)| *fx > 0.75).count();
    checks.check(clear_off.len() * 20 <= clear_n, format!("clrChange: {} of {clear_n} points differ ({} in the right quarter), e.g. {:?}", clear_off.len(), right(&clear_off), clear_off.first()));
    checks.check(turned_off.len() * 20 <= total, format!("turned 180°: {} of {total} points differ ({} in the logo's right quarter), e.g. {:?}", turned_off.len(), right(&turned_off), turned_off.first()));
    checks.done(&format!("P5a {file}"));
}

/// P5a: CMYK（YCCK）の JPEG に透明色を付けると右 1/4 が黒い四角になる：YCCK の JPEG。
pub(crate) fn corpus_report_pptx_p5a_ycck_jpeg_clear_colour_and_turn() {
    same_logo_three_ways("ycck-jpeg-clrchange.pptx", "p5a-ycck");
}

/// P5a: 同上：素の CMYK の JPEG（加工したものが縦縞に崩れる）。
pub(crate) fn corpus_report_pptx_p5a_cmyk_jpeg_clear_colour_and_turn() {
    same_logo_three_ways("cmyk-jpeg-svg-path.pptx", "p5a-cmyk");
}

/// The left and right ends (pixels) of the pixels of `area` that `which`
/// picks.
fn extent(page: &Page, area: EmuRect, which: impl Fn(Rgb) -> bool) -> Option<(i64, i64)> {
    let xs: Vec<i64> = page.pixels(area).into_iter().filter(|(_, _, c)| which(*c)).map(|(x, _, _)| x).collect();
    Some((*xs.iter().min()?, *xs.iter().max()?))
}

/// P6a: 文字の影（`a:rPr/a:effectLst/a:outerShdw`）を、均等割り付け（`algn="dist"`）や字間
/// （`spc`）を足さずに描くため、影が本体からずれて灰色の文字が二重に見える。
/// 影は各字の右下 3pt（45°）に付くので、マゼンタの字の左右の端から影の左右の端までの
/// ずれは、字間なしの行（対照）と同じ（±3pt）。
pub(crate) fn corpus_report_pptx_p6a_text_shadow_follows_spacing() {
    let pages = render("p6a-shadow-letter-spacing.pptx", "p6a");
    let page = &pages[0];
    let magenta = |c: Rgb| c[0] - c[1] > 100 && c[2] - c[1] > 100;
    let shadow = |c: Rgb| spread(c) <= 12 && c[0] <= 200;
    let ends = |y: i64| {
        let area = rect(0, y, 12_192_000, 1_000_000);
        let (body, shade) = (extent(page, area, magenta).expect("the text is drawn"), extent(page, area, shadow).expect("the shadow is drawn"));
        (shade.0 - body.0, shade.1 - body.1)
    };
    let control = ends(600_000);
    let tolerance = page.pt(3.0) as i64;
    let mut checks = Checks::default();
    for (name, y) in [("algn=dist", 2_200_000), ("spc=800", 3_800_000), ("spc=-150", 5_400_000)] {
        let off = ends(y);
        checks.check(
            (off.0 - control.0).abs() <= tolerance && (off.1 - control.1).abs() <= tolerance,
            format!("{name}: the shadow's ends from the text's (left, right) {off:?} px, as without spacing {control:?} (±{tolerance})"),
        );
    }
    checks.done("P6a text shadow");
}

/// P6c: 透明な部分のある PNG の図に付いた外側の影（`p:pic` の `a:outerShdw`）を、図の形でなく
/// 枠の四角で描くため、正解に無い灰色の四角が出る。PNG の不透明な所は中央
/// （横 15〜85%、縦 25〜75%）だけなので、枠の隅や端の近くは地の色 #DDEEE8 のまま（±8）。
pub(crate) fn corpus_report_pptx_p6c_picture_shadow_follows_transparency() {
    let pages = render("p6c-picture-shadow-transparent.pptx", "p6c");
    let frame = rect(1_000_000, 1_200_000, 8_000_000, 4_000_000);
    let points: Vec<((f64, f64), Rgb)> = [(0.05, 0.05), (0.95, 0.5), (0.5, 0.92), (0.97, 0.97)].iter().map(|p| (*p, pages[0].in_frame(frame, p.0, p.1))).collect();
    let grey: Vec<_> = points.iter().filter(|(_, c)| !near(*c, [0xdd, 0xee, 0xe8], 8)).collect();
    assert!(grey.is_empty(), "the transparent parts show the ground #DDEEE8, no shadow: {grey:?}");
}

/// The text ink of `area`, as positions from its corner.
fn ink_mask(page: &Page, area: EmuRect, ink: impl Fn(Rgb) -> bool) -> std::collections::HashSet<(i64, i64)> {
    let (x0, y0) = (page.px(area.x), page.px(area.y));
    page.pixels(area).into_iter().filter(|(_, _, c)| ink(*c)).map(|(x, y, _)| (x - x0, y - y0)).collect()
}

/// P6d: ワードアートの文字の変形（`a:bodyPr/a:prstTxWarp`）を読み込んで保存はするが、描画では
/// 無視するため、アーチ状の題名がまっすぐで大きいまま描かれる。左（textArchUp）は弧に
/// 沿うので、同じ文字の右（変形なし）と字の形が重ならない（重なりが和の 6 割未満）。
pub(crate) fn corpus_report_pptx_p6d_text_warp_drawn() {
    let pages = render("p6d-wordart-arch.pptx", "p6d");
    let navy = |c: Rgb| c[0] < 120 && c[2] > c[0] + 30;
    let left = ink_mask(&pages[0], rect(300_000, 600_000, 5_600_000, 3_300_000), navy);
    let right = ink_mask(&pages[0], rect(6_300_000, 600_000, 5_600_000, 3_300_000), navy);
    assert!(!right.is_empty(), "control: the plain text is drawn");
    let both = left.intersection(&right).count() as f64;
    let either = left.union(&right).count() as f64;
    assert!(both / either < 0.6, "the arched text is not the plain one: {:.0}% of their ink overlaps", both / either * 100.0);
}

/// P6e: 文字の光彩（`a:rPr/a:effectLst/a:glow`）を読まないため、文字の周りの色のにじみが出ない。
/// 光彩は半径 8pt・紫 #AA2BD4 の 60% なので、字の周りに紫がかった画素（赤と青が緑より
/// 40 以上大きい）が黒い字の画素の 4 分の 1 以上ある。光彩なしの行（対照）には無い。
pub(crate) fn corpus_report_pptx_p6e_text_glow_drawn() {
    let pages = render("p6e-text-glow.pptx", "p6e");
    let purple = |c: Rgb| c[0] - c[1] >= 40 && c[2] - c[1] >= 40;
    let black = |c: Rgb| c.iter().all(|v| *v < 80);
    let count = |y: i64, f: &dyn Fn(Rgb) -> bool| pages[0].pixels(rect(300_000, y, 11_000_000, 1_300_000)).into_iter().filter(|(_, _, c)| f(*c)).count();
    let (glow, ink) = (count(700_000, &purple), count(700_000, &black));
    assert!(ink > 0, "the text is drawn");
    assert!(glow * 4 >= ink, "a purple glow around the text: {glow} purple pixels, {ink} of the text");
}

/// P7b: 入っていない日本語の書体の代わり（Hiragino）を全角の幅で並べ、詰めた字幅（`palt`）も
/// 英数字の縮めも使わないため、行が長くなって折り返しが増える。PowerPoint では
/// HGP創英角ｺﾞｼｯｸUB の題名、ＭＳ ゴシックの出典、ＭＳ Ｐゴシックの本文、Meiryo UI の注記の
/// どれも 1 行。
pub(crate) fn corpus_report_pptx_p7b_japanese_substitute_width() {
    let pages = render("P7b-ja-substitute-width.pptx", "p7b");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let mut checks = Checks::default();
    for (name, frame, size) in [
        ("HGP創英角ｺﾞｼｯｸUB 48pt", rect(457_200, 254_000, 6_172_200, 1_651_000), 48.0),
        ("ＭＳ ゴシック 12pt", rect(457_200, 2_730_500, 1_986_280, 279_400), 12.0),
        ("ＭＳ Ｐゴシック 10.5pt", rect(457_200, 3_302_000, 7_993_380, 508_000), 10.5),
        ("Meiryo UI 10pt", rect(457_200, 4_064_000, 7_802_880, 508_000), 10.0),
    ] {
        let area = rect(frame.x + pt(2.0), frame.y - pt(10.0), frame.width - pt(4.0), frame.height + pt(20.0));
        let n = lines(page, area, dark, page.pt(size * 0.3));
        checks.check(n == 1, format!("{name}: {n} lines"));
    }
    checks.done("P7b Japanese substitutes");
}

/// P7c: 入っていない欧文の書体（Franklin Gothic Book）の代わりを SF（`.SystemUIFont`）で描くため
/// 行が 13% 長くなる。PowerPoint では約 532pt で、内側 575pt の枠に 1 行で入る。
pub(crate) fn corpus_report_pptx_p7c_latin_substitute_width() {
    let pages = render("P7c-latin-substitute-width.pptx", "p7c");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let count = |y: i64| lines(page, rect(457_200 + pt(2.0), y, 7_485_380 - pt(4.0), 508_000 + pt(20.0)), dark, page.pt(11.0 * 0.3));
    let arial = count(1_778_000);
    assert_eq!(arial, 1, "control: the Arial line is one line");
    let franklin = count(762_000);
    assert_eq!(franklin, 1, "the Franklin Gothic Book line is one line");
}

/// P8d: 円グラフの「自動調整」（`bestFit`）のラベルを、いつも半径の 65% の内側に 1 行で置く
/// ため、小さな扇のラベルが重なる。PowerPoint では小さな扇（6%・4%）のラベルは円の外：
/// 円（扇の色の画素の面積から出した半径）より外に、ラベルの文字の画素がある。
pub(crate) fn corpus_report_pptx_p8d_pie_best_fit_labels_outside_small_slices() {
    let pages = render("p8c-pie-bestfit-labels.pptx", "p8d");
    let page = &pages[0];
    let slices: [Rgb; 5] = [[0x44, 0x72, 0xc4], [0xed, 0x7d, 0x31], [0xa5, 0xa5, 0xa5], [0xff, 0xc0, 0x00], [0x5b, 0x9b, 0xd5]];
    let frame = rect(200_000, 900_000, 4_300_000, 4_300_000);
    let pixels = page.pixels(frame);
    let pie: Vec<(i64, i64)> = pixels.iter().filter(|(_, _, c)| slices.iter().any(|s| near(*c, *s, 12))).map(|(x, y, _)| (*x, *y)).collect();
    assert!(!pie.is_empty(), "the pie is drawn");
    let n = pie.len() as f64;
    let (cx, cy) = (pie.iter().map(|p| p.0 as f64).sum::<f64>() / n, pie.iter().map(|p| p.1 as f64).sum::<f64>() / n);
    let radius = (n / std::f64::consts::PI).sqrt();
    let text = |c: Rgb| c.iter().all(|v| *v < 110);
    let outside = pixels.iter().filter(|(x, y, c)| text(*c) && ((*x as f64 - cx).powi(2) + (*y as f64 - cy).powi(2)).sqrt() > radius + page.pt(2.0)).count();
    assert!(outside >= 30, "the small slices' labels are outside the pie (radius {radius:.0} px): {outside} text pixels outside");
}

/// P8e: 描画域が自動のグラフで、最後の既定に落ちると題名と軸の目盛りの文字の幅を見ないため、
/// 円グラフが題名に近づき、横棒グラフの項目名の左が切れる。
/// 円グラフ：題名（18pt）の字の行のまとまりは 18pt の 1.5 倍以下で、その下に 4pt 以上の
/// 空きがあってから、ラベルか円が始まる（重ならない）。
/// 横棒グラフ：項目名は枠の内側に収まり、枠の左端から 1pt 以内に字が無い。
pub(crate) fn corpus_report_pptx_p8e_automatic_plot_area_leaves_room() {
    let pages = render("p8d-chart-automatic-plot-area.pptx", "p8e");
    let page = &pages[0];
    let mut checks = Checks::default();
    let pie = rect(200_000, 900_000, 4_000_000, 4_000_000);
    // From 3pt below the frame's top, past an outline of the chart.
    let column = rect(pie.x + pie.width * 35 / 100, pie.y + 38_100, pie.width * 30 / 100, pie.height / 2);
    let mut rows: Vec<(i64, bool)> = Vec::new();
    for (_, y, c) in page.pixels(column) {
        let used = c.iter().any(|v| *v < 200);
        match rows.last_mut() {
            Some((row, any)) if *row == y => *any |= used,
            _ => rows.push((y, used)),
        }
    }
    let title_start = rows.iter().position(|(_, used)| *used);
    let title_end = title_start.and_then(|s| rows[s..].iter().position(|(_, used)| !*used).map(|e| s + e));
    let next = title_end.and_then(|e| rows[e..].iter().position(|(_, used)| *used).map(|n| e + n));
    match (title_start, title_end, next) {
        (Some(s), Some(e), Some(n)) => {
            let (height, gap) = ((e - s) as f64, (n - e) as f64);
            checks.check(height <= page.pt(18.0 * 1.5), format!("pie: the title's band is {height} px high, the title alone is at most {:.0}", page.pt(27.0)));
            checks.check(gap >= page.pt(4.0), format!("pie: {gap} px between the title and the plot, at least {:.0} (4pt)", page.pt(4.0)));
        }
        other => checks.check(false, format!("pie: no title then plot found {other:?}")),
    }
    let bars = rect(4_400_000, 900_000, 4_600_000, 4_000_000);
    let edge = rect(bars.x - 254_000, bars.y, 254_000 + 12_700, bars.height);
    let cut = page.pixels(edge).into_iter().filter(|(_, _, c)| dark(*c)).count();
    checks.check(cut == 0, format!("bars: the category names stay inside the frame, {cut} text pixels at or past its left edge"));
    checks.done("P8e automatic plot area");
}

/// P8c: `vert270` の縦書きで、スライドの端にかかる行の字が、回す前の位置で描画範囲の外と
/// 判定されて 1 字ずつ丸ごと消える。右端にぴったりの箱でも、はみ出すのは 2 行目の
/// 字の縁だけなので、黄色の字の画素はスライドの中の同じ箱の 8 割以上ある。
pub(crate) fn corpus_report_pptx_p8c_sideways_text_at_slide_edge() {
    let pages = render("p8e-sideways-text-overflow.pptx", "p8c");
    let yellow = |c: Rgb| c[0] > 200 && c[1] > 200 && c[2] < 100;
    let count = |x: i64, y: i64| pages[0].pixels(rect(x - 254_000, y, 574_862 + 508_000, 2_400_000)).into_iter().filter(|(_, _, c)| yellow(*c)).count();
    let (inside, edge) = (count(1_500_000, 1_200_000), count(8_569_138, 4_000_000));
    assert!(inside > 0, "control: the text inside the slide is drawn");
    assert!(edge as f64 >= 0.8 * inside as f64, "the text at the slide's edge: {edge} yellow pixels, inside the slide {inside}");
}

/// The area (EMU) of the pixels `x0..x1` × `y0..y1`.
fn area_px(page: &Page, x0: i64, y0: i64, x1: i64, y1: i64) -> EmuRect {
    let emu = |px: i64| (px as f64 / page.per_emu).round() as i64;
    rect(emu(x0), emu(y0), emu(x1 - x0), emu(y1 - y0))
}

/// The bands of rows (first and last pixel row) holding ink in `area`, each at least `min_height` pixels high:
/// the lines of text, top to bottom.
fn ink_bands(page: &Page, area: EmuRect, ink: impl Fn(Rgb) -> bool, min_height: f64) -> Vec<(i64, i64)> {
    let rows: std::collections::BTreeSet<i64> = page.pixels(area).into_iter().filter(|(_, _, c)| ink(*c)).map(|(_, y, _)| y).collect();
    let mut bands: Vec<(i64, i64)> = Vec::new();
    for y in rows {
        match bands.last_mut() {
            Some((_, end)) if *end + 1 == y => *end = y,
            _ => bands.push((y, y)),
        }
    }
    bands.into_iter().filter(|(a, b)| (b - a + 1) as f64 >= min_height).collect()
}

/// The runs of columns (first and last pixel column) holding ink in `area`, left to right: the glyphs, or the
/// glyphs that touch.
fn ink_runs(page: &Page, area: EmuRect, ink: impl Fn(Rgb) -> bool) -> Vec<(i64, i64)> {
    let columns: std::collections::BTreeSet<i64> = page.pixels(area).into_iter().filter(|(_, _, c)| ink(*c)).map(|(x, _, _)| x).collect();
    let mut runs: Vec<(i64, i64)> = Vec::new();
    for x in columns {
        match runs.last_mut() {
            Some((_, end)) if *end + 1 == x => *end = x,
            _ => runs.push((x, x)),
        }
    }
    runs
}

/// How much ink `area` holds, in pixels of full black (the mean darkness of the pixels × their number).
fn ink_mass(page: &Page, area: EmuRect) -> f64 {
    page.pixels(area).into_iter().map(|(_, _, c)| (255.0 - (c[0] + c[1] + c[2]) as f64 / 3.0) / 255.0).sum()
}

/// The share of the pixel columns `x0..x1` where no pixel of the rows `y0..=y1` is `ink`: the gaps of a dashed
/// line (a solid one has none).
fn gap_share(page: &Page, (x0, x1): (i64, i64), (y0, y1): (i64, i64), ink: impl Fn(Rgb) -> bool) -> f64 {
    let gaps = (x0..x1).filter(|x| !(y0..=y1).any(|y| page.pixel(*x, y).is_some_and(&ink))).count();
    gaps as f64 / (x1 - x0).max(1) as f64
}

fn red(c: Rgb) -> bool {
    c[0] > 200 && c[1] < 80 && c[2] < 80
}

/// Pixels of any colour but near-white, as the text of a glyph with thin strokes.
fn inked(c: Rgb) -> bool {
    c.iter().all(|v| *v < 200)
}

/// reg-pptx-b1（後退。92341f7c1 では正しかった。原因の見当 3cc3ed93c。todo-pptx P7b の副作用）：
/// 入っていない比例の日本語の書体（Meiryo UI）に palt を付けると、全角の開き括弧「（」だけが字詰めの位置
/// （左へ約 0.5 字）に描かれて送りは縮まないため、半角の数字が続くと括弧の後ろに約 1 字の隙間ができる。
/// 24pt の「（1）…」「（2）…」「（3）…」で、括弧の字と数字の間は、書体が Meiryo（比例でないので palt なし。
/// 同じスライドの下の箱、対照）で約 9px（0.2 字）、Meiryo UI で約 33px（0.7 字）。どちらの箱も 0.4 字以下。
pub(crate) fn corpus_report_pptx_reg_palt_bracket_gap() {
    let pages = render("reg-palt-bracket-gap.pptx", "reg-palt-bracket-gap");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let em = page.pt(24.0);
    let mut checks = Checks::default();
    for (name, top) in [("Meiryo UI", 40.0), ("Meiryo (control)", 280.0)] {
        let bands = ink_bands(page, rect(pt(40.0), pt(top), pt(300.0), pt(200.0)), inked, em * 0.3);
        checks.check(bands.len() >= 3, format!("{name}: {} lines of text found, at least 3", bands.len()));
        for (n, (y0, y1)) in bands.iter().take(3).enumerate() {
            let runs = ink_runs(page, area_px(page, page.px(pt(40.0)), *y0, page.px(pt(340.0)), y1 + 1), inked);
            let Some(gap) = runs.get(1).map(|second| (second.0 - runs[0].1 - 1) as f64) else {
                checks.check(false, format!("{name} line {}: fewer than 2 glyphs found", n + 1));
                continue;
            };
            checks.check(gap <= em * 0.4, format!("{name} line {}: {gap} px between （ and the digit, at most {:.0} (0.4 em)", n + 1, em * 0.4));
        }
    }
    checks.done("reg-pptx-b1: the gap after a full-width opening bracket");
}

/// reg-pptx-b2（後退。92341f7c1 では正しかった。原因の見当 323727f6e。todo-pptx P7c の副作用）：
/// 入っていない欧文の書体の代わりを Arial にしたため、26pt の本文では代わりが .SystemUIFont だった旧ビルドより字幅が
/// 約 2〜4% 広く、行が増える。4:3 のスライド（720pt 幅。16:9 では .SystemUIFont の字幅の変わり方で向きが逆になる）、
/// 幅 320pt の箱、26pt、kern=1200。ユーザーの非公開の実文書では、1 行目の語句が PowerPoint でも旧ビルドでも 1 行に収まって
/// 2 行になる段落が、Arial では 3 行になった。同じ型のダミー文（1 行目の語句が Arial で 866px、箱の内側が 853px）の
/// 3 つの段落は、旧ビルドの幅（Arial の約 96%）では 2 + 2 + 2 の 6 行、Arial の幅では 3 + 2 + 2 の 7 行。
pub(crate) fn corpus_report_pptx_reg_latin_fallback_width() {
    let pages = render("reg-latin-fallback-width.pptx", "reg-latin-fallback-width");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let n = lines(page, rect(pt(10.0), pt(10.0), pt(340.0), pt(310.0)), dark, page.pt(26.0 * 0.3));
    assert!(n <= 6, "a missing Latin font drawn as wide as Arial: {n} lines in the 3 paragraphs, at most 6 as with the narrower stand-in");
}

/// pptx-C001-b1（pptx-C005-b2 も同じきっかけ。todo に項目なし）：段落の前後の間隔 `a:spcBef`・`a:spcAft` の割合
/// （`a:spcPct`）を、文字の大きさの割合で計算する。PowerPoint は行の高さ（文字の大きさ × 1.2）の割合。
/// Arial 18pt の 5 段落の行の頭の間隔（上の端どうし）は、spcBef・spcAft 100% で 21.6 + 21.6 = 43.2pt（ibisOffice は 39.6pt）、
/// spcBef 50%（段落でも lstStyle でも）で 21.6 + 10.8 = 32.4pt（同 30.6pt）。spcPts 21.6 の対照は両方 43.2pt。
pub(crate) fn corpus_report_pptx_spcpct_follows_line_height() {
    let pages = render("spcpct-line-basis.pptx", "spcpct-line-basis");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let mut checks = Checks::default();
    for (name, left, least, most) in [
        ("spcBef 100%", 20.0, 42.0, 44.5),
        ("spcBef 50%", 210.0, 31.6, 33.4),
        ("spcAft 100%", 400.0, 42.0, 44.5),
        ("lstStyle spcBef 50%", 590.0, 31.6, 33.4),
        ("spcPts 21.6 (control)", 780.0, 42.0, 44.5),
    ] {
        let bands = ink_bands(page, rect(pt(left + 3.0), pt(43.0), pt(164.0), pt(390.0)), dark, page.pt(18.0 * 0.3));
        if bands.len() != 5 {
            checks.check(false, format!("{name}: {} lines of text found, 5 expected", bands.len()));
            continue;
        }
        let pitch = (bands[4].0 - bands[0].0) as f64 / 4.0 / page.pt(1.0);
        checks.check((least..=most).contains(&pitch), format!("{name}: lines {pitch:.1}pt apart, between {least} and {most}"));
    }
    checks.done("pptx-C001-b1: percentage paragraph spacing");
}

/// pptx-C002-b1（todo に項目なし）：円グラフ形（`prst="pie"`）の図形に入れた文字の枠が、プリセットの定義
/// （`presetShapeDefinitions.xml` の pie の `<rect>`）で上と右に縦横の違う値が入っていて、幅も高さも 0 の 1 点になる。
/// 文字が 1 字ずつ縦に割れ、図形の左の端へ寄る。PowerPoint では、同じ形の「試験」（20pt 太字）が円の左右の中央に 1 行で出る。
/// 下半分の pie の 2 つ（日本語と ABCDE）は 1 行で、字の左右の端の中点が図形の左右の中央から 15%（30pt）以内。
/// 3/4 の pie（既定）は 1 行。ellipse の対照も 1 行・中央。
pub(crate) fn corpus_report_pptx_pie_text_rect() {
    let pages = render("pie-text-rect.pptx", "pie-text-rect");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let mut checks = Checks::default();
    for (name, left, centred) in [("A pie lower half, Japanese", 30.0, true), ("B pie lower half, ABCDE", 250.0, true), ("C ellipse (control)", 470.0, true), ("D pie 3/4", 690.0, false)] {
        let area = rect(pt(left), pt(130.0), pt(200.0), pt(240.0));
        let n = lines(page, area, dark, page.pt(20.0 * 0.3));
        checks.check(n == 1, format!("{name}: {n} lines of text, 1 expected"));
        if let (true, Some((l, r))) = (centred, extent(page, area, dark)) {
            let off = ((l + r) as f64 / 2.0 - page.px(pt(left + 100.0)) as f64).abs();
            checks.check(off <= page.pt(30.0), format!("{name}: the text's middle is {off:.0} px from the shape's middle, at most {:.0} (30pt)", page.pt(30.0)));
        }
    }
    checks.done("pptx-C002-b1: the text rectangle of a pie");
}

/// pptx-C004-b1（todo に項目なし）：名前に太さを含む書体（HGP創英角ｺﾞｼｯｸUB・HGPｺﾞｼｯｸE・HG創英角ｺﾞｼｯｸUB）が
/// 入っていないとき、`b="1"` が無ければ代わりの書体を通常の太さで描く（`WEIGHT_SUFFIXES` に UB・E・EB が無い）。
/// PowerPoint は書体の持つ太さで描く（太い）。32pt の同じ文の 3 行は、通常の太さのメイリオの行（4 行目）より
/// インクの量が 1.3 倍以上（`b="1"` を付けた対照では約 1.7 倍）。
pub(crate) fn corpus_report_pptx_named_heavy_font_is_heavy() {
    let pages = render("heavy-named-font.pptx", "heavy-named-font");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let mass = |top: f64| ink_mass(page, rect(pt(40.0), pt(top), pt(880.0), pt(70.0)));
    let normal = mass(340.0);
    assert!(normal > 0.0, "control: the Meiryo line is drawn");
    let mut checks = Checks::default();
    for (name, top) in [("HGP創英角ｺﾞｼｯｸUB", 40.0), ("HGPｺﾞｼｯｸE", 140.0), ("HG創英角ｺﾞｼｯｸUB", 240.0)] {
        let ratio = mass(top) / normal;
        checks.check(ratio >= 1.3, format!("{name}: {ratio:.2} times the ink of the regular line, at least 1.3"));
    }
    checks.done("pptx-C004-b1: fonts named for their weight");
}

/// pptx-C004-b5（todo に項目なし）：丸数字（U+2460〜）を、東アジアの文字として扱わず（`is_east_asian` が U+2E80 以上）、
/// run の `latin` の書体（Calibri の代わり）の小さな字形・半角ほどの送りで描くため、前後に隙間ができる。
/// PowerPoint は `ea` の書体（メイリオ）の全角の丸数字で描く。36pt の「…特徴②～」の 4 行のうち、`latin` も
/// メイリオの 3 行目は詰まる。ほかの 3 行（`latin` が +mn-ea（Calibri）・Calibri）の幅は、3 行目より 0.15 字（5pt）を
/// 超えて広くならない（今は約 24px = 0.33 字広い）。
/// 注意：同じプロセスで先に別の試験（p2・p5a・p6 など）が何枚か描かれると緑になる（順序依存。原因は未調査）。単独、または
/// 先頭で回すと赤なので、`all()` の先頭に登録する。
pub(crate) fn corpus_report_pptx_circled_digit_in_east_asian_font() {
    let pages = render("circled-digit-latin-font.pptx", "circled-digit-latin-font");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let width = |top: f64| extent(page, rect(pt(40.0), pt(top), pt(880.0), pt(80.0)), dark).map(|(l, r)| (r - l) as f64);
    let control = width(230.0).expect("the line whose latin font is Meiryo is drawn");
    let mut checks = Checks::default();
    for (name, top) in [("latin=+mn-ea", 30.0), ("latin=Calibri, ea=メイリオ", 130.0), ("latin=Calibri, ea=+mn-ea", 330.0)] {
        let wide = width(top).unwrap_or(0.0) - control;
        checks.check(wide <= page.pt(36.0 * 0.15), format!("{name}: {wide:.0} px wider than the Meiryo line, at most {:.0} (0.15 em)", page.pt(36.0 * 0.15)));
    }
    checks.done("pptx-C004-b5: a circled digit");
}

/// pptx-C005-b1（todo に項目なし）：`a:br` だけで作った空行の高さを、br 自身の文字の大きさ（`a:br/a:rPr sz`）でなく、
/// 段落の `endParaRPr`（無ければ既定の 18pt）で決める。PowerPoint は br の文字の大きさ × 1.2。
/// 2 段落目が `<a:br sz=400/>` + 20pt の「Ccc three」のとき、1 つ前の行（20pt の「Bbb two」）の上の端から
/// 「Ccc three」の上の端まで 24 + 4.8 = 28.8pt（`endParaRPr sz=400` を足した対照は 28.0pt。今は 45pt）。31pt 以下。
pub(crate) fn corpus_report_pptx_br_blank_line_takes_its_own_size() {
    let pt = |p: f64| (p * 12_700.0) as i64;
    let mut checks = Checks::default();
    for (name, file, label) in [("br only", "br-blank-line-size.pptx", "br-blank"), ("with endParaRPr (control)", "br-blank-line-size-ctrl.pptx", "br-blank-ctrl")] {
        let pages = render(file, label);
        let page = &pages[0];
        let bands = ink_bands(page, rect(pt(333.0), pt(103.0), pt(294.0), pt(144.0)), dark, page.pt(20.0 * 0.3));
        if bands.len() != 3 {
            checks.check(false, format!("{name}: {} lines of text found, 3 expected", bands.len()));
            continue;
        }
        let pitch = (bands[2].0 - bands[1].0) as f64 / page.pt(1.0);
        checks.check(pitch <= 31.0, format!("{name}: the line after the blank one starts {pitch:.1}pt below the line before it, at most 31"));
    }
    checks.done("pptx-C005-b1: the height of a line made of a:br");
}

/// pptx-C005-b3（todo に項目なし）：`a:tr h` の合計が表の枠の高さと（1% 以内で）等しい表は、保存されたときに組み終えた
/// 行の高さとして読む（`fixed_table`）が、文字が折り返して収まらない行も広げず、2 行目が下の行の罫線に重なる。
/// PowerPoint は読み込んだときに組み直して、その行を広げて下の行を押し下げる。20pt・行間 30pt の文が 2 行になる行は
/// 約 64〜66pt に広がる（今は 48.2pt のまま）。1 行目の下の罫線から 2 行目の下の罫線まで 60pt 以上
/// （枠の高さを合計の 105% にした対照は約 64pt）。
pub(crate) fn corpus_report_pptx_table_row_grows_for_wrapped_text() {
    let pt = |p: f64| (p * 12_700.0) as i64;
    let orange = |c: Rgb| near(c, [0xed, 0x7d, 0x31], 60);
    let mut checks = Checks::default();
    for (name, file, label) in [("frame = sum of the rows", "table-rows-grow.pptx", "table-rows"), ("frame 105% (control)", "table-rows-grow-ctrl.pptx", "table-rows-ctrl")] {
        let pages = render(file, label);
        let page = &pages[0];
        let (x0, x1) = (page.px(pt(70.0)), page.px(pt(450.0)));
        let mut rules: Vec<i64> = Vec::new();
        for y in page.px(pt(60.0))..page.px(pt(420.0)) {
            let share = (x0..x1).filter(|x| page.pixel(*x, y).is_some_and(orange)).count() as f64 / (x1 - x0) as f64;
            if share >= 0.6 && rules.last().is_none_or(|last| y - last > 4) {
                rules.push(y);
            }
        }
        if rules.len() < 3 {
            checks.check(false, format!("{name}: {} horizontal rules found, at least 3", rules.len()));
            continue;
        }
        let height = (rules[1] - rules[0]) as f64 / page.pt(1.0);
        checks.check(height >= 60.0, format!("{name}: the second row is {height:.1}pt high, at least 60 for its 2 lines"));
    }
    checks.done("pptx-C005-b3: a table row with wrapped text");
}

/// pptx-C006-b1（todo に項目なし）：`marL=0`・`indent=0` の記号付きの段落で、本文の 1 行目が記号の左端（0）から始まり、
/// 記号が本文の頭に重なる。PowerPoint は 1 行目の本文を記号の終わりから始める（記号の幅の分右）。
/// Arial 20pt の「Alpha beta gamma delta epsilon」は、記号が無ければ幅約 284pt（箱の左から右端まで）。
/// 「•」（約 7pt）が付けば、右端は箱の左から 288pt 以上（今は 284pt）。
pub(crate) fn corpus_report_pptx_bullet_without_hang_pushes_text() {
    let pages = render("bullet-no-hang.pptx", "bullet-no-hang");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let bands = ink_bands(page, rect(pt(50.0), pt(20.0), pt(300.0), pt(80.0)), dark, page.pt(20.0 * 0.3));
    assert!(!bands.is_empty(), "control: the paragraph is drawn");
    let (x0, x1) = (page.px(pt(50.0)), page.px(pt(350.0)));
    let (_, right) = extent(page, area_px(page, x0, bands[0].0, x1, bands[0].1 + 1), dark).expect("the first line is drawn");
    let width = (right - x0) as f64 / page.pt(1.0);
    assert!(width >= 288.0, "the first line ends {width:.1}pt from the box's left; the text alone is about 284pt wide, a bullet pushes it right: at least 288");
}

/// pptx-C007-b1（todo-pptx P7b の「英数字の縮めは未対応」に当たる）：入っていない比例の日本語の書体（ＭＳ Ｐゴシック・
/// Meiryo UI・HGPｺﾞｼｯｸE）の全角の数字・英字が、全角の幅（1 字）で並ぶ。PowerPoint はその書体の字形で、半角に近い詰まった幅。
/// 32pt の「第２回（２０２０）：ＸＹＺ・ＡＢＣ」の 3 行（UD デジタル 教科書体の行は書体の字幅が不明なので見ない）の幅は、
/// 半角の「第2回(2020):XYZ･ABC」の対照の幅の 1.08 倍以下（今は約 1.19 倍）。
pub(crate) fn corpus_report_pptx_fullwidth_alnum_in_proportional_font() {
    let bug = render("fullwidth-alnum-proportional.pptx", "fullwidth-alnum");
    let ctrl = render("fullwidth-alnum-proportional-ctrl.pptx", "fullwidth-alnum-ctrl");
    let pt = |p: f64| (p * 12_700.0) as i64;
    let width = |page: &Page, top: f64| extent(page, rect(pt(40.0), pt(top), pt(880.0), pt(50.0)), dark).map(|(l, r)| (r - l) as f64);
    let control = width(&ctrl[0], 40.0).expect("the control line is drawn");
    let mut checks = Checks::default();
    for (name, top) in [("ＭＳ Ｐゴシック", 40.0), ("Meiryo UI", 150.0), ("HGPｺﾞｼｯｸE", 370.0)] {
        let ratio = width(&bug[0], top).unwrap_or(0.0) / control;
        checks.check(ratio <= 1.08, format!("{name}: {ratio:.2} times the half-width control's width, at most 1.08"));
    }
    checks.done("pptx-C007-b1: full-width digits and letters in a proportional font");
}

/// pptx-C008-b1（todo に項目なし）：タブ文字を `defTabSz`（既定の刻み）・`tabLst` の刻みで進めず、約 40pt ずつしか進めない。
/// 行頭の「タブ 2 つ + 全角空白 3 つ」の白い文字が、PowerPoint では枠の左端（100pt）+ 左余白 7.2pt + 144pt + 48pt = 299.2pt から始まり、
/// 青い角丸四角（290pt から）の内側に収まる。今は約 235pt から始まって四角の左の外（白地）へはみ出し、
/// 四角の左端の 8pt の幅（290〜298pt）に白い字の画素がある。
pub(crate) fn corpus_report_pptx_tabs_advance_to_tab_stops() {
    let pages = render("tab-stop-width.pptx", "tab-stop-width");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let white = |c: Rgb| c.iter().all(|v| *v > 235);
    let count = |area| page.pixels(area).into_iter().filter(|(_, _, c)| white(*c)).count();
    let text = count(rect(pt(298.0), pt(200.0), pt(400.0), pt(55.0)));
    assert!(text > 0, "control: the white text is drawn inside the blue shape");
    let edge = count(rect(pt(290.0), pt(200.0), pt(8.0), pt(55.0)));
    assert_eq!(edge, 0, "the text starts at 299.2pt, so the shape's left 8pt (290-298pt) hold no white text: {edge} white pixels");
}

/// pptx-C023-b1（todo に項目なし）：閉じた図形（rect・roundRect）の輪郭で、`sysDash`・`sysDot`（隙間が線幅 1 つ分の破線）に
/// `cap="sq"` を付けると、角キャップが隙間を埋めて実線になる。PowerPoint は隙間が残る破線・点線。
/// 上の辺の真ん中の 200pt のうち、赤い線の画素が途切れる所（隙間）が 5% 以上。cap=flat の sysDash（D）は対照。
pub(crate) fn corpus_report_pptx_sysdash_with_square_cap_keeps_gaps() {
    let pages = render("sysdash-square-cap.pptx", "sysdash-square-cap");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let mut checks = Checks::default();
    for (name, left, top) in [
        ("A rect sysDash cap=sq 6pt", 40.0, 70.0),
        ("B rect sysDot cap=sq 6pt", 340.0, 70.0),
        ("C roundRect sysDash cap=sq 3pt", 640.0, 70.0),
        ("D rect sysDash cap=flat (control)", 40.0, 290.0),
    ] {
        let y = page.px(pt(top));
        let share = gap_share(page, (page.px(pt(left + 20.0)), page.px(pt(left + 220.0))), (y - 1, y + 1), red);
        checks.check(share >= 0.05, format!("{name}: {:.0}% of the top edge is gap, at least 5%", share * 100.0));
    }
    checks.done("pptx-C023-b1: sysDash with a square cap");
}

/// pptx-C023-b2（todo に項目なし）：グラフの線・枠線・目盛線の破線（`prstDash`）が実線になる（`chart_view.rs` の `Mark::Stroke`
/// が破線を持たない）。左の折れ線グラフの縦軸の目盛線（`dash`）は、上の端の 1 本の画素の 85% 以下しか埋まらず（4:3 の
/// 破線で約 57%）、右の積み上げ棒グラフの 1 本目の棒の下の部分の枠線（`sysDash`、赤）は、縦の 85% 以下しか赤くない
/// （3:1 の破線で約 75%。今はどちらもほぼ 100%）。
pub(crate) fn corpus_report_pptx_chart_dashes_drawn() {
    let pages = render("chart-dash-ignored.pptx", "chart-dash-ignored");
    let page = &pages[0];
    let grey = |c: Rgb| c.iter().all(|v| *v < 215) && spread(c) < 20;
    let mut checks = Checks::default();
    let covered = |y: i64| 1.0 - gap_share(page, (150, 900), (y, y), grey);
    let gridline = (125..160).map(covered).fold(0.0, f64::max);
    checks.check(gridline > 0.0, "control: the top gridline is found".to_string());
    checks.check(gridline <= 0.85, format!("the dash gridline is {:.0}% covered, at most 85%", gridline * 100.0));
    let bar = (1040..1062).map(|x| (650..870).filter(|y| page.pixel(x, *y).is_some_and(red)).count() as f64 / 220.0).fold(0.0, f64::max);
    checks.check(bar > 0.0, "control: the red bar outline is found".to_string());
    checks.check(bar <= 0.85, format!("the sysDash bar outline is {:.0}% red along its length, at most 85%", bar * 100.0));
    checks.done("pptx-C023-b2: dashes in a chart");
}

/// pptx-C023-b3（todo に項目なし）：表のセルの罫線（`a:lnT`・`a:lnB`）の破線が、表の枠の高さ（`cy`）が行の高さ（`a:tr h`）の
/// 合計と違うと実線になる。PowerPoint は枠の高さに関わらず行の高さのとおりの破線。3 つの表（行は 60pt × 3 行で同じ。枠の高さは
/// 180pt = 合計（A、対照）、198pt（B）、162pt（C））の 1 行目と 2 行目の間の罫線に、隙間が 10% 以上。
pub(crate) fn corpus_report_pptx_table_dash_with_other_frame_height() {
    let pages = render("table-dash-frame-height.pptx", "table-dash-frame-height");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let mut checks = Checks::default();
    for (name, left, width) in [("A frame 180pt = rows (control)", 60.0, 240.0), ("B frame 198pt", 400.0, 240.0), ("C frame 162pt", 700.0, 200.0)] {
        let y = page.px(pt(150.0));
        let share = gap_share(page, (page.px(pt(left + 10.0)), page.px(pt(left + width - 10.0))), (y - 1, y + 1), dark);
        checks.check(share >= 0.1, format!("{name}: {:.0}% of the rule between rows 1 and 2 is gap, at least 10%", share * 100.0));
    }
    checks.done("pptx-C023-b3: table borders when the frame is not the rows' sum");
}

/// pptx-C023-b4（todo に項目なし。pptx-C023 の記号の字形の差のうち、太字にならない矢印だけ）：入っていない書体（Noto Sans JP）を
/// 指定した run の矢印「→」に `b="1"` を付けても、太字にならない（`b="0"` の対照と同じ字形・同じ太さ）。
/// PowerPoint は太字の字形。48pt の A（b=1）のインクの量は、B（b=0）の 1.15 倍以上（今は同じ）。
pub(crate) fn corpus_report_pptx_symbol_arrow_follows_bold() {
    let pages = render("symbol-glyph-bold.pptx", "symbol-glyph-bold");
    let page = &pages[0];
    let pt = |p: f64| (p * 12_700.0) as i64;
    let bold = ink_mass(page, rect(pt(40.0), pt(70.0), pt(280.0), pt(90.0)));
    let plain = ink_mass(page, rect(pt(340.0), pt(70.0), pt(280.0), pt(90.0)));
    assert!(plain > 0.0, "control: the b=0 arrow is drawn");
    assert!(bold >= 1.15 * plain, "the b=1 arrow's ink is {:.2} times the b=0 arrow's, at least 1.15", bold / plain);
}

/// The bounding box (left, top, right, bottom pixel) of the pixels of `area` that `which` picks.
fn bbox(page: &Page, area: EmuRect, which: impl Fn(Rgb) -> bool) -> Option<(i64, i64, i64, i64)> {
    let hits: Vec<(i64, i64)> = page.pixels(area).into_iter().filter(|(_, _, c)| which(*c)).map(|(x, y, _)| (x, y)).collect();
    Some((hits.iter().map(|p| p.0).min()?, hits.iter().map(|p| p.1).min()?, hits.iter().map(|p| p.0).max()?, hits.iter().map(|p| p.1).max()?))
}

/// Black text: every channel under 40 (on a blue or grey bar the text's own pixels, not the bar's colour).
fn very_dark(c: Rgb) -> bool {
    c.iter().all(|v| *v < 40)
}

/// The EMU the pixel `px` starts at.
fn emu_at(page: &Page, px: i64) -> i64 {
    (px as f64 / page.per_emu).round() as i64
}

fn white(c: Rgb) -> bool {
    c.iter().all(|v| *v >= 235)
}

fn check_picture_fill_outline(file: &str) {
    let pages = render(file, "picfill-outline-shifted");
    let page = &pages[0];
    let mut checks = Checks::default();
    // Boxes of 5200000 (tile) and 4800000 (stretch) wide, the first row from y=1292469, the last row 673235 high from y=4514227.
    for (name, x) in [("tile", 3_400_000), ("stretch", 8_800_000)] {
        let top = page.at(x, 1_292_469);
        checks.check(near(top, [127, 127, 127], 30), format!("{name}: the box's top edge is the grey outline (#7F7F7F, 3pt) at its own place: {top:?}"));
        let below = page.at(x, 4_514_227 + 673_235 + 673_235 - 12_700);
        checks.check(white(below), format!("{name}: below the last box there is no outline (a box's height under its own place): {below:?}"));
    }
    checks.done("pptx-C010-b2: outline of a shape with a picture fill");
}

/// pptx-C010-b2（todo に項目なし）：絵の塗り（`a:blipFill`、タイルと引き伸ばし）と見える輪郭線（灰色 3pt）を持つ角丸四角形で、
/// 輪郭線が図形の高さの分だけ下にずれ、図形の位置には輪郭線が出ない。PowerPoint は図形の縁を囲む。
/// 1 つ目の箱の上端（灰色 #7F7F7F）が灰色で、最後の箱の下に空の枠（ずれた輪郭線）が無い（白）。今は上端が塗りの色で、
/// 箱の下に灰色の枠が出る。
pub(crate) fn corpus_report_pptx_picture_fill_outline_stays_on_shape() {
    check_picture_fill_outline("picfill-outline-shifted.pptx");
}

fn check_scaled_image_edge(file: &str) {
    let pages = render_slides_at(file, "image-scaled-edge-line", &[], 960);
    let page = &pages[0];
    let (right, bottom) = (571_500 + 3_600_450, 571_500 + 4_248_150);
    let lightest = |area: EmuRect| page.pixels(area).into_iter().map(|(_, _, c)| *c.iter().min().unwrap()).min().expect("pixels");
    let side = lightest(rect(right - 40_000, 1_500_000, 50_000, 2_500_000));
    let foot = lightest(rect(1_000_000, bottom - 40_000, 2_500_000, 50_000));
    let mut checks = Checks::default();
    checks.check(side >= 245, format!("the right edge column of the enlarged picture's white ground: darkest pixel {side}, at least 245 (nothing there)"));
    checks.check(foot >= 245, format!("the bottom edge row of the enlarged picture's white ground: darkest pixel {foot}, at least 245 (nothing there)"));
    checks.done("pptx-C010-b1: edge lines of a scaled picture");
}

/// pptx-C010-b1（todo に項目なし）：画像を画素数と違う大きさ（ここでは 1.5 倍）に拡大して描くと、白い地の右端の列と下端の行に
/// 薄い灰色（約 219）の 1 画素の線が出る（拡大の補間が縁の外の透明な画素を混ぜるため）。PowerPoint にこの線は無く、画像の白い地は
/// 紙の白に溶ける。960 画素幅（96 dpi）で描くと、1.5 倍の左の絵だけに出て、原寸の右の絵には出ない。
/// 画像の白い地（円から離れた所）の右端・下端の細い帯に、245 より暗い画素が無い。
pub(crate) fn corpus_report_pptx_scaled_image_has_no_edge_line() {
    check_scaled_image_edge("image-scaled-edge-line.pptx");
}

fn check_close_paren(file: &str) {
    let pages = render(file, "close-paren-after-space");
    let page = &pages[0];
    let frame = rect(1_270_000, 1_524_000, 2_722_880, 1_524_000);
    let inner = rect(frame.x + 25_400, frame.y + 25_400, frame.width - 50_800, frame.height - 50_800);
    let bands = ink_bands(page, inner, dark, 6.0);
    assert_eq!(bands.len(), 2, "the text is two lines: {bands:?}");
    let second = area_px(page, page.px(inner.x), bands[1].0, page.px(inner.x + inner.width), bands[1].1 + 1);
    let (left, right) = extent(page, second, dark).expect("the second line has ink");
    let want = page.pt(40.0) as i64;
    assert!(
        right - left >= want,
        "pptx-C011-b1: the second line holds \"(\", three full-width spaces of 20pt and \")\" together (ink {} px wide, at least {want}); a lone \")\" is the defect",
        right - left
    );
}

/// pptx-C011-b1（todo に項目なし）：全角空白 3 つの後ろの閉じ括弧「)」が行頭に落ち、開き括弧「(」が前の行の末尾に残る。
/// 20pt の全角 9 字の後ろの「(　　　)」を、9 字と「(」が入るだけの幅の枠に入れる。PowerPoint は「(」を行末に置かず（行末禁則）、
/// 「(　　　)」ごと次の行へ送る（2 行目は「(」と「)」が約 60pt 離れて並ぶ）。今は「空白の後ろ」を無条件に切れ目にするため、
/// 2 行目が「)」だけになる。対照（空白を全角の数字に替えたもの）は「(１２３)」ごと 2 行目に来る。
pub(crate) fn corpus_report_pptx_close_paren_after_space_does_not_start_line() {
    check_close_paren("close-paren-after-space.pptx");
}

fn check_hyphen_break(file: &str) {
    let pages = render(file, "latin-break-slash-hyphen");
    let page = &pages[0];
    // The lower box: 4826000 wide, 91440 insets, Arial 12pt: "Maze layouts, stacking and ceramic assortment, tide-forces vector, ..."
    let frame = rect(1_270_000, 2_667_000, 4_826_000, 889_000);
    let inner = rect(frame.x + 91_440, frame.y + 25_400, frame.width - 2 * 91_440, frame.height - 50_800);
    let bands = ink_bands(page, inner, dark, 6.0);
    assert!(bands.len() >= 2, "the sentence is two lines: {bands:?}");
    let first = area_px(page, page.px(inner.x), bands[0].0, page.px(inner.x + inner.width), bands[0].1 + 1);
    let (left, right) = extent(page, first, dark).expect("the first line has ink");
    let width = (right - left) as f64;
    let inner_width = page.px(inner.width) as f64;
    assert!(
        width >= 0.88 * inner_width,
        "pptx-C011-b2: the first line breaks after \"tide-\" (ink {width} px of the {inner_width} px inside); breaking before \"tide-forces\" leaves it at {:.0}%, under 88%",
        100.0 * width / inner_width
    );
}

/// pptx-C011-b2（todo に項目なし）：英字の連なりの中の「-」の後ろで改行できず、「tide-forces」を切らずに丸ごと次の行へ送る。
/// PowerPoint は「-」（と URL の「/」）の後ろで切れるので、380pt の枠の「Maze layouts, stacking and ceramic assortment, tide-」
/// で 1 行目が終わり、1 行目は枠の内側の幅の約 92% を使う。今は「assortment,」で終わり、約 82%。対照（「-」の後ろに空白を足したもの）は 92%。
/// （同じスライドの URL の「/」の後ろの切れ目は、この文書の枠の幅では期待の位置を決めきれないため試験にしない。）
pub(crate) fn corpus_report_pptx_latin_break_after_hyphen() {
    check_hyphen_break("latin-break-slash-hyphen.pptx");
}

/// pptx-C013-b1（todo に項目なし。pptx-C004-b5・pptx-C009 の記号の字形とは別）：欧文の書体（Arial）の buFont と buChar「•」の
/// 箇条書き記号が、本文の書体でなくアプリ既定の書体で描かれ、小さく（本文中の「•」の約 0.75 倍）低く（文字の中ほどより約 7px 下）出る。
/// 32pt の 4 行。PowerPoint の記号は Arial の「•」で、本文中の「•」と同じ大きさ（16×16px）、中心は行の字の上端から約 25px。
/// 今は 12×12px、中心が上端から約 32px。
pub(crate) fn corpus_report_pptx_bullet_char_drawn_in_text_font() {
    let pages = render("bullet-char-arial-glyph.pptx", "bullet-char-arial-glyph");
    let page = &pages[0];
    let (top, bottom) = (120, 196);
    let (bx0, by0, bx1, by1) = bbox(page, area_px(page, 118, top, 180, bottom), dark).expect("the bullet is drawn");
    let (_, ty0, _, _) = bbox(page, area_px(page, 190, top, 600, bottom), dark).expect("the text is drawn");
    let mut checks = Checks::default();
    checks.check(bx1 - bx0 + 1 >= 14 && by1 - by0 + 1 >= 14, format!("the bullet is {}×{} px, at least 14×14 (Arial's bullet at 32pt)", bx1 - bx0 + 1, by1 - by0 + 1));
    let middle = (by0 + by1) / 2 - ty0;
    checks.check(middle <= 28, format!("the bullet's middle is {middle} px below the text's top, at most 28 (it sits at the middle of the letters)"));
    checks.done("pptx-C013-b1: bullet glyph of buChar");
}

fn check_mn_ea_circle(file: &str) {
    let pages = render(file, "defaulttextstyle-mnea-symbols");
    let page = &pages[0];
    let runs = ink_runs(page, area_px(page, 60, 150, 1000, 300), dark);
    let first = runs.first().expect("the first line has ink");
    let diameter = first.1 - first.0 + 1;
    assert!(
        diameter >= 100,
        "pptx-C013-b2: the circle of the 72pt line is {diameter} px across (the em is 144 px; the full-width circle of the Japanese font is about one em), at least 100"
    );
}

/// pptx-C013-b2（todo に項目なし。P7b の書体の代わりとは別）：`presentation.xml` の `defaultTextStyle` に `a:ea typeface="+mn-ea"`
/// があり（テーマの ea は空）、run の書体が latin だけ（ＭＳ Ｐゴシック）のとき、72pt の「○」が直径約 0.44em（63px）の小さな
/// 丸になる。PowerPoint は ＭＳ ゴシックの全角の「○」（約 1em）で描く。`a:ea +mn-ea` を外した対照は 127px（0.88em）。
/// 今は 63px、期待は 100px 以上。
pub(crate) fn corpus_report_pptx_default_text_style_mn_ea_keeps_symbol_size() {
    check_mn_ea_circle("defaulttextstyle-mnea-symbols.pptx");
}

fn check_msgothic_symbols(file: &str) {
    let pages = render(file, "msgothic-symbols-fallback");
    let page = &pages[0];
    let mut checks = Checks::default();
    // 60pt (120 px an em) bold: row 1 is Verdana (it has no triangle), row 2 is MS PGothic (not installed).
    for (name, area) in [("Verdana ▷", area_px(page, 450, 120, 670, 270)), ("ＭＳ Ｐゴシック ▶▶", area_px(page, 450, 270, 745, 440))] {
        match bbox(page, area, dark) {
            Some((_, y0, _, y1)) => checks.check(y1 - y0 + 1 <= 78, format!("{name}: the triangle is {} px high, at most 78 (0.65 em; PowerPoint draws MS Gothic's, about 0.5 em)", y1 - y0 + 1)),
            None => checks.check(false, format!("{name}: nothing is drawn")),
        }
    }
    checks.done("pptx-C013-b3: triangles of a font without them");
}

/// pptx-C013-b3（todo に項目なし。P7b と同じ書体の代わりだが、症状が字幅でなく字形の大きさ）：記号の「▷」「▶」が、PowerPoint では
/// ＭＳ ゴシックの小さな字形（0.5em ほど）で出るのに、ヒラギノ系の大きな字形（高さ 0.93em）で出る。Verdana を latin・ea に指定した行
/// （Verdana に ▷ が無い字を ＭＳ ゴシックで描く PowerPoint の選び方）と、入っていない ＭＳ Ｐゴシックの行の 2 通り。
/// 60pt の太字で、三角の高さは 0.65em（78px）以下。今は 112px（どちらも）。対照（Arial Unicode MS）は 58px。
pub(crate) fn corpus_report_pptx_symbols_use_small_gothic_shapes() {
    check_msgothic_symbols("msgothic-symbols-fallback.pptx");
}

fn check_picture_round_rect(file: &str) {
    let pages = render(file, "pic-roundrect-clip");
    let page = &pages[0];
    let frame = rect(762_000, 1_524_000, 4_572_000, 2_540_000);
    let mut checks = Checks::default();
    for (name, x, y) in [
        ("top left", frame.x + 50_000, frame.y + 50_000),
        ("top right", frame.x + frame.width - 50_000, frame.y + 50_000),
        ("bottom left", frame.x + 50_000, frame.y + frame.height - 50_000),
        ("bottom right", frame.x + frame.width - 50_000, frame.y + frame.height - 50_000),
    ] {
        let c = page.at(x, y);
        checks.check(white(c), format!("{name}: a corner of a picture cut to a rounded rectangle (adj 20000, a radius of 508000 EMU) shows the slide's white, not the picture's orange mark: {c:?}"));
    }
    checks.done("pptx-C014-b1: picture cut to roundRect");
}

/// pptx-C014-b1（todo に項目なし。P5a の画像の切り抜き（ellipse など）とは別の形）：図（`p:pic`）の `prstGeom prst="roundRect"`
/// （`adj` 20000）の角丸の切り抜きが効かず、画像が四角のまま描かれる（`flowChartAlternateProcess` など別の形は切り抜かれる）。
/// 四隅に橙の印を置いた画像で、半径は短辺の 20%（508000 EMU）なので、四隅から 50000 EMU の点は切り抜きの外（白）になる。
/// 今は 4 つとも橙のまま。対照（`rect`）は同じ見え方で、正しい。
pub(crate) fn corpus_report_pptx_picture_round_rect_is_clipped() {
    check_picture_round_rect("pic-roundrect-clip.pptx");
}

fn check_rotated_picture_fill(file: &str) {
    let pages = render(file, "rotated-picfill-clip");
    let page = &pages[0];
    let mut checks = Checks::default();
    for (name, x, y, w, h) in [("cloud, rotWithShape=0", 508_000, 762_000, 3_048_000, 2_540_000), ("cloud, rotWithShape=1", 4_318_000, 762_000, 3_048_000, 2_540_000), ("custGeom pentagon", 8_128_000, 762_000, 3_048_000, 2_540_000)] {
        let c = page.at(x + w / 50, y + h / 50);
        checks.check(white(c), format!("{name}: the frame's top-left corner is outside the 30°-turned shape, so the picture (black mark there) is not drawn: {c:?}"));
    }
    checks.done("pptx-C014-b2: picture fill of a turned shape");
}

/// pptx-C014-b2（todo に項目なし）：回転（`rot` が 0 でない）した、画像の塗り（`a:blipFill`）の図形（雲・五角形の `custGeom`）が、
/// 図形の形で切り抜かれず、画像の四角い範囲が丸ごと描かれる（`rotWithShape="1"` でも）。画像の左上に黒い印があるので、
/// 図形の枠の左上の隅（枠の 2% 内側）は、図形の外なので白地のまま。今は 3 つとも黒。対照（回転 0）は白。
pub(crate) fn corpus_report_pptx_rotated_picture_fill_is_clipped() {
    check_rotated_picture_fill("rotated-picfill-clip.pptx");
}

fn check_clr_change_with_duotone(file: &str) {
    let pages = render(file, "clrchange-with-duotone");
    let page = &pages[0];
    let ground = page.at(11_800_000, 6_500_000);
    let c = page.at(508_000 + 2_540_000 / 25, 1_016_000 + 2_032_000 / 25);
    assert!(near(c, ground, 14), "pptx-C014-b3: the black ground of a picture with clrChange (black to transparent) and duotone shows the slide ({ground:?}), the black is {c:?}");
}

/// pptx-C014-b3（todo に項目なし。P5a の clrChange とは別：duotone との併用）：同じ `a:blip` に `a:clrChange`（黒→透明）と
/// `a:duotone` があると、clrChange が効かず黒い地が残る（clrChange だけの図では黒が透明になる）。PowerPoint は黒い地を透明にし、
/// 人物だけを二色化の色で描くので、画像の左上の隅（黒い地。4% 内側）にはスライドの背景（#FFF2B0）が見える。今は黒。
pub(crate) fn corpus_report_pptx_clr_change_with_duotone_makes_ground_transparent() {
    check_clr_change_with_duotone("clrchange-with-duotone.pptx");
}

fn check_percent_stacked_labels(file: &str) {
    let pages = render(file, "percentstacked-label-value");
    let page = &pages[0];
    let bands = ink_bands(page, area_px(page, 330, 100, 480, 800), very_dark, 8.0);
    assert!(!bands.is_empty(), "the first bar's labels are drawn");
    let mut checks = Checks::default();
    for (y0, y1) in bands {
        let (l, r) = extent(page, area_px(page, 330, y0, 480, y1 + 1), very_dark).unwrap();
        checks.check(r - l + 1 <= 70, format!("the label at y {y0}..{y1} is {} px wide, at most 70 (a value like 4.3 at 9pt, not a ten-digit fraction)", r - l + 1));
    }
    checks.done("pptx-C015-b1: labels of a percentStacked bar chart");
}

/// pptx-C015-b1（todo に項目なし。pptx-C038 の H290 とは別）：百分率積み上げ棒グラフ（`grouping="percentStacked"`）のデータラベル
/// （`showVal`）が、元の値（4.3・2.4・2 …）でなく棒の長さの計算に使った構成比（0.4942528736 など小数 10 桁）で出る。
/// PowerPoint のラベルは元の値。最初の棒の 3 つのラベルは幅 70 画素以下（元の値は 11〜31px、今は 135px 以上）。
/// 対照（`grouping="stacked"`）は元の値。
pub(crate) fn corpus_report_pptx_percent_stacked_labels_show_values() {
    check_percent_stacked_labels("percentstacked-label-value.pptx");
}

fn check_tick_labels_low(file: &str) {
    let pages = render(file, "ticklblpos-low-top-axis");
    let page = &pages[0];
    let area = area_px(page, 150, 896, 1750, 1000);
    let ink = page.pixels(area).into_iter().filter(|(_, _, c)| c.iter().all(|v| *v < 100)).count();
    assert!(ink >= 100, "pptx-C015-b2: the value axis' numbers (0, 2,000, 4,000, 6,000; tickLblPos=low) are below the plot: {ink} dark pixels there, at least 100");
}

/// pptx-C015-b2（todo に項目なし）：横棒グラフの値軸が上（`axPos="t"`、`crosses="max"`）にあり `tickLblPos="low"` のとき、目盛りの数字
/// （0・2,000・4,000・6,000）は PowerPoint では描画域の下端の下に出る。今は `low` を読まず軸のそば（枠の上端の外）に置くので、
/// 数字が出ない。描画域の下（枠の下端の手前）の帯に、数字の黒い画素が 100 以上。今は 0。対照（軸が下）は 986。
pub(crate) fn corpus_report_pptx_tick_labels_low_go_below_the_plot() {
    check_tick_labels_low("ticklblpos-low-top-axis.pptx");
}

fn check_point_label(file: &str) {
    let pages = render(file, "point-dlbl-showval");
    let page = &pages[0];
    let bands = ink_bands(page, area_px(page, 1380, 100, 1778, 660), |c| c.iter().all(|v| *v < 80), 8.0);
    assert!(bands.len() >= 2, "pptx-C015-b3: the two series' last points carry their own labels (1030 and 720): {bands:?}");
}

/// pptx-C015-b3（todo に項目なし）：折れ線グラフの系列のデータラベルが `showVal=0` で、最後の点だけ `c:dLbl` の `showVal=1` のとき、
/// PowerPoint は右端の 2 点（1030 と 720）にだけラベルを出す。今は系列の設定が 0 だと点の設定を見ず、ラベルが 1 つも出ない。
/// 右端の帯（最後の点の周り）に、字の高さのある黒い行の塊が 2 つ以上。今は 0（格子線の細い行だけ）。対照（系列も `showVal=1`）は 2 つ。
pub(crate) fn corpus_report_pptx_point_label_show_value_overrides_series() {
    check_point_label("point-dlbl-showval.pptx");
}

fn check_pie_label_box(file: &str) {
    let pages = render(file, "pie-leader-lines-box");
    let page = &pages[0];
    let frame = rect(762_000, 508_000, 10_668_000, 5_842_000);
    let boxed = page.pixels(frame).into_iter().filter(|(_, _, c)| near(*c, [64, 64, 64], 3)).count();
    assert!(boxed >= 8_000, "pptx-C015-b5: the labels' box fill (#404040, `c:dLbls/c:spPr`) is drawn: {boxed} pixels of it, at least 8000 (12 boxes of 2 lines)");
}

/// pptx-C015-b5（todo に項目なし。P8d の `bestFit` のラベルとは別）：円グラフのデータラベルの箱の塗り（`c:dLbls/c:spPr`、濃い灰色
/// #404040）を描かない。白い文字が白い背景の上に出て読めなくなる。12 個のラベルの箱（2 行）で #404040 の画素が 8000 以上。
/// 今は文字の縁の約 3000 画素だけ。（引き出し線（`showLeaderLines`）も描かないが、ここでは箱だけを確かめる。）
pub(crate) fn corpus_report_pptx_pie_label_box_fill_is_drawn() {
    check_pie_label_box("pie-leader-lines-box.pptx");
}

fn check_bar_label_offset(file: &str) {
    let pages = render(file, "bar-label-offset-scale");
    let page = &pages[0];
    let blue = |c: Rgb| near(c, [79, 129, 189], 10);
    let (_, top, _, _) = bbox(page, area_px(page, 415, 100, 430, 900), blue).expect("the first bar is drawn");
    let bands = ink_bands(page, area_px(page, 392, 100, 452, 900), very_dark, 8.0);
    assert!(
        bands.iter().any(|(_, y1)| *y1 < top),
        "pptx-C015-b6: the label \"39\" (manualLayout y=-0.26 of the chart frame) lies above the bar's top (y {top}), not inside it: label rows {bands:?}"
    );
}

/// pptx-C015-b6（todo に項目なし）：積み上げ棒グラフの上の系列のラベルの手のずらし（`c:dLbl/c:layout/c:manualLayout` の y=-0.26）を、
/// グラフの枠の高さでなく描画域の高さ（枠の約 66%）で換算するため、ラベルが棒の中に残る。PowerPoint はグラフの枠の 26%
/// （520pt の枠で約 135pt）上へ動かし、棒の頂点の約 19pt 上に出す。3 番目の棒（56）の頂点より上にラベルがある。
/// 今は頂点の約 70px 下（棒の中）。
pub(crate) fn corpus_report_pptx_bar_label_offset_follows_chart_frame() {
    check_bar_label_offset("bar-label-offset-scale.pptx");
}

fn check_auto_axis_min(file: &str) {
    let pages = render(file, "auto-axis-min");
    let page = &pages[0];
    let frame = rect(762_000, 508_000, 10_668_000, 5_842_000);
    let (_, y0, _, y1) = bbox(page, frame, |c| c[0] > 150 && c[1] < 80 && (50..160).contains(&c[2])).expect("the points are drawn");
    let want = page.px(frame.height) * 25 / 100;
    assert!(
        y1 - y0 >= want,
        "pptx-C015-b7: the points (y 81.5 to 86.0, the value axis automatic: PowerPoint draws it from about 80 to 88) spread {} px high, at least {want} (a quarter of the chart); on 0 to 100 they are flat",
        y1 - y0
    );
}

/// pptx-C015-b7（todo に項目なし。pptx-C038 の H290 と同じきっかけ）：散布図の値軸の範囲が自動（`c:min`・`c:max` が無い）で、
/// データが 81.5〜86.0 と狭いとき、PowerPoint は軸を 0 からでなく約 80〜88 にする（最小が最大の約 5/6 以上なら 0 から始めない）。
/// 今は必ず 0 以下から始め（0〜100）、点が上に平らに潰れる（高さ 50px。枠の 5%）。点の縦の広がりは枠の高さの 25% 以上
/// （約 60% になる）。対照（`c:min 80`・`c:max 88`）は 490px。
pub(crate) fn corpus_report_pptx_auto_axis_does_not_start_at_zero_for_narrow_data() {
    check_auto_axis_min("auto-axis-min.pptx");
}

fn check_rotated_picture_outline(file: &str) {
    let pages = render(file, "rotated-pic-outline-shadow");
    let page = &pages[0];
    let (cx, cy) = (800_000.0 + 1_940_162.0 / 2.0, 1_500_000.0 + 2_585_872.0 / 2.0);
    let (hw, hh) = (1_940_162.0 / 2.0, 2_585_872.0 / 2.0);
    let theta = (921_223.0_f64 / 60_000.0).to_radians();
    let (sin, cos) = theta.sin_cos();
    let area = rect(500_000, 1_200_000, 2_600_000, 3_300_000);
    let (mut band, mut grey) = (0, 0);
    for (px, py, c) in page.pixels(area) {
        let (dx, dy) = (emu_at(page, px) as f64 - cx, emu_at(page, py) as f64 - cy);
        let (u, v) = (dx * cos + dy * sin, -dx * sin + dy * cos);
        let outside = (u.abs() - hw).max(v.abs() - hh);
        if (2_540.0..=7_620.0).contains(&outside) {
            band += 1;
            if spread(c) <= 24 && (90..=180).contains(&c[0]) {
                grey += 1;
            }
        }
    }
    let corner = (cx + hw * cos - hh * sin, cy + hw * sin + hh * cos);
    let shade = page.at(corner.0 as i64 + 38_100, corner.1 as i64 + 38_100);
    let mut checks = Checks::default();
    checks.check(band > 0 && grey * 10 >= band * 6, format!("the 1.5pt grey outline (#808080) runs along the turned picture's edge: {grey} of {band} pixels just outside the edge are grey, at least 60%"));
    checks.check(shade[0] < 235, format!("the shadow (6pt towards the bottom right) shows past the turned picture's bottom right corner: {shade:?}"));
    checks.done("pptx-C016-b1: outline and shadow of a turned picture");
}

/// pptx-C016-b1（todo に項目なし）：回した（`rot`）画像（`p:pic`）の枠線（`a:ln`、灰色 1.5pt）と影（`a:outerShdw`）が描かれない
/// （回さない画像には出る。図形 `p:sp` は回しても出る）。PowerPoint は回した矩形（15.35°）の縁に灰色の線を引き、右下へ 6pt の影を付ける。
/// 縁のすぐ外（0.2〜0.6pt）の画素の 6 割以上が灰色、右下の角の外 3pt の点が影で白でない。今は枠線も影も無い（どちらも白）。
/// 対照（`rot` を外したもの）は両方が出る。
pub(crate) fn corpus_report_pptx_rotated_picture_keeps_outline_and_shadow() {
    check_rotated_picture_outline("rotated-pic-outline-shadow.pptx");
}

fn check_table_row_pitch(file: &str) {
    let pages = render(file, "table-row-h0");
    let page = &pages[0];
    let bands = ink_bands(page, rect(1_650_000, 600_000, 20_000, 4_300_000), |c| c.iter().all(|v| *v < 120), 1.0);
    assert_eq!(bands.len(), 32, "the 32 rows' top borders: {bands:?}");
    let pitch = (bands[31].0 - bands[0].0) as f64 / 31.0;
    assert!(pitch <= 20.4, "pptx-C016-b2: the rows (h=\"0\", 8pt Arial) are {pitch:.2} px apart ({:.2}pt), at most 20.4 (10.2pt; PowerPoint lays them 10.0pt apart)", pitch / 2.0);
}

/// pptx-C016-b2（todo に項目なし。pptx-C005-b3 は行の高さの合計が枠と等しい表の話で別）：行の高さが未指定（`a:tr h="0"`）の 8pt
/// Arial の表（上の余白 0.375pt）で、ibisOffice の行が PowerPoint の 10.0pt より 0.5pt 高い 10.5pt になる。32 行で約 16pt 長くなり、
/// 表の下端が枠の下へはみ出して下の画像を隠す。1920 画素幅で行の上の罫線の間隔が 20.4px（10.2pt）以下。今は 21.0px。
/// 対照（行の高さに 126682 EMU を書いたもの）は 19.97px。
pub(crate) fn corpus_report_pptx_table_rows_of_height_zero_are_10pt_apart() {
    check_table_row_pitch("table-row-h0.pptx");
}

// ---- 第 3 回（corpus-report-3）：束 pptx-C017〜C028 の仮バグと、悪化を検知する緑の試験 ----

fn saturated(c: Rgb) -> bool {
    spread(c) >= 70
}

/// Grey text (a chart's labels): dark enough, with no colour.
fn grey_text(c: Rgb) -> bool {
    c.iter().all(|v| *v < 170) && spread(c) < 30
}

fn frame_px(page: &Page, frame: EmuRect) -> (i64, i64, i64, i64) {
    (page.px(frame.x), page.px(frame.y), page.px(frame.width), page.px(frame.height))
}

fn check_pie_auto_plot(file: &str) {
    let pages = render(file, "chart-pie-auto-plot");
    let page = &pages[0];
    let (x0, y0, w, h) = frame_px(page, rect(1_270_000, 508_000, 6_464_300, 5_549_900));
    let (_, t, _, b) = bbox(page, area_px(page, x0, y0, x0 + w, y0 + h * 86 / 100), saturated).expect("the pie is drawn");
    let d = (b - t + 1) as f64 / h as f64;
    let centre = ((t + b) as f64 / 2.0 - (y0 as f64 + h as f64 / 2.0)) / h as f64;
    eprintln!("{file}: pie diameter {d:.3} of the frame height, centre {centre:+.3}");
    let mut checks = Checks::default();
    checks.check(d >= 0.68, format!("the pie is {d:.3} of the frame's height across, at least 0.68 (PowerPoint: 0.70; a fixed 72% of the height for the plot leaves 0.65)"));
    checks.check(centre.abs() <= 0.04, format!("the pie's centre is {centre:+.3} of the frame's height from the frame's middle, within 0.04 (PowerPoint: +0.01; ibisOffice puts it 0.07 above)"));
    checks.done("pptx-C017-b1: the automatic plot area of a pie chart with a title, outside labels and a legend");
}

fn check_doughnut_auto_plot(file: &str) {
    let pages = render(file, "chart-doughnut-auto-plot");
    let page = &pages[0];
    let (x0, y0, w, h) = frame_px(page, rect(4_191_000, 609_600, 3_810_000, 2_768_600));
    let (_, t, _, b) = bbox(page, area_px(page, x0, y0, x0 + w, y0 + h * 86 / 100), saturated).expect("the doughnut is drawn");
    let d = (b - t + 1) as f64 / h as f64;
    let top = (t - y0) as f64 / h as f64;
    eprintln!("{file}: doughnut diameter {d:.3}, top {top:.3} of the frame height");
    let mut checks = Checks::default();
    checks.check(d <= 0.60, format!("the doughnut is {d:.3} of the frame's height across, at most 0.60 (PowerPoint: 0.56; a fixed 72% of the height leaves 0.66)"));
    checks.check(top >= 0.20, format!("the doughnut's top is {top:.3} of the frame's height below its top, at least 0.20 (PowerPoint: 0.23, under the title; ibisOffice 0.12, touching it)"));
    checks.done("pptx-C017-b2: the automatic plot area of a doughnut chart in a low frame");
}

/// pptx-C017-b1・b2（todo に項目なし。P8e の円グラフ側と同じ分岐の続き。b1 と b2 は同じ分岐で向きが逆なので 1 本にまとめた）：
/// 円・ドーナツ・グラフで描画域が自動（`c:plotArea/c:layout` が空）のとき、ibisOffice は題・凡例・外側ラベルの高さを見ず、枠の高さの固定の割合
/// （72% など。chart_view.rs `automatic_plot` の最後の既定）で描く。
///   - b1：大きい枠（509×437pt）の円は、PowerPoint では直径が枠の高さの 0.70 で中心が枠の縦の真ん中。今は 0.65 で、中心が 0.07 上寄り。
///   - b2：低い枠（300×218pt）のドーナツは、PowerPoint では外径が枠の高さの 0.56 で、上端が題の下（枠の上から 0.23）。今は 0.65 で、上端 0.12 が題に触れる。
/// 対照（描画域を正解 PDF から逆算して手指定したもの）は、どちらも正解に近い。
pub(crate) fn corpus_report_pptx_pie_auto_plot_area_follows_title_and_labels() {
    check_pie_auto_plot("chart-pie-auto-plot.pptx");
    check_doughnut_auto_plot("chart-doughnut-auto-plot.pptx");
}

fn check_radar_axis_label_size(file: &str) {
    let pages = render(file, "chart-radar-axis-label-size");
    let page = &pages[0];
    let bands = ink_bands(page, area_px(page, 706, 190, 746, 560), grey_text, 6.0);
    let mut heights: Vec<i64> = bands.iter().map(|(a, b)| b - a + 1).collect();
    heights.sort_unstable();
    eprintln!("{file}: tick label heights {heights:?}");
    assert!(heights.len() >= 4, "the value axis' tick labels (5 to 40) are drawn at the axis: {bands:?}");
    let median = heights[heights.len() / 2];
    assert!(median >= 17, "pptx-C017-b4: the value axis' tick labels (`c:valAx/c:txPr` sz=1197, 12pt) are {median}px high (digits of 12pt at 2px a point: about 19px), not the fixed 8pt's 13px");
}

/// pptx-C017-b4（todo に項目なし）：レーダーチャートの値軸の目盛りの文字（`c:valAx/c:txPr` の `sz`=1197）が、PowerPoint では 12pt（項目名と同じ大きさ）で
/// 軸の左側に右揃えで並ぶ。ibisOffice は 8pt の固定（chart_view.rs の `ChartText { size: 800 }`）で値軸の `txPr` を見ず、軸の右側に寄せる。
/// 目盛りの数字（5〜40）の高さが 17px 以上（12pt は約 19px、8pt は 13px）。対照（24pt）も 8pt のまま描くので、対照の緑の試験は足さない。
pub(crate) fn corpus_report_pptx_radar_axis_labels_follow_the_text_size() {
    check_radar_axis_label_size("chart-radar-axis-label-size.pptx");
}

fn check_category_labels_in_frame(file: &str) {
    let pages = render(file, "chart-category-labels-left-room");
    let page = &pages[0];
    let (x0, y0, _, h) = frame_px(page, rect(508_000, 508_000, 4_927_600, 5_499_100));
    let ink = page.pixels(area_px(page, x0, y0, x0 + 4, y0 + h)).into_iter().filter(|(_, _, c)| c.iter().any(|v| *v < 200)).count();
    assert_eq!(ink, 0, "pptx-C017-b5: the category labels end at the chart frame's left edge (x {x0}): {ink} pixels of text are cut by it (PowerPoint shortens a long label with \"…\" to fit the room left of the plot)");
}

/// pptx-C017-b5（todo に項目なし）：横棒グラフの描画域を手指定（`manualLayout`、`layoutTarget=inner`、x=0.274）して左の余白（枠の 27%）が狭いと、項目名
/// （長い名前の項目）が入りきらない。PowerPoint は長い項目名を「Aldermoor and…」のように「…」で終えて枠に収める（正解 PDF）。
/// ibisOffice は全文を右揃えで描き、左端が枠で切れて「ldermoor and Brookside」のように先頭が欠ける。枠の左端の 4 画素の帯に文字の画素が無い。
/// 対照（左の余白を枠の 45% に広げたもの）は全部が収まる。
pub(crate) fn corpus_report_pptx_category_labels_stay_inside_the_frame() {
    check_category_labels_in_frame("chart-category-labels-left-room.pptx");
}

fn check_legend_wraps(file: &str) {
    let pages = render(file, "chart-legend-wraps-12pt");
    let page = &pages[0];
    let rows = ink_bands(page, area_px(page, 60, 750, 1126, 846), grey_text, 8.0);
    eprintln!("{file}: legend rows {rows:?}");
    assert!(rows.len() >= 2, "pptx-C017-b6: the 10 legend entries (12pt, a legend box of 448pt × 36pt) wrap to 2 rows of 5 (PowerPoint), not 1 row that runs past the box with the texts on one another: {} row(s) {rows:?}", rows.len());
}

/// pptx-C017-b6（todo に項目なし）：凡例を手指定（`c:manualLayout`、幅 448pt・高さ 36pt）し、文字が 12pt・系列が 10 本のとき、PowerPoint は 2 段（5 つずつ）に折り返す。
/// ibisOffice は「並びの幅が箱の幅の 1.15 倍以内なら 1 段のまま」（chart_view.rs の `row_least > area.w * 1.15`。代わりの書体が少し広いための余裕）で
/// 1 段に押し込み、「Decile 1Decile 2…」と文字が重なって右端で切れる。凡例の文字の段が 2 つ以上。対照（文字を 10pt）は 2 段。
pub(crate) fn corpus_report_pptx_legend_wraps_when_the_row_is_too_wide() {
    check_legend_wraps("chart-legend-wraps-12pt.pptx");
}

fn check_long_title_wraps(file: &str) {
    let pages = render(file, "chart-long-title-wraps");
    let page = &pages[0];
    let (x0, y0, w, _) = frame_px(page, rect(1_270_000, 508_000, 6_286_500, 4_457_700));
    let lines = ink_bands(page, area_px(page, x0, y0, x0 + w, y0 + 120), very_dark, 8.0);
    eprintln!("{file}: title lines {lines:?}");
    assert!(lines.len() >= 2, "pptx-C017-b8: the long title (126 characters, positioned by `manualLayout` x, y only) wraps at the frame's width (3 lines in PowerPoint), not one line cut at the frame's right edge: {} line(s) {lines:?}", lines.len());
}

/// pptx-C017-b8（todo に項目なし）：長い題（126 文字、位置は手指定の x・y だけで幅の指定なし）を、PowerPoint は枠の幅で 3 行に折り返し、描画域は題の下から始める。
/// ibisOffice は題を 1 つのラベルにして 1 行で描き（chart_view.rs `title_label`）、右端が枠で切れ、描画域は上へ伸びて値軸の上の目盛りと題が重なる。
/// 題の上端の帯（枠の上 60pt）の黒い文字の行が 2 つ以上。対照（短い題）は 1 行で崩れない（期待が 1 行なので緑の試験は足さない）。
pub(crate) fn corpus_report_pptx_long_chart_title_wraps_at_the_frame_width() {
    check_long_title_wraps("chart-long-title-wraps.pptx");
}

fn check_data_table_height(file: &str) {
    let pages = render(file, "chart-data-table");
    let page = &pages[0];
    let (x0, y0, w, h) = frame_px(page, rect(381_000, 508_000, 8_229_600, 4_699_000));
    let ink = |c: Rgb| c.iter().all(|v| (120..236).contains(v)) && spread(c) < 14;
    let rows: Vec<i64> = (y0 + h * 80 / 100..y0 + h)
        .filter(|y| (x0..x0 + w).filter(|x| page.pixel(*x, *y).is_some_and(ink)).count() as i64 >= w * 6 / 10)
        .collect();
    eprintln!("{file}: rule rows {rows:?}");
    let span = rows.last().zip(rows.first()).map(|(l, f)| l - f).unwrap_or(0);
    assert!(span >= 80, "pptx-C017-b7: the data table (`c:dTable`: a row of the category names and one row for each of the 2 series) is at least 80px high from its first rule to its last (about 32px a row for 12pt text); ibisOffice squeezes it into {span}px: rules at {rows:?}");
}

/// pptx-C017-b7（todo に項目なし）：グラフのデータ表（`c:dTable`、`showKeys=1`、罫線あり）を、PowerPoint は軸の下に項目名の行と系列ごとの行（各行 22px 前後 = 1920px 幅で約 32px）の
/// 整列した表にする。ibisOffice は表の分の高さを描画域から引かず（手指定の inner では枠の下の余白だけ）、表が約 60px に縮み、項目名の行が軸の文字と重なり、
/// 系列の行の数値が 1 つ上にずれる（chart_view.rs `draw_data_table`）。最初の罫線から最後の罫線までが 80px 以上（今は約 60px）。
/// 対照（`c:dTable` を外したもの）は表を持たないので緑の試験は足さない。しきい値は正解 PDF の行の高さ（22px、幅 1334px 想定）からの見積もり。
pub(crate) fn corpus_report_pptx_chart_data_table_keeps_its_row_height() {
    check_data_table_height("chart-data-table.pptx");
}

fn lum(c: Rgb) -> i32 {
    c[0] + c[1] + c[2]
}

fn check_just_line_before_br(file: &str, stretched: bool) {
    let pages = render(file, "just-line-before-br");
    let page = &pages[0];
    let lines = ink_bands(page, area_px(page, 200, 200, 1000, 600), dark, 8.0);
    assert!(lines.len() >= 2, "the text's lines are drawn: {lines:?}");
    let (_, right) = extent(page, area_px(page, 200, lines[1].0, 1000, lines[1].1 + 1), dark).unwrap();
    eprintln!("{file}: the second line ends at x {right} (the box's text edge is x 986)");
    if stretched {
        assert!(right >= 960, "pptx-C018-b1: the line before an `a:br` of a `algn=\"just\"` paragraph (\"なにぬねのを開く\") is spread to the box's width (text edge x 986): it ends at x {right}, at least 960 (left-packed it ends near 430)");
    } else {
        assert!(right <= 700, "a paragraph's last line is not spread: the second line ends at x {right}, at most 700");
    }
}

/// pptx-C018-b1（todo に項目なし）：両端揃え（`algn="just"`）の段落で、`a:br`（Shift+Enter の改行）の直前の行を、PowerPoint は
/// 箱の幅いっぱいに割り付ける（段落の終わりの行だけが割り付けられない）。ibisOffice は `a:br` を段落の区切りと同じに描き（rich_text.rs の
/// `TextAlign::Justify => n + 1 < line_count`）、`a:br` の直前の行も左に詰める。2 行目「なにぬねのを開く」（`a:br` の直前）の右端が箱の文字の右端（x 986）の近く（960 以上）。
/// 今は左に詰まって x 430 付近。対照（`a:br` を段落の区切りに替えたもの）は割り付けない（右端が 700 以下）。
pub(crate) fn corpus_report_pptx_just_line_before_br_is_spread() {
    check_just_line_before_br("just-line-before-br.pptx", true);
}

/// For the first two lines of the text of `just-latin-word-spacing`: how many gaps of 9px or more lie between the ink runs of a line (a gap
/// between words is about 14px, one between the letters of a word under 6px), and where the first line ends.
fn wide_gaps(file: &str, label: &str) -> (Vec<usize>, i64) {
    let pages = render(file, label);
    let page = &pages[0];
    let lines = ink_bands(page, area_px(page, 200, 200, 1320, 720), dark, 8.0);
    assert!(lines.len() >= 2, "the text's lines are drawn: {lines:?}");
    let counts: Vec<usize> = lines[..2]
        .iter()
        .map(|(a, b)| {
            let runs = ink_runs(page, area_px(page, 200, *a, 1320, *b + 1), dark);
            runs.windows(2).filter(|w| w[1].0 - w[0].1 - 1 >= 9).count()
        })
        .collect();
    let (_, right) = extent(page, area_px(page, 200, lines[0].0, 1320, lines[0].1 + 1), dark).unwrap();
    (counts, right)
}

/// pptx-C018-b2（todo に項目なし）：両端揃え（`algn="just"`）の欧文の段落で、PowerPoint は語と語の間（空白）だけを広げ、語の中の字間は
/// 変えない。ibisOffice は余りを全部の文字の間に均等に足す（rich_text.rs の `extra = (room - shown_width) / (shown_chars - 1)` を全文字に）ので、
/// 「m a t e r i a l」のように字が離れる。1・2 行目で、字と字の間（インクの塊の間）が 9px 以上開いている所の数が、語の数から 1 を引いた数（5 と 4）に収まる（8 以下。今は 20 以上）。
/// 対照（左揃えで字間が自然なもの）は 5 と 4。
pub(crate) fn corpus_report_pptx_just_latin_spreads_words_not_letters() {
    let (counts, _) = wide_gaps("just-latin-word-spacing.pptx", "just-latin-word-spacing");
    eprintln!("gaps of 9px or more: just {counts:?}");
    let mut checks = Checks::default();
    for (i, n) in counts.iter().enumerate() {
        checks.check(*n <= 8, format!("line {}: {n} gaps of 9px or more between the ink runs, at most 8 (one between each two of the 6 or 5 words): the letters inside the words are spread too", i + 1));
    }
    checks.done("pptx-C018-b2: a justified Latin line spreads the spaces between words, not the letters of a word");
}

fn ink_width_and_mass(page: &Page, area: EmuRect) -> (f64, f64) {
    let (l, r) = extent(page, area, |c| c.iter().all(|v| *v < 110)).expect("the line is drawn");
    ((r - l + 1) as f64, ink_mass(page, area))
}

/// pptx-C019-b1（todo に項目なし。P7b・P7c とは別）：run の書体のうち、その文字の種類（欧文は latin、和文は ea）の書体が Mac に無く、
/// もう一方の書体が入っているとき、ibisOffice は入っていない書体を run のもう一方の書体に譲る（shape_view.rs `Fonts::font`）。
///   - A：latin=Avenir Roman（無い）・ea=Times New Roman（有る）の欧文は、PowerPoint ではセリフの無い太字（Windows の代わり）。ibisOffice は Times New Roman の太字（セリフ体）にする。
///   - B：和文は ea=HGPｺﾞｼｯｸE（無い）・latin=游明朝（有る）。PowerPoint は太いゴシック体。ibisOffice は細い明朝体にする。
/// 対照（A は ea を、B は latin を書かない）は、A が Arial の太字、B が Hiragino Sans 系のゴシック体。A の幅と B の幅・インクの量が対照と同じ（A は ±4%、B は幅 ±4%・インク ±12%）。
pub(crate) fn corpus_report_pptx_missing_font_does_not_defer_to_the_other_scripts_font() {
    let min = render("other-script-font.pptx", "other-script-font");
    let ctrl = render("other-script-font-ctrl.pptx", "other-script-font-ctrl");
    let a = |page: &Page| ink_width_and_mass(page, area_px(page, 88, 100, 1832, 244));
    let b = |page: &Page| ink_width_and_mass(page, area_px(page, 88, 380, 1832, 544));
    let (am, ac) = (a(&min[0]), a(&ctrl[0]));
    let (bm, bc) = (b(&min[0]), b(&ctrl[0]));
    eprintln!("A: width {:.0} vs {:.0}, mass {:.0} vs {:.0}; B: width {:.0} vs {:.0}, mass {:.0} vs {:.0}", am.0, ac.0, am.1, ac.1, bm.0, bc.0, bm.1, bc.1);
    let mut checks = Checks::default();
    checks.check((am.0 / ac.0 - 1.0).abs() <= 0.04, format!("A (latin Avenir Roman, ea Times New Roman): the line is {:.0}px wide, the control's sans-serif bold {:.0}px (within 4%): drawn in the serif Times New Roman instead", am.0, ac.0));
    checks.check((bm.0 / bc.0 - 1.0).abs() <= 0.04 && (bm.1 / bc.1 - 1.0).abs() <= 0.12, format!("B (ea HGPｺﾞｼｯｸE, latin 游明朝): width {:.0}px and ink {:.0} against the control's gothic {:.0}px and {:.0} (within 4% and 12%): drawn in a thin mincho instead", bm.0, bm.1, bc.0, bc.1));
    checks.done("pptx-C019-b1: a font the Mac lacks defers to the other script's font");
}

/// pptx-C019-b2（todo に項目なし。P7c と同じ関数・同じ原因の仲間だが、幅でなく字形の種類）：欧文の書体名がセリフ体（Goudy Old Style・Merriweather）で Mac に入っていないとき、
/// ibisOffice は入っていない欧文の書体を一律に Arial（セリフ無し）にする（shape_view.rs `Fonts::family` の最後）。PowerPoint（Windows）はセリフ体で描く（正解 PDF）。
/// 対照（書体名を Times New Roman にしたもの）はセリフ体。A・B の行の幅が対照（Times New Roman）の ±6% に収まる（Arial の太字は約 +12% 広い）。
pub(crate) fn corpus_report_pptx_serif_latin_font_is_not_drawn_in_arial() {
    let min = render("serif-latin-to-sans.pptx", "serif-latin-to-sans");
    let ctrl = render("serif-latin-to-sans-ctrl.pptx", "serif-latin-to-sans-ctrl");
    let a = |page: &Page| ink_width_and_mass(page, area_px(page, 88, 100, 1832, 264));
    let b = |page: &Page| ink_width_and_mass(page, area_px(page, 88, 400, 1832, 504));
    let (am, ac) = (a(&min[0]), a(&ctrl[0]));
    let (bm, bc) = (b(&min[0]), b(&ctrl[0]));
    eprintln!("A: width {:.0} vs {:.0}, mass {:.0} vs {:.0}; B: width {:.0} vs {:.0}, mass {:.0} vs {:.0}", am.0, ac.0, am.1, ac.1, bm.0, bc.0, bm.1, bc.1);
    let mut checks = Checks::default();
    checks.check((am.0 / ac.0 - 1.0).abs() <= 0.06, format!("A (Goudy Old Style): the line is {:.0}px wide, a serif face like the control's Times New Roman ({:.0}px) within 6%: it is Arial's width", am.0, ac.0));
    checks.check((bm.0 / bc.0 - 1.0).abs() <= 0.06, format!("B (Merriweather): the line is {:.0}px wide, a serif face like the control's Times New Roman ({:.0}px) within 6%: it is Arial's width", bm.0, bc.0));
    checks.done("pptx-C019-b2: a Latin serif font the Mac lacks is drawn in a serif face");
}

fn check_line_gradient(file: &str) {
    let pages = render(file, "line-gradient");
    let page = &pages[0];
    let vertical = page.at(emu_at(page, 600), emu_at(page, 560));
    let horizontal = page.at(emu_at(page, 760), emu_at(page, 300));
    eprintln!("{file}: the vertical bar {vertical:?}, the horizontal bar {horizontal:?}");
    assert!(lum(horizontal) <= lum(vertical) - 40, "pptx-C020-b1: the line's `a:gradFill` (135°, dark at the top right) paints the horizontal bar (top, next to the arrow) darker than the vertical bar (bottom): the horizontal bar {horizontal:?}, the vertical bar {vertical:?}");
}

/// pptx-C020-b1（todo に項目なし）：図形の輪郭線（`a:ln`）が `a:gradFill`（`a:lin ang=8100000`、135°、右上が濃く左下が淡い）の折れ線（左下から上へ、右へ）。
/// PowerPoint は外接箱に 135° で掛けるので、縦棒（下）は淡く、右へ向かう横棒（矢印の側）は濃い。ibisOffice は辺ごとに経路の長さの位置で単色にし（shape_view.rs の
/// `stroke.fill` の扱い）、`ang`・外接箱・向きを使わず、経路の始点を濃・終点を淡として割り付けるので逆になる（縦棒が濃く、横棒が淡い）。横棒の色が縦棒より明るさの和で 40 以上暗い。
/// 対照（同じ線を逆向きの経路にしたもの。PowerPoint では同じ見た目）は横棒が濃い。
pub(crate) fn corpus_report_pptx_line_gradient_follows_the_angle_not_the_path() {
    check_line_gradient("line-gradient.pptx");
}

fn check_cmyk_icc(file: &str) {
    let pages = render(file, "cmyk-icc-jpeg");
    let page = &pages[0];
    let frame = rect(2_032_000, 1_524_000, 7_620_000, 5_080_000);
    let mut checks = Checks::default();
    for (fx, want, tolerance, what) in [(1.0 / 6.0, [95, 95, 150], 18, "top left #5F5F96 (greyish purple)"), (0.5, [255, 215, 225], 14, "top middle #FFD7E1 (pale pink)"), (5.0 / 6.0, [60, 120, 200], 18, "top right #3C78C8 (blue)")] {
        let c = page.in_frame(frame, fx, 0.25);
        eprintln!("{file}: {what}: {c:?}");
        checks.check(near(c, want, tolerance), format!("the swatch {what}: {want:?} (±{tolerance}), drawn {c:?}"));
    }
    checks.done("pptx-C020-b2: a CMYK JPEG with an ICC profile");
}

/// pptx-C020-b2（todo に項目なし。P5a の clrChange・回転とは別）：4 成分（CMYK）の JPEG（埋め込みの ICC プロファイル付き）を、ibisOffice は
/// 素朴な変換（image クレートの `to_rgba8`。render/preload.rs `cmyk_jpeg_as_png`）で RGB にして ICC プロファイルを使わない。PowerPoint は ICC で sRGB に変換する
/// （正解 PDF の色）。3×2 の色見本の上段は、左 #5F5F96（灰みの紫）・中 #FFD7E1（淡い桃）・右 #3C78C8。今は左が鮮やかな青 #4C5CED、右が #488BFE。
/// 対照（同じ絵の RGB の JPEG）は元の色どおり。
pub(crate) fn corpus_report_pptx_cmyk_jpeg_uses_its_icc_profile() {
    check_cmyk_icc("cmyk-icc-jpeg.pptx");
}

fn check_theme_gradient_tint(file: &str) {
    let pages = render(file, "theme-gradient-tint");
    let page = &pages[0];
    let frame = rect(1_016_000, 762_000, 10_160_000, 5_588_000);
    let bottom = page.in_frame(frame, 0.5, 0.96);
    eprintln!("{file}: the bottom of the gradient {bottom:?}");
    assert!(near(bottom, [255, 191, 136], 20), "pptx-C020-b3: the bottom of the rectangle (`fillRef idx=2`: the theme gradient with tint 15% and satMod 350% on accent6 F79646) is orange #FFBF88 (±20), drawn {bottom:?} (a pale pink: the tint is mixed in linear light)");
}

/// pptx-C020-b3（todo に項目なし）：図形が `p:style/a:fillRef idx="2"` だけで塗られ、テーマの `fillStyleLst` の 2 番目が Office 2007 型の
/// グラデーション（accent6 F79646 に `tint 50%/37%/15%`・`satMod 300%/350%`、下から上）のとき、PowerPoint は下ほど濃い橙（下端 #FFBF88、上端 #FFEADA）。
/// ibisOffice は `tint` を線形光で白へ寄せる（ibis-pptx/src/theme.rs の `map_linear`）ので全体が淡い桃色（下端 #FFD4C0）になる。PowerPoint の正確な式は未特定なので、
/// 期待は正解 PDF の下端の色 #FFBF88（各 ±20）だけにした。対照（正解 PDF の色を sRGB で並べたもの）は #FFBE8B。
pub(crate) fn corpus_report_pptx_theme_gradient_tint_is_not_pale() {
    check_theme_gradient_tint("theme-gradient-tint.pptx");
}

fn check_text_glow(file: &str) {
    let pages = render(file, "text-glow-large");
    let page = &pages[0];
    let area = area_px(page, 700, 400, 1060, 600);
    let text = bbox(page, area, |c| c.iter().all(|v| *v > 235)).expect("the white text is drawn");
    let blue = |c: Rgb| c[0] < 90 && c[2] > 150 && c[2] - c[0] > 90;
    let reach = bbox(page, area, blue).map(|b| (text.0 - b.0).max(text.1 - b.1).max(b.2 - text.2).max(b.3 - text.3)).unwrap_or(0);
    eprintln!("{file}: the text's box {text:?}, the blue reaches {reach}px beyond it");
    assert!(reach <= 14, "pptx-C020-b4: the 20pt glow (`a:glow rad=\"254000\"`, #0033CC, opaque) of the white 16pt text is a thin dark-blue rim (2 to 4px at 2px a point; the text stays legible): the blue reaches {reach}px past the text's box, at most 14 (ibisOffice paints a blotch up to 40px wide)");
}

/// pptx-C020-b4（todo に項目なし。P6e の 8pt・60% の光彩は直っている）：白い 16pt の太字の文字に大きい光彩（`a:rPr/a:effectLst/a:glow rad="254000"`＝20pt、
/// 0033CC、透明度なし）。PowerPoint は文字の輪郭に沿った濃い青の細いふち（2〜4px）で、吹き出しの水色はそのまま見え、文字が読める。ibisOffice は光彩を文字ごとに
/// 字形を重ねて描き（shape_view.rs の `r.style.glow`、rich_text.rs）、半径 20pt でまだらな青い塊になって文字の周り 40px 近くへ広がる。青い画素の外接が白い文字の外接から 14px 以内。
/// 対照（`a:glow` を外したもの）は光彩が無いので緑の試験は足さない。
pub(crate) fn corpus_report_pptx_large_text_glow_is_a_thin_rim() {
    check_text_glow("text-glow-large.pptx");
}

fn check_shape_glow_nofill(file: &str) {
    let pages = render(file, "shape-glow-nofill");
    let page = &pages[0];
    let ground = page.at(emu_at(page, 120), emu_at(page, 300));
    let frame = rect(1_270_000, 2_540_000, 9_652_000, 1_397_000);
    let mut checks = Checks::default();
    for (fx, fy) in [(0.03, 0.5), (0.97, 0.5), (0.5, 0.08), (0.5, 0.92)] {
        let c = page.in_frame(frame, fx, fy);
        eprintln!("{file}: ({fx},{fy}) {c:?}, the ground {ground:?}");
        checks.check(near(c, ground, 20), format!("({fx}, {fy}) of the rectangle: the dark ground {ground:?} (±20), drawn {c:?}"));
    }
    checks.done("pptx-C020-b5: a rectangle with no fill and no line draws no glow");
}

/// pptx-C020-b5（todo に項目なし）：塗りも線も無い図形（p:style なし）に `a:effectLst/a:glow rad="127000"`（10pt、accent1）があるとき、PowerPoint は何も描かない
/// （文字だけ見える）。ibisOffice は光彩を `auto.fill` を見ずに図形の箱を広げて box-shadow で描き（shape_view.rs の glow。影のほうは塗りがあるときだけ）、箱の範囲ぜんたいが
/// 明るい青 #4F81BD 系のぼけた箱になる。暗い地の上の箱の四辺の内側の点が地の色（±20）のまま。対照（`a:glow` を外したもの）は箱が出ない。
pub(crate) fn corpus_report_pptx_glow_of_a_shape_without_fill_draws_nothing() {
    check_shape_glow_nofill("shape-glow-nofill.pptx");
}

fn check_callout_tail_filled(file: &str) {
    let pages = render(file, "callout-tail-gradient");
    let page = &pages[0];
    let c = page.at(emu_at(page, 800), emu_at(page, 560));
    eprintln!("{file}: inside the tail {c:?}");
    assert!(c.iter().any(|v| *v < 235), "pptx-C021-b2: the tail of the callout (outside the shape's frame) is filled with the gradient's colour like the body, not left white: {c:?}");
}

/// pptx-C021-b2（todo に項目なし）：吹き出し（`wedgeRoundRectCallout`）の尾が図形の枠（`a:ext`）の外へ出る大きさの調整値（adj2=-169614 = 枠の高さの 1.7 倍だけ上）で、塗りが
/// 縦方向の線形グラデーション（`a:lin ang=16200000`）のとき、PowerPoint は尾を本体と同じ色（橙のグラデーションの続き）で塗る。ibisOffice は本体の枠の中だけを塗り、尾は塗りなしで
/// 輪郭の細い線だけになる（横のグラデーション（ang=0）では塗られる）。尾の内側の点が白でない。対照（塗りを単色にしたもの）は尾も塗られる。
pub(crate) fn corpus_report_pptx_callout_tail_outside_the_frame_gets_the_gradient() {
    check_callout_tail_filled("callout-tail-gradient.pptx");
}

fn check_accent_callout_bar_end(file: &str) {
    let pages = render(file, "accent-callout-arrow");
    let page = &pages[0];
    let widest = (826..850).map(|y| (820..876).filter(|x| page.pixel(*x, y).is_some_and(dark)).count()).max().unwrap_or(0);
    eprintln!("{file}: the widest row of ink near the accent bar's lower end: {widest}px");
    assert!(widest <= 14, "pptx-C021-b3: the accent bar of `accentBorderCallout2` has no arrowhead at its end (only the leader line has one): {widest}px of ink in one row near the bar's lower end, at most 14 (the bar is 8px wide; an arrowhead is 40px)");
}

/// pptx-C021-b3（todo に項目なし）：`accentBorderCallout2`（accent バーのある吹き出し線。accentCallout1〜3・accentBorderCallout1〜3 も同じ）の線（`a:ln`）に
/// 矢じり（`a:tailEnd type="triangle"`）が付くとき、PowerPoint は引き出し線の先の 1 つだけに矢じりを付け、縦のアクセントバーの端には付けない。ibisOffice は開いた path の
/// すべてに矢じりを付け、バー（縦線）の下端にも下向きの矢じり（幅 40px）が出る。バーの下端の少し上の 1 行のインクの幅が 14px 以下（バーは 8px）。
/// 対照は型が `borderCallout2`（バーの無い型）なので、バーの無い見え方の確かめにならず緑の試験は足さない。
pub(crate) fn corpus_report_pptx_accent_callout_bar_has_no_arrowhead() {
    check_accent_callout_bar_end("accent-callout-arrow.pptx");
}

fn check_gradient_line(file: &str) {
    let pages = render(file, "gradient-line");
    let page = &pages[0];
    let y = 4_800_000;
    let (x0, x1) = (1_285_973, 1_285_973 + 9_637_203);
    let left = page.at(x0 + (x1 - x0) / 20, y);
    let right = page.at(x1 - (x1 - x0) / 20, y);
    eprintln!("{file}: the lower line's left {left:?}, right {right:?}");
    assert!(left[0] - right[0] >= 60, "pptx-C021-b4: the lower line (`a:ln` with a `a:gradFill`: orange FF9933 to blue) changes colour along its length: left {left:?}, right {right:?} (the red channel falls by at least 60); ibisOffice draws the first stop's colour all along");
}

/// pptx-C021-b4（todo に項目なし）：線（`p:cxnSp` の `a:ln`）の塗りが `a:gradFill`（橙 FF9933 から青の 2 色、`a:lin ang=0`）のとき、PowerPoint は線に沿って橙から
/// 青へ変わる（元文書では橙から透明へ薄れる帯）。ibisOffice は先頭の停止位置の色だけを線の色にし（reader.rs の `stroke`）、線全体が橙一色になる。
/// 下の線の左端近くと右端近くの色で、赤の値が 60 以上下がる（今は同じ）。対照（単色の橙）は変わらないので緑の試験は足さない。
pub(crate) fn corpus_report_pptx_connector_gradient_line_changes_colour() {
    check_gradient_line("gradient-line.pptx");
}

fn check_growing_gridspan(file: &str) {
    let pages = render(file, "table-growing-gridspan");
    let page = &pages[0];
    let lines = ink_bands(page, area_px(page, 535, 215, 645, 380), dark, 5.0);
    eprintln!("{file}: the last cell's text lines {lines:?}");
    assert_eq!(lines.len(), 1, "pptx-C022-b1: \"400万円\" (right of a `gridSpan=4` cell, in a table whose frame is taller than its rows) is on one line, not one character a line: {lines:?}");
}

/// pptx-C022-b1（todo に項目なし）：`a:graphicFrame` の高さが行の高さの合計より 1% を超えて大きい表（ibisOffice は growing_table の経路）の
/// 最後の行に `gridSpan=4` のセル（「合計」）があり、その右にセル（「400万円」）が続くと、PowerPoint は「400万円」を 1 行で出す。ibisOffice は `hMerge` のセルでも列の番号を進める
/// （table_view.rs `growing_table` の `col += span`）ので、右端のセルの幅が 0 になり、1 文字ごとに折り返して縦に 5 行（4・0・0・万・円）になり、表が下へ伸びる。
/// 最終行の右のセルの文字の行が 1 つ。対照（枠の高さ＝行の高さの合計。固定の表の経路）は 1 行。
pub(crate) fn corpus_report_pptx_table_cell_after_a_grid_span_keeps_its_width() {
    check_growing_gridspan("table-growing-gridspan.pptx");
}

fn check_zero_height_row(file: &str) {
    let pages = render(file, "table-zero-height-row");
    let page = &pages[0];
    let lines = ink_bands(page, area_px(page, 566, 120, 664, 230), dark, 6.0);
    eprintln!("{file}: the header's text lines {lines:?}");
    assert!(lines.len() >= 3, "pptx-C022-b2: the second header row (`a:tr h=\"0\"`, rowSpan cells above, the frame taller than the rows) is as high as its 2 lines of text, under the first header row's \"取組み\": {} line(s) {lines:?}, at least 3 (the text of the two rows overlaps)", lines.len());
}

/// pptx-C022-b2（todo に項目なし。pptx-C016-b2 の `h=0` の行とは経路が違う）：`rowSpan` のあるセルと `a:tr h="0"` の行があり、行の高さの合計が枠の高さより
/// 1% を超えて小さい表で、PowerPoint は文字に合わせて広げた高さ（枠の `cy` にだけ保存される）で 2 段目の行を出し、2 行の文字が読める。ibisOffice は `h=0` の行を 0 のまま
/// 他の行へ比例配分する（ibis-doc-model/src/table.rs `row_heights`、table_view.rs で rowSpan のある表は固定の表）ので、2 段目の行が潰れて「甲乙丙丁戊己庚」が見出しの「取組み」に重なり、
/// データ行が高くなる。見出しの文字の行（「取組み」と 2 段目の 2 行）が 3 つ以上。対照（2 段目の行の `h` を広げた高さにしたもの）は 2 行で読める。
pub(crate) fn corpus_report_pptx_zero_height_row_takes_the_height_of_its_text() {
    check_zero_height_row("table-zero-height-row.pptx");
}

/// The chart frame of the pptx-C024 documents (the same in all): 1680 × 880 pixels from (120, 120).
const C24_FRAME: (i64, i64, i64, i64) = (120, 120, 1680, 880);

fn check_series_lines(file: &str) {
    let pages = render(file, "chart-bar-serlines");
    let page = &pages[0];
    let rows = ink_bands(page, area_px(page, 722, 150, 728, 955), |c| c.iter().all(|v| *v < 235), 1.0);
    eprintln!("{file}: marks in the gap between the first two bars {rows:?}");
    assert!(rows.len() >= 2, "pptx-C024-b6: the series lines (`c:serLines` of a percentStacked bar chart) join the same boundaries of neighbouring bars: {} line(s) cross the gap between the first two bars, at least 2 (one for each boundary between the 3 series)", rows.len());
}

/// pptx-C024-b6（todo に項目なし）：積み上げ（`stacked`・`percentStacked`）の棒グラフの `<c:serLines>`（系列の連結線）を、PowerPoint は隣り合う
/// 棒の同じ系列の境目どうしを結ぶ細い灰色の線（0.75pt）として描く。ibisOffice は `serLines` を of-pie の `connector` にしか読まず（ibis-pptx/src/chart.rs）、棒には読む所も描く所も無い。
/// 1 本目と 2 本目の棒の間の帯を横切る線が 2 本以上（3 系列の境目は 2 つ）。今は 0。対照（`serLines` を外したもの）は同じ描画（要素を無視している）なので緑の試験は足さない。
pub(crate) fn corpus_report_pptx_stacked_bar_series_lines_are_drawn() {
    check_series_lines("chart-bar-serlines.pptx");
}

fn check_custom_error_bars(file: &str) {
    let pages = render(file, "chart-errbars-custom");
    let page = &pages[0];
    let (x0, y0, w, h) = C24_FRAME;
    let mut longest = 0;
    for x in x0..x0 + w - 200 {
        let mut run = 0;
        for y in y0 + 30..y0 + h - 100 {
            if page.pixel(x, y).is_some_and(|c| c.iter().all(|v| *v < 140) && spread(c) < 20) {
                run += 1;
                longest = longest.max(run);
            } else {
                run = 0;
            }
        }
    }
    eprintln!("{file}: the longest dark vertical line is {longest}px");
    assert!(longest <= 80, "pptx-C024-b7: the custom error bars (`errValType=\"cust\"`, ±0.006 to ±0.018 on values near 0.7 of an axis to 0.9) are short (about 10px); a dark vertical line {longest}px long crosses the plot (ibisOffice draws ±1.0 of the axis)");
}

/// pptx-C024-b7（todo に項目なし）：誤差線が `errValType="cust"` と `c:plus`・`c:minus`（numRef の numCache に点ごとの値、ここでは 0.006〜0.018）のとき、PowerPoint は
/// 各棒の上に短い誤差線（軸 0〜90% で 5〜16px）を引く。ibisOffice は `c:val` しか読まず（ibis-pptx/src/chart.rs の誤差線）、`cust` は 1.0 で描いて紙面を貫く長い縦線になる。
/// 暗い縦線の最長が 80px 以下（今は約 800px）。対照（`fixedVal` 0.01）は短い誤差線になる（位置のずれは b11 の別のバグ）ので、緑の試験は b11 に合わせて足さない。
pub(crate) fn corpus_report_pptx_custom_error_bars_use_their_values() {
    check_custom_error_bars("chart-errbars-custom.pptx");
}

fn check_trendline_span(file: &str) {
    let pages = render(file, "chart-trendline-unsorted");
    let page = &pages[0];
    let (x0, y0, w, h) = C24_FRAME;
    let columns = (x0 + 100..x0 + w - 60).filter(|x| (y0 + 20..y0 + h - 110).any(|y| page.pixel(*x, y).is_some_and(|c| c.iter().any(|v| *v < 190)))).count();
    eprintln!("{file}: {columns} columns with ink in the plot");
    assert!(columns >= 600, "pptx-C024-b8: the linear trendline of a scatter series whose x values are not in order runs from the smallest x (10) to the largest (1320): {columns} pixel columns of the plot hold ink, at least 600 (the 12 markers alone cover about 80, the line from x 1060 to 1320 about 180)");
}

/// pptx-C024-b8（todo に項目なし）：散布図の系列に近似直線（`c:trendline`、linear）があり、x の値が昇順に並んでいないとき、PowerPoint は x の最小（10）から
/// 最大（1320）までの全域に直線を引く。ibisOffice は最初の点と最後の点の x（1320 と 1060）を最小・最大に使い（chart_view.rs `draw_trendline` の `valid_points.first()`・`.last()`）、
/// 1060〜1320 の短い線だけを引く。プロットの中でインクのある画素の列が 600 以上（直線が全域を通れば約 900、今は約 250）。対照（同じ点を x の昇順にしたもの）は全域に引かれる。
pub(crate) fn corpus_report_pptx_trendline_spans_the_whole_x_range() {
    check_trendline_span("chart-trendline-unsorted.pptx");
}

fn check_secondary_deleted_axis(file: &str) {
    let pages = render(file, "chart-secondary-deleted-axis");
    let page = &pages[0];
    let (x0, y0, w, h) = C24_FRAME;
    let (_, t, _, b) = bbox(page, area_px(page, x0, y0, x0 + w, y0 + h - 60), |c| near(c, [0, 112, 192], 30)).expect("the blue area is drawn");
    eprintln!("{file}: the blue area is {}px high", b - t + 1);
    assert!(b - t + 1 <= 480, "pptx-C024-b9: the blue area (10 to 20; a secondary axis with `c:delete val=\"1\"` draws on the primary axis' scale 0 to 45) is at most 480px high, under the pink stack (20 to 40); ibisOffice stretches it to the pink's top: {}px", b - t + 1);
}

/// pptx-C024-b9（todo に項目なし）：面グラフで二次軸のグループの値軸が `c:delete val="1"`（非表示）のとき、PowerPoint は二次軸の系列を一次軸の目盛りで描く
/// （正解 PDF の青は桃色の 1,116/2,023 = 0.552 倍の高さ）。ibisOffice は非表示の二次軸を自動の範囲で別に伸ばし、青が桃色と同じ最上端まで伸びて桃色が隠れる。
/// 一次軸の積み上げ面は 20〜40、二次軸（delete=1）の青の面は 10〜20。青の高さが 480px 以下（一次軸の 0〜45 で 20 は約 360px。今は約 650px）。対照（青を一次軸に置いたもの）は期待どおり。
pub(crate) fn corpus_report_pptx_deleted_secondary_axis_uses_the_primary_scale() {
    check_secondary_deleted_axis("chart-secondary-deleted-axis.pptx");
}

fn check_scatter_legend_key(file: &str) {
    let pages = render(file, "chart-scatter-legend-key");
    let page = &pages[0];
    let (x0, y0, w, h) = C24_FRAME;
    let keys = ink_runs(page, area_px(page, x0, y0 + h - 60, x0 + w, y0 + h), saturated);
    eprintln!("{file}: coloured marks in the legend {keys:?}");
    assert!(keys.len() >= 3, "pptx-C024-b10: each of the 3 legend entries of a scatter series with no line (`a:ln` noFill) and a circle marker shows its marker in the series' colour: {} coloured mark(s) in the legend, at least 3", keys.len());
}

/// pptx-C024-b10（todo に項目なし）：散布図の系列が線 noFill と円の `c:marker` を持つとき、PowerPoint の凡例は色付きの丸と系列名（Series 1〜3）を並べる。
/// ibisOffice は凡例の記号の枠を `s.stroke.is_some()` のときだけ作り（chart_view.rs の凡例）、マーカーだけの系列は文字だけで色も付かない。凡例の帯の色付きの記号が 3 つ以上
/// （今は 0）。対照（系列に線を付けたもの）は線と丸の記号が出る。
pub(crate) fn corpus_report_pptx_scatter_legend_shows_marker_keys() {
    check_scatter_legend_key("chart-scatter-legend-key.pptx");
}

fn check_error_bar_position(file: &str) {
    let pages = render(file, "chart-errbars-bar-position");
    let page = &pages[0];
    let (x0, y0, w, h) = C24_FRAME;
    let plot = area_px(page, x0 + 20, y0 + 20, x0 + w - 20, y0 + h - 130);
    let bar = |c: Rgb| near(c, [189, 215, 238], 14);
    let bars = ink_runs(page, area_px(page, x0 + 20, y0 + h - 80, x0 + w - 20, y0 + h - 79), bar);
    let lines = ink_runs(page, plot, |c| c.iter().all(|v| *v < 110));
    eprintln!("{file}: bars {bars:?}, dark marks {lines:?}");
    assert!(bars.len() >= 5, "the 5 bars are drawn: {bars:?}");
    let missing: Vec<usize> = bars.iter().enumerate().filter(|(_, (a, b))| !lines.iter().any(|(l, r)| (l + r) / 2 >= *a && (l + r) / 2 <= *b)).map(|(i, _)| i).collect();
    assert!(missing.is_empty(), "pptx-C024-b11: the error bars (`c:errBars`, fixedVal 1) of a clustered column chart stand in the middle of each bar: no error bar over bar(s) {missing:?} (bars at {bars:?}, error bars at {lines:?})");
}

/// pptx-C024-b11（todo に項目なし）：縦棒グラフ（clustered）の系列の誤差線（`c:errBars`。`fixedVal` でも `cust` でも）を、PowerPoint は各棒の中心に値±1 の縦線とキャップで引く。
/// ibisOffice は誤差線の x を棒の間隔の半分の間隔で並べ（chart_view.rs の棒グラフの誤差線の点 `pts`）、線が棒の位置と合わず、右の 2 本の棒の上には線が無い。5 本の棒それぞれの
/// 横の範囲の中に誤差線（暗い縦線）の中心がある。対照（同じ誤差線を折れ線グラフに付けたもの）は棒が無く、別の図なので緑の試験は足さない。
pub(crate) fn corpus_report_pptx_column_error_bars_stand_on_their_bars() {
    check_error_bar_position("chart-errbars-bar-position.pptx");
}

fn count_darkened(page: &Page, area: EmuRect) -> usize {
    page.pixels(area).into_iter().filter(|(_, _, c)| c[0] < 100 && c[2] < 225).count()
}

/// pptx-C025-b1（todo に項目なし。P6a のカードの「あわせて」の blurRad の残り）：文字の影（`a:rPr/a:effectLst/a:outerShdw blurRad="38100"` と
/// `blurRad="152400"`、黒 43%、右下 3pt）を、PowerPoint はぼかして描く（ぼかしの半径が大きいほど広く淡い）。ibisOffice は `Shadow.blur` を読んでも文字の影では捨て
/// （shape_view.rs の影は `(色, dx, dy)` の 3 つ組）、字形をそのまま黒 43% で 1 回重ねる。ぼかし 12pt の行の影の画素（水色の地より暗い画素）の数が、ぼかし 3pt の行の 1.4 倍以上
/// （今は同じ。広がるぶん多い）。対照（ぼかし 0）の 2 行は同じ形なので緑の試験は足さない。
pub(crate) fn corpus_report_pptx_text_shadow_blur_widens_the_shadow() {
    let pages = render("text-shadow-blur.pptx", "text-shadow-blur");
    let page = &pages[0];
    let (small, large) = (count_darkened(page, area_px(page, 60, 120, 1860, 300)), count_darkened(page, area_px(page, 60, 540, 1860, 720)));
    eprintln!("shadow pixels: blur 3pt {small}, blur 12pt {large}");
    assert!(large as f64 >= 1.4 * small as f64, "pptx-C025-b1: the shadow of the 12pt blur (blurRad=152400) spreads wider than the 3pt blur's (blurRad=38100): {large} pixels darker than the ground against {small}, at least 1.4 times (ibisOffice draws both as the same hard shadow)");
}

/// pptx-C025-b2（todo に項目なし。P6a と同じ種類の漏れ）：run の字間（`a:rPr spc="800"`）と文字の輪郭線（`a:ln w="12700"`、赤）を併用すると、
/// ibisOffice は本体の字を字間を足した位置、輪郭線を字間なしの位置に描く（rich_text.rs の輪郭は `at + glyph.position` だけ。本体は `+ (piece.spacing + extra) * before`）ので、
/// 字ごとにずれて二重像になる。PowerPoint は輪郭線が本体の字と同じ位置に付く。輪郭の赤い画素の横の範囲が、同じ字間の輪郭なしの黒い行（下の行）の横の範囲の 95% 以上
/// （今は約 88%。輪郭が字間なしで縮んでいる）。対照は字間を外したもの（期待が違う形）なので緑の試験は足さない。
pub(crate) fn corpus_report_pptx_text_outline_follows_letter_spacing() {
    let pages = render("text-outline-spacing.pptx", "text-outline-spacing");
    let page = &pages[0];
    let redish = |c: Rgb| c[0] > 170 && c[0] - c[1] > 50 && c[0] - c[2] > 50;
    let (rl, rr) = extent(page, area_px(page, 60, 120, 1860, 300), redish).expect("the outline is drawn");
    let (bl, br) = extent(page, area_px(page, 60, 580, 1860, 760), dark).expect("the plain line is drawn");
    eprintln!("outline {rl}..{rr} ({}px), plain line {bl}..{br} ({}px)", rr - rl + 1, br - bl + 1);
    assert!((rr - rl + 1) as f64 >= 0.95 * (br - bl + 1) as f64, "pptx-C025-b2: the outline (red, 1pt) of the line with spc=800 spans {}px, as wide as the plain line with the same spacing ({}px) within 5%: the outline is drawn without the letter spacing the body gets", rr - rl + 1, br - bl + 1);
}

/// pptx-C025-b3（todo に項目なし。P6e の glow は直っていて、innerShdw が残っている）：run の `a:effectLst/a:innerShdw`（黄色、blurRad=8pt、dist=6pt、下向き、74%）を、
/// PowerPoint は文字の内側の縁（特に下向き）の黄色いにじみにする（ほぼ白の文字が黄色がかる）。ibisOffice は effectLst の子のうち outerShdw と glow しか読まず
/// （ibis-pptx/src/style.rs `RunStyle`）、純粋な白い文字のままで強調色が消える。ほぼ白の太字（72pt）の文字の箱に、黄色い画素（赤 220 超・緑 190 超・青 190 未満）が 200 以上
/// （今は 0）。対照（effectLst なし）は黄色が出ないので緑の試験は足さない。
pub(crate) fn corpus_report_pptx_text_inner_shadow_tints_the_glyph_edge() {
    let pages = render("text-inner-shadow.pptx", "text-inner-shadow");
    let page = &pages[0];
    let yellow = page.pixels(area_px(page, 60, 120, 1860, 380)).into_iter().filter(|(_, _, c)| c[0] > 220 && c[1] > 190 && c[2] < 190).count();
    eprintln!("yellow pixels {yellow}");
    assert!(yellow >= 200, "pptx-C025-b3: the inner shadow (yellow, 74%, 6pt down) tints the near-white 72pt text's lower edges yellow: {yellow} yellowish pixels, at least 200 (ibisOffice draws plain near-white text)");
}

fn check_geom_char_size(file: &str) {
    let pages = render(file, "geom-char-in-east-asian-font");
    let page = &pages[0];
    let runs = ink_runs(page, area_px(page, 80, 185, 400, 250), dark);
    eprintln!("{file}: first ink runs {:?}", &runs[..runs.len().min(3)]);
    let (a, b) = runs[0];
    assert!(b - a + 1 >= 42, "pptx-C026-b1: the \"□\" (U+25A1) of a ja-JP run drawn in the East Asian font is a full-width box (32pt: about 50px wide), not the half-width Latin glyph: {}px wide, at least 42", b - a + 1);
}

/// pptx-C026-b1（todo に項目なし。pptx-C009-b1 の Wingdings の置き換え表とは別）：本文の run の文字 □・■・○（U+25A1・25A0・25CB。箇条書き記号でなく
/// `<a:t>□元気がない</a:t>` の文字）が、PowerPoint では ja-JP の run で ea（Jpan）書体（メイリオ）の全幅の大きさ（32pt で約 50px 角）で描かれる。ibisOffice は `is_east_asian(c) = c >= 0x2e80`
/// （ibis-pptx/src/style.rs）で Latin 扱いにし、Latin 書体の半角ほどの小さい字形（約 30px）で描く。1 行目の □ の幅が 42px 以上。
/// 対照（run に `a:ea typeface="Hiragino Sans"` を足したもの）は全幅。
pub(crate) fn corpus_report_pptx_geometric_shapes_are_drawn_in_the_east_asian_font() {
    check_geom_char_size("geom-char-in-east-asian-font.pptx");
}

fn pic_corner(file: &str, x: i64, want: Option<Rgb>) {
    let pages = render(file, "pic-sppr-solid-fill");
    let page = &pages[0];
    let frame = rect(x, 1_428_750, 1_143_000, 1_143_000);
    let c = page.in_frame(frame, 0.08, 0.08);
    let navy = page.at(1_000_000, 2_800_000);
    eprintln!("{file}: the picture's corner {c:?}, the shape behind {navy:?}");
    match want {
        Some(want) => assert!(near(c, want, 24), "pptx-C027-b1: the picture's `spPr/solidFill` fills its frame behind the transparent PNG: the corner (transparent in the PNG) is {want:?} (±24), drawn {c:?} (the navy shape behind)"),
        None => assert!(near(c, navy, 20), "without a `spPr/solidFill` the shape behind shows at the PNG's transparent corner: {navy:?}, drawn {c:?}"),
    }
}

/// pptx-C027-b1（todo に項目なし）：`p:pic` の `p:spPr` 直下の `a:solidFill`（左は FFFFFF、右は FFE000）を、PowerPoint は画像（透明な部分のある PNG）の後ろに
/// 図形の枠いっぱいに敷く。ibisOffice は `p:pic` の `spPr` から塗りを読まず（reader.rs `picture`、`Picture` に塗りの項目が無い）、紺の角丸の箱の上に透明な PNG をそのまま重ねるので、
/// 白・黄の四角が出ない。画像の枠の隅（PNG では透明）が、左は白、右は黄（255,224,0）。対照（`spPr` の塗りを外したもの）は後ろの紺が見える。
pub(crate) fn corpus_report_pptx_picture_solid_fill_shows_behind_the_png() {
    pic_corner("pic-sppr-solid-fill.pptx", 1_428_750, Some([255, 255, 255]));
    pic_corner("pic-sppr-solid-fill.pptx", 4_667_250, Some([255, 224, 0]));
}

fn check_emf_rop_mask(file: &str, transparent: bool) {
    let pages = render(file, "emf-rop-mask-opaque");
    let page = &pages[0];
    let c = page.in_frame(rect(1_016_000, 1_016_000, 2_794_000, 2_794_000), 0.6, 0.15);
    eprintln!("{file}: outside the L {c:?}");
    if transparent {
        assert!(near(c, [255, 179, 71], 20), "pptx-C028-b1: outside the L the picture is transparent (the EMF's 1bpp mask drawn with SRCPAINT and the 8bpp image with SRCAND): the orange band (255,179,71) shows (±20), drawn {c:?}");
    } else {
        assert!(near(c, [255, 255, 255], 12), "the 8bpp image drawn with SRCCOPY is an opaque white box: {c:?}");
    }
}

/// pptx-C028-b1（todo に項目なし。P3c は直っている）：EMF の中に `EMR_STRETCHDIBITS` が 2 つ続き（1 つ目は 1bpp のマスク、ラスターオペレーション 0x00EE0086＝SRCPAINT、
/// 2 つ目は 8bpp の絵、0x008800C6＝SRCAND。同じ位置・大きさ）、Windows の昔からの「透明な絵」の作り方。物体の外側は下にあるものがそのまま見える。ibisOffice は
/// `BitBltRasterOp` を見ず（ibis-emf/src/emf/mod.rs `place_dib`・`stretch_dibits`）、どの rop でも PNG にして不透明に貼るので、物体（L 字）の外が不透明な白になる。
/// 橙の帯（255,179,71）の上の L 字の外の点が、橙（±20）。対照（2 つ目の rop を SRCCOPY にしたもの。不透明な白が正しい）は白。
pub(crate) fn corpus_report_pptx_emf_rop_pair_is_transparent_outside_the_object() {
    check_emf_rop_mask("emf-rop-mask-opaque.pptx", true);
}

fn check_emf_frame(file: &str) {
    let pages = render(file, "emf-frame-vs-bounds");
    let page = &pages[0];
    let content = |frame: EmuRect| {
        let (l, _, r, _) = bbox(page, frame, |c| c.iter().any(|v| *v < 240)).expect("the picture is drawn");
        (r - l + 1) as f64 / page.px(frame.width) as f64
    };
    let a = content(rect(381_000, 762_000, 3_166_517, 3_754_492));
    let b = content(rect(5_080_000, 762_000, 4_572_000, 3_111_500));
    eprintln!("{file}: the drawing fills {a:.2} (A, cropped) and {b:.2} (B) of its frame's width");
    let mut checks = Checks::default();
    checks.check(a <= 0.85, format!("A (`srcRect r=42496`): the drawing (the left 43% of an EMF whose frame is 150.5 × 102.6 mm) lies inside the cropped frame with room on the right: {a:.2} of its width, at most 0.85 (ibisOffice stretches it over the whole frame)"));
    checks.check(b <= 0.55, format!("B (no crop): the drawing fills the left 42% of the frame: {b:.2} of its width, at most 0.55 (ibisOffice fills 0.75)"));
    checks.done("pptx-C028-b2: an EMF's header bounds smaller than its frame");
}

/// 悪化の検知（緑）：pptx-C028-b2（直った。main の 0e2a24491「N6: EMF は bounds でなく枠（rclFrame）で図の箱に置く」）。元の仮バグの説明：EMF のヘッダーで、描いた物の外接の矩形 `rclBounds` が画像の枠 `rclFrame`（0.01mm 単位）より小さい（左 43% だけに描き、
/// 右に空き）とき、PowerPoint は空きを含めた枠で絵を置く（`a:srcRect` もその枠にかかる）。ibisOffice は viewBox を `rclFrame` でなく `rclBounds` にする（ibis-emf/src/emf/mod.rs の
/// `bounds`・`view`）ので、絵が枠いっぱいに拡大され、`srcRect` が拡大後の絵にかかって右が切れる。A（`srcRect r=42496` の枠）と B（切り抜きなしの枠）の中で、絵が占める横の割合が
/// A で 0.85 以下（今は 0.99）、B で 0.55 以下（今は 0.75。PowerPoint は 0.42）。対照（`rclBounds` だけを枠全体に直したもの）は A も B も収まる。
pub(crate) fn corpus_report_pptx_guard_emf_is_placed_by_its_frame_not_its_bounds() {
    check_emf_frame("emf-frame-vs-bounds.pptx");
}

/// 悪化の検知（緑）：pptx-C028-b2 の対照（`rclBounds` を枠全体に直したもの）も、絵が枠の中の正しい割合を占める。
pub(crate) fn corpus_report_pptx_guard_emf_with_bounds_equal_to_its_frame() {
    check_emf_frame("emf-frame-vs-bounds-ctrl.pptx");
}

fn check_rotated_picture_effects(file: &str, turned: bool) {
    let pages = render(file, "rotated-pic-blip-effects");
    let page = &pages[0];
    let (clear, faded) = if turned {
        let (w, h) = (page.px(2_032_000) as f64, page.px(2_032_000) as f64);
        let (cx, cy) = (page.px(1_016_000) as f64 + w / 2.0, page.px(1_143_000) as f64 + h / 2.0);
        let (dx, dy) = (-0.46 * w, -0.46 * h);
        let theta = 30.0_f64.to_radians();
        let (sin, cos) = theta.sin_cos();
        let corner = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
        (page.at(emu_at(page, corner.0 as i64), emu_at(page, corner.1 as i64)), page.in_frame(rect(1_016_000, 3_810_000, 2_032_000, 2_032_000), 0.39, 0.44))
    } else {
        (page.in_frame(rect(1_016_000, 1_143_000, 2_032_000, 2_032_000), 0.04, 0.04), page.in_frame(rect(1_016_000, 3_810_000, 2_032_000, 2_032_000), 0.37, 0.5))
    };
    eprintln!("{file}: the white corner {clear:?}, the faded circle {faded:?}");
    let mut checks = Checks::default();
    checks.check(near(clear, [255, 179, 71], 20), format!("`a:clrChange` (white to transparent): the picture's white corner shows the orange band (255,179,71) (±20): {clear:?}"));
    checks.check(near(faded, [176, 158, 123], 30), format!("`a:alphaModFix amt=35000`: the circle's blue half is mixed with the band, (176,158,123) (±30): {faded:?}"));
    checks.done("pptx-C028-b3: the effects of a picture's `a:blip`");
}

/// pptx-C028-b3（todo に項目なし。pptx-C016-b1・C014-b2 と同じ「回した絵」の分岐で、別の要素）：回した（`rot`、反転）`p:pic` で、同じ
/// `a:blip` の効果（`a:clrChange` の白→透明、`a:alphaModFix` の透明度）を、PowerPoint は回さない絵と同じに掛ける。ibisOffice は回した絵では `rotated_image` で返り（shape_view.rs の
/// `turned` の分岐が `styled`・`cleared` より前に return。渡すのは切り抜きだけ）、効果を渡さないので、白い地が残り、円が不透明のまま。30° 回した左上の絵（clrChange）の白い角の点が
/// 橙の帯（255,179,71）、左下の絵（alphaModFix 35%）の円の青い側が混ざった色（176,158,123）。対照（回転を 0 にしたもの）は効果が効く。
pub(crate) fn corpus_report_pptx_turned_picture_keeps_its_blip_effects() {
    check_rotated_picture_effects("rotated-pic-blip-effects.pptx", true);
}

// ---- 悪化の検知（緑）：第 1・2 回の赤の試験の対照、直った仮バグ ----

/// 悪化の検知（緑）：pptx-C004-b2（最新の main で直った）の対照と最小文書。箇条書きの番号が太字の run に追随し、番号のインクの量が、番号を太字の run の文字として書いた対照の 8 割以上。
pub(crate) fn corpus_report_pptx_guard_bullet_follows_bold_run() {
    let bug = render("bullet-bold.pptx", "bullet-bold");
    let ctrl = render("bullet-bold-ctrl.pptx", "bullet-bold-ctrl");
    let pt = |p: f64| (p * 12_700.0) as i64;
    let mut checks = Checks::default();
    for (name, top) in [("2.", 40.0), ("1)", 160.0)] {
        let area = rect(pt(40.0), pt(top), pt(53.0), pt(80.0));
        let (bullet, control) = (ink_mass(&bug[0], area), ink_mass(&ctrl[0], area));
        checks.check(control > 0.0, format!("{name}: control: the number is drawn"));
        checks.check(bullet >= 0.8 * control, format!("{name}: the bullet's ink {bullet:.0} px, the bold text's {control:.0} px: at least 80%"));
    }
    checks.done("pptx-C004-b2: a bullet of a bold run");
}

/// The slant of the blue text in `area`: how far right the centre of its upper third lies from the centre of its lower third (the glyphs' own
/// distribution adds a constant, so it is compared with the same text upright).
fn slant(page: &Page, area: EmuRect) -> f64 {
    let blue = |c: Rgb| c[2] > 150 && c[0] < 110;
    let hits: Vec<(i64, i64)> = page.pixels(area).into_iter().filter(|(_, _, c)| blue(*c)).map(|(x, y, _)| (x, y)).collect();
    let (top, bottom) = (hits.iter().map(|p| p.1).min().unwrap(), hits.iter().map(|p| p.1).max().unwrap());
    let third = (bottom - top) / 3;
    let centre = |lo: i64, hi: i64| {
        let xs: Vec<i64> = hits.iter().filter(|p| p.1 >= lo && p.1 <= hi).map(|p| p.0).collect();
        xs.iter().sum::<i64>() as f64 / xs.len().max(1) as f64
    };
    centre(top, top + third) - centre(bottom - third, bottom)
}

fn italic_slants(file: &str) -> Vec<f64> {
    let pages = render(file, "italic-japanese-font");
    let page = &pages[0];
    [80, 260, 440, 620, 800].iter().map(|y| slant(page, area_px(page, 80, *y, 1840, *y + 120))).collect()
}

/// 悪化の検知（緑）：pptx-C004-b3（直った）：`a:rPr i="1"` の日本語の書体（游ゴシック・メイリオ・ＭＳ Ｐゴシック。太字との併用も）の行と Arial の行が、すべて右に傾いた斜体で描かれる。
/// 行の上の 3 分の 1 の字の中心と下の 3 分の 1 の字の中心の水平の差（字の並びで決まる）が、`i` を外した同じ文面の対照より 3px 以上右（今は 5〜9px）。
pub(crate) fn corpus_report_pptx_guard_italic_japanese_font_is_slanted() {
    let italic = italic_slants("italic-japanese-font.pptx");
    let upright = italic_slants("italic-japanese-font-ctrl.pptx");
    eprintln!("slants: italic {italic:?}, upright {upright:?}");
    let mut checks = Checks::default();
    for (i, name) in ["游ゴシック", "メイリオ", "ＭＳ Ｐゴシック", "メイリオ（太字）", "Arial"].iter().enumerate() {
        let lean = italic[i] - upright[i];
        checks.check(lean >= 3.0, format!("{name} with i=\"1\" leans right of the same text without i by {lean:.1}px, at least 3"));
    }
    checks.done("pptx-C004-b3: an italic run in a Japanese font");
}

/// The scenarios, for `pptx::scenarios::all`.
pub(crate) fn all() -> Vec<(&'static str, fn())> {
    vec![
        ("pptx::corpus_report_pptx_p1b_thin_text_outline_keeps_fill", corpus_report_pptx_p1b_thin_text_outline_keeps_fill),
        ("pptx::corpus_report_pptx_p2a_picture_fill_effects", corpus_report_pptx_p2a_picture_fill_effects),
        ("pptx::corpus_report_pptx_p2b_picture_alpha", corpus_report_pptx_p2b_picture_alpha),
        ("pptx::corpus_report_pptx_p2c_picture_grayscale", corpus_report_pptx_p2c_picture_grayscale),
        ("pptx::corpus_report_pptx_p2d_picture_bilevel", corpus_report_pptx_p2d_picture_bilevel),
        ("pptx::corpus_report_pptx_p2e_picture_contrast", corpus_report_pptx_p2e_picture_contrast),
        ("pptx::corpus_report_pptx_p3a_vector_emf_drawn", corpus_report_pptx_p3a_vector_emf_drawn),
        ("pptx::corpus_report_pptx_p3b_wmf_drawn", corpus_report_pptx_p3b_wmf_drawn),
        ("pptx::corpus_report_pptx_p3c_mixed_emf_parts_in_place", corpus_report_pptx_p3c_mixed_emf_parts_in_place),
        ("pptx::corpus_report_pptx_p4a_hidden_slide_not_exported", corpus_report_pptx_p4a_hidden_slide_not_exported),
        ("pptx::corpus_report_pptx_p5a_ycck_jpeg_clear_colour_and_turn", corpus_report_pptx_p5a_ycck_jpeg_clear_colour_and_turn),
        ("pptx::corpus_report_pptx_p5a_cmyk_jpeg_clear_colour_and_turn", corpus_report_pptx_p5a_cmyk_jpeg_clear_colour_and_turn),
        ("pptx::corpus_report_pptx_p6a_text_shadow_follows_spacing", corpus_report_pptx_p6a_text_shadow_follows_spacing),
        ("pptx::corpus_report_pptx_p6c_picture_shadow_follows_transparency", corpus_report_pptx_p6c_picture_shadow_follows_transparency),
        ("pptx::corpus_report_pptx_p6d_text_warp_drawn", corpus_report_pptx_p6d_text_warp_drawn),
        ("pptx::corpus_report_pptx_p6e_text_glow_drawn", corpus_report_pptx_p6e_text_glow_drawn),
        ("pptx::corpus_report_pptx_p7b_japanese_substitute_width", corpus_report_pptx_p7b_japanese_substitute_width),
        ("pptx::corpus_report_pptx_p7c_latin_substitute_width", corpus_report_pptx_p7c_latin_substitute_width),
        ("pptx::corpus_report_pptx_p8c_sideways_text_at_slide_edge", corpus_report_pptx_p8c_sideways_text_at_slide_edge),
        ("pptx::corpus_report_pptx_p8d_pie_best_fit_labels_outside_small_slices", corpus_report_pptx_p8d_pie_best_fit_labels_outside_small_slices),
        ("pptx::corpus_report_pptx_p8e_automatic_plot_area_leaves_room", corpus_report_pptx_p8e_automatic_plot_area_leaves_room),
        ("pptx::corpus_report_pptx_guard_emf_is_placed_by_its_frame_not_its_bounds", corpus_report_pptx_guard_emf_is_placed_by_its_frame_not_its_bounds),
        ("pptx::corpus_report_pptx_guard_emf_with_bounds_equal_to_its_frame", corpus_report_pptx_guard_emf_with_bounds_equal_to_its_frame),
        ("pptx::corpus_report_pptx_guard_bullet_follows_bold_run", corpus_report_pptx_guard_bullet_follows_bold_run),
        ("pptx::corpus_report_pptx_guard_italic_japanese_font_is_slanted", corpus_report_pptx_guard_italic_japanese_font_is_slanted),
        ("pptx::corpus_report_pptx_chart_dashes_drawn", corpus_report_pptx_chart_dashes_drawn),
        ("pptx::corpus_report_pptx_percent_stacked_labels_show_values", corpus_report_pptx_percent_stacked_labels_show_values),
        ("pptx::corpus_report_pptx_tick_labels_low_go_below_the_plot", corpus_report_pptx_tick_labels_low_go_below_the_plot),
        ("pptx::corpus_report_pptx_point_label_show_value_overrides_series", corpus_report_pptx_point_label_show_value_overrides_series),
        ("pptx::corpus_report_pptx_pie_label_box_fill_is_drawn", corpus_report_pptx_pie_label_box_fill_is_drawn),
        ("pptx::corpus_report_pptx_bar_label_offset_follows_chart_frame", corpus_report_pptx_bar_label_offset_follows_chart_frame),
        ("pptx::corpus_report_pptx_auto_axis_does_not_start_at_zero_for_narrow_data", corpus_report_pptx_auto_axis_does_not_start_at_zero_for_narrow_data),
        ("pptx::corpus_report_pptx_pie_auto_plot_area_follows_title_and_labels", corpus_report_pptx_pie_auto_plot_area_follows_title_and_labels),
        ("pptx::corpus_report_pptx_radar_axis_labels_follow_the_text_size", corpus_report_pptx_radar_axis_labels_follow_the_text_size),
        ("pptx::corpus_report_pptx_category_labels_stay_inside_the_frame", corpus_report_pptx_category_labels_stay_inside_the_frame),
        ("pptx::corpus_report_pptx_legend_wraps_when_the_row_is_too_wide", corpus_report_pptx_legend_wraps_when_the_row_is_too_wide),
        ("pptx::corpus_report_pptx_long_chart_title_wraps_at_the_frame_width", corpus_report_pptx_long_chart_title_wraps_at_the_frame_width),
        ("pptx::corpus_report_pptx_chart_data_table_keeps_its_row_height", corpus_report_pptx_chart_data_table_keeps_its_row_height),
        ("pptx::corpus_report_pptx_trendline_spans_the_whole_x_range", corpus_report_pptx_trendline_spans_the_whole_x_range),
        ("pptx::corpus_report_pptx_deleted_secondary_axis_uses_the_primary_scale", corpus_report_pptx_deleted_secondary_axis_uses_the_primary_scale),
        ("pptx::corpus_report_pptx_scatter_legend_shows_marker_keys", corpus_report_pptx_scatter_legend_shows_marker_keys),
        ("pptx::corpus_report_pptx_column_error_bars_stand_on_their_bars", corpus_report_pptx_column_error_bars_stand_on_their_bars),
        ("pptx::corpus_report_pptx_reg_palt_bracket_gap", corpus_report_pptx_reg_palt_bracket_gap),
        ("pptx::corpus_report_pptx_spcpct_follows_line_height", corpus_report_pptx_spcpct_follows_line_height),
        ("pptx::corpus_report_pptx_pie_text_rect", corpus_report_pptx_pie_text_rect),
        ("pptx::corpus_report_pptx_named_heavy_font_is_heavy", corpus_report_pptx_named_heavy_font_is_heavy),
        ("pptx::corpus_report_pptx_br_blank_line_takes_its_own_size", corpus_report_pptx_br_blank_line_takes_its_own_size),
        ("pptx::corpus_report_pptx_table_row_grows_for_wrapped_text", corpus_report_pptx_table_row_grows_for_wrapped_text),
        ("pptx::corpus_report_pptx_bullet_without_hang_pushes_text", corpus_report_pptx_bullet_without_hang_pushes_text),
        ("pptx::corpus_report_pptx_tabs_advance_to_tab_stops", corpus_report_pptx_tabs_advance_to_tab_stops),
        ("pptx::corpus_report_pptx_sysdash_with_square_cap_keeps_gaps", corpus_report_pptx_sysdash_with_square_cap_keeps_gaps),
        ("pptx::corpus_report_pptx_table_cell_after_a_grid_span_keeps_its_width", corpus_report_pptx_table_cell_after_a_grid_span_keeps_its_width),
        ("pptx::corpus_report_pptx_glow_of_a_shape_without_fill_draws_nothing", corpus_report_pptx_glow_of_a_shape_without_fill_draws_nothing),
        ("pptx::corpus_report_pptx_callout_tail_outside_the_frame_gets_the_gradient", corpus_report_pptx_callout_tail_outside_the_frame_gets_the_gradient),
        ("pptx::corpus_report_pptx_accent_callout_bar_has_no_arrowhead", corpus_report_pptx_accent_callout_bar_has_no_arrowhead),
        ("pptx::corpus_report_pptx_line_gradient_follows_the_angle_not_the_path", corpus_report_pptx_line_gradient_follows_the_angle_not_the_path),
        ("pptx::corpus_report_pptx_picture_round_rect_is_clipped", corpus_report_pptx_picture_round_rect_is_clipped),
        ("pptx::corpus_report_pptx_rotated_picture_fill_is_clipped", corpus_report_pptx_rotated_picture_fill_is_clipped),
        ("pptx::corpus_report_pptx_clr_change_with_duotone_makes_ground_transparent", corpus_report_pptx_clr_change_with_duotone_makes_ground_transparent),
        ("pptx::corpus_report_pptx_rotated_picture_keeps_outline_and_shadow", corpus_report_pptx_rotated_picture_keeps_outline_and_shadow),
        ("pptx::corpus_report_pptx_turned_picture_keeps_its_blip_effects", corpus_report_pptx_turned_picture_keeps_its_blip_effects),
        ("pptx::corpus_report_pptx_emf_rop_pair_is_transparent_outside_the_object", corpus_report_pptx_emf_rop_pair_is_transparent_outside_the_object),
        ("pptx::corpus_report_pptx_table_dash_with_other_frame_height", corpus_report_pptx_table_dash_with_other_frame_height),
        ("pptx::corpus_report_pptx_close_paren_after_space_does_not_start_line", corpus_report_pptx_close_paren_after_space_does_not_start_line),
        ("pptx::corpus_report_pptx_latin_break_after_hyphen", corpus_report_pptx_latin_break_after_hyphen),
        ("pptx::corpus_report_pptx_table_rows_of_height_zero_are_10pt_apart", corpus_report_pptx_table_rows_of_height_zero_are_10pt_apart),
        ("pptx::corpus_report_pptx_just_line_before_br_is_spread", corpus_report_pptx_just_line_before_br_is_spread),
        ("pptx::corpus_report_pptx_just_latin_spreads_words_not_letters", corpus_report_pptx_just_latin_spreads_words_not_letters),
        ("pptx::corpus_report_pptx_zero_height_row_takes_the_height_of_its_text", corpus_report_pptx_zero_height_row_takes_the_height_of_its_text),
        ("pptx::corpus_report_pptx_text_outline_follows_letter_spacing", corpus_report_pptx_text_outline_follows_letter_spacing),
        ("pptx::corpus_report_pptx_circled_digit_in_east_asian_font", corpus_report_pptx_circled_digit_in_east_asian_font),
        ("pptx::corpus_report_pptx_reg_latin_fallback_width", corpus_report_pptx_reg_latin_fallback_width),
        ("pptx::corpus_report_pptx_fullwidth_alnum_in_proportional_font", corpus_report_pptx_fullwidth_alnum_in_proportional_font),
        ("pptx::corpus_report_pptx_symbol_arrow_follows_bold", corpus_report_pptx_symbol_arrow_follows_bold),
        ("pptx::corpus_report_pptx_bullet_char_drawn_in_text_font", corpus_report_pptx_bullet_char_drawn_in_text_font),
        ("pptx::corpus_report_pptx_default_text_style_mn_ea_keeps_symbol_size", corpus_report_pptx_default_text_style_mn_ea_keeps_symbol_size),
        ("pptx::corpus_report_pptx_symbols_use_small_gothic_shapes", corpus_report_pptx_symbols_use_small_gothic_shapes),
        ("pptx::corpus_report_pptx_missing_font_does_not_defer_to_the_other_scripts_font", corpus_report_pptx_missing_font_does_not_defer_to_the_other_scripts_font),
        ("pptx::corpus_report_pptx_serif_latin_font_is_not_drawn_in_arial", corpus_report_pptx_serif_latin_font_is_not_drawn_in_arial),
        ("pptx::corpus_report_pptx_geometric_shapes_are_drawn_in_the_east_asian_font", corpus_report_pptx_geometric_shapes_are_drawn_in_the_east_asian_font),
        ("pptx::corpus_report_pptx_picture_fill_outline_stays_on_shape", corpus_report_pptx_picture_fill_outline_stays_on_shape),
        ("pptx::corpus_report_pptx_scaled_image_has_no_edge_line", corpus_report_pptx_scaled_image_has_no_edge_line),
        ("pptx::corpus_report_pptx_cmyk_jpeg_uses_its_icc_profile", corpus_report_pptx_cmyk_jpeg_uses_its_icc_profile),
        ("pptx::corpus_report_pptx_theme_gradient_tint_is_not_pale", corpus_report_pptx_theme_gradient_tint_is_not_pale),
        ("pptx::corpus_report_pptx_connector_gradient_line_changes_colour", corpus_report_pptx_connector_gradient_line_changes_colour),
        ("pptx::corpus_report_pptx_stacked_bar_series_lines_are_drawn", corpus_report_pptx_stacked_bar_series_lines_are_drawn),
        ("pptx::corpus_report_pptx_custom_error_bars_use_their_values", corpus_report_pptx_custom_error_bars_use_their_values),
        ("pptx::corpus_report_pptx_text_shadow_blur_widens_the_shadow", corpus_report_pptx_text_shadow_blur_widens_the_shadow),
        ("pptx::corpus_report_pptx_text_inner_shadow_tints_the_glyph_edge", corpus_report_pptx_text_inner_shadow_tints_the_glyph_edge),
        ("pptx::corpus_report_pptx_picture_solid_fill_shows_behind_the_png", corpus_report_pptx_picture_solid_fill_shows_behind_the_png),
        ("pptx::corpus_report_pptx_synthetic_bold_keeps_the_regular_advance", corpus_report_pptx_synthetic_bold_keeps_the_regular_advance),
    ]
}

/// Unfixed corpus_report bug scenarios (skipped in normal `cargo test`, run with `--ignored` or `--include-ignored`).
pub(crate) fn ignored() -> Vec<(&'static str, fn())> {
    vec![
        // Unfixed: the text glow's reach depends on the text size as well as the radius (PowerPoint, Mac and Windows),
        // and no formula is known yet. Measurements: tasks/doc/glow-measurements-20261030.md.
        ("pptx::corpus_report_pptx_large_text_glow_is_a_thin_rim", corpus_report_pptx_large_text_glow_is_a_thin_rim),
    ]
}

/// コーパスの 1 文書の 1 枚：「ＭＳ ゴシック」の太字（b="1"、18pt）の全角 16 字の 1 行（幅 311pt の角丸の箱、左右の内側の余白 8.5pt）を、PowerPoint は 1 行に
/// 収める。ＭＳ ゴシックには太字の face が無く、PowerPoint は普通の face の字を太らせるだけで送りは変えない（16 字で 288pt）。ibisOffice は Windows で
/// DirectWrite の擬似太字の送り（1 字 18.35pt）で測り、最後の 1 字が 2 行目に折り返す。太字の行も普通の行（対照）と同じ 1 行（今は 2 行）。
pub(crate) fn corpus_report_pptx_synthetic_bold_keeps_the_regular_advance() {
    let pages = render("msgothic-bold-fits.pptx", "msgothic-bold-fits");
    let page = &pages[0];
    let mut lines = Vec::new();
    for (label, y) in [("bold", 326326_i64), ("regular", 1000000), ("bold-proportional", 1700000)] {
        lines.push((label, ink_bands(page, rect(258858, y, 3960000, 390286), dark, 4.0).len()));
    }
    eprintln!("text lines: {lines:?}");
    assert!(lines.iter().all(|(_, n)| *n == 1), "the bold line of 16 full-width characters at 18pt (288pt) fits the 294.8pt inside its box as the regular one does: lines of text {lines:?}");
}
