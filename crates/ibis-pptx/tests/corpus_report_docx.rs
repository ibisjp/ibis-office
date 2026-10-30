// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The docx defects of the corpus report (the corpus compared with Word for
//! the web's PDFs) that show in what is read: each reads a small file of
//! `test/corpus-report/docx/` and checks the model against what Word draws.
//!
//! TODO: このファイルは、実文書の比較（コーパス）から見つかった崩れの試験の
//! 仮置き場で、ずっと残すものではない。試験が通るようになったら（崩れを直した
//! ら）、その試験を本番の試験に移すか、試験として要らなければ消し、最小文書
//! （`test/corpus-report/docx/`）も一緒に移すか消す。
//! 本番の移し先は `crates/ibis-pptx/tests/` の機能ごとの試験（例
//! `text_bullets_and_runs.rs`）。
//! 崩れが残っている試験（赤）はここに置いたままにする。

use ibis_doc_model::{Block, Command, Document, FlowCommand, FlowDocument, FlowParagraph, FlowPos, FlowRunKind, LineCompound, Presentation, Shape, ShapeKind, Stroke, TextBody, VerticalMerge};

fn read(name: &str) -> Presentation {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/corpus-report/docx").join(name);
    ibis_pptx::read(&std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn flow(p: &Presentation) -> &FlowDocument {
    p.flow.as_deref().expect("a document")
}

fn paragraphs(blocks: &[Block]) -> Vec<&FlowParagraph> {
    let mut out = Vec::new();
    for b in blocks {
        match b {
            Block::Paragraph(p) => out.push(p),
            Block::Table(t) => {
                for c in t.rows.iter().flat_map(|r| &r.cells) {
                    out.extend(paragraphs(&c.blocks));
                }
            }
        }
    }
    out
}

fn shapes(doc: &FlowDocument) -> Vec<&Shape> {
    paragraphs(&doc.body).into_iter().flat_map(|p| &p.runs).filter_map(|r| if let FlowRunKind::Shape(s) = &r.kind { Some(&**s) } else { None }).collect()
}

fn shape(doc: &FlowDocument, id: u64) -> &Shape {
    shapes(doc).into_iter().find(|s| s.id.0 == id).unwrap_or_else(|| panic!("shape {id}"))
}

fn body_text(body: &TextBody) -> String {
    body.paragraphs.iter().flat_map(|p| &p.runs).map(|r| r.text.as_str()).collect()
}

fn stroke(s: &Shape) -> Option<&Stroke> {
    match &s.kind {
        ShapeKind::Line(stroke) => Some(stroke),
        ShapeKind::Auto(a) => a.stroke.as_ref(),
        _ => None,
    }
}

/// S3「テキストボックスの中で、下線・タブ・改行が消え、空の段落が詰められる」:
/// Word はテキストボックスの中でも下線を引く。
#[test]
fn corpus_report_docx_s3_underline_in_text_box() {
    let p = read("r8-underline-in-textbox.docx");
    let doc = flow(&p);
    let body = shapes(doc).into_iter().find_map(|s| s.text_body()).expect("the text box's text");
    let runs: Vec<(&str, bool)> = body.paragraphs.iter().flat_map(|p| &p.runs).map(|r| (r.text.as_str(), r.style.underline)).collect();
    for word in ["Name", "Under"] {
        assert!(runs.iter().any(|(t, u)| t.contains(word) && *u), "{word} is underlined: {runs:?}");
    }
}

/// A text box's runs take the style's `w:kern` (half points, kerned from that size on) as the renderer's `KernMin` (hundredths of a point, kerned
/// above it): `w:kern 2` is 1 pt, so 99 (Word's PDF of corpus 127: the pairs of a text box's Century text are kerned as the body's).
/// `vml-textbox-inner-width-compat15-probe.docx`: Normal has `w:kern 2`.
#[test]
fn a_text_boxs_runs_take_the_styles_kerning() {
    let p = read("vml-textbox-inner-width-compat15-probe.docx");
    let doc = flow(&p);
    let body = shapes(doc).into_iter().find_map(|s| s.text_body()).expect("the text box's text");
    let run = body.paragraphs.iter().flat_map(|p| &p.runs).find(|r| !r.text.trim().is_empty()).expect("a run");
    assert_eq!(run.style.kern.0, Some(99), "w:kern 2 is kerned from 1 pt on");
}

/// S3「テキストボックスの中で、下線・タブ・改行が消え、空の段落が詰められる」:
/// Word は左の箱の空の段落（固定 24pt）を残し、TEXT を1行下に描く。
#[test]
fn corpus_report_docx_s3_empty_paragraph_in_text_box_kept() {
    let p = read("r9-empty-para-in-textbox.docx");
    let doc = flow(&p);
    let counts: Vec<usize> = shapes(doc).iter().filter_map(|s| s.text_body()).map(|b| b.paragraphs.len()).collect();
    assert_eq!(counts, vec![2, 1], "the left box keeps its empty paragraph");
}

/// S5「VML だけで描かれた線（w:pict の v:line）が表示されない」:
/// Word は段落の間に2本の線を描く。
#[test]
fn corpus_report_docx_s5_vml_line_is_read() {
    let p = read("vml_line_only.docx");
    assert_eq!(shapes(flow(&p)).len(), 2, "both v:line are read as shapes");
}

/// S6「図形のスタイル（wps:style の lnRef）の線を使わず、a:ln の無い図形の枠線が出ない」:
/// a:ln の無い四角・大括弧・直線はテーマの線（idx=1、6350 EMU）で描かれ、
/// w の無い a:ln ＋ lnRef idx=2 の四角はテーマの 25400 EMU（2pt）になる。
#[test]
fn corpus_report_docx_s6_line_from_style_reference() {
    let p = read("shapes-1-lnref-outline.docx");
    let doc = flow(&p);
    let widths: Vec<(u64, Option<i64>)> = [101, 102, 103, 104].iter().map(|&id| (id, stroke(shape(doc, id)).map(|s| s.width))).collect();
    assert_eq!(widths, vec![(101, Some(6350)), (102, Some(6350)), (103, Some(6350)), (104, Some(25_400))]);
}

/// S7「図形の線の cmpd（二重線など）を無視し、一重線で描く」:
/// 4つの四角は sng・dbl・thickThin・tri の線になる。
#[test]
fn corpus_report_docx_s7_compound_line() {
    let p = read("shapes-2-compound-line.docx");
    let doc = flow(&p);
    let compounds: Vec<Option<LineCompound>> = [106, 107, 108, 109].iter().map(|&id| stroke(shape(doc, id)).map(|s| s.compound)).collect();
    assert_eq!(compounds, vec![Some(LineCompound::Single), Some(LineCompound::Double), Some(LineCompound::ThickThin), Some(LineCompound::Triple)]);
}

/// S11「テキストボックスの縦書き（bodyPr の vert="eaVert"・"vert"）を読まず、横書きで描く」:
/// eaVert の箱は縦書き、vert の箱は文字全体を右に 90 度回す。
#[test]
fn corpus_report_docx_s11_vertical_text_box() {
    let p = read("vtext-textbox-eavert.docx");
    let doc = flow(&p);
    let body = |id| shape(doc, id).text_body().expect("text").clone();
    assert!(!body(101).vertical, "horz");
    assert!(body(102).vertical, "eaVert is vertical");
    assert_eq!(body(103).sideways, 5_400_000, "vert turns the text a quarter");
}

/// S12「テキストボックスの中の表が描かれない」:
/// Word は枠の中に3行2列の表を描く。
#[test]
fn corpus_report_docx_s12_table_in_text_box() {
    let p = read("tbl-in-textbox.docx");
    let doc = flow(&p);
    let text = shapes(doc).into_iter().find_map(|s| s.text_body()).map(body_text).expect("the text box's text");
    for cell in ["地区名", "市町村名", "北地区", "さくら市、つばき村"] {
        assert!(text.contains(cell), "the table's cell {cell} is in the text box: {text}");
    }
}

/// S13「180 度回した図形の文字が逆さまになり、全行が1行に重なる（upright を読まない）」:
/// bodyPr の upright="1" で、Word は文字を正立のまま描く。
#[test]
fn corpus_report_docx_s13_upright_text() {
    let p = read("shape-rot180-upright.docx");
    let doc = flow(&p);
    let s = shapes(doc).into_iter().find(|s| s.rotation == 10_800_000).expect("the turned callout");
    assert!(s.text_body().expect("text").upright, "upright=1 is read");
}

/// G1「Wingdings の箇条書き記号 U+F09F が「?」入りの四角（文字化け）になる」:
/// Word は小さな丸い記号「•」を描く。
#[test]
fn corpus_report_docx_g1_wingdings_f09f_bullet() {
    let p = read("r4-wingdings-bullet.docx");
    let doc = flow(&p);
    let labels = ibis_doc_model::list_labels(doc);
    let first = paragraphs(&doc.body)[0];
    let label = labels.get(&first.id).cloned().unwrap_or_default();
    assert_eq!(label, "•", "U+F09F of Wingdings: {:?}", label.chars().map(|c| format!("U+{:04X}", c as u32)).collect::<Vec<_>>());
}

/// I3「縦に結合したセルの続きのセルにある番号付き段落も数え、後の番号が1つずつ進む」:
/// Word は結合の続きのセルの段落を数えず、4行目は ③ になる。
#[test]
fn corpus_report_docx_i3_merged_continuation_not_numbered() {
    let p = read("vmerge-numbering.docx");
    let doc = flow(&p);
    let labels = ibis_doc_model::list_labels(doc);
    let Some(Block::Table(t)) = doc.body.iter().find(|b| matches!(b, Block::Table(_))) else { panic!("the table") };
    assert_eq!(t.rows[2].cells[0].merge, VerticalMerge::Continue);
    let shown: Vec<String> = t.rows.iter().filter(|r| r.cells[0].merge != VerticalMerge::Continue).map(|r| paragraphs(&r.cells[0].blocks)[0]).map(|p| labels.get(&p.id).cloned().unwrap_or_default()).collect();
    assert_eq!(shown, vec!["①", "②", "③"], "the labels of the rows shown");
}

/// E1「ドロップダウンなどのコンテンツコントロールで、段落内の位置の数え方が食い違う
/// （デバッグ版で panic、カーソルがずれる）」: Word と保存側は段落の途中のドロップダウンを
/// 1位置と数える。段落「A＋ドロップダウン（Option1）＋BC」は4位置で、モデルの位置の数
/// （FlowParagraph::len）も4であること。あわせて、コントロールの前後に打った字が保存して
/// 読み直してもその場所にあること。
#[test]
fn corpus_report_docx_e1_dropdown_is_one_position() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/corpus-report/docx/dropdown.docx");
    let opened = ibis_pptx::open(std::fs::read(&path).unwrap()).unwrap();
    let mut document = Document::new(opened.presentation.clone());
    let p = paragraphs(&flow(document.presentation()).body)[0].clone();
    let commands = vec![
        Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p.id, p.len() - 1), text: "Z".into(), format: None }),
        Command::Flow(FlowCommand::InsertText { at: FlowPos::new(p.id, 1), text: "Y".into(), format: None }),
    ];
    document.execute(commands).unwrap();
    let saved = opened.source.save(document.presentation(), ibis_pptx::SaveOptions::default()).unwrap();
    let reread = ibis_pptx::read(&saved).unwrap();
    let text: String = paragraphs(&flow(&reread).body)[0]
        .runs
        .iter()
        .map(|r| match &r.kind {
            FlowRunKind::Text(t) => t.clone(),
            FlowRunKind::DropDownList { items, selected, .. } => format!("[{}]", items.get(*selected).map_or("", |(shown, _)| shown.as_str())),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(text, "AY[Option1]BZC", "typed before and after the drop-down, saved and read back");
    assert_eq!(p.len(), 4, "positions of A, the drop-down, B, C: {:?}", p.runs.iter().map(|r| (&r.kind, r.len())).collect::<Vec<_>>());
}

/// docx-C020-b1・docx-C025-b1「VML の図形が `type="#_x0000_tNNN"` で同じ `w:pict` の
/// `v:shapetype`（大かっこ t185・テキスト ボックス t202）の形を参照するだけで、自分では
/// `path` を持たないと、図形ごと（テキスト ボックスの文字も）読み捨てる」: Word は
/// 「第　期報告書」の行の枠なしのテキスト ボックス（2 行の文字）とそれを囲む大かっこ、
/// 「ダミー宛先　殿」の下の 2 行を囲む大かっこの、合わせて 3 つを描く。もう 1 つの文書
/// （大かっこ 1 つ）も同じ。`mc:AlternateContent` の無い、旧い書式の VML の形。
/// （統合: C020-b1 と C025-b1 は同じきっかけなので 1 本にした。）
#[test]
fn corpus_report_docx_c020_b1_vml_shapetype_reference_is_read() {
    let mut failures = Vec::new();
    let p = read("vml-shapetype-reference.docx");
    let doc = flow(&p);
    let found = shapes(doc);
    if found.len() != 3 {
        failures.push(format!("vml-shapetype-reference: {} shapes are read; Word draws 3 (the text box and two brackets)", found.len()));
    }
    let text: String = found.iter().filter_map(|s| s.text_body()).map(body_text).collect();
    for want in ["日から", "日まで"] {
        if !text.contains(want) {
            failures.push(format!("vml-shapetype-reference: the text box's 「…{want}」 is not read (text {text:?})"));
        }
    }
    let p = read("vml-bracket-shapetype.docx");
    let n = shapes(flow(&p)).len();
    if n != 1 {
        failures.push(format!("vml-bracket-shapetype: {n} shapes are read; Word draws the bracket around the two lines"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// docx-C020-b2「本文の行の中の VML の画像（`w:pict`／`v:shape type="#_x0000_t75"`／
/// `v:imagedata r:id`）を読まない」: Word は段落の中に 150pt × 90pt の画像を置き
/// （正解の PDF で、同じ形の画像が本文の幅いっぱいに出る）、下の本文は画像の下に来る。
/// 今は画像も画像の高さも無い。
#[test]
fn corpus_report_docx_c020_b2_vml_imagedata_picture_is_read() {
    let p = read("vml-imagedata-picture.docx");
    let doc = flow(&p);
    let pictures: Vec<(i64, i64)> = paragraphs(&doc.body)
        .into_iter()
        .flat_map(|p| &p.runs)
        .filter_map(|r| if let FlowRunKind::Picture { size, .. } = &r.kind { Some((size.width, size.height)) } else { None })
        .collect();
    assert_eq!(pictures, vec![(1_905_000, 1_143_000)], "the inline picture (150pt × 90pt in EMU) of v:imagedata");
}

/// docx-C028-b1「`w:sym`（`w:font="Wingdings" w:char="F0FE"`）を、書体を見ずに低位バイトの
/// 字（U+00FE の「þ」）にする」: Word は Wingdings の 0xFE を「☑」で描く
/// （正解の PDF「チェック（☑）を入れてください」）。2 か所とも。
/// todo-docx の G1（Wingdings の箇条書き記号 F09F、[x]）は箇条書き専用の表で、本文の `w:sym` は通らない。
#[test]
fn corpus_report_docx_c028_b1_wingdings_sym_is_a_check_box() {
    let p = read("wingdings-sym.docx");
    let doc = flow(&p);
    let text: Vec<String> = paragraphs(&doc.body)
        .into_iter()
        .map(|p| p.runs.iter().filter_map(|r| if let FlowRunKind::Text(t) = &r.kind { Some(t.as_str()) } else { None }).collect())
        .collect();
    let marks: Vec<Vec<char>> = text.iter().map(|t| t.chars().filter(|c| matches!(c, '☑' | 'þ')).collect()).collect();
    assert_eq!(marks, vec![vec!['☑'], vec!['☑']], "the Wingdings 0xFE of w:sym in each paragraph: {text:?}");
}
