// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The pptx defects the corpus comparison found (PowerPoint for the web's
//! PDF as the reference), each read from its smallest file in
//! `test/corpus-report/pptx/` and checked in the model against what
//! PowerPoint shows.
//!
//! TODO: このファイルは、実文書の比較（コーパス）から見つかった崩れの試験の
//! 仮置き場で、ずっと残すものではない。試験が通るようになったら（崩れを直した
//! ら）、その試験を本番の試験に移すか、試験として要らなければ消し、最小文書
//! （`test/corpus-report/pptx/`）も一緒に移すか消す。
//! 本番の移し先は `crates/ibis-pptx/tests/` の機能ごとの試験（例
//! `text_bullets_and_runs.rs`）。
//! 崩れが残っている試験（赤）はここに置いたままにする。

use ibis_doc_model::{Fill, MediaFormat, Presentation, Rgb, Shape, ShapeKind, Slide, TextBody};

fn read(name: &str) -> Presentation {
    let path = format!("{}/../../test/corpus-report/pptx/{name}", env!("CARGO_MANIFEST_DIR"));
    let data = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    ibis_pptx::read(&data).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn named<'s>(slide: &'s Slide, name: &str) -> &'s Shape {
    let id = slide.shape_names.iter().find(|(_, n)| n.as_str() == name).map(|(id, _)| *id).unwrap_or_else(|| panic!("no shape named {name}"));
    slide.shape(id).unwrap_or_else(|| panic!("shape {name} is not on the slide"))
}

fn channels(rgb: Rgb) -> (u32, u32, u32) {
    ((rgb.0 >> 16) & 0xff, (rgb.0 >> 8) & 0xff, rgb.0 & 0xff)
}

fn table_of(slide: &Slide) -> &ibis_doc_model::Table {
    slide.shapes.iter().find_map(|s| match &s.kind {
        ShapeKind::Table(t) => Some(t.as_ref()),
        _ => None,
    }).expect("the slide has a table")
}

fn bullets(body: &TextBody) -> Vec<String> {
    body.paragraphs.iter().filter(|p| !p.runs.iter().all(|r| r.text.is_empty())).map(|p| p.bullet.as_ref().map(|b| b.text.clone()).unwrap_or_default()).collect()
}

/// P1a: マスターの `p:bgRef idx="1002"` などで、テーマの `bgFillStyleLst` ではなく
/// `fillStyleLst` を引くため、暗いグラデーションの背景が明るい灰色になる。
/// idx 1001 以上は bgFillStyleLst[idx-1001]（ECMA-376）。1002 は bg2（暗い紺 1F3A68）を
/// shade 48% にした縦のグラデーションなので、最初の色は bg2 より明るくならない。
#[test]
fn corpus_report_pptx_p1a_bgref_1002_takes_bg_fill_style() {
    let p = read("bgref-1002-dark.pptx");
    let background = &p.slides[0].background;
    let Fill::Linear { stops, .. } = background else { panic!("the background is bgFillStyleLst[1], a linear gradient: {background:?}") };
    let (r, g, b) = channels(stops[0].color.rgb);
    assert!(r <= 0x1f && g <= 0x3a && b <= 0x68, "the gradient's top is bg2 (#1F3A68) shaded, no lighter than it: #{:06X}", stops[0].color.rgb.0);
}

/// P1a: 同上。idx="1003" は bgFillStyleLst の 3 番目（緑 00B050）で、fillStyleLst の
/// 3 番目（赤 FF0000）ではない。
#[test]
fn corpus_report_pptx_p1a_bgref_1003_is_green_not_red() {
    let p = read("bgref-1003-red-or-green.pptx");
    let background = &p.slides[0].background;
    assert!(matches!(background, Fill::Solid(c) if c.rgb == Rgb(0x00b050)), "the background is bgFillStyleLst[2], green #00B050: {background:?}");
}

/// P2a: 背景の画像の塗りで、伸縮の範囲（`a:fillRect`）が捨てられ、写真が全面に描かれる。
/// 範囲（上 -23%・下 9%）は塗りに持たせる。
#[test]
fn corpus_report_pptx_p2a_background_picture_keeps_fill_rect() {
    let p = read("bg-fillrect.pptx");
    let Fill::Blip(blip) = &p.slides[0].background else { panic!("the background keeps its fill rectangle: {:?}", p.slides[0].background) };
    assert_eq!(blip.fill_rect, Some([0, -23_000, 0, 9_000]));
}

/// P2a: 背景の画像の塗りで、透明度（`a:alphaModFix`）が捨てられ、写真が濃いまま全面に描かれる。
#[test]
fn corpus_report_pptx_p2a_background_picture_keeps_opacity() {
    let p = read("p3d-alphamodfix.pptx");
    let Fill::Blip(blip) = &p.slides[0].background else { panic!("the background keeps its opacity: {:?}", p.slides[0].background) };
    assert_eq!(blip.effects.opacity, Some(14_000));
}

/// P2a: 図形の画像の塗りで、透明度・明るさ・タイル・範囲の指定が捨てられ、5つとも同じ
/// 不透明な全面の画像になる。
#[test]
fn corpus_report_pptx_p2a_shape_picture_fill_keeps_its_adjustments() {
    let p = read("picture-effects.pptx");
    let blip = |name: &str| match &named(&p.slides[1], name).kind {
        ShapeKind::Auto(a) => match &a.fill {
            Fill::Blip(b) => Some((**b).clone()),
            _ => None,
        },
        _ => None,
    };
    assert!(blip("R20").is_none(), "a plain picture fill stays Fill::Image");
    assert_eq!(blip("R21").unwrap().effects.opacity, Some(30_000));
    assert_eq!(blip("R22").unwrap().luminance, Some((40_000, -50_000)));
    let tiled = blip("R23").unwrap();
    assert_eq!((tiled.effects.opacity, tiled.tile.map(|t| (t.sx, t.sy))), (Some(30_000), Some((25_000, 25_000))));
    assert_eq!(blip("R24").unwrap().fill_rect, Some([25_000, 0, 25_000, 0]));
}

/// P1c: `a:tblPr` に `a:tableStyleId` が無い表に、`tableStyles.xml` の `def`（既定の
/// 表のスタイル）を当てはめるため、白い表が青く塗られる。PowerPoint はスタイルなしで
/// 描くので、どのセルにも塗りが無い。
#[test]
fn corpus_report_pptx_p1c_table_without_style_id_has_no_fill() {
    let p = read("table-no-style-id.pptx");
    let table = table_of(&p.slides[0]);
    let filled: Vec<(usize, usize, &Fill)> =
        table.rows.iter().enumerate().flat_map(|(r, row)| row.cells.iter().enumerate().map(move |(c, cell)| (r, c, &cell.fill))).filter(|(_, _, f)| **f != Fill::None).collect();
    assert!(filled.is_empty(), "no cell is filled (no table style): {filled:?}");
}

/// P4a: 非表示のスライド（`show="0"`）は、書き出し・印刷から外れる（番号は数える）。
/// ここでは読み込みが 2 枚目を非表示として持つことを確かめる（書き出しの既定は
/// `ibis-office-app` の `export`）。
#[test]
fn corpus_report_pptx_p4a_hidden_slide_is_read_as_hidden() {
    let p = read("p4a-hidden-middle-slide.pptx");
    assert_eq!(p.slides.iter().map(|s| s.hidden).collect::<Vec<_>>(), vec![false, true, false]);
}

/// P4b: 番号付きの箇条書きが、間に挟まった箇条書きの無い空の段落で途切れず、
/// 番号が続いてしまう（正解「3.」→ ibisOffice「4.」）。PowerPoint は左「3. 3.」・
/// 中「1. 1.」・右「3. 4.」（右は番号付きの空の段落なので続く）。
#[test]
fn corpus_report_pptx_p4b_numbering_restarts_after_empty_unbulleted_paragraph() {
    let p = read("p4b-number-after-empty-paragraph.pptx");
    let slide = &p.slides[0];
    let lists: Vec<Vec<String>> = slide.shapes.iter().filter_map(|s| s.text_body()).map(bullets).filter(|b| b.iter().any(|t| !t.is_empty())).collect();
    assert_eq!(lists, vec![vec!["3.", "3."], vec!["1.", "1."], vec!["3.", "4."]], "the numbers of the three boxes");
}

/// P5b: `tableStyles.xml` に定義の無い組み込みの表スタイル（`{5C22544A-…}` など）では
/// 表のセルが塗られず、後ろにあるマスターの濃い青の帯が透けて行名を隠す。
/// PowerPoint は中間スタイル 2 - アクセント 1 を内蔵していて、見出し行は accent1
/// （4F81BD）、ほかの行も薄い色で塗られる。
#[test]
fn corpus_report_pptx_p5b_builtin_table_style_fills_cells() {
    let p = read("builtin-table-style.pptx");
    let table = table_of(&p.slides[0]);
    let header: Vec<&Fill> = table.rows[0].cells.iter().map(|c| &c.fill).collect();
    assert!(header.iter().all(|f| matches!(f, Fill::Solid(c) if c.rgb == Rgb(0x4f81bd))), "the header row is accent1 (#4F81BD): {header:?}");
    let body: Vec<&Fill> = table.rows[1..].iter().flat_map(|r| r.cells.iter().map(|c| &c.fill)).collect();
    assert!(body.iter().all(|f| matches!(f, Fill::Solid(c) if c.alpha == 255)), "the other rows are filled, hiding the band behind: {body:?}");
}

/// P6b: run の空の `<a:effectLst/>` が、`lstStyle` などから受け継いだ文字の影を
/// 消さないため、PowerPoint では影の無い文字に影が付く。
#[test]
fn corpus_report_pptx_p6b_empty_effect_list_removes_inherited_shadow() {
    let p = read("p6b-empty-effectlst.pptx");
    let slide = &p.slides[0];
    let shadow = |name: &str| named(slide, name).text_body().unwrap().paragraphs[0].runs[0].style.shadow.clone();
    assert!(shadow("T10").is_some(), "the lstStyle's shadow reaches a run without effectLst");
    assert_eq!(shadow("T11"), None, "the run's empty effectLst means no shadow");
}

/// P7a: 数式（`a14:m` の分数）が落ち、符号だけが残る。PowerPoint は「−」の右に
/// 5/3 を組むので、図形には分子 5 と分母 3 が残る（数式として読むか、
/// mc:Fallback の分数の画像を塗りにした図形を選ぶか）。
#[test]
fn corpus_report_pptx_p7a_math_fraction_is_kept() {
    let p = read("P7a-math-fraction.pptx");
    let shape = named(&p.slides[0], "Math 1");
    let text = shape.text_body().map(|b| b.text()).unwrap_or_default();
    let fallback_picture = matches!(&shape.kind, ShapeKind::Auto(a) if matches!(a.fill, Fill::Image(_) | Fill::TileImage { .. } | Fill::Blip(_))) || matches!(shape.kind, ShapeKind::Picture(_));
    assert!((text.contains('5') && text.contains('3')) || fallback_picture, "the fraction 5/3 is kept (as text or as the fallback picture): text {text:?}");
}

/// P8a: 拡張子が `.tmp` の画像（中身は PNG）を拡張子だけで判定するため、
/// 「画像（未対応）」の灰色の枠になる。[Content_Types].xml が image/png と言い、
/// 中身も PNG なので PNG として読む。
#[test]
fn corpus_report_pptx_p8a_png_named_tmp_is_png() {
    let p = read("p8a-png-as-tmp.pptx");
    let formats: Vec<MediaFormat> = p.slides[0]
        .shapes
        .iter()
        .filter_map(|s| match &s.kind {
            ShapeKind::Picture(pic) => p.media.iter().find(|m| m.id == pic.media).map(|m| m.format),
            _ => None,
        })
        .collect();
    assert_eq!(formats, vec![MediaFormat::Png, MediaFormat::Png], "image1.png and image2.tmp are both PNG pictures");
}

/// P8b: グループが縦と横で違う倍率に伸びているとき、中の 90° 回転の図形の幅と
/// 高さに縦横の倍率を逆に掛けるため、帯が横に伸びて番号の箱や枠の外にはみ出す。
/// 横 2.0 倍・縦 1.4 倍のグループの 800000×3200000 の帯（90°）は、見た目の枠が
/// 赤の破線（2100000, 1440000, 6400000×1120000）に収まる：回す前の幅は
/// 800000×1.4 = 1120000、高さは 3200000×2.0 = 6400000。
#[test]
fn corpus_report_pptx_p8b_turned_shape_in_scaled_group() {
    let p = read("p8b-turned-shape-in-scaled-group.pptx");
    let band = named(&p.slides[0], "Band 12");
    assert_eq!(band.rotation, 5_400_000);
    let f = band.frame;
    let centre = (f.x + f.width / 2, f.y + f.height / 2);
    assert!((centre.0 - 5_300_000).abs() <= 2 && (centre.1 - 2_000_000).abs() <= 2, "the band's centre is the dashed box's: {centre:?}");
    assert!((f.width - 1_120_000).abs() <= 2 && (f.height - 6_400_000).abs() <= 2, "the unturned band is 1120000 × 6400000 (turned, the dashed box): {} × {}", f.width, f.height);
}

/// pptx-C009-b2（todo に項目なし。pptx-C009-b1 の「Wingdings を別の字形にする」は環境の差が疑われるので別）：箇条書きの記号が
/// `buFont="Wingdings"`・`buChar="p"` のとき、読み込みが記号の文字を置き換え表に無い符号として「•」にしてしまう。
/// PowerPoint の記号は Wingdings の「p」の太い枠の四角（正解 PDF）。置き換え表にある「q」（❑）の対照は四角が出る。
/// 記号は「•」でなく、四角の字（U+25A0〜25FF、U+2750〜2752）になる。
#[test]
fn corpus_report_pptx_wingdings_p_bullet_is_a_square() {
    let p = read("wingdings-p-bullet-glyph.pptx");
    let slide = &p.slides[0];
    let labels: Vec<String> = slide.shapes.iter().filter_map(|s| s.text_body()).flat_map(bullets).filter(|b| !b.is_empty()).collect();
    assert_eq!(labels.len(), 3, "the three items carry a bullet: {labels:?}");
    let square = |t: &String| t.chars().count() == 1 && t.chars().all(|c| matches!(c as u32, 0x25a0..=0x25ff | 0x2750..=0x2752));
    assert!(labels.iter().all(square), "Wingdings \"p\" is a square (U+25A0..25FF or U+2750..2752), not the \"•\" a code without a mapping falls to: {labels:?}");
}

/// pptx-C015-b4（todo に項目なし）：データラベルが「セル範囲の値」（各 `c:dLbl` の `a:fld type="CELLRANGE"`、文字は仮の
/// 「[CELLRANGE]」）のとき、実際の文字は系列の `c15:datalabelsRange/c15:dlblRangeCache`（「12,300 (4%)」など）にある。
/// 読み込みは仮の文字をそのままラベルの文にしてしまい、全部の棒に「[CELLRANGE]」が出る。PowerPoint はキャッシュの文字を出す。
/// ラベルの文に「[CELLRANGE]」が無く、キャッシュの「12,300 (4%)」がある。
#[test]
fn corpus_report_pptx_cell_range_label_takes_the_cached_text() {
    let p = read("cellrange-label.pptx");
    let chart = p.slides.iter().flat_map(|s| &s.shapes).find_map(|s| match &s.kind {
        ShapeKind::Chart(c) => Some(c),
        _ => None,
    }).expect("the slide has a chart");
    let texts: Vec<String> = chart
        .groups
        .iter()
        .flat_map(|g| &g.series)
        .filter_map(|s| s.labels.as_ref())
        .flat_map(|l| &l.points)
        .filter_map(|p| p.text.as_ref())
        .map(|pieces| pieces.iter().map(|piece| if let ibis_doc_model::LabelPiece::Text(t) = piece { t.as_str() } else { "" }).collect::<String>())
        .collect();
    assert!(!texts.is_empty(), "the five labels carry their own text");
    assert!(texts.iter().all(|t| !t.contains("[CELLRANGE]")), "the placeholder \"[CELLRANGE]\" is not a label's text: {texts:?}");
    assert!(texts.iter().any(|t| t.contains("12,300 (4%)")), "the cached text \"12,300 (4%)\" (c15:dlblRangeCache) is a label's text: {texts:?}");
}

// ---- 第 3 回（corpus-report-3）：束 pptx-C017〜C028 の仮バグと、悪化を検知する緑の試験 ----

fn chart_of(p: &Presentation) -> &ibis_doc_model::Chart {
    p.slides.iter().flat_map(|s| &s.shapes).find_map(|s| match &s.kind {
        ShapeKind::Chart(c) => Some(c.as_ref()),
        _ => None,
    }).expect("the slide has a chart")
}

fn solid(fill: &Fill) -> Option<u32> {
    match fill {
        Fill::Solid(c) => Some(c.rgb.0),
        _ => None,
    }
}

fn check_chart_title_text(file: &str) {
    let p = read(file);
    let chart = chart_of(&p);
    let title = chart.title.as_ref().map(|t| t.text.clone());
    assert!(title.as_ref().is_some_and(|t| !t.trim().is_empty()), "pptx-C017-b3: the chart title is shown with a text (PowerPoint's default \"Chart Title\" for an empty `c:title`): {title:?}");
}

/// pptx-C017-b3（todo に項目なし）：`c:title` に `c:tx` が無く（`c:overlay` と書式だけ）、`autoTitleDeleted=0` のグラフ（系列が 2 本）は、
/// PowerPoint が既定の題「Chart Title」を灰色で出し、凡例と描画域がその分だけ下がる。読み込みは「系列が 1 本のときだけ系列名」にして
/// 複数系列では題を捨てる（`chart.title` が None）ので、題が出ず、凡例が上端に寄る。題が文字つきで残る。
/// 対照（題に文字「Chart Title」を書いたもの）は題が残る。
#[test]
fn corpus_report_pptx_chart_title_without_text_gets_default_text() {
    check_chart_title_text("chart-title-without-text.pptx");
}

fn check_table_text_other_style(align_file: &str, size_file: &str, want_size: u32) {
    let mut failures = Vec::new();
    let aligned = read(align_file);
    for (r, row) in table_of(&aligned.slides[0]).rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            for (i, para) in cell.text.paragraphs.iter().enumerate() {
                if para.align.unwrap_or(ibis_doc_model::TextAlign::Left) != ibis_doc_model::TextAlign::Left {
                    failures.push(format!("pptx-C018-b3: cell ({r},{c}) paragraph {i} has no `algn` and is {:?} (the presentation's defaultTextStyle says algn=\"ctr\"); the master's otherStyle (algn=\"l\") applies: left", para.align));
                }
            }
        }
    }
    let sized = read(size_file);
    for (r, row) in table_of(&sized.slides[0]).rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            for run in cell.text.paragraphs.iter().flat_map(|para| &para.runs) {
                if run.style.font_size != Some(want_size) {
                    failures.push(format!("pptx-C022-b3: cell ({r},{c}) run {:?} has no `sz` and is {:?} (defaultTextStyle); the master's otherStyle says {want_size}", run.text, run.style.font_size));
                }
            }
        }
    }
    assert!(failures.is_empty(), "the text of a table cell follows the master's otherStyle, not the presentation's defaultTextStyle:\n  {}", failures.join("\n  "));
}

/// pptx-C018-b3（todo に項目なし。C022-b3 と同じきっかけなので 1 本にまとめた）・pptx-C022-b3：表のセルの文字（`a:rPr` に `sz` が無く、
/// 段落に `algn` が無いもの）の既定を、読み込みがマスターの `p:txStyles/p:otherStyle` でなく、`presentation.xml` の `p:defaultTextStyle`
/// にする（reader.rs `text_body` は、プレースホルダーでない文字では defaultTextStyle だけを土台にする）。PowerPoint は表のセルの文字に
/// otherStyle を使う（正解 PDF）。
///   - C018-b3：defaultTextStyle が algn="ctr"、otherStyle が algn="l" のとき、algn の無い段落は左揃え。今は中央。
///   - C022-b3：defaultTextStyle が sz=1800、otherStyle が sz=1350 のとき、`sz` の無いセルの文字は 13.5pt。今は 18pt。
/// 対照（defaultTextStyle と otherStyle が同じ値）は、どちらも通る。
#[test]
fn corpus_report_pptx_table_text_follows_other_style() {
    check_table_text_other_style("table-text-other-style-align.pptx", "table-othertext-size.pptx", 1350);
}

fn check_chart_theme_override(file: &str) {
    let p = read(file);
    let chart = chart_of(&p);
    let colours: Vec<Option<u32>> = chart.groups.iter().flat_map(|g| &g.series).map(|s| solid(&s.fill)).collect();
    assert_eq!(colours, vec![Some(0xED7D31), Some(0x70AD47)], "pptx-C024-b1: the series take the chart's own theme (themeOverride: accent1 #ED7D31, accent2 #70AD47), not the slide theme's (#4F81BD, #C0504D)");
}

/// pptx-C024-b1（todo に項目なし）：グラフの part が自分のテーマ（chart の rels の `themeOverride`）を持つとき、系列の色
/// （`schemeClr accent1`、色指定なしの自動の accent2）は PowerPoint ではチャートのテーマ（accent1=ED7D31・accent2=70AD47）で解かれる。
/// 読み込みはスライドのテーマ（accent1=4F81BD・accent2=C0504D）で解くので、棒の色が違う（リポジトリに themeOverride を読む所が無い）。
/// 対照（themeOverride を外し、同じ色をスライドのテーマに書いたもの）は ED7D31・70AD47。
#[test]
fn corpus_report_pptx_chart_theme_override_colours_series() {
    check_chart_theme_override("chart-theme-override.pptx");
}

fn check_chart_style_mono(file: &str) {
    let p = read(file);
    let chart = chart_of(&p);
    let colours: Vec<Option<u32>> = chart.groups.iter().flat_map(|g| &g.series).map(|s| solid(&s.fill)).collect();
    assert_eq!(colours, vec![Some(0x4BACC6)], "pptx-C024-b2: a series without a colour under `c:style val=7` is the theme's accent5 (#4BACC6), not accent1 (#4F81BD)");
}

/// pptx-C024-b2（todo に項目なし）：系列に色指定が無く、チャートの `c:style` が 3〜8（accentN の単色）のとき、PowerPoint は系列の自動色に
/// その accent（style 7 は accent5）を使う（正解 PDF の棒はテーマの accent5 に画素まで一致）。読み込みは `c:style` を読んで持つが、
/// 自動色は `accent{index % 6 + 1}` で style を見ない（chart.rs `default_color`）。スライドのテーマの accent5 は 4BACC6（青緑）、accent1 は
/// 4F81BD（青）。1 系列。対照（系列の色を `schemeClr accent5` で明示）は 4BACC6。
#[test]
fn corpus_report_pptx_chart_style_mono_uses_the_styles_accent() {
    check_chart_style_mono("chart-style-mono.pptx");
}

fn check_line_auto_markers(file: &str) {
    use ibis_doc_model::MarkerSymbol::{Diamond, Square};
    let p = read(file);
    let chart = chart_of(&p);
    let markers: Vec<Option<ibis_doc_model::MarkerSymbol>> = chart.groups.iter().flat_map(|g| &g.series).map(|s| s.marker.as_ref().map(|m| m.symbol)).collect();
    assert_eq!(markers, vec![Some(Diamond), Some(Square)], "pptx-C024-b3: the two series of a line chart with `<c:marker val=\"1\"/>` and no marker of their own get PowerPoint's automatic markers (a diamond, then a square)");
}

/// pptx-C024-b3（todo に項目なし）：折れ線グラフが `<c:marker val="1"/>` で、系列に `c:marker` も `c:spPr` も無いとき、PowerPoint は系列の順に
/// 菱形・四角・三角…の自動のマーカーを線上と凡例に付ける。読み込みは `c:marker` のある系列だけ `Marker` を作る（自動のマーカーは散布図にだけ）
/// ので、マーカーが無く線だけになる。1 本目が菱形、2 本目が四角。対照（各系列に diamond・square を明示）は通る。
#[test]
fn corpus_report_pptx_line_chart_gets_automatic_markers() {
    check_line_auto_markers("chart-line-auto-marker.pptx");
}

fn check_auto_marker_size(file: &str) {
    let p = read(file);
    let chart = chart_of(&p);
    let sizes: Vec<Option<u32>> = chart.groups.iter().flat_map(|g| &g.series).map(|s| s.marker.as_ref().map(|m| m.size)).collect();
    assert_eq!(sizes, vec![Some(7)], "pptx-C024-b4: the automatic marker of a scatter series (no `c:marker`) is 7pt (about 12px square in the reference), not 5pt");
}

/// pptx-C024-b4（todo に項目なし）：散布図（`scatterStyle lineMarker`）で系列の線が noFill、`c:marker` が無いとき、PowerPoint の自動のマーカーは
/// 7pt の菱形（正解 PDF で約 12px 角）。読み込みの `auto_marker` は `size: 5` で、点が小さい。マーカーの大きさは 7。
/// 対照（`c:marker` に diamond・size 7 を明示）は 7。
#[test]
fn corpus_report_pptx_scatter_automatic_marker_is_7pt() {
    check_auto_marker_size("chart-auto-marker-size.pptx");
}

fn check_chart_pattern_fill(file: &str) {
    let p = read(file);
    let chart = chart_of(&p);
    let fills: Vec<&Fill> = chart.groups.iter().flat_map(|g| &g.series).map(|s| &s.fill).collect();
    let presets: Vec<Option<&str>> = fills.iter().map(|f| match f {
        Fill::Pattern { preset, .. } => Some(preset.as_str()),
        _ => None,
    }).collect();
    assert_eq!(presets, vec![Some("wdUpDiag"), Some("ltUpDiag")], "pptx-C024-b5: a series filled with `a:pattFill` keeps its pattern (foreground and background), not the foreground colour alone: {fills:?}");
}

/// pptx-C024-b5（todo に項目なし）：グラフの系列・点（`c:dPt`）の `c:spPr` が `a:pattFill`（斜線・点など）のとき、読み込みは前景色だけの
/// べた塗り（`Fill::Solid`）にする（chart.rs の `"pattFill" => … fgClr`）。PowerPoint は模様（凡例の記号も模様）で描く。前景が白で背景が
/// 色の系列（ltUpDiag、前景 FFFFFF・背景 4BACC6）は白地に白で消える。系列の塗りが `Fill::Pattern`（wdUpDiag と ltUpDiag）になる。
/// 対照（べた塗り）は通らないので緑の試験は足さない（期待が模様のため）。
#[test]
fn corpus_report_pptx_chart_pattern_fill_is_kept() {
    check_chart_pattern_fill("chart-pattfill.pptx");
}

fn check_bullet_labels_are_squares(file: &str) {
    let p = read(file);
    let labels: Vec<String> = p.slides[0].shapes.iter().filter_map(|s| s.text_body()).flat_map(bullets).filter(|b| !b.is_empty()).collect();
    assert_eq!(labels.len(), 3, "the three items carry a bullet: {labels:?}");
    let square = |t: &String| t.chars().count() == 1 && t.chars().all(|c| matches!(c as u32, 0x25a0..=0x25ff | 0x2750..=0x2752));
    assert!(labels.iter().all(square), "Wingdings \"q\" is a square (U+25A0..25FF or U+2750..2752): {labels:?}");
}

fn check_cell_range_text(file: &str) {
    let p = read(file);
    let chart = chart_of(&p);
    let texts: Vec<String> = chart
        .groups
        .iter()
        .flat_map(|g| &g.series)
        .filter_map(|s| s.labels.as_ref())
        .flat_map(|l| &l.points)
        .filter_map(|p| p.text.as_ref())
        .map(|pieces| pieces.iter().map(|piece| if let ibis_doc_model::LabelPiece::Text(t) = piece { t.as_str() } else { "" }).collect::<String>())
        .collect();
    assert!(!texts.is_empty(), "the five labels carry their own text");
    assert!(texts.iter().all(|t| !t.contains("[CELLRANGE]")), "no label's text is the placeholder \"[CELLRANGE]\": {texts:?}");
    assert!(texts.iter().any(|t| t.contains("12,300 (4%)")), "the text \"12,300 (4%)\" is a label's text: {texts:?}");
}

/// The text of a shape that is not a placeholder takes its size from the presentation's `defaultTextStyle` only (no `sz`: 18 pt),
/// not the master's `otherStyle` (12 pt in `shape-text-default-size.pptx`), which a table cell's text follows (PowerPoint's PDF of a
/// slide of the corpus: a run with no `sz` in the title bar is 18 pt where the master's otherStyle says 12 pt).
#[test]
fn a_shape_text_without_size_is_18_and_a_table_cell_text_follows_the_other_style() {
    let p = read("shape-text-default-size.pptx");
    let slide = &p.slides[0];
    let shape_size = named(slide, "Box").text_body().expect("a text").paragraphs[0].runs[0].style.font_size;
    let cell_size = table_of(slide).rows[0].cells[0].text.paragraphs[0].runs[0].style.font_size;
    assert_eq!((shape_size, cell_size), (Some(1800), Some(1200)), "the shape's run, then the table cell's run");
}

/// A bullet of a symbol typeface keeps what the file says (`buFont` and `buChar`), beside the Unicode character that stands in for
/// its glyph: where Wingdings is installed it is drawn with the typeface (PowerPoint's PDF: Wingdings' own check mark, 1.0 of the
/// bullet's size), the stand-in is drawn at 0.75 of it.
#[test]
fn a_symbol_typeface_bullet_keeps_the_typeface_and_its_code() {
    let p = read("wingdings-p-bullet-glyph.pptx");
    let symbols: Vec<ibis_doc_model::SymbolBullet> = p.slides[0].shapes.iter().filter_map(|s| s.text_body()).flat_map(|b| b.paragraphs.iter()).filter_map(|para| para.bullet.as_ref()?.symbol.clone()).collect();
    assert_eq!(symbols.len(), 3, "the three items carry a symbol bullet: {symbols:?}");
    assert!(symbols.iter().all(|s| s.font.contains("Wingdings") && s.stand_in_percent == 75), "{symbols:?}");
    assert!(symbols.iter().any(|s| s.code == "p"), "the code the file writes: {symbols:?}");
}
