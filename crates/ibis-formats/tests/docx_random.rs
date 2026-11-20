// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! G4 of `tasks/doc/editing-plan.md`: random sequences of editing commands
//! on documents. For each seed, 50 commands chosen at random from the
//! document's state; then: no panic, the save succeeds and reads back as
//! the edited model, and undoing everything gives the first model and saves
//! the original file byte for byte.
//!
//! `IBIS_G4_RUNS` sets the runs per file (default 1000), `IBIS_G4_SEED` runs
//! one seed (to reproduce a failure), and `IBIS_G4_OUT` keeps every tenth
//! saved file there for the outside checks.

use ibis_doc_model::{
    Block, Command, Document, FlowAlign, FlowCommand, FlowDocument, FlowParagraph, FlowPos, LineHeight, ParaChange, Rgb,
    RunChange, VerticalPosition,
};
use ibis_formats::SaveOptions;

/// xorshift64*: small, and the same on every machine.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { (self.next() % n as u64) as usize }
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// Paragraphs grouped by the list of blocks they are in.
fn lists(doc: &FlowDocument) -> Vec<Vec<&FlowParagraph>> {
    fn walk<'a>(blocks: &'a [Block], out: &mut Vec<Vec<&'a FlowParagraph>>) {
        let mut here = Vec::new();
        for b in blocks {
            match b {
                Block::Paragraph(p) => here.push(p),
                Block::Table(t) => {
                    for row in &t.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, out);
                        }
                    }
                }
            }
        }
        if !here.is_empty() {
            out.push(here);
        }
    }
    let mut out = Vec::new();
    walk(&doc.body, &mut out);
    walk(&doc.header, &mut out);
    walk(&doc.footer, &mut out);
    out
}

const TEXTS: &[&str] = &["あ", "漢字", "abc", " ", "テキスト", "1,234", "\n", "\t", "x\ny", "\u{b}", "（注）", "é"];

fn random_command(rng: &mut Rng, doc: &FlowDocument) -> FlowCommand {
    let lists = lists(doc);
    // Mostly the body (the first list), sometimes cells, header, footer.
    let list = if rng.chance(70) { &lists[0] } else { &lists[rng.below(lists.len())] };
    let i = rng.below(list.len());
    let p = list[i];
    let at = FlowPos::new(p.id, rng.below(p.len() + 1));
    // A second position: in the same paragraph, or up to 3 paragraphs on.
    let j = if rng.chance(70) { i } else { (i + 1 + rng.below(3)).min(list.len() - 1) };
    let q = list[j];
    let to = FlowPos::new(q.id, if j == i { (at.offset + rng.below(8)).min(q.len()) } else { rng.below(q.len() + 1) });
    match rng.below(10) {
        0..=3 => {
            let text: String = (0..1 + rng.below(3)).map(|_| TEXTS[rng.below(TEXTS.len())]).collect();
            FlowCommand::InsertText { at, text, format: None }
        }
        4..=5 => FlowCommand::Delete { from: at, to },
        6..=7 => {
            let change = match rng.below(14) {
                0 => RunChange::Bold(rng.chance(50)),
                1 => RunChange::Italic(rng.chance(50)),
                2 => RunChange::Underline(rng.chance(50)),
                3 => RunChange::Strikethrough(rng.chance(50)),
                4 => RunChange::Size([800, 1050, 1200, 2000][rng.below(4)]),
                5 => RunChange::Color(Rgb([0xff0000, 0x0070c0, 0x000000][rng.below(3)])),
                6 => RunChange::Highlight([None, Some(Rgb(0xffff00)), Some(Rgb(0x123456))][rng.below(3)]),
                7 => RunChange::Font("Arial".into()),
                8 => RunChange::AllFonts("游明朝".into()),
                9 => RunChange::Position([VerticalPosition::Superscript, VerticalPosition::Subscript, VerticalPosition::Baseline][rng.below(3)]),
                10 => RunChange::EastAsianFont("ＭＳ ゴシック".into()),
                11 => RunChange::Spacing([None, Some(12_700), Some(-6_350)][rng.below(3)]),
                12 => RunChange::Scale([None, Some(150), Some(80)][rng.below(3)]),
                _ => RunChange::Clear,
            };
            FlowCommand::SetRunFormat { from: at, to, change }
        }
        8 if rng.chance(30) => {
            // Lists: an item of a list (made when the document has none),
            // not an item, a level.
            let bullet = rng.chance(50);
            match (rng.below(4), doc.styles.numbering.find(bullet)) {
                (0, _) | (_, None) => FlowCommand::SetNumbering { numbering: doc.styles.numbering.with_new_list(bullet).0 },
                (1, Some(id)) => FlowCommand::SetParagraphFormat { first: p.id, last: q.id, change: ParaChange::List(Some(id)) },
                (2, _) => FlowCommand::SetParagraphFormat { first: p.id, last: q.id, change: ParaChange::List(None) },
                _ => FlowCommand::SetParagraphFormat { first: p.id, last: q.id, change: ParaChange::Level(rng.below(3) as u32) },
            }
        }
        8 => {
            let change = match rng.below(8) {
                0 => ParaChange::Alignment([FlowAlign::Left, FlowAlign::Center, FlowAlign::Right, FlowAlign::Justify, FlowAlign::Distribute][rng.below(5)]),
                1 => ParaChange::LineSpacing([LineHeight::Auto(240), LineHeight::Auto(360), LineHeight::Exact(254_000), LineHeight::AtLeast(228_600)][rng.below(4)]),
                2 => ParaChange::SpaceBefore(rng.below(24) as i64 * 12_700),
                3 => ParaChange::SpaceAfter(rng.below(24) as i64 * 12_700),
                4 => ParaChange::LeftIndent(rng.below(4) as i64 * 266_700),
                5 => ParaChange::RightIndent(rng.below(4) as i64 * 266_700),
                6 => ParaChange::FirstLineIndent(rng.below(5) as i64 * 133_350 - 266_700),
                _ => {
                    let ids: Vec<String> = doc.styles.styles.iter().filter(|(_, s)| s.kind == ibis_doc_model::StyleKind::Paragraph).map(|(id, _)| id.clone()).collect();
                    ParaChange::Style(if ids.is_empty() || rng.chance(20) { None } else { Some(ids[rng.below(ids.len())].clone()) })
                }
            };
            FlowCommand::SetParagraphFormat { first: p.id, last: q.id, change }
        }
        _ => FlowCommand::InsertText { at, text: "\n".into(), format: None },
    }
}

fn normalized(doc: &FlowDocument) -> FlowDocument {
    fn walk(blocks: &mut [Block], next: &mut u32) {
        for b in blocks {
            *next += 1;
            match b {
                Block::Paragraph(p) => p.id = *next,
                Block::Table(t) => {
                    t.id = *next;
                    for row in &mut t.rows {
                        *next += 1;
                        row.id = *next;
                        for cell in &mut row.cells {
                            *next += 1;
                            cell.id = *next;
                            walk(&mut cell.blocks, next);
                        }
                    }
                }
            }
        }
    }
    let mut d = doc.clone();
    let mut next = 0;
    walk(&mut d.body, &mut next);
    walk(&mut d.header, &mut next);
    walk(&mut d.footer, &mut next);
    for list in [&mut d.header_first, &mut d.footer_first, &mut d.header_even, &mut d.footer_even] {
        walk(list, &mut next);
    }
    d.id_floor = 0;
    d
}

/// One run: `Err` says what went wrong.
fn run(data: &[u8], opened: &ibis_formats::Opened, seed: u64, out: Option<&std::path::Path>, name: &str) -> Result<usize, String> {
    let mut rng = Rng::new(seed);
    let mut document = Document::new(opened.presentation.clone());
    let mut rejected = 0;
    for _ in 0..50 {
        let command = random_command(&mut rng, document.presentation().flow.as_deref().unwrap());
        let shown = format!("{command:?}");
        if document.execute(vec![Command::Flow(command)]).is_err() {
            rejected += 1;
        }
        // Checking after each command finds the first that saves wrongly.
        if std::env::var_os("IBIS_G4_STEP").is_some() {
            let edited = document.presentation();
            let saved = opened.source.save(edited, SaveOptions::default()).map_err(|e| format!("save after {shown}: {e}"))?;
            let reread = ibis_formats::read(&saved).map_err(|e| format!("read back: {e}"))?;
            if normalized(reread.flow.as_deref().unwrap()) != normalized(edited.flow.as_deref().unwrap()) {
                return Err(format!("reads back differently after {shown}"));
            }
        }
    }
    let edited = document.presentation().clone();
    let saved = opened.source.save(&edited, SaveOptions::default()).map_err(|e| format!("save: {e}"))?;
    let reread = ibis_formats::read(&saved).map_err(|e| format!("read back: {e}"))?;
    let (a, b) = (normalized(reread.flow.as_deref().unwrap()), normalized(edited.flow.as_deref().unwrap()));
    if a != b {
        let (la, lb) = (lists(&a).concat(), lists(&b).concat());
        let detail = la.iter().zip(&lb).find(|(x, y)| x != y).map(|(x, y)| {
            let (rx, ry) = (x.runs.iter().map(|r| (&r.kind, &r.format, &r.char_style)).collect::<Vec<_>>(), y.runs.iter().map(|r| (&r.kind, &r.format, &r.char_style)).collect::<Vec<_>>());
            if rx != ry { format!("runs\n saved {rx:?}\n model {ry:?}") } else { format!("paragraph\n saved {x:?}\n model {y:?}") }
        });
        return Err(format!("reads back differently ({} / {} paragraphs): {}", la.len(), lb.len(), detail.unwrap_or_default()));
    }
    if let Some(dir) = out
        && seed % 10 == 0
    {
        std::fs::write(dir.join(format!("{name}-{seed}.docx")), &saved).map_err(|e| e.to_string())?;
    }
    while document.undo() {}
    if document.presentation().flow != opened.presentation.flow {
        return Err("undoing everything is not the first model".into());
    }
    let again = opened.source.save(document.presentation(), SaveOptions::default()).map_err(|e| format!("save after undo: {e}"))?;
    if again != data {
        return Err("undone and saved is not the original file".into());
    }
    Ok(rejected)
}

/// Not run by default (G4 is not part of the plan now; 2026-09-30): run it
/// with `cargo test --release -p ibis-formats --test docx_random -- --ignored`.
#[test]
#[ignore]
fn random_edits() {
    let runs: u64 = std::env::var("IBIS_G4_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(1000);
    let only: Option<u64> = std::env::var("IBIS_G4_SEED").ok().and_then(|v| v.parse().ok());
    let out = std::env::var("IBIS_G4_OUT").ok().map(std::path::PathBuf::from);
    if let Some(dir) = &out {
        std::fs::create_dir_all(dir).unwrap();
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test");
    let mut failures = Vec::new();
    for (name, file) in [("report", "editing/docx-report.docx"), ("statement", "benchmark/securities-registration-statement.docx")] {
        let data = std::fs::read(root.join(file)).unwrap();
        let opened = ibis_formats::open(data.clone()).unwrap();
        let seeds: Vec<u64> = match only {
            Some(s) => vec![s],
            None => (0..runs).collect(),
        };
        let mut rejected = 0;
        for seed in seeds {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&data, &opened, seed, out.as_deref(), name)));
            match result {
                Ok(Ok(r)) => rejected += r,
                Ok(Err(e)) => failures.push(format!("{name} seed {seed}: {e}")),
                Err(_) => failures.push(format!("{name} seed {seed}: panic")),
            }
        }
        eprintln!("{name}: {runs} runs, {rejected} commands rejected (out of range)");
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
